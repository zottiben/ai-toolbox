# mcp/presets/ — ready-to-drop MCP configs

Version-controlled MCP server configs — the "dotfiles for MCP" idea. Instead of
re-remembering how to wire Supabase or Chrome DevTools in every repo, drop the
preset in and go. Each `.json` is a mergeable `mcpServers` block.

## Two rules the research made clear

1. **Adopt, don't rebuild.** If a service ships an official MCP, use it — that's
   what these presets are. Only *build* an MCP when no CLI exists and the API is
   proprietary (for this stack, that's just eBay — spec lives in the relik repo).
2. **Few, not a fleet.** MCP tool schemas load *before* any work — a heavy server
   (e.g. GitHub's) can cost 40–55k tokens up front. Install per-repo only what
   that repo needs, and prefer a **CLI + skill** (`skills/cli/`) when a CLI already
   exists (`flyctl`, `gh`, `aws`, `supabase` writes).

## How to use a preset in a repo

MCP config for a project lives in **`.mcp.json` at the repo root** (not in
`.claude/`). Each preset file is a complete, valid `.mcp.json` on its own. Say you
want Supabase:

1. **No `.mcp.json` yet?** Just copy the preset: `cp mcp/presets/supabase.json <repo>/.mcp.json`.
   **Already have one?** Copy the inner server entry into your existing
   `mcpServers` object (merge — see the two-server example below).
2. Export the secrets it needs in your shell (`SUPABASE_ACCESS_TOKEN`,
   `SUPABASE_PROJECT_REF`); the `${VARS}` resolve from your environment — never
   paste real tokens into the file.
3. Restart Claude Code in that repo and run `/mcp` to confirm it connected (and to
   complete any OAuth step, e.g. Expo/Sentry).

Prefer not to hand-edit JSON? The CLI does it for you:
`claude mcp add-json supabase '<the server object>'` (`claude mcp --help` for the
`--scope project|user|local` flag). The server's tools are then available in that
repo only; delete the block (or `claude mcp remove`) to uninstall. Want it
everywhere? Use `--scope user` (writes to `~/.claude.json`).

A `.mcp.json` with two presets merged in:

```json
{ "mcpServers": {
    "supabase": { "command": "npx", "args": ["-y", "@supabase/mcp-server-supabase@latest", "--read-only", "--project-ref=${SUPABASE_PROJECT_REF}"], "env": { "SUPABASE_ACCESS_TOKEN": "${SUPABASE_ACCESS_TOKEN}" } },
    "context7": { "command": "npx", "args": ["-y", "@upstash/context7-mcp@latest"] }
} }
```

## Catalog

| Preset (file) | Source | Secrets | Use for | Caveat |
|---|---|---|---|---|
| `supabase.json` | official `supabase/mcp` | `SUPABASE_ACCESS_TOKEN`, `SUPABASE_PROJECT_REF` | schema, migrations, **`get_advisors` (RLS/security lint)** | ⚠️ documented RLS-bypass exfiltration risk — **`--read-only`, dev/staging only, NEVER prod** |
| `context7.json` | official Upstash | none | version-correct library docs — kills hallucinated APIs | cheapest high-ROI install |
| `chrome-devtools.json` | official Google | none | CWV / Lighthouse / network / perf the agent can't otherwise see | profiling, not e2e |
| `playwright.json` | official Microsoft | none | a11y-tree e2e automation ("does it work") | pairs with chrome-devtools |
| `figma-framelink.json` | `GLips/Figma-Context-MCP` | `FIGMA_API_KEY` | pull design context on any Figma plan | the official Dev-Mode MCP is better *with* a paid seat |
| `expo.json` | official Expo (remote) | OAuth via `/mcp` (or Expo PAT) | EAS build triage, TestFlight crash/review, RN DevTools | covers much of an "App Store Connect" need |
| `sentry.json` | official Sentry (remote) | OAuth via `/mcp` | crash RCA (Seer) in-editor | stdio alt: `npx @sentry/mcp-server` |

### Adopt these too

- **Unity** (`CoplayDev/unity-mcp`) — scene / GameObject / play-mode control for game dev. **Auto-configures:** add the Unity package (Package Manager → git URL `https://github.com/CoplayDev/unity-mcp.git?path=/MCPForUnity#main`), then *Window → MCP for Unity → Configure All Detected Clients* writes the client config (a `uv` Python server) for you — no hand-written preset.
- **Postgres MCP Pro** (`crystaldba/postgres-mcp`, needs `DATABASE_URI`) — index tuning + health. Avoid the archived `@modelcontextprotocol/server-postgres` (unpatched SQLi).
- **GitHub** (official) — but scope its toolsets, or just use `gh` + `skills/cli/gh`.
- **ClickUp** (official `mcp.clickup.com`, OAuth) · **Resend** (official `resend/resend-mcp`) — worth it only for real task/broadcast management; a plain API call beats them for one-offs.

> Exact package names / flags evolve (some shipped after my training cutoff).
> Verify against each server's README before trusting a preset, and correct it
> here — that's the point of versioning them.
