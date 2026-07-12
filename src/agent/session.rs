use super::provider::{
    AgentMessage, CodexMessage, FromProviderMessage, PiMessage, Provider, ProviderEnum,
};
use anyhow::{Context, Result, anyhow};
use serde::Deserialize;
use std::path::Path;

#[derive(Deserialize, Debug)]
pub struct Session {
    pub id: String,
    pub provider: ProviderEnum,
    pub ts: String,
    pub cwd: String,
    pub messages: Option<Vec<SessionMessage>>,
    pub first_message: String,
}

#[derive(Deserialize, Debug)]
pub struct SessionMessage {
    pub id: String,
    pub provider: ProviderEnum,
    pub ts: String,
    pub role: String,
    pub text: String,
}

pub fn list_sessions(provider: &Provider) -> Result<Vec<Session>> {
    let file_paths = super::provider::walk_dir(&provider.dir)?;

    Ok(file_paths
        .iter()
        .filter_map(|path| parse_session(provider, path, false).ok())
        .collect())
}

pub fn load_session(provider: &Provider, session_id: String) -> Result<Session> {
    let file_paths = super::provider::walk_dir(&provider.dir)?;
    let mut parse_error = None;

    for path in file_paths {
        let session = match parse_session(provider, &path, true) {
            Ok(session) => session,
            Err(error) => {
                parse_error = Some(error);
                continue;
            }
        };

        if session.id == session_id {
            return Ok(session);
        }
    }

    if let Some(error) = parse_error {
        return Err(error).with_context(|| format!("failed to find session {session_id}"));
    }

    Err(anyhow!(
        "session {session_id} was not found for {:?}",
        provider.name
    ))
}

fn parse_session(provider: &Provider, path: &Path, include_messages: bool) -> Result<Session> {
    let data = parse_messages(provider, path)?;
    let initialized_message = data
        .iter()
        .find(|message| message.typ == "session")
        .with_context(|| format!("missing session metadata in {}", path.display()))?;
    let first_message = data
        .iter()
        .find(|message| message.typ == "message" && message.role.as_deref() == Some("user"))
        .or_else(|| data.iter().find(|message| message.typ == "message"))
        .and_then(|message| message.text.clone())
        .unwrap_or_else(|| "(no text messages)".to_string());
    let messages = include_messages.then(|| {
        data.iter()
            .filter(|message| {
                message.typ == "message"
                    && matches!(message.role.as_deref(), Some("user" | "assistant"))
                    && message.text.is_some()
            })
            .map(|message| SessionMessage {
                id: message.id.clone(),
                provider: provider.name,
                ts: message.timestamp.clone(),
                role: message
                    .role
                    .clone()
                    .unwrap_or_else(|| "message".to_string()),
                text: message.text.clone().unwrap_or_default(),
            })
            .collect()
    });

    Ok(Session {
        id: initialized_message.id.clone(),
        provider: provider.name,
        ts: initialized_message.timestamp.clone(),
        cwd: initialized_message
            .cwd
            .clone()
            .with_context(|| format!("missing cwd in {}", path.display()))?,
        messages,
        first_message,
    })
}

fn parse_messages(provider: &Provider, path: &Path) -> Result<Vec<AgentMessage>> {
    match provider.name {
        ProviderEnum::Codex => CodexMessage::parse_vec(path),
        ProviderEnum::Pi => PiMessage::parse_vec(path),
    }
}

#[cfg(test)]
mod tests {
    use super::load_session;
    use crate::agent::provider::{Provider, ProviderEnum};
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn loads_pi_session_messages() {
        let data = concat!(
            r#"{"type":"session","version":3,"id":"pi-session","timestamp":"2026-07-12T01:00:00Z","cwd":"/tmp/pi"}"#,
            "\n",
            r#"{"type":"message","id":"user-1","parentId":null,"timestamp":"2026-07-12T01:01:00Z","message":{"role":"user","content":[{"type":"text","text":"Hello"}],"timestamp":1}}"#,
            "\n",
            r#"{"type":"message","id":"assistant-1","parentId":"user-1","timestamp":"2026-07-12T01:02:00Z","message":{"role":"assistant","content":[{"type":"text","text":"Hi there"}],"api":"responses","provider":"test","model":"test-model","usage":{"input":1,"output":1,"cacheRead":0,"cacheWrite":0,"totalTokens":2,"cost":{"input":0.0,"output":0.0,"cacheRead":0.0,"cacheWrite":0.0,"total":0.0}},"stopReason":"stop","timestamp":2,"responseId":"response-1"}}"#,
        );
        let (dir, provider) = test_provider(ProviderEnum::Pi, "session.jsonl", data);

        let session = load_session(&provider, "pi-session".to_string()).unwrap();

        assert_eq!(session.first_message, "Hello");
        let messages = session.messages.unwrap();
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].role, "user");
        assert_eq!(messages[1].text, "Hi there");
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn loads_pretty_printed_codex_session_messages() {
        let data = r#"
            {
                "timestamp": "2026-07-12T01:00:00Z",
                "type": "session_meta",
                "payload": {
                    "id": "codex-session",
                    "timestamp": "2026-07-12T01:00:00Z",
                    "cwd": "/tmp/codex"
                }
            }
            {
                "timestamp": "2026-07-12T01:01:00Z",
                "type": "event_msg",
                "payload": {
                    "type": "user_message",
                    "message": "Read this"
                }
            }
        "#;
        let (dir, provider) = test_provider(ProviderEnum::Codex, "session.jsonl", data);

        let session = load_session(&provider, "codex-session".to_string()).unwrap();

        assert_eq!(session.cwd, "/tmp/codex");
        assert_eq!(session.messages.unwrap()[0].text, "Read this");
        fs::remove_dir_all(dir).unwrap();
    }

    fn test_provider(name: ProviderEnum, file_name: &str, data: &str) -> (PathBuf, Provider) {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("his-load-session-{nonce}"));
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join(file_name), data).unwrap();
        let provider = Provider {
            name,
            dir: dir.to_string_lossy().into_owned(),
        };
        (dir, provider)
    }
}
