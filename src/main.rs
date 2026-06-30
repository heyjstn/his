pub mod agent;
pub mod cli;
pub mod tui;

use crate::cli::run;
use agent::provider::Provider;
use agent::session::Session;
use serde::Deserialize;
use std::fs;
use std::process::ExitCode;

#[derive(Debug)]
pub enum RuntimeErr {
    InvalidNumArgs,
    UnsupportedCommand,
    InvalidConfigDir,
    Generic(String),
}

// pub const DEFAULT_CONFIG_DIR: &str = "$HOME/.his";

fn main() -> Result<ExitCode, RuntimeErr> {
    Ok(run())
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
    println!("{:?}", config.list_sessions());
    Ok(())
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

    fn list_sessions(&self) -> Vec<Session> {
        match self.providers.as_ref() {
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
