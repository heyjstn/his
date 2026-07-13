use crate::agent::{Agent, AgentKind};
use anyhow::{Context, Result, anyhow};
use serde::Deserialize;
use std::collections::HashSet;
use std::path::Path;

#[derive(Deserialize, Debug)]
pub struct Session {
    pub id: String,
    pub agent: AgentKind,
    pub ts: String,
    pub cwd: String,
    pub messages: Option<Vec<SessionMessage>>,
    pub first_message: String,
}

#[derive(Deserialize, Debug)]
pub struct SessionMessage {
    pub id: String,
    pub agent: AgentKind,
    pub ts: String,
    pub role: String,
    pub text: String,
    pub phase: Option<String>,
    pub tool_path: Option<String>,
    #[serde(default)]
    pub tool_contents: Vec<String>,
}

#[derive(Debug)]
pub struct SessionRepository<'a> {
    agents: &'a [Agent],
}

impl<'a> SessionRepository<'a> {
    pub fn new(agents: &'a [Agent]) -> Result<Self> {
        let mut agent_kinds = HashSet::new();
        for agent in agents {
            if !agent_kinds.insert(agent.kind) {
                return Err(anyhow!(
                    "agent {:?} is configured more than once",
                    agent.kind
                ));
            }
        }
        Ok(Self { agents })
    }

    pub fn list_sessions(&self) -> Result<Vec<Session>> {
        let mut sessions = Vec::new();
        for agent in self.agents {
            sessions.extend(list_agent_sessions(agent)?);
        }
        sessions.sort_by_key(|session| session.ts.clone());
        Ok(sessions)
    }

    pub fn load_session(&self, agent_kind: AgentKind, session_id: &str) -> Result<Session> {
        let agent = self
            .agents
            .iter()
            .find(|agent| agent.kind == agent_kind)
            .ok_or_else(|| anyhow!("agent {agent_kind:?} is not configured"))?;

        load_agent_session(agent, session_id)
            .with_context(|| format!("failed to load session {session_id}"))
    }
}

fn list_agent_sessions(agent: &Agent) -> Result<Vec<Session>> {
    let file_paths = agent.get_session_paths()?;

    Ok(file_paths
        .iter()
        .filter_map(|path| parse_session(agent, path, false).ok())
        .collect())
}

fn load_agent_session(agent: &Agent, session_id: &str) -> Result<Session> {
    let file_paths = agent.get_session_paths()?;
    let mut parse_error = None;

    for path in file_paths {
        let session = match parse_session(agent, &path, true) {
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
        agent.kind
    ))
}

fn parse_session(agent: &Agent, path: &Path, include_messages: bool) -> Result<Session> {
    let data = agent.parse(path)?;
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
                agent: agent.kind,
                ts: message.timestamp.clone(),
                role: message
                    .role
                    .clone()
                    .unwrap_or_else(|| "message".to_string()),
                text: message.text.clone().unwrap_or_default(),
                phase: message.phase.clone(),
                tool_path: message.tool_path.clone(),
                tool_contents: message.tool_contents.clone(),
            })
            .collect()
    });

    Ok(Session {
        id: initialized_message.id.clone(),
        agent: agent.kind,
        ts: initialized_message.timestamp.clone(),
        cwd: initialized_message
            .cwd
            .clone()
            .with_context(|| format!("missing cwd in {}", path.display()))?,
        messages,
        first_message,
    })
}

