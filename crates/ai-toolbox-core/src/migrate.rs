//! Folding a pre-`.agents/` repo onto the canonical layout.
//!
//! The old shape kept a copy of everything per harness - `.claude/hooks`, `.codex/hooks`,
//! `.claude/mcp` and so on - and those copies drift from each other the moment one is
//! updated. The canonical shape has one copy under `.agents/` with each harness pointing
//! at it, so this moves the files and re-points the configs that name them.
//!
//! Two rules run through all of it.
//!
//! **A move is not a merge.** Where both paths hold the same thing, the old copy goes.
//! Where they hold *different* things, nothing is touched and the conflict is reported -
//! picking a winner would silently discard whichever version somebody had been editing.
//!
//! **`.pi/mcp.json` folds by meaning, not by copying.** Pi's extra keys are inert to
//! Claude Code, so they can move to the shared file; a `!command` header cannot, because
//! Claude Code would send that literal string. Getting this wrong produces a config that
//! looks migrated and does not work, so each rule below carries the reason it exists.

use std::path::{Path, PathBuf};

use serde_json::{Map, Value};

use crate::action::Action;
use crate::error::{Error, Result};
use crate::install::Plan;
use crate::paths;

/// Pi keys Claude Code ignores, so they are safe in the shared `.mcp.json`.
/// Verified against Claude Code 2.1.220: unknown server keys are ignored, not rejected.
const SHAREABLE_PI_KEYS: [&str; 11] = [
    "lifecycle",
    "directTools",
    "idleTimeout",
    "requestTimeoutMs",
    "exposeResources",
    "toolPrefix",
    "includeTools",
    "excludeTools",
    "approveTools",
    "protocolVersion",
    "disabled",
];

/// The per-harness directories that become one `.agents/` directory, and where each goes.
const MOVES: [(&str, &str); 5] = [
    (".claude/hooks", paths::AGENTS_HOOKS),
    (".codex/hooks", paths::AGENTS_HOOKS),
    (".claude/mcp", paths::AGENTS_MCP),
    (".codex/mcp", paths::AGENTS_MCP),
    (".pi/mcp", paths::AGENTS_MCP),
];

#[derive(Debug, Default, serde::Serialize)]
pub struct Report {
    /// Files that would move, as `from -> to`.
    pub moved: Vec<String>,
    /// Paths holding different content under both layouts. Left exactly as they are.
    pub conflicts: Vec<String>,
    /// Things worth saying that are not a file change.
    pub notes: Vec<String>,
}

impl Report {
    pub fn is_empty(&self) -> bool {
        self.moved.is_empty() && self.conflicts.is_empty() && self.notes.is_empty()
    }
}

/// Plan the whole migration. Writes nothing; `action::apply` does that.
pub fn plan(plan: &mut Plan, repo: &Path) -> Result<Report> {
    let mut report = Report::default();

    for (from, to) in MOVES {
        move_dir(plan, repo, from, to, &mut report)?;
    }
    skills(plan, repo, &mut report)?;
    repoint(plan, repo, &mut report)?;
    fold_pi(plan, repo, &mut report)?;
    prune_empty(plan, repo)?;

    Ok(report)
}

/// Remove a harness directory that the migration has emptied.
///
/// Not tidiness. `harness::detect` treats the presence of `.pi/` as "this repo uses Pi",
/// so an empty one left behind makes every later command write Pi config for a repo that
/// has none.
fn prune_empty(plan: &mut Plan, repo: &Path) -> Result<()> {
    for name in [".claude", ".codex", ".pi"] {
        let dir = repo.join(name);
        if !dir.is_dir() {
            continue;
        }
        // Empty *after* this plan runs: everything in it now is either already gone or
        // scheduled for removal.
        let entries = std::fs::read_dir(&dir).map_err(|e| Error::io(&dir, e))?;
        let mut survives = false;
        for entry in entries {
            let path = entry.map_err(|e| Error::io(&dir, e))?.path();
            let removed = plan.actions.iter().any(|action| {
                matches!(action.kind, crate::action::Kind::Remove) && action.path == path
            });
            if !removed {
                survives = true;
                break;
            }
        }
        if !survives {
            plan.push(Action::remove(&dir, format!("remove empty {name}/"))?);
        }
    }
    Ok(())
}

