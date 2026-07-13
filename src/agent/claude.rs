use super::{AgentRecord, ParsedRecord};
use crate::session::{MessagePhase, MessageRole, SessionMessage, SessionTimestamp};
use serde::Deserialize;
use serde_json::Value;
use std::path::PathBuf;

const EDIT_TOOL: &str = "Edit";
const WRITE_TOOL: &str = "Write";

#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
pub(super) enum ClaudeRecord {
    #[serde(rename = "user")]
    User(ConversationEvent),
    #[serde(rename = "assistant")]
    Assistant(ConversationEvent),
    #[serde(other)]
    Ignored,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ConversationEvent {
    session_id: String,
    timestamp: String,
    cwd: PathBuf,
    #[serde(default)]
    is_sidechain: bool,
    #[serde(default)]
    is_meta: bool,
    #[serde(default)]
    is_compact_summary: bool,
    message: ConversationMessage,
}

#[derive(Debug, Deserialize)]
struct ConversationMessage {
    content: MessageContent,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum MessageContent {
    Text(String),
    Parts(Vec<ContentPart>),
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
enum ContentPart {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "thinking")]
    Thinking { thinking: String },
    #[serde(rename = "tool_use")]
    ToolUse {
        name: String,
        #[serde(default)]
        input: Value,
    },
    #[serde(other)]
    Ignored,
}

impl AgentRecord for ClaudeRecord {
    fn into_records(self) -> Vec<ParsedRecord> {
        match self {
            Self::User(event) => event.into_records(MessageRole::User),
            Self::Assistant(event) => event.into_records(MessageRole::Assistant),
            Self::Ignored => vec![ParsedRecord::Ignored],
        }
    }
}

impl From<ClaudeRecord> for ParsedRecord {
    fn from(record: ClaudeRecord) -> Self {
        record
            .into_records()
            .into_iter()
            .next()
            .unwrap_or(Self::Ignored)
    }
}

impl ConversationEvent {
    fn into_records(self, role: MessageRole) -> Vec<ParsedRecord> {
        if self.is_sidechain {
            return vec![ParsedRecord::Ignored];
        }

        let timestamp = SessionTimestamp::new(self.timestamp);
        let mut records = vec![ParsedRecord::Session {
            id: self.session_id,
            timestamp: timestamp.clone(),
            cwd: Some(self.cwd),
        }];
        if self.is_meta || self.is_compact_summary {
            return records;
        }

        match role {
            MessageRole::User => {
                if let Some(text) = self.message.content.text() {
                    records.push(ParsedRecord::Message(message(
                        timestamp,
                        MessageRole::User,
                        text,
                        None,
                    )));
                }
            }
            MessageRole::Assistant => {
                records.extend(self.message.content.assistant_records(timestamp));
            }
        }
        records
    }
}

impl MessageContent {
    fn text(&self) -> Option<String> {
        match self {
            Self::Text(text) => non_empty(text.clone()),
            Self::Parts(parts) => joined_parts(parts, |part| match part {
                ContentPart::Text { text } => Some(text),
                ContentPart::Thinking { .. }
                | ContentPart::ToolUse { .. }
                | ContentPart::Ignored => None,
            }),
        }
    }

