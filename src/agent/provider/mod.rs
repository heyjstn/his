pub mod codex;
pub mod pi;

use crate::agent::session::{
    Session, list_sessions as list_sessions_impl, load_session as load_session_impl,
};
use anyhow::{Context, Result};
use serde::Deserialize;
use serde::de::DeserializeOwned;
use std::fs;
use std::path::{Path, PathBuf};

pub use codex::CodexMessage;
pub use pi::PiMessage;

#[derive(Clone, Copy, Deserialize, Debug, Eq, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum ProviderEnum {
    Codex,
    Pi,
}

#[derive(Deserialize, Debug)]
pub struct Provider {
    pub name: ProviderEnum,
    pub dir: String,
}

pub trait FromProviderMessage: Sized + DeserializeOwned {
    fn from_message_str(s: &str) -> Result<AgentMessage>
    where
        AgentMessage: From<Self>,
    {
        let original_data =
            serde_json::from_str::<Self>(s).context("failed to parse provider message as JSON")?;
        Ok(original_data.into())
    }

    fn read_to_string(path: &Path) -> Result<String> {
        fs::read_to_string(path)
            .with_context(|| format!("failed to read session file {}", path.display()))
    }

    fn parse_vec(path: &Path) -> Result<Vec<AgentMessage>>
    where
        AgentMessage: From<Self>,
    {
        let file = Self::read_to_string(path)?;
        serde_json::Deserializer::from_str(&file)
            .into_iter::<Self>()
            .map(|message| {
                message
                    .map(Into::into)
                    .with_context(|| format!("failed to parse session file {}", path.display()))
            })
            .collect()
    }
}

/// Generic message for all coding agents message
#[derive(Deserialize, Debug)]
pub struct AgentMessage {
    #[serde(rename = "type")]
    pub typ: String,
    pub id: String,
    pub parent_id: Option<String>,
    pub timestamp: String,
    pub cwd: Option<String>,
    pub role: Option<String>,
    pub text: Option<String>,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub tool_call_id: Option<String>,
    pub tool_name: Option<String>,
    pub is_error: Option<bool>,
}

impl FromProviderMessage for AgentMessage {}

impl Provider {
    pub fn list_sessions(&self) -> Result<Vec<Session>> {
        list_sessions_impl(self)
    }

    pub fn load_session(&self, session_id: String) -> Result<Session> {
        load_session_impl(self, session_id)
    }
}

pub(crate) fn walk_dir(dir: impl AsRef<Path>) -> Result<Vec<PathBuf>> {
    let dir = dir.as_ref();
    let entries = fs::read_dir(dir)
        .with_context(|| format!("failed to read provider directory {}", dir.display()))?;

    let mut file_paths = vec![];
    for entry in entries {
        let file =
            entry.with_context(|| format!("failed to read an entry in {}", dir.display()))?;
        let typ = file
            .file_type()
            .with_context(|| format!("failed to inspect {}", file.path().display()))?;
        let path = file.path();
        if typ.is_file() {
            file_paths.push(path)
        } else if typ.is_dir() {
            file_paths.extend(walk_dir(&path)?)
        }
    }

    Ok(file_paths)
}
