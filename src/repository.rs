use crate::agent::AgentKind;
use crate::config::AgentConfig;
use crate::session::{SessionDetail, SessionLocator, SessionSummary};
use anyhow::{Context, Result, anyhow, bail};
use std::collections::HashSet;
use std::ffi::OsStr;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

const SESSION_FILE_EXTENSION: &str = "jsonl";
const CLAUDE_SUBAGENT_DIRECTORY: &str = "subagents";

#[derive(Debug)]
pub(crate) struct SessionCatalog {
    pub(crate) sessions: Vec<SessionSummary>,
    pub(crate) warnings: Vec<RepositoryWarning>,
}

impl SessionCatalog {
    pub(crate) fn warning_message(&self) -> Option<String> {
        match self.warnings.len() {
            0 => None,
            count => Some(format!(
                "Skipped {count} unreadable session sources; details were written to stderr."
            )),
        }
    }
}

#[derive(Debug)]
pub(crate) struct RepositoryWarning {
    pub(crate) agent: AgentKind,
    pub(crate) path: PathBuf,
    pub(crate) error: String,
}

impl fmt::Display for RepositoryWarning {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{} session source {}: {}",
            self.agent,
            self.path.display(),
            self.error
        )
    }
}

#[derive(Debug)]
pub(crate) struct SessionRepository {
    agents: Vec<AgentConfig>,
}

impl SessionRepository {
    pub(crate) fn new(agents: Vec<AgentConfig>) -> Result<Self> {
        let mut agent_kinds = HashSet::new();
        for agent in &agents {
            if !agent_kinds.insert(agent.kind) {
                return Err(anyhow!(
                    "agent {:?} is configured more than once",
                    agent.kind
                ));
            }
        }
        Ok(Self { agents })
    }

    pub(crate) fn list_sessions(&self) -> SessionCatalog {
        let mut sessions = Vec::new();
        let mut warnings = Vec::new();

        for agent in &self.agents {
            let discovery = match discover_session_paths(agent) {
                Ok(discovery) => discovery,
                Err(error) => {
                    warnings.push(RepositoryWarning {
                        agent: agent.kind,
                        path: agent.directory.clone(),
                        error: format!("{error:#}"),
                    });
                    continue;
                }
            };
            warnings.extend(discovery.warnings);

            for path in discovery.paths {
                match agent.kind.parse_summary(&path) {
                    Ok(summary) => sessions.push(SessionSummary {
                        id: summary.id,
                        agent: agent.kind,
                        timestamp: summary.timestamp,
                        cwd: summary.cwd,
                        first_message: summary.first_message,
                        locator: SessionLocator::new(path),
                    }),
                    Err(error) => warnings.push(RepositoryWarning {
                        agent: agent.kind,
                        path,
                        error: format!("{error:#}"),
                    }),
                }
            }
        }

        sessions.sort_by(|left, right| {
            right
                .timestamp
                .cmp(&left.timestamp)
                .then_with(|| left.id.cmp(&right.id))
        });
        SessionCatalog { sessions, warnings }
    }

    pub(crate) fn load_session(&self, summary: &SessionSummary) -> Result<SessionDetail> {
        if !self.agents.iter().any(|agent| agent.kind == summary.agent) {
            bail!("agent {:?} is not configured", summary.agent);
        }

        let session = summary
            .agent
            .parse_detail(summary.locator.path())
            .with_context(|| format!("failed to load session {}", summary.id))?;
        if session.id != summary.id {
            bail!(
                "session identifier changed from {} to {} in {}",
                summary.id,
                session.id,
                summary.locator.path().display()
            );
        }

        Ok(SessionDetail {
            agent: summary.agent,
            timestamp: session.timestamp,
            cwd: session.cwd,
            messages: session.messages,
        })
    }
}

struct SessionDiscovery {
    paths: Vec<PathBuf>,
    warnings: Vec<RepositoryWarning>,
}

type DiscoveryEntry = std::result::Result<Option<PathBuf>, RepositoryWarning>;

fn discover_session_paths(agent: &AgentConfig) -> Result<SessionDiscovery> {
    let metadata = fs::metadata(&agent.directory).with_context(|| {
        format!(
            "failed to inspect agent directory {}",
            agent.directory.display()
        )
    })?;
    if !metadata.is_dir() {
        bail!(
            "agent directory {} is not a directory",
            agent.directory.display()
        );
    }

    let entries = WalkDir::new(&agent.directory).into_iter().map(|entry| {
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) => {
                return Err(RepositoryWarning {
                    agent: agent.kind,
                    path: error
                        .path()
                        .unwrap_or(agent.directory.as_path())
                        .to_path_buf(),
                    error: format!("failed to walk agent directory: {error}"),
                });
            }
        };
        let is_session = entry.file_type().is_file()
            && entry.path().extension() == Some(OsStr::new(SESSION_FILE_EXTENSION))
            && is_agent_session_path(agent.kind, entry.path());
        Ok(is_session.then(|| entry.into_path()))
    });
    Ok(collect_discovery(entries))
}

