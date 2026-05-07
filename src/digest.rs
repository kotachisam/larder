use anyhow::Result;
use rusqlite::params;
use serde::Serialize;

use crate::cli::{DigestArgs, OutputFormat};
use crate::config::Paths;
use crate::store::Store;
use crate::util::{clean_for_display, fmt_ts, since_seconds, snip};

#[derive(Debug, Clone, Serialize)]
pub struct DigestEntry {
    pub question: String,
    pub count: i64,
    pub last_seen: i64,
    pub example_command: Option<String>,
}

pub fn frequency(store: &Store, since: i64, top: usize) -> Result<Vec<DigestEntry>> {
    let conn = store.conn.lock().unwrap();
    let mut stmt = conn.prepare(
        r#"
        SELECT
            LOWER(TRIM(question))     AS qkey,
            COUNT(*)                  AS n,
            MAX(ts)                   AS last_seen,
            (
                SELECT command FROM entries e2
                WHERE LOWER(TRIM(e2.question)) = LOWER(TRIM(entries.question))
                  AND e2.command IS NOT NULL
                ORDER BY ts DESC LIMIT 1
            )                         AS example_command
        FROM entries
        WHERE question IS NOT NULL
          AND TRIM(question) <> ''
          AND ts >= ?1
        GROUP BY qkey
        ORDER BY n DESC, last_seen DESC
        LIMIT ?2
        "#,
    )?;
    let rows = stmt.query_map(params![since, top as i64], |r| {
        Ok(DigestEntry {
            question: r.get::<_, String>(0)?,
            count: r.get(1)?,
            last_seen: r.get(2)?,
            example_command: r.get(3)?,
        })
    })?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}

pub fn run(args: DigestArgs) -> Result<()> {
    let paths = Paths::resolve()?;
    let store = Store::open(&paths.db_path)?;
    let since = since_seconds(args.since.as_deref())?;
    let entries = frequency(&store, since, args.top)?;
    print!("{}", render_digest(&entries, args.format));
    Ok(())
}

fn render_digest(entries: &[DigestEntry], format: OutputFormat) -> String {
    if entries.is_empty() {
        return match format {
            OutputFormat::Json => "[]\n".to_string(),
            _ => "no questions in window\n".to_string(),
        };
    }
    match format {
        OutputFormat::Json => serde_json::to_string_pretty(entries).unwrap_or_default() + "\n",
        OutputFormat::Md => render_md(entries),
        OutputFormat::Text => render_text(entries),
    }
}

fn render_text(entries: &[DigestEntry]) -> String {
    use std::fmt::Write;
    let mut out = String::new();
    for (i, e) in entries.iter().enumerate() {
        let _ = writeln!(
            out,
            "{:>2}. ×{:<4} {}  ({})",
            i + 1,
            e.count,
            snip(&clean_for_display(&e.question), 80, false),
            fmt_ts(e.last_seen)
        );
        if let Some(cmd) = &e.example_command {
            let _ = writeln!(out, "      $ {}", snip(cmd, 120, false));
        }
    }
    out
}

fn render_md(entries: &[DigestEntry]) -> String {
    use std::fmt::Write;
    let mut out = String::new();
    let _ = writeln!(out, "| # | Count | Last Seen | Question |");
    let _ = writeln!(out, "|---|-------|-----------|----------|");
    for (i, e) in entries.iter().enumerate() {
        let _ = writeln!(
            out,
            "| {} | {} | {} | {} |",
            i + 1,
            e.count,
            fmt_ts(e.last_seen),
            snip(&clean_for_display(&e.question), 120, false).replace('|', "\\|")
        );
    }
    out
}