    fn assistant_records(&self, timestamp: SessionTimestamp) -> Vec<ParsedRecord> {
        let Self::Parts(parts) = self else {
            return self
                .text()
                .map(|text| {
                    ParsedRecord::Message(message(timestamp, MessageRole::Assistant, text, None))
                })
                .into_iter()
                .collect();
        };

        let mut records = Vec::new();
        if let Some(thinking) = joined_parts(parts, |part| match part {
            ContentPart::Thinking { thinking } => Some(thinking),
            ContentPart::Text { .. } | ContentPart::ToolUse { .. } | ContentPart::Ignored => None,
        }) {
            records.push(ParsedRecord::Message(message(
                timestamp.clone(),
                MessageRole::Assistant,
                thinking,
                Some(MessagePhase::Commentary),
            )));
        }

        records.extend(parts.iter().filter_map(|part| {
            let ContentPart::ToolUse { name, input } = part else {
                return None;
            };
            file_change(name, input).map(|(path, contents)| {
                let mut record = message(
                    timestamp.clone(),
                    MessageRole::Assistant,
                    name.clone(),
                    Some(MessagePhase::ToolCall),
                );
                record.tool_path = path;
                record.tool_contents = contents;
                ParsedRecord::Message(record)
            })
        }));

        if let Some(text) = self.text() {
            records.push(ParsedRecord::Message(message(
                timestamp,
                MessageRole::Assistant,
                text,
                None,
            )));
        }
        records
    }
}

fn joined_parts<'a>(
    parts: &'a [ContentPart],
    value: impl Fn(&'a ContentPart) -> Option<&'a String>,
) -> Option<String> {
    non_empty(
        parts
            .iter()
            .filter_map(value)
            .map(String::as_str)
            .collect::<Vec<_>>()
            .join("\n"),
    )
}

fn non_empty(value: String) -> Option<String> {
    (!value.is_empty()).then_some(value)
}

fn file_change(name: &str, input: &Value) -> Option<(Option<PathBuf>, Vec<String>)> {
    let content_field = match name {
        EDIT_TOOL => "new_string",
        WRITE_TOOL => "content",
        _ => return None,
    };
    let path = input
        .get("file_path")
        .and_then(Value::as_str)
        .map(PathBuf::from);
    let contents = input
        .get(content_field)
        .and_then(Value::as_str)
        .map(str::to_string)
        .into_iter()
        .collect();
    Some((path, contents))
}

fn message(
    timestamp: SessionTimestamp,
    role: MessageRole,
    text: String,
    phase: Option<MessagePhase>,
) -> SessionMessage {
    SessionMessage {
        timestamp,
        role,
        text,
        phase,
        tool_path: None,
        tool_contents: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::ClaudeRecord;
    use crate::agent::{AgentRecord, ParsedRecord};
    use crate::session::{MessagePhase, MessageRole};
    use std::path::Path;

    #[test]
    fn converts_user_text_and_assistant_content() {
        let user = parse_record(
            r#"{"type":"user","sessionId":"session-id","timestamp":"2026-07-13T01:00:00Z","cwd":"/work/project","message":{"role":"user","content":"Inspect the project"}}"#,
        );
        let assistant = parse_record(
            r#"{"type":"assistant","sessionId":"session-id","timestamp":"2026-07-13T01:01:00Z","cwd":"/work/project","message":{"role":"assistant","content":[{"type":"thinking","thinking":"Checking the implementation"},{"type":"tool_use","name":"Edit","input":{"file_path":"/work/project/src/main.rs","old_string":"before","new_string":"after"}},{"type":"tool_use","name":"Bash","input":{"command":"cargo test"}},{"type":"text","text":"Implemented"}]}}"#,
        );

        let user_records = user.into_records();
        let assistant_records = assistant.into_records();

        let ParsedRecord::Session { id, cwd, .. } = &user_records[0] else {
            panic!("expected session metadata");
        };
        assert_eq!(id, "session-id");
        assert_eq!(cwd.as_deref(), Some(Path::new("/work/project")));
        let ParsedRecord::Message(user_message) = &user_records[1] else {
            panic!("expected user message");
        };
        assert_eq!(user_message.role, MessageRole::User);
        assert_eq!(user_message.text, "Inspect the project");
        assert_eq!(assistant_records.len(), 4);
        let ParsedRecord::Message(thinking) = &assistant_records[1] else {
            panic!("expected thinking message");
        };
        assert_eq!(thinking.phase, Some(MessagePhase::Commentary));
        let ParsedRecord::Message(edit) = &assistant_records[2] else {
            panic!("expected edit message");
        };
        assert_eq!(edit.phase, Some(MessagePhase::ToolCall));
        assert_eq!(
            edit.tool_path.as_deref(),
            Some(Path::new("/work/project/src/main.rs"))
        );
        assert_eq!(edit.tool_contents, ["after"]);
        let ParsedRecord::Message(answer) = &assistant_records[3] else {
            panic!("expected assistant answer");
        };
        assert_eq!(answer.text, "Implemented");
    }

    #[test]
    fn ignores_synthetic_messages_tool_results_sidechains_and_unknown_records() {
        let metadata = parse_record(
            r#"{"type":"user","sessionId":"session-id","timestamp":"2026-07-13T01:01:00Z","cwd":"/work/project","isMeta":true,"message":{"role":"user","content":"Internal command context"}}"#,
        );
        let compact_summary = parse_record(
            r#"{"type":"user","sessionId":"session-id","timestamp":"2026-07-13T01:01:00Z","cwd":"/work/project","isCompactSummary":true,"message":{"role":"user","content":"Internal compact summary"}}"#,
        );
        let tool_result = parse_record(
            r#"{"type":"user","sessionId":"session-id","timestamp":"2026-07-13T01:02:00Z","cwd":"/work/project","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"call-id","content":"private output"}]}}"#,
        );
        let sidechain = parse_record(
            r#"{"type":"assistant","sessionId":"session-id","timestamp":"2026-07-13T01:03:00Z","cwd":"/work/project","isSidechain":true,"message":{"role":"assistant","content":[{"type":"text","text":"Subagent output"}]}}"#,
        );
        let unknown = parse_record(r#"{"type":"future-record","payload":{}}"#);

        assert!(matches!(
            metadata.into_records().as_slice(),
            [ParsedRecord::Session { .. }]
        ));
        assert!(matches!(
            compact_summary.into_records().as_slice(),
            [ParsedRecord::Session { .. }]
        ));
        assert!(matches!(
            tool_result.into_records().as_slice(),
            [ParsedRecord::Session { .. }]
        ));
        assert!(matches!(
            sidechain.into_records().as_slice(),
            [ParsedRecord::Ignored]
        ));
        assert!(matches!(unknown, ClaudeRecord::Ignored));
    }

    #[test]
    fn converts_write_tool_content() {
        let record = parse_record(
            r#"{"type":"assistant","sessionId":"session-id","timestamp":"2026-07-13T01:01:00Z","cwd":"/work/project","message":{"role":"assistant","content":[{"type":"tool_use","name":"Write","input":{"file_path":"/work/project/new.txt","content":"new file"}}]}}"#,
        );

        let records = record.into_records();
        let ParsedRecord::Message(write) = &records[1] else {
            panic!("expected write message");
        };

        assert_eq!(
            write.tool_path.as_deref(),
            Some(Path::new("/work/project/new.txt"))
        );
        assert_eq!(write.tool_contents, ["new file"]);
    }

    fn parse_record(json: &str) -> ClaudeRecord {
        serde_json::from_str(json).unwrap()
    }
}