fn is_agent_session_path(agent: AgentKind, path: &Path) -> bool {
    agent != AgentKind::Claude
        || path.parent().and_then(Path::file_name) != Some(OsStr::new(CLAUDE_SUBAGENT_DIRECTORY))
}

fn collect_discovery(entries: impl IntoIterator<Item = DiscoveryEntry>) -> SessionDiscovery {
    let mut paths = Vec::new();
    let mut warnings = Vec::new();
    for entry in entries {
        match entry {
            Ok(Some(path)) => paths.push(path),
            Ok(None) => {}
            Err(warning) => warnings.push(warning),
        }
    }
    paths.sort();
    SessionDiscovery { paths, warnings }
}

#[cfg(test)]
mod tests {
    use super::{
        CLAUDE_SUBAGENT_DIRECTORY, RepositoryWarning, SessionRepository, collect_discovery,
    };
    use crate::agent::AgentKind;
    use crate::config::AgentConfig;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_TEST_DIRECTORY: AtomicU64 = AtomicU64::new(0);
    const CLAUDE_FIXTURE: &str = include_str!("../tests/fixtures/claude/session.jsonl");
    const CLAUDE_SIDECHAIN_FIXTURE: &str = r#"{"type":"assistant","sessionId":"claude-fixture","timestamp":"2026-07-14T01:01:00Z","cwd":"/work/claude-project","isSidechain":true,"message":{"role":"assistant","content":[{"type":"text","text":"Internal subagent output"}]}}"#;
    const CODEX_FIXTURE: &str = include_str!("../tests/fixtures/codex/session.jsonl");
    const PI_FIXTURE: &str = include_str!("../tests/fixtures/pi/session.jsonl");

    #[test]
    fn loads_sanitized_agent_fixtures() {
        let claude_directory = test_directory("claude-fixture");
        let codex_directory = test_directory("codex-fixture");
        let pi_directory = test_directory("pi-fixture");
        let claude_subagent_directory = claude_directory
            .join("project/session-id")
            .join(CLAUDE_SUBAGENT_DIRECTORY);
        fs::create_dir_all(&claude_directory).unwrap();
        fs::create_dir_all(&claude_subagent_directory).unwrap();
        fs::create_dir_all(&codex_directory).unwrap();
        fs::create_dir_all(&pi_directory).unwrap();
        fs::write(claude_directory.join("session.jsonl"), CLAUDE_FIXTURE).unwrap();
        fs::write(
            claude_subagent_directory.join("agent-id.jsonl"),
            CLAUDE_SIDECHAIN_FIXTURE,
        )
        .unwrap();
        fs::write(codex_directory.join("session.jsonl"), CODEX_FIXTURE).unwrap();
        fs::write(pi_directory.join("session.jsonl"), PI_FIXTURE).unwrap();
        let repository = SessionRepository::new(vec![
            AgentConfig {
                kind: AgentKind::Claude,
                directory: claude_directory.clone(),
            },
            AgentConfig {
                kind: AgentKind::Codex,
                directory: codex_directory.clone(),
            },
            AgentConfig {
                kind: AgentKind::Pi,
                directory: pi_directory.clone(),
            },
        ])
        .unwrap();

        let catalog = repository.list_sessions();
        let claude = catalog
            .sessions
            .iter()
            .find(|session| session.agent == AgentKind::Claude)
            .unwrap();
        let codex = catalog
            .sessions
            .iter()
            .find(|session| session.agent == AgentKind::Codex)
            .unwrap();
        let detail = repository.load_session(codex).unwrap();
        let claude_detail = repository.load_session(claude).unwrap();

        assert_eq!(catalog.sessions.len(), 3);
        assert!(catalog.warnings.is_empty());
        assert_eq!(catalog.sessions[0].id, "claude-fixture");
        assert_eq!(claude.first_message, "Inspect the Claude Code fixture");
        assert_eq!(claude_detail.messages.len(), 4);
        assert_eq!(codex.first_message, "Inspect the Codex fixture");
        assert_eq!(detail.messages.len(), 2);
        fs::remove_dir_all(claude_directory).unwrap();
        fs::remove_dir_all(codex_directory).unwrap();
        fs::remove_dir_all(pi_directory).unwrap();
    }

