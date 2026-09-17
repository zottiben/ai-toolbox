//! The command surface.
//!
//! Every command keeps the name, the arguments and the meaning it has in
//! `bin/ai-toolbox`, because the bash becomes a shim over this binary and anything that
//! shells out to `ai-toolbox <cmd>` - the skills included - must not notice the change.
//! `--json` is the one addition, and it is what the board reads.

use std::path::PathBuf;

use clap::{Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(
    name = "ai-toolbox",
    version,
    about = "Set up the ai-toolbox hooks, MCP servers and skills in a repo.",
    // Matching the bash, which accepts --repo and --harness anywhere in the line.
    args_conflicts_with_subcommands = false
)]
pub struct Cli {
    /// The repo to act on. Defaults to the working directory.
    #[arg(long, global = true, value_name = "PATH")]
    pub repo: Option<PathBuf>,

    /// claude|codex|pi|both|all, or a comma-separated list. Defaults to the harnesses
    /// this machine and repo already use.
    #[arg(long, global = true, value_name = "NAME")]
    pub harness: Option<String>,

    /// Machine-readable output. The board reads this; so can you.
    #[arg(long, global = true)]
    pub json: bool,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// What this repo has installed, per harness.
    Status,
    /// Everything the catalogue offers.
    List,
    /// What to install here, based on the stack.
    Recommend,
}

impl Cli {
    pub fn repo(&self) -> anyhow::Result<PathBuf> {
        let repo = match &self.repo {
            Some(path) => path.clone(),
            None => std::env::current_dir()?,
        };
        if !repo.is_dir() {
            anyhow::bail!("target repo not found: {}", repo.display());
        }
        Ok(repo.canonicalize()?)
    }
}
