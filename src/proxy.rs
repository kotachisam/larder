use std::hash::{DefaultHasher, Hash, Hasher};
use std::sync::Arc;

use anyhow::{Context, Result};
use axum::{
    Router,
    body::Body,
    extract::{Request, State},
    http::{HeaderMap, Method, StatusCode},
    response::Response,
    routing::any,
};
use chrono::Utc;
use futures::StreamExt;
use serde_json::Value;
use tracing::{info, warn};

use crate::cli::ProxyArgs;
use crate::config::Paths;
use crate::store::{Entry, EntryKind, SessionMeta, Store};

pub fn run(args: ProxyArgs) -> Result<()> {
    let paths = Paths::resolve()?;
    let store = Arc::new(Store::open(&paths.db_path)?);
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .context("creating tokio runtime")?;
    runtime.block_on(serve(args, store))
}

async fn serve(args: ProxyArgs, store: Arc<Store>) -> Result<()> {
    let upstream = args.to.trim_end_matches('/').to_string();
    let state = Arc::new(ProxyState {
        upstream: upstream.clone(),
        store,
        client: reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(600))
            .build()
            .context("building reqwest client")?,
    });

    let app = Router::new()
        .route("/{*path}", any(proxy_handler))
        .with_state(state);

    let bind_addr = format!("{}:{}", args.bind, args.port);
    let listener = tokio::net::TcpListener::bind(&bind_addr)
        .await
        .with_context(|| format!("binding {}", bind_addr))?;
    info!(
        "larder proxy listening on http://{} → {}",
        bind_addr, upstream
    );
    eprintln!(
        "larder proxy listening on http://{} → forwarding to {}",
        bind_addr, upstream
    );
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .context("axum serve")?;
    Ok(())
}

async fn shutdown_signal() {
    let _ = tokio::signal::ctrl_c().await;
    eprintln!("\nlarder proxy shutting down");
}

struct ProxyState {
    upstream: String,
    store: Arc<Store>,
    client: reqwest::Client,
}

async fn proxy_handler(
    State(state): State<Arc<ProxyState>>,
    req: Request,
) -> Result<Response, (StatusCode, String)> {
    let method = req.method().clone();
    let uri = req.uri().clone();
    let path_and_query = uri
        .path_and_query()
        .map(|pq| pq.as_str())
        .unwrap_or(uri.path());
    let upstream_url = format!("{}{}", state.upstream, path_and_query);
    let headers = req.headers().clone();
    let body_bytes = axum::body::to_bytes(req.into_body(), usize::MAX)
        .await
        .map_err(|e| (StatusCode::BAD_REQUEST, format!("read body: {}", e)))?;

    let should_capture =
        method == Method::POST && (uri.path() == "/api/chat" || uri.path() == "/api/generate");

    let parsed_request = if should_capture {
        serde_json::from_slice::<Value>(&body_bytes).ok()
    } else {
        None
    };

    let upstream_response = forward_to_upstream(
        &state.client,
        &method,
        &upstream_url,
        &headers,
        body_bytes.to_vec(),
    )
    .await
    .map_err(|e| (StatusCode::BAD_GATEWAY, format!("upstream error: {}", e)))?;

    let status = upstream_response.status();
    let response_headers = upstream_response.headers().clone();
    let upstream_stream = upstream_response.bytes_stream();

    if should_capture
        && status.is_success()
        && let Some(req_json) = parsed_request
    {
        let endpoint = uri.path().to_string();
        let store = state.store.clone();
        return Ok(stream_with_capture(
            status,
            response_headers,
            upstream_stream,
            req_json,
            endpoint,
            store,
        ));
    }

    let body = Body::from_stream(upstream_stream);
    let mut response = Response::new(body);
    *response.status_mut() = status;
    *response.headers_mut() = response_headers;
    Ok(response)
}

async fn forward_to_upstream(
    client: &reqwest::Client,
    method: &Method,
    url: &str,
    headers: &HeaderMap,
    body: Vec<u8>,
) -> Result<reqwest::Response> {
    let mut req = client.request(method.clone(), url).body(body);
    for (name, value) in headers.iter() {
        if name == "host" || name == "content-length" {
            continue;
        }
        req = req.header(name.as_str(), value);
    }
    Ok(req.send().await?)
}

fn stream_with_capture<S>(
    status: StatusCode,
    response_headers: HeaderMap,
    upstream_stream: S,
    request_body: Value,
    endpoint: String,
    store: Arc<Store>,
) -> Response
where
    S: futures::Stream<Item = reqwest::Result<bytes::Bytes>> + Send + 'static,
{
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<Result<bytes::Bytes, std::io::Error>>();
    let endpoint_for_task = endpoint.clone();
    tokio::spawn(async move {
        let mut accumulator = ResponseAccumulator::default();
        let mut stream = Box::pin(upstream_stream);
        while let Some(chunk_result) = stream.next().await {
            match chunk_result {
                Ok(chunk) => {
                    accumulator.feed(&chunk);
                    if tx.send(Ok(chunk)).is_err() {
                        break;
                    }
                }
                Err(e) => {
                    let _ = tx.send(Err(std::io::Error::other(e.to_string())));
                    return;
                }
            }
        }
        drop(tx);
        if let Err(e) =
            persist_exchange(&store, &endpoint_for_task, request_body, accumulator).await
        {
            warn!(error = ?e, "failed to persist proxy exchange");
        }
    });

    let body = Body::from_stream(tokio_stream_wrapper(rx));
    let mut response = Response::new(body);
    *response.status_mut() = status;
    *response.headers_mut() = response_headers;
    response
}

