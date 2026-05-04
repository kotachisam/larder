pub mod queries;
pub mod schema;

use std::path::Path;
use std::sync::{Arc, Mutex};

use anyhow::Result;
use rusqlite::Connection;

pub struct Store {
    pub conn: Arc<Mutex<Connection>>,
}

impl Store {
    pub fn open(_path: &Path) -> Result<Self> {
        todo!("store open with WAL + schema migration")
    }
}

#[derive(Debug, Clone)]
pub struct Entry {
    pub session_id: String,
    pub ts: i64,
    pub kind: EntryKind,
    pub question: Option<String>,
    pub answer_summary: Option<String>,
    pub command: Option<String>,
    pub command_stdout: Option<String>,
    pub command_stderr: Option<String>,
    pub interrupted: bool,
    pub truncated: bool,
    pub tool_use_id: Option<String>,
    pub parent_uuid: Option<String>,
    pub source_line: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryKind {
    Bash,
    Qa,
}

impl EntryKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Bash => "bash",
            Self::Qa => "qa",
        }
    }
}
