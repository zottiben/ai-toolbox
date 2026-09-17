//! `ai-toolbox recommend` - what to install here, and what is already in.
//!
//! The bash prints the recommendation flat. This marks what the repo already has,
//! because the useful reading of a recommendation in a configured repo is the
//! difference, not the whole list.

use ai_toolbox_core::detect::Recommendation;
use ai_toolbox_core::{Kind, Survey};

use crate::out;

pub fn run(survey: &Survey, json: bool) -> anyhow::Result<()> {
    let rec = &survey.recommendation;
    if json {
        println!("{}", serde_json::to_string_pretty(rec)?);
        return Ok(());
    }

    let detected = rec.detected_labels();
    let summary = if detected.is_empty() {
        "unknown - generic set".to_string()
    } else {
        detected.join(" ")
    };
    out::heading(&format!("Detected: {summary}"));

    group(survey, "hooks", &rec.hooks, Kind::Hook);
    group(survey, "mcp", &rec.mcp, Kind::Server);
    group(survey, "skills", &rec.skills, Kind::Skill);
    println!(
        "  {:<8} {}  {}",
        "rules",
        out::list(&rec.rules),
        out::dim("(inline into AGENTS.md)")
    );

    for note in &rec.notes {
        out::info(note);
    }
    report_missing(survey, rec);
    Ok(())
}

fn group(survey: &Survey, label: &str, wanted: &[String], kind: Kind) {
    let rendered: Vec<String> = wanted
        .iter()
        .map(|name| {
            if installed(survey, kind, name) {
                out::dim(&format!("{name} ✓"))
            } else {
                name.clone()
            }
        })
        .collect();
    println!("  {:<8} {}", label, out::list(&rendered));
}

/// Whether the repo already has the thing being recommended.
///
/// The names do not line up by themselves: a preset is recommended by its own name but
/// installs servers under theirs, and a group skill is recommended as `cli/gh` but lands
/// as `gh`. Both are resolved through the catalogue rather than by string surgery.
fn installed(survey: &Survey, kind: Kind, name: &str) -> bool {
    match kind {
        Kind::Hook => survey.inventory.hook(name).is_some(),
        Kind::Skill => survey
            .inventory
            .skill(name.rsplit('/').next().unwrap_or(name))
            .is_some(),
        Kind::Server => survey
            .report
            .items_of(Kind::Server)
            .iter()
            .any(|item| item.catalogue_key.as_deref() == Some(name)),
        Kind::Helper => survey.inventory.helper(name).is_some(),
    }
}

fn report_missing(survey: &Survey, rec: &Recommendation) {
    let missing = [
        ("hooks", &rec.hooks, Kind::Hook),
        ("MCP presets", &rec.mcp, Kind::Server),
        ("skills", &rec.skills, Kind::Skill),
    ]
    .into_iter()
    .filter(|(_, wanted, kind)| wanted.iter().any(|n| !installed(survey, *kind, n)))
    .count();

    if missing == 0 {
        out::info("Everything recommended for this stack is already installed.");
        return;
    }
    out::info("Run 'ai-toolbox bootstrap' to install (with confirmation), or a per-group command.");
}
