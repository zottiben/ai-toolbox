//! Planning an install.
//!
//! Every function here returns actions and writes nothing (D3). The shape is the same
//! each time: work out the one canonical copy under `.agents/`, then add whatever each
//! selected harness needs to find it.
//!
//! The asymmetry between the harnesses is the whole design, and it is worth having in
//! view while reading this:
//!
//! - hook *scripts* are shared, because every harness takes an arbitrary command path.
//!   Only the wiring differs, and that is a per-harness file.
//! - `.mcp.json` is shared by Claude Code and Pi; only Codex needs a generated copy.
//! - skills are one folder, because SKILL.md is a cross-agent standard. Claude Code is
//!   the only harness that will not look in `.agents/skills`, and it follows a symlink.

use std::path::{Path, PathBuf};

use serde_json::{Map, Value};

use crate::action::Action;
use crate::catalogue::Catalogue;
use crate::convert;
use crate::error::{Error, Result};
use crate::harness::Harness;
use crate::machine;
use crate::merge;
use crate::paths;

/// A plan, plus anything the user has to be told that is not a file change.
///
/// Actions accumulate into one of these, and a plan is also the record of what earlier
/// steps in it are *going* to write. That second job matters: `.codex/config.toml` gets
/// its hook wiring from one step and its MCP servers from another, and if the second
/// step read the file off disk it would compute its merge from a version without the
/// first step's changes and then overwrite them. So a planner reads through the plan,
/// and only falls back to disk for a path nothing has staged yet.
#[derive(Debug, Default)]
pub struct Plan {
    pub actions: Vec<Action>,
    /// Real harness differences a conversion had to make, surfaced rather than buried.
    pub warnings: Vec<String>,
    /// Secrets the installed servers need, and where each one has to go.
    pub secrets: Vec<crate::secrets::Secret>,
}

impl Plan {
    pub fn changes(&self) -> usize {
        self.actions.iter().filter(|a| !a.noop).count()
    }

    pub fn is_noop(&self) -> bool {
        self.changes() == 0
    }

    /// Add an action, folding it into any earlier one for the same path.
    ///
    /// Two actions on one path would otherwise collide at apply time: the first writes,
    /// and the second - planned against the state before it - finds the file changed and
    /// is refused as stale. Folding keeps the preview in the order the steps were
    /// planned, which is the order a reader expects.
    ///
    /// The summaries are joined rather than replaced. `.codex/config.toml` gets its hook
    /// wiring from one step and its MCP servers from another, and a preview that
    /// mentioned only the second would have someone conclude their hooks were not wired.
    pub fn push(&mut self, mut action: Action) {
        match self.actions.iter().position(|a| a.path == action.path) {
            Some(at) => {
                let previous = &self.actions[at];
                if previous.summary != action.summary {
                    action.summary = format!("{}; {}", previous.summary, action.summary);
                }
                self.actions[at] = action;
            }
            None => self.actions.push(action),
        }
    }

    pub fn warn(&mut self, message: impl Into<String>) {
        self.warnings.push(message.into());
    }

    fn note_secret(&mut self, secret: &crate::secrets::Secret) {
        if !self.secrets.contains(secret) {
            self.secrets.push(secret.clone());
        }
    }

    /// What a path will hold once this plan is applied, or what it holds now.
    fn staged(&self, path: &Path) -> Option<Vec<u8>> {
        self.actions
            .iter()
            .rev()
            .find(|a| a.path == path)
            .and_then(|a| match &a.kind {
                crate::action::Kind::Write { contents, .. } => Some(contents.clone()),
                _ => None,
            })
    }

    fn read_json(&self, path: &Path) -> Result<Value> {
        match self.staged(path) {
            Some(bytes) => parse_json(path, &String::from_utf8_lossy(&bytes)),
            None => read_json_or_empty(path),
        }
    }

    /// The same read a planner does, for the repair paths in [`crate::doctor`] that
    /// build a Codex config outside this module.
    pub fn read_toml_public(&self, path: &Path) -> Result<toml::Value> {
        self.read_toml(path)
    }

