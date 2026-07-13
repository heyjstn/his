use super::{AgentRecord, ParsedRecord};
use crate::session::{MessagePhase, MessageRole, SessionMessage, SessionTimestamp};
use serde::Deserialize;
use serde_json::Value;
use std::path::PathBuf;

const EDIT_TOOL: &str = "edit";

#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
pub(super) enum PiRecord {
    #[serde(rename = "session")]
    Session(SessionInfo),
    #[serde(rename = "message")]
    Message(MessageEvent),
    #[serde(other)]
    Ignored,
}

#[derive(Debug, Deserialize)]
pub(super) struct SessionInfo {
    id: String,
    timestamp: String,
    cwd: PathBuf,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct MessageEvent {
    timestamp: String,
    message: ConversationMessage,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "role")]
enum ConversationMessage {
    #[serde(rename = "user")]
    User(UserMessage),
    #[serde(rename = "assistant")]
    Assistant(AssistantMessage),
    #[serde(other)]
    Ignored,
}

#[derive(Debug, Deserialize)]
struct UserMessage {
    #[serde(default)]
    content: Vec<ContentPart>,
}

#[derive(Debug, Deserialize)]
struct AssistantMessage {
    #[serde(default)]
    content: Vec<ContentPart>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type")]
enum ContentPart {
    #[serde(rename = "text")]
    Text(TextContent),
    #[serde(rename = "thinking")]
    Thinking(ThinkingContent),
    #[serde(rename = "toolCall")]
    ToolCall(ToolCallContent),
    #[serde(other)]
    Ignored,
}

#[derive(Debug, Clone, Deserialize)]
struct TextContent {
    text: String,
}

#[derive(Debug, Clone, Deserialize)]
struct ThinkingContent {
    thinking: String,
}

#[derive(Debug, Clone, Deserialize)]
struct ToolCallContent {
    name: String,
    #[serde(default)]
    arguments: Value,
}

impl AgentRecord for PiRecord {
    fn into_records(self) -> Vec<ParsedRecord> {
        match self {
            Self::Session(session) => vec![ParsedRecord::Session {
                id: session.id,
                timestamp: SessionTimestamp::new(session.timestamp),
                cwd: Some(session.cwd),
            }],
            Self::Message(event) => event.into_records(),
            Self::Ignored => vec![ParsedRecord::Ignored],
        }
    }
}

impl From<PiRecord> for ParsedRecord {
    fn from(record: PiRecord) -> Self {
        record
            .into_records()
            .into_iter()
            .next()
            .unwrap_or(Self::Ignored)
    }
}

impl MessageEvent {
    fn into_records(self) -> Vec<ParsedRecord> {
        match self.message {
            ConversationMessage::User(message) => content_text(&message.content)
                .map(|text| {
                    ParsedRecord::Message(SessionMessage {
                        timestamp: SessionTimestamp::new(self.timestamp),
                        role: MessageRole::User,
                        text,
                        phase: None,
                        tool_path: None,
                        tool_contents: Vec::new(),
                    })
                })
                .into_iter()
                .collect(),
            ConversationMessage::Assistant(message) => {
                Self::assistant_records(self.timestamp, message.content)
            }
            ConversationMessage::Ignored => vec![ParsedRecord::Ignored],
        }
    }

