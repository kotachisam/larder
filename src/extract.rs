use std::collections::HashMap;

use anyhow::Result;

use crate::store::Entry;
use crate::transcript::Event;

#[allow(dead_code)]
pub struct Extractor {
    pending_question: Option<PendingQuestion>,
    open_tool_uses: HashMap<String, PendingBash>,
    current_assistant_text: String,
}

#[allow(dead_code)]
struct PendingQuestion {
    text: String,
    source_line: i64,
    ts: i64,
    parent_uuid: Option<String>,
}

#[allow(dead_code)]
struct PendingBash {
    question: Option<String>,
    answer_summary: String,
    command: String,
    source_line: i64,
    ts: i64,
    parent_uuid: Option<String>,
}

impl Extractor {
    pub fn new() -> Self {
        Self {
            pending_question: None,
            open_tool_uses: HashMap::new(),
            current_assistant_text: String::new(),
        }
    }

    pub fn step(&mut self, _event: Event, _source_line: i64) -> Result<Vec<Entry>> {
        todo!("event-by-event extraction")
    }

    pub fn flush(&mut self) -> Vec<Entry> {
        todo!("flush remaining open_tool_uses as interrupted")
    }
}

impl Default for Extractor {
    fn default() -> Self {
        Self::new()
    }
}
