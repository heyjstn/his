pub mod agent;
pub mod cli;
pub mod tui;

use crate::cli::run;
use agent::provider::{Provider, ProviderEnum};
use agent::session::Session;
use anyhow::{Context, Result, anyhow};
use serde::Deserialize;
use std::fs;
use std::path::Path;
use std::process::ExitCode;

// pub const DEFAULT_CONFIG_DIR: &str = "$HOME/.his";

fn main() -> Result<ExitCode> {
    run()
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

    #[test]
    fn test_config_parse_error_has_context() {
        let error = Config::from_json("not json").unwrap_err();

        assert!(format!("{error:#}").starts_with("failed to parse config"));
    }
}

fn list_sessions(config: &Config) -> Result<()> {
    println!("{:?}", config.list_sessions()?);
    Ok(())
}

#[derive(Deserialize, Debug)]
struct Config {
    providers: Option<Vec<Provider>>,
}

impl Config {
    fn new(dir: impl AsRef<Path>) -> Result<Config> {
        let dir = dir.as_ref();
        let data = fs::read_to_string(dir)
            .with_context(|| format!("failed to read config from {}", dir.display()))?;
        Self::from_json(&data)
            .with_context(|| format!("failed to load config from {}", dir.display()))
    }

    fn from_json(data: &str) -> Result<Config> {
        let config: Config = serde_json::from_str(data).context("failed to parse config")?;
        Ok(config)
    }

    fn list_sessions(&self) -> Result<Vec<Session>> {
        match self.providers.as_ref() {
            Some(providers) => {
                let mut sessions = Vec::new();
                for provider in providers {
                    sessions.extend(provider.list_sessions()?);
                }
                sessions.sort_by_key(|s| s.ts.clone());
                Ok(sessions)
            }
            None => Ok(vec![]),
        }
    }

    fn load_session(&self, provider_name: ProviderEnum, session_id: String) -> Result<Session> {
        let provider = self
            .providers
            .as_ref()
            .and_then(|providers| {
                providers
                    .iter()
                    .find(|provider| provider.name == provider_name)
            })
            .ok_or_else(|| anyhow!("provider {provider_name:?} is not configured"))?;

        provider
            .load_session(session_id.clone())
            .with_context(|| format!("failed to load session {session_id}"))
    }
}
