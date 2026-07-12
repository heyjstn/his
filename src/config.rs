use crate::agent::provider::{Provider, ProviderEnum};
use crate::agent::session::Session;
use anyhow::{Context, Result, anyhow};
use serde::Deserialize;
use std::fs;
use std::path::Path;

#[derive(Deserialize, Debug)]
pub(crate) struct Config {
    providers: Option<Vec<Provider>>,
}

impl Config {
    pub(crate) fn new(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let data = fs::read_to_string(path)
            .with_context(|| format!("failed to read config from {}", path.display()))?;
        Self::from_json(&data)
            .with_context(|| format!("failed to load config from {}", path.display()))
    }

    fn from_json(data: &str) -> Result<Self> {
        serde_json::from_str(data).context("failed to parse config")
    }

    pub(crate) fn list_sessions(&self) -> Result<Vec<Session>> {
        let Some(providers) = self.providers.as_ref() else {
            return Ok(Vec::new());
        };

        let mut sessions = Vec::new();
        for provider in providers {
            sessions.extend(provider.list_sessions()?);
        }
        sessions.sort_by_key(|session| session.ts.clone());
        Ok(sessions)
    }

    pub(crate) fn load_session(
        &self,
        provider_name: ProviderEnum,
        session_id: String,
    ) -> Result<Session> {
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

#[cfg(test)]
mod tests {
    use super::Config;

    #[test]
    fn loads_config() {
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
    fn parse_error_has_context() {
        let error = Config::from_json("not json").unwrap_err();

        assert!(format!("{error:#}").starts_with("failed to parse config"));
    }
}
