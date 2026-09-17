//! Holding a repo up against the catalogue (D5).
//!
//! One comparison answers both of the questions worth asking - "what here did not come
//! from the toolkit?" and "what here has stopped working?" - so there is one result
//! type. Every installed item lands in exactly one `Origin`, and `Local` is a perfectly
//! good place to land: a hand-written skill is a thing people have, not a fault.
//!
//! Telling a local edit apart from an item that is simply out of date needs the clone's
//! git log, so that question is asked of [`crate::history`] - but only for items that
//! already differ, which in a healthy repo is none of them. It is asked here rather than
//! in `doctor` so that one piece of code decides it and every caller agrees.

use std::path::PathBuf;

use crate::catalogue::Catalogue;
use crate::hash::Hash;
use crate::history;
use crate::inventory::{Inventory, SkillsLink};

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(tag = "state", rename_all = "kebab-case")]
pub enum Origin {
    /// From the catalogue, unchanged.
    Managed,
    /// From the catalogue, edited since, and matching no version the catalogue ever
    /// shipped. Someone's own change. Reported, never overwritten without being asked.
    Modified,
    /// From the catalogue, and matching a version it *used* to ship. Out of date rather
    /// than edited, so updating it loses nothing.
    Stale,
    /// Not in the catalogue at all. Someone's own work.
    Local,
    /// Referenced but not usable.
    Broken { why: String },
}

impl Origin {
    pub fn is_managed(&self) -> bool {
        matches!(self, Origin::Managed)
    }

    pub fn needs_attention(&self) -> bool {
        matches!(
            self,
            Origin::Modified | Origin::Stale | Origin::Broken { .. }
        )
    }

