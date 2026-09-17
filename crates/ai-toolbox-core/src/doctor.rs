//! What is wrong with this repo, and which of it can be put right.
//!
//! Two rules run through every check here.
//!
//! **A finding names the consequence, not the symptom.** "`.claude/skills` is a dangling
//! symlink" is a fact; "Claude Code sees no skills in this repo" is what the person
//! reading it needs to know. The failures this tool exists to catch are the quiet ones -
//! a hook that stopped running, a server that will not start - and they are quiet
//! precisely because nothing tells you the consequence.
//!
//! **Nothing that could be someone's own work is repaired.** An item edited in place
//! stays exactly as it is, and so does a `CLAUDE.md` that has stopped importing
//! `AGENTS.md`. Only things the catalogue can rebuild, or wiring that has come loose,
//! are touched by `--fix`.
//!
//! Telling those apart is what [`history`](crate::history) is for: an installed item
//! that matches an *older* catalogue version is out of date and safe to update, while
//! one that matches no version ever published is a local edit.

use std::path::{Path, PathBuf};

use crate::action::Action;
use crate::catalogue::Catalogue;
use crate::classify::{Kind, Origin, Report};
use crate::convert;
use crate::error::Result;
use crate::install::Plan;
use crate::inventory::{Inventory, SkillsLink};
use crate::{merge, paths};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Severity {
    /// Something is not working right now.
    Broken,
    /// Working, but not as intended, or about to stop.
    Warning,
    /// Worth knowing. Not a fault.
    Note,
}

