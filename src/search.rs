use anyhow::Result;
use serde::Serialize;

use crate::cli::AskArgs;
use crate::store::Store;

#[derive(Debug, Clone, Serialize)]
pub struct Hit {
    pub id: i64,
    pub ts: i64,
    pub project_path: String,
    pub question: Option<String>,
    pub command: Option<String>,
    pub stdout: Option<String>,
    pub stderr: Option<String>,
    pub summary: Option<String>,
    pub score: f64,
}

pub fn search(_store: &Store, _query: &str, _limit: usize) -> Result<Vec<Hit>> {
    todo!("FTS5 search with BM25 ranking")
}

pub fn run(_args: AskArgs) -> Result<()> {
    todo!("ask command: open store, build query, render output")
}
