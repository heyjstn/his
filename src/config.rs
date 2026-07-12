use crate::agent::provider::Provider;
use anyhow::{Context, Result};
use serde::Deserialize;
use std::fs;
use std::path::Path;

#[derive(Deserialize, Debug)]
pub(crate) struct Config {
    pub(crate) providers: Option<Vec<Provider>>,
}

pub(crate) fn load(path: impl AsRef<Path>) -> Result<Config> {
    let path = path.as_ref();
    let data = fs::read_to_string(path)
        .with_context(|| format!("failed to read config from {}", path.display()))?;
    from_json(&data).with_context(|| format!("failed to load config from {}", path.display()))
}

fn from_json(data: &str) -> Result<Config> {
    serde_json::from_str(data).context("failed to parse config")
}

#[cfg(test)]
mod tests {
    use super::from_json;

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

        let config = from_json(json).unwrap();

        assert_eq!(config.providers.unwrap().len(), 1);
    }

    #[test]
    fn defaults_to_no_providers() {
        let config = from_json("{}").unwrap();

        assert!(config.providers.is_none());
    }

    #[test]
    fn accepts_null_providers() {
        let config = from_json(r#"{"providers":null}"#).unwrap();

        assert!(config.providers.is_none());
    }

    #[test]
    fn parse_error_has_context() {
        let error = from_json("not json").unwrap_err();

        assert!(format!("{error:#}").starts_with("failed to parse config"));
    }
}
