use super::{AgentRecord, ParsedRecord};
use crate::session::{MessagePhase, MessageRole, SessionMessage, SessionTimestamp};
use anyhow::{Context, Result};
use serde::Deserialize;
use serde_json::Value;
use std::path::{Path, PathBuf};

#[derive(Deserialize, Debug)]
pub(super) struct CodexRecord {
    #[serde(default)]
    timestamp: String,
    #[serde(rename = "type")]
    typ: String,
    #[serde(default)]
    payload: Value,
}

impl AgentRecord for CodexRecord {
    fn parse_records(path: &Path) -> Result<Vec<ParsedRecord>> {
        let reader = Self::reader(path)?;
        let records = serde_json::Deserializer::from_reader(reader)
            .into_iter::<Self>()
            .map(|record| {
                record.with_context(|| format!("failed to parse session file {}", path.display()))
            })
            .collect::<Result<Vec<_>>>()?;

        Ok(convert_records(records))
    }
}

impl From<CodexRecord> for ParsedRecord {
    fn from(value: CodexRecord) -> Self {
        let payload_type = string_field(&value.payload, "type");
        if value.typ == "session_meta" {
            let timestamp = string_field(&value.payload, "timestamp").unwrap_or(&value.timestamp);
            let id = string_field(&value.payload, "id")
                .or_else(|| string_field(&value.payload, "session_id"))
                .map(str::to_string)
                .unwrap_or_else(|| format!("{timestamp}:session_meta"));
            return Self::Session {
                id,
                timestamp: SessionTimestamp::new(timestamp),
                cwd: string_field(&value.payload, "cwd").map(PathBuf::from),
            };
        }

        let (role, text) = match (value.typ.as_str(), payload_type) {
            ("event_msg", Some("user_message")) => (
                Some(MessageRole::User),
                string_field(&value.payload, "message").map(str::to_string),
            ),
            ("event_msg", Some("agent_message")) => (
                Some(MessageRole::Assistant),
                string_field(&value.payload, "message").map(str::to_string),
            ),
            ("response_item", Some("message"))
                if string_field(&value.payload, "role") == Some("assistant") =>
            {
                (Some(MessageRole::Assistant), output_text(&value.payload))
            }
            _ => (None, None),
        };
        let (Some(role), Some(text)) = (role, text) else {
            return Self::Ignored;
        };
        Self::Message(SessionMessage {
            timestamp: SessionTimestamp::new(value.timestamp),
            role,
            text,
            phase: string_field(&value.payload, "phase").map(MessagePhase::from_provider),
            tool_path: None,
            tool_contents: Vec::new(),
        })
    }
}

fn string_field<'a>(value: &'a Value, field: &str) -> Option<&'a str> {
    value.get(field).and_then(Value::as_str)
}