    pub fn label(&self) -> &'static str {
        match self {
            Origin::Managed => "managed",
            Origin::Modified => "modified",
            Origin::Stale => "stale",
            Origin::Local => "local",
            Origin::Broken { .. } => "broken",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Kind {
    Hook,
    Skill,
    Server,
    Helper,
}

impl Kind {
    pub fn label(self) -> &'static str {
        match self {
            Kind::Hook => "hook",
            Kind::Skill => "skill",
            Kind::Server => "server",
            Kind::Helper => "helper",
        }
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct Item {
    pub kind: Kind,
    pub name: String,
    /// The catalogue entry this came from, where there is one. For a group skill this is
    /// the full key (`cli/gh`) while `name` is the installed directory (`gh`).
    pub catalogue_key: Option<String>,
    pub origin: Origin,
    pub path: PathBuf,
    pub description: Option<String>,
    pub hash: Hash,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct Report {
    pub items: Vec<Item>,
    /// Catalogue entries a repo could add, by kind - what the GUI offers to install.
    pub available: Available,
}

#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct Available {
    pub hooks: Vec<String>,
    pub skills: Vec<String>,
    pub presets: Vec<String>,
}

impl Report {
    pub fn of(kind: Kind) -> impl Fn(&&Item) -> bool {
        move |item| item.kind == kind
    }

    pub fn items_of(&self, kind: Kind) -> Vec<&Item> {
        self.items.iter().filter(Report::of(kind)).collect()
    }

    pub fn local(&self) -> Vec<&Item> {
        self.items
            .iter()
            .filter(|i| matches!(i.origin, Origin::Local))
            .collect()
    }

    pub fn needing_attention(&self) -> Vec<&Item> {
        self.items
            .iter()
            .filter(|i| i.origin.needs_attention())
            .collect()
    }

    pub fn counts(&self) -> Counts {
        let mut counts = Counts::default();
        for item in &self.items {
            match item.origin {
                Origin::Managed => counts.managed += 1,
                Origin::Modified => counts.modified += 1,
                Origin::Stale => counts.stale += 1,
                Origin::Local => counts.local += 1,
                Origin::Broken { .. } => counts.broken += 1,
            }
        }
        counts
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize)]
pub struct Counts {
    pub managed: usize,
    pub modified: usize,
    pub stale: usize,
    pub local: usize,
    pub broken: usize,
}

impl Counts {
    pub fn total(&self) -> usize {
        self.managed + self.modified + self.stale + self.local + self.broken
    }
}

pub fn classify(inventory: &Inventory, catalogue: &Catalogue) -> Report {
    let mut items = Vec::new();

    for hook in &inventory.hooks {
        let entry = catalogue.hook(&hook.name);
        // A hook that cannot execute will not run, whatever its content says - so that
        // beats comparing it to the catalogue.
        let origin = if !hook.executable {
            Origin::Broken {
                why: "not executable - chmod +x, or reinstall it".to_string(),
            }
        } else {
            match entry {
                Some(def) if def.hash == hook.hash => Origin::Managed,
                Some(_) => drifted(catalogue, &format!("hooks/{}.sh", hook.name), &hook.hash),
                None => Origin::Local,
            }
        };
        items.push(Item {
            kind: Kind::Hook,
            name: hook.name.clone(),
            catalogue_key: entry.map(|d| d.name.clone()),
            origin,
            path: hook.path.clone(),
            description: entry.and_then(|d| d.summary.clone()),
            hash: hook.hash.clone(),
        });
    }

    for skill in &inventory.skills {
        let entry = catalogue.skill(&skill.name);
        let origin = match entry {
            Some(def) if def.hash == skill.hash => Origin::Managed,
            Some(def) => drifted(catalogue, &format!("skills/{}", def.key), &skill.hash),
            None => Origin::Local,
        };
        items.push(Item {
            kind: Kind::Skill,
            name: skill.name.clone(),
            catalogue_key: entry.map(|d| d.key.clone()),
            origin,
            path: skill.path.clone(),
            // A local skill describes itself; a managed one is described by the
            // catalogue, which is the copy that gets updated.
            description: entry
                .and_then(|d| d.description.clone())
                .or_else(|| skill.description.clone()),
            hash: skill.hash.clone(),
        });
    }

    for helper in &inventory.helpers {
        let entry = catalogue.helper(&helper.name);
        let origin = if !helper.executable {
            Origin::Broken {
                why: "not executable - the server it launches will fail to start".to_string(),
            }
        } else {
            match entry {
                Some(def) if def.hash == helper.hash => Origin::Managed,
                Some(_) => drifted(catalogue, &format!("mcp/{}", helper.name), &helper.hash),
                None => Origin::Local,
            }
        };
        items.push(Item {
            kind: Kind::Helper,
            name: helper.name.clone(),
            catalogue_key: entry.map(|d| d.name.clone()),
            origin,
            path: helper.path.clone(),
            description: None,
            hash: helper.hash.clone(),
        });
    }

    for server in &inventory.servers {
        let preset = catalogue.preset_for_server(&server.name);
        // A helper that is not on disk is a server that cannot start, and the error it
        // produces at connect time names the missing file rather than the server.
        let missing: Vec<&String> = server
            .helpers
            .iter()
            .filter(|name| inventory.helper(name).is_none())
            .collect();
        let origin = if !missing.is_empty() {
            Origin::Broken {
                why: format!(
                    "launches through {}, which is not in {}",
                    missing
                        .iter()
                        .map(|s| s.as_str())
                        .collect::<Vec<_>>()
                        .join(", "),
                    crate::paths::AGENTS_MCP
                ),
            }
        } else {
            match preset {
                Some(def) => {
                    // Pi's installer adds `lifecycle` and `directTools` to the shared
                    // file, so a Pi-configured repo would otherwise read as modified
                    // against every preset it has. Those two keys are the installer's
                    // own, not a local edit.
                    let stripped = without_pi_keys(&server.definition);
                    let catalogue_definition = def.servers.get(&server.name);
                    if catalogue_definition.map(crate::hash::json)
                        == Some(crate::hash::json(&stripped))
                    {
                        Origin::Managed
                    } else {
                        Origin::Modified
                    }
                }
                None => Origin::Local,
            }
        };
        items.push(Item {
            kind: Kind::Server,
            name: server.name.clone(),
            catalogue_key: preset.map(|p| p.name.clone()),
            origin,
            path: inventory.repo.join(crate::paths::SHARED_MCP),
            description: None,
            hash: server.hash.clone(),
        });
    }

    items.sort_by(|a, b| (a.kind, &a.name).cmp(&(b.kind, &b.name)));

    Report {
        available: available(inventory, catalogue),
        items,
    }
}

/// An item that differs from the catalogue: is it out of date, or edited here?
///
/// Answered from the clone's git history. Where there is no history to read - a tarball
/// install, or git missing - every difference reads as `Modified`, which is the safe
/// direction: it is the one that leaves the file alone and asks a human.
fn drifted(catalogue: &Catalogue, relative: &str, installed: &Hash) -> Origin {
    if history::is_former_version(&catalogue.root, relative, installed) {
        Origin::Stale
    } else {
        Origin::Modified
    }
}

/// Pi's two knobs, which `ai-toolbox mcp --harness pi` adds to the shared `.mcp.json`
/// and which therefore are not evidence of a hand edit.
fn without_pi_keys(definition: &serde_json::Value) -> serde_json::Value {
    let mut stripped = definition.clone();
    if let Some(map) = stripped.as_object_mut() {
        map.remove("lifecycle");
        map.remove("directTools");
    }
    stripped
}

fn available(inventory: &Inventory, catalogue: &Catalogue) -> Available {
    Available {
        hooks: catalogue
            .hooks
            .iter()
            .filter(|h| inventory.hook(&h.name).is_none())
            .map(|h| h.name.clone())
            .collect(),
        skills: catalogue
            .skills
            .iter()
            .filter(|s| inventory.skill(&s.name).is_none())
            .map(|s| s.key.clone())
            .collect(),
        // A preset counts as present once every server it ships is in the repo. A
        // half-installed multi-server preset is still worth offering.
        presets: catalogue
            .presets
            .iter()
            .filter(|p| {
                !p.servers
                    .keys()
                    .all(|name| inventory.server(name).is_some())
            })
            .map(|p| p.name.clone())
            .collect(),
    }
}

/// The one-line verdict a project list needs, without rendering the whole report.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum State {
    /// Nothing from the toolkit is here.
    Unconfigured,
    /// Set up, and everything checks out.
    Healthy,
    /// Set up, with something modified or local worth a look.
    Attention,
    /// Set up, with something that is not working.
    Broken,
}

pub fn state(inventory: &Inventory, report: &Report) -> State {
    if inventory.is_empty() {
        return State::Unconfigured;
    }
    let counts = report.counts();
    // A dangling skills link is breakage the item counts cannot see: every item is fine,
    // and Claude Code still reaches none of them.
    let cut_off = matches!(
        inventory.claude.skills,
        SkillsLink::Link {
            resolves: false,
            ..
        }
    );
    if counts.broken > 0 || cut_off {
        return State::Broken;
    }
    // A legacy layout is not breakage. The hooks under `.claude/hooks` are wired to
    // `.claude/hooks` and still run - the repo works, it is just on the old shape and
    // will drift. Doctor rates it a warning, and these two must not disagree: a project
    // list that calls a working repo broken is a list people stop believing.
    if counts.modified > 0 || counts.stale > 0 || !inventory.legacy.is_empty() {
        return State::Attention;
    }
    State::Healthy
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{catalogue, Fixture};

    #[test]
    fn a_freshly_installed_repo_is_entirely_managed() {
        let fixture = Fixture::configured();
        let report = classify(&fixture.inventory(), &catalogue());
        let counts = report.counts();

        assert_eq!(counts.modified, 0, "{:?}", report.needing_attention());
        assert_eq!(counts.local, 0);
        assert_eq!(counts.broken, 0);
        assert!(counts.managed >= 5, "expected hooks, skills and a server");
        assert_eq!(state(&fixture.inventory(), &report), State::Healthy);
    }

    #[test]
    fn a_hand_edited_hook_reads_as_modified_not_as_local() {
        let fixture = Fixture::configured();
        fixture.append(".agents/hooks/format-on-edit.sh", "\n# my own tweak\n");

        let report = classify(&fixture.inventory(), &catalogue());
        let hook = report
            .items_of(Kind::Hook)
            .into_iter()
            .find(|i| i.name == "format-on-edit")
            .unwrap();

        // No version of the catalogue ever shipped this, so it is an edit, not an
        // out-of-date copy.
        assert_eq!(hook.origin, Origin::Modified);
        // It still knows where it came from, which is what makes "update it" offerable.
        assert_eq!(hook.catalogue_key.as_deref(), Some("format-on-edit"));
        assert_eq!(state(&fixture.inventory(), &report), State::Attention);
    }

    #[test]
    fn an_item_matching_an_older_catalogue_version_is_stale_rather_than_modified() {
        let fixture = Fixture::configured();
        let catalogue = catalogue();

        // A real former version of skills/handoff, taken from the clone's own history
        // rather than invented - an invented one could only ever be Modified.
        let log = std::process::Command::new("git")
            .arg("-C")
            .arg(&catalogue.root)
            .args(["log", "-n2", "--format=%H", "--", "skills/handoff"])
            .output()
            .unwrap();
        let commits: Vec<String> = String::from_utf8_lossy(&log.stdout)
            .lines()
            .map(str::to_string)
            .collect();
        assert!(
            commits.len() >= 2,
            "handoff must have changed at least once"
        );

        let body = std::process::Command::new("git")
            .arg("-C")
            .arg(&catalogue.root)
            .arg("show")
            .arg(format!("{}:skills/handoff/SKILL.md", commits[1]))
            .output()
            .unwrap();
        fixture.write(
            ".agents/skills/handoff/SKILL.md",
            &String::from_utf8_lossy(&body.stdout),
        );

        let inventory = fixture.inventory();
        let report = classify(&inventory, &catalogue);
        let skill = report
            .items_of(Kind::Skill)
            .into_iter()
            .find(|i| i.name == "handoff")
            .unwrap();
        assert_eq!(skill.origin, Origin::Stale);
        assert_eq!(report.counts().stale, 1);
        assert_eq!(report.counts().modified, 0);
        assert_eq!(state(&inventory, &report), State::Attention);
    }

    #[test]
    fn a_hand_written_skill_is_local_and_keeps_its_own_description() {
        let fixture = Fixture::configured();
        fixture.write_skill(".agents/skills/deploy-thing", "how we deploy the thing");

        let report = classify(&fixture.inventory(), &catalogue());
        let local = report.local();

        assert_eq!(local.len(), 1);
        assert_eq!(local[0].name, "deploy-thing");
        assert_eq!(local[0].kind, Kind::Skill);
        assert!(local[0].catalogue_key.is_none());
        assert_eq!(
            local[0].description.as_deref(),
            Some("how we deploy the thing")
        );
        // Local is not a fault, so it does not drag the repo out of health.
        assert_eq!(state(&fixture.inventory(), &report), State::Healthy);
    }

    #[test]
    fn a_server_nobody_shipped_is_local() {
        let fixture = Fixture::configured();
        fixture.write_mcp(serde_json::json!({
            "mcpServers": {
                "context7": { "command": "npx", "args": ["-y", "@upstash/context7-mcp@latest"] },
                "our-internal": { "command": "node", "args": ["tools/mcp.js"] }
            }
        }));

        let report = classify(&fixture.inventory(), &catalogue());
        let names: Vec<&str> = report.local().iter().map(|i| i.name.as_str()).collect();
        assert_eq!(names, vec!["our-internal"]);
    }

    #[test]
    fn pis_own_keys_do_not_make_a_shared_server_look_hand_edited() {
        let fixture = Fixture::configured();
        // Exactly what `ai-toolbox mcp context7 --harness pi` leaves behind.
        fixture.write_mcp(serde_json::json!({
            "mcpServers": {
                "context7": {
                    "command": "npx",
                    "args": ["-y", "@upstash/context7-mcp@latest"],
                    "lifecycle": "eager",
                    "directTools": true
                }
            }
        }));

        let report = classify(&fixture.inventory(), &catalogue());
        let server = report
            .items_of(Kind::Server)
            .into_iter()
            .find(|i| i.name == "context7")
            .unwrap();
        assert_eq!(server.origin, Origin::Managed);
    }

    #[test]
    fn a_real_change_to_a_server_still_reads_as_modified() {
        let fixture = Fixture::configured();
        fixture.write_mcp(serde_json::json!({
            "mcpServers": {
                "context7": { "command": "npx", "args": ["-y", "@upstash/context7-mcp@0.1.0"] }
            }
        }));

        let report = classify(&fixture.inventory(), &catalogue());
        let server = report
            .items_of(Kind::Server)
            .into_iter()
            .find(|i| i.name == "context7")
            .unwrap();
        assert_eq!(server.origin, Origin::Modified);
    }

    /// `.mcp.json` holding exactly what the named preset ships, which is what the
    /// installer writes. Built from the catalogue rather than transcribed, so a change
    /// to the preset cannot leave these tests asserting against a definition that no
    /// longer exists.
    fn install_preset(fixture: &Fixture, preset: &str) {
        let catalogue = catalogue();
        let def = catalogue.preset(preset).unwrap();
        let servers: serde_json::Map<String, serde_json::Value> = def
            .servers
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        fixture.write_mcp(serde_json::json!({ "mcpServers": servers }));
    }

    #[test]
    fn a_server_whose_launcher_is_missing_is_broken_and_says_which() {
        let fixture = Fixture::configured();
        // The preset is installed but its launcher is not - the half-install that fails
        // at connect time with a message naming neither.
        install_preset(&fixture, "supabase");

        let report = classify(&fixture.inventory(), &catalogue());
        let server = &report.items_of(Kind::Server)[0];
        match &server.origin {
            Origin::Broken { why } => assert!(why.contains("with-dotenv.sh"), "got: {why}"),
            other => panic!("expected broken, got {other:?}"),
        }
        assert_eq!(state(&fixture.inventory(), &report), State::Broken);
    }

    #[test]
    fn installing_the_launcher_leaves_that_server_fully_managed() {
        let fixture = Fixture::configured();
        install_preset(&fixture, "supabase");
        fixture.install_helper(&catalogue(), "with-dotenv.sh");

        let inventory = fixture.inventory();
        let report = classify(&inventory, &catalogue());
        let server = &report.items_of(Kind::Server)[0];
        assert_eq!(server.origin, Origin::Managed);
        assert_eq!(state(&inventory, &report), State::Healthy);
    }

    #[test]
    fn a_multi_server_preset_is_managed_once_all_of_its_servers_are_in() {
        let fixture = Fixture::configured();
        install_preset(&fixture, "supabase-multi-env");
        fixture.install_helper(&catalogue(), "with-dotenv.sh");

        let report = classify(&fixture.inventory(), &catalogue());
        let servers = report.items_of(Kind::Server);
        assert_eq!(servers.len(), 2);
        assert!(servers.iter().all(|s| s.origin == Origin::Managed));
        // Both halves trace back to the one preset, which is how the GUI groups them.
        assert!(servers
            .iter()
            .all(|s| s.catalogue_key.as_deref() == Some("supabase-multi-env")));
        assert!(!report
            .available
            .presets
            .contains(&"supabase-multi-env".to_string()));
    }

    #[test]
    fn a_hook_that_lost_its_executable_bit_is_broken_rather_than_managed() {
        use std::os::unix::fs::PermissionsExt;
        let fixture = Fixture::configured();
        let hook = fixture.path().join(".agents/hooks/session-context.sh");
        std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o644)).unwrap();

        let report = classify(&fixture.inventory(), &catalogue());
        let item = report
            .items_of(Kind::Hook)
            .into_iter()
            .find(|i| i.name == "session-context")
            .unwrap();
        assert!(matches!(item.origin, Origin::Broken { .. }));
    }

