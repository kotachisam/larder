use std::fmt::Write;

use anyhow::Result;
use owo_colors::OwoColorize;

use crate::cli::OutputFormat;
use crate::search::{Hit, PromptHit};
use crate::util::{DisplayMode, clean_for_display, fmt_ts, snip};

pub fn render_hits(
    hits: &[Hit],
    format: OutputFormat,
    color: bool,
    mode: DisplayMode,
) -> Result<String> {
    if hits.is_empty() {
        return Ok(match format {
            OutputFormat::Json => "[]\n".to_string(),
            _ => "no matches\n".to_string(),
        });
    }
    match format {
        OutputFormat::Json => Ok(serde_json::to_string_pretty(hits)? + "\n"),
        OutputFormat::Md => Ok(render_markdown(hits, mode)),
        OutputFormat::Text => Ok(render_text(hits, color, mode)),
    }
}

pub fn render_command_only(hits: &[Hit]) -> Option<String> {
    hits.iter().find_map(|h| h.command.clone())
}

fn fmt_prose(s: &str, max: usize, mode: DisplayMode) -> String {
    match mode {
        DisplayMode::Compact => snip(&clean_for_display(s), max, false),
        DisplayMode::Full => clean_for_display(s),
        DisplayMode::Raw => s.to_string(),
    }
}

fn fmt_literal(s: &str, max: usize, mode: DisplayMode) -> String {
    match mode {
        DisplayMode::Compact => snip(s, max, false),
        DisplayMode::Full | DisplayMode::Raw => s.to_string(),
    }
}

fn render_text(hits: &[Hit], color: bool, mode: DisplayMode) -> String {
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
            let _ = writeln!(out, "  {} {}", label, fmt_prose(q, 240, mode));
        }
        if let Some(cmd) = &h.command {
            let label = if color {
                "$".green().to_string()
            } else {
                "$".to_string()
            };
            let _ = writeln!(out, "  {} {}", label, fmt_literal(cmd, 320, mode));
        }
        if let Some(stdout) = &h.stdout {
            let _ = writeln!(out, "  ↳ {}", fmt_literal(stdout, 240, mode));
        }
        if let Some(summary) = &h.summary
            && h.command.is_none()
        {
            let _ = writeln!(out, "  > {}", fmt_prose(summary, 280, mode));
        }
        out.push('\n');
    }
    out
}

fn render_markdown(hits: &[Hit], mode: DisplayMode) -> String {
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
            let _ = writeln!(out, "**Q:** {}", fmt_prose(q, 320, mode));
        }
        if let Some(cmd) = &h.command {
            let _ = writeln!(out, "```bash\n{}\n```", fmt_literal(cmd, 800, mode));
        }
        if let Some(stdout) = &h.stdout {
            let _ = writeln!(out, "```\n{}\n```", fmt_literal(stdout, 800, mode));
        }
        if let Some(summary) = &h.summary
            && h.command.is_none()
        {
            let _ = writeln!(out, "> {}", fmt_prose(summary, 600, mode));
        }
        out.push('\n');
    }
    out
}

pub fn render_prompts(
    hits: &[PromptHit],
    format: OutputFormat,
    color: bool,
    mode: DisplayMode,
) -> Result<String> {
    if hits.is_empty() {
        return Ok(match format {
            OutputFormat::Json => "[]\n".to_string(),
            _ => "no matches\n".to_string(),
        });
    }
    match format {
        OutputFormat::Json => Ok(serde_json::to_string_pretty(hits)? + "\n"),
        OutputFormat::Md => Ok(render_prompts_md(hits, mode)),
        OutputFormat::Text => Ok(render_prompts_text(hits, color, mode)),
    }
}

fn render_prompts_text(hits: &[PromptHit], color: bool, mode: DisplayMode) -> String {
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
        let _ = writeln!(out, "  {} {}", label, fmt_prose(&h.prompt_text, 320, mode));
        out.push('\n');
    }
    out
}

fn render_prompts_md(hits: &[PromptHit], mode: DisplayMode) -> String {
    let mut out = String::new();
    for (i, h) in hits.iter().enumerate() {
        let _ = writeln!(
            out,
            "### {}. {} — `{}` _(history)_",
            i + 1,
            fmt_ts(h.ts),
            h.project_path
        );
        let _ = writeln!(out, "**Q:** {}", fmt_prose(&h.prompt_text, 800, mode));
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
