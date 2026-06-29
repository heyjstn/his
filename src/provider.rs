use crate::RuntimeErr;
use serde::de::DeserializeOwned;
use serde::Deserialize;
use std::fs;
use std::fs::DirEntry;
use std::io;
use std::str::FromStr;

#[derive(Deserialize, Debug)]
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

pub trait AgentMessage: Sized + DeserializeOwned {
    fn from_message_str(s: &str) -> Result<Self, RuntimeErr> {
        serde_json::from_str(s).map_err(|err| RuntimeErr::Generic(err.to_string()))
    }

    fn read_to_string(path: &str) -> Result<String, RuntimeErr> {
        fs::read_to_string(path).map_err(|err| RuntimeErr::Generic(err.to_string()))
    }

    fn parse_one(path: &str) -> Result<Self, RuntimeErr> {
        let file = Self::read_to_string(path)?;
        Ok(Self::from_message_str(&file)?)
    }

    fn parse_vec(path: &str) -> Result<Vec<Self>, RuntimeErr> {
        let file = Self::read_to_string(path)?;
        file.lines()
            .map(|line| Self::from_message_str(line))
            .collect()
    }
}

#[derive(Deserialize, Debug)]
pub struct PiMessage {
    #[serde(rename = "type")]
    pub typ: String,
    pub version: Option<usize>,
    pub id: String,
    pub timestamp: String,
    pub cwd: Option<String>,
}

#[derive(Deserialize, Debug)]
pub struct CodexMessage {}

impl AgentMessage for PiMessage {}

impl AgentMessage for CodexMessage {}

impl Provider {
    pub fn list_sessions(&self) -> Vec<Session> {
        match self.name {
            ProviderEnum::Pi => {
                let dirs = walk_dir(&self.dir).unwrap();
                let sessions: Vec<Session> = dirs
                    .iter()
                    .map(|dir| {
                        let data: Vec<PiMessage> =
                            PiMessage::parse_vec(dir.path().as_path().to_str().unwrap()).unwrap();
                        let initialized_message = data.get(0).unwrap();
                        Session {
                            id: initialized_message.id.clone(),
                            provider: ProviderEnum::Pi,
                            ts: initialized_message.timestamp.clone(),
                            cwd: initialized_message.cwd.clone().unwrap(),
                            messages: None,
                        }
                    })
                    .collect();
                return sessions;
            }
            ProviderEnum::Codex => {}
        }
        vec![]
    }

    pub fn load_session(self, session_id: String) -> Session {
        todo!()
    }
}

fn walk_dir(dir: &String) -> Result<Vec<DirEntry>, io::Error> {
    let cur = fs::read_dir(dir)?;

    let list: Vec<Result<DirEntry, io::Error>> = cur.collect();

    let mut file_paths: Vec<DirEntry> = vec![];
    for res_file in list {
        let file = res_file?;
        let typ = file.file_type()?;
        let path = file.path().to_str().unwrap().to_string();
        if typ.is_file() {
            file_paths.push(file)
        } else if typ.is_dir() {
            file_paths.extend(walk_dir(&path)?)
        }
    }

    Ok(file_paths)
}

#[cfg(test)]
mod tests {
    use crate::provider::{walk_dir, Provider, ProviderEnum};
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