impl Severity {
    pub fn label(self) -> &'static str {
        match self {
            Severity::Broken => "broken",
            Severity::Warning => "warning",
            Severity::Note => "note",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Code {
    SkillsLinkDangling,
    SkillsLinkElsewhere,
    SkillsOwnCopy,
    HookWiredButMissing,
    HookUnwired,
    NotExecutable,
    HelperMissing,
    CodexOutOfStep,
    LegacyLayout,
    AgentsMissing,
    ClaudeNotImporting,
    ItemStale,
    ItemModified,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct Finding {
    pub severity: Severity,
    pub code: Code,
    /// One line, saying what it means rather than what was observed.
    pub what: String,
    /// What to do about it, for the ones `--fix` will not touch.
    pub advice: Option<String>,
    pub path: PathBuf,
    pub repairable: bool,
}

impl Finding {
    fn new(
        severity: Severity,
        code: Code,
        path: impl Into<PathBuf>,
        what: impl Into<String>,
    ) -> Finding {
        Finding {
            severity,
            code,
            what: what.into(),
            advice: None,
            path: path.into(),
            repairable: false,
        }
    }

    fn repairable(mut self) -> Finding {
        self.repairable = true;
        self
    }

    fn advise(mut self, advice: impl Into<String>) -> Finding {
        self.advice = Some(advice.into());
        self
    }
}

/// Everything wrong with a repo, worst first.
pub fn diagnose(
    repo: &Path,
    inventory: &Inventory,
    report: &Report,
    catalogue: &Catalogue,
) -> Vec<Finding> {
    let mut found = Vec::new();

    // A repo nobody has set up is not a broken one. Without this, every unconfigured
    // directory on the machine would light up as faults in the board's project list.
    if inventory.is_empty() {
        return found;
    }

    skills_link(repo, inventory, &mut found);
    hooks(repo, inventory, catalogue, &mut found);
    executables(report, &mut found);
    helpers(report, &mut found);
    codex(repo, inventory, &mut found);
    legacy(repo, inventory, &mut found);
    knowledge(repo, inventory, &mut found);
    drifted(report, &mut found);

    found.sort_by_key(|f| f.severity);
    found
}

fn skills_link(repo: &Path, inventory: &Inventory, found: &mut Vec<Finding>) {
    let path = repo.join(paths::CLAUDE_SKILLS);
    match &inventory.claude.skills {
        SkillsLink::Link { resolves: false, target } => found.push(
            Finding::new(
                Severity::Broken,
                Code::SkillsLinkDangling,
                &path,
                format!(
                    "Claude Code sees no skills here: .claude/skills points at {target}, which does not exist"
                ),
            )
            .repairable(),
        ),
        SkillsLink::Link { target, .. } if target != paths::CLAUDE_SKILLS_TARGET => found.push(
            Finding::new(
                Severity::Warning,
                Code::SkillsLinkElsewhere,
                &path,
                format!(".claude/skills points at {target}, not the canonical {}", paths::CLAUDE_SKILLS_TARGET),
            )
            .advise("left alone in case it is deliberate - relink it by hand if not"),
        ),
        SkillsLink::OwnCopy { skills } => found.push(
            Finding::new(
                Severity::Warning,
                Code::SkillsOwnCopy,
                &path,
                format!(".claude/skills is a real directory holding {skills} skill(s), so they are maintained separately from .agents/skills"),
            )
            .advise("run 'ai-toolbox migrate' to fold them into .agents/skills and link it"),
        ),
        // Missing entirely is only a fault once there are skills to point at.
        SkillsLink::Missing if !inventory.skills.is_empty() => found.push(
            Finding::new(
                Severity::Broken,
                Code::SkillsLinkDangling,
                &path,
                format!(
                    "Claude Code sees none of the {} skill(s) here: .claude/skills does not exist",
                    inventory.skills.len()
                ),
            )
            .repairable(),
        ),
        _ => {}
    }
}

fn hooks(repo: &Path, inventory: &Inventory, catalogue: &Catalogue, found: &mut Vec<Finding>) {
    for (harness, wired) in inventory.all_wired() {
        let script = wired.script.trim_end_matches(".sh");
        if inventory.hook(script).is_some() {
            continue;
        }
        // A command that is not one of ours - someone's own script - is their business.
        if !wired.command.contains(paths::AGENTS_HOOKS) {
            continue;
        }
        let known = catalogue.hook(script).is_some();
        let mut finding = Finding::new(
            Severity::Broken,
            Code::HookWiredButMissing,
            repo.join(paths::AGENTS_HOOKS).join(&wired.script),
            format!(
                "{harness} is wired to run {} on {}, but the script is not in {}",
                wired.script,
                wired.event,
                paths::AGENTS_HOOKS
            ),
        );
        if known {
            finding = finding.repairable();
        } else {
            finding = finding.advise(
                "not in the catalogue either - remove the wiring by hand, or restore the script",
            );
        }
        found.push(finding);
    }

    // The other direction: installed and doing nothing. Not broken - nothing has
    // stopped working - but it is almost certainly not what was intended.
    for hook in &inventory.hooks {
        let wired = inventory
            .all_wired()
            .iter()
            .any(|(_, w)| w.script.trim_end_matches(".sh") == hook.name);
        if wired {
            continue;
        }
        found.push(
            Finding::new(
                Severity::Warning,
                Code::HookUnwired,
                &hook.path,
                format!(
                    "the {} hook is installed but wired to no harness, so it never runs",
                    hook.name
                ),
            )
            .repairable(),
        );
    }
}

fn executables(report: &Report, found: &mut Vec<Finding>) {
    for item in &report.items {
        let Origin::Broken { why } = &item.origin else {
            continue;
        };
        if !why.contains("not executable") {
            continue;
        }
        found.push(
            Finding::new(
                Severity::Broken,
                Code::NotExecutable,
                &item.path,
                format!(
                    "{} {} cannot run: it has lost its executable bit",
                    item.kind.label(),
                    item.name
                ),
            )
            .repairable(),
        );
    }
}

fn helpers(report: &Report, found: &mut Vec<Finding>) {
    for item in report.items_of(Kind::Server) {
        let Origin::Broken { why } = &item.origin else {
            continue;
        };
        if !why.contains("launches through") {
            continue;
        }
        found.push(
            Finding::new(
                Severity::Broken,
                Code::HelperMissing,
                &item.path,
                format!("the {} server will not start: it {why}", item.name),
            )
            .repairable(),
        );
    }
}

/// Codex cannot read `.mcp.json`, so its config is a generated copy - and a generated
/// copy that has fallen behind its source is a server the user believes they have.
///
/// Only servers that `.mcp.json` names are checked. A server added to `.codex/config.toml`
/// by hand is a Codex-only server, which is a legitimate thing to have.
fn codex(repo: &Path, inventory: &Inventory, found: &mut Vec<Finding>) {
    if !inventory.codex.config {
        return;
    }
    let path = repo.join(paths::CODEX_CONFIG);
    let Ok(text) = std::fs::read_to_string(&path) else {
        return;
    };
    let Ok(config) = toml::from_str::<toml::Value>(&text) else {
        return;
    };
    let tables = config.get("mcp_servers").and_then(|s| s.as_table());

    let mut missing = Vec::new();
    let mut stale = Vec::new();
    for server in &inventory.servers {
        let (expected, _) = convert::codex::server(&server.name, &server.definition);
        let Some(actual) = tables.and_then(|t| t.get(&server.name)) else {
            missing.push(server.name.clone());
            continue;
        };
        let Ok(expected) = convert::to_toml(&expected) else {
            continue;
        };
        if &expected != actual {
            stale.push(server.name.clone());
        }
    }
    if missing.is_empty() && stale.is_empty() {
        return;
    }

    let mut parts = Vec::new();
    if !missing.is_empty() {
        parts.push(format!("missing {}", missing.join(", ")));
    }
    if !stale.is_empty() {
        parts.push(format!("out of date for {}", stale.join(", ")));
    }
    found.push(
        Finding::new(
            Severity::Broken,
            Code::CodexOutOfStep,
            &path,
            format!(
                "Codex will not see what {} says it should: its generated config is {}",
                paths::SHARED_MCP,
                parts.join(" and ")
            ),
        )
        .repairable(),
    );
}

fn legacy(repo: &Path, inventory: &Inventory, found: &mut Vec<Finding>) {
    if inventory.legacy.is_empty() {
        return;
    }
    found.push(
        Finding::new(
            Severity::Warning,
            Code::LegacyLayout,
            repo,
            format!(
                "still on the per-harness layout: {} - those copies drift from .agents/",
                inventory.legacy.join(" ")
            ),
        )
        .advise("run 'ai-toolbox migrate' to fold them into .agents/ and re-point the configs"),
    );
}

fn knowledge(repo: &Path, inventory: &Inventory, found: &mut Vec<Finding>) {
    if !inventory.agents_md {
        found.push(
            Finding::new(
                Severity::Warning,
                Code::AgentsMissing,
                repo.join("AGENTS.md"),
                "this repo is configured but has no AGENTS.md, so every harness starts with no project knowledge",
            )
            .repairable(),
        );
    }

    // CLAUDE.md exists only to pull AGENTS.md in. One that has stopped doing so leaves
    // Claude Code reading nothing while the file sits there looking correct.
    if !inventory.claude_md || !inventory.agents_md {
        return;
    }
    let path = repo.join("CLAUDE.md");
    let Ok(text) = std::fs::read_to_string(&path) else {
        return;
    };
    if text.contains("@AGENTS.md") {
        return;
    }
    found.push(
        Finding::new(
            Severity::Warning,
            Code::ClaudeNotImporting,
            &path,
            "CLAUDE.md does not import AGENTS.md, so Claude Code reads none of it",
        )
        .advise("add a line reading @AGENTS.md - left alone because this file is yours"),
    );
}

/// Items that differ from the catalogue.
///
/// Which of the two kinds of difference this is was already settled by `classify`, using
/// the clone's history - so it is read here rather than worked out a second time.
fn drifted(report: &Report, found: &mut Vec<Finding>) {
    for item in &report.items {
        match item.origin {
            Origin::Stale => found.push(
                Finding::new(
                    Severity::Note,
                    Code::ItemStale,
                    &item.path,
                    format!(
                        "{} {} is an older version of the catalogue's copy",
                        item.kind.label(),
                        item.name
                    ),
                )
                .repairable(),
            ),
            Origin::Modified => found.push(
                Finding::new(
                    Severity::Note,
                    Code::ItemModified,
                    &item.path,
                    format!(
                        "{} {} has been edited here and is no version the catalogue ever shipped",
                        item.kind.label(),
                        item.name
                    ),
                )
                .advise(
                    "left alone - this is your edit. Reinstall it to take the catalogue's copy",
                ),
            ),
            _ => {}
        }
    }
}

/// Turn the repairable findings into writes. Anything else is left exactly as it is.
pub fn repair(
    plan: &mut Plan,
    repo: &Path,
    inventory: &Inventory,
    report: &Report,
    catalogue: &Catalogue,
    findings: &[Finding],
) -> Result<()> {
    for finding in findings.iter().filter(|f| f.repairable) {
        match finding.code {
            Code::SkillsLinkDangling => {
                // The target first, then the link - recreating the link over a missing
                // target would produce a fresh dangling one.
                plan.push(Action::directory(
                    repo.join(paths::AGENTS_SKILLS),
                    format!("create {}", paths::AGENTS_SKILLS),
                )?);
                plan.push(Action::symlink(
                    repo.join(paths::CLAUDE_SKILLS),
                    paths::CLAUDE_SKILLS_TARGET,
                    format!(
                        "link  {}  ->  {}",
                        paths::CLAUDE_SKILLS,
                        paths::CLAUDE_SKILLS_TARGET
                    ),
                )?);
            }
            Code::HookWiredButMissing => {
                let name = file_stem(&finding.path);
                crate::install::hooks(plan, repo, catalogue, &[name], &configured(inventory))?;
            }
            Code::HookUnwired => {
                let name = file_stem(&finding.path);
                crate::install::hooks(plan, repo, catalogue, &[name], &configured(inventory))?;
            }
            Code::NotExecutable => {
                // Same bytes, executable bit restored. Reading the file rather than
                // taking the catalogue's copy keeps a local edit intact: the fault here
                // is the permission, not the content.
                let contents =
                    std::fs::read(&finding.path).map_err(|e| crate::Error::io(&finding.path, e))?;
                plan.push(Action::write_executable(
                    &finding.path,
                    contents,
                    format!("chmod +x {}", relative_to(&finding.path, repo)),
                )?);
            }
            Code::HelperMissing => repair_helpers(plan, repo, inventory, catalogue)?,
            Code::CodexOutOfStep => repair_codex(plan, repo, inventory)?,
            Code::AgentsMissing => {
                crate::install::knowledge_files(plan, repo, catalogue, &configured(inventory))?;
            }
            Code::ItemStale => repair_stale(plan, repo, report, catalogue, &finding.path)?,
            // Everything else is either someone's own work or needs migrate.
            Code::SkillsLinkElsewhere
            | Code::SkillsOwnCopy
            | Code::LegacyLayout
            | Code::ClaudeNotImporting
            | Code::ItemModified => {}
        }
    }
    Ok(())
}

/// Which harnesses this repo uses, so a repair wires for the same set that was there.
fn configured(inventory: &Inventory) -> Vec<crate::Harness> {
    let mut harnesses = crate::harness::configured(&inventory.repo);
    if harnesses.is_empty() {
        harnesses.push(crate::Harness::Claude);
    }
    harnesses
}

fn repair_helpers(
    plan: &mut Plan,
    repo: &Path,
    inventory: &Inventory,
    catalogue: &Catalogue,
) -> Result<()> {
    for server in &inventory.servers {
        for name in &server.helpers {
            if inventory.helper(name).is_some() {
                continue;
            }
            let Some(helper) = catalogue.helper(name) else {
                continue;
            };
            plan.push(Action::write_executable(
                repo.join(paths::AGENTS_MCP).join(name),
                std::fs::read(&helper.path).map_err(|e| crate::Error::io(&helper.path, e))?,
                format!("mcp   {name}  ->  {}/", paths::AGENTS_MCP),
            )?);
        }
    }
    Ok(())
}

/// Regenerate Codex's servers from `.mcp.json`, the source of truth - leaving any
/// Codex-only server in the file untouched.
fn repair_codex(plan: &mut Plan, repo: &Path, inventory: &Inventory) -> Result<()> {
    let path = repo.join(paths::CODEX_CONFIG);
    let mut config = plan.read_toml_public(&path)?;
    let mut servers = serde_json::Map::new();
    for server in &inventory.servers {
        let (converted, _) = convert::codex::server(&server.name, &server.definition);
        servers.insert(server.name.clone(), converted);
    }
    convert::codex::merge_servers(&mut config, &servers)?;
    plan.push(Action::write(
        &path,
        toml::to_string(&config)
            .map_err(|e| crate::Error::Catalogue(format!("serialising the Codex config: {e}")))?,
        format!(
            "codex {} regenerated from {}",
            paths::CODEX_CONFIG,
            paths::SHARED_MCP
        ),
    )?);
    Ok(())
}

fn repair_stale(
    plan: &mut Plan,
    repo: &Path,
    report: &Report,
    catalogue: &Catalogue,
    path: &Path,
) -> Result<()> {
    let Some(item) = report.items.iter().find(|i| i.path == path) else {
        return Ok(());
    };
    let Some(key) = item.catalogue_key.as_deref() else {
        return Ok(());
    };
    match item.kind {
        Kind::Skill => {
            let skill = catalogue
                .skill(key)
                .expect("classified against this catalogue");
            plan.push(Action::copy_tree(
                &skill.path,
                repo.join(paths::AGENTS_SKILLS).join(&skill.name),
                format!("skill {} updated to the catalogue's copy", skill.key),
            )?);
        }
        Kind::Hook => {
            let hook = catalogue
                .hook(key)
                .expect("classified against this catalogue");
            plan.push(Action::write_executable(
                repo.join(paths::AGENTS_HOOKS).join(format!("{key}.sh")),
                std::fs::read(&hook.path).map_err(|e| crate::Error::io(&hook.path, e))?,
                format!("hook  {key} updated to the catalogue's copy"),
            )?);
        }
        Kind::Helper => {
            let helper = catalogue
                .helper(key)
                .expect("classified against this catalogue");
            plan.push(Action::write_executable(
                repo.join(paths::AGENTS_MCP).join(key),
                std::fs::read(&helper.path).map_err(|e| crate::Error::io(&helper.path, e))?,
                format!("mcp   {key} updated to the catalogue's copy"),
            )?);
        }
        Kind::Server => {}
    }
    Ok(())
}

fn file_stem(path: &Path) -> String {
    path.file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned()
}

fn relative_to(path: &Path, repo: &Path) -> String {
    path.strip_prefix(repo)
        .unwrap_or(path)
        .to_string_lossy()
        .into_owned()
}

/// Exposed so the CLI can show a repair's effect on `.mcp.json` without rebuilding it.
pub fn render_json(document: &serde_json::Value) -> String {
    merge::to_string(document)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action;
    use crate::testing::{catalogue, Fixture};

    fn run(fixture: &Fixture) -> (Inventory, Report, Vec<Finding>) {
        let catalogue = catalogue();
        let inventory = fixture.inventory();
        let report = crate::classify(&inventory, &catalogue);
        let findings = diagnose(fixture.path(), &inventory, &report, &catalogue);
        (inventory, report, findings)
    }

    fn codes(findings: &[Finding]) -> Vec<Code> {
        findings.iter().map(|f| f.code).collect()
    }

    /// Diagnose, repair, and diagnose again.
    fn fix(fixture: &Fixture) -> Vec<Finding> {
        let catalogue = catalogue();
        let (inventory, report, findings) = run(fixture);
        let mut plan = Plan::default();
        repair(
            &mut plan,
            fixture.path(),
            &inventory,
            &report,
            &catalogue,
            &findings,
        )
        .unwrap();
        let outcome = action::apply(&plan.actions).unwrap();
        assert!(
            outcome.stale.is_empty(),
            "repair collided: {:?}",
            outcome.stale
        );
        run(fixture).2
    }

    #[test]
    fn a_healthy_repo_has_nothing_to_report() {
        let fixture = Fixture::configured();
        assert!(run(&fixture).2.is_empty(), "{:?}", run(&fixture).2);
    }

    #[test]
    fn an_unconfigured_directory_is_not_a_pile_of_faults() {
        // Every repo on the machine gets diagnosed for the project list. One nobody has
        // set up must come back clean, or the list is noise.
        let fixture = Fixture::bare();
        assert!(run(&fixture).2.is_empty());
    }

    #[test]
    fn a_dangling_skills_link_is_broken_and_is_repaired() {
        let fixture = Fixture::configured();
        fixture.remove(".agents/skills");

        let findings = run(&fixture).2;
        assert_eq!(codes(&findings), vec![Code::SkillsLinkDangling]);
        assert_eq!(findings[0].severity, Severity::Broken);
        assert!(
            findings[0].what.contains("sees no skills"),
            "{}",
            findings[0].what
        );

        assert!(fix(&fixture).is_empty());
        assert!(fixture.path().join(".agents/skills").is_dir());
        assert!(std::fs::metadata(fixture.path().join(".claude/skills")).is_ok());
    }

    #[test]
    fn a_wired_hook_whose_script_is_gone_is_broken_and_is_reinstalled() {
        let fixture = Fixture::configured();
        fixture.remove(".agents/hooks/format-on-edit.sh");

        let findings = run(&fixture).2;
        assert!(codes(&findings).contains(&Code::HookWiredButMissing));
        let finding = findings
            .iter()
            .find(|f| f.code == Code::HookWiredButMissing)
            .unwrap();
        assert!(finding.repairable);
        assert!(finding.what.contains("PostToolUse"), "{}", finding.what);

        assert!(fix(&fixture).is_empty());
        assert!(fixture
            .path()
            .join(".agents/hooks/format-on-edit.sh")
            .is_file());
    }

    #[test]
    fn a_hook_wired_to_nothing_is_a_warning_and_gets_wired() {
        let fixture = Fixture::configured();
        fixture.install_hook(&catalogue(), "guard-irreversible");

        let findings = run(&fixture).2;
        let finding = findings
            .iter()
            .find(|f| f.code == Code::HookUnwired)
            .unwrap();
        assert_eq!(finding.severity, Severity::Warning);
        assert!(finding.what.contains("never runs"), "{}", finding.what);

        assert!(fix(&fixture).is_empty());
        let settings =
            std::fs::read_to_string(fixture.path().join(".claude/settings.json")).unwrap();
        assert!(settings.contains("guard-irreversible.sh"));
    }

    #[test]
    fn someone_elses_hook_in_the_wiring_is_not_reported() {
        let fixture = Fixture::configured();
        fixture.write_json(
            ".claude/settings.json",
            serde_json::json!({
                "hooks": {
                    "PreToolUse": [{
                        "matcher": "Bash",
                        "hooks": [{ "type": "command", "command": "./scripts/our-own-check.sh" }]
                    }]
                }
            }),
        );
        let findings = run(&fixture).2;
        assert!(
            !codes(&findings).contains(&Code::HookWiredButMissing),
            "a script outside .agents/hooks is not this tool's business: {findings:?}"
        );
    }

    #[test]
    fn a_hook_that_lost_its_executable_bit_is_repaired_without_losing_a_local_edit() {
        use std::os::unix::fs::PermissionsExt;
        let fixture = Fixture::configured();
        let hook = fixture.path().join(".agents/hooks/format-on-edit.sh");
        fixture.append(".agents/hooks/format-on-edit.sh", "\n# our own tweak\n");
        std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o644)).unwrap();

        let findings = run(&fixture).2;
        assert!(codes(&findings).contains(&Code::NotExecutable));

        let after = fix(&fixture);
        assert!(!codes(&after).contains(&Code::NotExecutable));
        // The fault was the permission, so the content is none of the repair's business.
        assert!(
            std::fs::read_to_string(&hook)
                .unwrap()
                .contains("our own tweak"),
            "repairing a permission must not overwrite an edit"
        );
    }

    #[test]
    fn a_server_whose_launcher_is_missing_is_repaired_by_installing_it() {
        let fixture = Fixture::configured();
        let catalogue = catalogue();
        let preset = catalogue.preset("supabase").unwrap();
        fixture.write_mcp(serde_json::json!({ "mcpServers": preset.servers }));

        let findings = run(&fixture).2;
        assert!(codes(&findings).contains(&Code::HelperMissing));
        assert!(findings
            .iter()
            .find(|f| f.code == Code::HelperMissing)
            .unwrap()
            .what
            .contains("will not start"));

        assert!(!codes(&fix(&fixture)).contains(&Code::HelperMissing));
        assert!(fixture.path().join(".agents/mcp/with-dotenv.sh").is_file());
    }

    #[test]
    fn a_codex_config_that_has_fallen_behind_is_regenerated() {
        let fixture = Fixture::configured();
        // A server added to .mcp.json without Codex being updated - exactly what an
        // agent editing the file by hand leaves behind.
        fixture.write_mcp(serde_json::json!({
            "mcpServers": {
                "context7": { "command": "npx", "args": ["-y", "@upstash/context7-mcp@latest"] },
                "playwright": { "command": "npx", "args": ["-y", "@playwright/mcp@latest"] }
            }
        }));

        let findings = run(&fixture).2;
        let finding = findings
            .iter()
            .find(|f| f.code == Code::CodexOutOfStep)
            .unwrap();
        assert!(finding.what.contains("playwright"), "{}", finding.what);

        assert!(!codes(&fix(&fixture)).contains(&Code::CodexOutOfStep));
        let config = std::fs::read_to_string(fixture.path().join(".codex/config.toml")).unwrap();
        assert!(config.contains("[mcp_servers.playwright]"));
        // The hook wiring already in the file is not collateral damage.
        assert!(config.contains("format-on-edit.sh"), "{config}");
    }

    #[test]
    fn a_codex_only_server_is_not_reported_as_drift() {
        let fixture = Fixture::configured();
        fixture.append(
            ".codex/config.toml",
            "\n[mcp_servers.codex-only]\ncommand = \"node\"\nargs = [\"x.js\"]\n",
        );
        let findings = run(&fixture).2;
        assert!(
            !codes(&findings).contains(&Code::CodexOutOfStep),
            "a server only Codex has is a legitimate thing to have: {findings:?}"
        );
    }

    #[test]
    fn a_legacy_layout_is_reported_with_the_command_that_resolves_it() {
        let fixture = Fixture::configured();
        std::fs::create_dir_all(fixture.path().join(".claude/hooks")).unwrap();

        let findings = run(&fixture).2;
        let finding = findings
            .iter()
            .find(|f| f.code == Code::LegacyLayout)
            .unwrap();
        assert!(
            !finding.repairable,
            "migrate is its own command, not a --fix"
        );
        assert!(finding.advice.as_ref().unwrap().contains("migrate"));
    }

    #[test]
    fn a_configured_repo_without_agents_md_gets_one_scaffolded() {
        let fixture = Fixture::configured();
        fixture.remove("AGENTS.md");

        assert!(codes(&run(&fixture).2).contains(&Code::AgentsMissing));
        assert!(!codes(&fix(&fixture)).contains(&Code::AgentsMissing));
        assert!(fixture.path().join("AGENTS.md").is_file());
    }

    #[test]
    fn a_claude_md_that_stopped_importing_agents_md_is_reported_but_not_touched() {
        let fixture = Fixture::configured();
        fixture.write("CLAUDE.md", "# notes\n\nsomeone deleted the import\n");

        let findings = run(&fixture).2;
        let finding = findings
            .iter()
            .find(|f| f.code == Code::ClaudeNotImporting)
            .unwrap();
        assert!(!finding.repairable);

        fix(&fixture);
        assert_eq!(
            std::fs::read_to_string(fixture.path().join("CLAUDE.md")).unwrap(),
            "# notes\n\nsomeone deleted the import\n",
            "CLAUDE.md is the user's file"
        );
    }

    #[test]
    fn an_older_catalogue_version_is_stale_and_is_updated() {
        let fixture = Fixture::configured();
        let catalogue = catalogue();
        // The version of skills/handoff before a398dd6 - a real former version, taken
        // from the clone's own history rather than invented.
        let previous = crate::history::versions(&catalogue.root, "skills/handoff");
        assert!(
            previous.len() >= 2,
            "the fixture needs handoff to have changed"
        );

        let out = std::process::Command::new("git")
            .arg("-C")
            .arg(&catalogue.root)
            .args(["log", "-n2", "--format=%H", "--", "skills/handoff"])
            .output()
            .unwrap();
        let older = String::from_utf8_lossy(&out.stdout)
            .lines()
            .nth(1)
            .unwrap()
            .to_string();
        let body = std::process::Command::new("git")
            .arg("-C")
            .arg(&catalogue.root)
            .arg("show")
            .arg(format!("{older}:skills/handoff/SKILL.md"))
            .output()
            .unwrap();
        fixture.write(
            ".agents/skills/handoff/SKILL.md",
            &String::from_utf8_lossy(&body.stdout),
        );

        let findings = run(&fixture).2;
        let finding = findings.iter().find(|f| f.code == Code::ItemStale).unwrap();
        assert!(finding.repairable);
        assert!(finding.what.contains("older version"), "{}", finding.what);

        let after = fix(&fixture);
        assert!(!codes(&after).contains(&Code::ItemStale));
        assert!(!codes(&after).contains(&Code::ItemModified));
    }

    #[test]
    fn a_local_edit_is_never_repaired() {
        let fixture = Fixture::configured();
        fixture.append(".agents/skills/pre-pr/SKILL.md", "\n## our own step\n");

        let findings = run(&fixture).2;
        let finding = findings
            .iter()
            .find(|f| f.code == Code::ItemModified)
            .unwrap();
        assert!(!finding.repairable);
        assert!(finding.advice.as_ref().unwrap().contains("your edit"));

        fix(&fixture);
        assert!(
            std::fs::read_to_string(fixture.path().join(".agents/skills/pre-pr/SKILL.md"))
                .unwrap()
                .contains("our own step"),
            "--fix overwrote a local edit"
        );
    }

    #[test]
    fn six_faults_at_once_are_all_found_and_the_repairable_ones_all_fixed() {
        use std::os::unix::fs::PermissionsExt;
        let fixture = Fixture::configured();
        let catalogue = catalogue();

        fixture.remove(".agents/hooks/format-on-edit.sh");
        std::fs::set_permissions(
            fixture.path().join(".agents/hooks/session-context.sh"),
            std::fs::Permissions::from_mode(0o644),
        )
        .unwrap();
        fixture.write_mcp(serde_json::json!({
            "mcpServers": catalogue.preset("supabase").unwrap().servers
        }));
        fixture.remove("AGENTS.md");
        fixture.append(".agents/skills/pre-pr/SKILL.md", "\n## our own step\n");
        std::fs::create_dir_all(fixture.path().join(".codex/hooks")).unwrap();

        let findings = run(&fixture).2;
        let seen = codes(&findings);
        for expected in [
            Code::HookWiredButMissing,
            Code::NotExecutable,
            Code::HelperMissing,
            Code::CodexOutOfStep,
            Code::AgentsMissing,
            Code::ItemModified,
            Code::LegacyLayout,
        ] {
            assert!(seen.contains(&expected), "missed {expected:?} in {seen:?}");
        }
        // Worst first, so the broken ones are what you read.
        assert_eq!(findings[0].severity, Severity::Broken);

        let after = fix(&fixture);
        let left = codes(&after);
        // The two that are nobody's business but the user's, and nothing else.
        assert!(left.contains(&Code::ItemModified));
        assert!(left.contains(&Code::LegacyLayout));
        assert!(
            !left.iter().any(|c| matches!(
                c,
                Code::HookWiredButMissing
                    | Code::NotExecutable
                    | Code::HelperMissing
                    | Code::CodexOutOfStep
                    | Code::AgentsMissing
            )),
            "something repairable survived --fix: {left:?}"
        );
    }
}
