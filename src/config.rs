use crate::agent::AgentKind;
use anyhow::{Context, Result};
use serde::Deserialize;
use std::env::{self, VarError};
use std::fs;
use std::path::{Path, PathBuf};

const CONFIG_FILE_NAME: &str = "config.toml";

#[derive(Debug)]
pub(crate) struct Config {
    pub(crate) agents: Vec<AgentConfig>,
}

#[derive(Clone, Debug)]
pub(crate) struct AgentConfig {
    pub(crate) kind: AgentKind,
    pub(crate) directory: PathBuf,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawConfig {
    #[serde(default)]
    agents: Vec<RawAgentConfig>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawAgentConfig {
    kind: AgentKind,
    dir: String,
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
    let raw: RawConfig = toml::from_str(data).context("failed to parse config")?;
    let agents = raw
        .agents
        .into_iter()
        .map(|agent| {
            let directory = shellexpand::env_with_context(&agent.dir, |variable| {
                environment(variable).map(Some)
            })
            .with_context(|| {
                format!(
                    "failed to resolve environment variables in agent directory {:?}",
                    agent.dir
                )
            })?;
            Ok(AgentConfig {
                kind: agent.kind,
                directory: PathBuf::from(directory.into_owned()),
            })
        })
        .collect::<Result<Vec<_>>>()?;

    Ok(Config { agents })
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

        assert_eq!(config.agents.len(), 1);
        assert_eq!(config.agents[0].directory, PathBuf::from("/tmp/.codex"));
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn defaults_to_no_agents() {
        let config = from_toml("").unwrap();

        assert!(config.agents.is_empty());
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

        assert_eq!(
            config.agents[0].directory,
            PathBuf::from(TEST_PWD).join("tests/.pi/agent/sessions")
        );
        assert_eq!(
            config.agents[1].directory,
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

    fn test_directory() -> PathBuf {
        let sequence = NEXT_TEST_DIRECTORY.fetch_add(1, Ordering::Relaxed);
        let directory =
            std::env::temp_dir().join(format!("his-config-test-{}-{sequence}", std::process::id()));
        fs::create_dir_all(&directory).unwrap();
        directory
    }
}
