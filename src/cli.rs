use crate::{Config, list_sessions, tui};
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

pub fn run() -> ExitCode {
    let cli = Cli::parse();

    let dir = format!(
        "{}/{}",
        env::current_dir().unwrap().to_str().unwrap(),
        "tests/.his"
    );
    let config = Config::new(dir).expect("failed");

    match cli.command {
        None => tui::run(&config).expect("failed"),
        Some(Command::ListSession) => list_sessions(&config).expect("failed"),
    }
    ExitCode::SUCCESS
}