/// Move a per-harness directory's files into the shared one.
///
/// A file that already exists at the destination is dropped rather than overwritten: the
/// canonical copy is the one the configs will point at, so the old one has nothing left
/// to do.
fn move_dir(plan: &mut Plan, repo: &Path, from: &str, to: &str, report: &mut Report) -> Result<()> {
    let source = repo.join(from);
    if !source.is_dir() {
        return Ok(());
    }
    let entries = std::fs::read_dir(&source).map_err(|e| Error::io(&source, e))?;
    for entry in entries {
        let entry = entry.map_err(|e| Error::io(&source, e))?;
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        if paths::is_os_noise(&name) {
            plan.push(Action::remove(&path, format!("remove {from}/{name}"))?);
            continue;
        }
        let target = repo.join(to).join(&name);

        if target.exists() {
            plan.push(Action::remove(
                &path,
                format!("drop {from}/{name} - {to}/{name} already has it"),
            )?);
            continue;
        }
        if path.is_dir() {
            plan.push(Action::copy_tree(
                &path,
                &target,
                format!("move {from}/{name}  ->  {to}/"),
            )?);
        } else {
            let contents = std::fs::read(&path).map_err(|e| Error::io(&path, e))?;
            let executable = is_executable(&path);
            let summary = format!("move {from}/{name}  ->  {to}/");
            plan.push(if executable {
                Action::write_executable(&target, contents, summary)?
            } else {
                Action::write(&target, contents, summary)?
            });
        }
        plan.push(Action::remove(&path, format!("remove {from}/{name}"))?);
        report.moved.push(format!("{from}/{name} -> {to}/{name}"));
    }
    // The now-empty directory goes too, so `status` stops calling the repo legacy.
    plan.push(Action::remove(&source, format!("remove {from}/"))?);
    Ok(())
}

/// Fold `.claude/skills` into `.agents/skills` and replace it with the symlink.
fn skills(plan: &mut Plan, repo: &Path, report: &mut Report) -> Result<()> {
    let link = repo.join(paths::CLAUDE_SKILLS);
    let canonical = repo.join(paths::AGENTS_SKILLS);

    match std::fs::symlink_metadata(&link) {
        // Already a link: nothing to fold.
        Ok(meta) if meta.file_type().is_symlink() => return Ok(()),
        Ok(meta) if meta.is_dir() => {}
        // No .claude/skills at all. Link it, so Claude Code can see what is there.
        _ => {
            if canonical.is_dir() {
                plan.push(Action::symlink(
                    &link,
                    paths::CLAUDE_SKILLS_TARGET,
                    format!(
                        "link  {}  ->  {}",
                        paths::CLAUDE_SKILLS,
                        paths::CLAUDE_SKILLS_TARGET
                    ),
                )?);
            }
            return Ok(());
        }
    }

    let mut blocked = false;
    let entries = std::fs::read_dir(&link).map_err(|e| Error::io(&link, e))?;
    for entry in entries {
        let entry = entry.map_err(|e| Error::io(&link, e))?;
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        if paths::is_os_noise(&name) {
            plan.push(Action::remove(
                &path,
                format!("remove .claude/skills/{name}"),
            )?);
            continue;
        }
        if !path.is_dir() {
            continue;
        }
        let target = canonical.join(&name);

        if target.is_dir() {
            // Same skill under both paths: keep the canonical one, drop the duplicate.
            // Different content is somebody's edit, and choosing for them is not this
            // command's job.
            if crate::hash::dir(&path)? == crate::hash::dir(&target)? {
                plan.push(Action::remove(
                    &path,
                    format!("drop .claude/skills/{name} - identical to the canonical copy"),
                )?);
            } else {
                blocked = true;
                report.conflicts.push(format!(
                    ".claude/skills/{name} and {}/{name} differ - reconcile them by hand, then re-run",
                    paths::AGENTS_SKILLS
                ));
            }
            continue;
        }
        plan.push(Action::copy_tree(
            &path,
            &target,
            format!("move .claude/skills/{name}  ->  {}/", paths::AGENTS_SKILLS),
        )?);
        plan.push(Action::remove(
            &path,
            format!("remove .claude/skills/{name}"),
        )?);
        report.moved.push(format!(
            ".claude/skills/{name} -> {}/{name}",
            paths::AGENTS_SKILLS
        ));
    }

    // Only replace the directory with the link once nothing is left in it - a conflict
    // means somebody still has work in there.
    if blocked {
        report.notes.push(
            ".claude/skills was left as a directory because of the conflicts above".to_string(),
        );
        return Ok(());
    }
    plan.push(Action::remove(&link, "remove .claude/skills/".to_string())?);
    plan.push(Action::symlink(
        &link,
        paths::CLAUDE_SKILLS_TARGET,
        format!(
            "link  {}  ->  {}",
            paths::CLAUDE_SKILLS,
            paths::CLAUDE_SKILLS_TARGET
        ),
    )?);
    Ok(())
}

