//! The commands that write: `hooks`, `mcp`, `skill`, `init`, `bootstrap` and the two
//! once-per-machine ones.
//!
//! Each builds a plan and hands it to [`run`], so the preview, the `--dry-run` path, the
//! `--json` output and the follow-up advice are written once rather than eight times.

use ai_toolbox_core::action;
use ai_toolbox_core::install::Plan;
use ai_toolbox_core::machine::PiMcp;
use ai_toolbox_core::secrets::Source;
use ai_toolbox_core::{paths, Harness, Machine};

use crate::out;

#[derive(Debug, Clone, Copy)]
pub struct Options {
    pub dry_run: bool,
    pub json: bool,
}

/// Show a plan, apply it unless this is a dry run, then say what has to happen next.
pub fn run(plan: &Plan, harnesses: &[Harness], options: Options) -> anyhow::Result<()> {
    if options.json {
        let payload = serde_json::json!({
            "actions": plan.actions,
            "lines": plan.actions.iter().map(|a| a.line()).collect::<Vec<_>>(),
            "changes": plan.changes(),
            "warnings": plan.warnings,
            "secrets": plan.secrets,
            "applied": !options.dry_run,
        });
        if !options.dry_run {
            let outcome = action::apply(&plan.actions)?;
            report_stale(&outcome);
        }
        println!("{}", serde_json::to_string_pretty(&payload)?);
        return Ok(());
    }

    for action in &plan.actions {
        if action.noop {
            out::info(&format!("  {}", action.line()));
        } else {
            out::ok(&action.summary);
        }
    }

    if options.dry_run {
        out::info(&format!(
            "--dry-run: {} change(s) not applied.",
            plan.changes()
        ));
    } else {
        let outcome = action::apply(&plan.actions)?;
        report_stale(&outcome);
        if plan.is_noop() && !plan.actions.is_empty() {
            out::info("Everything was already in place.");
        }
    }

    for warning in &plan.warnings {
        out::warn(warning);
    }
    report_secrets(plan);
    report_next_steps(harnesses, plan);
    Ok(())
}

/// A path that changed between planning and applying is reported, never overwritten -
/// the agent that wrote it was there first.
fn report_stale(outcome: &action::Outcome) {
    for path in &outcome.stale {
        out::warn(&format!(
            "{} changed while this was being prepared - left alone. Re-run to include it.",
            path.display()
        ));
    }
}

fn report_secrets(plan: &Plan) {
    let environment: Vec<&str> = plan
        .secrets
        .iter()
        .filter(|s| s.source == Source::Environment)
        .map(|s| s.name.as_str())
        .collect();
    if !environment.is_empty() {
        out::warn("Set these in your environment before use (never inline a token):");
        for name in environment {
            eprintln!("    {name}");
        }
    }

    let dotenv: Vec<&str> = plan
        .secrets
        .iter()
        .filter(|s| s.source == Source::DotEnv)
        .map(|s| s.name.as_str())
        .collect();
    if !dotenv.is_empty() {
        out::warn(
            "Put these in the repo's .env (read at launch via with-dotenv - no export needed):",
        );
        for name in dotenv {
            eprintln!("    {name}");
        }
    }
}

/// How to make each harness pick the change up. Only printed when MCP servers were part
/// of the plan, since that is the case where restarting and authenticating is needed.
fn report_next_steps(harnesses: &[Harness], plan: &Plan) {
    let touched_mcp = plan
        .actions
        .iter()
        .any(|a| a.path.ends_with(paths::SHARED_MCP) || a.path.ends_with(paths::PI_MCP));
    if !touched_mcp {
        return;
    }
    for harness in harnesses {
        match harness {
            Harness::Claude => out::info(
                "Claude Code: restart in this repo, then /mcp to connect (and complete any OAuth).",
            ),
            Harness::Codex => {
                out::info("Codex: run 'codex mcp login <server>' for OAuth servers; restart codex to load .codex/config.toml.");
                out::info("Codex ignores project .codex/ until the project is trusted - run 'codex' here and approve.");
            }
            Harness::Pi => match Machine::read().pi_mcp {
                PiMcp::Ready => out::info(
                    "Pi: restart pi in this repo, then /mcp to connect (OAuth servers open a browser login on first connect; tokens go to the OS credential store).",
                ),
                _ => out::warn(
                    "Pi: run 'ai-toolbox pi-init' once to enable MCP, then restart pi and /mcp to connect.",
                ),
            },
        }
    }
}

/// `ai-toolbox pi-init` - the once-per-machine Pi MCP setup.
///
/// Not a file plan: Pi owns its own install, so this drives `pi install` and then checks
/// that both halves actually landed.
pub fn pi_init() -> anyhow::Result<()> {
    let machine = Machine::read();
    if machine.pi_mcp == PiMcp::Ready {
        out::ok("Pi MCP adapter already installed globally (nothing to do)");
        return Ok(());
    }
    if which("pi").is_none() {
        anyhow::bail!(
            "pi is not on PATH. Install it first: npm install -g --ignore-scripts @earendil-works/pi-coding-agent"
        );
    }

    out::info(&format!(
        "Installing {} globally (pi install npm:{})...",
        paths::PI_MCP_PACKAGE,
        paths::PI_MCP_PACKAGE
    ));
    let status = std::process::Command::new("pi")
        .arg("install")
        .arg(format!("npm:{}", paths::PI_MCP_PACKAGE))
        .status()?;
    if !status.success() {
        anyhow::bail!("pi install failed");
    }

    // Re-read rather than trust the exit code: the package being on disk and Pi knowing
    // about it are two different things, and a half-install fails later with a message
    // that mentions neither.
    if Machine::read().pi_mcp != PiMcp::Ready {
        anyhow::bail!(
            "pi install completed but {} was not registered. Run 'pi install npm:{}' and inspect {}/settings.json.",
            paths::PI_MCP_PACKAGE,
            paths::PI_MCP_PACKAGE,
            machine.pi_agent_dir.display()
        );
    }
    out::ok("Pi MCP adapter installed globally");
    out::info(
        "Next: in a repo, run 'ai-toolbox mcp <preset...> --harness pi'; restart pi, then /mcp.",
    );
    Ok(())
}

fn which(program: &str) -> Option<std::path::PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|dir| dir.join(program))
        .find(|candidate| candidate.is_file())
}
