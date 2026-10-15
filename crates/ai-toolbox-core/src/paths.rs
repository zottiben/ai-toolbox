//! The canonical repo layout, in one place.
//!
//! One copy of everything that has content, under the tool-agnostic `.agents/` tree,
//! plus `AGENTS.md` and `.mcp.json` at the root. Codex and Pi read `AGENTS.md` and the
//! skills natively; `.mcp.json` is read by Claude Code and by Pi's MCP adapter, not by
//! Pi's built-in MCP.
//! Claude Code reads neither `.agents/skills` nor `AGENTS.md`, but it does follow a
//! symlinked skills directory and it does take an arbitrary hook command path - so
//! `.claude/` holds only pointers.

pub const AGENTS_SKILLS: &str = ".agents/skills";
pub const AGENTS_HOOKS: &str = ".agents/hooks";
pub const AGENTS_MCP: &str = ".agents/mcp";
pub const SHARED_MCP: &str = ".mcp.json";

pub const CLAUDE_SETTINGS: &str = ".claude/settings.json";
pub const CLAUDE_SKILLS: &str = ".claude/skills";
pub const CODEX_CONFIG: &str = ".codex/config.toml";
/// Pi's own MCP file, read by its built-in MCP (Pi 0.99 and later). It holds complete
/// server definitions in Pi's format, never adapter settings.
pub const PI_MCP: &str = ".pi/mcp.json";
/// pi-mcp-adapter's project file (adapter 3.0 and later): settings, imports and the
/// per-server keys that would be wrong for Claude Code or Pi's built-in MCP.
pub const PI_MCP_ADAPTER: &str = ".pi/mcp-adapter.json";

/// What `.claude/skills` has to point at. Relative on purpose: an absolute link would
/// resolve to the main worktree from inside every other one, which looks like it works
/// right up until two worktrees need different skills.
pub const CLAUDE_SKILLS_TARGET: &str = "../.agents/skills";

/// Where older installs kept a per-harness copy of everything. Their presence is what
/// `ai-toolbox migrate` exists to resolve.
pub const LEGACY_DIRS: [&str; 5] = [
    ".claude/hooks",
    ".claude/mcp",
    ".codex/hooks",
    ".codex/mcp",
    ".pi/mcp",
];

/// Files the operating system leaves lying about, which are not configuration.
///
/// This matters more than it looks. macOS writes `.DS_Store` into any directory somebody
/// opens in Finder, and one inside a skill folder would change that folder's hash - so
/// the skill would read as edited against the catalogue, for ever, and doctor would keep
/// advising a person about a file they never touched. Found in the wild, in
/// `.agents/.DS_Store` and `.agents/skills/.DS_Store`.
pub fn is_os_noise(name: &str) -> bool {
    matches!(
        name,
        ".DS_Store" | "Thumbs.db" | "desktop.ini" | ".localized"
    )
}

/// The Pi MCP adapter package. Pi 0.99 and later has MCP built in, so this is optional
/// there; older Pi needs it for any MCP at all. It is installed once per machine rather
/// than per repo, and once installed it replaces the built-in MCP in sessions.
pub const PI_MCP_PACKAGE: &str = "pi-mcp-adapter";