#[cfg(test)]
mod tests {
    use super::SessionRepository;
    use crate::agent::{Agent, AgentKind};
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
            r#"{"type":"message","id":"assistant-1","parentId":"user-1","timestamp":"2026-07-12T01:02:00Z","message":{"role":"assistant","content":[{"type":"thinking","thinking":"Checking the request","thinkingSignature":"signature"},{"type":"toolCall","id":"call-1","name":"read","arguments":{}},{"type":"toolCall","id":"call-2","name":"edit","arguments":{"path":"/tmp/pi/file.rs","edits":[{"oldText":"before","newText":"after"}]}},{"type":"text","text":"Hi there"}],"api":"responses","provider":"test","model":"test-model","usage":{"input":1,"output":1,"cacheRead":0,"cacheWrite":0,"totalTokens":2,"cost":{"input":0.0,"output":0.0,"cacheRead":0.0,"cacheWrite":0.0,"total":0.0}},"stopReason":"stop","timestamp":2,"responseId":"response-1"}}"#,
        );
        let (dir, agent) = test_agent(AgentKind::Pi, "session.jsonl", data);

        let repository = SessionRepository::new(std::slice::from_ref(&agent)).unwrap();
        let session = repository
            .load_session(AgentKind::Pi, "pi-session")
            .unwrap();

        assert_eq!(session.first_message, "Hello");
        let messages = session.messages.unwrap();
        assert_eq!(messages.len(), 4);
        assert_eq!(messages[0].role, "user");
        assert_eq!(messages[1].text, "Checking the request");
        assert_eq!(messages[1].phase.as_deref(), Some("commentary"));
        assert_eq!(messages[2].text, "edit");
        assert_eq!(messages[2].phase.as_deref(), Some("tool_call"));
        assert_eq!(messages[2].tool_path.as_deref(), Some("/tmp/pi/file.rs"));
        assert_eq!(messages[2].tool_contents, ["after"]);
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
                    "input": "*** Begin Patch\n+edited content\n*** End Patch",
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
        let (dir, agent) = test_agent(AgentKind::Codex, "session.jsonl", data);

        let repository = SessionRepository::new(std::slice::from_ref(&agent)).unwrap();
        let session = repository
            .load_session(AgentKind::Codex, "codex-session")
            .unwrap();

        assert_eq!(session.cwd, "/tmp/codex");
        let messages = session.messages.unwrap();
        assert_eq!(messages[0].text, "Read this");
        assert_eq!(messages[1].phase.as_deref(), Some("commentary"));
        assert_eq!(messages.len(), 3);
        assert_eq!(messages[2].text, "Implementation plan");
        assert_eq!(messages[2].phase.as_deref(), Some("final_answer"));
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn returns_no_sessions_without_agents() {
        let repository = SessionRepository::new(&[]).unwrap();

        assert!(repository.list_sessions().unwrap().is_empty());
    }

    #[test]
    fn rejects_an_unconfigured_agent() {
        let repository = SessionRepository::new(&[]).unwrap();

        let error = repository
            .load_session(AgentKind::Codex, "missing")
            .unwrap_err();

        assert_eq!(format!("{error:#}"), "agent Codex is not configured");
    }

    #[test]
    fn lists_sessions_in_timestamp_order() {
        let earlier_data = r#"{"type":"session","version":3,"id":"earlier","timestamp":"2026-07-11T01:00:00Z","cwd":"/tmp/earlier"}"#;
        let later_data = r#"{"type":"session","version":3,"id":"later","timestamp":"2026-07-12T01:00:00Z","cwd":"/tmp/later"}"#;
        let (dir, agent) = test_agent(AgentKind::Pi, "earlier.jsonl", earlier_data);
        let nested_dir = dir.join("nested");
        fs::create_dir(&nested_dir).unwrap();
        fs::write(nested_dir.join("later.jsonl"), later_data).unwrap();
        let agents = [agent];
        let repository = SessionRepository::new(&agents).unwrap();

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
    fn loads_from_the_requested_agent() {
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
        let (codex_dir, codex_agent) = test_agent(AgentKind::Codex, "codex.jsonl", codex_data);
        let (pi_dir, pi_agent) = test_agent(AgentKind::Pi, "pi.jsonl", pi_data);
        let agents = [codex_agent, pi_agent];
        let repository = SessionRepository::new(&agents).unwrap();

        let session = repository
            .load_session(AgentKind::Pi, "pi-session")
            .unwrap();

        assert_eq!(session.agent, AgentKind::Pi);
        assert_eq!(session.cwd, "/tmp/pi");
        fs::remove_dir_all(codex_dir).unwrap();
        fs::remove_dir_all(pi_dir).unwrap();
    }

    #[test]
    fn rejects_duplicate_agent_types() {
        let agents = [
            Agent {
                kind: AgentKind::Pi,
                dir: "/tmp/first".to_string(),
            },
            Agent {
                kind: AgentKind::Pi,
                dir: "/tmp/second".to_string(),
            },
        ];

        let error = SessionRepository::new(&agents).unwrap_err();

        assert_eq!(
            format!("{error:#}"),
            "agent Pi is configured more than once"
        );
    }

    fn test_agent(kind: AgentKind, file_name: &str, data: &str) -> (PathBuf, Agent) {
        let sequence = NEXT_TEST_DIR.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "his-load-session-{}-{sequence}-{file_name}",
            std::process::id()
        ));
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join(file_name), data).unwrap();
        let agent = Agent {
            kind,
            dir: dir.to_string_lossy().into_owned(),
        };
        (dir, agent)
    }
}
