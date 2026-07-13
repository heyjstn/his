use super::provider::{
    AgentMessage, CodexMessage, FromProviderMessage, PiMessage, Provider, ProviderEnum,
};
use anyhow::{Context, Result, anyhow};
use serde::Deserialize;
use std::collections::HashSet;
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
    pub phase: Option<String>,
}

#[derive(Debug)]
pub struct SessionRepository<'a> {
    providers: &'a [Provider],
}

impl<'a> SessionRepository<'a> {
    pub fn new(providers: &'a [Provider]) -> Result<Self> {
        let mut provider_names = HashSet::new();
        for provider in providers {
            if !provider_names.insert(provider.name) {
                return Err(anyhow!(
                    "provider {:?} is configured more than once",
                    provider.name
                ));
            }
        }
        Ok(Self { providers })
    }

    pub fn list_sessions(&self) -> Result<Vec<Session>> {
        let mut sessions = Vec::new();
        for provider in self.providers {
            sessions.extend(list_provider_sessions(provider)?);
        }
        sessions.sort_by_key(|session| session.ts.clone());
        Ok(sessions)
    }

    pub fn load_session(&self, provider_name: ProviderEnum, session_id: &str) -> Result<Session> {
        let provider = self
            .providers
            .iter()
            .find(|provider| provider.name == provider_name)
            .ok_or_else(|| anyhow!("provider {provider_name:?} is not configured"))?;

        load_provider_session(provider, session_id)
            .with_context(|| format!("failed to load session {session_id}"))
    }
}

fn list_provider_sessions(provider: &Provider) -> Result<Vec<Session>> {
    let file_paths = super::provider::walk_dir(&provider.dir)?;

    Ok(file_paths
        .iter()
        .filter_map(|path| parse_session(provider, path, false).ok())
        .collect())
}