    fn assistant_records(raw_timestamp: String, content: Vec<ContentPart>) -> Vec<ParsedRecord> {
        let timestamp = SessionTimestamp::new(raw_timestamp);
        let mut records = Vec::new();

        if let Some(thinking) = content_thinking(&content) {
            records.push(ParsedRecord::Message(SessionMessage {
                timestamp: timestamp.clone(),
                role: MessageRole::Assistant,
                text: thinking,
                phase: Some(MessagePhase::Commentary),
                tool_path: None,
                tool_contents: Vec::new(),
            }));
        }

        records.extend(
            content_tool_calls(&content)
                .filter(|tool_call| tool_call.name == EDIT_TOOL)
                .map(|tool_call| {
                    ParsedRecord::Message(SessionMessage {
                        timestamp: timestamp.clone(),
                        role: MessageRole::Assistant,
                        text: tool_call.name.clone(),
                        phase: Some(MessagePhase::ToolCall),
                        tool_path: edit_path(tool_call),
                        tool_contents: edit_contents(tool_call),
                    })
                }),
        );

        if let Some(text) = content_text(&content) {
            records.push(ParsedRecord::Message(SessionMessage {
                timestamp,
                role: MessageRole::Assistant,
                text,
                phase: None,
                tool_path: None,
                tool_contents: Vec::new(),
            }));
        }

        if records.is_empty() {
            records.push(ParsedRecord::Ignored);
        }
        records
    }
}

fn content_text(content: &[ContentPart]) -> Option<String> {
    joined_content(content, |part| match part {
        ContentPart::Text(content) => Some(content.text.as_str()),
        ContentPart::Thinking(_) | ContentPart::ToolCall(_) | ContentPart::Ignored => None,
    })
}

fn content_thinking(content: &[ContentPart]) -> Option<String> {
    joined_content(content, |part| match part {
        ContentPart::Thinking(content) => Some(content.thinking.as_str()),
        ContentPart::Text(_) | ContentPart::ToolCall(_) | ContentPart::Ignored => None,
    })
}

fn joined_content<'a>(
    content: &'a [ContentPart],
    value: impl Fn(&'a ContentPart) -> Option<&'a str>,
) -> Option<String> {
    let joined = content
        .iter()
        .filter_map(value)
        .collect::<Vec<_>>()
        .join("\n");
    (!joined.is_empty()).then_some(joined)
}

fn content_tool_calls(content: &[ContentPart]) -> impl Iterator<Item = &ToolCallContent> {
    content.iter().filter_map(|part| match part {
        ContentPart::ToolCall(content) => Some(content),
        ContentPart::Text(_) | ContentPart::Thinking(_) | ContentPart::Ignored => None,
    })
}

fn edit_path(tool_call: &ToolCallContent) -> Option<PathBuf> {
    tool_call
        .arguments
        .get("path")
        .and_then(Value::as_str)
        .map(PathBuf::from)
}

fn edit_contents(tool_call: &ToolCallContent) -> Vec<String> {
    tool_call
        .arguments
        .get("edits")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|edit| edit.get("newText").and_then(Value::as_str))
        .map(str::to_string)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::PiRecord;
    use crate::agent::{AgentRecord, ParsedRecord};
    use crate::session::MessagePhase;

    #[test]
    fn converts_thinking_and_edit_calls_to_separate_messages() {
        let record = serde_json::from_str::<PiRecord>(
            r#"{"type":"message","id":"assistant-1","parentId":"user-1","timestamp":"2026-07-12T01:02:00Z","message":{"role":"assistant","content":[{"type":"thinking","thinking":"Inspecting","thinkingSignature":"signature"},{"type":"toolCall","id":"call-1","name":"read","arguments":{}},{"type":"toolCall","id":"call-2","name":"edit","arguments":{"path":"/tmp/file.rs","edits":[{"oldText":"before","newText":"after"}]}},{"type":"text","text":"Done"}],"provider":"test","model":"test-model"}}"#,
        )
        .unwrap();

        let converted = record.into_records();

        assert_eq!(converted.len(), 3);
        let ParsedRecord::Message(thinking) = &converted[0] else {
            panic!("expected thinking message");
        };
        assert_eq!(thinking.phase, Some(MessagePhase::Commentary));
        let ParsedRecord::Message(edit) = &converted[1] else {
            panic!("expected edit message");
        };
        assert_eq!(edit.phase, Some(MessagePhase::ToolCall));
        assert_eq!(
            edit.tool_path.as_deref(),
            Some(std::path::Path::new("/tmp/file.rs"))
        );
        assert_eq!(edit.tool_contents, ["after"]);
        let ParsedRecord::Message(answer) = &converted[2] else {
            panic!("expected answer message");
        };
        assert_eq!(answer.text, "Done");
    }

    #[test]
    fn tolerates_unknown_record_and_content_types() {
        let ignored = serde_json::from_str::<PiRecord>(
            r#"{"type":"future_record","id":"future","payload":{}}"#,
        )
        .unwrap();
        assert!(matches!(ignored, PiRecord::Ignored));

        let record = serde_json::from_str::<PiRecord>(
            r#"{"type":"message","id":"user-1","timestamp":"2026-07-12T01:02:00Z","message":{"role":"user","content":[{"type":"future_content","value":"ignored"},{"type":"text","text":"Visible"}]}}"#,
        )
        .unwrap();
        let converted = record.into_records();
        let ParsedRecord::Message(message) = &converted[0] else {
            panic!("expected visible user message");
        };
        assert_eq!(message.text, "Visible");
    }
}