    fn read_toml(&self, path: &Path) -> Result<toml::Value> {
        match self.staged(path) {
            Some(bytes) => {
                toml::from_str(&String::from_utf8_lossy(&bytes)).map_err(|e| Error::toml(path, e))
            }
            None => read_toml_or_empty(path),
        }
    }
}

/// Install hook scripts, and wire them up for each selected harness.
pub fn hooks(
    plan: &mut Plan,
    repo: &Path,
    catalogue: &Catalogue,
    names: &[String],
    harnesses: &[Harness],
) -> Result<()> {
    let mut installed = Vec::new();

    // `_lib.sh` is sourced by the others, so it goes in whenever any hook does - a hook
    // installed without it fails at the moment it first runs.
    let library = catalogue.root.join("hooks/_lib.sh");
    if !names.is_empty() && library.is_file() {
        plan.push(Action::write_executable(
            repo.join(paths::AGENTS_HOOKS).join("_lib.sh"),
            std::fs::read(&library).map_err(|e| Error::io(&library, e))?,
            format!("hook  {}/_lib.sh", paths::AGENTS_HOOKS),
        )?);
    }

    for name in names {
        let hook = catalogue.hook(name).ok_or_else(|| {
            Error::Catalogue(format!("no such hook: {name} (see 'ai-toolbox list')"))
        })?;
        let file = format!("{name}.sh");
        plan.push(Action::write_executable(
            repo.join(paths::AGENTS_HOOKS).join(&file),
            std::fs::read(&hook.path).map_err(|e| Error::io(&hook.path, e))?,
            format!("hook  {}/{file}", paths::AGENTS_HOOKS),
        )?);
        installed.push(file);
    }
    if installed.is_empty() {
        return Ok(());
    }

    let wiring = catalogue.hook_wiring()?;
    for harness in harnesses {
        match harness {
            Harness::Claude => wire_claude(plan, repo, &wiring, &installed)?,
            Harness::Codex => wire_codex(plan, repo, &wiring, &installed)?,
            // Pi's extension API is TypeScript, so a shell hook has nothing to attach
            // to. Said out loud, because silently installing nothing looks like a bug.
            Harness::Pi => plan.warn(
                "pi: shell hooks aren't wired for pi yet (pi uses TypeScript extensions) - skipped.",
            ),
        }
    }
    Ok(())
}

/// Claude Code's wiring already spells `${CLAUDE_PROJECT_DIR}/.agents/hooks/...`, so the
/// shipped file needs no rewriting - only merging into whatever settings.json holds.
fn wire_claude(plan: &mut Plan, repo: &Path, wiring: &Value, installed: &[String]) -> Result<()> {
    let path = repo.join(paths::CLAUDE_SETTINGS);
    let mut settings = plan.read_json(&path)?;
    merge::hooks(&mut settings, wiring, installed);
    plan.push(Action::write(
        path,
        merge::to_string(&settings),
        format!(
            "wired {}  ->  {}/",
            paths::CLAUDE_SETTINGS,
            paths::AGENTS_HOOKS
        ),
    )?);
    Ok(())
}

/// Codex runs hooks from the session root, so its command paths are cwd-relative - there
/// is no `${CLAUDE_PROJECT_DIR}` to expand, and leaving one in would break every hook.
fn wire_codex(plan: &mut Plan, repo: &Path, wiring: &Value, installed: &[String]) -> Result<()> {
    let path = repo.join(paths::CODEX_CONFIG);
    let mut config = plan.read_toml(&path)?;
    let relative = rewrite_hook_commands(wiring, installed);
    convert::codex::merge_hooks(&mut config, &relative)?;
    plan.push(Action::write(
        path,
        to_toml_string(&config)?,
        format!(
            "codex {} [hooks] -> {}/",
            paths::CODEX_CONFIG,
            paths::AGENTS_HOOKS
        ),
    )?);
    Ok(())
}

