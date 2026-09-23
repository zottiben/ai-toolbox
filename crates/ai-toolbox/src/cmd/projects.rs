//! `ai-toolbox projects` - the repos on this machine and how each one is doing.

use std::path::{Path, PathBuf};

use ai_toolbox_core::classify::Counts;
use ai_toolbox_core::registry::{self, Registry, ScanOptions, Summary};
use ai_toolbox_core::{Catalogue, State};

use crate::out;

pub fn list(registry: &Registry, catalogue: &Catalogue, json: bool) -> anyhow::Result<()> {
    let repos = registry.all()?;
    let summaries: Vec<Summary> = repos
        .iter()
        .map(|repo| registry::summarise(repo, catalogue))
        .collect();

    if json {
        println!("{}", serde_json::to_string_pretty(&summaries)?);
        return Ok(());
    }

    if summaries.is_empty() {
        out::heading("Projects");
        out::info("none known yet - run 'ai-toolbox projects scan' to find them.");
        return Ok(());
    }

    out::heading(&format!("Projects ({})", summaries.len()));
    let width = summaries
        .iter()
        .map(|s| s.repo.name().len())
        .max()
        .unwrap_or(0)
        .clamp(12, 28);

    for summary in &summaries {
        println!(
            "  {:<width$}  {:<28} {}",
            summary.repo.name(),
            state(summary),
            detail(summary),
            width = width
        );
    }

    let unconfigured = summaries
        .iter()
        .filter(|s| s.state == State::Unconfigured && s.exists)
        .count();
    let broken = summaries
        .iter()
        .filter(|s| s.state == State::Broken)
        .count();
    let drifted = summaries.iter().filter(|s| !s.worktrees_in_step).count();

    println!();
    if broken > 0 {
        let needs = agrees(broken, "needs", "need");
        out::info(&format!(
            "{broken} {needs} repair: ai-toolbox doctor --repo <path> --fix"
        ));
    }
    if drifted > 0 {
        let has = agrees(drifted, "has", "have");
        out::info(&format!(
            "{drifted} {has} worktrees out of step: ai-toolbox worktrees --repo <path> --sync"
        ));
    }
    if unconfigured > 0 {
        let is = agrees(unconfigured, "is", "are");
        out::info(&format!(
            "{unconfigured} {is} not set up: ai-toolbox init --repo <path>"
        ));
    }
    Ok(())
}

/// The verb for a count, so a list of one does not read "1 are not set up".
fn agrees(count: usize, one: &'static str, many: &'static str) -> &'static str {
    if count == 1 {
        one
    } else {
        many
    }
}

pub fn scan(registry: &mut Registry, roots: &[PathBuf], json: bool) -> anyhow::Result<()> {
    let roots = if roots.is_empty() {
        registry.scan_roots()?
    } else {
        roots.to_vec()
    };
    if roots.is_empty() {
        anyhow::bail!(
            "no scan roots. Pass --root <path>, and it will be remembered for next time."
        );
    }

    let result = registry::scan(registry, &roots, &ScanOptions::default())?;
    // Remembering a root that was asked for by hand means the next scan needs no flags.
    for root in &roots {
        if root.is_dir() {
            registry.add_scan_root(root)?;
        }
    }

    if json {
        println!("{}", serde_json::to_string_pretty(&result)?);
        return Ok(());
    }

    out::heading(&format!(
        "Scanned {}",
        roots
            .iter()
            .map(|r| r.display().to_string())
            .collect::<Vec<_>>()
            .join(", ")
    ));
    for path in &result.added {
        out::ok(&format!("found {}", path.display()));
    }
    for root in &result.missing_roots {
        out::warn(&format!("{} is not there - skipped", root.display()));
    }
    out::info(&format!(
        "{} repo(s): {} new, {} already known.",
        result.found(),
        result.added.len(),
        result.known.len()
    ));
    Ok(())
}

pub fn forget(registry: &mut Registry, path: &Path, json: bool) -> anyhow::Result<()> {
    // Resolved the same way it was recorded, so forgetting from inside a worktree
    // forgets the project rather than missing it.
    let target = registry::main_worktree(path).unwrap_or_else(|_| path.to_path_buf());
    let removed = registry.forget(&target)?;
    if json {
        println!(
            "{}",
            serde_json::json!({ "path": target, "removed": removed })
        );
        return Ok(());
    }
    if removed {
        out::ok(&format!("forgot {}", target.display()));
        out::info("the files are untouched - this only removes it from the list.");
    } else {
        out::info(&format!("{} was not in the list.", target.display()));
    }
    Ok(())
}

pub fn prune(registry: &mut Registry, json: bool) -> anyhow::Result<()> {
    let gone = registry.prune()?;
    if json {
        println!("{}", serde_json::to_string_pretty(&gone)?);
        return Ok(());
    }
    for path in &gone {
        out::ok(&format!("forgot {} (no longer on disk)", path.display()));
    }
    if gone.is_empty() {
        out::info("every known repo is still there.");
    }
    Ok(())
}

fn state(summary: &Summary) -> String {
    if !summary.exists {
        return out::dim("gone from disk");
    }
    match summary.state {
        State::Unconfigured => out::dim("not set up"),
        State::Healthy if !summary.worktrees_in_step => out::yellow("worktrees out of step"),
        State::Healthy => out::green("healthy"),
        State::Attention => out::yellow("needs a look"),
        State::Broken => out::red("broken"),
    }
}

/// The right-hand column: what is actually installed, and anything else worth knowing.
fn detail(summary: &Summary) -> String {
    if !summary.exists {
        return out::dim(&summary.repo.path.display().to_string());
    }
    // A repo that could not be read says why. "broken" with no reason is not something
    // anybody can act on. The repo is named in the column to the left, so the error's
    // copy of its path is dropped and the file and the fault are what is left.
    if let Some(problem) = &summary.problem {
        let prefix = format!("{}/", summary.repo.path.display());
        return out::dim(problem.strip_prefix(&prefix).unwrap_or(problem));
    }
    let mut parts = Vec::new();

    if !summary.harnesses.is_empty() {
        parts.push(
            summary
                .harnesses
                .iter()
                .map(|h| h.key())
                .collect::<Vec<_>>()
                .join("+"),
        );
    }
    let counted = counts(&summary.counts);
    if !counted.is_empty() {
        parts.push(counted);
    }
    if summary.worktrees > 1 {
        parts.push(format!("{} worktrees", summary.worktrees));
    }
    if parts.is_empty() && !summary.stack.is_empty() {
        // Nothing installed yet, so say what it is instead - that is what makes an
        // unconfigured repo worth clicking on.
        parts.push(
            summary
                .stack
                .iter()
                .map(|s| s.label())
                .collect::<Vec<_>>()
                .join(" "),
        );
    }
    out::dim(&parts.join("  "))
}

fn counts(counts: &Counts) -> String {
    let mut parts = Vec::new();
    if counts.managed > 0 {
        parts.push(format!("{} managed", counts.managed));
    }
    if counts.stale > 0 {
        parts.push(format!("{} stale", counts.stale));
    }
    if counts.modified > 0 {
        parts.push(format!("{} edited", counts.modified));
    }
    if counts.local > 0 {
        parts.push(format!("{} local", counts.local));
    }
    if counts.broken > 0 {
        parts.push(format!("{} broken", counts.broken));
    }
    parts.join(", ")
}
