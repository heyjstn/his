pub mod codex;
pub mod pi;

use crate::RuntimeErr;
use crate::agent::session::{
    Session, list_sessions as list_sessions_impl, load_session as load_session_impl,
};
use serde::Deserialize;
use serde::de::DeserializeOwned;
use std::fs;
use std::fs::DirEntry;
use std::io;

pub use codex::CodexMessage;
pub use pi::PiMessage;

#[derive(Deserialize, Debug)]
#[serde(rename_all = "lowercase")]
pub enum ProviderEnum {
    Codex,
    Pi,
}

impl ProviderEnum {
    pub(crate) fn clone(&self) -> ProviderEnum {
        match self {
            &ProviderEnum::Pi => ProviderEnum::Pi,
            &ProviderEnum::Codex => ProviderEnum::Codex,
        }
    }
}

#[derive(Deserialize, Debug)]
pub struct Provider {
    pub name: ProviderEnum,
    pub dir: String,
}

pub trait FromProviderMessage: Sized + DeserializeOwned {
    fn from_message_str(s: &str) -> Result<AgentMessage, RuntimeErr>
    where
        AgentMessage: From<Self>,
    {
        let original_data =
            serde_json::from_str::<Self>(s).map_err(|err| RuntimeErr::Generic(err.to_string()))?;
        Ok(original_data.into())
    }

    fn read_to_string(path: &str) -> Result<String, RuntimeErr> {
        fs::read_to_string(path).map_err(|err| RuntimeErr::Generic(err.to_string()))
    }

    fn parse_vec(path: &str) -> Result<Vec<AgentMessage>, RuntimeErr>
    where
        AgentMessage: From<Self>,
    {
        let file = Self::read_to_string(path)?;
        file.lines()
            .map(|line| Self::from_message_str(line))
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
    pub fn list_sessions(&self) -> Vec<Session> {
        list_sessions_impl(self)
    }

    pub fn load_session(self, session_id: String) -> Session {
        load_session_impl(&self, session_id)
    }
}

pub(crate) fn walk_dir(dir: &String) -> Result<Vec<String>, io::Error> {
    let cur = fs::read_dir(dir)?;

    let list: Vec<Result<DirEntry, io::Error>> = cur.collect();

    let mut file_paths: Vec<String> = vec![];
    for res_file in list {
        let file = res_file?;
        let typ = file.file_type()?;
        let path = file.path().to_str().unwrap().to_string();
        if typ.is_file() {
            file_paths.push(file.path().to_str().unwrap().to_string())
        } else if typ.is_dir() {
            file_paths.extend(walk_dir(&path)?)
        }
    }

    Ok(file_paths)
}

#[cfg(test)]
mod tests {
    use crate::agent::provider::{Provider, ProviderEnum, walk_dir};
    use std::env;

    #[test]
    fn test_list_sessions_1() {
        let cwd = env::current_dir().unwrap().to_str().unwrap().to_string();
        let dir = format!("{}/tests/.codex/sessions", cwd);

        let provider = Provider {
            name: ProviderEnum::Codex,
            dir,
        };

        println!("{:?}", walk_dir(&format!("{}/tests/.codex/sessions", cwd)));

        let sessions = provider.list_sessions();
        assert!(sessions.is_empty());
    }

    #[test]
    fn test_list_sessions_2() {
        let cwd = env::current_dir().unwrap().to_str().unwrap().to_string();
        let dir = format!("{}/tests/.pi/agent/sessions", cwd);

        let provider = Provider {
            name: ProviderEnum::Pi,
            dir,
        };

        println!("{:?}", provider.list_sessions());
        // assert!(!provider.list_sessions().is_empty());
    }
}
