use crate::agent::provider::Provider;
use anyhow::{Context, Result};
use serde::Deserialize;
use std::fs;
use std::path::Path;

const CONFIG_FILE_NAME: &str = "config.toml";

#[derive(Deserialize, Debug)]
pub(crate) struct Config {
    pub(crate) providers: Option<Vec<Provider>>,
}

pub(crate) fn load(directory: impl AsRef<Path>) -> Result<Config> {
    let path = directory.as_ref().join(CONFIG_FILE_NAME);
    let data = fs::read_to_string(&path)
        .with_context(|| format!("failed to read config from {}", path.display()))?;
    from_toml(&data).with_context(|| format!("failed to load config from {}", path.display()))
}

fn from_toml(data: &str) -> Result<Config> {
    toml::from_str(data).context("failed to parse config")
}

#[cfg(test)]
mod tests {
    use super::{CONFIG_FILE_NAME, from_toml, load};
    use std::fs;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static NEXT_TEST_DIRECTORY: AtomicUsize = AtomicUsize::new(0);

    #[test]
    fn loads_config_from_config_toml() {
        let directory = test_directory();
        let data = r#"
            [[providers]]
            name = "codex"
            dir = "$HOME/.codex"
        "#;
        fs::write(directory.join(CONFIG_FILE_NAME), data).unwrap();

        let config = load(&directory).unwrap();

        assert_eq!(config.providers.unwrap().len(), 1);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn defaults_to_no_providers() {
        let config = from_toml("").unwrap();

        assert!(config.providers.is_none());
    }

    #[test]
    fn parse_error_includes_config_path() {
        let directory = test_directory();
        let path = directory.join(CONFIG_FILE_NAME);
        fs::write(&path, "not toml").unwrap();

        let error = load(&directory).unwrap_err();

        assert!(format!("{error:#}").starts_with(&format!(
            "failed to load config from {}: failed to parse config",
            path.display()
        )));
        fs::remove_dir_all(directory).unwrap();
    }

    fn test_directory() -> std::path::PathBuf {
        let sequence = NEXT_TEST_DIRECTORY.fetch_add(1, Ordering::Relaxed);
        let directory =
            std::env::temp_dir().join(format!("his-config-test-{}-{sequence}", std::process::id()));
        fs::create_dir_all(&directory).unwrap();
        directory
    }
}
