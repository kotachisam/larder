use anyhow::Result;
use rusqlite::params;
use serde::Serialize;

use crate::cli::{AskArgs, AskedArgs};
use crate::config::Paths;
use crate::format_qa::{render_command_only, render_hits, render_prompts};
use crate::results_cache::{self, CachedHit, ResultsCache};
use crate::store::Store;
use crate::util::{DisplayMode, atty_stdout, since_seconds};

fn display_mode(full: bool, raw: bool) -> DisplayMode {
    match (raw, full) {
        (true, _) => DisplayMode::Raw,
        (false, true) => DisplayMode::Full,
        (false, false) => DisplayMode::Compact,
    }
}

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
    #[serde(skip_serializing_if = "is_zero")]
    #[serde(default)]
    pub more_in_session: usize,
}

fn is_zero(n: &usize) -> bool {
    *n == 0
}

const SESSION_DEDUPE_OVERSAMPLE: usize = 5;

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
    let oversample = (limit * SESSION_DEDUPE_OVERSAMPLE) as i64;
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
    let rows = stmt.query_map(params![fts_query, oversample], |r| {
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
            more_in_session: 0,
        })
    })?;
    let mut all = Vec::new();
    for row in rows {
        all.push(row?);
    }
    Ok(dedupe_by_session(all, limit))
}

fn dedupe_by_session(hits: Vec<Hit>, limit: usize) -> Vec<Hit> {
    use std::collections::HashMap;
    let mut first_index: HashMap<String, usize> = HashMap::new();
    let mut deduped: Vec<Hit> = Vec::new();
    for hit in hits {
        match first_index.get(&hit.session_id).copied() {
            Some(idx) => {
                deduped[idx].more_in_session += 1;
            }
            None => {
                first_index.insert(hit.session_id.clone(), deduped.len());
                deduped.push(hit);
            }
        }
    }
    deduped.truncate(limit);
    deduped
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
            more_in_session: 0,
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
        let _ = write_cache("ask", &hits);
        let color = !args.no_color && atty_stdout();
        let mode = display_mode(args.full, args.raw);
        let out = render_hits(&hits, args.format, color, mode)?;
        print!("{}", out);
        Ok(())
    }
}

pub fn write_cache(produced_by: &str, hits: &[Hit]) -> Result<()> {
    let cache = ResultsCache {
        produced_by: produced_by.to_string(),
        ts: chrono::Utc::now().timestamp(),
        hits: hits
            .iter()
            .enumerate()
            .map(|(i, h)| CachedHit {
                rank: i + 1,
                entry_id: h.id,
                session_id: h.session_id.clone(),
                ts: h.ts,
                project_path: h.project_path.clone(),
            })
            .collect(),
    };
    results_cache::write(&cache)
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
    let mode = display_mode(args.full, args.raw);
    let out = render_prompts(&hits, args.format, color, mode)?;
    print!("{}", out);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hit(session_id: &str, score: f64) -> Hit {
        Hit {
            id: 0,
            session_id: session_id.to_string(),
            ts: 0,
            project_path: "/p".to_string(),
            kind: "qa".to_string(),
            question: None,
            command: None,
            stdout: None,
            stderr: None,
            summary: None,
            score,
            is_subagent: false,
            parent_session_id: None,
            subagent_description: None,
            raw_matches: None,
            more_in_session: 0,
        }
    }

    #[test]
    fn dedupe_collapses_same_session_with_count() {
        let hits = vec![
            hit("a", -5.0),
            hit("a", -4.5),
            hit("a", -4.0),
            hit("b", -3.0),
        ];
        let out = dedupe_by_session(hits, 10);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].session_id, "a");
        assert_eq!(out[0].more_in_session, 2);
        assert_eq!(out[1].session_id, "b");
        assert_eq!(out[1].more_in_session, 0);
    }

    #[test]
    fn dedupe_preserves_order_by_first_appearance() {
        let hits = vec![hit("z", -10.0), hit("a", -9.0), hit("z", -1.0)];
        let out = dedupe_by_session(hits, 10);
        assert_eq!(out[0].session_id, "z");
        assert_eq!(out[1].session_id, "a");
    }

    #[test]
    fn dedupe_truncates_to_limit() {
        let hits = vec![
            hit("a", -5.0),
            hit("b", -4.0),
            hit("c", -3.0),
            hit("d", -2.0),
        ];
        let out = dedupe_by_session(hits, 2);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].session_id, "a");
        assert_eq!(out[1].session_id, "b");
    }

    #[test]
    fn dedupe_handles_empty_input() {
        let out = dedupe_by_session(Vec::new(), 5);
        assert!(out.is_empty());
    }

    #[test]
    fn dedupe_keeps_first_hits_score() {
        let hits = vec![hit("a", -10.0), hit("a", -5.0)];
        let out = dedupe_by_session(hits, 5);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].score, -10.0);
    }
}
