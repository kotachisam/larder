use std::collections::HashMap;

use anyhow::Result;
use chrono::DateTime;
use serde_json::Value;

use crate::store::{Entry, EntryKind};
use crate::transcript::{ContentBlock, Event, MessageContent};

const ANSWER_SUMMARY_MAX: usize = 1000;
const COMMAND_OUTPUT_MAX: usize = 4000;

pub struct Extractor {
    session_id: String,
    pending_question: Option<PendingQuestion>,
    open_tool_uses: HashMap<String, PendingBash>,
    current_assistant_text: String,
    current_assistant_parent: Option<String>,
    current_assistant_ts: i64,
}

#[derive(Clone)]
struct PendingQuestion {
    text: String,
    ts: i64,
    source_line: i64,
    parent_uuid: Option<String>,
    answered: bool,
}

struct PendingBash {
    question: Option<String>,
    answer_summary: String,
    command: String,
    source_line: i64,
    ts: i64,
    parent_uuid: Option<String>,
}

impl Extractor {
    pub fn new(session_id: impl Into<String>) -> Self {
        Self {
            session_id: session_id.into(),
            pending_question: None,
            open_tool_uses: HashMap::new(),
            current_assistant_text: String::new(),
            current_assistant_parent: None,
            current_assistant_ts: 0,
        }
    }

