pub mod agent;

use agent::provider::Provider;
use agent::session::Session;
use serde::Deserialize;
use std::env::args_os;
use std::fs;
use std::process::ExitCode;

#[derive(Debug)]
pub enum RuntimeErr {
    InvalidNumArgs,
    UnsupportedCommand,
    InvalidConfigDir,
    Generic(String),
}

#[derive(PartialEq)]
enum Command {
    ListSession,
    Version,
}

const DEFAULT_CONFIG_DIR: &str = "$HOME/.his";

fn main() -> Result<ExitCode, RuntimeErr> {
    let mut args = args_os().skip(1).peekable();

    let cmd = args
        .find_map(|arg| {
            Some(match arg.to_str()? {
                "ls" => Command::ListSession,
                "version" => Command::Version,
                _ => return None,
            })
        })
        .ok_or(RuntimeErr::UnsupportedCommand)?;

    let config = Config::new(DEFAULT_CONFIG_DIR.to_string())?;

    match cmd {
        Command::ListSession => list_sessions(&config)?,
        Command::Version => todo!(),
    }

    Ok(ExitCode::SUCCESS)
}

#[cfg(test)]
mod tests {
    use crate::Config;

    #[test]
    fn test_load_config_ok() {
        let json = r#"
            {
                "providers": [
                    {
                        "name": "codex",
                        "dir": "$HOME/.codex"
                    }
                ]
            }
            "#;
        let config = Config::from_json(json).unwrap();
        assert_eq!(config.providers.unwrap().len(), 1);
    }
}

fn list_sessions(config: &Config) -> Result<(), RuntimeErr> {
    todo!()
}

#[derive(Deserialize, Debug)]
struct Config {
    providers: Option<Vec<Provider>>,
}

impl Config {
    fn new(dir: String) -> Result<Config, RuntimeErr> {
        let data = fs::read_to_string(dir).map_err(|err| RuntimeErr::Generic(err.to_string()))?;
        Self::from_json(&data)
    }

    fn from_json(data: &str) -> Result<Config, RuntimeErr> {
        let config: Config =
            serde_json::from_str(&data).map_err(|err| RuntimeErr::Generic(err.to_string()))?;
        Ok(config)
    }

    fn list_sessions(self) -> Vec<Session> {
        match self.providers {
            Some(providers) => {
                let mut sessions: Vec<Session> = providers
                    .iter()
                    .flat_map(|provider| provider.list_sessions())
                    .collect();
                sessions.sort_by_key(|s| s.ts.clone());
                sessions
            }
            None => vec![],
        }
    }
}
