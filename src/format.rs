use std::fmt::Write;

use anyhow::Result;
use owo_colors::OwoColorize;

use crate::cli::OutputFormat;
use crate::search::{Hit, PromptHit};
use crate::util::{fmt_ts, snip};

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
        let badge = subagent_badge(h, false);
        let score_or_hits = match h.raw_matches {
            Some(n) => format!("{} raw matches", n),
            None => format!("score {:.2}", h.score),
        };
        let header = format!(
            "[{}] {} · {}{} · {}",
            i + 1,
            fmt_ts(h.ts),
            h.project_path,
            badge,
            score_or_hits
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
            let _ = writeln!(out, "  {} {}", label, snip(q, 240, raw));
        }
        if let Some(cmd) = &h.command {
            let label = if color {
                "$".green().to_string()
            } else {
                "$".to_string()
            };
            let _ = writeln!(out, "  {} {}", label, snip(cmd, 320, raw));
        }
        if let Some(stdout) = &h.stdout {
            let _ = writeln!(out, "  ↳ {}", snip(stdout, 240, raw));
        }
        if let Some(summary) = &h.summary
            && h.command.is_none()
        {
            let _ = writeln!(out, "  > {}", snip(summary, 280, raw));
        }
        out.push('\n');
    }
    out
}

fn render_markdown(hits: &[Hit], raw: bool) -> String {
    let mut out = String::new();
    for (i, h) in hits.iter().enumerate() {
        let badge = subagent_badge(h, true);
        let _ = writeln!(
            out,
            "### {}. {} — `{}`{}",
            i + 1,
            fmt_ts(h.ts),
            h.project_path,
            badge
        );
        if let Some(q) = &h.question {
            let _ = writeln!(out, "**Q:** {}", snip(q, 320, raw));
        }
        if let Some(cmd) = &h.command {
            let _ = writeln!(out, "```bash\n{}\n```", snip(cmd, 800, raw));
        }
        if let Some(stdout) = &h.stdout {
            let _ = writeln!(out, "```\n{}\n```", snip(stdout, 800, raw));
        }
        if let Some(summary) = &h.summary
            && h.command.is_none()
        {
            let _ = writeln!(out, "> {}", snip(summary, 600, raw));
        }
        out.push('\n');
    }
    out
}

pub fn render_prompts(hits: &[PromptHit], format: OutputFormat, color: bool) -> Result<String> {
    if hits.is_empty() {
        return Ok(match format {
            OutputFormat::Json => "[]\n".to_string(),
            _ => "no matches\n".to_string(),
        });
    }
    match format {
        OutputFormat::Json => Ok(serde_json::to_string_pretty(hits)? + "\n"),
        OutputFormat::Md => Ok(render_prompts_md(hits)),
        OutputFormat::Text => Ok(render_prompts_text(hits, color)),
    }
}

fn render_prompts_text(hits: &[PromptHit], color: bool) -> String {
    let mut out = String::new();
    for (i, h) in hits.iter().enumerate() {
        let extra = if h.pasted_chars > 0 {
            format!(" · {} pasted chars", h.pasted_chars)
        } else {
            String::new()
        };
        let header = format!(
            "[{}] {} · {} [history]{} · score {:.2}",
            i + 1,
            fmt_ts(h.ts),
            h.project_path,
            extra,
            h.score
        );
        let _ = if color {
            writeln!(out, "{}", header.bold())
        } else {
            writeln!(out, "{}", header)
        };
        let label = if color {
            "Q:".cyan().to_string()
        } else {
            "Q:".to_string()
        };
        let _ = writeln!(out, "  {} {}", label, snip(&h.prompt_text, 320, false));
        out.push('\n');
    }
    out
}

fn render_prompts_md(hits: &[PromptHit]) -> String {
    let mut out = String::new();
    for (i, h) in hits.iter().enumerate() {
        let _ = writeln!(
            out,
            "### {}. {} — `{}` _(history)_",
            i + 1,
            fmt_ts(h.ts),
            h.project_path
        );
        let _ = writeln!(out, "**Q:** {}", snip(&h.prompt_text, 800, false));
        out.push('\n');
    }
    out
}

fn subagent_badge(h: &Hit, italic: bool) -> String {
    if !h.is_subagent {
        return String::new();
    }
    let label = match h.subagent_description.as_deref() {
        Some(d) if !d.is_empty() => format!("subagent: \"{}\"", snip(d, 60, false)),
        _ => "subagent".to_string(),
    };
    if italic {
        format!(" _({})_", label)
    } else {
        format!(" [{}]", label)
    }
}

