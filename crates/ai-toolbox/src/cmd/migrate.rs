//! `ai-toolbox migrate` - fold a per-harness repo onto the canonical layout.

use std::path::Path;

use ai_toolbox_core::install::Plan;
use ai_toolbox_core::{action, migrate};

use crate::out;

pub fn run(repo: &Path, dry_run: bool, json: bool) -> anyhow::Result<()> {
    let mut plan = Plan::default();
    let report = migrate::plan(&mut plan, repo)?;

    if json {
        let payload = serde_json::json!({
            "repo": repo,
            "needed": migrate::needed(repo),
            "changes": plan.changes(),
            "actions": plan.actions.iter().map(|a| a.line()).collect::<Vec<_>>(),
            "moved": report.moved,
            "conflicts": report.conflicts,
            "notes": report.notes,
        });
        if !dry_run {
            action::apply(&plan.actions)?;
        }
        println!("{}", serde_json::to_string_pretty(&payload)?);
        return Ok(());
    }

    out::heading(&format!("Migrate: {}", repo.display()));
    if plan.is_noop() && report.conflicts.is_empty() {
        out::ok("already on the canonical layout - nothing to move");
        return Ok(());
    }

    for action in &plan.actions {
        if !action.noop {
            out::ok(&action.summary);
        }
    }
    for note in &report.notes {
        out::info(note);
    }

    // Reported after the plan, because these are the parts it deliberately did not do.
    for conflict in &report.conflicts {
        out::warn(conflict);
    }

    if dry_run {
        out::info(&format!(
            "--dry-run: {} change(s) not applied. Re-run without it to migrate.",
            plan.changes()
        ));
        return Ok(());
    }

    let outcome = action::apply(&plan.actions)?;
    for path in &outcome.stale {
        out::warn(&format!(
            "{} changed while this was being prepared - left alone. Re-run to include it.",
            path.display()
        ));
    }

    out::info("Verify with 'ai-toolbox doctor', then restart each harness.");
    // The files and the paths inside the configs moved together, so splitting them
    // across two commits leaves one of them broken in between.
    out::info(
        "Commit the move in one go - the paths inside settings.json / config.toml changed with it.",
    );
    Ok(())
}
