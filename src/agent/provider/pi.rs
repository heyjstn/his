use crate::agent::provider::{AgentMessage, FromProviderMessage};
use serde::Deserialize;

#[derive(Deserialize, Debug)]
pub struct PiMessage {
    #[serde(rename = "type")]
    pub typ: String,
    pub version: Option<usize>,
    pub id: String,
    pub timestamp: String,
    pub cwd: Option<String>,
}

impl FromProviderMessage for PiMessage {}

impl From<PiMessage> for AgentMessage {
    fn from(value: PiMessage) -> Self {
        AgentMessage {
            typ: value.typ,
            id: value.id,
            timestamp: value.timestamp,
            cwd: value.cwd,
        }
    }
}
