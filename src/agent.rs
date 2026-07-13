mod codex;
mod pi;

use anyhow::{Context, Result, bail};
use serde::Deserialize;
use serde::de::DeserializeOwned;
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

pub use codex::CodexMessage;
pub use pi::PiMessage;

pub(crate) const COMMENTARY_PHASE: &str = "commentary";
pub(crate) const TOOL_CALL_PHASE: &str = "tool_call";
const SESSION_FILE_EXTENSION: &str = "jsonl";

#[derive(Clone, Copy, Deserialize, Debug, Eq, Hash, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum AgentKind {
    Codex,
    Pi,
}

#[derive(Deserialize, Debug)]
pub struct Agent {
    pub kind: AgentKind,
    pub dir: String,
}

impl Agent {
    pub(crate) fn get_session_paths(&self) -> Result<Vec<PathBuf>> {
        let metadata = fs::metadata(&self.dir)
            .with_context(|| format!("failed to inspect agent directory {}", self.dir))?;
        if !metadata.is_dir() {
            bail!("agent directory {} is not a directory", self.dir);
        }

        let mut paths = Vec::new();
        for entry in WalkDir::new(&self.dir) {
            let entry =
                entry.with_context(|| format!("failed to walk agent directory {}", self.dir))?;
            if !entry.file_type().is_file()
                || entry.path().extension() != Some(OsStr::new(SESSION_FILE_EXTENSION))
            {
                continue;
            }
            paths.push(entry.into_path());
        }
        Ok(paths)
    }

    pub(crate) fn parse(&self, path: &Path) -> Result<Vec<Message>> {
        match self.kind {
            AgentKind::Codex => CodexMessage::parse(path),
            AgentKind::Pi => PiMessage::parse(path),
        }
    }
}

pub trait SessionRecord: Sized + DeserializeOwned {
    fn into_messages(self) -> Vec<Message>
    where
        Message: From<Self>,
    {
        vec![self.into()]
    }

    fn from_message_str(s: &str) -> Result<Message>
    where
        Message: From<Self>,
    {
        let original_data =
            serde_json::from_str::<Self>(s).context("failed to parse agent message as JSON")?;
        Ok(original_data.into())
    }

    fn read_to_string(path: &Path) -> Result<String> {
        fs::read_to_string(path)
            .with_context(|| format!("failed to read session file {}", path.display()))
    }

    fn parse(path: &Path) -> Result<Vec<Message>>
    where
        Message: From<Self>,
    {
        let file = Self::read_to_string(path)?;
        let mut converted = Vec::new();
        for message in serde_json::Deserializer::from_str(&file).into_iter::<Self>() {
            let message = message
                .with_context(|| format!("failed to parse session file {}", path.display()))?;
            converted.extend(message.into_messages());
        }
        Ok(converted)
    }
}

/// Generic message for all coding agents.
#[derive(Clone, Deserialize, Debug)]
pub struct Message {
    #[serde(rename = "type")]
    pub typ: String,
    pub id: String,
    pub parent_id: Option<String>,
    pub timestamp: String,
    pub cwd: Option<String>,
    pub role: Option<String>,
    pub text: Option<String>,
    pub phase: Option<String>,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub tool_call_id: Option<String>,
    pub tool_name: Option<String>,
    pub tool_path: Option<String>,
    #[serde(default)]
    pub tool_contents: Vec<String>,
    pub is_error: Option<bool>,
}

impl SessionRecord for Message {}

#[cfg(test)]
mod tests {
    use super::{Agent, AgentKind};
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_TEST_DIR: AtomicU64 = AtomicU64::new(0);

    #[test]
    fn finds_only_jsonl_session_files_recursively() {
        let dir = test_path("recursive");
        let nested_dir = dir.join("nested.jsonl");
        fs::create_dir_all(&nested_dir).unwrap();
        let root_file = dir.join("root.jsonl");
        let nested_file = nested_dir.join("nested.jsonl");
        fs::write(&root_file, "").unwrap();
        fs::write(&nested_file, "").unwrap();
        for file_name in ["notes.txt", "uppercase.JSONL", "backup.jsonl.bak", "README"] {
            fs::write(nested_dir.join(file_name), "").unwrap();
        }
        let agent = agent(&dir);

        let mut paths = agent.get_session_paths().unwrap();
        paths.sort();
        let mut expected = vec![root_file, nested_file];
        expected.sort();

        assert_eq!(paths, expected);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn rejects_a_missing_session_directory() {
        let dir = test_path("missing");
        let agent = agent(&dir);

        let error = agent.get_session_paths().unwrap_err();

        assert!(
            format!("{error:#}").starts_with("failed to inspect agent directory"),
            "unexpected error: {error:#}"
        );
    }

    #[test]
    fn rejects_a_file_as_the_session_directory() {
        let path = test_path("file");
        fs::write(&path, "").unwrap();
        let agent = agent(&path);

        let error = agent.get_session_paths().unwrap_err();

        assert_eq!(
            error.to_string(),
            format!("agent directory {} is not a directory", path.display())
        );
        fs::remove_file(path).unwrap();
    }

    fn agent(dir: &std::path::Path) -> Agent {
        Agent {
            kind: AgentKind::Codex,
            dir: dir.to_string_lossy().into_owned(),
        }
    }

    fn test_path(name: &str) -> PathBuf {
        let sequence = NEXT_TEST_DIR.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!(
            "his-agent-{}-{sequence}-{name}",
            std::process::id()
        ))
    }
}