/// Re-point every config that names a moved path.
fn repoint(plan: &mut Plan, repo: &Path, report: &mut Report) -> Result<()> {
    for relative in [paths::CLAUDE_SETTINGS, paths::SHARED_MCP] {
        let path = repo.join(relative);
        if !path.is_file() {
            continue;
        }
        let text = std::fs::read_to_string(&path).map_err(|e| Error::io(&path, e))?;
        let document: Value = serde_json::from_str(&text).map_err(|e| Error::json(&path, e))?;
        let rewritten = rewrite(&document);
        if rewritten == document {
            continue;
        }
        plan.push(Action::write(
            &path,
            crate::merge::to_string(&rewritten),
            format!("{relative}: re-pointed at the .agents/ paths"),
        )?);
        report.notes.push(format!("{relative} re-pointed"));
    }

    let path = repo.join(paths::CODEX_CONFIG);
    if path.is_file() {
        let text = std::fs::read_to_string(&path).map_err(|e| Error::io(&path, e))?;
        let config: toml::Value = toml::from_str(&text).map_err(|e| Error::toml(&path, e))?;
        // Through JSON, so one rewriter handles both formats rather than two that could
        // disagree about what a legacy path looks like.
        let as_json = serde_json::to_value(&config)
            .map_err(|e| Error::Catalogue(format!("reading {}: {e}", path.display())))?;
        let rewritten = rewrite(&as_json);
        if rewritten != as_json {
            let back: toml::Value = serde_json::from_value(rewritten)
                .map_err(|e| Error::Catalogue(format!("rewriting {}: {e}", path.display())))?;
            let rendered = toml::to_string(&back)
                .map_err(|e| Error::Catalogue(format!("serialising the Codex config: {e}")))?;
            plan.push(Action::write(
                &path,
                rendered,
                format!("{}: re-pointed at the .agents/ paths", paths::CODEX_CONFIG),
            )?);
            report
                .notes
                .push(format!("{} re-pointed", paths::CODEX_CONFIG));
        }
    }
    Ok(())
}

/// Rewrite every legacy per-harness path found anywhere in a document.
fn rewrite(value: &Value) -> Value {
    match value {
        Value::String(text) => {
            let mut out = crate::convert::canonical_helper_path(text);
            for old in [".claude/hooks/", ".codex/hooks/"] {
                out = out.replace(old, &format!("{}/", paths::AGENTS_HOOKS));
            }
            Value::String(out)
        }
        Value::Array(items) => Value::Array(items.iter().map(rewrite).collect()),
        Value::Object(map) => {
            Value::Object(map.iter().map(|(k, v)| (k.clone(), rewrite(v))).collect())
        }
        other => other.clone(),
    }
}

