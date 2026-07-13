use crate::config;
use crate::repository::SessionRepository;
use crate::session::SessionSummary;
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
    let repository = SessionRepository::new(config.agents)?;

    match cli.command {
        None => tui::run(&repository)?,
        Some(Command::ListSession) => list_sessions(&repository)?,
    }
    Ok(ExitCode::SUCCESS)
}

fn list_sessions(repository: &SessionRepository) -> Result<()> {
    let catalog = repository.list_sessions();
    for warning in &catalog.warnings {
        eprintln!("warning: {warning}");
    }
    for session in &catalog.sessions {
        println!("{}", session_summary_line(session));
    }
    Ok(())
}

fn session_summary_line(session: &SessionSummary) -> String {
    let first_message = session.first_message.replace(['\r', '\n'], " ");
    format!(
        "{}\t{}\t{}\t{}\t{}",
        session.timestamp.as_str(),
        session.agent,
        session.id,
        session.cwd.display(),
        first_message
    )
}

fn config_home(value: Option<OsString>) -> Result<PathBuf> {
    let Some(value) = value.filter(|value| !value.is_empty()) else {
        return Err(anyhow!("{HIS_HOME_ENV} must be set to a non-empty path"));
    };
    Ok(value.into())
}

#[cfg(test)]
mod tests {
    use super::{config_home, session_summary_line};
    use crate::agent::AgentKind;
    use crate::session::{SessionLocator, SessionSummary, SessionTimestamp};
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

    #[test]
    fn formats_session_summaries_without_internal_locator_details() {
        let summary = SessionSummary {
            id: "session-id".to_string(),
            agent: AgentKind::Codex,
            timestamp: SessionTimestamp::new("2026-07-13T01:00:00Z"),
            cwd: PathBuf::from("/work/project"),
            first_message: "First\nmessage".to_string(),
            locator: SessionLocator::new(PathBuf::from("/private/session.jsonl")),
        };

        let line = session_summary_line(&summary);

        assert_eq!(
            line,
            "2026-07-13T01:00:00Z\tCodex\tsession-id\t/work/project\tFirst message"
        );
        assert!(!line.contains("/private/session.jsonl"));
    }
}
