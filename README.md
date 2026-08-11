# ai-toolbox

A portable, harness-agnostic toolkit that **complements** AI coding harnesses
(Claude Code, Codex, Pi, OpenCode) instead of wrapping them in process. **One copy of
everything that has content**, in the tool-agnostic places: `AGENTS.md`, `.mcp.json`,
and `.agents/{skills,hooks,mcp}`. Codex and Pi read those natively. Claude Code reads
neither `AGENTS.md` nor `.agents/`, so it gets pointers instead of copies - a one-line
`CLAUDE.md`, a `.claude/skills` symlink, and hook wiring that references the shared
scripts. Nothing is authored, or maintained, twice.

It's a **personal leverage tool.** Its only job is to make *you* faster and keep
*you* in control — encode durable knowledge once, scaffold it into any project in
minutes, and never let the tooling get in the model's way. It is not a framework
and it does not think it knows better than you.

**Contents:** [The 7 laws](#the-7-laws-how-we-keep-this-from-rotting)
· [Layout](#repository-layout) · [Quick start](#quick-start) · [The CLI](#the-ai-toolbox-cli)
· [Knowledge files](#knowledge-files-agentsmd--claudemd--rulesmd) · [Templates](#templates)
· [Starters](#starters) · [Skills](#skills) · [Hooks](#hooks) · [MCP](#mcp)
· [Background automation](#background-automation) · [Examples](#examples)

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
    lib/                 # its helpers: walkthrough UI + steps, MCP converters, layout migration
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
```
That is it. The symlink points at `bin/ai-toolbox` in the clone, so a `git pull`
updates the command with no re-install. The other two once-per-machine jobs —
`ai-toolbox base-charter` (the always-on charter into each harness's global config) and
`ai-toolbox pi-init` (Pi's MCP client package, which its core doesn't ship) — are
offered by `ai-toolbox setup` below, or you can run them directly.

**Per repo — start here.** From inside the target repo:
```bash
ai-toolbox setup    # the guided walkthrough
```
One interview, one plan to approve, nothing written until you do. It picks the
harnesses (pre-checking the ones you have), offers the once-per-machine bits, folds an
older per-harness layout into `.agents/` if it finds one, scaffolds the knowledge files,
installs the MCPs/hooks/skills you confirm, takes any missing MCP tokens straight into
the repo `.env`, and finishes by telling you exactly how to start each harness. Install
[charmbracelet/gum](https://github.com/charmbracelet/gum) (`brew install gum`) for the
full TUI — without it the same walkthrough runs on plain prompts, and
`AI_TOOLBOX_NO_GUM=1` forces that mode.

**Which entry point?** They differ only in how much they decide for you:

| | Interactive | Writes knowledge files | Best for |
|---|---|---|---|
| `ai-toolbox setup` | full walkthrough | yes (skeletons) | the normal case, and any first run |
| `ai-toolbox init [--yes]` | four `[Y/n]` prompts | yes (skeletons) | you know what you want; `--yes` for unattended/CI |
| `ai-toolbox bootstrap [--yes]` | four `[Y/n]` prompts | no | adding tooling to a repo whose `AGENTS.md` already exists |
| `/toolbox-init` (skill) | model interviews you | yes (**authored**, not skeletons) | when you want the model to write `AGENTS.md` properly |

Only the skill actually *authors* `AGENTS.md` — the CLI drops a skeleton for you to fill
in. Install it once with `ai-toolbox skill init --user`. See first, install nothing?
`ai-toolbox recommend` (or `ai-toolbox status` for what's already there).

Every per-repo command auto-detects which harnesses are initialised (`~/.claude`,
`~/.codex`, `~/.pi`); `--harness claude|codex|pi|both|all`, or a comma-separated list
like `--harness claude,pi`, overrides it.

**Per repo — take one piece at a time.** Every command is idempotent; run it from
the repo (or add `--repo <path>`):

| Want… | Run | Details |
|---|---|---|
| The whole thing, guided | `ai-toolbox setup` | [Quick start](#quick-start) |
| Project knowledge | `ai-toolbox init` (or the `/toolbox-init` skill to author by interview) | [Templates](#templates) |
| To collapse an older layout | `ai-toolbox migrate` (`--dry-run` first) | [What lands in your repo](#what-lands-in-your-repo) |
| Enforced guardrails | `ai-toolbox hooks` | [Hooks](#hooks) |
| An MCP (Supabase, Chrome…) | `ai-toolbox mcp supabase context7` | [MCP](#mcp) |
| Helper skills | `ai-toolbox skill pre-pr capture` (or `--user` for everywhere) | [Skills](#skills) |
| Stack rule snippets | `ai-toolbox rules go react` (prints them to inline into `AGENTS.md`) | [Starters](#starters) |
| A greenfield design guide | `cp -r templates/design <repo>/design/` and fill the skeletons | [Templates](#templates) |

Start small: an `AGENTS.md` + `ai-toolbox hooks` already puts you ahead.

## What lands in your repo

One copy of everything that has content, in the tool-agnostic places. Per-harness
directories hold **pointers and generated output only** - nothing you maintain by hand,
nothing to keep in sync.

```
AGENTS.md                 instructions            Codex + Pi read natively
.mcp.json                 MCP servers             Claude Code + Pi read natively
.agents/skills/<name>/    skills                  Codex + Pi read natively
.agents/hooks/*.sh        hook scripts            referenced by both wirings
.agents/mcp/*.sh          .env helpers            referenced by every harness

CLAUDE.md                 one line: @AGENTS.md    Claude Code doesn't read AGENTS.md
.claude/skills            symlink -> ../.agents/skills
.claude/settings.json     hook wiring only
.codex/config.toml        generated: MCP tables + hook wiring
.pi/mcp.json              only a server Pi must define differently (today: pixellab)
```

Why the split is shaped this way, verified against the current releases rather than
assumed:

| | reads `AGENTS.md` | reads `.agents/skills` | reads `.mcp.json` |
|---|---|---|---|
| Claude Code 2.1.220 | no | no (but follows a symlink) | yes |
| Codex 0.144.1 | yes | yes | no |
| Pi 0.84.1 + adapter | yes | yes | yes |

Claude Code is the only holdout on the tool-agnostic paths, and there is no setting to
change it - the feature requests for both `AGENTS.md` and configurable skill paths are
closed unimplemented. The symlink and the `@AGENTS.md` import are what keep it on one
source anyway.

The direction of that symlink is not arbitrary: **Codex does not follow a symlinked
`.agents/skills`** ([openai/codex#11314](https://github.com/openai/codex/issues/11314)),
so the canonical directory has to be the real one and `.claude/skills` the link. Two more
things worth knowing: **Codex ignores project `.codex/` entirely until you trust the
project**, and Pi likewise loads `.agents/skills` only after you trust the project folder.

**Coming from an older layout?** `ai-toolbox migrate` folds the per-harness copies into
`.agents/`, re-points `settings.json` / `config.toml` / `.mcp.json` at them, replaces
`.claude/skills` with the symlink, and regenerates the Codex tables. It is idempotent,
and `--dry-run` prints the whole plan first. `ai-toolbox setup` offers it automatically
when it spots the old shape.

## The `ai-toolbox` CLI

The deterministic setup engine for the whole toolkit. Instead of hand-copying files
and merging JSON, you run one idempotent command. It's what `/toolbox-init` calls
under the hood, and reads the toolbox's own directories at runtime — so
`ai-toolbox list` never drifts. `bin/ai-toolbox` is the whole CLI; `bin/lib/` holds the
pieces bash shouldn't do — the walkthrough's UI (`ui.sh`) and steps (`setup.sh`), the
TOML writer and MCP converters (`codex_toml.py`, `mcp_codex.py`, `mcp_pi.py`), and the
layout migration (`migrate.py`).

**Setup** — from the repo root, `./install.sh` symlinks `ai-toolbox` into a bin dir
already on your `PATH` (preferring `~/.local/bin`); if none exists it creates
`~/.local/bin` and adds it to your shell rc. The link points at `bin/ai-toolbox`, so
`git pull` updates the command. No installer? Run it by path
(`~/Developer/ai-toolbox/bin/ai-toolbox …`) — it resolves its own location through
symlinks, and `AI_TOOLBOX` overrides it.

| Command | Does |
|---|---|
| `ai-toolbox setup` | The guided walkthrough - pick harnesses, prerequisites, knowledge files, MCPs, hooks, skills, secrets; approve one plan; get per-harness start instructions. |
| `ai-toolbox init [--yes\|--dry-run]` | Scaffold `AGENTS.md`/`CLAUDE.md`, then bootstrap the tailored functional layer. The unattended per-repo entry point. |
| `ai-toolbox bootstrap [--yes\|--dry-run]` | Just the functional layer — detect the stack, show a tailored set, install each group you confirm. |
| `ai-toolbox recommend` | Print the recommended set for this repo (read-only). |
| `ai-toolbox status` | What's installed here — the canonical layer, then each harness's pointers; flags a legacy layout. |
| `ai-toolbox migrate [--dry-run\|--no-symlink]` | Fold an older per-harness layout into `.agents/` and re-point every config at it. Idempotent. |
| `ai-toolbox list` | Everything the toolbox offers (hooks · presets · skills · rules). |
| `ai-toolbox hooks [name...]` | Copy hook scripts **once** into `.agents/hooks/` + wire them into Claude `.claude/settings.json` and/or Codex `.codex/config.toml` `[hooks]` (default: all). |
| `ai-toolbox mcp <preset...>` | Merge MCP preset(s) into `.mcp.json` (the source of truth), add Pi's knobs, generate `.codex/config.toml` `[mcp_servers.*]`; prints required secrets; auto-drops helper scripts a preset needs. |
| `ai-toolbox skill <name...> [--user]` | Install skill(s) into `.agents/skills/` and link `.claude/skills` at it (add `--user` for the `~/` equivalents, `--no-symlink` to copy instead). A group name like `cli` installs each child. |
| `ai-toolbox rules <stack...>` | **Print** rule snippets to inline into `AGENTS.md` (nothing is written). |
| `ai-toolbox with-dotenv` | Drop the `.env` loader into `.agents/mcp/`. |
| `ai-toolbox pi-init` | Install `pi-mcp-adapter` globally — Pi's MCP client, which its core does not ship (once per machine). |
| `ai-toolbox base-charter` | Append the always-on charter to each detected harness's global config — `~/.claude/CLAUDE.md`, `~/.codex/AGENTS.md`, `~/.pi/agent/AGENTS.md` (once per machine). |
| `ai-toolbox help` | The full list. |

**Notes.** `--repo <path>` targets another repo (default: current dir).
`--harness claude|codex|both` picks which harness(es) to write for; by default it
**auto-detects** which homes exist (`~/.claude`, `~/.codex`, or repo-local
`.claude`/`.codex`) and installs for those, falling back to Claude when neither is
present. Every command is idempotent — hooks dedupe, MCP servers overwrite by name,
`base-charter` is marker-guarded. Merges need `python3` on `PATH` (`tomllib`, 3.11+,
for Codex TOML; the Codex serializer lives in `bin/lib/`). It writes only the target
repo's `.claude/` + `.mcp.json` and/or `.codex/` + `.agents/skills/` (and the global
charter file for `base-charter`); it never writes a real secret. What it *can't* do
for you: export MCP secrets, complete OAuth (`/mcp` / `codex mcp login`), restart the
harness — it prints those follow-ups after an install.

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
`ai-toolbox base-charter` appends it to each harness home it detects (the charter is
harness-neutral prose, so the same text serves all of them).

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
| `slim` | debloat a bloated `AGENTS.md` — extract procedural content into its own skill, condense verbose rules (losing no constraint), relocate misplaced content to its right home. The fix for what `lint` diagnoses |
| `handoff` | checkpoint before you clear/compact the context — commit + push + certify green + refresh a committed `HANDOFF.md` so a fresh session resumes exactly where you left off |
| `pre-pr` | run the project's real checks + native review before you push |
| `audit` | self-audit the whole toolbox for drift |
| `babysit-pr` | shepherd a PR to green, triaging each item |
| `cli/*` | CLI-wrapper skills — `fly`, `gh`, `supabase`, `eas`, `aws` |

`SKILL.md` (`name` + `description` frontmatter) is a **cross-agent standard** — the
same folder works in Claude Code, Codex, and Pi unmodified. Install with `ai-toolbox
skill <name…>`: the folder lands **once** in `.agents/skills/` (or `~/.agents/skills/`
with `--user`), which Codex and Pi read natively. Claude Code only ever looks in
`.claude/skills/`, but it *follows that path when it is a symlink* (verified on
2.1.220), so the CLI points `.claude/skills` at `../.agents/skills` rather than keeping
a second copy. `--no-symlink` copies instead, for filesystems where symlinks are
awkward. Pi loads a project's `.agents/skills/` only after you trust the project folder
at its first-run prompt. A group name like `cli` installs each child. Codex can add an optional `agents/openai.yaml` inside a skill
folder for its own UI metadata; the toolbox skills don't need one, and any
Claude-only frontmatter is simply ignored by Codex.

## Hooks

Where the toolkit stops being advisory: your `AGENTS.md` *tells* the model the
rules; a hook *enforces* them — deterministically, every time. **Both harnesses run
these:** the scripts read the tool call from stdin JSON (`tool_name` / `tool_input`),
block with **exit code 2** + a stderr reason, and inject context via
`hookSpecificOutput.additionalContext` — a contract Claude Code and Codex share. The
same script lives once in `.agents/hooks/` and both harnesses are pointed at it - both
take an arbitrary command path, so there is nothing to copy: Claude Code via
`${CLAUDE_PROJECT_DIR}/.agents/hooks/…` in `settings.json`, Codex via a cwd-relative
`.agents/hooks/…` in `.codex/config.toml` `[hooks]`. Pi has no shell-hook mechanism at
all (its extensions are TypeScript), so hooks are a two-harness feature.

| Script | Event | Does |
|---|---|---|
| `format-on-edit.sh` | PostToolUse (Write\|Edit) | formats the edited file with the project's own prettier/eslint/gofmt/ruff. Never blocks. |
| `guard-irreversible.sh` | PreToolUse (Bash) | blocks release-tag pushes (billed builds), force-push, `rm -rf`, secret-file reads. |
| `protect-generated.sh` | PreToolUse (Write\|Edit) | blocks edits to generated files (regenerate from source instead). |
| `conventional-commit.sh` | PreToolUse (Bash) | enforces Conventional Commits on `git commit -m`. |
| `session-context.sh` | SessionStart | injects branch / uncommitted count / last commit so a session starts oriented. |

Install with `ai-toolbox hooks` (copies the scripts + `_lib.sh` and wires only their
entries; needs `jq` or `python3`). Confirm with `/hooks` in Claude Code, or by
reviewing + trusting the hook the first time you run `codex`. **One honest Codex
caveat:** Codex names its shell tool `Bash` (so `guard-irreversible` +
`conventional-commit` fire) and `SessionStart` fires, but edits go through
`apply_patch`, whose hooks are still maturing upstream ([codex#16732]) — so the
`Write|Edit` hooks (`format-on-edit`, `protect-generated`) are wired with the correct
matcher but won't reliably fire on Codex until that lands. They work fully on Claude Code.

[codex#16732]: https://github.com/openai/codex/issues/16732

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
| **mobile** | verifying an Expo / React Native change on an Android emulator or iOS simulator — run, tap, type, screenshot. The mobile "see it work". |
| **unity** | game dev — drive the editor, enter play mode, run tests, inspect the scene. The game "see it work". |

The through-line: prefer the MCP that lets you *verify the real thing*
(chrome-devtools / mobile / unity), *pull source-of-truth context* (figma /
clickup), or *act as the user* (claude-in-chrome). A project's `AGENTS.md` should
name which to use.

**`.mcp.json` is the repo's MCP source of truth.** Claude Code reads it natively, and so
does Pi's adapter - a repo with a `.mcp.json` and no `.pi/` directory at all connects
fine (verified). `ai-toolbox mcp` merges the presets into that one file, adds Pi's two
knobs to it (`lifecycle`, `directTools` - unknown keys that Claude Code ignores), and
only then derives what cannot be shared:

- **`.codex/config.toml`** is *generated output*. Codex cannot read `.mcp.json`
  (verified against 0.144.1), so its `[mcp_servers.<name>]` tables are produced from the
  same source: a `${VAR}` env secret becomes `env_vars = ["VAR"]` (host-env passthrough),
  a `${VAR}` in args becomes a `with-dotenv.sh` wrap that expands it at launch, and a
  remote `headersHelper` becomes `bearer_token_env_var` (with a printed caveat - see
  below). Re-running the command regenerates it; don't hand-edit it.
- **`.pi/mcp.json`** appears only for a server whose Pi definition would be *wrong* for
  Claude Code. Today that is exactly one case: a `headersHelper` server, because Pi needs
  a `headers` value that Claude Code would send as a literal string. Everything else lives
  in the shared file.

Global scopes still exist if you want them (`~/.claude.json`, `~/.codex/config.toml`,
`~/.pi/agent/mcp.json`); the toolbox works per repo.

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
| `mobile` | `mobile-next/mobile-mcp` (`@mobilenext/mobile-mcp`) | none | drive an Android emulator **or** iOS simulator (also real devices) — screenshot, tap, type, swipe, inspect UI; the mobile "see it actually work" | Node ≥ 22; Android needs Platform Tools + SDK, iOS needs Xcode (macOS); telemetry disabled in preset |
| `clickup` | official ClickUp (remote) | OAuth via `/mcp` | pull ticket/task context, update status | install only where ClickUp is the planning source of truth |
| `figma` | official Figma (remote) | OAuth via `/mcp` | pull design context; create/edit Figma content on supported plans | Starter/View seats get 6 read tool calls/month; higher seats use API-style rate limits |
| `figma-framelink` | `GLips/Figma-Context-MCP` (stdio) | `FIGMA_API_KEY` | API-key fallback for design context | distinct server name, so it can coexist with official `figma` |
| `expo` | official Expo (remote) | OAuth via `/mcp` | EAS build triage, TestFlight crash/review, RN DevTools | covers much of an "App Store Connect" need |
| `sentry` | official Sentry (remote) | OAuth via `/mcp` | crash RCA (Seer) in-editor | stdio alt: `npx @sentry/mcp-server` |
| `pixellab` | official PixelLab (remote) | `PIXELLAB_API_TOKEN` (in repo `.env`) | generate pixel-art characters, animations, tilesets (4/8-dir sprites, isometric, terrain) - game-art asset gen | Claude uses `headersHelper`; Pi runs the same helper as a `!command` header; Codex requires an exported token; game/pixel-art only |
| `unity` | `CoplayDev/unity-mcp` | none | drive the Unity Editor — scenes, GameObjects, play-mode, run tests, edit scripts; the game "see it actually work" | needs `uv` + the MCP-for-Unity package; `--directory` defaults to the **macOS** server path (see note) |

> **Unity setup.** The `unity` preset needs the MCP-for-Unity package in your project
> (Unity → Package Manager → *Add from git URL* →
> `https://github.com/CoplayDev/unity-mcp.git?path=/MCPForUnity#main`) plus
> [`uv`](https://docs.astral.sh/uv/). The preset's `--directory` points at the **macOS**
> default server path (`~/Library/Application Support/UnityMCP/UnityMcpServer/src`); on
> Linux/Windows or a custom install, run *Window → MCP for Unity → Configure All Detected
> Clients* — it detects Claude Code and writes the exact path for you.

**Adopt these too:** **Postgres MCP Pro** (`crystaldba/postgres-mcp`, needs
`DATABASE_URI` — avoid the archived `server-postgres`, unpatched SQLi), **GitHub**
(official — scope its toolsets, or just use `gh` + `skills/cli/gh`), and **Resend**
(only for real broadcast management). Package names/flags evolve — verify against each
server's upstream README before trusting a preset.

**Secrets from a root `.env`.** `${VARS}` in a config resolve from the environment
Claude Code was *launched* with — it does **not** auto-load a repo `.env`, and an
unset `${VAR}` with no default makes it fail to parse the config. So a token that
lives only in `.env` needs a bridge, and it differs by transport:

- **stdio servers** — run through **`with-dotenv.sh`** (`ai-toolbox with-dotenv`, or
  `ai-toolbox mcp` drops it automatically for presets that use it): it loads `.env`
  at launch, then execs the server. Set the server's `command` to
  `.agents/mcp/with-dotenv.sh` and put the real command after a `--`; `--need VAR`
  documents + requires a var, `--env-file` picks another file, `--set TGT=SRC` remaps
  a per-env token name. **This is the canonical form for every secret-bearing stdio
  preset**, precisely because it behaves the same in all three harnesses - which is what
  lets one `.mcp.json` entry serve all of them.
- **remote HTTP servers** — there's no local process to wrap, so a static
  `Authorization: Bearer ${VAR}` header can't read `.env`. Use **`headersHelper`** — a
  command Claude Code runs at *connect time* whose JSON stdout becomes the request
  headers. **`dotenv-header.sh`** does this: `"headersHelper": ".agents/mcp/dotenv-header.sh
  PIXELLAB_API_TOKEN"` reads the token from `.env` and emits `{"Authorization":"Bearer …"}`.
  `ai-toolbox mcp` drops the helper automatically (see the `pixellab` preset). Needs a
  recent Claude Code (`headersHelper`, ≥ v2.1.193); it runs arbitrary shell, so it
  executes only after you accept the workspace-trust prompt. Pi has no `headersHelper`
  but runs any header value starting with `!` as a command, so the same script covers it
  with `--raw` (which prints the bare value instead of a JSON object).

**On Codex the `.env` bridge differs.** `with-dotenv.sh` works identically - Codex runs
the same shared wrapper, and `ai-toolbox mcp` even routes a bare `${VAR}`-in-args server
through it (Codex doesn't interpolate config strings, so the wrapper expands them at
launch). But Codex has **no `headersHelper`**:
a remote server's token maps to `bearer_token_env_var`, which reads the token from the
environment you launch `codex` from - so for a remote MCP on Codex, **export the token**
(it won't be read from the repo `.env`). A `${VAR}` stdio env secret maps to
`env_vars = ["VAR"]`, the same host-env passthrough Claude Code's `${VAR}` already relies on.

**On Pi the MCP client is a package.** Pi's core is minimal; MCP comes from the
`pi-mcp-adapter` package. Run `ai-toolbox pi-init` once per machine (it runs
`pi install npm:pi-mcp-adapter` globally and is idempotent) - this is "initialise a
Pi instance with MCP capabilities". After that, `ai-toolbox mcp <preset...>` is all you
need - Pi reads the repo's `.mcp.json` directly. Restart pi, trust the project folder
when it asks, and `/mcp` to check status.

Pi **never interpolates `command`/`args`**, which is why every secret-bearing stdio
preset uses `.agents/mcp/with-dotenv.sh`: it loads the repo `.env` and expands `${VAR}`
in the args at launch, so tokens like `FIGMA_API_KEY` / `SUPABASE_ACCESS_TOKEN` stay in
`.env` (no export needed) and the *same* entry works in Claude Code and Codex. For remote
servers Pi runs a header value that starts with `!` as a *command* at connect time and
uses its stdout - so a `headersHelper` preset like `pixellab` gets a `.pi/mcp.json`
override of `"Authorization": "!.agents/mcp/dotenv-header.sh --raw PIXELLAB_API_TOKEN"`,
keeping the same repo-`.env` workflow with no bridge process and no token in the config.
That is the one server that needs a Pi-only definition.

**OAuth is clean on Pi:** a remote server with no static credential (the `clickup`,
`figma`, `expo`, and `sentry` presets today, or any remote MCP you add later) needs no
`auth` key at all - the adapter treats an absent one as deferred auto-detected OAuth and
runs the full OAuth2 + PKCE flow in the browser on first connect, storing the token in
the OS credential store so it survives restarts. No `codex mcp login` equivalent to run.

**Two Pi-specific knobs the converter sets for you.** *stdio* servers get
`lifecycle: "eager"` + `directTools: true` - they are local, cheap to spawn, and their
tools land in the prompt exactly like Claude Code. *Remote* servers get
`lifecycle: "lazy"` and stay behind the adapter's one-tool proxy, because the SaaS
servers are the tool-heavy ones (pixellab alone registers 72 tools, ClickUp 55) and the
adapter itself warns once a session passes ~75 direct tools. The proxy still searches,
describes, and calls them from cached metadata without connecting. Both keys live on the
server in `.mcp.json`, so flip `directTools: true` (or `"lazy"`/`"eager"`) right there
when you want the other trade-off - Claude Code ignores them.

One merge rule worth knowing: the adapter reads `.mcp.json` **and** `.pi/mcp.json` and
merges them per server, key by key, so a repo set up for both Claude Code and Pi ends up
with the Pi entry layered over the shared one. That is why the converter never changes a
server's transport kind - a `command` layered over a `url` would leave an entry with
both, which the adapter rejects.

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
| GitHub Action (`anthropics/claude-code-action@v1` · `openai/codex-action@v1`) | the battle-tested unattended agent — PR review, `@claude` / `@codex` mentions, scheduled reports, CI autofix | true unattended, runs in CI not your laptop |
| Routines (managed cloud cron) | scheduled agents that run with your laptop closed (min 1h). Research preview — a green run ≠ success. | nightly triage, dependency bumps, docs-drift PRs |

**The one rule: bound the loop.** Every reliable setup engineers against the
runaway-agent failure mode — turn caps (`/goal … stop after N`), `/loop`'s 7-day
expiry, `--max-turns` in CI, and **sandboxing for any
`--dangerously-skip-permissions` run.** Never wire an unbounded auto-fix loop.

What's here: two drop-in capped workflows — `background/claude-github-action.yml`
(`cp` to `<repo>/.github/workflows/claude.yml`, add an `ANTHROPIC_API_KEY` secret) and
`background/codex-github-action.yml` (`openai/codex-action@v1`, bounded by a read-only
sandbox + `drop-sudo`; add an `OPENAI_API_KEY` secret) — install whichever matches your
harness, or both; `background/loop.md.template` (customize what a bare `/loop` does per
repo); and the **`babysit-pr`** skill — a *triaging* PR watcher that assesses each item
as fix/dismiss/escalate rather than blindly auto-applying.

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
