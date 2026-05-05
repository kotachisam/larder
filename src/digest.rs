use anyhow::Result;
use chrono::{DateTime, Utc};
use rusqlite::params;
use serde::Serialize;

use crate::cli::{DigestArgs, OutputFormat};
use crate::config::Paths;
use crate::store::Store;

#[derive(Debug, Clone, Serialize)]
pub struct DigestEntry {
    pub question: String,
    pub count: i64,
    pub last_seen: i64,
    pub example_command: Option<String>,
}

pub trait Aggregator {
    fn frequency(&self, store: &Store, since: i64, top: usize) -> Result<Vec<DigestEntry>>;
}

pub struct SqlAggregator;

impl Aggregator for SqlAggregator {
    fn frequency(&self, store: &Store, since: i64, top: usize) -> Result<Vec<DigestEntry>> {
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
}

pub fn run(args: DigestArgs) -> Result<()> {
    let paths = Paths::resolve()?;
    let store = Store::open(&paths.db_path)?;
    let since = since_seconds(args.since.as_deref())?;
    let agg = SqlAggregator;
    let entries = agg.frequency(&store, since, args.top)?;
    print!("{}", render_digest(&entries, args.format));
    Ok(())
}

fn since_seconds(spec: Option<&str>) -> Result<i64> {
    let now = Utc::now().timestamp();
    let Some(s) = spec else {
        return Ok(0);
    };
    let dur = humantime::parse_duration(s)
        .map_err(|e| anyhow::anyhow!("invalid --since '{}': {}", s, e))?;
    Ok(now - dur.as_secs() as i64)
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
            truncate(&e.question, 80),
            fmt_ts(e.last_seen)
        );
        if let Some(cmd) = &e.example_command {
            let _ = writeln!(out, "      $ {}", truncate(cmd, 120));
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
            truncate(&e.question, 120).replace('|', "\\|")
        );
    }
    out
}

fn fmt_ts(ts: i64) -> String {
    DateTime::<Utc>::from_timestamp(ts, 0)
        .map(|d| d.format("%Y-%m-%d %H:%M").to_string())
        .unwrap_or_else(|| "?".to_string())
}

fn truncate(s: &str, max: usize) -> String {
    let one_line = s
        .replace('\n', " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    if one_line.len() <= max {
        return one_line;
    }
    let mut end = max;
    while !one_line.is_char_boundary(end) && end > 0 {
        end -= 1;
    }
    format!("{}…", &one_line[..end])
}
