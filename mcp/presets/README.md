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

## Install (per repo)

Merge a preset's `mcpServers` block into the repo's `.mcp.json` (or global
`~/.claude.json`). Fill `${ENV_VARS}` from your environment — never commit tokens.

## Catalog

| Preset (file) | Source | Secrets | Use for | Caveat |
|---|---|---|---|---|
| `supabase.json` | official `supabase/mcp` | `SUPABASE_ACCESS_TOKEN`, `SUPABASE_PROJECT_REF` | schema, migrations, **`get_advisors` (RLS/security lint)** | ⚠️ documented RLS-bypass exfiltration risk — **`--read-only`, dev/staging only, NEVER prod** |
| `context7.json` | official Upstash | none | version-correct library docs — kills hallucinated APIs | cheapest high-ROI install |
| `chrome-devtools.json` | official Google | none | CWV / Lighthouse / network / perf the agent can't otherwise see | profiling, not e2e |
| `playwright.json` | official Microsoft | none | a11y-tree e2e automation ("does it work") | pairs with chrome-devtools |
| `figma-framelink.json` | `GLips/Figma-Context-MCP` | `FIGMA_API_KEY` | pull design context on any Figma plan | the official Dev-Mode MCP is better *with* a paid seat |

### Adopt these too (no preset file yet — confirm the exact endpoint/command from the source, then add one)

- **Expo** (official, free, remote `mcp.expo.dev`) — EAS build triage + TestFlight crash/review data. Covers much of an "App Store Connect" need.
- **Sentry** (official, remote `mcp.sentry.dev`, OAuth) — crash RCA in-editor.
- **Unity** (`CoplayDev/unity-mcp`) — scene / GameObject / play-mode control for game dev.
- **Postgres MCP Pro** (`crystaldba/postgres-mcp`, needs `DATABASE_URI`) — index tuning + health. Avoid the archived `@modelcontextprotocol/server-postgres` (unpatched SQLi).
- **GitHub** (official) — but scope its toolsets, or just use `gh` + `skills/cli/gh`.
- **ClickUp** (official `mcp.clickup.com`, OAuth) · **Resend** (official `resend/resend-mcp`) — worth it only for real task/broadcast management; a plain API call beats them for one-offs.

> Exact package names / flags evolve (some shipped after my training cutoff).
> Verify against each server's README before trusting a preset, and correct it
> here — that's the point of versioning them.
