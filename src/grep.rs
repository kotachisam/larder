use std::path::PathBuf;
use std::process::Command;
use std::time::SystemTime;

use anyhow::{Context, Result, bail};
use chrono::Utc;

use crate::cli::GrepArgs;
use crate::config::Paths;
use crate::transcript::walk;

pub fn run(args: GrepArgs) -> Result<()> {
    let paths = Paths::resolve()?;
    let root = args.path.clone().unwrap_or(paths.transcripts_dir.clone());
    let since_ts = since_seconds(args.since.as_deref())?;
    let transcripts = walk(&root)?;

    let files: Vec<PathBuf> = transcripts
        .into_iter()
        .filter(|tp| match &args.project {
            Some(p) => tp.project_path == *p || tp.project_path.starts_with(p),
            None => true,
        })
        .filter(|tp| since_ts == 0 || file_mtime(&tp.source_path) >= since_ts)
        .map(|tp| tp.source_path)
        .collect();

    if files.is_empty() {
        eprintln!("no transcripts match filters");
        std::process::exit(1);
    }

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
    cmd.args(&files);

    match cmd.status() {
        Ok(status) => std::process::exit(status.code().unwrap_or(1)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            bail!("ripgrep (rg) not found in PATH. install with: brew install ripgrep")
        }
        Err(e) => Err(e).context("spawning rg"),
    }
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

fn file_mtime(path: &std::path::Path) -> i64 {
    std::fs::metadata(path)
        .ok()
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}
