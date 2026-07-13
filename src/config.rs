use crate::agent::Agent;
use anyhow::{Context, Result};
use serde::Deserialize;
use std::env::{self, VarError};
use std::fs;
use std::path::Path;

const CONFIG_FILE_NAME: &str = "config.toml";

#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub(crate) struct Config {
    pub(crate) agents: Option<Vec<Agent>>,
}

pub(crate) fn load(directory: impl AsRef<Path>) -> Result<Config> {
    let path = directory.as_ref().join(CONFIG_FILE_NAME);
    let data = fs::read_to_string(&path)
        .with_context(|| format!("failed to read config from {}", path.display()))?;
    from_toml(&data).with_context(|| format!("failed to load config from {}", path.display()))
}

fn from_toml(data: &str) -> Result<Config> {
    from_toml_with_environment(data, |variable| env::var(variable))
}

fn from_toml_with_environment(
    data: &str,
    mut environment: impl FnMut(&str) -> std::result::Result<String, VarError>,
) -> Result<Config> {
    let mut config: Config = toml::from_str(data).context("failed to parse config")?;
    for agent in config.agents.iter_mut().flatten() {
        agent.dir =
            shellexpand::env_with_context(&agent.dir, |variable| environment(variable).map(Some))
                .with_context(|| {
                    format!(
                        "failed to resolve environment variables in agent directory {:?}",
                        agent.dir
                    )
                })?
                .into_owned();
    }
    Ok(config)
}

#[cfg(test)]
mod tests {
    use super::{CONFIG_FILE_NAME, from_toml, from_toml_with_environment, load};
    use std::env::VarError;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static NEXT_TEST_DIRECTORY: AtomicUsize = AtomicUsize::new(0);

    #[test]
    fn loads_agents_from_config_toml() {
        let directory = test_directory();
        let data = r#"
            [[agents]]
            kind = "codex"
            dir = "/tmp/.codex"
        "#;
        fs::write(directory.join(CONFIG_FILE_NAME), data).unwrap();

        let config = load(&directory).unwrap();

        let agents = config.agents.unwrap();
        assert_eq!(agents.len(), 1);
        assert_eq!(agents[0].dir, "/tmp/.codex");
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn defaults_to_no_agents() {
        let config = from_toml("").unwrap();

        assert!(config.agents.is_none());
    }

    #[test]
    fn resolves_environment_variables_in_agent_directories() {
        const TEST_HOME: &str = "/home/test-user";
        const TEST_PWD: &str = "/work/his";

        let config = from_toml_with_environment(
            r#"
                [[agents]]
                kind = "pi"
                dir = "$PWD/tests/.pi/agent/sessions"

                [[agents]]
                kind = "codex"
                dir = "$HOME/.codex/sessions"
            "#,
            |variable| match variable {
                "HOME" => Ok(TEST_HOME.to_owned()),
                "PWD" => Ok(TEST_PWD.to_owned()),
                _ => Err(VarError::NotPresent),
            },
        )
        .unwrap();

        let agents = config.agents.unwrap();
        assert_eq!(
            PathBuf::from(&agents[0].dir),
            PathBuf::from(TEST_PWD).join("tests/.pi/agent/sessions")
        );
        assert_eq!(
            PathBuf::from(&agents[1].dir),
            PathBuf::from(TEST_HOME).join(".codex/sessions")
        );
    }

    #[test]
    fn rejects_undefined_environment_variables_in_agent_directories() {
        let error = from_toml_with_environment(
            r#"
                [[agents]]
                kind = "pi"
                dir = "$HIS_CONFIG_TEST_UNDEFINED_AGENT_DIRECTORY"
            "#,
            |_| Err(VarError::NotPresent),
        )
        .unwrap_err();

        assert!(format!("{error:#}").starts_with(
            "failed to resolve environment variables in agent directory \"$HIS_CONFIG_TEST_UNDEFINED_AGENT_DIRECTORY\""
        ));
    }

    #[test]
    fn rejects_legacy_providers_config() {
        let error = from_toml(
            r#"
                [[providers]]
                name = "codex"
                dir = "/tmp/.codex"
            "#,
        )
        .unwrap_err();

        assert!(format!("{error:#}").contains("unknown field `providers`"));
    }

    #[test]
    fn resolves_environment_variables_in_provider_directories() {
        const TEST_HOME: &str = "/home/test-user";
        const TEST_PWD: &str = "/work/his";

        let config = from_toml_with_environment(
            r#"
                [[providers]]
                name = "pi"
                dir = "$PWD/tests/.pi/agent/sessions"

                [[providers]]
                name = "codex"
                dir = "$HOME/.codex/sessions"
            "#,
            |variable| match variable {
                "HOME" => Ok(TEST_HOME.to_owned()),
                "PWD" => Ok(TEST_PWD.to_owned()),
                _ => Err(VarError::NotPresent),
            },
        )
        .unwrap();

        let providers = config.providers.unwrap();
        assert_eq!(
            PathBuf::from(&providers[0].dir),
            PathBuf::from(TEST_PWD).join("tests/.pi/agent/sessions")
        );
        assert_eq!(
            PathBuf::from(&providers[1].dir),
            PathBuf::from(TEST_HOME).join(".codex/sessions")
        );
    }

    #[test]
    fn rejects_undefined_environment_variables_in_provider_directories() {
        let error = from_toml_with_environment(
            r#"
                [[providers]]
                name = "pi"
                dir = "$HIS_CONFIG_TEST_UNDEFINED_PROVIDER_DIRECTORY"
            "#,
            |_| Err(VarError::NotPresent),
        )
        .unwrap_err();

        assert!(format!("{error:#}").starts_with(
            "failed to resolve environment variables in provider directory \"$HIS_CONFIG_TEST_UNDEFINED_PROVIDER_DIRECTORY\""
        ));
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
