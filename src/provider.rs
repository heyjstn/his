use serde::Deserialize;
use std::any::Any;
use std::fs;
use std::fs::{DirEntry, File};
use std::io::Error;

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

impl Provider {
    pub fn list_sessions(&self) -> Vec<Session> {
        match self.name {
            ProviderEnum::Pi => {
                let session_dir = format!("{}/sessions", &self.dir);
                walk_dir(&session_dir);
            }
            ProviderEnum::Codex => {}
        }
        vec![]
    }

    pub fn load_session(self, session_id: String) -> Session {
        todo!()
    }
}

fn walk_dir(dir: &String) -> Result<Vec<DirEntry>, Error> {
    let cur = fs::read_dir(dir)?;

    let list: Vec<Result<DirEntry, Error>> = cur.collect();

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
    use std::{env, fs};

    #[test]
    fn test_list_sessions() {
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
}
