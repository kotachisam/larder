use std::collections::HashMap;
use std::fmt::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};
use owo_colors::OwoColorize;
use serde_json::Value;

use crate::cli::GrepArgs;
use crate::config::Paths;
use crate::results_cache::{self, CachedHit, ResultsCache};
use crate::search::{Hit, hits_by_entry_ids};
use crate::store::Store;
use crate::util::{atty_stdout, clean_for_display, fmt_ts, since_seconds, snip};

pub fn run(args: GrepArgs) -> Result<()> {
    let paths = Paths::resolve()?;
    let store = Store::open(&paths.db_path)?;
    let since_ts = since_seconds(args.since.as_deref())?;
    let files = if let Some(custom_path) = &args.path {
        collect_files_from_disk(custom_path)?
    } else {
        store.transcript_paths(args.project.as_deref(), since_ts)?
    };
    if files.is_empty() {
        eprintln!("no transcripts match filters (run `larder ingest` if you haven't yet)");
        std::process::exit(1);
    }
    if args.raw {
        run_raw(&args, &files)
    } else {
        run_pretty(&args, &store, &files)
    }
}

fn run_raw(args: &GrepArgs, files: &[PathBuf]) -> Result<()> {
    let color = if args.no_color { "never" } else { "auto" };
    let mut cmd = Command::new("rg");
    cmd.arg(format!("--color={}", color));
    cmd.arg("--heading");
    cmd.arg("--line-number");
    if !args.rg_args.iter().any(|a| a.starts_with("--max-columns")) {
        cmd.arg("--max-columns=300");
        cmd.arg("--max-columns-preview");
    }
    if args.literal {
        cmd.arg("-F");
    }
    for extra in &args.rg_args {
        cmd.arg(extra);
    }
    cmd.arg("-e").arg(&args.pattern);
    cmd.args(files);
    match cmd.status() {
        Ok(status) => std::process::exit(status.code().unwrap_or(1)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            bail!("ripgrep (rg) not found in PATH. install with: brew install ripgrep")
        }
        Err(e) => Err(e).context("spawning rg"),
    }
}

fn run_pretty(args: &GrepArgs, store: &Store, files: &[PathBuf]) -> Result<()> {
    let matches = run_rg_json(args, files)?;
    if matches.is_empty() {
        println!("no matches");
        return Ok(());
    }
    let mut counts: HashMap<i64, usize> = HashMap::new();
    for m in &matches {
        let Some(session_id) = path_to_session_id(&m.file) else {
            continue;
        };
        if let Ok(Some(entry_id)) = store.entry_at_or_before(&session_id, m.line) {
            *counts.entry(entry_id).or_insert(0) += 1;
        }
    }
    if counts.is_empty() {
        println!(
            "{} raw match(es) found but none mapped to indexed entries — run `larder ingest` first?",
            matches.len()
        );
        return Ok(());
    }
    let entry_ids: Vec<i64> = counts.keys().copied().collect();
    let mut hits = hits_by_entry_ids(store, &entry_ids)?;
    for hit in &mut hits {
        hit.raw_matches = counts.get(&hit.id).copied();
    }
    let mut groups = group_into_turns(store, hits)?;
    if args.by_hits {
        groups.sort_by(|a, b| {
            b.total_matches
                .cmp(&a.total_matches)
                .then_with(|| b.ts.cmp(&a.ts))
        });
    } else {
        groups.sort_by_key(|g| std::cmp::Reverse(g.ts));
    }
    groups.truncate(args.limit);
    let _ = write_grep_cache(&groups);
    let color = !args.no_color && atty_stdout();
    print!("{}", render_groups(&groups, color));
    Ok(())
}

fn write_grep_cache(groups: &[GrepGroup]) -> Result<()> {
    let cache = ResultsCache {
        produced_by: "grep".to_string(),
        ts: chrono::Utc::now().timestamp(),
        hits: groups
            .iter()
            .enumerate()
            .map(|(i, g)| CachedHit {
                rank: i + 1,
                entry_id: g.representative_entry_id,
                session_id: g.session_id.clone(),
                ts: g.ts,
                project_path: g.project_path.clone(),
            })
            .collect(),
    };
    results_cache::write(&cache)
}

#[derive(Debug)]
struct GrepGroup {
    ts: i64,
    session_id: String,
    representative_entry_id: i64,
    project_path: String,
    is_subagent: bool,
    subagent_description: Option<String>,
    question: Option<String>,
    answer_summary: Option<String>,
    commands: Vec<GrepCommand>,
    qa_only_matches: usize,
    total_matches: usize,
}

#[derive(Debug)]
struct GrepCommand {
    command: String,
    stdout: Option<String>,
    matches: usize,
}

