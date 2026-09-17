//! The `ai-toolbox` command.
//!
//! Thin on purpose: it resolves the clone, reads the repo, and hands both to a command
//! module. Everything that decides anything lives in `ai-toolbox-core`, so the board and
//! the desktop app get the same answers without a second implementation (D1).

mod cli;
mod cmd;
mod out;

use std::io::IsTerminal;

use clap::Parser;

use ai_toolbox_core::{detect, install, root, survey, Catalogue, Harness, Machine};

use cli::{Cli, Command};
use cmd::install::Options;

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
    let options = Options {
        dry_run: cli.dry_run,
        json: cli.json,
    };

    match &cli.command {
        Command::List => return cmd::list::run(&catalogue, cli.json),
        Command::Rules { names } => return cmd::rules::run(&catalogue, names),
        Command::PiInit => return cmd::install::pi_init(),
        _ => {}
    }

    let repo = cli.repo()?;
    let harnesses = cli.harnesses(&repo)?;

    match &cli.command {
        Command::List | Command::Rules { .. } | Command::PiInit => unreachable!("handled above"),

        Command::Status => {
            let survey = survey(&repo, &catalogue)?;
            cmd::status::run(&survey, &Machine::read(), &catalogue, cli.json)
        }
        Command::Recommend => {
            let survey = survey(&repo, &catalogue)?;
            cmd::recommend::run(&survey, cli.json)
        }
        Command::Worktrees { sync } => {
            let comparison = ai_toolbox_core::worktree::compare(&repo)?;
            cmd::worktrees::run(&comparison, *sync, cli.dry_run, cli.json)
        }
        Command::Doctor { fix } => {
            let survey = survey(&repo, &catalogue)?;
            let findings =
                ai_toolbox_core::diagnose(&repo, &survey.inventory, &survey.report, &catalogue);
            cmd::doctor::run(&survey, &catalogue, &findings, *fix, cli.dry_run, cli.json)
        }

        Command::Hooks { names } => {
            // No names means all of them, as the bash has always done.
            let names = if names.is_empty() {
                catalogue.hooks.iter().map(|h| h.name.clone()).collect()
            } else {
                names
                    .iter()
                    .map(|n| n.trim_end_matches(".sh").to_string())
                    .collect::<Vec<_>>()
            };
            let mut plan = install::Plan::default();
            install::hooks(&mut plan, &repo, &catalogue, &names, &harnesses)?;
            cmd::install::run(&plan, &harnesses, options)
        }
        Command::Mcp { names } => {
            let mut plan = install::Plan::default();
            install::mcp(&mut plan, &repo, &catalogue, names, &harnesses)?;
            cmd::install::run(&plan, &harnesses, options)
        }
        Command::Skill {
            names,
            user,
            no_symlink,
        } => {
            let root = if *user {
                ai_toolbox_core::machine::home()
            } else {
                repo.clone()
            };
            let mut plan = install::Plan::default();
            install::skills(&mut plan, &root, &catalogue, names, &harnesses, !no_symlink)?;
            cmd::install::run(&plan, &harnesses, options)
        }
        Command::BaseCharter { path } => {
            let mut plan = install::Plan::default();
            install::base_charter(&mut plan, &catalogue, &harnesses, path.as_deref())?;
            cmd::install::run(&plan, &harnesses, options)?;
            out::info(
                "This is the one always-on artifact - do this once per machine, not per repo.",
            );
            Ok(())
        }
        Command::WithDotenv => {
            let mut plan = install::Plan::default();
            install::with_dotenv(&mut plan, &repo, &catalogue)?;
            cmd::install::run(&plan, &harnesses, options)?;
            out::info("One copy for every harness - they all take a command path.");
            Ok(())
        }

        Command::Bootstrap { yes } => bootstrap(&cli, &repo, &catalogue, &harnesses, *yes, false),
        Command::Init { yes } => bootstrap(&cli, &repo, &catalogue, &harnesses, *yes, true),
    }
}

/// Detect the stack, show what that calls for, confirm, install.
///
/// `scaffold` adds the AGENTS.md / CLAUDE.md skeletons, which is the only difference
/// between `init` and `bootstrap`.
fn bootstrap(
    cli: &Cli,
    repo: &std::path::Path,
    catalogue: &Catalogue,
    harnesses: &[Harness],
    yes: bool,
    scaffold: bool,
) -> anyhow::Result<()> {
    let recommendation = detect::recommend(repo);
    let options = Options {
        dry_run: cli.dry_run,
        json: cli.json,
    };

    if !cli.json {
        let detected = recommendation.detected_labels();
        out::heading(&format!(
            "Detected: {}",
            if detected.is_empty() {
                "unknown - generic set".to_string()
            } else {
                detected.join(" ")
            }
        ));
        println!("  {:<8} {}", "hooks", out::list(&recommendation.hooks));
        println!("  {:<8} {}", "mcp", out::list(&recommendation.mcp));
        println!("  {:<8} {}", "skills", out::list(&recommendation.skills));
        println!(
            "  {:<8} {}  {}",
            "rules",
            out::list(&recommendation.rules),
            out::dim("(inline into AGENTS.md)")
        );
        for note in &recommendation.notes {
            out::info(note);
        }
    }

    let plan = install::everything(
        repo,
        catalogue,
        harnesses,
        &recommendation.hooks,
        &recommendation.mcp,
        &recommendation.skills,
        scaffold,
    )?;

    if !options.dry_run && !yes && !cli.json {
        if !std::io::stdin().is_terminal() {
            anyhow::bail!(
                "not a tty - pass --yes to install non-interactively (or run each group by hand)."
            );
        }
        if !confirm(&format!("Install {} change(s)?", plan.changes()))? {
            out::info("Nothing installed.");
            return Ok(());
        }
    }

    cmd::install::run(&plan, harnesses, options)?;

    if !recommendation.rules.is_empty() && !cli.json {
        out::info(&format!(
            "Rule snippets for this stack: ai-toolbox rules {}",
            recommendation.rules.join(" ")
        ));
    }
    if scaffold && !options.dry_run && !cli.json {
        out::info(
            "Fill in AGENTS.md by hand, or run the /toolbox-init skill to author it by interview.",
        );
    }
    Ok(())
}

fn confirm(question: &str) -> anyhow::Result<bool> {
    use std::io::Write;
    eprint!("{question} [Y/n] ");
    std::io::stderr().flush()?;
    let mut answer = String::new();
    std::io::stdin().read_line(&mut answer)?;
    Ok(!answer.trim_start().starts_with(['n', 'N']))
}
