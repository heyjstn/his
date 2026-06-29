use crate::{Config, DEFAULT_CONFIG_DIR, list_sessions};
use clap::{Parser, Subcommand};
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

pub fn run() -> ExitCode {
    let cli = Cli::parse();

    let config = Config::new(DEFAULT_CONFIG_DIR.to_string()).expect("failed");

    match cli.command {
        None => println!("{:?}", "Entering TUI"),
        Some(Command::ListSession) => list_sessions(&config).expect("failed"),
    }
    ExitCode::SUCCESS
}
