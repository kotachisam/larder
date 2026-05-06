use std::collections::HashMap;
use std::fmt::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::Result;
use chrono::{DateTime, Utc};
use owo_colors::OwoColorize;
use serde_json::Value;
use walkdir::WalkDir;

use crate::cli::FindArgs;
use crate::config::Paths;
use crate::search::{Hit, PromptHit, search, search_prompts};
use crate::store::Store;

const FS_SCAN_PATHS: &[&str] = &["Developer", "Documents", ".claude/projects"];
const FS_SKIP_DIRS: &[&str] = &[
    "node_modules",
    "target",
    ".git",
    "dist",
    ".next",
    ".venv",
    "__pycache__",
    "build",
];
const FS_MAX_DEPTH: usize = 6;
const FS_MAX_HITS: usize = 30;

pub fn run(args: FindArgs) -> Result<()> {
    let paths = Paths::resolve()?;
    let store = Store::open(&paths.db_path)?;
    let query = args.query.join(" ");
    if query.trim().is_empty() {
        anyhow::bail!("query is empty");
    }

    let entries = search(&store, &query, args.limit, args.no_subagents)?;
    let prompts = if args.no_prompts {
        Vec::new()
    } else {
        let since_ts = since_seconds(args.since.as_deref())?;
        search_prompts(
            &store,
            &query,
            args.limit,
            since_ts,
            args.project.as_deref(),
        )?
    };
    let raw_grep = if args.no_grep {
        Vec::new()
    } else {
        raw_grep_per_project(&store, &query, args.project.as_deref())?
    };
    let files = if args.no_files {
        Vec::new()
    } else {
        filesystem_find(&query)?
    };

    let color = !args.no_color && atty_stdout();
    print!(
        "{}",
        render(
            &query, &entries, &raw_grep, &prompts, &files, args.limit, color
        )
    );
    Ok(())
}

#[derive(Debug)]
struct RawGrepProject {
    project_path: String,
    match_count: usize,
    last_ts: i64,
}

fn raw_grep_per_project(
    store: &Store,
    query: &str,
    project: Option<&str>,
) -> Result<Vec<RawGrepProject>> {
    let files = store.transcript_paths(project, 0)?;
    if files.is_empty() {
        return Ok(Vec::new());
    }
    let path_to_project = lookup_project_paths(store)?;
    let mut cmd = Command::new("rg");
    cmd.arg("--json")
        .arg("-F")
        .arg("-e")
        .arg(query)
        .args(&files);
    let output = match cmd.output() {
        Ok(o) => o,
        Err(_) => return Ok(Vec::new()),
    };
    let mut counts: HashMap<String, (usize, i64)> = HashMap::new();
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
        let Some(path_str) = v.pointer("/data/path/text").and_then(|x| x.as_str()) else {
            continue;
        };
        let pb = PathBuf::from(path_str);
        let project_path = path_to_project
            .get(&pb)
            .cloned()
            .unwrap_or_else(|| "?".to_string());
        let mtime = std::fs::metadata(&pb)
            .ok()
            .and_then(|m| m.modified().ok())
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        let entry = counts.entry(project_path).or_insert((0, 0));
        entry.0 += 1;
        if mtime > entry.1 {
            entry.1 = mtime;
        }
    }
    let mut out: Vec<RawGrepProject> = counts
        .into_iter()
        .map(|(project_path, (match_count, last_ts))| RawGrepProject {
            project_path,
            match_count,
            last_ts,
        })
        .collect();
    out.sort_by(|a, b| {
        b.match_count
            .cmp(&a.match_count)
            .then_with(|| b.last_ts.cmp(&a.last_ts))
    });
    Ok(out)
}

fn lookup_project_paths(store: &Store) -> Result<HashMap<PathBuf, String>> {
    let conn = store.conn.lock().unwrap();
    let mut stmt = conn.prepare("SELECT source_path, project_path FROM sessions")?;
    let rows = stmt.query_map([], |r| {
        Ok((r.get::<_, String>(0)?.into(), r.get::<_, String>(1)?))
    })?;
    let mut map = HashMap::new();
    for row in rows {
        let (path, project): (PathBuf, String) = row?;
        map.insert(path, project);
    }
    Ok(map)
}

