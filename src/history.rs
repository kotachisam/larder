use std::fs::File;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::io::{BufRead, BufReader};
use std::path::Path;

use anyhow::{Context, Result};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tracing::debug;

use crate::store::Store;

#[derive(Debug, Clone, Default, Serialize)]
pub struct HistoryStats {
    pub lines_seen: usize,
    pub prompts_inserted: usize,
    pub prompts_skipped_noise: usize,
    pub prompts_skipped_duplicate: usize,
}

#[derive(Debug, Deserialize)]
struct HistoryLine {
    display: Option<String>,
    timestamp: Option<i64>,
    project: Option<String>,
    #[serde(rename = "pastedContents")]
    pasted_contents: Option<Value>,
}

pub fn ingest(store: &Store, path: &Path) -> Result<HistoryStats> {
    let mut stats = HistoryStats::default();
    if !path.exists() {
        debug!(path = %path.display(), "no history.jsonl, skipping");
        return Ok(stats);
    }
    let file = File::open(path).with_context(|| format!("open {}", path.display()))?;
    let reader = BufReader::new(file);
    let now = Utc::now().timestamp();
    for line in reader.lines() {
        let line = match line {
            Ok(l) if !l.trim().is_empty() => l,
            _ => continue,
        };
        stats.lines_seen += 1;
        let parsed: HistoryLine = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(_) => continue,
        };
        let Some(text) = parsed.display.as_deref().map(str::trim) else {
            stats.prompts_skipped_noise += 1;
            continue;
        };
        if !is_meaningful(text) {
            stats.prompts_skipped_noise += 1;
            continue;
        }
        let ts_secs = parsed.timestamp.map(|ms| ms / 1000).unwrap_or(0);
        let project = parsed.project.unwrap_or_else(|| "?".to_string());
        let pasted_chars = pasted_size(&parsed.pasted_contents);
        let hash = source_hash(ts_secs, text);
        match store.insert_prompt(ts_secs, &project, text, pasted_chars, &hash, now)? {
            true => stats.prompts_inserted += 1,
            false => stats.prompts_skipped_duplicate += 1,
        }
    }
    Ok(stats)
}

fn is_meaningful(text: &str) -> bool {
    if text.is_empty() {
        return false;
    }
    if text.starts_with('/') {
        return false;
    }
    true
}

fn pasted_size(value: &Option<Value>) -> i64 {
    match value {
        Some(Value::Object(map)) => map
            .values()
            .filter_map(|v| v.as_str().map(|s| s.len() as i64))
            .sum(),
        _ => 0,
    }
}

fn source_hash(ts: i64, text: &str) -> String {
    let mut hasher = DefaultHasher::new();
    ts.hash(&mut hasher);
    text.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

pub fn default_path() -> Result<std::path::PathBuf> {
    let base = directories::BaseDirs::new().context("could not resolve home directory")?;
    Ok(base.home_dir().join(".claude").join("history.jsonl"))
}