/// The shipped wiring, reduced to the installed scripts and re-pointed at cwd-relative
/// paths. Returns the events map alone, which is what the Codex merge takes.
fn rewrite_hook_commands(wiring: &Value, installed: &[String]) -> Value {
    let mut events = Map::new();
    let Some(source) = wiring.get("hooks").and_then(|h| h.as_object()) else {
        return Value::Object(events);
    };
    for (event, matchers) in source {
        let mut kept = Vec::new();
        for matcher in matchers.as_array().into_iter().flatten() {
            let hooks: Vec<Value> = matcher
                .get("hooks")
                .and_then(|h| h.as_array())
                .into_iter()
                .flatten()
                .filter_map(|hook| {
                    let command = hook.get("command")?.as_str()?;
                    let script = command.rsplit('/').next()?;
                    if !installed.iter().any(|name| name == script) {
                        return None;
                    }
                    let mut rewritten = hook.clone();
                    rewritten.as_object_mut()?.insert(
                        "command".to_string(),
                        Value::String(format!("{}/{script}", paths::AGENTS_HOOKS)),
                    );
                    Some(rewritten)
                })
                .collect();
            if hooks.is_empty() {
                continue;
            }
            let mut entry = matcher.clone();
            if let Some(object) = entry.as_object_mut() {
                object.insert("hooks".to_string(), Value::Array(hooks));
            }
            kept.push(entry);
        }
        if !kept.is_empty() {
            events.insert(event.clone(), Value::Array(kept));
        }
    }
    Value::Object(events)
}

/// Add MCP presets: the canonical `.mcp.json` first, then whatever each harness needs
/// derived from it.
pub fn mcp(
    plan: &mut Plan,
    repo: &Path,
    catalogue: &Catalogue,
    names: &[String],
    harnesses: &[Harness],
) -> Result<()> {
    if names.is_empty() {
        return Ok(());
    }

    let mut chosen = Vec::new();
    for name in names {
        let preset = catalogue.preset(name).ok_or_else(|| {
            Error::Catalogue(format!("no such preset: {name} (see 'ai-toolbox list')"))
        })?;
        chosen.push(preset);
        for secret in &preset.secrets {
            plan.note_secret(secret);
        }
    }

    // Each preset carries its own servers into the conversions. Handing every preset the
    // combined set would convert each server once per preset - harmless for the output,
    // which inserts by name, but it would repeat every warning as many times as there
    // are presets on the command line.
    let pairs: Vec<(&str, &Map<String, Value>)> = chosen
        .iter()
        .map(|p| (p.name.as_str(), &p.servers))
        .collect();
    let servers: Map<String, Value> = chosen
        .iter()
        .flat_map(|preset| preset.servers.iter())
        .map(|(name, definition)| (name.clone(), definition.clone()))
        .collect();

    // The canonical file, which Claude Code and Pi both read directly.
    let shared_path = repo.join(paths::SHARED_MCP);
    let mut shared = plan.read_json(&shared_path)?;
    merge::mcp_servers(&mut shared, &servers);

    // Pi layers its knobs onto the same file, so it has to run before the write action
    // is built or the preview would show content that is one step out of date.
    let pi = harnesses
        .contains(&Harness::Pi)
        .then(|| convert::pi::convert(&pairs));
    if let Some(converted) = &pi {
        merge::overlay_server_keys(&mut shared, &converted.shared);
    }
    plan.push(Action::write(
        &shared_path,
        merge::to_string(&shared),
        format!(
            "mcp   {}  ->  {}",
            chosen
                .iter()
                .map(|p| p.name.as_str())
                .collect::<Vec<_>>()
                .join(" "),
            paths::SHARED_MCP
        ),
    )?);

    add_helpers(plan, repo, catalogue, &shared)?;

    for harness in harnesses {
        match harness {
            Harness::Claude => {}
            Harness::Codex => {
                let converted = convert::codex::servers(&pairs);
                let path = repo.join(paths::CODEX_CONFIG);
                let mut config = plan.read_toml(&path)?;
                convert::codex::merge_servers(&mut config, &converted.servers)?;
                plan.push(Action::write(
                    &path,
                    to_toml_string(&config)?,
                    format!("codex {} [mcp_servers]", paths::CODEX_CONFIG),
                )?);
                plan.warnings.extend(converted.warnings);
                // The conversion can introduce a wrapper the shared file never named.
                let as_json = toml_to_json(&config);
                add_helpers(plan, repo, catalogue, &as_json)?;
            }
            Harness::Pi => {
                let Some(converted) = &pi else { continue };
                plan.warnings.extend(converted.warnings.iter().cloned());
                // An empty override file is one more thing to keep in step for no
                // reason, so it is only written when something genuinely cannot be shared.
                if converted.overrides.is_empty() {
                    continue;
                }
                let path = repo.join(paths::PI_MCP);
                let mut document = plan.read_json(&path)?;
                merge::mcp_servers(&mut document, &converted.overrides);
                plan.push(Action::write(
                    &path,
                    merge::to_string(&document),
                    format!("mcp   {} (Pi-only overrides)", paths::PI_MCP),
                )?);
                add_helpers(plan, repo, catalogue, &document)?;
            }
        }
    }
    Ok(())
}

