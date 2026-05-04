use std::path::{Path, PathBuf};

use anyhow::Result;

pub struct TranscriptPath {
    pub session_id: String,
    pub project_path: String,
    pub source_path: PathBuf,
}

pub fn walk(_root: &Path) -> Result<Vec<TranscriptPath>> {
    todo!("walk transcripts dir")
}

pub fn decode_project_path(_encoded: &str) -> String {
    todo!("decode encoded-cwd")
}
