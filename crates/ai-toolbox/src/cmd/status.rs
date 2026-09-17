//! `ai-toolbox status` - what this repo has, arranged by which harness reads it.
//!
//! The layout follows the bash: the canonical copy first, then each harness's view of
//! it, because the recurring question is not "what is installed" but "will Claude Code
//! actually see it". Anything that is a machine-wide fact rather than a repo one says so.

use ai_toolbox_core::classify::{Kind, Origin};
use ai_toolbox_core::inventory::SkillsLink;
use ai_toolbox_core::machine::PiMcp;
use ai_toolbox_core::{paths, Catalogue, Machine, Survey};

use crate::out;

pub fn run(
    survey: &Survey,
    machine: &Machine,
    catalogue: &Catalogue,
    json: bool,
) -> anyhow::Result<()> {
    if json {
        // The survey serialises itself, so this and the board's project endpoint emit
        // the same shape rather than two hand-built ones that drift.
        let mut payload = serde_json::to_value(survey)?;
        if let Some(object) = payload.as_object_mut() {
            object.insert(
                "counts".to_string(),
                serde_json::to_value(survey.report.counts())?,
            );
            object.insert("machine".to_string(), serde_json::to_value(machine)?);
            object.insert(
                "catalogue_root".to_string(),
                serde_json::to_value(&catalogue.root)?,
            );
        }
        println!("{}", serde_json::to_string_pretty(&payload)?);
        return Ok(());
    }

    let inventory = &survey.inventory;
    out::heading(&format!("Repo: {}", inventory.repo.display()));

    println!(
        "{} {}",
        out::bold("Canonical"),
        out::dim("(one copy - .agents/ + repo root)")
    );
    out::field("AGENTS.md", &yes_or_none(inventory.agents_md));
    out::field("skills", &out::list(&names(survey, Kind::Skill)));
    out::field("hooks", &out::list(&names(survey, Kind::Hook)));
    out::field("mcp", &out::list(&names(survey, Kind::Server)));
    out::field("helpers", &out::list(&names(survey, Kind::Helper)));

    println!(
        "\n{} {}",
        out::bold("Claude Code"),
        out::dim("(pointers only)")
    );
    out::field("CLAUDE.md", &yes_or_none(inventory.claude_md));
    out::field("skills", &skills_link(&inventory.claude.skills));
    out::field("hooks", &out::list(&wired(&inventory.claude.wired)));
    out::field("mcp", &format!("reads {}", paths::SHARED_MCP));

    println!("\n{}", out::bold("Codex"));
    out::field("AGENTS.md", &native_or_none(inventory.agents_md));
    out::field("skills", &format!("{} (native)", paths::AGENTS_SKILLS));
    out::field("hooks", &out::list(&wired(&inventory.codex.wired)));
    out::field("mcp", &out::list(&inventory.codex.servers));

    println!("\n{}", out::bold("Pi"));
    out::field("client", &pi_client(machine.pi_mcp));
    out::field("AGENTS.md", &native_or_none(inventory.agents_md));
    out::field("skills", &format!("{} (native)", paths::AGENTS_SKILLS));
    out::field("mcp", &pi_mcp_line(survey));

    report_attention(survey);
    report_legacy(survey);

    out::info(
        "'ai-toolbox recommend' suggests what to add for this stack; 'ai-toolbox list' shows everything.",
    );
    Ok(())
}

fn names(survey: &Survey, kind: Kind) -> Vec<String> {
    survey
        .report
        .items_of(kind)
        .iter()
        .map(|item| match &item.origin {
            // A name on its own reads as fine, so anything that is not gets a mark. This
            // is the difference between status telling you and status reassuring you.
            // Stale and modified get different marks because they mean opposite things:
            // one is safe to update, the other is somebody's work.
            Origin::Managed => item.name.clone(),
            Origin::Stale => out::yellow(&format!("{}~", item.name)),
            Origin::Modified => out::yellow(&format!("{}*", item.name)),
            Origin::Local => out::dim(&format!("{}+", item.name)),
            Origin::Broken { .. } => out::red(&format!("{}!", item.name)),
        })
        .collect()
}

fn wired(hooks: &[ai_toolbox_core::inventory::WiredHook]) -> Vec<String> {
    let mut scripts: Vec<String> = hooks
        .iter()
        .map(|h| h.script.trim_end_matches(".sh").to_string())
        .collect();
    scripts.sort();
    scripts.dedup();
    scripts
}

fn skills_link(link: &SkillsLink) -> String {
    match link {
        SkillsLink::Missing => out::none(),
        SkillsLink::Link {
            target,
            resolves: true,
        } => format!("-> {target}"),
        SkillsLink::Link {
            target,
            resolves: false,
        } => out::red(&format!(
            "-> {target} (dangling - Claude Code sees no skills here)"
        )),
        SkillsLink::OwnCopy { skills } => {
            out::yellow(&format!("own copy of {skills} - run: ai-toolbox migrate"))
        }
    }
}

fn pi_client(state: PiMcp) -> String {
    match state {
        PiMcp::Ready => format!("{} (global)", paths::PI_MCP_PACKAGE),
        PiMcp::Missing => out::yellow("not installed - run: ai-toolbox pi-init"),
        PiMcp::NoPi => out::dim("pi is not on this machine"),
    }
}

fn pi_mcp_line(survey: &Survey) -> String {
    let overrides = &survey.inventory.pi.overrides;
    if overrides.is_empty() {
        return format!("reads {}", paths::SHARED_MCP);
    }
    let names: Vec<&str> = overrides.iter().map(|s| s.name.as_str()).collect();
    format!(
        "reads {} + overrides: {}",
        paths::SHARED_MCP,
        names.join(" ")
    )
}

/// The legend, printed only when something in the output used it.
fn report_attention(survey: &Survey) {
    let attention = survey.report.needing_attention();
    let local = survey.report.local();
    if attention.is_empty() && local.is_empty() {
        return;
    }
    println!();
    for item in &attention {
        let line = format!("{} {}", item.kind.label(), item.name);
        match &item.origin {
            Origin::Broken { why } => out::warn(&format!("{line}: {why}")),
            Origin::Stale => out::warn(&format!(
                "{line}: an older version of the catalogue's copy - 'ai-toolbox doctor --fix' updates it"
            )),
            Origin::Modified => out::warn(&format!(
                "{line}: edited here - 'ai-toolbox doctor' says so, and leaves it alone"
            )),
            _ => {}
        }
    }
    if !local.is_empty() {
        let names: Vec<String> = local
            .iter()
            .map(|i| format!("{} {}", i.kind.label(), i.name))
            .collect();
        out::info(&format!("not from the catalogue: {}", names.join(", ")));
    }
}

fn report_legacy(survey: &Survey) {
    if survey.inventory.legacy.is_empty() {
        return;
    }
    println!();
    out::warn(&format!(
        "legacy per-harness copies still here: {}",
        survey.inventory.legacy.join(" ")
    ));
    out::warn("run 'ai-toolbox migrate' to fold them into .agents/ and re-point the configs.");
}

fn yes_or_none(present: bool) -> String {
    if present {
        "yes".to_string()
    } else {
        out::none()
    }
}

fn native_or_none(present: bool) -> String {
    if present {
        "native".to_string()
    } else {
        out::none()
    }
}