    #[test]
    fn discovers_jsonl_files_recursively_and_reports_malformed_files() {
        let directory = test_directory("discovery");
        let nested = directory.join("nested.jsonl");
        fs::create_dir_all(&nested).unwrap();
        fs::write(
            directory.join("valid.jsonl"),
            pi_session("valid", "2026-07-13T01:00:00Z"),
        )
        .unwrap();
        fs::write(nested.join("broken.jsonl"), "not json").unwrap();
        for name in ["notes.txt", "uppercase.JSONL", "backup.jsonl.bak"] {
            fs::write(nested.join(name), "ignored").unwrap();
        }
        let repository = repository(AgentKind::Pi, &directory);

        let catalog = repository.list_sessions();

        assert_eq!(catalog.sessions.len(), 1);
        assert_eq!(catalog.sessions[0].id, "valid");
        assert_eq!(catalog.warnings.len(), 1);
        assert!(catalog.warnings[0].path.ends_with("broken.jsonl"));
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn returns_directory_failures_as_warnings() {
        let directory = test_directory("missing");
        let repository = repository(AgentKind::Pi, &directory);

        let catalog = repository.list_sessions();

        assert!(catalog.sessions.is_empty());
        assert_eq!(catalog.warnings.len(), 1);
        assert!(
            catalog.warnings[0]
                .error
                .contains("failed to inspect agent directory")
        );
    }

    #[test]
    fn retains_healthy_paths_when_a_walk_entry_fails() {
        let healthy = PathBuf::from("/sessions/healthy.jsonl");
        let warning = RepositoryWarning {
            agent: AgentKind::Codex,
            path: PathBuf::from("/sessions/unreadable"),
            error: "permission denied".to_string(),
        };

        let discovery = collect_discovery([Ok(Some(healthy.clone())), Err(warning), Ok(None)]);

        assert_eq!(discovery.paths, [healthy]);
        assert_eq!(discovery.warnings.len(), 1);
        assert_eq!(
            discovery.warnings[0].path,
            PathBuf::from("/sessions/unreadable")
        );
    }

    #[test]
    fn lists_newest_sessions_first_and_loads_the_selected_path() {
        let directory = test_directory("load");
        fs::create_dir_all(&directory).unwrap();
        fs::write(
            directory.join("older.jsonl"),
            pi_session_with_message("older", "2026-07-12T01:00:00Z", "Older"),
        )
        .unwrap();
        fs::write(
            directory.join("newer.jsonl"),
            pi_session_with_message("newer", "2026-07-13T01:00:00Z", "Newer"),
        )
        .unwrap();
        let repository = repository(AgentKind::Pi, &directory);

        let catalog = repository.list_sessions();
        let detail = repository.load_session(&catalog.sessions[1]).unwrap();

        assert_eq!(catalog.sessions[0].id, "newer");
        assert_eq!(catalog.sessions[1].id, "older");
        assert_eq!(detail.messages[0].text, "Older");
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn lists_from_early_metadata_without_parsing_trailing_detail_records() {
        let directory = test_directory("summary-only");
        fs::create_dir_all(&directory).unwrap();
        fs::write(
            directory.join("session.jsonl"),
            format!(
                "{}\n{}\nnot json",
                pi_session("summary", "2026-07-13T01:00:00Z"),
                r#"{"type":"message","id":"user","timestamp":"2026-07-13T01:01:00Z","message":{"role":"user","content":[{"type":"text","text":"Visible summary"}]}}"#
            ),
        )
        .unwrap();
        let repository = repository(AgentKind::Pi, &directory);

        let catalog = repository.list_sessions();
        let error = repository.load_session(&catalog.sessions[0]).unwrap_err();

        assert_eq!(catalog.sessions[0].first_message, "Visible summary");
        assert!(catalog.warnings.is_empty());
        assert!(format!("{error:#}").contains("failed to parse session file"));
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn rejects_duplicate_agent_kinds() {
        let agents = vec![
            AgentConfig {
                kind: AgentKind::Pi,
                directory: PathBuf::from("/tmp/first"),
            },
            AgentConfig {
                kind: AgentKind::Pi,
                directory: PathBuf::from("/tmp/second"),
            },
        ];

        let error = SessionRepository::new(agents).unwrap_err();

        assert_eq!(error.to_string(), "agent Pi is configured more than once");
    }

    fn repository(kind: AgentKind, directory: &Path) -> SessionRepository {
        SessionRepository::new(vec![AgentConfig {
            kind,
            directory: directory.to_path_buf(),
        }])
        .unwrap()
    }

    fn pi_session(id: &str, timestamp: &str) -> String {
        format!(
            r#"{{"type":"session","version":3,"id":"{id}","timestamp":"{timestamp}","cwd":"/tmp/{id}"}}"#
        )
    }

    fn pi_session_with_message(id: &str, timestamp: &str, text: &str) -> String {
        format!(
            "{}\n{}",
            pi_session(id, timestamp),
            format_args!(
                r#"{{"type":"message","id":"user-{id}","timestamp":"{timestamp}","message":{{"role":"user","content":[{{"type":"text","text":"{text}"}}]}}}}"#
            )
        )
    }

    fn test_directory(name: &str) -> PathBuf {
        let sequence = NEXT_TEST_DIRECTORY.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!(
            "his-repository-test-{}-{sequence}-{name}",
            std::process::id()
        ))
    }
}
