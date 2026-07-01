# ai-toolbox

A portable, harness-agnostic toolkit that **complements** AI coding harnesses
(Claude Code, Codex, OpenCode) instead of wrapping them in process.

It's a **personal leverage tool.** Its only job is to make *you* faster and keep
*you* in control — encode durable knowledge once, scaffold it into any project in
minutes, and never let the tooling get in the model's way. It is not a framework
and it does not think it knows better than you.

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

## What's in it

The product is an **authoring system**, not a pile of bespoke files. You point it
at any project and it produces the knowledge files you then fill in.

```
ai-toolbox/
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
    init/                # scaffold a project's AGENTS.md (stack-aware, incl. game engines)
    capture/             # turn a correction into a permanent AGENTS.md rule
    lint/                # health-check knowledge files for drift/bloat
    pre-pr/              # run this project's real checks + native review before you push
    audit/               # self-audit the toolbox for drift (keeps it lean)
    cli/                 # CLI-wrapper skills (fly, gh, supabase, eas, aws) — CLI+skill instead of heavy MCPs
    babysit-pr/          # shepherd a PR to green with triage (bounded)
  mcp/                 # capability layer — registry, when-to-use, template + presets/ (ready configs to drop into a repo)
  hooks/               # functional guardrails — shell hooks that ENFORCE the AGENTS.md rules
  background/          # recurring/unattended automation — /loop & /goal guide, PR-babysitter, GitHub Action template
  examples/
    relik-AGENTS.md      # a real SaaS pack — proof the format works
    keepy-uppy-AGENTS.md # a real Unity/game pack (game dev is first-class)
```

## The three knowledge files (non-overlapping roles)

- **`AGENTS.md`** — the single source of truth. Stack, layout, commands, hard
  rules. Cross-harness (Codex + OpenCode native; Claude Code via the adapter).
- **`CLAUDE.md`** — thin. Default is literally `@AGENTS.md`, plus optional
  Claude-only notes. Kept near-empty so it can't drift.
- **`RULES.md`** — **opt-in.** Only worth it for hard constraints shared across
  projects or read by humans/CI too; then `AGENTS.md` imports it. Otherwise skip
  it — no empty stubs.

The **base charter** (`starters/base-charter.md`) is the one always-on artifact.
It goes in each harness's *global* config (`~/.claude/CLAUDE.md` import,
`~/.codex/AGENTS.md`, OpenCode global), on every machine you clone this onto.

## Using it in a project

`ai-toolbox` is a library you pull *from*, not a dependency you install. Clone it
once, then copy the pieces a repo needs — nothing here is all-or-nothing.

**Once per machine**
- Clone this repo (e.g. `~/Developer/ai-toolbox`).
- Append `starters/base-charter.md` into your global config so it's always on —
  Claude Code `~/.claude/CLAUDE.md`, Codex `~/.codex/AGENTS.md`.

**Per repo — take only what you want:**

| Want… | Do this | Details in |
|---|---|---|
| Project knowledge | run the `init` skill, or `cp templates/AGENTS.template.md <repo>/AGENTS.md` and fill it; add a `CLAUDE.md` of just `@AGENTS.md` | `templates/`, `starters/rules/` |
| Enforced guardrails | `cp hooks/*.sh <repo>/.claude/hooks/ && chmod +x …` + merge `hooks/settings.hooks.json` into `.claude/settings.json` | `hooks/` |
| An MCP (Supabase, Chrome…) | merge `mcp/presets/<name>.json` into `<repo>/.mcp.json`, fill env vars, `/mcp` to auth | `mcp/presets/` |
| Helper skills | `cp -r skills/cli/* <repo>/.claude/skills/` (or `skills/<name>` into `~/.claude/skills/` for everywhere) | `skills/` |
| A greenfield design guide | `cp -r templates/design <repo>/design/` and fill the skeletons | `templates/design/` |

Each directory's README has the step-by-step. Start small: an `AGENTS.md` + one
hook already puts you ahead.

## Status

- [x] Format + convention proven (a ~90-line `AGENTS.md` reliably steers a fresh
      model on hard, safety-critical rules — validated adversarially, 3/3)
- [x] Agnostic structure: templates + starters + skills + examples
- [x] The three templates (`AGENTS` / `CLAUDE` / `RULES`)
- [x] Group 1 skills: `init` (generator), `capture` (gotcha flywheel), `lint` (health-check)
- [x] Group 3 (starter library) — stack snippets (typescript/unity/go/react/expo/supabase/bun/vue/laravel/aws) + agent starters (template + convention-reviewer example)
- [x] Examples — `relik` (SaaS) + `keepy-uppy` (Unity) — format proven on a SaaS *and* a game
- [x] Design layer — 4 greenfield templates (concept · design-system · system-GDD · product-brief) + mode-aware generator
- [x] Group 2 — `pre-pr` gate + reproduce-before-you-fix norm (in base charter)
- [x] Group 4 (ergonomics & governance) — portable MCP config + self-audit skill + feature-brief template
- [~] Functional layer — hooks (tested) · MCP presets (supabase/context7/chrome/playwright/figma/expo/sentry) · CLI skills (fly/gh/supabase/eas/aws) · background layer (`/loop`+`/goal` guide, babysit-pr, GH Action) · capture+init upgrades
- [ ] Greenfield example (chief-of-geese); validate skills live; retire Software Teams / JDI from repos
