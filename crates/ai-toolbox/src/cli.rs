//! The command surface.
//!
//! Every command keeps the name, the arguments and the meaning it has in
//! `bin/ai-toolbox`, because the bash becomes a shim over this binary and anything that
//! shells out to `ai-toolbox <cmd>` - the skills included - must not notice the change.
//! `--json` is the one addition, and it is what the board reads.

use std::path::PathBuf;

use clap::{Parser, Subcommand};

use ai_toolbox_core::{harness, Harness};

#[derive(Debug, Parser)]
#[command(
    name = "ai-toolbox",
    version,
    about = "Set up the ai-toolbox hooks, MCP servers and skills in a repo."
)]
pub struct Cli {
    /// The repo to act on. Defaults to the working directory.
    #[arg(long, global = true, value_name = "PATH")]
    pub repo: Option<PathBuf>,

    /// claude|codex|pi|both|all, or a comma-separated list. Defaults to the harnesses
    /// this machine and repo already use.
    #[arg(long, global = true, value_name = "NAME")]
    pub harness: Option<String>,

    /// Show what would change and write nothing.
    #[arg(long, global = true)]
    pub dry_run: bool,

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
    /// Whether every worktree of this repo is configured the same way.
    Worktrees {
        /// Bring the ones that are behind into step with the main worktree.
        #[arg(long)]
        sync: bool,
    },
    /// What is broken here, and optionally put it right.
    Doctor {
        /// Repair what can be repaired. Anything that might be your own work is left.
        #[arg(long)]
        fix: bool,
    },

    /// Install hook scripts and wire them up. All of them when none is named.
    Hooks { names: Vec<String> },
    /// Add MCP presets to .mcp.json and each harness's config.
    Mcp {
        #[arg(required = true)]
        names: Vec<String>,
    },
    /// Install skills into .agents/skills.
    Skill {
        #[arg(required = true)]
        names: Vec<String>,
        /// Install into the home directory instead of the repo.
        #[arg(long)]
        user: bool,
        /// Copy into .claude/skills instead of linking it, for a filesystem without
        /// symlinks.
        #[arg(long)]
        no_symlink: bool,
    },
    /// Print stack rule snippets to paste into AGENTS.md. Writes nothing.
    Rules {
        #[arg(required = true)]
        names: Vec<String>,
    },

    /// Detect the stack and install what it calls for.
    Bootstrap {
        /// Install without asking.
        #[arg(long, short = 'y')]
        yes: bool,
    },
    /// Scaffold AGENTS.md and CLAUDE.md, then bootstrap.
    Init {
        #[arg(long, short = 'y')]
        yes: bool,
    },

    /// Append the always-on charter to each harness's global config. Once per machine.
    BaseCharter {
        /// Write to this file instead of the harnesses' own.
        path: Option<PathBuf>,
    },
    /// Drop the .env launcher some presets shell out to.
    WithDotenv,
    /// Install Pi's MCP client. Once per machine.
    PiInit,
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

    /// The harnesses to write for: what `--harness` says, or the ones already in use.
    pub fn harnesses(&self, repo: &std::path::Path) -> anyhow::Result<Vec<Harness>> {
        match &self.harness {
            Some(spec) => harness::select(spec).map_err(|e| anyhow::anyhow!(e)),
            None => Ok(harness::detect(repo, &ai_toolbox_core::machine::home())),
        }
    }
}
