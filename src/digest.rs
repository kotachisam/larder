use anyhow::Result;
use serde::Serialize;

use crate::cli::DigestArgs;
use crate::store::Store;

#[derive(Debug, Clone, Serialize)]
pub struct DigestEntry {
    pub question: String,
    pub count: i64,
    pub last_seen: i64,
    pub example_command: Option<String>,
}

pub fn digest(_store: &Store, _since_seconds: i64, _top: usize) -> Result<Vec<DigestEntry>> {
    todo!("frequency aggregation over entries.question")
}

pub fn run(_args: DigestArgs) -> Result<()> {
    todo!("digest command: open store, aggregate, render")
}
