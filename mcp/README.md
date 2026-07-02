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

## Reading secrets from a root `.env` (no sourcing)

`${VARS}` in a config resolve from the *process* environment — so a token that
lives only in a `.env` won't be found unless you `source` it first. To avoid that,
run the server through **`with-dotenv.sh`** (in this folder): it loads the `.env`
at launch, then execs the server. Copy it into the repo once:

```bash
mkdir -p .claude/mcp && cp <ai-toolbox>/mcp/with-dotenv.sh .claude/mcp/ && chmod +x .claude/mcp/with-dotenv.sh
```

Then set the server's `command` to it and put the real command after a `--`:

```json
"command": ".claude/mcp/with-dotenv.sh",
"args": ["--", "npx", "-y", "<server>", "..."]
```

The server now reads its env vars (e.g. `SUPABASE_ACCESS_TOKEN`) straight from your
root `.env` — no sourcing. Works for any MCP or CLI. Point at another file with
`--env-file .env.staging`. Add `--need SUPABASE_ACCESS_TOKEN` to document the
required var in the args and fail with a clear message if it's missing — instead
of the server starting and erroring later.

## Multiple environments (e.g. staging + prod)

Add one server entry per environment — they're just separate names under
`mcpServers`, differing only by `--project-ref` (refs aren't secret; hard-code
them). A Supabase personal access token is account-level, so **the same token
usually works for every project** — keep one `SUPABASE_ACCESS_TOKEN` in your
`.env` and both entries pick it up:

```json
"supabase-staging": { "command": ".claude/mcp/with-dotenv.sh",
  "args": ["--need","SUPABASE_ACCESS_TOKEN","--","npx","-y","@supabase/mcp-server-supabase@latest","--read-only","--project-ref=STAGING_REF"] },
"supabase-prod":    { "command": ".claude/mcp/with-dotenv.sh",
  "args": ["--need","SUPABASE_ACCESS_TOKEN","--","npx","-y","@supabase/mcp-server-supabase@latest","--read-only","--project-ref=PROD_REF"] }
```

Ready-made: `presets/supabase-multi-env.json`. **Keep prod `--read-only`** — a
read-write MCP against production is the documented exfiltration risk.

> Different token per env? Add `--set SUPABASE_ACCESS_TOKEN=SUPABASE_STAGING_TOKEN`
> (and the prod equivalent) before the `--` to map each env's token from `.env`
> onto the name the server reads.

