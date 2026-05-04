use anyhow::Result;

use crate::store::{Entry, Store};

impl Store {
    pub fn insert_entries(&self, _session_id: &str, _entries: &[Entry]) -> Result<usize> {
        todo!("insert_entries with INSERT OR IGNORE")
    }

    pub fn last_source_line(&self, _session_id: &str) -> Result<Option<i64>> {
        todo!("last_source_line")
    }

    pub fn session_count(&self) -> Result<i64> {
        todo!("session_count")
    }

    pub fn entry_count(&self) -> Result<i64> {
        todo!("entry_count")
    }
}
