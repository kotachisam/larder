use std::path::{Path, PathBuf};

use anyhow::Result;
use serde::Deserialize;
use walkdir::WalkDir;

#[derive(Debug, Clone)]
pub struct TranscriptPath {
    pub session_id: String,
    pub project_path: String,
    pub source_path: PathBuf,
    pub provider: &'static str,
    pub parent_session_id: Option<String>,
    pub is_subagent: bool,
    pub subagent_description: Option<String>,
    pub subagent_type: Option<String>,
}

#[derive(Debug, Deserialize)]
struct SubagentMeta {
    #[serde(rename = "agentType")]
    agent_type: Option<String>,
    description: Option<String>,
}

pub fn walk(root: &Path) -> Result<Vec<TranscriptPath>> {
    if !root.exists() {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    for entry in WalkDir::new(root).min_depth(2).max_depth(4) {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("jsonl") {
            continue;
        }
        let Some(session_id) = path.file_stem().and_then(|s| s.to_str()).map(String::from) else {
            continue;
        };
        let Some(parent) = path.parent() else {
            continue;
        };
        let parent_name = parent.file_name().and_then(|s| s.to_str()).unwrap_or("");

        if parent_name == "subagents" {
            let Some(grandparent) = parent.parent() else {
                continue;
            };
            let Some(parent_session_id) = grandparent
                .file_name()
                .and_then(|s| s.to_str())
                .map(String::from)
            else {
                continue;
            };
            let project_dir = grandparent
                .parent()
                .and_then(|p| p.file_name())
                .and_then(|s| s.to_str())
                .unwrap_or("");
            let (subagent_description, subagent_type) = read_subagent_meta(path);
            out.push(TranscriptPath {
                session_id,
                project_path: decode_project_path(project_dir),
                source_path: path.to_path_buf(),
                provider: "claude",
                parent_session_id: Some(parent_session_id),
                is_subagent: true,
                subagent_description,
                subagent_type,
            });
        } else if parent_name.starts_with('-') && entry.depth() == 2 {
            out.push(TranscriptPath {
                session_id,
                project_path: decode_project_path(parent_name),
                source_path: path.to_path_buf(),
                provider: "claude",
                parent_session_id: None,
                is_subagent: false,
                subagent_description: None,
                subagent_type: None,
            });
        }
    }
    out.sort_by_key(|tp| tp.is_subagent);
    Ok(out)
}

fn read_subagent_meta(jsonl_path: &Path) -> (Option<String>, Option<String>) {
    let meta_path = jsonl_path.with_extension("meta.json");
    let raw = match std::fs::read_to_string(&meta_path) {
        Ok(s) => s,
        Err(_) => return (None, None),
    };
    match serde_json::from_str::<SubagentMeta>(&raw) {
        Ok(m) => (m.description, m.agent_type),
        Err(_) => (None, None),
    }
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
