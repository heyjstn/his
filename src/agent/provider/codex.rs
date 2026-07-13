use crate::agent::provider::{AgentMessage, FromProviderMessage, TOOL_CALL_PHASE};
use anyhow::{Context, Result};
use serde::Deserialize;
use serde_json::Value;
use std::path::Path;

const APPLY_PATCH_TOOL: &str = "apply_patch";
const APPLY_PATCH_LABEL: &str = "apply patch";

#[derive(Deserialize, Debug)]
pub struct CodexMessage {
    pub timestamp: String,
    #[serde(rename = "type")]
    pub typ: String,
    pub payload: Value,
}

impl FromProviderMessage for CodexMessage {
    fn parse_vec(path: &Path) -> Result<Vec<AgentMessage>> {
        let file = Self::read_to_string(path)?;
        let messages = serde_json::Deserializer::from_str(&file)
            .into_iter::<Self>()
            .map(|message| {
                message.with_context(|| format!("failed to parse session file {}", path.display()))
            })
            .collect::<Result<Vec<_>>>()?;

        Ok(convert_messages(messages))
    }
}

impl From<CodexMessage> for AgentMessage {
    fn from(value: CodexMessage) -> Self {
        let payload_type = string_field(&value.payload, "type");
        let is_session = value.typ == "session_meta";
        let is_edit_tool_call = value.typ == "response_item" && is_edit_tool_call(&value.payload);
        let (role, text) = match (value.typ.as_str(), payload_type) {
            ("event_msg", Some("user_message")) => (
                Some("user".to_string()),
                string_field(&value.payload, "message").map(str::to_string),
            ),
            ("event_msg", Some("agent_message")) => (
                Some("assistant".to_string()),
                string_field(&value.payload, "message").map(str::to_string),
            ),
            ("response_item", Some("message"))
                if string_field(&value.payload, "role") == Some("assistant") =>
            {
                let text = output_text(&value.payload);
                (text.as_ref().map(|_| "assistant".to_string()), text)
            }
            ("response_item", _) if is_edit_tool_call => (
                Some("assistant".to_string()),
                Some(APPLY_PATCH_LABEL.to_string()),
            ),
            _ => (None, None),
        };
        let is_message = role.is_some();
        let timestamp = if is_session {
            string_field(&value.payload, "timestamp")
                .unwrap_or(&value.timestamp)
                .to_string()
        } else {
            value.timestamp
        };
        let id = string_field(&value.payload, "id")
            .or_else(|| string_field(&value.payload, "session_id"))
            .map(str::to_string)
            .unwrap_or_else(|| format!("{}:{}", timestamp, value.typ));

        AgentMessage {
            typ: if is_session {
                "session".to_string()
            } else if is_message {
                "message".to_string()
            } else {
                value.typ
            },
            id,
            parent_id: None,
            timestamp,
            cwd: is_session
                .then(|| string_field(&value.payload, "cwd"))
                .flatten()
                .map(str::to_string),
            role,
            text,
            phase: if is_edit_tool_call {
                Some(TOOL_CALL_PHASE.to_string())
            } else {
                string_field(&value.payload, "phase").map(str::to_string)
            },
            provider: string_field(&value.payload, "model_provider").map(str::to_string),
            model: string_field(&value.payload, "model").map(str::to_string),
            tool_call_id: string_field(&value.payload, "call_id").map(str::to_string),
            tool_name: string_field(&value.payload, "name").map(str::to_string),
            tool_path: None,
            tool_contents: if is_edit_tool_call {
                string_field(&value.payload, "input")
                    .map(|input| vec![input.to_string()])
                    .unwrap_or_default()
            } else {
                Vec::new()
            },
            is_error: None,
        }
    }
}

