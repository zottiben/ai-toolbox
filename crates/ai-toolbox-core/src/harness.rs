//! The coding agents this toolkit configures, and how to tell which ones a repo uses.

use std::path::{Path, PathBuf};

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
#[serde(rename_all = "lowercase")]
pub enum Harness {
    Claude,
    Codex,
    Pi,
}

pub const ALL: [Harness; 3] = [Harness::Claude, Harness::Codex, Harness::Pi];

impl Harness {
    pub fn key(self) -> &'static str {
        match self {
            Harness::Claude => "claude",
            Harness::Codex => "codex",
            Harness::Pi => "pi",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Harness::Claude => "Claude Code",
            Harness::Codex => "Codex",
            Harness::Pi => "Pi",
        }
    }

    /// Where its always-on charter goes. Once per machine, not per repo.
    pub fn charter_path(self, home: &Path, pi_agent_dir: &Path) -> PathBuf {
        match self {
            Harness::Claude => home.join(".claude/CLAUDE.md"),
            Harness::Codex => home.join(".codex/AGENTS.md"),
            Harness::Pi => pi_agent_dir.join("AGENTS.md"),
        }
    }

    pub fn parse(text: &str) -> Option<Harness> {
        match text {
            "claude" => Some(Harness::Claude),
            "codex" => Some(Harness::Codex),
            "pi" => Some(Harness::Pi),
            _ => None,
        }
    }
}

impl std::fmt::Display for Harness {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.key())
    }
}

/// Expand `--harness` the way the bash does, including its two aliases.
pub fn select(spec: &str) -> std::result::Result<Vec<Harness>, String> {
    match spec {
        "both" => return Ok(vec![Harness::Claude, Harness::Codex]),
        "all" => return Ok(ALL.to_vec()),
        _ => {}
    }
    let mut out = Vec::new();
    for part in spec.split(',').map(str::trim).filter(|p| !p.is_empty()) {
        let harness = Harness::parse(part).ok_or_else(|| {
            format!(
                "unknown harness: {part} (use claude|codex|pi|both|all, or a comma-separated list)"
            )
        })?;
        if !out.contains(&harness) {
            out.push(harness);
        }
    }
    if out.is_empty() {
        return Err("--harness needs claude|codex|pi|both|all".to_string());
    }
    Ok(out)
}

/// Which harnesses this repo is already set up for - a home directory for one, or its
/// footprint in the repo.
///
/// The fallback matters: a repo with no harness at all gets Claude, which is what the
/// bash has always done and what keeps `ai-toolbox init` in a fresh repo behaving the
/// way it used to.
pub fn detect(repo: &Path, home: &Path) -> Vec<Harness> {
    let mut found = Vec::new();
    if home.join(".claude").is_dir()
        || repo.join(".claude").is_dir()
        || repo.join(".mcp.json").is_file()
    {
        found.push(Harness::Claude);
    }
    if home.join(".codex").is_dir() || repo.join(".codex").is_dir() {
        found.push(Harness::Codex);
    }
    if home.join(".pi").is_dir() || repo.join(".pi").is_dir() {
        found.push(Harness::Pi);
    }
    if found.is_empty() {
        found.push(Harness::Claude);
    }
    found
}

/// Which harnesses this *repo* has been configured for, ignoring what is installed on
/// the machine. This is the honest answer for a project list, where "you have Claude
/// Code installed" says nothing about the repo in front of you.
pub fn configured(repo: &Path) -> Vec<Harness> {
    let mut found = Vec::new();
    if repo.join(".claude/settings.json").is_file() || repo.join(".claude/skills").exists() {
        found.push(Harness::Claude);
    }
    if repo.join(".codex/config.toml").is_file() {
        found.push(Harness::Codex);
    }
    if repo.join(".pi/mcp.json").is_file() {
        found.push(Harness::Pi);
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn select_expands_the_aliases_and_rejects_nonsense() {
        assert_eq!(
            select("both").unwrap(),
            vec![Harness::Claude, Harness::Codex]
        );
        assert_eq!(select("all").unwrap().len(), 3);
        assert_eq!(
            select("pi,claude").unwrap(),
            vec![Harness::Pi, Harness::Claude]
        );
        assert_eq!(select("pi,pi").unwrap(), vec![Harness::Pi]);
        assert!(select("emacs").is_err());
        assert!(select("").is_err());
    }

    #[test]
    fn a_repo_with_nothing_still_gets_claude() {
        let repo = tempfile::tempdir().unwrap();
        let home = tempfile::tempdir().unwrap();
        assert_eq!(detect(repo.path(), home.path()), vec![Harness::Claude]);
        // But it is not *configured* for anything, which is a different question.
        assert!(configured(repo.path()).is_empty());
    }

    #[test]
    fn a_bare_mcp_json_counts_as_claude_because_claude_reads_it_natively() {
        let repo = tempfile::tempdir().unwrap();
        let home = tempfile::tempdir().unwrap();
        std::fs::write(repo.path().join(".mcp.json"), "{}").unwrap();
        assert!(detect(repo.path(), home.path()).contains(&Harness::Claude));
    }

    #[test]
    fn configured_reads_the_repo_and_ignores_the_machine() {
        let repo = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(repo.path().join(".codex")).unwrap();
        std::fs::write(repo.path().join(".codex/config.toml"), "").unwrap();
        assert_eq!(configured(repo.path()), vec![Harness::Codex]);
    }
}
