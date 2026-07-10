# ai-toolbox

A portable, harness-agnostic toolkit that **complements** AI coding harnesses
(Claude Code, Codex, OpenCode) instead of wrapping them in process.

It's a **personal leverage tool.** Its only job is to make *you* faster and keep
*you* in control — encode durable knowledge once, scaffold it into any project in
minutes, and never let the tooling get in the model's way. It is not a framework
and it does not think it knows better than you.

**Contents:** [Why](#why-this-exists) · [The 7 laws](#the-7-laws-how-we-keep-this-from-rotting)
· [Layout](#repository-layout) · [Quick start](#quick-start) · [The CLI](#the-ai-toolbox-cli)
· [Knowledge files](#knowledge-files-agentsmd--claudemd--rulesmd) · [Templates](#templates)
· [Starters](#starters) · [Skills](#skills) · [Hooks](#hooks) · [MCP](#mcp)
· [Background automation](#background-automation) · [Examples](#examples) · [Status](#status)

## Why this exists

It replaces a homegrown framework (Software Teams / JDI) that drifted into
~89k tokens of always-on process — 500-line commands, mandatory multi-agent
gates, an identity override. In practice that made the models **lazier, slower,
lower-quality, more prone to break things, and more likely to state wrong
information.** This toolkit is the opposite bet: minimal surface, native-first,
your judgment left intact.

## The 7 laws (how we keep this from rotting)

1. **Context budget is sacred.** Exactly one artifact loads always-on: the base
   charter. Everything else loads only where it's relevant.
2. **A skill is a recipe, not a program.** ~60-line cap. If it needs branching
   cathedrals, the model should decide, not the doc.
3. **One source of truth.** Author-once, read natively. No macro/include system,
   no "author A → generate B → drift."
4. **No custom orchestration.** Lean entirely on the harness's native subagents,
   planning, and todos.
5. **Never override the model's identity or judgment.**
6. **Build only for stacks actually in use** (on-disk *or* at-work). Nothing
   speculative.
7. **Inventory native capability first; build only in the gaps.**

## Repository layout

The product is an **authoring system**, not a pile of bespoke files. You point it
at any project and it produces the knowledge files you then fill in. Every section
below documents one directory — this README is the single doc for the whole repo.

```
ai-toolbox/
  install.sh           # one-time: puts the `ai-toolbox` command on your PATH
  bin/ai-toolbox       # the CLI that installs any of the below into a repo (idempotent)
  templates/           # blank, well-structured skeletons you fill in per project
    AGENTS.template.md   # canonical project knowledge (the source of truth)
    CLAUDE.template.md   # thin Claude Code adapter (@AGENTS.md)
    RULES.template.md    # OPTIONAL shared hard-constraints doc
    design/              # greenfield design-guide skeletons (concept, design-system, GDD, product-brief)
    feature-brief.md     # OPTIONAL crisp target for a feature/task
  starters/            # reusable, opt-in best-practice content (a library)
    base-charter.md      # universal norms — the one always-on artifact
    rules/               # rule snippets by stack (unity, go, react, expo, supabase, bun, …)
    agents/              # generic agent starters + an authoring template
  skills/              # the tooling (portable playbooks + thin harness adapters)
    init/ capture/ lint/ pre-pr/ audit/ babysit-pr/ cli/
  mcp/                 # capability layer — registry, template, presets/ (ready configs)
  hooks/               # functional guardrails — shell hooks that ENFORCE the AGENTS.md rules
  background/          # recurring/unattended automation — /loop & /goal guide, babysit-pr, GH Action
  examples/            # real AGENTS.md packs (a SaaS + a Unity game) — proof the format works
```

## Quick start

`ai-toolbox` is a library you pull *from*, not a dependency you install. Clone it
once and run the installer; the **`ai-toolbox` CLI** then does per-repo setup for
you — no more hand-copying files or merging JSON. Nothing here is all-or-nothing.

**Once per machine**
```bash
git clone <this repo> ~/Developer/ai-toolbox && cd ~/Developer/ai-toolbox
./install.sh                 # puts `ai-toolbox` on your PATH (no manual PATH edits)
ai-toolbox base-charter      # appends the always-on charter to ~/.claude/CLAUDE.md
```

**Per repo — the smart path (recommended).** From inside the target repo:
```bash
ai-toolbox init     # scaffolds AGENTS.md/CLAUDE.md + installs the tailored tooling you confirm
```
Prefer the model to author `AGENTS.md` by interview? Install the skill once
(`ai-toolbox skill init --user`) and run `/toolbox-init` in Claude Code instead — it
detects the stack, asks for the non-obvious rules, then does the same install. See
first, install nothing? `ai-toolbox recommend` (or `ai-toolbox status` for what's
already there).

**Per repo — take one piece at a time.** Every command is idempotent; run it from
the repo (or add `--repo <path>`):

| Want… | Run | Details |
|---|---|---|
| Project knowledge | `ai-toolbox init` (or the `/toolbox-init` skill to author by interview) | [Templates](#templates) |
| Enforced guardrails | `ai-toolbox hooks` | [Hooks](#hooks) |
| An MCP (Supabase, Chrome…) | `ai-toolbox mcp supabase context7` | [MCP](#mcp) |
| Helper skills | `ai-toolbox skill pre-pr capture` (or `--user` for everywhere) | [Skills](#skills) |
| Stack rule snippets | `ai-toolbox rules go react` (prints them to inline into `AGENTS.md`) | [Starters](#starters) |
| A greenfield design guide | `cp -r templates/design <repo>/design/` and fill the skeletons | [Templates](#templates) |

Start small: an `AGENTS.md` + `ai-toolbox hooks` already puts you ahead.

## The `ai-toolbox` CLI

The deterministic setup engine for the whole toolkit. Instead of hand-copying files
and merging JSON, you run one idempotent command. It's what `/toolbox-init` calls
under the hood, and reads the toolbox's own directories at runtime — so
`ai-toolbox list` never drifts.

**Setup** — from the repo root, `./install.sh` symlinks `ai-toolbox` into a bin dir
already on your `PATH` (preferring `~/.local/bin`); if none exists it creates
`~/.local/bin` and adds it to your shell rc. The link points at `bin/ai-toolbox`, so
`git pull` updates the command. No installer? Run it by path
(`~/Developer/ai-toolbox/bin/ai-toolbox …`) — it resolves its own location through
symlinks, and `AI_TOOLBOX` overrides it.

| Command | Does |
|---|---|
| `ai-toolbox init [--yes\|--dry-run]` | Scaffold `AGENTS.md`/`CLAUDE.md`, then bootstrap the tailored functional layer. The per-repo entry point. |
| `ai-toolbox bootstrap [--yes\|--dry-run]` | Just the functional layer — detect the stack, show a tailored set, install each group you confirm. |
| `ai-toolbox recommend` | Print the recommended set for this repo (read-only). |
| `ai-toolbox status` | What's installed here (hooks · MCP servers · skills · knowledge files). |
| `ai-toolbox list` | Everything the toolbox offers (hooks · presets · skills · rules). |
| `ai-toolbox hooks [name...]` | Copy hook scripts + wire only their entries into `.claude/settings.json` (default: all). |
| `ai-toolbox mcp <preset...>` | Merge MCP preset(s) into `.mcp.json`; prints required secrets; auto-drops `with-dotenv.sh` when a preset needs it. |
| `ai-toolbox skill <name...> [--user]` | Copy skill(s) into `.claude/skills/` (or `~/.claude/skills/`). A group name like `cli` installs each child. |
| `ai-toolbox rules <stack...>` | **Print** rule snippets to inline into `AGENTS.md` (nothing is written). |
| `ai-toolbox with-dotenv` | Drop the `.env` loader into `.claude/mcp/`. |
| `ai-toolbox base-charter` | Append the always-on charter to `~/.claude/CLAUDE.md` (once per machine). |
| `ai-toolbox help` | The full list. |

**Notes.** `--repo <path>` targets another repo (default: current dir). Every
command is idempotent — hooks dedupe, MCP servers overwrite by name, `base-charter`
is marker-guarded. JSON merges need `jq` **or** `python3` on `PATH`. It writes only
the target repo's `.claude/` and `.mcp.json` (and `~/.claude/CLAUDE.md` for
`base-charter`); it never writes a real secret. What it *can't* do for you: export
MCP secrets, complete OAuth (`/mcp`), restart Claude Code — it prints those
follow-ups after an install.

## Knowledge files: AGENTS.md · CLAUDE.md · RULES.md

Three files with non-overlapping roles:

- **`AGENTS.md`** — the single source of truth. Stack, layout, commands, hard
  rules. Cross-harness (Codex + OpenCode native; Claude Code via the adapter).
- **`CLAUDE.md`** — thin. Default is literally `@AGENTS.md`, plus optional
  Claude-only notes. Kept near-empty so it can't drift.
- **`RULES.md`** — **opt-in.** Only worth it for hard constraints shared across
  projects or read by humans/CI too; then `AGENTS.md` imports it. Otherwise skip
  it — no empty stubs.

The **base charter** (`starters/base-charter.md`) is the one always-on artifact. It
goes in each harness's *global* config (`~/.claude/CLAUDE.md` import,
`~/.codex/AGENTS.md`, OpenCode global), on every machine you clone this onto —
`ai-toolbox base-charter` does it.

## Templates

Blank, well-structured skeletons you copy into a repo and fill. Nothing is
generated — once copied, the file is yours (no regeneration pipeline, no drift). The
fastest path is `ai-toolbox init` (or the `/toolbox-init` skill), which does the
`AGENTS.md` + `CLAUDE.md` step for you, stack-aware; reach for the raw templates
when you'd rather write by hand.

| Template | Copy to | Then |
|---|---|---|
| `AGENTS.template.md` | `<repo>/AGENTS.md` | fill stack / layout / commands / hard-rules; inline relevant `starters/rules/*`. |
| `CLAUDE.template.md` | `<repo>/CLAUDE.md` | usually just `@AGENTS.md` (the Claude Code adapter). |
| `RULES.template.md` | `<repo>/RULES.md` | **OPTIONAL** — only for constraints shared across projects or read by humans/CI. |
| `feature-brief.md` | `<repo>/docs/<feature>.md` | **OPTIONAL** crisp target for a big or fuzzy task. Not a gate. |
| `design/` | `<repo>/design/` | greenfield design guide — see below. |

### The design layer (greenfield)

Upstream, design-first artifacts a *greenfield* project needs before there's code to
harvest. Fill them yourself or with Claude Design; keep each lean (a compass, not a
novel). `ai-toolbox init`'s greenfield mode scaffolds these and writes a thin
`AGENTS.md` whose **Design** section points at them.

| Template | For | Captures |
|---|---|---|
| `concept.md` | any project | vision, core loop, pillars + anti-pillars, scope/non-goals, MVP tiers |
| `design-system.md` | any project with UI | brand voice + colour/type/spacing tokens, components, usage rules |
| `system-gdd.md` | games | per-system deep dive (rules, formulas, edge cases, tuning, acceptance) |
| `product-brief.md` | sites & SaaS | problem, users/JTBD, flows, screens & states, requirements |

Two life-stages, one body of knowledge: **greenfield**, the design guide leads and
code follows it; **brownfield**, as code lands, conventions graduate into `AGENTS.md`
hard rules and `design-system.md` tokens graduate into a code conformance gate (as
in the `relik` example). *The artifact shapes, not the machinery* — no approval
gates, no review swarms; just good skeletons.

## Starters

Reusable, **opt-in** best-practice content — a library you pull from and then edit.
Nothing here is mandatory or always-on.

- **`base-charter.md`** — universal engineering norms; the one always-on artifact.
  `ai-toolbox base-charter` appends it to `~/.claude/CLAUDE.md` (once per machine;
  Codex: append to `~/.codex/AGENTS.md`).
- **`rules/`** — lean per-stack snippets of the non-obvious footguns. Available:
  `typescript` · `go` · `react` · `expo` · `supabase` · `bun` · `vue` · `laravel`
  · `aws` · `unity`. These are **not** installed as files — you **inline** the
  relevant bullets into a project's `AGENTS.md`, deleting what doesn't apply.
  `ai-toolbox rules <stack…>` prints them; `ai-toolbox init` inlines the matching
  ones automatically. Seeds, not gospel — keep each short.
- **`agents/`** — generic subagent starters + `AGENT.template.md`, and one worked
  example (`convention-reviewer.md`, reviews a diff against the project's own
  `AGENTS.md` rules). Copy one into `<repo>/.claude/agents/<name>/AGENT.md` (or
  `~/.claude/agents/`) and customize.

**When NOT to build an agent (the important part).** Your harness already ships
`/code-review`, `/security-review`, `/verify`, `/run`, plus native subagents. A
custom agent earns its place only when it needs an **isolated context**, is a
**distinct review lens** native review doesn't apply, or is a **project-specific
specialist**. If a native tool does the job, use the native tool. Keep the folder
small — a roster of agents is how frameworks rot.

## Skills

Portable markdown skills (`<name>/SKILL.md` — YAML frontmatter + a short body).
Each becomes a `/slash-command` and is auto-selected by its `description`.

| Skill | Does |
|---|---|
| `init` | scaffold a project's `AGENTS.md` + bootstrap the functional layer (stack-aware, greenfield + brownfield) |
| `capture` | turn a correction into a permanent `AGENTS.md` rule (the flywheel) |
| `lint` | health-check a knowledge file for drift/bloat |
| `pre-pr` | run the project's real checks + native review before you push |
| `audit` | self-audit the whole toolbox for drift |
| `babysit-pr` | shepherd a PR to green, triaging each item |
| `cli/*` | CLI-wrapper skills — `fly`, `gh`, `supabase`, `eas`, `aws` |

Install with `ai-toolbox skill <name…>` (add `--user` to put them in
`~/.claude/skills/` so `/<name>` works in every repo; a group name like `cli`
installs each child). **Codex** reads the same `SKILL.md` from `~/.codex/skills/` —
symlink to use a skill in both harnesses (Claude-only frontmatter is ignored).

## Hooks

Where the toolkit stops being advisory: your `AGENTS.md` *tells* the model the
rules; a hook *enforces* them — deterministically, every time. These are **Claude
Code** hooks (configured in `settings.json`); the shell logic is reusable.

| Script | Event | Does |
|---|---|---|
| `format-on-edit.sh` | PostToolUse (Write\|Edit) | formats the edited file with the project's own prettier/eslint/gofmt/ruff. Never blocks. |
| `guard-irreversible.sh` | PreToolUse (Bash) | blocks release-tag pushes (billed builds), force-push, `rm -rf`, secret-file reads. |
| `protect-generated.sh` | PreToolUse (Write\|Edit) | blocks edits to generated files (regenerate from source instead). |
| `conventional-commit.sh` | PreToolUse (Bash) | enforces Conventional Commits on `git commit -m`. |
| `session-context.sh` | SessionStart | injects branch / uncommitted count / last commit so a session starts oriented. |

Blocking hooks use **exit code 2** (stderr is fed back to the model as the reason);
context hooks emit `hookSpecificOutput.additionalContext` on stdout. Install with
`ai-toolbox hooks` (copies the scripts + `_lib.sh` and wires only their entries into
`.claude/settings.json`; needs `jq` or `python3`). Run `/hooks` in Claude Code to
confirm they're registered.

**Tune them** — the patterns are starters: `guard-irreversible.sh`'s release-tag
block is opt-in (drop it where it doesn't apply); `protect-generated.sh` reads
`GENERATED_GLOBS` (pipe-separated regex) to override its defaults. **Safety:** hooks
run arbitrary shell with your permissions on every matching action — read a script
before enabling it, and keep them fast (they're in the critical path).

## MCP

How the model reaches systems it otherwise can't touch. This layer version-controls
**which MCPs you use, when to reach for each, and how to wire them**. It stores **no
secrets** — server definitions carry `${ENV_VAR}` placeholders; real tokens stay in
your environment.

**The registry — when to reach for which:**

| MCP | Reach for it when… |
|---|---|
| **chrome-devtools** | debugging a web page — DOM/console/network, perf traces, Lighthouse, screenshots. The web "see it actually work". |
| **claude-in-chrome** | you need a real Chrome you're logged into — authenticated flows, apps behind a login, acting as yourself. |
| **figma** | implementing a design — pull frames, measurements, colours, tokens. Feeds the design-system layer. |
| **clickup** | pulling ticket/task context into the work, or updating status. The planning source of truth. |
| **ios-simulator** | verifying an Expo / React Native change — build, run, tap, screenshot. The mobile "see it work". |
| **unity** | game dev — drive the editor, enter play mode, run tests, inspect the scene. The game "see it work". |

The through-line: prefer the MCP that lets you *verify the real thing*
(chrome-devtools / ios-simulator / unity), *pull source-of-truth context* (figma /
clickup), or *act as the user* (claude-in-chrome). A project's `AGENTS.md` should
name which to use. Config lives per harness: Claude Code project `.mcp.json` or
global `~/.claude.json`; Codex `~/.codex/config.toml`; OpenCode its own file.

**Two rules the research made clear.** (1) **Adopt, don't rebuild** — if a service
ships an official MCP, use it; only *build* one when no CLI exists and the API is
proprietary. (2) **Few, not a fleet** — MCP tool schemas load *before* any work (a
heavy server can cost 40–55k tokens up front), so install per-repo only what that
repo needs, and prefer a **CLI + skill** (`skills/cli/`) when a CLI already exists.

**Presets** (`mcp/presets/`) are ready-to-drop, mergeable configs. `ai-toolbox mcp
<preset…>` merges them into `.mcp.json`, prints the secrets they expect, and reminds
you to `/mcp`:

| Preset | Source | Secrets | Use for | Caveat |
|---|---|---|---|---|
| `supabase` | official `supabase/mcp` | `SUPABASE_ACCESS_TOKEN`, `SUPABASE_PROJECT_REF` | schema, migrations, `get_advisors` (RLS/security lint) | ⚠️ documented RLS-bypass exfiltration risk — **`--read-only`, dev/staging only, NEVER prod** |
| `context7` | official Upstash | none | version-correct library docs — kills hallucinated APIs | cheapest high-ROI install |
| `chrome-devtools` | official Google | none | CWV / Lighthouse / network / perf | profiling, not e2e |
| `playwright` | official Microsoft | none | a11y-tree e2e automation ("does it work") | pairs with chrome-devtools |
| `figma-framelink` | `GLips/Figma-Context-MCP` | `FIGMA_API_KEY` | pull design context on any Figma plan | official Dev-Mode MCP is better *with* a paid seat |
| `expo` | official Expo (remote) | OAuth via `/mcp` | EAS build triage, TestFlight crash/review, RN DevTools | covers much of an "App Store Connect" need |
| `sentry` | official Sentry (remote) | OAuth via `/mcp` | crash RCA (Seer) in-editor | stdio alt: `npx @sentry/mcp-server` |

**Adopt these too:** **Unity** (`CoplayDev/unity-mcp`, self-configures via *Window →
MCP for Unity → Configure*), **Postgres MCP Pro** (`crystaldba/postgres-mcp`, needs
`DATABASE_URI` — avoid the archived `server-postgres`, unpatched SQLi), **GitHub**
(official — scope its toolsets, or just use `gh` + `skills/cli/gh`), **ClickUp** /
**Resend** (only for real task/broadcast management). Package names/flags evolve —
verify against each server's upstream README before trusting a preset.

**Secrets from a root `.env`.** `${VARS}` resolve from the *process* environment, so
a token that lives only in `.env` won't be found unless sourced. Run the server
through **`with-dotenv.sh`** (`ai-toolbox with-dotenv`, or `ai-toolbox mcp` drops it
automatically for presets that use it): it loads `.env` at launch, then execs the
server. Set the server's `command` to `.claude/mcp/with-dotenv.sh` and put the real
command after a `--`; `--need VAR` documents + requires a var, `--env-file` picks
another file, `--set TGT=SRC` remaps a per-env token name.

**Multiple environments.** Add one server entry per env (they differ only by
`--project-ref`; refs aren't secret). A Supabase PAT is account-level, so the same
`SUPABASE_ACCESS_TOKEN` usually works for every project — see
`presets/supabase-multi-env.json`. **Keep prod `--read-only`** — a read-write MCP
against production is the documented exfiltration risk.

## Background automation

The "set it and forget it" layer. Most of this is **built into Claude Code** — the
job here is to point you at it and encode the one rule that keeps it safe.

| Feature | What | Good for |
|---|---|---|
| `/loop [interval] [prompt]` | re-runs a prompt on an interval in-session (7-day auto-expiry). Can call skills: `/loop 20m /babysit-pr 1234`. Bare `/loop` runs a maintenance prompt (customize via `.claude/loop.md`). | poll CI, tend a PR, periodic checks while the terminal is open |
| `/goal <condition>` | keeps working turn-after-turn until a condition holds. **Always cap it:** "… stop after N turns". | "all tests pass and lint clean" |
| Background Bash (Ctrl-B) | move dev servers / builds / watchers off the main thread; completion lands as a notification | long-running processes, no polling |
| GitHub Action (`anthropics/claude-code-action@v1`) | the battle-tested unattended agent — PR review, `@claude` mentions, scheduled reports, CI autofix | true unattended, runs in CI not your laptop |
| Routines (managed cloud cron) | scheduled agents that run with your laptop closed (min 1h). Research preview — a green run ≠ success. | nightly triage, dependency bumps, docs-drift PRs |

**The one rule: bound the loop.** Every reliable setup engineers against the
runaway-agent failure mode — turn caps (`/goal … stop after N`), `/loop`'s 7-day
expiry, `--max-turns` in CI, and **sandboxing for any
`--dangerously-skip-permissions` run.** Never wire an unbounded auto-fix loop.

What's here: `background/claude-github-action.yml` (a drop-in capped workflow — `cp`
it to `<repo>/.github/workflows/claude.yml`, add an `ANTHROPIC_API_KEY` secret),
`background/loop.md.template` (customize what a bare `/loop` does per repo), and the
**`babysit-pr`** skill — a *triaging* PR watcher that assesses each item as
fix/dismiss/escalate rather than blindly auto-applying.

## Examples

Real, source-verified `AGENTS.md` files — proof the format works, and a model to
imitate. Read them for **shape and altitude**, not to copy verbatim.

| Example | Stack | Shows |
|---|---|---|
| `relik-AGENTS.md` | Bun / Go / Expo / Supabase SaaS | hard rules an AI can't guess — RLS on new tables, draft-filtered aggregates, billed-tag guard, never-edit generated types |
| `keepy-uppy-AGENTS.md` | Unity 6 game | game-dev rules — empty-scene/code-gen, asmdef boundaries, Godot-port magic numbers, stubbed Steam |

Match their shape: a tight header (stack + layout + real commands), then a short list
of *only the non-obvious, get-it-wrong-without-being-told* rules — each with the
rule, why it matters, and how it's enforced. If yours grows past ~100 lines, run the
`lint` skill on it.

## Status

- [x] Format + convention proven (a ~90-line `AGENTS.md` reliably steers a fresh
      model on hard, safety-critical rules — validated adversarially, 3/3)
- [x] Agnostic structure: templates + starters + skills + examples
- [x] The three templates (`AGENTS` / `CLAUDE` / `RULES`)
- [x] Group 1 skills: `init` (generator), `capture` (gotcha flywheel), `lint` (health-check)
- [x] Group 3 (starter library) — stack snippets + agent starters
- [x] Examples — `relik` (SaaS) + `keepy-uppy` (Unity) — format proven on a SaaS *and* a game
- [x] Design layer — 4 greenfield templates + mode-aware generator
- [x] Group 2 — `pre-pr` gate + reproduce-before-you-fix norm (in base charter)
- [x] Group 4 (ergonomics & governance) — portable MCP config + self-audit skill + feature-brief template
- [x] Bootstrapper — `install.sh` + the `ai-toolbox` CLI (one-command per-repo setup); docs consolidated into this README
- [~] Functional layer — hooks (tested) · MCP presets · CLI skills (fly/gh/supabase/eas/aws) · background layer · capture+init upgrades
- [ ] Greenfield example (chief-of-geese); validate skills live; retire Software Teams / JDI from repos