fn is_edit_tool_call(payload: &Value) -> bool {
    string_field(payload, "type") == Some("custom_tool_call")
        && string_field(payload, "name") == Some(APPLY_PATCH_TOOL)
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

fn convert_messages(messages: Vec<CodexMessage>) -> Vec<AgentMessage> {
    let mut converted = Vec::with_capacity(messages.len());
    let mut pending_assistant: Option<(usize, AssistantMessageSource)> = None;

    for message in messages {
        let is_message_record = is_message_record(&message);
        let source = assistant_message_source(&message);
        let message = AgentMessage::from(message);

        if message.typ != "message" || message.text.is_none() {
            if is_message_record {
                pending_assistant = None;
            }
            converted.push(message);
            continue;
        }

        let Some(source) = source else {
            pending_assistant = None;
            converted.push(message);
            continue;
        };

        if let Some((index, pending_source)) = pending_assistant {
            let pending = &mut converted[index];
            if source != pending_source && is_same_utterance(pending, &message) {
                if pending.phase.is_none() {
                    pending.phase = message.phase;
                }
                pending_assistant = None;
                continue;
            }
        }

        let index = converted.len();
        converted.push(message);
        pending_assistant = Some((index, source));
    }

    converted
}

fn is_message_record(message: &CodexMessage) -> bool {
    matches!(
        (message.typ.as_str(), string_field(&message.payload, "type")),
        ("event_msg", Some("user_message" | "agent_message")) | ("response_item", Some("message"))
    )
}

fn assistant_message_source(message: &CodexMessage) -> Option<AssistantMessageSource> {
    match (
        message.typ.as_str(),
        string_field(&message.payload, "type"),
        string_field(&message.payload, "role"),
    ) {
        ("event_msg", Some("agent_message"), _) => Some(AssistantMessageSource::Event),
        ("response_item", Some("message"), Some("assistant")) => {
            Some(AssistantMessageSource::Response)
        }
        _ => None,
    }
}

fn is_same_utterance(left: &AgentMessage, right: &AgentMessage) -> bool {
    left.role == right.role
        && left.text == right.text
        && (left.phase == right.phase || left.phase.is_none() || right.phase.is_none())
}

#[cfg(test)]
mod tests {
    use super::{CodexMessage, convert_messages};
    use crate::agent::provider::{AgentMessage, TOOL_CALL_PHASE};

    #[test]
    fn converts_assistant_response_output_text() {
        let message = parse_message(
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

        let converted = AgentMessage::from(message);

        assert_eq!(converted.typ, "message");
        assert_eq!(converted.role.as_deref(), Some("assistant"));
        assert_eq!(converted.text.as_deref(), Some("Visible answer"));
        assert_eq!(converted.phase.as_deref(), Some("final_answer"));
    }

    #[test]
    fn ignores_response_items_without_assistant_output_text() {
        for payload in [
            r#"{"type":"message","role":"user","content":[{"type":"output_text","text":"User"}]}"#,
            r#"{"type":"message","role":"assistant","content":[{"type":"input_text","text":"Input"}]}"#,
        ] {
            let message = parse_message(format!(
                r#"{{"timestamp":"2026-07-13T01:00:00Z","type":"response_item","payload":{payload}}}"#
            ));

            let converted = AgentMessage::from(message);

            assert_ne!(converted.typ, "message");
            assert!(converted.text.is_none());
        }
    }

    #[test]
    fn converts_only_apply_patch_calls_to_commentary_messages() {
        let edit = parse_message(
            r#"{"timestamp":"2026-07-13T01:00:00Z","type":"response_item","payload":{"type":"custom_tool_call","id":"custom","name":"apply_patch","input":"*** Begin Patch","call_id":"call-2"}}"#,
        );

        let converted = AgentMessage::from(edit);

        assert_eq!(converted.typ, "message");
        assert_eq!(converted.role.as_deref(), Some("assistant"));
        assert_eq!(converted.text.as_deref(), Some("apply patch"));
        assert_eq!(converted.phase.as_deref(), Some(TOOL_CALL_PHASE));
        assert_eq!(converted.tool_call_id.as_deref(), Some("call-2"));
        assert_eq!(converted.tool_name.as_deref(), Some("apply_patch"));
        assert_eq!(converted.tool_path, None);
        assert_eq!(converted.tool_contents, ["*** Begin Patch"]);

        for payload in [
            r#"{"type":"function_call","name":"exec_command"}"#,
            r#"{"type":"function_call","name":"apply_patch"}"#,
            r#"{"type":"custom_tool_call","name":"exec_command"}"#,
            r#"{"type":"function_call","name":"request_user_input"}"#,
            r#"{"type":"function_call","name":"update_plan"}"#,
        ] {
            let call = parse_message(format!(
                r#"{{"timestamp":"2026-07-13T01:00:00Z","type":"response_item","payload":{payload}}}"#
            ));

            let converted = AgentMessage::from(call);

            assert_ne!(converted.typ, "message");
            assert!(converted.role.is_none());
            assert!(converted.text.is_none());
            assert!(converted.phase.is_none());
        }
    }

    #[test]
    fn deduplicates_only_matching_cross_representation_messages() {
        let messages = [
            event_message("Repeated", "commentary"),
            response_message("Repeated", "commentary"),
            event_message("First update", "commentary"),
            event_message("Second update", "commentary"),
        ];

        let converted = convert_messages(messages.into_iter().map(parse_message).collect());
        let texts = converted
            .iter()
            .filter_map(|message| message.text.as_deref())
            .collect::<Vec<_>>();

        assert_eq!(texts, ["Repeated", "First update", "Second update"]);
    }

    #[test]
    fn does_not_deduplicate_across_an_ignored_message_record() {
        let messages = [
            event_message("Repeated", "commentary"),
            r#"{"timestamp":"2026-07-13T01:00:01Z","type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"Continue"}]}}"#.to_string(),
            response_message("Repeated", "commentary"),
        ];

        let converted = convert_messages(messages.into_iter().map(parse_message).collect());
        let texts = converted
            .iter()
            .filter_map(|message| message.text.as_deref())
            .collect::<Vec<_>>();

        assert_eq!(texts, ["Repeated", "Repeated"]);
    }

    fn parse_message(json: impl AsRef<str>) -> CodexMessage {
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
