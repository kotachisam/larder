use std::path::Path;

use anyhow::Result;
use serde::Serialize;

use crate::cli::IngestArgs;
use crate::store::Store;

#[derive(Debug, Clone, Default, Serialize)]
pub struct IngestStats {
    pub sessions_seen: usize,
    pub sessions_new: usize,
    pub sessions_updated: usize,
    pub entries_inserted: usize,
    pub entries_skipped: usize,
}

pub fn ingest_path(_store: &Store, _path: &Path) -> Result<IngestStats> {
    todo!("session-level ingest with skip-if-unchanged + line-based incremental")
}

pub fn run(_args: IngestArgs) -> Result<()> {
    todo!("ingest command: walk transcripts dir, ingest each")
}
