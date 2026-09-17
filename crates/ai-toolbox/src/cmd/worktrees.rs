//! `ai-toolbox worktrees` - are they all configured the same, and `sync` to make them so.

use ai_toolbox_core::install::Plan;
use ai_toolbox_core::worktree::{Comparison, Standing, State};
use ai_toolbox_core::{action, worktree};

use crate::out;

pub fn run(comparison: &Comparison, sync: bool, dry_run: bool, json: bool) -> anyhow::Result<()> {
    let mut plan = Plan::default();
    if sync {
        for state in comparison.out_of_step() {
            worktree::converge(&mut plan, &comparison.reference, &state.worktree.path)?;
        }
    }

    if json {
        let payload = serde_json::json!({
            "reference": comparison.reference,
            "uniform": comparison.is_uniform(),
            "worktrees": comparison.worktrees,
            "changes": plan.changes(),
            "actions": plan.actions.iter().map(|a| a.line()).collect::<Vec<_>>(),
        });
        if sync && !dry_run {
            action::apply(&plan.actions)?;
        }
        println!("{}", serde_json::to_string_pretty(&payload)?);
        return Ok(());
    }

    if comparison.worktrees.is_empty() {
        out::heading("Worktrees");
        out::info("not a git repository - nothing to compare.");
        return Ok(());
    }

    out::heading(&format!("Worktrees of {}", comparison.reference.display()));
    for state in &comparison.worktrees {
        println!("  {:<24} {}", state.worktree.label(), describe(state));
    }

    if comparison.is_uniform() {
        println!();
        out::info("every worktree is configured the same way.");
        return Ok(());
    }

    if !sync {
        println!();
        out::info(&format!(
            "{} worktree(s) are out of step: ai-toolbox worktrees --sync",
            comparison.out_of_step().len()
        ));
        return Ok(());
    }

    out::heading(&format!("Sync ({} change(s))", plan.changes()));
    for action in &plan.actions {
        if action.noop {
            out::info(&format!("  {}", action.line()));
        } else {
            out::ok(&action.summary);
        }
    }
    if dry_run {
        out::info("--dry-run: nothing was written.");
        return Ok(());
    }
    let outcome = action::apply(&plan.actions)?;
    for path in &outcome.stale {
        out::warn(&format!(
            "{} changed while this was being prepared - left alone. Re-run to include it.",
            path.display()
        ));
    }
    // Only the reference's copies were pushed out, so anything a worktree alone had is
    // still there - and still worth mentioning, since sync did not take it.
    let extras: usize = comparison
        .worktrees
        .iter()
        .map(|state| state.diff.extra.len())
        .sum();
    if extras > 0 {
        out::info(&format!(
            "{extras} file(s) exist only in a worktree and were left alone - copy them into {} by hand to share them.",
            comparison.reference.display()
        ));
    }
    Ok(())
}

fn describe(state: &State) -> String {
    match state.standing {
        Standing::Reference => out::dim("reference"),
        Standing::InStep => out::dim("in step"),
        // Not damage: a worktree added since the repo was set up has none of this,
        // because none of it is tracked. Saying so beats listing seventeen faults.
        Standing::Unconfigured => {
            out::yellow("not configured yet - nothing from the toolkit is here")
        }
        Standing::Diverged => out::yellow(&state.diff.summary()),
        Standing::Unusable => out::dim(
            &state
                .worktree
                .prunable
                .clone()
                .unwrap_or_else(|| "nothing on disk".to_string()),
        ),
    }
}
