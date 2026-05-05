pub mod queries;
pub mod schema;

use std::path::Path;
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};
use rusqlite::{Connection, OptionalExtension};

use crate::store::schema::{MIGRATIONS, SCHEMA_V1_SQL};

pub struct Store {
    pub conn: Arc<Mutex<Connection>>,
}

impl Store {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("creating data dir {}", parent.display()))?;
        }
        let conn = Connection::open(path)
            .with_context(|| format!("opening sqlite at {}", path.display()))?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        conn.pragma_update(None, "temp_store", "MEMORY")?;
        conn.execute_batch(SCHEMA_V1_SQL)
            .context("applying schema v1")?;
        let current: i32 = conn
            .query_row("SELECT MAX(version) FROM schema_version", [], |r| r.get(0))
            .optional()?
            .unwrap_or(1);
        for (target, sql) in MIGRATIONS {
            if *target > current {
                conn.execute_batch(sql)
                    .with_context(|| format!("applying migration to v{}", target))?;
            }
        }
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
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

#[derive(Debug, Clone)]
pub struct SessionMeta {
    pub session_id: String,
    pub provider: String,
    pub project_path: String,
    pub source_path: String,
    pub source_mtime: i64,
    pub source_size: i64,
    pub started_at: Option<i64>,
    pub ended_at: Option<i64>,
    pub message_count: i64,
    pub parent_session_id: Option<String>,
    pub is_subagent: bool,
}
