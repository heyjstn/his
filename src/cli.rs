use crate::config::Config;
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
        .join("tests/.his");
    let config = Config::new(dir)?;

    match cli.command {
        None => tui::run(&config)?,
        Some(Command::ListSession) => list_sessions(&config)?,
    }
    Ok(ExitCode::SUCCESS)
}

fn list_sessions(config: &Config) -> Result<()> {
    println!("{:?}", config.list_sessions()?);
    Ok(())
}
