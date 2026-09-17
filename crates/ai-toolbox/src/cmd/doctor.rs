//! `ai-toolbox doctor` - what is wrong here, and `--fix` to put it right.
//!
//! Findings are printed worst first, because the broken ones are what somebody opening
//! this wants. `--fix` previews its actions before applying them: a repair that touches
//! files without saying which is not something to run on a repo you care about.

use ai_toolbox_core::doctor::{self, Finding, Severity};
use ai_toolbox_core::install::Plan;
use ai_toolbox_core::{action, Catalogue, Survey};

use crate::out;

pub fn run(
    survey: &Survey,
    catalogue: &Catalogue,
    fix: bool,
    dry_run: bool,
    json: bool,
) -> anyhow::Result<()> {
    let findings = &survey.findings;
    let mut plan = Plan::default();
    if fix {
        doctor::repair(
            &mut plan,
            &survey.inventory.repo,
            &survey.inventory,
            &survey.report,
            catalogue,
            findings,
        )?;
    }

    if json {
        let payload = serde_json::json!({
            "repo": survey.inventory.repo,
            "state": survey.state,
            "findings": findings,
            "repairs": plan.actions.iter().map(|a| a.line()).collect::<Vec<_>>(),
            "changes": plan.changes(),
        });
        if fix && !dry_run {
            action::apply(&plan.actions)?;
        }
        println!("{}", serde_json::to_string_pretty(&payload)?);
        return Ok(());
    }

    out::heading(&format!("Doctor: {}", survey.inventory.repo.display()));
    if findings.is_empty() {
        out::ok("nothing to report");
        return Ok(());
    }

    for finding in findings {
        let tag = match finding.severity {
            Severity::Broken => out::red("broken "),
            Severity::Warning => out::yellow("warning"),
            Severity::Note => out::dim("note   "),
        };
        println!("{tag}  {}", finding.what);
        // Where, and what to do, indented under the thing they belong to. A finding
        // about the repo itself has no path worth repeating back.
        let where_ = relative(finding, survey);
        if !where_.is_empty() {
            println!("         {}", out::dim(&where_));
        }
        if let Some(advice) = &finding.advice {
            println!("         {}", out::dim(advice));
        }
    }

    let counts = summarise(findings);
    println!();
    out::info(&counts);

    if !fix {
        let repairable = findings.iter().filter(|f| f.repairable).count();
        if repairable > 0 {
            out::info(&format!(
                "{repairable} of these can be repaired: ai-toolbox doctor --fix"
            ));
        }
        return Ok(());
    }

    if plan.actions.is_empty() {
        out::info("nothing here can be repaired automatically - see the advice above.");
        return Ok(());
    }

    out::heading(&format!("Repair ({} change(s))", plan.changes()));
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
    out::info("Re-run 'ai-toolbox doctor' to confirm.");
    Ok(())
}

fn relative(finding: &Finding, survey: &Survey) -> String {
    finding
        .path
        .strip_prefix(&survey.inventory.repo)
        .unwrap_or(&finding.path)
        .to_string_lossy()
        .into_owned()
}

fn summarise(findings: &[Finding]) -> String {
    let count = |severity: Severity| findings.iter().filter(|f| f.severity == severity).count();
    let parts: Vec<String> = [
        (count(Severity::Broken), "broken"),
        (count(Severity::Warning), "warning"),
        (count(Severity::Note), "note"),
    ]
    .into_iter()
    .filter(|(n, _)| *n > 0)
    .map(|(n, label)| format!("{n} {label}"))
    .collect();
    parts.join(", ")
}
