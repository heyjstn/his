use super::provider::{CodexMessage, FromProviderMessage, PiMessage, Provider, ProviderEnum};
use serde::Deserialize;

#[derive(Deserialize, Debug)]
pub struct Session {
    pub id: String,
    pub provider: ProviderEnum,
    pub ts: String,
    pub cwd: String,
    pub messages: Option<Vec<SessionMessage>>,
}

#[derive(Deserialize, Debug)]
pub struct SessionMessage {
    pub id: String,
    pub provider: ProviderEnum,
    pub ts: String,
    pub text: String,
}

pub fn list_sessions(provider: &Provider) -> Vec<Session> {
    let file_paths = super::provider::walk_dir(&provider.dir).unwrap();
    let sessions: Vec<Session> = file_paths
        .iter()
        .map(|path| {
            match provider.name {
                ProviderEnum::Codex => CodexMessage::parse_vec(path).unwrap(),
                ProviderEnum::Pi => PiMessage::parse_vec(path).unwrap()
            }
        })
        .map(|data| {
            let initialized_message = data.get(0).unwrap();
            Session {
                id: initialized_message.id.clone(),
                provider: provider.name.clone(),
                ts: initialized_message.timestamp.clone(),
                cwd: initialized_message.cwd.clone().unwrap(),
                messages: None,
            }
        })
        .collect();
    sessions
    // }
}

pub fn load_session(_provider: &Provider, _session_id: String) -> Session {
    todo!()
}
