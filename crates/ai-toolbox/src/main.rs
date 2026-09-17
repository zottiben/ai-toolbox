//! The `ai-toolbox` command.
//!
//! Thin on purpose: it resolves the clone, reads the repo, and hands both to a command
//! module. Everything that decides anything lives in `ai-toolbox-core`, so the board and
//! the desktop app get the same answers without a second implementation (D1).

mod cli;
mod cmd;
mod out;

use clap::Parser;

use ai_toolbox_core::{root, survey, Catalogue, Machine};

use cli::{Cli, Command};

fn main() {
    if let Err(err) = run() {
        // `{err:#}` so the cause chain comes with it - an IO error whose path is three
        // frames down is not useful on its own.
        eprintln!("{} {err:#}", out::red("ai-toolbox:"));
        std::process::exit(1);
    }
}

fn run() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let catalogue = Catalogue::load(root::find()?)?;

    match cli.command {
        Command::List => cmd::list::run(&catalogue, cli.json),
        Command::Status => {
            let survey = survey(cli.repo()?, &catalogue)?;
            cmd::status::run(&survey, &Machine::read(), &catalogue, cli.json)
        }
        Command::Recommend => {
            let survey = survey(cli.repo()?, &catalogue)?;
            cmd::recommend::run(&survey, cli.json)
        }
    }
}
