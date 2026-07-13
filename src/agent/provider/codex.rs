use crate::agent::provider::{AgentMessage, FromProviderMessage};
use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize, Debug)]
pub struct CodexMessage {
    pub timestamp: String,
    #[serde(rename = "type")]
    pub typ: String,
    pub payload: Value,
}

impl FromProviderMessage for CodexMessage {}

impl From<CodexMessage> for AgentMessage {
    fn from(value: CodexMessage) -> Self {
        let payload_type = string_field(&value.payload, "type");
        let is_session = value.typ == "session_meta";
        let role = match (value.typ.as_str(), payload_type) {
            ("event_msg", Some("user_message")) => Some("user".to_string()),
            ("event_msg", Some("agent_message")) => Some("assistant".to_string()),
            _ => None,
        };
        let is_message = role.is_some();
        let text = is_message
            .then(|| string_field(&value.payload, "message"))
            .flatten()
            .map(str::to_string);
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
            phase: string_field(&value.payload, "phase").map(str::to_string),
            provider: string_field(&value.payload, "model_provider").map(str::to_string),
            model: string_field(&value.payload, "model").map(str::to_string),
            tool_call_id: string_field(&value.payload, "call_id").map(str::to_string),
            tool_name: string_field(&value.payload, "name").map(str::to_string),
            is_error: None,
        }
    }
}

fn string_field<'a>(value: &'a Value, field: &str) -> Option<&'a str> {
    value.get(field).and_then(Value::as_str)
}