/// Copy in any `.agents/mcp/*.sh` launcher a document's servers name. A server whose
/// launcher is missing fails at connect time with a message that mentions neither.
fn add_helpers(
    plan: &mut Plan,
    repo: &Path,
    catalogue: &Catalogue,
    document: &Value,
) -> Result<()> {
    let text = document.to_string();
    let needle = format!("{}/", paths::AGENTS_MCP);
    let mut seen: Vec<String> = Vec::new();
    let mut rest = text.as_str();
    while let Some(at) = rest.find(&needle) {
        let after = &rest[at + needle.len()..];
        let end = after
            .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.'))
            .unwrap_or(after.len());
        let name = &after[..end];
        rest = &after[end..];
        if !name.ends_with(".sh") || seen.iter().any(|s| s == name) {
            continue;
        }
        seen.push(name.to_string());
        let Some(helper) = catalogue.helper(name) else {
            continue;
        };
        plan.push(Action::write_executable(
            repo.join(paths::AGENTS_MCP).join(name),
            std::fs::read(&helper.path).map_err(|e| Error::io(&helper.path, e))?,
            format!("mcp   {name}  ->  {}/", paths::AGENTS_MCP),
        )?);
    }
    Ok(())
}

/// Install skills into the one canonical folder, and point Claude Code at it.
///
/// `root` is the repo, or the home directory for a user-wide install.
pub fn skills(
    plan: &mut Plan,
    root: &Path,
    catalogue: &Catalogue,
    keys: &[String],
    harnesses: &[Harness],
    symlink: bool,
) -> Result<()> {
    if keys.is_empty() {
        return Ok(());
    }

    let mut chosen = Vec::new();
    for key in keys {
        // A group like `cli` installs each of its children flat, which is how they are
        // named once installed.
        let group: Vec<_> = catalogue
            .skills
            .iter()
            .filter(|s| s.group.as_deref() == Some(key.as_str()))
            .collect();
        if !group.is_empty() {
            chosen.extend(group);
            continue;
        }
        chosen.push(catalogue.skill(key).ok_or_else(|| {
            Error::Catalogue(format!("no such skill: {key} (see 'ai-toolbox list')"))
        })?);
    }

    for skill in &chosen {
        plan.push(Action::copy_tree(
            &skill.path,
            root.join(paths::AGENTS_SKILLS).join(&skill.name),
            format!(
                "skill {}  ->  {}/{}",
                skill.key,
                paths::AGENTS_SKILLS,
                skill.name
            ),
        )?);
    }

    if !harnesses.contains(&Harness::Claude) {
        return Ok(());
    }
    let link = root.join(paths::CLAUDE_SKILLS);
    if symlink {
        for action in claude_skills_link(&link)? {
            plan.push(action);
        }
    } else {
        // For a filesystem without symlinks: a second real copy, kept in step by
        // reinstalling rather than by the link.
        for skill in &chosen {
            plan.push(Action::copy_tree(
                &skill.path,
                link.join(&skill.name),
                format!(
                    "skill {}  ->  {}/{}",
                    skill.key,
                    paths::CLAUDE_SKILLS,
                    skill.name
                ),
            )?);
        }
    }
    Ok(())
}