fn filesystem_find(query: &str) -> Result<Vec<PathBuf>> {
    let needle = query.to_lowercase();
    let needle = needle.replace(' ', "-");
    let home = match directories::BaseDirs::new() {
        Some(b) => b.home_dir().to_path_buf(),
        None => return Ok(Vec::new()),
    };
    let mut results = Vec::new();
    for sub in FS_SCAN_PATHS {
        let root = home.join(sub);
        if !root.exists() {
            continue;
        }
        for entry in WalkDir::new(&root)
            .max_depth(FS_MAX_DEPTH)
            .into_iter()
            .filter_entry(|e| {
                let name = e.file_name().to_string_lossy();
                !FS_SKIP_DIRS.iter().any(|skip| name == *skip)
            })
            .flatten()
        {
            if !entry.file_type().is_file() {
                continue;
            }
            let name_lower = entry.file_name().to_string_lossy().to_lowercase();
            if name_lower.contains(&needle) {
                results.push(entry.path().to_path_buf());
                if results.len() >= FS_MAX_HITS {
                    return Ok(results);
                }
            }
        }
    }
    Ok(results)
}

fn render(
    query: &str,
    entries: &[Hit],
    raw_grep: &[RawGrepProject],
    prompts: &[PromptHit],
    files: &[PathBuf],
    limit: usize,
    color: bool,
) -> String {
    let mut out = String::new();
    let title = format!("larder find: \"{}\"", query);
    let _ = if color {
        writeln!(out, "{}", title.bold())
    } else {
        writeln!(out, "{}", title)
    };
    out.push('\n');

    section_header(
        &mut out,
        &format!("Conversations ({} hits)", entries.len()),
        color,
    );
    if entries.is_empty() {
        let _ = writeln!(out, "  no indexed-entry matches");
    } else {
        for (i, h) in entries.iter().take(limit).enumerate() {
            let badge = if h.is_subagent { " [subagent]" } else { "" };
            let _ = writeln!(
                out,
                "  [{}] {} · {}{} · score {:.2}",
                i + 1,
                fmt_ts(h.ts),
                h.project_path,
                badge,
                h.score
            );
            if let Some(q) = &h.question {
                let _ = writeln!(out, "      Q: {}", snip(q, 200));
            }
            if let Some(a) = &h.summary {
                let _ = writeln!(out, "      > {}", snip(a, 220));
            }
        }
    }
    out.push('\n');

    section_header(
        &mut out,
        &format!("Raw transcript matches ({} projects)", raw_grep.len()),
        color,
    );
    if raw_grep.is_empty() {
        let _ = writeln!(out, "  no literal matches in raw transcripts");
    } else {
        for hit in raw_grep.iter().take(limit) {
            let last = if hit.last_ts > 0 {
                format!(" · last edit {}", fmt_ts(hit.last_ts))
            } else {
                String::new()
            };
            let _ = writeln!(
                out,
                "  {:>3} matches in {}{}",
                hit.match_count, hit.project_path, last
            );
        }
    }
    out.push('\n');

    section_header(
        &mut out,
        &format!("Prompt history ({} hits)", prompts.len()),
        color,
    );
    if prompts.is_empty() {
        let _ = writeln!(out, "  no matches in typed-prompt history");
    } else {
        for (i, p) in prompts.iter().take(limit).enumerate() {
            let _ = writeln!(
                out,
                "  [{}] {} · {} [history] · score {:.2}",
                i + 1,
                fmt_ts(p.ts),
                p.project_path,
                p.score
            );
            let _ = writeln!(out, "      Q: {}", snip(&p.prompt_text, 220));
        }
    }
    out.push('\n');

    section_header(
        &mut out,
        &format!("Files on disk ({} hits)", files.len()),
        color,
    );
    if files.is_empty() {
        let _ = writeln!(
            out,
            "  no filename matches under ~/Developer ~/Documents ~/.claude/projects"
        );
    } else {
        for f in files.iter().take(limit * 2) {
            let _ = writeln!(out, "  {}", f.display());
        }
        if files.len() > limit * 2 {
            let _ = writeln!(out, "  … and {} more", files.len() - limit * 2);
        }
    }
    out.push('\n');

    out
}

fn section_header(out: &mut String, title: &str, color: bool) {
    let line = format!("=== {} ===", title);
    let _ = if color {
        writeln!(out, "{}", line.cyan())
    } else {
        writeln!(out, "{}", line)
    };
}

fn fmt_ts(ts: i64) -> String {
    if ts == 0 {
        return "?".to_string();
    }
    DateTime::<Utc>::from_timestamp(ts, 0)
        .map(|d| d.format("%Y-%m-%d %H:%M").to_string())
        .unwrap_or_else(|| "?".to_string())
}

fn snip(s: &str, max: usize) -> String {
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

fn since_seconds(spec: Option<&str>) -> Result<i64> {
    let Some(s) = spec else { return Ok(0) };
    let now = Utc::now().timestamp();
    let dur = humantime::parse_duration(s)
        .map_err(|e| anyhow::anyhow!("invalid --since '{}': {}", s, e))?;
    Ok(now - dur.as_secs() as i64)
}

fn atty_stdout() -> bool {
    use std::io::IsTerminal;
    std::io::stdout().is_terminal()
}

#[allow(dead_code)]
fn _unused(_: &Path) {}
