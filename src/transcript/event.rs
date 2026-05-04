use serde::Deserialize;
use serde_json::Value;

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum Event {
    User(UserEvent),
    Assistant(AssistantEvent),
    QueueOperation(QueueOperationEvent),
    #[serde(other)]
    Other,
}

#[derive(Debug, Deserialize)]
pub struct UserEvent {
    pub uuid: Option<String>,
    #[serde(rename = "parentUuid")]
    pub parent_uuid: Option<String>,
    pub timestamp: Option<String>,
    #[serde(rename = "sessionId")]
    pub session_id: Option<String>,
    pub message: Option<MessageEnvelope>,
    #[serde(rename = "toolUseResult")]
    pub tool_use_result: Option<ToolUseResult>,
}

#[derive(Debug, Deserialize)]
pub struct AssistantEvent {
    pub uuid: Option<String>,
    #[serde(rename = "parentUuid")]
    pub parent_uuid: Option<String>,
    pub timestamp: Option<String>,
    #[serde(rename = "sessionId")]
    pub session_id: Option<String>,
    pub message: Option<MessageEnvelope>,
}

#[derive(Debug, Deserialize)]
pub struct QueueOperationEvent {
    pub timestamp: Option<String>,
    pub content: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct MessageEnvelope {
    pub role: Option<String>,
    pub model: Option<String>,
    pub content: Option<Vec<ContentBlock>>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentBlock {
    Text {
        text: String,
    },
    Thinking {
        thinking: Option<String>,
    },
    ToolUse {
        id: String,
        name: String,
        input: Value,
    },
    ToolResult {
        tool_use_id: String,
        content: Option<Value>,
        is_error: Option<bool>,
    },
    #[serde(other)]
    Other,
}

#[derive(Debug, Deserialize)]
pub struct ToolUseResult {
    pub stdout: Option<String>,
    pub stderr: Option<String>,
    pub interrupted: Option<bool>,
    #[serde(rename = "isImage")]
    pub is_image: Option<bool>,
}