/// Fold `.pi/mcp.json` into `.mcp.json`, keeping behind only what genuinely cannot be
/// shared.
///
/// Three fix-ups for config older versions of this toolbox emitted, each of which
/// produces a server that looks configured and does not work:
///
/// - `transport` is not a pi-mcp-adapter key at all - it is `httpTransport`.
/// - `auth: {"type": "oauth"}` is the wrong shape. The adapter takes the string
///   `"oauth"`, and an object makes `supportsOAuth()` return false, silently disabling
///   the OAuth it was meant to enable. Omitting the key entirely means deferred
///   auto-detected OAuth, which is the behaviour wanted.
/// - an override that changes a server's transport kind leaves the merged entry with both
///   `command` and `url`, which the adapter rejects outright.
fn fold_pi(plan: &mut Plan, repo: &Path, report: &mut Report) -> Result<()> {
    let pi_path = repo.join(paths::PI_MCP);
    if !pi_path.is_file() {
        return Ok(());
    }
    let pi_text = std::fs::read_to_string(&pi_path).map_err(|e| Error::io(&pi_path, e))?;
    let pi_doc: Value = serde_json::from_str(&pi_text).map_err(|e| Error::json(&pi_path, e))?;
    let Some(pi_servers) = pi_doc.get("mcpServers").and_then(|s| s.as_object()) else {
        return Ok(());
    };

    let shared_path = repo.join(paths::SHARED_MCP);
    let mut shared: Value = if shared_path.is_file() {
        let text = std::fs::read_to_string(&shared_path).map_err(|e| Error::io(&shared_path, e))?;
        serde_json::from_str(&text).map_err(|e| Error::json(&shared_path, e))?
    } else {
        Value::Object(Map::new())
    };
    // Applied to the shared document too, since it may still name a legacy helper.
    shared = rewrite(&shared);
    let shared_servers = shared
        .as_object_mut()
        .expect("an mcp document is an object")
        .entry("mcpServers")
        .or_insert_with(|| Value::Object(Map::new()));
    let Some(shared_servers) = shared_servers.as_object_mut() else {
        return Ok(());
    };

    let mut kept = Map::new();
    let mut moved = 0usize;
    let mut stale = Vec::new();

    for (name, raw) in pi_servers {
        let entry = normalise_pi(&rewrite(raw));
        let Some(entry) = entry.as_object() else {
            continue;
        };

        let Some(target) = shared_servers.get_mut(name) else {
            // Pi-only server: it belongs in the shared file, where Claude Code sees it.
            shared_servers.insert(name.clone(), Value::Object(entry.clone()));
            moved += 1;
            continue;
        };
        let Some(target) = target.as_object_mut() else {
            continue;
        };

        if let (Some(a), Some(b)) = (transport(entry), transport(target)) {
            if a != b {
                stale.push(name.clone());
                continue;
            }
        }

        let mut stay = Map::new();
        for (key, value) in entry {
            if SHAREABLE_PI_KEYS.contains(&key.as_str()) {
                target.insert(key.clone(), value.clone());
            } else if target.get(key) == Some(value) {
                // Identical to the shared definition: pure duplication, drop it.
            } else {
                stay.insert(key.clone(), value.clone());
            }
        }
        if stay.is_empty() {
            moved += 1;
        } else {
            kept.insert(name.clone(), Value::Object(stay));
        }
    }

    if moved > 0 {
        report.notes.push(format!(
            "{} server(s) folded into {}",
            moved,
            paths::SHARED_MCP
        ));
    }
    if !kept.is_empty() {
        let names: Vec<&str> = kept.keys().map(|k| k.as_str()).collect();
        report.notes.push(format!(
            "{} kept in {} - cannot be shared with Claude Code",
            names.join(", "),
            paths::PI_MCP
        ));
    }
    for name in &stale {
        report.conflicts.push(format!(
            "{name} in {} changed the server's transport kind, which the adapter rejects - dropped. Re-run: ai-toolbox mcp {name}",
            paths::PI_MCP
        ));
    }

    plan.push(Action::write(
        &shared_path,
        crate::merge::to_string(&shared),
        format!("folded {} into {}", paths::PI_MCP, paths::SHARED_MCP),
    )?);

    if kept.is_empty() {
        plan.push(Action::remove(
            &pi_path,
            format!("remove {}", paths::PI_MCP),
        )?);
    } else {
        plan.push(Action::write(
            &pi_path,
            crate::merge::to_string(&serde_json::json!({ "mcpServers": kept })),
            format!("{} reduced to the Pi-only overrides", paths::PI_MCP),
        )?);
    }
    Ok(())
}

/// Strip keys older versions emitted that pi-mcp-adapter does not accept.
fn normalise_pi(entry: &Value) -> Value {
    let mut out = entry.clone();
    let Some(map) = out.as_object_mut() else {
        return out;
    };
    map.remove("transport");
    // The adapter takes the *string* "oauth"; any other shape disables OAuth silently.
    if map.get("auth").is_some_and(|a| !a.is_string()) {
        map.remove("auth");
    }
    out
}

fn transport(entry: &Map<String, Value>) -> Option<&'static str> {
    if entry.contains_key("url") {
        return Some("url");
    }
    entry.contains_key("command").then_some("command")
}

fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path).is_ok_and(|m| m.permissions().mode() & 0o111 != 0)
}

