use std::fmt::Write;

use anyhow::Result;
use chrono::{DateTime, Utc};
use owo_colors::OwoColorize;

use crate::cli::OutputFormat;
use crate::search::Hit;

pub fn render_hits(hits: &[Hit], format: OutputFormat, color: bool, raw: bool) -> Result<String> {
    if hits.is_empty() {
        return Ok(match format {
            OutputFormat::Json => "[]\n".to_string(),
            _ => "no matches\n".to_string(),
        });
    }
    match format {
        OutputFormat::Json => Ok(serde_json::to_string_pretty(hits)? + "\n"),
        OutputFormat::Md => Ok(render_markdown(hits, raw)),
        OutputFormat::Text => Ok(render_text(hits, color, raw)),
    }
}

pub fn render_command_only(hits: &[Hit]) -> Option<String> {
    hits.iter().find_map(|h| h.command.clone())
}

fn render_text(hits: &[Hit], color: bool, raw: bool) -> String {
    let mut out = String::new();
    for (i, h) in hits.iter().enumerate() {
        let header = format!(
            "[{}] {} · {} · score {:.2}",
            i + 1,
            fmt_ts(h.ts),
            h.project_path,
            h.score
        );
        if color {
            let _ = writeln!(out, "{}", header.bold());
        } else {
            let _ = writeln!(out, "{}", header);
        }
        if let Some(q) = &h.question {
            let label = if color {
                "Q:".cyan().to_string()
            } else {
                "Q:".to_string()
            };
            let _ = writeln!(out, "  {} {}", label, snip(q, raw, 240));
        }
        if let Some(cmd) = &h.command {
            let label = if color {
                "$".green().to_string()
            } else {
                "$".to_string()
            };
            let _ = writeln!(out, "  {} {}", label, snip(cmd, raw, 320));
        }
        if let Some(stdout) = &h.stdout {
            let _ = writeln!(out, "  ↳ {}", snip(stdout, raw, 240));
        }
        if let Some(summary) = &h.summary
            && h.command.is_none()
        {
            let _ = writeln!(out, "  > {}", snip(summary, raw, 280));
        }
        out.push('\n');
    }
    out
}

fn render_markdown(hits: &[Hit], raw: bool) -> String {
    let mut out = String::new();
    for (i, h) in hits.iter().enumerate() {
        let _ = writeln!(
            out,
            "### {}. {} — `{}`",
            i + 1,
            fmt_ts(h.ts),
            h.project_path
        );
        if let Some(q) = &h.question {
            let _ = writeln!(out, "**Q:** {}", snip(q, raw, 320));
        }
        if let Some(cmd) = &h.command {
            let _ = writeln!(out, "```bash\n{}\n```", snip(cmd, raw, 800));
        }
        if let Some(stdout) = &h.stdout {
            let _ = writeln!(out, "```\n{}\n```", snip(stdout, raw, 800));
        }
        if let Some(summary) = &h.summary
            && h.command.is_none()
        {
            let _ = writeln!(out, "> {}", snip(summary, raw, 600));
        }
        out.push('\n');
    }
    out
}

fn fmt_ts(ts: i64) -> String {
    DateTime::<Utc>::from_timestamp(ts, 0)
        .map(|d| d.format("%Y-%m-%d %H:%M").to_string())
        .unwrap_or_else(|| "?".to_string())
}

fn snip(s: &str, raw: bool, max: usize) -> String {
    let collapsed = if raw {
        s.to_string()
    } else {
        s.replace('\n', " ")
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
    };
    if collapsed.len() <= max {
        collapsed
    } else {
        let mut end = max;
        while !collapsed.is_char_boundary(end) && end > 0 {
            end -= 1;
        }
        format!("{}…", &collapsed[..end])
    }
}