fn load_provider_session(provider: &Provider, session_id: &str) -> Result<Session> {
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
                phase: message.phase.clone(),
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
    use super::SessionRepository;
    use crate::agent::provider::{Provider, ProviderEnum};
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_TEST_DIR: AtomicU64 = AtomicU64::new(0);

    #[test]
    fn loads_pi_session_messages() {
        let data = concat!(
            r#"{"type":"session","version":3,"id":"pi-session","timestamp":"2026-07-12T01:00:00Z","cwd":"/tmp/pi"}"#,
            "\n",
            r#"{"type":"message","id":"user-1","parentId":null,"timestamp":"2026-07-12T01:01:00Z","message":{"role":"user","content":[{"type":"text","text":"Hello"}],"timestamp":1}}"#,
            "\n",
            r#"{"type":"message","id":"assistant-1","parentId":"user-1","timestamp":"2026-07-12T01:02:00Z","message":{"role":"assistant","content":[{"type":"thinking","thinking":"Checking the request","thinkingSignature":"signature"},{"type":"toolCall","id":"call-1","name":"read","arguments":{}},{"type":"toolCall","id":"call-2","name":"edit","arguments":{}},{"type":"text","text":"Hi there"}],"api":"responses","provider":"test","model":"test-model","usage":{"input":1,"output":1,"cacheRead":0,"cacheWrite":0,"totalTokens":2,"cost":{"input":0.0,"output":0.0,"cacheRead":0.0,"cacheWrite":0.0,"total":0.0}},"stopReason":"stop","timestamp":2,"responseId":"response-1"}}"#,
        );
        let (dir, provider) = test_provider(ProviderEnum::Pi, "session.jsonl", data);

        let repository = SessionRepository::new(std::slice::from_ref(&provider)).unwrap();
        let session = repository
            .load_session(ProviderEnum::Pi, "pi-session")
            .unwrap();

        assert_eq!(session.first_message, "Hello");
        let messages = session.messages.unwrap();
        assert_eq!(messages.len(), 4);
        assert_eq!(messages[0].role, "user");
        assert_eq!(messages[1].text, "Checking the request");
        assert_eq!(messages[1].phase.as_deref(), Some("commentary"));
        assert_eq!(messages[2].text, "edit");
        assert_eq!(messages[2].phase.as_deref(), Some("tool_call"));
        assert_eq!(messages[3].text, "Hi there");
        assert_eq!(messages[3].phase, None);
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
            {
                "timestamp": "2026-07-12T01:02:00Z",
                "type": "event_msg",
                "payload": {
                    "type": "agent_message",
                    "message": "Working on it",
                    "phase": "commentary"
                }
            }
            {
                "timestamp": "2026-07-12T01:02:30Z",
                "type": "response_item",
                "payload": {
                    "type": "function_call",
                    "id": "tool-call",
                    "name": "exec_command",
                    "arguments": "{}",
                    "call_id": "call-1"
                }
            }
            {
                "timestamp": "2026-07-12T01:02:45Z",
                "type": "response_item",
                "payload": {
                    "type": "custom_tool_call",
                    "id": "edit-call",
                    "name": "apply_patch",
                    "input": "*** Begin Patch",
                    "call_id": "call-2"
                }
            }
            {
                "timestamp": "2026-07-12T01:03:00Z",
                "type": "response_item",
                "payload": {
                    "type": "message",
                    "id": "final-answer",
                    "role": "assistant",
                    "content": [
                        {
                            "type": "output_text",
                            "text": "Implementation plan"
                        }
                    ],
                    "phase": "final_answer"
                }
            }
        "#;
        let (dir, provider) = test_provider(ProviderEnum::Codex, "session.jsonl", data);

        let repository = SessionRepository::new(std::slice::from_ref(&provider)).unwrap();
        let session = repository
            .load_session(ProviderEnum::Codex, "codex-session")
            .unwrap();

        assert_eq!(session.cwd, "/tmp/codex");
        let messages = session.messages.unwrap();
        assert_eq!(messages[0].text, "Read this");
        assert_eq!(messages[1].phase.as_deref(), Some("commentary"));
        assert_eq!(messages.len(), 4);
        assert_eq!(messages[2].text, "apply_patch");
        assert_eq!(messages[2].phase.as_deref(), Some("tool_call"));
        assert_eq!(messages[3].text, "Implementation plan");
        assert_eq!(messages[3].phase.as_deref(), Some("final_answer"));
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn returns_no_sessions_without_providers() {
        let repository = SessionRepository::new(&[]).unwrap();

        assert!(repository.list_sessions().unwrap().is_empty());
    }

    #[test]
    fn rejects_an_unconfigured_provider() {
        let repository = SessionRepository::new(&[]).unwrap();

        let error = repository
            .load_session(ProviderEnum::Codex, "missing")
            .unwrap_err();

        assert_eq!(format!("{error:#}"), "provider Codex is not configured");
    }

    #[test]
    fn lists_sessions_in_timestamp_order() {
        let earlier_data = r#"{"type":"session","version":3,"id":"earlier","timestamp":"2026-07-11T01:00:00Z","cwd":"/tmp/earlier"}"#;
        let later_data = r#"{"type":"session","version":3,"id":"later","timestamp":"2026-07-12T01:00:00Z","cwd":"/tmp/later"}"#;
        let (dir, provider) = test_provider(ProviderEnum::Pi, "earlier.jsonl", earlier_data);
        fs::write(dir.join("later.jsonl"), later_data).unwrap();
        let providers = [provider];
        let repository = SessionRepository::new(&providers).unwrap();

        let sessions = repository.list_sessions().unwrap();

        assert_eq!(
            sessions
                .iter()
                .map(|session| session.id.as_str())
                .collect::<Vec<_>>(),
            ["earlier", "later"]
        );
        assert!(sessions.iter().all(|session| session.messages.is_none()));
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn loads_from_the_requested_provider() {
        let codex_data = r#"
            {
                "timestamp": "2026-07-12T01:00:00Z",
                "type": "session_meta",
                "payload": {
                    "id": "codex-session",
                    "timestamp": "2026-07-12T01:00:00Z",
                    "cwd": "/tmp/codex"
                }
            }
        "#;
        let pi_data = r#"{"type":"session","version":3,"id":"pi-session","timestamp":"2026-07-12T01:00:00Z","cwd":"/tmp/pi"}"#;
        let (codex_dir, codex_provider) =
            test_provider(ProviderEnum::Codex, "codex.jsonl", codex_data);
        let (pi_dir, pi_provider) = test_provider(ProviderEnum::Pi, "pi.jsonl", pi_data);
        let providers = [codex_provider, pi_provider];
        let repository = SessionRepository::new(&providers).unwrap();

        let session = repository
            .load_session(ProviderEnum::Pi, "pi-session")
            .unwrap();

        assert_eq!(session.provider, ProviderEnum::Pi);
        assert_eq!(session.cwd, "/tmp/pi");
        fs::remove_dir_all(codex_dir).unwrap();
        fs::remove_dir_all(pi_dir).unwrap();
    }

    #[test]
    fn rejects_duplicate_provider_types() {
        let providers = [
            Provider {
                name: ProviderEnum::Pi,
                dir: "/tmp/first".to_string(),
            },
            Provider {
                name: ProviderEnum::Pi,
                dir: "/tmp/second".to_string(),
            },
        ];

        let error = SessionRepository::new(&providers).unwrap_err();

        assert_eq!(
            format!("{error:#}"),
            "provider Pi is configured more than once"
        );
    }

    fn test_provider(name: ProviderEnum, file_name: &str, data: &str) -> (PathBuf, Provider) {
        let sequence = NEXT_TEST_DIR.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "his-load-session-{}-{sequence}-{file_name}",
            std::process::id()
        ));
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join(file_name), data).unwrap();
        let provider = Provider {
            name,
            dir: dir.to_string_lossy().into_owned(),
        };
        (dir, provider)
    }
}
