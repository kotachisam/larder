use anyhow::Result;
use rusqlite::params;
use serde::Serialize;

use crate::cli::AskArgs;
use crate::config::Paths;
use crate::format::{render_command_only, render_hits};
use crate::store::Store;

#[derive(Debug, Clone, Serialize)]
pub struct Hit {
    pub id: i64,
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
            e.id, e.ts, s.project_path, e.kind,
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
            ts: r.get(1)?,
            project_path: r.get(2)?,
            kind: r.get(3)?,
            question: r.get(4)?,
            command: r.get(5)?,
            stdout: r.get(6)?,
            stderr: r.get(7)?,
            summary: r.get(8)?,
            score: r.get(9)?,
            is_subagent: r.get::<_, i64>(10)? != 0,
            parent_session_id: r.get(11)?,
            subagent_description: r.get(12)?,
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

fn atty_stdout() -> bool {
    use std::io::IsTerminal;
    std::io::stdout().is_terminal()
}
