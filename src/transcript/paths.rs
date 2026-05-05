use std::path::{Path, PathBuf};

use anyhow::Result;
use walkdir::WalkDir;

#[derive(Debug, Clone)]
pub struct TranscriptPath {
    pub session_id: String,
    pub project_path: String,
    pub source_path: PathBuf,
    pub provider: &'static str,
}

pub fn walk(root: &Path) -> Result<Vec<TranscriptPath>> {
    if !root.exists() {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    for entry in WalkDir::new(root).min_depth(2).max_depth(2) {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("jsonl") {
            continue;
        }
        let session_id = match path.file_stem().and_then(|s| s.to_str()) {
            Some(s) => s.to_string(),
            None => continue,
        };
        let project_dir = match path
            .parent()
            .and_then(|p| p.file_name())
            .and_then(|s| s.to_str())
        {
            Some(s) => s,
            None => continue,
        };
        out.push(TranscriptPath {
            session_id,
            project_path: decode_project_path(project_dir),
            source_path: path.to_path_buf(),
            provider: "claude",
        });
    }
    Ok(out)
}

pub fn decode_project_path(encoded: &str) -> String {
    let trimmed = encoded.strip_prefix('-').unwrap_or(encoded);
    format!("/{}", trimmed.replace('-', "/"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_typical_project_dir() {
        assert_eq!(
            decode_project_path("-Users-sam-r-Developer-oss-larder"),
            "/Users/sam/r/Developer/oss/larder"
        );
    }
}