/// The `.claude/skills` -> `../.agents/skills` link, or nothing at all when the path is
/// already someone's own content.
fn claude_skills_link(link: &Path) -> Result<Vec<Action>> {
    match std::fs::symlink_metadata(link) {
        // A real directory with skills in it is content, not an obstacle. Replacing it
        // would delete someone's work; `migrate` is the command that folds it in.
        Ok(meta) if meta.is_dir() && !meta.file_type().is_symlink() => {
            let occupied = std::fs::read_dir(link)
                .map(|entries| entries.flatten().next().is_some())
                .unwrap_or(false);
            if occupied {
                return Ok(Vec::new());
            }
            Ok(vec![Action::symlink(
                link,
                paths::CLAUDE_SKILLS_TARGET,
                format!(
                    "link  {}  ->  {}",
                    paths::CLAUDE_SKILLS,
                    paths::CLAUDE_SKILLS_TARGET
                ),
            )?])
        }
        // Pointing somewhere else on purpose: left alone, and reported by doctor.
        Ok(meta) if meta.file_type().is_symlink() => {
            let current = std::fs::read_link(link).map_err(|e| Error::io(link, e))?;
            if current.to_string_lossy() != paths::CLAUDE_SKILLS_TARGET {
                return Ok(Vec::new());
            }
            Ok(vec![Action::symlink(
                link,
                paths::CLAUDE_SKILLS_TARGET,
                format!(
                    "link  {}  ->  {}",
                    paths::CLAUDE_SKILLS,
                    paths::CLAUDE_SKILLS_TARGET
                ),
            )?])
        }
        _ => Ok(vec![Action::symlink(
            link,
            paths::CLAUDE_SKILLS_TARGET,
            format!(
                "link  {}  ->  {}",
                paths::CLAUDE_SKILLS,
                paths::CLAUDE_SKILLS_TARGET
            ),
        )?]),
    }
}

/// Scaffold the knowledge files: `AGENTS.md` for every harness, `CLAUDE.md` only when
/// Claude Code is in play, since it is nothing but the adapter that imports AGENTS.md.
pub fn knowledge_files(
    plan: &mut Plan,
    repo: &Path,
    catalogue: &Catalogue,
    harnesses: &[Harness],
) -> Result<()> {
    let scaffold = |plan: &mut Plan, template: &str, target: &str, summary: &str| -> Result<()> {
        let path = repo.join(target);
        // Never overwrite one that exists - it is the project's own knowledge, and a
        // skeleton dropped on top of it would be the worst outcome this tool could have.
        if path.exists() {
            return Ok(());
        }
        let source = catalogue.root.join(template);
        let body = std::fs::read(&source).map_err(|e| Error::io(&source, e))?;
        plan.push(Action::write(path, body, summary)?);
        Ok(())
    };

    scaffold(
        plan,
        "templates/AGENTS.template.md",
        "AGENTS.md",
        "scaffolded AGENTS.md (skeleton - fill in stack/commands/rules)",
    )?;
    if harnesses.contains(&Harness::Claude) {
        scaffold(
            plan,
            "templates/CLAUDE.template.md",
            "CLAUDE.md",
            "scaffolded CLAUDE.md (@AGENTS.md adapter)",
        )?;
    }
    Ok(())
}

/// Append the always-on charter to each harness's global config. Once per machine, not
/// per repo - which is why this takes no repo.
pub fn base_charter(
    plan: &mut Plan,
    catalogue: &Catalogue,
    harnesses: &[Harness],
    target: Option<&Path>,
) -> Result<()> {
    let home = machine::home();
    let pi_dir = machine::pi_agent_dir(&home);
    let paths: Vec<PathBuf> = match target {
        Some(path) => vec![path.to_path_buf()],
        None => harnesses
            .iter()
            .map(|h| h.charter_path(&home, &pi_dir))
            .collect(),
    };

    let source = catalogue.root.join("starters/base-charter.md");
    let body = std::fs::read_to_string(&source).map_err(|e| Error::io(&source, e))?;
    let marker = machine::CHARTER_MARKER;

    for path in paths {
        let existing = std::fs::read_to_string(&path).unwrap_or_default();
        if existing.contains(marker) {
            continue;
        }
        // Appended, never written over: these files are the user's own global rules, and
        // this block is a guest in them.
        let appended = format!("{existing}\n{marker}\n{body}{marker}\n");
        plan.push(Action::write(
            &path,
            appended,
            format!("appended base charter -> {}", display_home(&path, &home)),
        )?);
    }
    Ok(())
}

