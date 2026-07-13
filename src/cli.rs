use crate::config;
use crate::session::SessionRepository;
use crate::tui;
use anyhow::{Result, anyhow};
use clap::{Parser, Subcommand};
use std::env;
use std::ffi::OsString;
use std::path::PathBuf;
use std::process::ExitCode;

const HIS_HOME_ENV: &str = "HIS_HOME";

#[derive(Debug, Subcommand)]
pub enum Command {
    #[command(about = "List all coding sessions")]
    ListSession,
}

#[derive(Debug, Parser)]
#[command(
    name = "His",
    version = "0.1.0",
    about = "View your coding agent history here"
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

pub fn run() -> Result<ExitCode> {
    let cli = Cli::parse();

    let config = config::load(config_home(env::var_os(HIS_HOME_ENV))?)?;
    let agents = config.agents.as_deref().unwrap_or_default();
    let repository = SessionRepository::new(agents)?;

    match cli.command {
        None => tui::run(&repository)?,
        Some(Command::ListSession) => list_sessions(&repository)?,
    }
    Ok(ExitCode::SUCCESS)
}

fn list_sessions(repository: &SessionRepository<'_>) -> Result<()> {
    println!("{:?}", repository.list_sessions()?);
    Ok(())
}

fn config_home(value: Option<OsString>) -> Result<PathBuf> {
    let Some(value) = value.filter(|value| !value.is_empty()) else {
        return Err(anyhow!("{HIS_HOME_ENV} must be set to a non-empty path"));
    };
    Ok(value.into())
}

#[cfg(test)]
mod tests {
    use super::config_home;
    use std::ffi::OsString;
    use std::path::PathBuf;

    #[test]
    fn loads_config_home_from_environment_value() {
        let home = config_home(Some(OsString::from("/tmp/.his"))).unwrap();

        assert_eq!(home, PathBuf::from("/tmp/.his"));
    }

    #[test]
    fn rejects_missing_config_home() {
        let error = config_home(None).unwrap_err();

        assert_eq!(
            format!("{error:#}"),
            "HIS_HOME must be set to a non-empty path"
        );
    }

    #[test]
    fn rejects_empty_config_home() {
        let error = config_home(Some(OsString::new())).unwrap_err();

        assert_eq!(
            format!("{error:#}"),
            "HIS_HOME must be set to a non-empty path"
        );
    }
}
