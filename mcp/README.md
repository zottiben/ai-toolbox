# mcp/ — the portable capability layer

MCPs are how the model reaches systems it otherwise can't touch. This folder
version-controls **which MCPs you use, when to reach for each, and how to wire
them** — so any machine you clone onto gets the same capability layer.

It does *not* store secrets. Server definitions go in `mcp.json.template` with
env-var placeholders; real tokens stay in your environment, never in git.

## How to use

- **Want a ready config?** Grab it from **`presets/`** — mergeable `.json` files
  for the common servers, with step-by-step install (see `presets/README.md`).
- **A server not in presets?** Use `mcp.json.template` as the skeleton and fill it.
- **When to reach for which** is the registry below. Name the right MCP in a
  project's `AGENTS.md` so the model knows to use it.

## The registry — your MCPs and when to reach for each

| MCP | Reach for it when… |
|---|---|
| **chrome-devtools** | debugging a web page — DOM/console/network, performance traces, Lighthouse, screenshots. The web "see it actually work". |
| **claude-in-chrome** | you need a real Chrome you're logged into — authenticated flows, apps behind a login, acting as yourself. |
| **figma** | implementing a design — pull frames, measurements, colours, tokens. Feeds the design-system layer (`templates/design/design-system.md`). |
| **clickup** | pulling ticket/task context into the work, or updating status. The planning source of truth. |
| **ios-simulator** | verifying an Expo / React Native change — build, run, tap, screenshot. The mobile "see it actually work". |
| **unity** | game dev — drive the editor, enter play mode, run tests, inspect the scene. The game "see it actually work". |

**The through-line:** prefer the MCP that lets you *verify the real thing* over
guessing (chrome-devtools / ios-simulator / unity), *pull source-of-truth context*
(figma / clickup), or *act as the user* (claude-in-chrome). A project's `AGENTS.md`
should name which to use — e.g. "verify mobile changes in the ios-simulator MCP",
"drive the Unity editor via the unity MCP".

## Where config lives (per harness)

- **Claude Code:** project `.mcp.json`, global `~/.claude.json` (`mcpServers`), or
  via plugins / claude.ai connectors.
- **Codex:** `~/.codex/config.toml`.
- **OpenCode:** its own config file.

## Wiring a new machine

1. Copy `mcp.json.template` → your harness's MCP config location.
2. Fill each server's real `command`/`args`/`url` (from a working machine).
3. Provide secrets via environment variables (`${FIGMA_TOKEN}` etc.) — never inline.
4. Never commit a config that contains a real token.
