use std::collections::HashMap;
use std::fmt::Write;
use std::path::PathBuf;
use std::process::Command;

use anyhow::Result;
use owo_colors::OwoColorize;
use serde_json::Value;
use walkdir::WalkDir;

use crate::cli::FindArgs;
use crate::config::Paths;
use crate::search::{Hit, PromptHit, search, search_prompts, write_cache};
use crate::store::Store;
use crate::util::{atty_stdout, clean_for_display, fmt_ts, since_seconds, snip};

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
    let _ = write_cache("find", &entries);
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
    let path_to_project = store.source_paths_to_projects()?;
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

fn filesystem_find(query: &str) -> Result<Vec<PathBuf>> {
    let q_lower = query.to_lowercase();
    let q_tokens: Vec<&str> = q_lower
        .split(|c: char| !c.is_alphanumeric())
        .filter(|t| !t.is_empty())
        .collect();
    if q_tokens.is_empty() {
        return Ok(Vec::new());
    }
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
            if filename_matches_tokens(&name_lower, &q_tokens) {
                results.push(entry.path().to_path_buf());
                if results.len() >= FS_MAX_HITS {
                    return Ok(results);
                }
            }
        }
    }
    Ok(results)
}

fn filename_matches_tokens(filename_lower: &str, query_tokens: &[&str]) -> bool {
    let file_tokens: Vec<&str> = filename_lower
        .split(|c: char| !c.is_alphanumeric())
        .filter(|t| !t.is_empty())
        .collect();
    query_tokens
        .iter()
        .all(|qt| file_tokens.iter().any(|ft| ft == qt))
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
                "  [{}] {} · {}{}",
                i + 1,
                fmt_ts(h.ts),
                h.project_path,
                badge,
            );
            if let Some(q) = &h.question {
                let _ = writeln!(out, "      Q: {}", snip(&clean_for_display(q), 200, false));
            }
            if let Some(a) = &h.summary {
                let _ = writeln!(out, "      > {}", snip(&clean_for_display(a), 220, false));
            }
            if h.more_in_session > 0 {
                let _ = writeln!(
                    out,
                    "      +{} more {} in this session",
                    h.more_in_session,
                    if h.more_in_session == 1 {
                        "match"
                    } else {
                        "matches"
                    }
                );
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
                "  [{}] {} · {} [history]",
                i + 1,
                fmt_ts(p.ts),
                p.project_path,
            );
            let _ = writeln!(
                out,
                "      Q: {}",
                snip(&clean_for_display(&p.prompt_text), 220, false)
            );
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

#[cfg(test)]
mod tests {
    use super::*;

    fn tokens(q: &str) -> Vec<&str> {
        q.split(|c: char| !c.is_alphanumeric())
            .filter(|t| !t.is_empty())
            .collect()
    }

    #[test]
    fn aws_does_not_match_jaws() {
        let q = tokens("aws");
        assert!(!filename_matches_tokens("sp-jaws-outdoor.webp", &q));
        assert!(!filename_matches_tokens("australia-uranium-laws.md", &q));
        assert!(!filename_matches_tokens(
            "class-action-lawsuit-advice.md",
            &q
        ));
    }

    #[test]
    fn aws_matches_aws_named_files() {
        let q = tokens("aws");
        assert!(filename_matches_tokens("aws-deploy.sh", &q));
        assert!(filename_matches_tokens("setup_aws_keys.md", &q));
        assert!(filename_matches_tokens("aws.config", &q));
    }

    #[test]
    fn multi_token_query_requires_all_tokens() {
        let q = tokens("new feature");
        assert!(filename_matches_tokens("new-feature.md", &q));
        assert!(filename_matches_tokens("new_feature_proposal.md", &q));
        assert!(!filename_matches_tokens("new-readme.md", &q));
        assert!(!filename_matches_tokens("feature-flag.ts", &q));
    }

    #[test]
    fn case_insensitive_match() {
        let q = tokens("aws");
        assert!(filename_matches_tokens(
            "aws-config.md".to_lowercase().as_str(),
            &q
        ));
    }

    #[test]
    fn empty_query_returns_no_results() {
        assert!(filesystem_find("").unwrap().is_empty());
        assert!(filesystem_find("   ").unwrap().is_empty());
    }
}
