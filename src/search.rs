use anyhow::Result;
use rusqlite::params;
use serde::Serialize;

use crate::cli::{AskArgs, AskedArgs};
use crate::config::Paths;
use crate::format::{render_command_only, render_hits, render_prompts};
use crate::store::Store;
use crate::util::{atty_stdout, since_seconds};

#[derive(Debug, Clone, Serialize)]
pub struct Hit {
    pub id: i64,
    pub session_id: String,
    pub ts: i64,
    pub project_path: String,
    pub kind: String,
    pub question: Option<String>,
    pub command: Option<String>,
    pub stdout: Option<String>,
    pub stderr: Option<String>,
    pub summary: Option<String>,
    pub score: f64,
    pub is_subagent: bool,
    pub parent_session_id: Option<String>,
    pub subagent_description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raw_matches: Option<usize>,
}

pub fn search(
    store: &Store,
    query: &str,
    limit: usize,
    exclude_subagents: bool,
) -> Result<Vec<Hit>> {
    let conn = store.conn.lock().unwrap();
    let fts_query = build_fts_query(query);
    let where_subagent = if exclude_subagents {
        " AND s.is_subagent = 0"
    } else {
        ""
    };
    let sql = format!(
        r#"
        SELECT
            e.id, e.session_id, e.ts, s.project_path, e.kind,
            e.question, e.command, e.command_stdout, e.command_stderr,
            e.answer_summary, bm25(entries_fts) AS score,
            s.is_subagent, s.parent_session_id, s.subagent_description
        FROM entries_fts
        JOIN entries e ON e.id = entries_fts.rowid
        JOIN sessions s ON s.session_id = e.session_id
        WHERE entries_fts MATCH ?1{where_subagent}
        ORDER BY score ASC
        LIMIT ?2
        "#
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(params![fts_query, limit as i64], |r| {
        Ok(Hit {
            id: r.get(0)?,
            session_id: r.get(1)?,
            ts: r.get(2)?,
            project_path: r.get(3)?,
            kind: r.get(4)?,
            question: r.get(5)?,
            command: r.get(6)?,
            stdout: r.get(7)?,
            stderr: r.get(8)?,
            summary: r.get(9)?,
            score: r.get(10)?,
            is_subagent: r.get::<_, i64>(11)? != 0,
            parent_session_id: r.get(12)?,
            subagent_description: r.get(13)?,
            raw_matches: None,
        })
    })?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}

pub fn hits_by_entry_ids(store: &Store, entry_ids: &[i64]) -> Result<Vec<Hit>> {
    if entry_ids.is_empty() {
        return Ok(Vec::new());
    }
    let conn = store.conn.lock().unwrap();
    let placeholders = (1..=entry_ids.len())
        .map(|i| format!("?{}", i))
        .collect::<Vec<_>>()
        .join(",");
    let sql = format!(
        r#"
        SELECT
            e.id, e.session_id, e.ts, s.project_path, e.kind,
            e.question, e.command, e.command_stdout, e.command_stderr,
            e.answer_summary, 0.0 AS score,
            s.is_subagent, s.parent_session_id, s.subagent_description
        FROM entries e
        JOIN sessions s ON s.session_id = e.session_id
        WHERE e.id IN ({placeholders})
        "#
    );
    let mut stmt = conn.prepare(&sql)?;
    let params: Vec<&dyn rusqlite::ToSql> = entry_ids
        .iter()
        .map(|i| i as &dyn rusqlite::ToSql)
        .collect();
    let rows = stmt.query_map(&*params, |r| {
        Ok(Hit {
            id: r.get(0)?,
            session_id: r.get(1)?,
            ts: r.get(2)?,
            project_path: r.get(3)?,
            kind: r.get(4)?,
            question: r.get(5)?,
            command: r.get(6)?,
            stdout: r.get(7)?,
            stderr: r.get(8)?,
            summary: r.get(9)?,
            score: r.get(10)?,
            is_subagent: r.get::<_, i64>(11)? != 0,
            parent_session_id: r.get(12)?,
            subagent_description: r.get(13)?,
            raw_matches: None,
        })
    })?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}

fn build_fts_query(raw: &str) -> String {
    let tokens: Vec<String> = raw
        .split_whitespace()
        .map(|t| {
            let cleaned: String = t
                .chars()
                .filter(|c| c.is_alphanumeric() || *c == '_' || *c == '-')
                .collect();
            cleaned
        })
        .filter(|t| !t.is_empty())
        .map(|t| format!("\"{}\"", t))
        .collect();
    if tokens.is_empty() {
        raw.to_string()
    } else {
        tokens.join(" ")
    }
}

pub fn run(args: AskArgs) -> Result<()> {
    let paths = Paths::resolve()?;
    let store = Store::open(&paths.db_path)?;
    let query = args.query.join(" ");
    if query.trim().is_empty() {
        anyhow::bail!("query is empty");
    }
    let hits = search(&store, &query, args.limit, args.no_subagents)?;
    if args.cmd_only {
        match render_command_only(&hits) {
            Some(cmd) => {
                println!("{}", cmd);
                Ok(())
            }
            None => std::process::exit(1),
        }
    } else {
        let color = !args.no_color && atty_stdout();
        let out = render_hits(&hits, args.format, color, args.raw)?;
        print!("{}", out);
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct PromptHit {
    pub id: i64,
    pub ts: i64,
    pub project_path: String,
    pub prompt_text: String,
    pub pasted_chars: i64,
    pub score: f64,
}

pub fn search_prompts(
    store: &Store,
    query: &str,
    limit: usize,
    since_ts: i64,
    project: Option<&str>,
) -> Result<Vec<PromptHit>> {
    let conn = store.conn.lock().unwrap();
    let fts_query = build_fts_query(query);
    let normalized_project = project.map(|p| p.trim_end_matches('/').to_string());
    let base = r#"
        SELECT
            p.id, p.ts, p.project_path, p.prompt_text, p.pasted_chars,
            bm25(prompts_fts) AS score
        FROM prompts_fts
        JOIN prompts p ON p.id = prompts_fts.rowid
        WHERE prompts_fts MATCH ?1
          AND p.ts >= ?2
    "#;
    let rows = if let Some(project) = normalized_project.as_deref() {
        let sql = format!(
            "{base} AND (p.project_path = ?3 OR p.project_path LIKE ?3 || '/%') ORDER BY score ASC LIMIT ?4"
        );
        let mut stmt = conn.prepare(&sql)?;
        stmt.query_map(
            params![fts_query, since_ts, project, limit as i64],
            map_prompt_hit,
        )?
        .collect::<rusqlite::Result<Vec<_>>>()?
    } else {
        let sql = format!("{base} ORDER BY score ASC LIMIT ?3");
        let mut stmt = conn.prepare(&sql)?;
        stmt.query_map(params![fts_query, since_ts, limit as i64], map_prompt_hit)?
            .collect::<rusqlite::Result<Vec<_>>>()?
    };
    Ok(rows)
}

fn map_prompt_hit(r: &rusqlite::Row) -> rusqlite::Result<PromptHit> {
    Ok(PromptHit {
        id: r.get(0)?,
        ts: r.get(1)?,
        project_path: r.get(2)?,
        prompt_text: r.get(3)?,
        pasted_chars: r.get(4)?,
        score: r.get(5)?,
    })
}

pub fn run_asked(args: AskedArgs) -> Result<()> {
    let paths = Paths::resolve()?;
    let store = Store::open(&paths.db_path)?;
    let query = args.query.join(" ");
    if query.trim().is_empty() {
        anyhow::bail!("query is empty");
    }
    let since_ts = since_seconds(args.since.as_deref())?;
    let hits = search_prompts(
        &store,
        &query,
        args.limit,
        since_ts,
        args.project.as_deref(),
    )?;
    let color = !args.no_color && atty_stdout();
    let out = render_prompts(&hits, args.format, color)?;
    print!("{}", out);
    Ok(())
}