fn tokio_stream_wrapper(
    rx: tokio::sync::mpsc::UnboundedReceiver<Result<bytes::Bytes, std::io::Error>>,
) -> impl futures::Stream<Item = Result<bytes::Bytes, std::io::Error>> {
    async_stream::stream_unboundedreceiver(rx)
}

mod async_stream {
    use bytes::Bytes;
    use futures::Stream;
    use std::pin::Pin;
    use std::task::{Context, Poll};
    use tokio::sync::mpsc::UnboundedReceiver;

    pub fn stream_unboundedreceiver(
        rx: UnboundedReceiver<Result<Bytes, std::io::Error>>,
    ) -> ReceiverStream {
        ReceiverStream { rx }
    }

    pub struct ReceiverStream {
        rx: UnboundedReceiver<Result<Bytes, std::io::Error>>,
    }

    impl Stream for ReceiverStream {
        type Item = Result<Bytes, std::io::Error>;
        fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
            self.rx.poll_recv(cx)
        }
    }
}

#[derive(Default)]
struct ResponseAccumulator {
    buffer: Vec<u8>,
    content: String,
    model: Option<String>,
    final_object: Option<Value>,
}

impl ResponseAccumulator {
    fn feed(&mut self, chunk: &bytes::Bytes) {
        self.buffer.extend_from_slice(chunk);
        while let Some(newline_pos) = self.buffer.iter().position(|b| *b == b'\n') {
            let line: Vec<u8> = self.buffer.drain(..=newline_pos).collect();
            let trimmed = line.strip_suffix(b"\n").unwrap_or(&line);
            if trimmed.is_empty() {
                continue;
            }
            if let Ok(v) = serde_json::from_slice::<Value>(trimmed) {
                self.absorb_json(v);
            }
        }
    }

    fn finalize(mut self) -> Self {
        if !self.buffer.is_empty()
            && let Ok(v) = serde_json::from_slice::<Value>(&self.buffer)
        {
            self.absorb_json(v);
        }
        self
    }

    fn absorb_json(&mut self, v: Value) {
        if self.model.is_none()
            && let Some(m) = v.get("model").and_then(|x| x.as_str())
        {
            self.model = Some(m.to_string());
        }
        if let Some(piece) = v
            .pointer("/message/content")
            .and_then(|x| x.as_str())
            .or_else(|| v.get("response").and_then(|x| x.as_str()))
        {
            self.content.push_str(piece);
        }
        if v.get("done").and_then(|x| x.as_bool()) == Some(true) {
            self.final_object = Some(v);
        }
    }
}

async fn persist_exchange(
    store: &Arc<Store>,
    endpoint: &str,
    request: Value,
    accumulator: ResponseAccumulator,
) -> Result<()> {
    let acc = accumulator.finalize();
    let store = store.clone();
    let endpoint = endpoint.to_string();
    tokio::task::spawn_blocking(move || -> Result<()> {
        let model = acc
            .model
            .clone()
            .or_else(|| {
                request
                    .get("model")
                    .and_then(|x| x.as_str())
                    .map(String::from)
            })
            .unwrap_or_else(|| "unknown".to_string());

        let (first_user_message, latest_user_message, message_index) =
            extract_messages(&request, endpoint.as_str());
        let Some(first) = first_user_message else {
            return Ok(());
        };
        let session_id = derive_session_id(&first, &model);
        let now = Utc::now().timestamp();
        let project_path = format!("ollama://{}", model);

        let session_meta = SessionMeta {
            session_id: session_id.clone(),
            provider: "ollama-proxy".to_string(),
            project_path,
            source_path: format!("proxy://{}{}", model, endpoint),
            source_mtime: now,
            source_size: 0,
            started_at: Some(now),
            ended_at: Some(now),
            message_count: message_index + 1,
            parent_session_id: None,
            is_subagent: false,
            subagent_description: None,
            subagent_type: None,
        };
        store.upsert_session(&session_meta, now)?;

        let entry = Entry {
            session_id,
            ts: now,
            kind: EntryKind::Qa,
            question: latest_user_message,
            answer_summary: nonempty(acc.content),
            command: None,
            command_stdout: None,
            command_stderr: None,
            interrupted: false,
            truncated: false,
            tool_use_id: None,
            parent_uuid: None,
            source_line: message_index,
        };
        store.insert_entries(&entry.session_id.clone(), &[entry])?;
        Ok(())
    })
    .await
    .map_err(|e| anyhow::anyhow!("blocking task join: {}", e))??;
    Ok(())
}

fn extract_messages(request: &Value, endpoint: &str) -> (Option<String>, Option<String>, i64) {
    if endpoint == "/api/chat" {
        let messages = request
            .get("messages")
            .and_then(|m| m.as_array())
            .cloned()
            .unwrap_or_default();
        let first_user = messages
            .iter()
            .find(|m| m.get("role").and_then(|r| r.as_str()) == Some("user"))
            .and_then(|m| m.get("content").and_then(|c| c.as_str()))
            .map(String::from);
        let latest_user = messages
            .iter()
            .rev()
            .find(|m| m.get("role").and_then(|r| r.as_str()) == Some("user"))
            .and_then(|m| m.get("content").and_then(|c| c.as_str()))
            .map(String::from);
        (first_user, latest_user, messages.len() as i64)
    } else {
        let prompt = request
            .get("prompt")
            .and_then(|p| p.as_str())
            .map(String::from);
        (prompt.clone(), prompt, 1)
    }
}

fn derive_session_id(first_user_message: &str, model: &str) -> String {
    let mut hasher = DefaultHasher::new();
    first_user_message.hash(&mut hasher);
    model.hash(&mut hasher);
    format!(
        "ollama-{}-{:016x}",
        model.replace([':', '/'], "_"),
        hasher.finish()
    )
}

fn nonempty(s: String) -> Option<String> {
    if s.is_empty() { None } else { Some(s) }
}
