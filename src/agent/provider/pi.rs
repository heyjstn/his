use crate::agent::provider::{AgentMessage, FromProviderMessage};
use serde::{Deserialize, Serialize};
use serde_json::Value;

const COMMENTARY_PHASE: &str = "commentary";
const THINKING_CONTENT_TYPE: &str = "thinking";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum PiMessage {
    #[serde(rename = "session")]
    Session(SessionInfo),
    #[serde(rename = "model_change")]
    ModelChange(ModelChangeEvent),
    #[serde(rename = "thinking_level_change")]
    ThinkingLevelChange(ThinkingLevelChangeEvent),
    #[serde(rename = "message")]
    Message(MessageEvent),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionInfo {
    pub version: i64,
    pub id: String,
    pub timestamp: String,
    pub cwd: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelChangeEvent {
    pub id: String,
    pub parent_id: Option<String>,
    pub timestamp: String,
    pub provider: String,
    pub model_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThinkingLevelChangeEvent {
    pub id: String,
    pub parent_id: Option<String>,
    pub timestamp: String,
    pub thinking_level: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MessageEvent {
    pub id: String,
    pub parent_id: Option<String>,
    pub timestamp: String,
    pub message: Message,
}

/// A conversation message, discriminated by `role`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "role")]
pub enum Message {
    #[serde(rename = "user")]
    User(UserMessage),
    #[serde(rename = "assistant")]
    Assistant(AssistantMessage),
    #[serde(rename = "toolResult")]
    ToolResult(ToolResultMessage),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserMessage {
    pub content: Vec<ContentPart>,
    pub timestamp: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssistantMessage {
    pub content: Vec<ContentPart>,
    pub api: String,
    pub provider: String,
    pub model: String,
    pub usage: Usage,
    pub stop_reason: String,
    pub timestamp: i64,
    pub response_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolResultMessage {
    pub tool_call_id: String,
    pub tool_name: String,
    pub content: Vec<ContentPart>,
    pub is_error: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub details: Option<ToolResultDetails>,
    pub timestamp: i64,
}

/// A part of `message.content`, discriminated by `type`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ContentPart {
    #[serde(rename = "text")]
    Text(TextContent),
    #[serde(rename = "thinking")]
    Thinking(ThinkingContent),
    #[serde(rename = "toolCall")]
    ToolCall(ToolCallContent),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextContent {
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThinkingContent {
    pub thinking: String,
    pub thinking_signature: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallContent {
    pub id: String,
    pub name: String,
    /// Tool-specific args; shape depends on `name`.
    /// Examples:
    /// - bash: `{"command": "..."}`
    /// - read: `{"path": "..."}`
    /// - edit: `{"path": "...", "edits": [...]}`
    pub arguments: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Usage {
    pub input: i64,
    pub output: i64,
    pub cache_read: i64,
    pub cache_write: i64,
    pub total_tokens: i64,
    pub cost: Cost,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Cost {
    pub input: f64,
    pub output: f64,
    pub cache_read: f64,
    pub cache_write: f64,
    pub total: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolResultDetails {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub diff: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub patch: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub first_changed_line: Option<i64>,
}

impl FromProviderMessage for PiMessage {
    fn into_agent_messages(self) -> Vec<AgentMessage> {
        let PiMessage::Message(event) = &self else {
            return vec![self.into()];
        };
        let Message::Assistant(assistant) = &event.message else {
            return vec![self.into()];
        };
        let Some(thinking) = content_thinking(&assistant.content) else {
            return vec![self.into()];
        };

        let message = AgentMessage::from(self);
        let commentary = AgentMessage {
            id: format!("{}:{THINKING_CONTENT_TYPE}", message.id),
            text: Some(thinking),
            phase: Some(COMMENTARY_PHASE.to_string()),
            ..message.clone()
        };
        if message.text.is_none() {
            return vec![commentary];
        }

        vec![commentary, message]
    }
}

impl From<PiMessage> for AgentMessage {
    fn from(value: PiMessage) -> Self {
        match value {
            PiMessage::Session(event) => AgentMessage {
                typ: "session".to_string(),
                id: event.id,
                parent_id: None,
                timestamp: event.timestamp,
                cwd: Some(event.cwd),
                role: None,
                text: None,
                phase: None,
                provider: None,
                model: None,
                tool_call_id: None,
                tool_name: None,
                is_error: None,
            },
            PiMessage::ModelChange(event) => AgentMessage {
                typ: "model_change".to_string(),
                id: event.id,
                parent_id: event.parent_id,
                timestamp: event.timestamp,
                cwd: None,
                role: None,
                text: None,
                phase: None,
                provider: Some(event.provider),
                model: Some(event.model_id),
                tool_call_id: None,
                tool_name: None,
                is_error: None,
            },
            PiMessage::ThinkingLevelChange(event) => AgentMessage {
                typ: "thinking_level_change".to_string(),
                id: event.id,
                parent_id: event.parent_id,
                timestamp: event.timestamp,
                cwd: None,
                role: None,
                text: Some(event.thinking_level),
                phase: None,
                provider: None,
                model: None,
                tool_call_id: None,
                tool_name: None,
                is_error: None,
            },
            PiMessage::Message(event) => {
                let (role, text, provider, model, tool_call_id, tool_name, is_error) =
                    match event.message {
                        Message::User(message) => (
                            Some("user".to_string()),
                            content_text(&message.content),
                            None,
                            None,
                            None,
                            None,
                            None,
                        ),
                        Message::Assistant(message) => {
                            let tool_call = first_tool_call(&message.content);
                            (
                                Some("assistant".to_string()),
                                content_text(&message.content),
                                Some(message.provider),
                                Some(message.model),
                                tool_call.map(|tool_call| tool_call.id.clone()),
                                tool_call.map(|tool_call| tool_call.name.clone()),
                                None,
                            )
                        }
                        Message::ToolResult(message) => (
                            Some("tool_result".to_string()),
                            content_text(&message.content),
                            None,
                            None,
                            Some(message.tool_call_id),
                            Some(message.tool_name),
                            Some(message.is_error),
                        ),
                    };

                AgentMessage {
                    typ: "message".to_string(),
                    id: event.id,
                    parent_id: event.parent_id,
                    timestamp: event.timestamp,
                    cwd: None,
                    role,
                    text,
                    phase: None,
                    provider,
                    model,
                    tool_call_id,
                    tool_name,
                    is_error,
                }
            }
        }
    }
}

fn content_text(content: &[ContentPart]) -> Option<String> {
    let text = content
        .iter()
        .filter_map(|part| match part {
            ContentPart::Text(content) => Some(content.text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n");

    if text.is_empty() { None } else { Some(text) }
}

fn content_thinking(content: &[ContentPart]) -> Option<String> {
    let thinking = content
        .iter()
        .filter_map(|part| match part {
            ContentPart::Thinking(content) => Some(content.thinking.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n");

    if thinking.is_empty() {
        None
    } else {
        Some(thinking)
    }
}

fn first_tool_call(content: &[ContentPart]) -> Option<&ToolCallContent> {
    content.iter().find_map(|part| match part {
        ContentPart::ToolCall(content) => Some(content),
        _ => None,
    })
}

#[cfg(test)]
mod tests {
    use super::{COMMENTARY_PHASE, PiMessage};
    use crate::agent::provider::FromProviderMessage;

    #[test]
    fn converts_thinking_only_assistant_content_to_commentary() {
        let message = serde_json::from_str::<PiMessage>(
            r#"{"type":"message","id":"assistant-1","parentId":"user-1","timestamp":"2026-07-12T01:02:00Z","message":{"role":"assistant","content":[{"type":"thinking","thinking":"Inspecting the repository","thinkingSignature":"signature"},{"type":"toolCall","id":"call-1","name":"read","arguments":{}}],"api":"responses","provider":"test","model":"test-model","usage":{"input":1,"output":1,"cacheRead":0,"cacheWrite":0,"totalTokens":2,"cost":{"input":0.0,"output":0.0,"cacheRead":0.0,"cacheWrite":0.0,"total":0.0}},"stopReason":"toolUse","timestamp":2,"responseId":"response-1"}}"#,
        )
        .unwrap();

        let converted = message.into_agent_messages();

        assert_eq!(converted.len(), 1);
        assert_eq!(
            converted[0].text.as_deref(),
            Some("Inspecting the repository")
        );
        assert_eq!(converted[0].phase.as_deref(), Some(COMMENTARY_PHASE));
        assert_eq!(converted[0].tool_name.as_deref(), Some("read"));
    }
}
