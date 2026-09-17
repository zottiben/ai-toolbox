//! Keeping every worktree of a repo configured the same way (D6).
//!
//! Worth being plain about why this is needed at all, because it looks like something
//! git should already handle. Everything this toolkit writes - `.agents/`, `.mcp.json`,
//! `AGENTS.md`, `CLAUDE.md`, `.codex/` - is in the user's global gitignore. Untracked
//! files do not come along with `git worktree add`, so **a new worktree starts with none
//! of it**, and nothing in git will ever mention that. It is the default outcome, not an
//! edge case.
//!
//! So this is a maintenance job the tool performs repeatedly, not a migration it performs
//! once. Two consequences shape the code:
//!
//! - a fresh worktree is *empty*, not damaged, and is labelled that way. Reporting
//!   "17 files missing" for something that was created thirty seconds ago is noise.
//! - convergence copies from the reference into the other worktrees and never the other
//!   way, and never deletes. An agent working in a feature worktree is the likeliest
//!   source of an accidental edit and the least likely source of an intended one - but a
//!   file only that worktree has could be someone's work, so it is reported and left.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::action::Action;
use crate::error::{Error, Result};
use crate::git::{self, Worktree};
use crate::hash::{self, Hash};
use crate::install::Plan;
use crate::paths;

/// The files this tool owns. Everything else in a worktree is the project's, and none of
/// this tool's business.
///
/// `.claude/skills` is in the list as a symlink and is never walked through: it points at
/// `.agents/skills`, so following it would compare every skill twice.
const MANAGED_FILES: [&str; 6] = [
    paths::SHARED_MCP,
    "AGENTS.md",
    "CLAUDE.md",
    paths::CLAUDE_SETTINGS,
    paths::CODEX_CONFIG,
    paths::PI_MCP,
];

const MANAGED_TREES: [&str; 1] = [".agents"];

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
enum Entry {
    File { hash: Hash, executable: bool },
    Symlink { target: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Standing {
    /// The worktree everything else is compared against.
    Reference,
    /// Nothing from the toolkit is here. Almost always a worktree added since the repo
    /// was set up - expected, and one action away from fixed.
    Unconfigured,
    /// Identical to the reference.
    InStep,
    /// Configured, but not the same.
    Diverged,
    /// Git lists it, but there is nothing on disk to look at.
    Unusable,
}

#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct Diff {
    /// In the reference, not here.
    pub missing: Vec<String>,
    /// In both, with different content.
    pub different: Vec<String>,
    /// Here and not in the reference. Never removed - it could be someone's work.
    pub extra: Vec<String>,
}

impl Diff {
    pub fn is_empty(&self) -> bool {
        self.missing.is_empty() && self.different.is_empty() && self.extra.is_empty()
    }

