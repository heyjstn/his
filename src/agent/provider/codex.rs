use crate::agent::provider::{AgentMessage, FromProviderMessage};
use serde::Deserialize;

#[derive(Deserialize, Debug)]
pub struct CodexMessage {}

impl FromProviderMessage for CodexMessage {}

impl From<CodexMessage> for AgentMessage {
    fn from(_value: CodexMessage) -> Self {
        todo!()
    }
}
