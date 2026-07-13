use crate::agent::session::SessionRepository;
use crate::config;
use crate::tui;
use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use std::env;
use std::process::ExitCode;

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

    let dir = env::current_dir()
        .context("failed to determine the current directory")?
        .join(config::CONFIG_DIRECTORY_NAME);
    let config = config::load(dir)?;
    let providers = config.providers.as_deref().unwrap_or_default();
    let repository = SessionRepository::new(providers)?;

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
