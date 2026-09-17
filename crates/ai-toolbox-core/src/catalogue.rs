//! What the clone offers: hooks, MCP presets, skills and rule snippets.
//!
//! Read off disk on every call rather than embedded (D2), because a user dropping their
//! own hook into the clone and seeing it appear is a feature, not an accident. Nothing
//! here writes, and nothing here knows about a repo.

use std::path::{Path, PathBuf};

use crate::error::{Error, Result};
use crate::hash::{self, Hash};
use crate::secrets;

#[derive(Debug, Clone, serde::Serialize)]
pub struct Catalogue {
    pub root: PathBuf,
    pub hooks: Vec<HookDef>,
    pub presets: Vec<PresetDef>,
    pub skills: Vec<SkillDef>,
    pub rules: Vec<RuleDef>,
    /// `mcp/*.sh` - the launchers some presets shell out to, copied into `.agents/mcp/`
    /// alongside them. Catalogue items in their own right because a repo can have one
    /// without the preset that pulled it in, and that is worth being able to say.
    pub helpers: Vec<HelperDef>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct HookDef {
    pub name: String,
    pub path: PathBuf,
    pub hash: Hash,
    /// The first comment line of the script, which every shipped hook uses as its
    /// one-line description.
    pub summary: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct PresetDef {
    pub name: String,
    pub path: PathBuf,
    /// Server name to definition, **in file order**. Multi-server presets are normal -
    /// `supabase-multi-env` ships two - so a preset is never assumed to be one server.
    /// The order is the author's and is preserved all the way into `.mcp.json`: a
    /// preset that lists staging before prod meant that.
    pub servers: serde_json::Map<String, serde_json::Value>,
    pub hash: Hash,
    pub secrets: Vec<secrets::Secret>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct SkillDef {
    /// How it is asked for: `pre-pr`, or `cli/gh` for one inside a group.
    pub key: String,
    /// The directory name once installed. Group skills install flat, so `cli/gh` lands
    /// as `gh` - keeping both means a repo's `gh` can be matched back to its catalogue
    /// entry without guessing.
    pub name: String,
    pub group: Option<String>,
    pub path: PathBuf,
    pub hash: Hash,
    pub description: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct RuleDef {
    pub name: String,
    pub path: PathBuf,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct HelperDef {
    pub name: String,
    pub path: PathBuf,
    pub hash: Hash,
}

impl Catalogue {
    pub fn load(root: impl AsRef<Path>) -> Result<Catalogue> {
        let root = root.as_ref().to_path_buf();
        Ok(Catalogue {
            hooks: load_hooks(&root.join("hooks"))?,
            presets: load_presets(&root.join("mcp/presets"))?,
            skills: load_skills(&root.join("skills"))?,
            rules: load_rules(&root.join("starters/rules"))?,
            helpers: load_helpers(&root.join("mcp"))?,
            root,
        })
    }

    /// The Claude Code hook wiring the installer merges from. Read on demand because
    /// only the hook commands need it.
    pub fn hook_wiring(&self) -> Result<serde_json::Value> {
        let path = self.root.join("hooks/settings.hooks.json");
        read_json(&path)
    }

    pub fn hook(&self, name: &str) -> Option<&HookDef> {
        self.hooks.iter().find(|h| h.name == name)
    }

    pub fn preset(&self, name: &str) -> Option<&PresetDef> {
        self.presets.iter().find(|p| p.name == name)
    }

    /// Look a skill up by either spelling: `cli/gh` as the catalogue lists it, or `gh` as
    /// it appears once installed. The installed form is how the inventory finds its way
    /// back here, so both have to resolve.
    pub fn skill(&self, key: &str) -> Option<&SkillDef> {
        self.skills
            .iter()
            .find(|s| s.key == key)
            .or_else(|| self.skills.iter().find(|s| s.name == key))
    }

    pub fn helper(&self, name: &str) -> Option<&HelperDef> {
        self.helpers.iter().find(|h| h.name == name)
    }

    /// Which catalogue entry, if any, owns a server name found in a repo's `.mcp.json`.
    pub fn preset_for_server(&self, server: &str) -> Option<&PresetDef> {
        self.presets.iter().find(|p| p.servers.contains_key(server))
    }

    pub fn rule(&self, name: &str) -> Option<&RuleDef> {
        self.rules.iter().find(|r| r.name == name)
    }
}

fn load_hooks(dir: &Path) -> Result<Vec<HookDef>> {
    let mut hooks = Vec::new();
    for path in files_with_extension(dir, "sh")? {
        let name = stem(&path);
        // _lib.sh is sourced by the others, never installed on its own.
        if name == "_lib" {
            continue;
        }
        hooks.push(HookDef {
            hash: hash::file(&path)?,
            summary: script_summary(&path)?,
            name,
            path,
        });
    }
    hooks.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(hooks)
}

fn load_presets(dir: &Path) -> Result<Vec<PresetDef>> {
    let mut presets = Vec::new();
    for path in files_with_extension(dir, "json")? {
        let value = read_json(&path)?;
        let servers = value
            .get("mcpServers")
            .and_then(|s| s.as_object())
            .cloned()
            .ok_or_else(|| Error::Catalogue(format!("{}: no mcpServers object", path.display())))?;
        presets.push(PresetDef {
            name: stem(&path),
            hash: hash::json(&value),
            secrets: secrets::scan(&value),
            servers,
            path,
        });
    }
    presets.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(presets)
}

/// Skills are one level deep or two: `skills/pre-pr/SKILL.md`, or a group directory like
/// `skills/cli/` holding `gh/`, `fly/` and the rest. The group itself is not installable.
fn load_skills(dir: &Path) -> Result<Vec<SkillDef>> {
    let mut skills = Vec::new();
    for path in subdirectories(dir)? {
        let name = file_name(&path);
        if path.join("SKILL.md").is_file() {
            skills.push(read_skill(&path, name.clone(), name, None)?);
            continue;
        }
        for child in subdirectories(&path)? {
            if !child.join("SKILL.md").is_file() {
                continue;
            }
            let child_name = file_name(&child);
            skills.push(read_skill(
                &child,
                format!("{name}/{child_name}"),
                child_name,
                Some(name.clone()),
            )?);
        }
    }
    skills.sort_by(|a, b| a.key.cmp(&b.key));
    Ok(skills)
}

fn read_skill(path: &Path, key: String, name: String, group: Option<String>) -> Result<SkillDef> {
    let manifest = path.join("SKILL.md");
    let text = std::fs::read_to_string(&manifest).map_err(|e| Error::io(&manifest, e))?;
    Ok(SkillDef {
        key,
        name,
        group,
        hash: hash::dir(path)?,
        description: description_of(&text),
        path: path.to_path_buf(),
    })
}

fn load_rules(dir: &Path) -> Result<Vec<RuleDef>> {
    let mut rules = Vec::new();
    for path in files_with_extension(dir, "md")? {
        let name = stem(&path);
        if name == "README" {
            continue;
        }
        rules.push(RuleDef { name, path });
    }
    rules.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(rules)
}

fn load_helpers(dir: &Path) -> Result<Vec<HelperDef>> {
    let mut helpers = Vec::new();
    for path in files_with_extension(dir, "sh")? {
        helpers.push(HelperDef {
            name: file_name(&path),
            hash: hash::file(&path)?,
            path,
        });
    }
    helpers.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(helpers)
}

/// A missing catalogue directory is an empty list, not an error: a clone that has no
/// `starters/rules` should still be able to list its hooks.
fn entries(dir: &Path) -> Result<Vec<PathBuf>> {
    if !dir.is_dir() {
        return Ok(Vec::new());
    }
    let read = std::fs::read_dir(dir).map_err(|e| Error::io(dir, e))?;
    let mut out = Vec::new();
    for entry in read {
        out.push(entry.map_err(|e| Error::io(dir, e))?.path());
    }
    out.sort();
    Ok(out)
}

fn files_with_extension(dir: &Path, extension: &str) -> Result<Vec<PathBuf>> {
    Ok(entries(dir)?
        .into_iter()
        .filter(|p| p.is_file() && p.extension().is_some_and(|e| e == extension))
        .collect())
}

fn subdirectories(dir: &Path) -> Result<Vec<PathBuf>> {
    Ok(entries(dir)?.into_iter().filter(|p| p.is_dir()).collect())
}

fn stem(path: &Path) -> String {
    path.file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned()
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned()
}

pub(crate) fn read_json(path: &Path) -> Result<serde_json::Value> {
    let text = std::fs::read_to_string(path).map_err(|e| Error::io(path, e))?;
    serde_json::from_str(&text).map_err(|e| Error::json(path, e))
}

/// The first `#` comment line after the shebang, which is how every shipped hook
/// describes itself. Anything longer than a line is documentation, not a summary.
fn script_summary(path: &Path) -> Result<Option<String>> {
    let text = std::fs::read_to_string(path).map_err(|e| Error::io(path, e))?;
    Ok(text
        .lines()
        .skip_while(|line| line.starts_with("#!"))
        .find_map(|line| line.strip_prefix("# "))
        .map(|line| line.trim().to_string()))
}

/// A skill's one-line description, as the inventory reads it too - the same SKILL.md
/// format whether the folder is in the clone or in a repo.
pub fn description_of(skill_md: &str) -> Option<String> {
    frontmatter_field(skill_md, "description")
}

/// Pull one field out of a SKILL.md YAML frontmatter block.
///
/// Deliberately not a YAML parser. The frontmatter here is a handful of scalar fields,
/// and a description that runs onto continuation lines; a real parser would be a
/// dependency and a schema for something that only has to survive being displayed.
fn frontmatter_field(text: &str, field: &str) -> Option<String> {
    let body = text.strip_prefix("---\n")?;
    let end = body.find("\n---")?;
    let block = &body[..end];

    let mut lines = block.lines().skip_while(|line| {
        !line
            .trim_end()
            .strip_prefix(field)
            .is_some_and(|rest| rest.starts_with(':'))
    });
    let first = lines.next()?;
    let mut value = first.split_once(':')?.1.trim().to_string();

    // A wrapped description continues on indented lines until the next `key:` at the
    // left margin.
    for line in lines {
        if line.is_empty() || !line.starts_with(char::is_whitespace) {
            break;
        }
        value.push(' ');
        value.push_str(line.trim());
    }

    let value = value.trim().trim_matches('"').trim_matches('\'').trim();
    (!value.is_empty()).then(|| value.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The catalogue is read from the real clone in these tests rather than a fixture.
    /// A fixture would only prove the reader can read a fixture; what matters is that it
    /// reads the shipped content, which is the thing that changes.
    fn real() -> Catalogue {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        Catalogue::load(root).expect("the clone this crate lives in")
    }

    #[test]
    fn reads_the_shipped_hooks_and_leaves_out_the_library() {
        let catalogue = real();
        assert!(catalogue.hook("format-on-edit").is_some());
        assert!(catalogue.hook("_lib").is_none());
        assert!(catalogue.hooks.iter().all(|h| h.summary.is_some()));
    }

    #[test]
    fn a_multi_server_preset_keeps_both_servers_in_the_order_it_lists_them() {
        let catalogue = real();
        let preset = catalogue.preset("supabase-multi-env").unwrap();
        assert!(preset.servers.contains_key("supabase-staging"));
        assert!(preset.servers.contains_key("supabase-prod"));
        // Not alphabetical: the file puts staging first, and that reaches .mcp.json.
        assert_eq!(
            preset.servers.keys().collect::<Vec<_>>(),
            vec!["supabase-staging", "supabase-prod"]
        );
    }

    #[test]
    fn a_server_name_resolves_back_to_the_preset_that_ships_it() {
        let catalogue = real();
        let preset = catalogue.preset_for_server("supabase-prod").unwrap();
        assert_eq!(preset.name, "supabase-multi-env");
    }

    #[test]
    fn group_skills_are_flattened_the_way_they_install() {
        let catalogue = real();
        let gh = catalogue
            .skill("cli/gh")
            .expect("cli/gh is in the catalogue");
        assert_eq!(gh.name, "gh");
        assert_eq!(gh.group.as_deref(), Some("cli"));
        // The inventory only ever sees the installed name, so that has to resolve too.
        assert_eq!(catalogue.skill("gh").map(|s| &s.key), Some(&gh.key));
        // The group directory itself is not installable.
        assert!(catalogue.skills.iter().all(|s| s.key != "cli"));
    }

    #[test]
    fn skills_carry_the_description_from_their_frontmatter() {
        let catalogue = real();
        let skill = catalogue.skill("screen-record-demo").unwrap();
        let description = skill.description.as_deref().unwrap();
        assert!(description.contains("demo"), "got: {description}");
    }

    #[test]
    fn a_wrapped_frontmatter_description_is_joined_onto_one_line() {
        let text =
            "---\nname: x\ndescription: first part\n  and the rest of it\nmodel: y\n---\n\nbody\n";
        assert_eq!(
            frontmatter_field(text, "description").as_deref(),
            Some("first part and the rest of it")
        );
        assert_eq!(frontmatter_field(text, "model").as_deref(), Some("y"));
    }

    #[test]
    fn a_missing_catalogue_directory_reads_as_empty_rather_than_failing() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(temp.path().join("hooks")).unwrap();
        let catalogue = Catalogue::load(temp.path()).unwrap();
        assert!(catalogue.hooks.is_empty());
        assert!(catalogue.rules.is_empty());
    }
}
