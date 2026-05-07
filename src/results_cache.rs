use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::config::Paths;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResultsCache {
    pub produced_by: String,
    pub ts: i64,
    pub hits: Vec<CachedHit>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedHit {
    pub rank: usize,
    pub entry_id: i64,
    pub session_id: String,
    pub ts: i64,
    pub project_path: String,
}

pub fn cache_path() -> Result<PathBuf> {
    let paths = Paths::resolve()?;
    Ok(paths.data_dir.join("last_results.json"))
}

pub fn write(cache: &ResultsCache) -> Result<()> {
    let path = cache_path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
    }
    let json = serde_json::to_string_pretty(cache)?;
    fs::write(&path, json).with_context(|| format!("writing {}", path.display()))?;
    Ok(())
}

pub fn read() -> Result<Option<ResultsCache>> {
    let path = cache_path()?;
    if !path.exists() {
        return Ok(None);
    }
    let bytes = fs::read(&path).with_context(|| format!("reading {}", path.display()))?;
    let cache: ResultsCache =
        serde_json::from_slice(&bytes).with_context(|| format!("parsing {}", path.display()))?;
    Ok(Some(cache))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_serializes_and_deserializes() {
        let original = ResultsCache {
            produced_by: "ask".to_string(),
            ts: 1700000000,
            hits: vec![
                CachedHit {
                    rank: 1,
                    entry_id: 4234,
                    session_id: "abc".to_string(),
                    ts: 1700000000,
                    project_path: "/p".to_string(),
                },
                CachedHit {
                    rank: 2,
                    entry_id: 4235,
                    session_id: "def".to_string(),
                    ts: 1700000100,
                    project_path: "/q".to_string(),
                },
            ],
        };
        let json = serde_json::to_string(&original).unwrap();
        let parsed: ResultsCache = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.produced_by, "ask");
        assert_eq!(parsed.hits.len(), 2);
        assert_eq!(parsed.hits[0].entry_id, 4234);
        assert_eq!(parsed.hits[1].session_id, "def");
    }
}