/// Whether this repo has anything to migrate.
pub fn needed(repo: &Path) -> bool {
    if MOVES.iter().any(|(from, _)| repo.join(from).is_dir()) {
        return true;
    }
    if repo.join(paths::PI_MCP).is_file() {
        return true;
    }
    let link = repo.join(paths::CLAUDE_SKILLS);
    std::fs::symlink_metadata(&link).is_ok_and(|m| m.is_dir() && !m.file_type().is_symlink())
}

/// Where the migrated repo keeps its skills, for a caller that wants to report it.
pub fn canonical_skills() -> PathBuf {
    PathBuf::from(paths::AGENTS_SKILLS)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action;
    use crate::testing::{catalogue, Fixture};

    /// A repo in the old per-harness shape, as one set up by an older ai-toolbox.
    fn legacy() -> Fixture {
        let fixture = Fixture::bare();
        let catalogue = catalogue();

        // Hooks under .claude/hooks, wired to that path - and executable, because a
        // legacy repo that worked had working hooks. Migrating one that was already
        // broken is doctor's problem, not this module's.
        let hook = catalogue.hook("format-on-edit").unwrap();
        fixture.write(
            ".claude/hooks/format-on-edit.sh",
            &std::fs::read_to_string(&hook.path).unwrap(),
        );
        make_executable(&fixture.path().join(".claude/hooks/format-on-edit.sh"));
        fixture.write(
            ".claude/mcp/with-dotenv.sh",
            &std::fs::read_to_string(&catalogue.helper("with-dotenv.sh").unwrap().path).unwrap(),
        );
        make_executable(&fixture.path().join(".claude/mcp/with-dotenv.sh"));
        fixture.write_json(
            paths::CLAUDE_SETTINGS,
            serde_json::json!({
                "hooks": {
                    "PostToolUse": [{
                        "matcher": "Write|Edit",
                        "hooks": [{
                            "type": "command",
                            "command": "${CLAUDE_PROJECT_DIR}/.claude/hooks/format-on-edit.sh"
                        }]
                    }]
                }
            }),
        );
        // The server that names that helper, by its old per-harness path.
        fixture.write_mcp(serde_json::json!({
            "mcpServers": {
                "supabase": {
                    "command": ".claude/mcp/with-dotenv.sh",
                    "args": ["--need", "TOKEN", "--", "npx", "server"]
                }
            }
        }));
        // Skills as a real directory.
        fixture.write_skill(".claude/skills/own-skill", "our own thing");
        fixture
    }

    fn make_executable(path: &Path) {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    fn migrate(fixture: &Fixture) -> Report {
        let mut plan = Plan::default();
        let report = plan_for(&mut plan, fixture.path());
        let outcome = action::apply(&plan.actions).unwrap();
        assert!(
            outcome.stale.is_empty(),
            "migrate collided: {:?}",
            outcome.stale
        );
        report
    }

    fn plan_for(plan: &mut Plan, repo: &Path) -> Report {
        super::plan(plan, repo).expect("planning the migration")
    }

    #[test]
    fn a_canonical_repo_needs_no_migration() {
        let fixture = Fixture::configured();
        assert!(!needed(fixture.path()));

        let mut plan = Plan::default();
        let report = plan_for(&mut plan, fixture.path());
        assert!(plan.is_noop(), "{:?}", plan.actions);
        assert!(report.is_empty());
    }

    #[test]
    fn the_per_harness_directories_become_one() {
        let fixture = legacy();
        assert!(needed(fixture.path()));
        migrate(&fixture);

        assert!(fixture
            .path()
            .join(".agents/hooks/format-on-edit.sh")
            .is_file());
        assert!(fixture.path().join(".agents/mcp/with-dotenv.sh").is_file());
        assert!(!fixture.path().join(".claude/hooks").exists());
        assert!(!fixture.path().join(".claude/mcp").exists());
        assert!(!needed(fixture.path()));
    }

    #[test]
    fn a_moved_hook_keeps_its_executable_bit() {
        // A hook that arrives unexecutable does not run, and nothing says so until the
        // moment it was supposed to.
        let fixture = legacy();
        migrate(&fixture);
        assert!(is_executable(
            &fixture.path().join(".agents/hooks/format-on-edit.sh")
        ));
        assert!(is_executable(
            &fixture.path().join(".agents/mcp/with-dotenv.sh")
        ));
    }

    #[test]
    fn the_configs_are_re_pointed_at_where_the_files_now_are() {
        let fixture = legacy();
        migrate(&fixture);

        let settings =
            std::fs::read_to_string(fixture.path().join(paths::CLAUDE_SETTINGS)).unwrap();
        assert!(
            settings.contains(".agents/hooks/format-on-edit.sh"),
            "{settings}"
        );
        assert!(!settings.contains(".claude/hooks/"), "{settings}");

        let mcp = std::fs::read_to_string(fixture.path().join(paths::SHARED_MCP)).unwrap();
        assert!(mcp.contains(".agents/mcp/with-dotenv.sh"), "{mcp}");
    }

    #[test]
    fn skills_are_folded_and_the_directory_becomes_the_symlink() {
        let fixture = legacy();
        migrate(&fixture);

        assert!(fixture
            .path()
            .join(".agents/skills/own-skill/SKILL.md")
            .is_file());
        let link = fixture.path().join(paths::CLAUDE_SKILLS);
        assert_eq!(
            std::fs::read_link(&link).unwrap().to_str(),
            Some(paths::CLAUDE_SKILLS_TARGET)
        );
        // And it resolves, which is the whole point of the link.
        assert!(std::fs::canonicalize(&link)
            .unwrap()
            .join("own-skill")
            .is_dir());
    }

    #[test]
    fn a_skill_under_both_paths_with_the_same_content_is_deduplicated() {
        let fixture = legacy();
        fixture.write_skill(".claude/skills/shared", "same either side");
        fixture.write_skill(".agents/skills/shared", "same either side");

        let report = migrate(&fixture);
        assert!(report.conflicts.is_empty(), "{:?}", report.conflicts);
        assert!(fixture.path().join(".agents/skills/shared").is_dir());
    }

    #[test]
    fn a_skill_that_differs_under_both_paths_is_left_alone_and_reported() {
        let fixture = legacy();
        fixture.write_skill(".claude/skills/shared", "the version I was editing");
        fixture.write_skill(".agents/skills/shared", "the other version");

        let report = migrate(&fixture);
        assert_eq!(report.conflicts.len(), 1);
        assert!(report.conflicts[0].contains("shared"));
        // Both survive: choosing between somebody's two versions is not this command's job.
        assert!(fixture
            .path()
            .join(".claude/skills/shared/SKILL.md")
            .is_file());
        assert!(fixture
            .path()
            .join(".agents/skills/shared/SKILL.md")
            .is_file());
        // And the directory is not replaced by a link while work is still in it.
        let link = fixture.path().join(paths::CLAUDE_SKILLS);
        assert!(!std::fs::symlink_metadata(&link)
            .unwrap()
            .file_type()
            .is_symlink());
    }

    #[test]
    fn a_pi_only_server_moves_to_the_shared_file() {
        let fixture = Fixture::bare();
        fixture.write_mcp(serde_json::json!({ "mcpServers": {} }));
        fixture.write_json(
            paths::PI_MCP,
            serde_json::json!({
                "mcpServers": { "only-pi": { "command": "node", "args": ["x.js"] } }
            }),
        );

        migrate(&fixture);
        let shared: Value = serde_json::from_str(
            &std::fs::read_to_string(fixture.path().join(paths::SHARED_MCP)).unwrap(),
        )
        .unwrap();
        assert_eq!(shared["mcpServers"]["only-pi"]["command"], "node");
        assert!(
            !fixture.path().join(paths::PI_MCP).exists(),
            "an empty override file is one more thing to keep in step for no reason"
        );
    }

    #[test]
    fn pis_shareable_keys_move_and_a_bang_header_stays_behind() {
        let fixture = Fixture::bare();
        fixture.write_mcp(serde_json::json!({
            "mcpServers": { "remote": { "type": "http", "url": "https://x/api" } }
        }));
        fixture.write_json(
            paths::PI_MCP,
            serde_json::json!({
                "mcpServers": {
                    "remote": {
                        "lifecycle": "lazy",
                        "headers": { "Authorization": "!.agents/mcp/dotenv-header.sh --raw TOKEN" }
                    }
                }
            }),
        );

        migrate(&fixture);
        let shared: Value = serde_json::from_str(
            &std::fs::read_to_string(fixture.path().join(paths::SHARED_MCP)).unwrap(),
        )
        .unwrap();
        assert_eq!(shared["mcpServers"]["remote"]["lifecycle"], "lazy");
        assert!(
            shared["mcpServers"]["remote"].get("headers").is_none(),
            "Claude Code would send the literal '!command' string"
        );

        let pi: Value = serde_json::from_str(
            &std::fs::read_to_string(fixture.path().join(paths::PI_MCP)).unwrap(),
        )
        .unwrap();
        assert!(pi["mcpServers"]["remote"]["headers"].is_object());
    }

    #[test]
    fn the_three_fix_ups_for_config_older_versions_emitted() {
        let fixture = Fixture::bare();
        fixture.write_mcp(serde_json::json!({
            "mcpServers": {
                "remote": { "type": "http", "url": "https://x/api" },
                "local": { "command": "node", "args": ["x.js"] }
            }
        }));
        fixture.write_json(
            paths::PI_MCP,
            serde_json::json!({
                "mcpServers": {
                    // `transport` is not an adapter key; `auth` as an object silently
                    // disables the OAuth it looks like it enables.
                    "remote": { "transport": "sse", "auth": { "type": "oauth" }, "lifecycle": "lazy" },
                    // And an override that turns a stdio server into a remote one leaves
                    // the merged entry with both, which the adapter refuses.
                    "local": { "url": "https://elsewhere/api" }
                }
            }),
        );

        let report = migrate(&fixture);
        let shared: Value = serde_json::from_str(
            &std::fs::read_to_string(fixture.path().join(paths::SHARED_MCP)).unwrap(),
        )
        .unwrap();

        assert!(shared["mcpServers"]["remote"].get("transport").is_none());
        assert!(shared["mcpServers"]["remote"].get("auth").is_none());
        assert_eq!(shared["mcpServers"]["remote"]["lifecycle"], "lazy");

        // The transport-changing override is dropped, with the command to put it back.
        assert_eq!(shared["mcpServers"]["local"]["command"], "node");
        assert!(shared["mcpServers"]["local"].get("url").is_none());
        assert_eq!(report.conflicts.len(), 1);
        assert!(report.conflicts[0].contains("ai-toolbox mcp local"));
    }

    #[test]
    fn an_emptied_harness_directory_is_removed_so_detection_does_not_claim_it() {
        let fixture = Fixture::bare();
        fixture.write_mcp(serde_json::json!({ "mcpServers": {} }));
        fixture.write_json(
            paths::PI_MCP,
            serde_json::json!({ "mcpServers": { "only-pi": { "command": "node" } } }),
        );

        migrate(&fixture);
        assert!(
            !fixture.path().join(".pi").exists(),
            "an empty .pi/ makes harness detection write Pi config for a repo with none"
        );
        assert!(
            !crate::harness::detect(fixture.path(), fixture.path()).contains(&crate::Harness::Pi)
        );
    }

    #[test]
    fn a_harness_directory_with_something_left_in_it_stays() {
        let fixture = legacy();
        fixture.write(
            ".codex/config.toml",
            "[mcp_servers.kept]\ncommand = \"node\"\n",
        );
        migrate(&fixture);
        assert!(fixture.path().join(".codex/config.toml").is_file());
        // And .claude keeps its settings and the new skills link.
        assert!(fixture.path().join(paths::CLAUDE_SETTINGS).is_file());
    }

    #[test]
    fn migrating_twice_does_nothing_the_second_time() {
        let fixture = legacy();
        migrate(&fixture);

        let mut plan = Plan::default();
        let report = plan_for(&mut plan, fixture.path());
        assert!(
            plan.is_noop(),
            "second migrate wanted {} change(s): {:?}",
            plan.changes(),
            plan.actions
                .iter()
                .filter(|a| !a.noop)
                .map(|a| &a.summary)
                .collect::<Vec<_>>()
        );
        assert!(report.conflicts.is_empty());
    }

    #[test]
    fn a_migrated_repo_passes_doctor() {
        // The point of migrating is a repo that works, not one that has merely moved.
        let fixture = legacy();
        migrate(&fixture);

        let survey = crate::survey(fixture.path(), &catalogue()).unwrap();
        let broken: Vec<&crate::Finding> = survey
            .findings
            .iter()
            .filter(|f| f.severity == crate::Severity::Broken)
            .collect();
        assert!(broken.is_empty(), "{broken:?}");
    }
}