fn output_text(payload: &Value) -> Option<String> {
    let text = payload
        .get("content")?
        .as_array()?
        .iter()
        .filter(|content| string_field(content, "type") == Some("output_text"))
        .filter_map(|content| string_field(content, "text"))
        .collect::<Vec<_>>()
        .join("\n");

    (!text.is_empty()).then_some(text)
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum AssistantMessageSource {
    Event,
    Response,
}

fn convert_records(records: Vec<CodexRecord>) -> Vec<ParsedRecord> {
    let mut converted = Vec::with_capacity(records.len());
    let mut pending_assistant: Option<(usize, AssistantMessageSource)> = None;

    for record in records {
        let is_message_record = is_message_record(&record);
        let source = assistant_message_source(&record);
        let record = ParsedRecord::from(record);

        let ParsedRecord::Message(message) = &record else {
            if is_message_record {
                pending_assistant = None;
            }
            converted.push(record);
            continue;
        };

        let Some(source) = source else {
            pending_assistant = None;
            converted.push(record);
            continue;
        };

        if let Some((index, pending_source)) = pending_assistant {
            let ParsedRecord::Message(pending) = &mut converted[index] else {
                unreachable!("pending assistant index must reference a message");
            };
            if source != pending_source && is_same_utterance(pending, message) {
                if pending.phase.is_none() {
                    pending.phase = message.phase.clone();
                }
                pending_assistant = None;
                continue;
            }
        }

        let index = converted.len();
        converted.push(record);
        pending_assistant = Some((index, source));
    }

    converted
}

fn is_message_record(record: &CodexRecord) -> bool {
    matches!(
        (record.typ.as_str(), string_field(&record.payload, "type")),
        ("event_msg", Some("user_message" | "agent_message")) | ("response_item", Some("message"))
    )
}

fn assistant_message_source(record: &CodexRecord) -> Option<AssistantMessageSource> {
    match (
        record.typ.as_str(),
        string_field(&record.payload, "type"),
        string_field(&record.payload, "role"),
    ) {
        ("event_msg", Some("agent_message"), _) => Some(AssistantMessageSource::Event),
        ("response_item", Some("message"), Some("assistant")) => {
            Some(AssistantMessageSource::Response)
        }
        _ => None,
    }
}

fn is_same_utterance(left: &SessionMessage, right: &SessionMessage) -> bool {
    left.role == right.role
        && left.text == right.text
        && (left.phase == right.phase || left.phase.is_none() || right.phase.is_none())
}

#[cfg(test)]
mod tests {
    use super::{CodexRecord, convert_records};
    use crate::agent::ParsedRecord;
    use crate::session::{MessagePhase, MessageRole};

    #[test]
    fn converts_assistant_response_output_text() {
        let record = parse_record(
            r#"{
                "timestamp":"2026-07-13T01:00:00Z",
                "type":"response_item",
                "payload":{
                    "type":"message",
                    "id":"answer",
                    "role":"assistant",
                    "content":[
                        {"type":"reasoning","text":"hidden"},
                        {"type":"output_text","text":"Visible answer"}
                    ],
                    "phase":"final_answer"
                }
            }"#,
        );

        let ParsedRecord::Message(message) = ParsedRecord::from(record) else {
            panic!("expected a conversation message");
        };

        assert_eq!(message.role, MessageRole::Assistant);
        assert_eq!(message.text, "Visible answer");
        assert_eq!(message.phase, Some(MessagePhase::FinalAnswer));
    }

    #[test]
    fn ignores_non_display_records() {
        let future_record = parse_record(r#"{"type":"future_record"}"#);
        assert!(matches!(
            ParsedRecord::from(future_record),
            ParsedRecord::Ignored
        ));

        for payload in [
            r#"{"type":"message","role":"user","content":[{"type":"output_text","text":"User"}]}"#,
            r#"{"type":"function_call","name":"exec_command"}"#,
            r#"{"type":"custom_tool_call","name":"apply_patch","input":"patch"}"#,
        ] {
            let record = parse_record(format!(
                r#"{{"timestamp":"2026-07-13T01:00:00Z","type":"response_item","payload":{payload}}}"#
            ));

            assert!(matches!(ParsedRecord::from(record), ParsedRecord::Ignored));
        }
    }

    #[test]
    fn deduplicates_only_matching_cross_representation_messages() {
        let records = [
            event_message("Repeated", "commentary"),
            response_message("Repeated", "commentary"),
            event_message("First update", "commentary"),
            event_message("Second update", "commentary"),
        ];

        let converted = convert_records(records.into_iter().map(parse_record).collect());
        let texts = converted
            .iter()
            .filter_map(|record| match record {
                ParsedRecord::Message(message) => Some(message.text.as_str()),
                ParsedRecord::Session { .. } | ParsedRecord::Ignored => None,
            })
            .collect::<Vec<_>>();

        assert_eq!(texts, ["Repeated", "First update", "Second update"]);
    }

    fn parse_record(json: impl AsRef<str>) -> CodexRecord {
        serde_json::from_str(json.as_ref()).unwrap()
    }

    fn event_message(text: &str, phase: &str) -> String {
        format!(
            r#"{{"timestamp":"2026-07-13T01:00:00Z","type":"event_msg","payload":{{"type":"agent_message","message":"{text}","phase":"{phase}"}}}}"#
        )
    }

    fn response_message(text: &str, phase: &str) -> String {
        format!(
            r#"{{"timestamp":"2026-07-13T01:00:01Z","type":"response_item","payload":{{"type":"message","role":"assistant","content":[{{"type":"output_text","text":"{text}"}}],"phase":"{phase}"}}}}"#
        )
    }
}
