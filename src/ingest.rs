use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

use anyhow::{Context, Result};
use chrono::Utc;
use serde::Serialize;
use serde_json::Value;
use tracing::{debug, warn};

use crate::cli::IngestArgs;
use crate::config::Paths;
use crate::extract::Extractor;
use crate::store::{SessionMeta, Store};
use crate::transcript::{Event, TranscriptPath, walk};

#[derive(Debug, Clone, Default, Serialize)]
pub struct IngestStats {
    pub sessions_seen: usize,
    pub sessions_new: usize,
    pub sessions_updated: usize,
    pub sessions_unchanged: usize,
    pub entries_inserted: usize,
}

pub fn run(args: IngestArgs) -> Result<()> {
    let paths = Paths::resolve()?;
    let root = args.path.clone().unwrap_or(paths.transcripts_dir.clone());
    let store = Store::open(&paths.db_path)?;
    let transcripts = walk(&root)?;
    let mut totals = IngestStats::default();
    for tp in transcripts {
        totals.sessions_seen += 1;
        match ingest_one(&store, &tp, args.dry_run) {
            Ok(IngestOutcome::New(n)) => {
                totals.sessions_new += 1;
                totals.entries_inserted += n;
            }
            Ok(IngestOutcome::Updated(n)) => {
                totals.sessions_updated += 1;
                totals.entries_inserted += n;
            }
            Ok(IngestOutcome::Unchanged) => totals.sessions_unchanged += 1,
            Err(e) => warn!(session = %tp.session_id, error = ?e, "ingest failed"),
        }
    }
    println!(
        "ingest: {} sessions seen ({} new, {} updated, {} unchanged), {} entries inserted",
        totals.sessions_seen,
        totals.sessions_new,
        totals.sessions_updated,
        totals.sessions_unchanged,
        totals.entries_inserted
    );
    if args.dry_run {
        println!("(dry-run: no writes performed)");
    }
    Ok(())
}

enum IngestOutcome {
    New(usize),
    Updated(usize),
    Unchanged,
}

fn ingest_one(store: &Store, tp: &TranscriptPath, dry_run: bool) -> Result<IngestOutcome> {
    let meta_fs = std::fs::metadata(&tp.source_path)
        .with_context(|| format!("stat {}", tp.source_path.display()))?;
    let mtime = meta_fs
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let size = meta_fs.len() as i64;

    let prev = store.session_fingerprint(&tp.session_id)?;
    let is_new = prev.is_none();
    if let Some((prev_mtime, prev_size)) = prev
        && prev_mtime == mtime
        && prev_size == size
    {
        return Ok(IngestOutcome::Unchanged);
    }

    let parsed = parse_session(&tp.source_path)?;
    let project_path = parsed
        .cwd
        .clone()
        .unwrap_or_else(|| tp.project_path.clone());

    if dry_run {
        debug!(session = %tp.session_id, entries = parsed.entries.len(), "would ingest");
        return Ok(if is_new {
            IngestOutcome::New(parsed.entries.len())
        } else {
            IngestOutcome::Updated(parsed.entries.len())
        });
    }

    let session_meta = SessionMeta {
        session_id: tp.session_id.clone(),
        provider: tp.provider.to_string(),
        project_path,
        source_path: tp.source_path.to_string_lossy().to_string(),
        source_mtime: mtime,
        source_size: size,
        started_at: parsed.started_at,
        ended_at: parsed.ended_at,
        message_count: parsed.message_count,
    };
    store.upsert_session(&session_meta, Utc::now().timestamp())?;
    let inserted = store.insert_entries(&tp.session_id, &parsed.entries)?;
    Ok(if is_new {
        IngestOutcome::New(inserted)
    } else {
        IngestOutcome::Updated(inserted)
    })
}

struct ParsedSession {
    entries: Vec<crate::store::Entry>,
    cwd: Option<String>,
    started_at: Option<i64>,
    ended_at: Option<i64>,
    message_count: i64,
}

fn parse_session(path: &Path) -> Result<ParsedSession> {
    let file = File::open(path).with_context(|| format!("open {}", path.display()))?;
    let reader = BufReader::new(file);
    let session_id = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown")
        .to_string();
    let mut extractor = Extractor::new(session_id);
    let mut entries = Vec::new();
    let mut cwd: Option<String> = None;
    let mut started_at: Option<i64> = None;
    let mut ended_at: Option<i64> = None;
    let mut message_count: i64 = 0;
    for (idx, line) in reader.lines().enumerate() {
        let source_line = (idx + 1) as i64;
        let line = match line {
            Ok(l) if !l.trim().is_empty() => l,
            _ => continue,
        };
        let raw: Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(_) => continue,
        };
        if cwd.is_none() {
            cwd = raw
                .get("cwd")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
        }
        if let Some(ts_str) = raw.get("timestamp").and_then(|v| v.as_str())
            && let Ok(dt) = chrono::DateTime::parse_from_rfc3339(ts_str)
        {
            let ts = dt.timestamp();
            started_at = Some(started_at.map(|s| s.min(ts)).unwrap_or(ts));
            ended_at = Some(ended_at.map(|s| s.max(ts)).unwrap_or(ts));
        }
        let event: Event = match serde_json::from_value(raw) {
            Ok(e) => e,
            Err(_) => continue,
        };
        if matches!(event, Event::User(_) | Event::Assistant(_)) {
            message_count += 1;
        }
        match extractor.step(event, source_line) {
            Ok(mut e) => entries.append(&mut e),
            Err(e) => warn!(line = source_line, error = ?e, "extract step failed"),
        }
    }
    entries.append(&mut extractor.flush());
    Ok(ParsedSession {
        entries,
        cwd,
        started_at,
        ended_at,
        message_count,
    })
}