fn group_into_turns(store: &Store, hits: Vec<Hit>) -> Result<Vec<GrepGroup>> {
    let mut buckets: HashMap<(String, String), GrepGroup> = HashMap::new();
    for h in hits {
        let q_key = h.question.as_deref().unwrap_or("").trim().to_lowercase();
        let key = (h.session_id.clone(), q_key);
        let matches = h.raw_matches.unwrap_or(0);
        let entry = buckets.entry(key).or_insert_with(|| GrepGroup {
            ts: h.ts,
            session_id: h.session_id.clone(),
            representative_entry_id: h.id,
            project_path: h.project_path.clone(),
            is_subagent: h.is_subagent,
            subagent_description: h.subagent_description.clone(),
            question: h.question.clone(),
            answer_summary: None,
            commands: Vec::new(),
            qa_only_matches: 0,
            total_matches: 0,
        });
        entry.ts = entry.ts.max(h.ts);
        entry.total_matches += matches;
        if let Some(cmd) = h.command {
            entry.commands.push(GrepCommand {
                command: cmd,
                stdout: h.stdout,
                matches,
            });
        } else if h.kind == "qa" {
            if entry.answer_summary.is_none() {
                entry.answer_summary = h.summary.clone();
            }
            entry.qa_only_matches += matches;
        }
    }
    for ((sid, _q_key), group) in buckets.iter_mut() {
        if group.answer_summary.is_none()
            && let Some(q) = &group.question
        {
            group.answer_summary = store.qa_summary_for(sid, q).unwrap_or(None);
        }
    }
    Ok(buckets.into_values().collect())
}

fn render_groups(groups: &[GrepGroup], color: bool) -> String {
    if groups.is_empty() {
        return "no matches\n".to_string();
    }
    let mut out = String::new();
    for (i, g) in groups.iter().enumerate() {
        let badge = subagent_badge(g);
        let total = g.total_matches.max(g.qa_only_matches);
        let header = if g.commands.is_empty() {
            format!(
                "[{}] {} · {}{} · {} {} (qa-only)",
                i + 1,
                fmt_ts(g.ts),
                g.project_path,
                badge,
                total,
                pluralize(total, "match", "matches"),
            )
        } else {
            format!(
                "[{}] {} · {}{} · {} {} across {} {}",
                i + 1,
                fmt_ts(g.ts),
                g.project_path,
                badge,
                total,
                pluralize(total, "match", "matches"),
                g.commands.len(),
                pluralize(g.commands.len(), "command", "commands"),
            )
        };
        let _ = if color {
            writeln!(out, "{}", header.bold())
        } else {
            writeln!(out, "{}", header)
        };
        if let Some(q) = &g.question {
            let label = if color {
                "Q:".cyan().to_string()
            } else {
                "Q:".to_string()
            };
            let _ = writeln!(
                out,
                "  {} {}",
                label,
                snip(&clean_for_display(q), 240, false)
            );
        }
        if let Some(a) = &g.answer_summary {
            let label = if color {
                ">".magenta().to_string()
            } else {
                ">".to_string()
            };
            let _ = writeln!(
                out,
                "  {} {}",
                label,
                snip(&clean_for_display(a), 320, false)
            );
        }
        for cmd in &g.commands {
            let prompt = if color {
                "$".green().to_string()
            } else {
                "$".to_string()
            };
            let count_marker = if cmd.matches > 0 {
                format!(
                    "  ({} {})",
                    cmd.matches,
                    pluralize(cmd.matches, "match", "matches")
                )
            } else {
                String::new()
            };
            let _ = writeln!(
                out,
                "  {} {}{}",
                prompt,
                snip(&cmd.command, 280, false),
                count_marker
            );
            if let Some(stdout) = &cmd.stdout {
                let _ = writeln!(out, "    ↳ {}", snip(stdout, 200, false));
            }
        }
        out.push('\n');
    }
    out
}

fn pluralize<'a>(n: usize, one: &'a str, many: &'a str) -> &'a str {
    if n == 1 { one } else { many }
}

fn subagent_badge(g: &GrepGroup) -> String {
    if !g.is_subagent {
        return String::new();
    }
    match g.subagent_description.as_deref() {
        Some(d) if !d.is_empty() => format!(
            " [subagent: \"{}\"]",
            snip(&clean_for_display(d), 60, false)
        ),
        _ => " [subagent]".to_string(),
    }
}

#[derive(Debug)]
struct RgMatch {
    file: PathBuf,
    line: i64,
}

fn run_rg_json(args: &GrepArgs, files: &[PathBuf]) -> Result<Vec<RgMatch>> {
    let mut cmd = Command::new("rg");
    cmd.arg("--json");
    if args.literal {
        cmd.arg("-F");
    }
    for extra in &args.rg_args {
        cmd.arg(extra);
    }
    cmd.arg("-e").arg(&args.pattern);
    cmd.args(files);
    let output = match cmd.output() {
        Ok(o) => o,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            bail!("ripgrep (rg) not found in PATH. install with: brew install ripgrep")
        }
        Err(e) => return Err(e).context("spawning rg"),
    };
    let mut out = Vec::new();
    for line in output.stdout.split(|&b| b == b'\n') {
        if line.is_empty() {
            continue;
        }
        let v: Value = match serde_json::from_slice(line) {
            Ok(v) => v,
            Err(_) => continue,
        };
        if v.get("type").and_then(|t| t.as_str()) != Some("match") {
            continue;
        }
        let file = v
            .pointer("/data/path/text")
            .and_then(|x| x.as_str())
            .map(PathBuf::from);
        let line_num = v.pointer("/data/line_number").and_then(|x| x.as_i64());
        if let (Some(file), Some(line_num)) = (file, line_num) {
            out.push(RgMatch {
                file,
                line: line_num,
            });
        }
    }
    Ok(out)
}

fn path_to_session_id(path: &Path) -> Option<String> {
    path.file_stem()
        .and_then(|s| s.to_str())
        .map(|s| s.to_string())
}

fn collect_files_from_disk(root: &Path) -> Result<Vec<PathBuf>> {
    let transcripts = crate::transcript::walk(root)?;
    Ok(transcripts.into_iter().map(|tp| tp.source_path).collect())
}