    pub fn step(&mut self, event: Event, source_line: i64) -> Result<Vec<Entry>> {
        let mut out = Vec::new();
        match event {
            Event::User(u) => {
                let ts = parse_ts(u.timestamp.as_deref());
                if let Some(MessageContent::Text(text)) = u.message.as_ref().map(|m| &m.content) {
                    let trimmed = text.trim();
                    if !trimmed.is_empty() && !is_system_reminder(trimmed) {
                        if let Some(q) = self.flush_qa() {
                            out.push(q);
                        }
                        self.pending_question = Some(PendingQuestion {
                            text: trimmed.to_string(),
                            ts,
                            source_line,
                            parent_uuid: u.parent_uuid.clone(),
                            answered: false,
                        });
                        self.current_assistant_text.clear();
                        self.current_assistant_parent = None;
                    }
                }
                if let Some(MessageContent::Blocks(blocks)) = u.message.as_ref().map(|m| &m.content)
                {
                    for b in blocks {
                        if let ContentBlock::ToolResult {
                            tool_use_id,
                            content,
                            is_error,
                        } = b
                            && let Some(pending) = self.open_tool_uses.remove(tool_use_id)
                        {
                            let (stdout, stderr, interrupted) =
                                extract_tool_output(content, &u.tool_use_result);
                            out.push(Entry {
                                session_id: self.session_id.clone(),
                                ts: pending.ts,
                                kind: EntryKind::Bash,
                                question: pending.question,
                                answer_summary: nonempty(pending.answer_summary),
                                command: Some(pending.command),
                                command_stdout: nonempty(stdout),
                                command_stderr: nonempty(stderr),
                                interrupted: interrupted || is_error.unwrap_or(false),
                                truncated: false,
                                tool_use_id: Some(tool_use_id.clone()),
                                parent_uuid: pending.parent_uuid,
                                source_line: pending.source_line,
                            });
                        }
                    }
                }
            }
            Event::Assistant(a) => {
                let ts = parse_ts(a.timestamp.as_deref());
                self.current_assistant_ts = ts;
                self.current_assistant_parent = a.parent_uuid.clone();
                if let Some(MessageContent::Blocks(blocks)) = a.message.as_ref().map(|m| &m.content)
                {
                    for b in blocks {
                        match b {
                            ContentBlock::Text { text } => {
                                if !self.current_assistant_text.is_empty() {
                                    self.current_assistant_text.push('\n');
                                }
                                self.current_assistant_text.push_str(text);
                                if let Some(q) = self.pending_question.as_mut() {
                                    q.answered = true;
                                }
                            }
                            ContentBlock::ToolUse { id, name, input } if name == "Bash" => {
                                let command = input
                                    .get("command")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("")
                                    .to_string();
                                if command.is_empty() {
                                    continue;
                                }
                                let answer_snip = truncate(
                                    self.current_assistant_text.trim(),
                                    ANSWER_SUMMARY_MAX,
                                );
                                self.open_tool_uses.insert(
                                    id.clone(),
                                    PendingBash {
                                        question: self
                                            .pending_question
                                            .as_ref()
                                            .map(|p| p.text.clone()),
                                        answer_summary: answer_snip,
                                        command,
                                        source_line,
                                        ts,
                                        parent_uuid: a.parent_uuid.clone(),
                                    },
                                );
                                if let Some(q) = self.pending_question.as_mut() {
                                    q.answered = true;
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
            Event::QueueOperation(_) | Event::Other => {}
        }
        Ok(out)
    }

    pub fn flush(&mut self) -> Vec<Entry> {
        let mut out = Vec::new();
        if let Some(q) = self.flush_qa() {
            out.push(q);
        }
        let drained: Vec<_> = self.open_tool_uses.drain().collect();
        for (id, pending) in drained {
            out.push(Entry {
                session_id: self.session_id.clone(),
                ts: pending.ts,
                kind: EntryKind::Bash,
                question: pending.question,
                answer_summary: nonempty(pending.answer_summary),
                command: Some(pending.command),
                command_stdout: None,
                command_stderr: None,
                interrupted: true,
                truncated: false,
                tool_use_id: Some(id),
                parent_uuid: pending.parent_uuid,
                source_line: pending.source_line,
            });
        }
        out
    }

    fn flush_qa(&mut self) -> Option<Entry> {
        let q = self.pending_question.take()?;
        let answer = self.current_assistant_text.trim().to_string();
        self.current_assistant_text.clear();
        if !q.answered && answer.is_empty() {
            return None;
        }
        let truncated = answer.len() > ANSWER_SUMMARY_MAX;
        let summary = truncate(&answer, ANSWER_SUMMARY_MAX);
        Some(Entry {
            session_id: self.session_id.clone(),
            ts: q.ts,
            kind: EntryKind::Qa,
            question: Some(q.text),
            answer_summary: nonempty(summary),
            command: None,
            command_stdout: None,
            command_stderr: None,
            interrupted: false,
            truncated,
            tool_use_id: None,
            parent_uuid: q.parent_uuid,
            source_line: q.source_line,
        })
    }
}

fn parse_ts(ts: Option<&str>) -> i64 {
    ts.and_then(|s| DateTime::parse_from_rfc3339(s).ok())
        .map(|d| d.timestamp())
        .unwrap_or(0)
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        return s.to_string();
    }
    let mut end = max;
    while !s.is_char_boundary(end) && end > 0 {
        end -= 1;
    }
    s[..end].to_string()
}

fn nonempty(s: String) -> Option<String> {
    if s.is_empty() { None } else { Some(s) }
}

fn extract_tool_output(
    content: &Option<Value>,
    tool_use_result: &Option<crate::transcript::ToolUseResult>,
) -> (String, String, bool) {
    if let Some(tur) = tool_use_result {
        let stdout = truncate(tur.stdout.as_deref().unwrap_or(""), COMMAND_OUTPUT_MAX);
        let stderr = truncate(tur.stderr.as_deref().unwrap_or(""), COMMAND_OUTPUT_MAX);
        let interrupted = tur.interrupted.unwrap_or(false);
        return (stdout, stderr, interrupted);
    }
    let text = match content {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Array(arr)) => arr
            .iter()
            .filter_map(|v| v.get("text").and_then(|t| t.as_str()))
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    };
    (truncate(&text, COMMAND_OUTPUT_MAX), String::new(), false)
}

fn is_system_reminder(s: &str) -> bool {
    s.starts_with("<system-reminder>") || s.starts_with("<command-")
}