    #[test]
    fn available_lists_what_is_not_installed_and_leaves_out_what_is() {
        let fixture = Fixture::configured();
        let report = classify(&fixture.inventory(), &catalogue());

        assert!(!report
            .available
            .hooks
            .contains(&"format-on-edit".to_string()));
        assert!(report
            .available
            .hooks
            .contains(&"guard-irreversible".to_string()));
        assert!(!report.available.skills.contains(&"pre-pr".to_string()));
        // Group skills are offered by their catalogue key, which is how they are asked for.
        assert!(report.available.skills.contains(&"cli/gh".to_string()));
        assert!(!report.available.presets.contains(&"context7".to_string()));
    }

    #[test]
    fn a_legacy_layout_needs_a_look_rather_than_reading_as_broken() {
        // It still works: those hooks are wired to where they actually are. Doctor calls
        // this a warning, and the two verdicts have to agree.
        let fixture = Fixture::configured();
        std::fs::create_dir_all(fixture.path().join(".claude/hooks")).unwrap();

        let inventory = fixture.inventory();
        let report = classify(&inventory, &catalogue());
        assert_eq!(state(&inventory, &report), State::Attention);
    }

    #[test]
    fn a_dangling_skills_link_makes_the_repo_broken_even_when_every_item_is_fine() {
        let fixture = Fixture::configured();
        // The items all survive - only Claude Code's route to them is cut.
        fixture.remove(".agents/skills/pre-pr");
        fixture.remove(".agents/skills/handoff");
        fixture.remove(".agents/skills");

        let inventory = fixture.inventory();
        let report = classify(&inventory, &catalogue());
        assert_eq!(report.counts().broken, 0);
        assert_eq!(state(&inventory, &report), State::Broken);
    }

    #[test]
    fn an_untouched_directory_is_unconfigured_rather_than_healthy() {
        let fixture = Fixture::bare();
        let inventory = fixture.inventory();
        let report = classify(&inventory, &catalogue());
        assert_eq!(state(&inventory, &report), State::Unconfigured);
    }
}