    /// Counts of *items* rather than files, because "missing 3 skills" is what a person
    /// wants and "missing 14 files" is what the filesystem happens to contain.
    pub fn summary(&self) -> String {
        let mut parts = Vec::new();
        if !self.missing.is_empty() {
            parts.push(format!("missing {}", describe(&self.missing)));
        }
        if !self.different.is_empty() {
            parts.push(format!("{} differ", describe(&self.different)));
        }
        if !self.extra.is_empty() {
            parts.push(format!("{} only here", describe(&self.extra)));
        }
        if parts.is_empty() {
            return "in step".to_string();
        }
        // Semicolons between the clauses, because the items inside each are already
        // comma-separated: "missing hook a, skill b, .mcp.json only here" reads as one
        // list of three missing things.
        parts.join("; ")
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct State {
    pub worktree: Worktree,
    pub standing: Standing,
    pub diff: Diff,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct Comparison {
    pub reference: PathBuf,
    pub worktrees: Vec<State>,
}

impl Comparison {
    /// The ones that could be brought into step.
    pub fn out_of_step(&self) -> Vec<&State> {
        self.worktrees
            .iter()
            .filter(|w| matches!(w.standing, Standing::Unconfigured | Standing::Diverged))
            .collect()
    }

    pub fn is_uniform(&self) -> bool {
        self.out_of_step().is_empty()
    }
}

/// Compare every worktree of this repo against the main one.
///
/// A repo with a single worktree still gets a comparison, holding just the reference -
/// which is the honest answer to "are my worktrees in step" when there is only one.
pub fn compare(repo: &Path) -> Result<Comparison> {
    let worktrees = git::worktrees(repo);
    let Some(main) = worktrees
        .iter()
        .find(|w| w.is_main && w.is_usable())
        .map(|w| w.path.clone())
    else {
        // Not a repo, or a bare one: there is nothing to hold up against anything.
        return Ok(Comparison {
            reference: repo.to_path_buf(),
            worktrees: Vec::new(),
        });
    };

    let reference = snapshot(&main)?;
    let mut states = Vec::new();
    for worktree in worktrees {
        if worktree.is_main {
            states.push(State {
                worktree,
                standing: Standing::Reference,
                diff: Diff::default(),
            });
            continue;
        }
        if !worktree.is_usable() {
            states.push(State {
                worktree,
                standing: Standing::Unusable,
                diff: Diff::default(),
            });
            continue;
        }
        let here = snapshot(&worktree.path)?;
        let diff = difference(&reference, &here);
        let standing = if here.is_empty() && !reference.is_empty() {
            Standing::Unconfigured
        } else if diff.is_empty() {
            Standing::InStep
        } else {
            Standing::Diverged
        };
        states.push(State {
            worktree,
            standing,
            diff,
        });
    }
    Ok(Comparison {
        reference: main,
        worktrees: states,
    })
}

/// Bring one worktree into step with the reference.
///
/// Copies and overwrites; never deletes. A file the target has and the reference does not
/// stays exactly where it is and is reported instead.
pub fn converge(plan: &mut Plan, reference: &Path, target: &Path) -> Result<()> {
    let wanted = snapshot(reference)?;
    let here = snapshot(target)?;

    for (relative, entry) in &wanted {
        if here.get(relative) == Some(entry) {
            continue;
        }
        let from = reference.join(relative);
        let to = target.join(relative);
        let verb = if here.contains_key(relative) {
            "replace"
        } else {
            "copy"
        };
        match entry {
            Entry::File { executable, .. } => {
                let contents = std::fs::read(&from).map_err(|e| Error::io(&from, e))?;
                let summary = format!("{verb} {relative}");
                plan.push(if *executable {
                    Action::write_executable(&to, contents, summary)?
                } else {
                    Action::write(&to, contents, summary)?
                });
            }
            Entry::Symlink { target: points_at } => {
                // Relative, so it resolves inside *this* worktree. An absolute link
                // copied from the reference would silently point every worktree at the
                // main one, and they would look configured while sharing one set of
                // skills.
                plan.push(Action::symlink(
                    &to,
                    points_at.clone(),
                    format!("{verb} {relative}  ->  {points_at}"),
                )?);
            }
        }
    }
    Ok(())
}

/// Every managed file in a worktree, by path relative to it.
fn snapshot(root: &Path) -> Result<BTreeMap<String, Entry>> {
    let mut found = BTreeMap::new();
    for relative in MANAGED_FILES {
        if let Some(entry) = read_entry(&root.join(relative))? {
            found.insert(relative.to_string(), entry);
        }
    }
    // The skills link is a symlink by design, so it is read as one rather than walked.
    if let Some(entry) = read_entry(&root.join(paths::CLAUDE_SKILLS))? {
        found.insert(paths::CLAUDE_SKILLS.to_string(), entry);
    }
    for tree in MANAGED_TREES {
        walk(root, &root.join(tree), &mut found)?;
    }
    Ok(found)
}

fn walk(root: &Path, dir: &Path, found: &mut BTreeMap<String, Entry>) -> Result<()> {
    if !dir.is_dir() {
        return Ok(());
    }
    let read = std::fs::read_dir(dir).map_err(|e| Error::io(dir, e))?;
    for item in read {
        let entry = item.map_err(|e| Error::io(dir, e))?;
        // Not configuration, and reporting it as a difference between worktrees is pure
        // noise - `.agents/.DS_Store` turns up in real worktrees.
        if paths::is_os_noise(&entry.file_name().to_string_lossy()) {
            continue;
        }
        let path = entry.path();
        if path.is_dir() && !path.is_symlink() {
            walk(root, &path, found)?;
            continue;
        }
        let Some(entry) = read_entry(&path)? else {
            continue;
        };
        let relative = path
            .strip_prefix(root)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");
        found.insert(relative, entry);
    }
    Ok(())
}

fn read_entry(path: &Path) -> Result<Option<Entry>> {
    // symlink_metadata, so a link is read as a link rather than as whatever it points at.
    let Ok(meta) = std::fs::symlink_metadata(path) else {
        return Ok(None);
    };
    if meta.file_type().is_symlink() {
        let target = std::fs::read_link(path).map_err(|e| Error::io(path, e))?;
        return Ok(Some(Entry::Symlink {
            target: target.to_string_lossy().into_owned(),
        }));
    }
    if meta.is_dir() {
        return Ok(None);
    }
    Ok(Some(Entry::File {
        hash: hash::file(path)?,
        executable: is_executable(&meta),
    }))
}

fn is_executable(meta: &std::fs::Metadata) -> bool {
    use std::os::unix::fs::PermissionsExt;
    meta.permissions().mode() & 0o111 != 0
}

fn difference(reference: &BTreeMap<String, Entry>, here: &BTreeMap<String, Entry>) -> Diff {
    let mut diff = Diff::default();
    for (relative, entry) in reference {
        match here.get(relative) {
            None => diff.missing.push(relative.clone()),
            Some(found) if found != entry => diff.different.push(relative.clone()),
            Some(_) => {}
        }
    }
    for relative in here.keys() {
        if !reference.contains_key(relative) {
            diff.extra.push(relative.clone());
        }
    }
    diff
}

/// Turn a list of file paths into the items they belong to: "3 skills, 1 hook".
fn describe(paths: &[String]) -> String {
    let mut counts: BTreeMap<&str, Vec<String>> = BTreeMap::new();
    for path in paths {
        let (kind, name) = item_of(path);
        let names = counts.entry(kind).or_default();
        if !names.contains(&name) {
            names.push(name);
        }
    }
    let parts: Vec<String> = counts
        .iter()
        .map(|(kind, names)| {
            if names.len() == 1 && *kind != "file" {
                format!("{kind} {}", names[0])
            } else if *kind == "file" {
                names.join(", ")
            } else {
                format!("{} {kind}s", names.len())
            }
        })
        .collect();
    parts.join(", ")
}

/// Which item a managed path belongs to.
fn item_of(path: &str) -> (&'static str, String) {
    if let Some(rest) = path.strip_prefix(".agents/skills/") {
        let name = rest.split('/').next().unwrap_or(rest);
        return ("skill", name.to_string());
    }
    if let Some(rest) = path.strip_prefix(".agents/hooks/") {
        return ("hook", rest.trim_end_matches(".sh").to_string());
    }
    if let Some(rest) = path.strip_prefix(".agents/mcp/") {
        return ("helper", rest.to_string());
    }
    ("file", path.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action;
    use crate::testing::{Fixture, Lab};

    fn sync(reference: &Path, target: &Path) {
        let mut plan = Plan::default();
        converge(&mut plan, reference, target).unwrap();
        let outcome = action::apply(&plan.actions).unwrap();
        assert!(
            outcome.stale.is_empty(),
            "converge collided: {:?}",
            outcome.stale
        );
    }

    #[test]
    fn a_new_worktree_starts_with_none_of_it_because_these_files_are_untracked() {
        // The premise of this whole module, asserted rather than assumed. `git worktree
        // add` propagates tracked content only; the global gitignore is the reason these
        // files are untracked in the first place and stay that way.
        let lab = Lab::new();
        lab.configure();
        let feature = lab.add_worktree("feature");

        assert!(lab.main().join(".mcp.json").is_file());
        assert!(
            !feature.join(".mcp.json").exists(),
            "git worktree add carried an untracked file across - the premise has changed"
        );
        assert!(
            feature.join("README.md").is_file(),
            "tracked content does come across"
        );
    }

    #[test]
    fn a_fresh_worktree_reads_as_unconfigured_rather_than_as_damage() {
        let lab = Lab::new();
        lab.configure();
        lab.add_worktree("feature");

        let comparison = compare(lab.main()).unwrap();
        assert_eq!(comparison.worktrees.len(), 2);
        assert_eq!(comparison.worktrees[0].standing, Standing::Reference);
        assert_eq!(comparison.worktrees[1].standing, Standing::Unconfigured);
        assert!(!comparison.is_uniform());
    }

    #[test]
    fn converging_a_fresh_worktree_brings_it_fully_into_step() {
        let lab = Lab::new();
        lab.configure();
        let feature = lab.add_worktree("feature");

        sync(lab.main(), &feature);

        let comparison = compare(lab.main()).unwrap();
        assert!(
            comparison.is_uniform(),
            "{:?}",
            comparison.worktrees[1].diff
        );
        assert_eq!(comparison.worktrees[1].standing, Standing::InStep);
    }

    #[test]
    fn the_skills_link_resolves_inside_the_worktree_it_was_copied_into() {
        // The failure this guards against is silent: an absolute link would resolve, so
        // everything would look right while every worktree shared the main one's skills.
        let lab = Lab::new();
        lab.configure();
        let feature = lab.add_worktree("feature");
        sync(lab.main(), &feature);

        let link = feature.join(paths::CLAUDE_SKILLS);
        assert_eq!(
            std::fs::read_link(&link).unwrap().to_str(),
            Some(paths::CLAUDE_SKILLS_TARGET)
        );
        let resolved = std::fs::canonicalize(&link).unwrap();
        assert_eq!(
            resolved,
            std::fs::canonicalize(feature.join(paths::AGENTS_SKILLS)).unwrap(),
            "the link must reach this worktree's own skills, not the reference's"
        );
        assert!(resolved.join("pre-pr/SKILL.md").is_file());
    }

    #[test]
    fn converging_is_idempotent() {
        let lab = Lab::new();
        lab.configure();
        let feature = lab.add_worktree("feature");
        sync(lab.main(), &feature);

        let mut plan = Plan::default();
        converge(&mut plan, lab.main(), &feature).unwrap();
        assert!(
            plan.is_noop(),
            "second converge wanted {} change(s)",
            plan.changes()
        );
    }

    #[test]
    fn a_worktree_that_has_drifted_names_the_items_rather_than_the_files() {
        let lab = Lab::new();
        lab.configure();
        let feature = lab.add_worktree("feature");
        sync(lab.main(), &feature);

        std::fs::remove_dir_all(feature.join(".agents/skills/pre-pr")).unwrap();
        std::fs::remove_file(feature.join(".agents/hooks/format-on-edit.sh")).unwrap();

        let comparison = compare(lab.main()).unwrap();
        let state = &comparison.worktrees[1];
        assert_eq!(state.standing, Standing::Diverged);

        let summary = state.diff.summary();
        assert!(summary.contains("skill pre-pr"), "{summary}");
        assert!(summary.contains("hook format-on-edit"), "{summary}");
        assert!(summary.starts_with("missing "), "{summary}");
    }

    #[test]
    fn an_edit_in_a_worktree_is_reported_and_then_replaced_by_the_reference() {
        let lab = Lab::new();
        lab.configure();
        let feature = lab.add_worktree("feature");
        sync(lab.main(), &feature);

        let hook = feature.join(".agents/hooks/format-on-edit.sh");
        std::fs::write(&hook, "#!/usr/bin/env bash\n# changed in the worktree\n").unwrap();

        let comparison = compare(lab.main()).unwrap();
        assert!(comparison.worktrees[1].diff.summary().contains("differ"));

        // D6: the reference wins, but only through a plan that showed it first.
        let mut plan = Plan::default();
        converge(&mut plan, lab.main(), &feature).unwrap();
        assert!(plan.actions.iter().any(|a| a
            .summary
            .starts_with("replace .agents/hooks/format-on-edit.sh")));
        action::apply(&plan.actions).unwrap();
        assert_eq!(
            std::fs::read(&hook).unwrap(),
            std::fs::read(lab.main().join(".agents/hooks/format-on-edit.sh")).unwrap()
        );
    }

    #[test]
    fn os_litter_is_not_a_difference_between_worktrees() {
        let lab = Lab::new();
        lab.configure();
        let feature = lab.add_worktree("feature");
        sync(lab.main(), &feature);

        // Exactly what was found in the wild.
        std::fs::write(feature.join(".agents/.DS_Store"), [0u8, 1, 2]).unwrap();
        std::fs::write(feature.join(".agents/skills/.DS_Store"), [0u8, 1, 2]).unwrap();

        let comparison = compare(lab.main()).unwrap();
        assert!(
            comparison.is_uniform(),
            "Finder litter is not a configuration difference: {:?}",
            comparison.worktrees[1].diff
        );
    }

    #[test]
    fn something_only_the_worktree_has_is_reported_and_never_deleted() {
        let lab = Lab::new();
        lab.configure();
        let feature = lab.add_worktree("feature");
        sync(lab.main(), &feature);

        let own = feature.join(".agents/skills/branch-only/SKILL.md");
        std::fs::create_dir_all(own.parent().unwrap()).unwrap();
        std::fs::write(&own, "---\nname: branch-only\n---\n").unwrap();

        let comparison = compare(lab.main()).unwrap();
        let summary = comparison.worktrees[1].diff.summary();
        assert!(summary.contains("only here"), "{summary}");
        assert!(summary.contains("skill branch-only"), "{summary}");
        // One clause, so nothing to separate - the semicolon only appears with two.
        assert!(!summary.contains(';'), "{summary}");

        sync(lab.main(), &feature);
        assert!(
            own.is_file(),
            "converge deleted work that only the worktree had"
        );
    }

    #[test]
    fn a_repo_with_one_worktree_is_uniform() {
        let lab = Lab::new();
        lab.configure();
        let comparison = compare(lab.main()).unwrap();
        assert_eq!(comparison.worktrees.len(), 1);
        assert!(comparison.is_uniform());
    }

    #[test]
    fn a_directory_that_is_not_a_repository_compares_to_nothing() {
        let fixture = Fixture::configured();
        let comparison = compare(fixture.path()).unwrap();
        assert!(comparison.worktrees.is_empty());
        assert!(comparison.is_uniform());
    }

    #[test]
    fn comparing_from_inside_a_worktree_still_uses_the_main_one_as_reference() {
        let lab = Lab::new();
        lab.configure();
        let feature = lab.add_worktree("feature");

        // Someone running `ai-toolbox worktrees` from the feature worktree must get the
        // same answer as from main, or the two would disagree about who is behind.
        let comparison = compare(&feature).unwrap();
        assert_eq!(
            std::fs::canonicalize(&comparison.reference).unwrap(),
            std::fs::canonicalize(lab.main()).unwrap()
        );
        assert_eq!(comparison.worktrees[1].standing, Standing::Unconfigured);
    }

    #[test]
    fn describe_counts_items_not_files() {
        let paths = vec![
            ".agents/skills/pre-pr/SKILL.md".to_string(),
            ".agents/skills/pre-pr/extra.md".to_string(),
            ".agents/skills/gh/SKILL.md".to_string(),
            ".agents/hooks/format-on-edit.sh".to_string(),
            ".mcp.json".to_string(),
        ];
        let text = describe(&paths);
        assert!(text.contains("2 skills"), "{text}");
        assert!(text.contains("hook format-on-edit"), "{text}");
        assert!(text.contains(".mcp.json"), "{text}");
    }
}
