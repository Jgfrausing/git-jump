mod cmd;
mod config;
mod git;

use anyhow::Result;
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "git-jump",
    about = "Fuzzy branch switcher + contextual clone for git",
    version
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Clone a repo, picking the git identity from your profile rules.
    Clone {
        /// Repository URL (https or git@host:org/repo form).
        url: String,
        /// Optional target directory.
        dir: Option<String>,
    },
    /// Initialise + push the current project to a new remote repository.
    /// The repo name is taken from the project root directory.
    #[command(name = "new-repo")]
    NewRepo {
        /// Namespace URL (e.g. github.com/yourname). Prompted for if omitted.
        url: Option<String>,
    },
}

fn main() {
    if let Err(err) = run() {
        eprintln!("error: {:#}", err);
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        None => cmd::switch::run(),
        Some(Command::Clone { url, dir }) => cmd::clone::run(url, dir),
        Some(Command::NewRepo { url }) => cmd::new_repo::run(url),
    }
}