/// Drop the `.env` launcher on its own, for a hand-written server that needs it.
pub fn with_dotenv(plan: &mut Plan, repo: &Path, catalogue: &Catalogue) -> Result<()> {
    let helper = catalogue
        .helper("with-dotenv.sh")
        .ok_or_else(|| Error::Catalogue("with-dotenv.sh is not in this clone".to_string()))?;
    plan.push(Action::write_executable(
        repo.join(paths::AGENTS_MCP).join("with-dotenv.sh"),
        std::fs::read(&helper.path).map_err(|e| Error::io(&helper.path, e))?,
        format!("mcp   with-dotenv.sh  ->  {}/", paths::AGENTS_MCP),
    )?);
    Ok(())
}

/// Everything a repo needs, in one plan.
///
/// One plan rather than three applied in turn, so the whole thing previews as a unit -
/// and so the Codex config, which takes hook wiring from one step and MCP servers from
/// another, is built once from both.
pub fn everything(
    repo: &Path,
    catalogue: &Catalogue,
    harnesses: &[Harness],
    hook_names: &[String],
    preset_names: &[String],
    skill_keys: &[String],
    scaffold: bool,
) -> Result<Plan> {
    let mut plan = Plan::default();
    if scaffold {
        knowledge_files(&mut plan, repo, catalogue, harnesses)?;
    }
    hooks(&mut plan, repo, catalogue, hook_names, harnesses)?;
    mcp(&mut plan, repo, catalogue, preset_names, harnesses)?;
    skills(&mut plan, repo, catalogue, skill_keys, harnesses, true)?;
    Ok(plan)
}

fn read_json_or_empty(path: &Path) -> Result<Value> {
    if !path.is_file() {
        return Ok(Value::Object(Map::new()));
    }
    let text = std::fs::read_to_string(path).map_err(|e| Error::io(path, e))?;
    parse_json(path, &text)
}

/// Stricter than the read in [`crate::catalogue`], and deliberately so.
///
/// Everything parsed here is about to be merged into and written back, and the write
/// re-serialises the whole document - so a comment that went in would not come out.
/// [`crate::merge`] exists to leave the parts of a file nobody asked about alone, and
/// quietly deleting somebody's notes is the loudest way to break that. A trailing comma
/// carries no such meaning, so it is normalised away without comment.
fn parse_json(path: &Path, text: &str) -> Result<Value> {
    if text.trim().is_empty() {
        return Ok(Value::Object(Map::new()));
    }
    let stripped = crate::jsonc::strip(text);
    if stripped.had_comments {
        return Err(Error::json_comments(path));
    }
    serde_json::from_str(&stripped.text).map_err(|e| Error::json(path, e))
}

fn read_toml_or_empty(path: &Path) -> Result<toml::Value> {
    if !path.is_file() {
        return Ok(toml::Value::Table(toml::Table::new()));
    }
    let text = std::fs::read_to_string(path).map_err(|e| Error::io(path, e))?;
    toml::from_str(&text).map_err(|e| Error::toml(path, e))
}

fn to_toml_string(config: &toml::Value) -> Result<String> {
    toml::to_string(config)
        .map_err(|e| Error::Catalogue(format!("serialising the Codex config: {e}")))
}

/// A parsed Codex config as JSON, so the one helper scanner can read both formats.
fn toml_to_json(config: &toml::Value) -> Value {
    serde_json::to_value(config).unwrap_or(Value::Null)
}

fn display_home(path: &Path, home: &Path) -> String {
    match path.strip_prefix(home) {
        Ok(rest) => format!("~/{}", rest.display()),
        Err(_) => path.display().to_string(),
    }
}
