---
name: toolbox-init
description: Scaffold a project's AGENTS.md (+ a thin CLAUDE.md adapter, and an optional RULES.md). Detects the stack — web, backend, mobile, OR game engine (Unity/Godot/Unreal) — auto-fills the mechanical facts from real manifests, composes matching starter rule-snippets, and interviews you for the non-obvious rules only you know. Use when setting up AI knowledge files for a new or existing project.
---

# toolbox init — scaffold project knowledge

Produce hand-owned knowledge files for a project. Automate the boilerplate;
interview for the wisdom. **One-shot** — after this runs the files are yours,
with no regeneration dependency. Templates live in `templates/`, snippets in
`starters/rules/`.

## 0. Greenfield or brownfield?

- Real source present (manifests, code) → **brownfield**: harvest it (steps 1–6).
- Empty / planning-stage → **greenfield**: the design guide comes first —

  1. Confirm the *intended* stack + project type (game / site / SaaS) — ask.
  2. Scaffold `design/` from `templates/design/`: always `concept.md` +
     `design-system.md`; add `system-gdd.md` (games) or `product-brief.md`
     (sites/SaaS). Leave them as skeletons for the user (or Claude Design) to
     fill — never invent the creative content.
  3. Write a thin seed `AGENTS.md`: the intended stack + a **Design** section
     pointing at `design/` as the source of truth, with the rule *"greenfield —
     the design guide is the source of truth; build against it, ask when a
     decision isn't covered and record it there. As code lands, graduate
     conventions into Hard rules and design tokens into a conformance check."*
  4. Add the `CLAUDE.md` adapter; skip harvesting (nothing to harvest yet).

  Then hand off so the guide gets filled before real coding begins.

## 1. Detect the stack (read manifests — never guess)

| Signal | Stack |
|---|---|
| `package.json` (+ `bun.lock`/`pnpm-lock`) | Node/Bun/TS; its `scripts` are your commands |
| `go.mod` · `composer.json` · `pyproject.toml`/`requirements.txt` · `Cargo.toml` · `Gemfile` | Go · PHP/Laravel · Python · Rust · Ruby |
| `app.json`/`app.config.*` · `Podfile`/`android/` | Expo/React Native · native mobile |
| `turbo.json` · `nx.json` · workspaces | monorepo — map each app/package |
| `ProjectSettings/ProjectVersion.txt` (+ `Assets/`, `*.asmdef`) | **Unity** |
| `project.godot` · `*.uproject` | **Godot** · **Unreal** |
| `.github/workflows/*.yml` | real CI commands + gates (incl. GameCI) |

## 2. Auto-fill the mechanical facts (don't ask what you can read)

- Name + one-line purpose (from README), stack + versions, monorepo layout.
- **Commands copied verbatim** from `package.json` scripts / `Makefile` / turbo
  tasks / CI workflow: install · lint · test · build · codegen · deploy. Never
  invent a command — if you can't find it, mark it `TODO`.
- Test framework, and generated-file locations to never hand-edit (tygo, protobuf,
  OpenAPI, Unity `.meta`, etc.).

## 3. Compose matching starter snippets

Pull the relevant files from `starters/rules/` for each detected stack (e.g.
`go` + `expo` + `supabase`, or `unity`). Inline the rules that apply; drop the rest.

## 4. Interview for the non-obvious rules (the part only the user knows)

Ask a SHORT, stack-tuned set — a few at a time — and fold answers into "Hard
rules" as *rule / why / how it's enforced*:

- **Any project:** safety-critical constraints? cost or irreversible actions
  (paid builds, prod deploys, destructive migrations)? authz/multi-tenant rules?
  files never to hand-edit? release/versioning gotchas?
- **Web/SaaS:** row-level security / tenant isolation? migration conventions?
  design-system or lint gates that fail CI?
- **Mobile:** build-cost gating (EAS)? simulator smoke-test expectations? store
  review constraints?
- **Game (Unity/Godot/Unreal):** asmdef / assembly boundaries (what may reference
  what; where editor vs runtime code must live)? asset & `.meta` conventions
  (metas travel with assets; never hand-merge scene/prefab YAML)? EditMode vs
  PlayMode test split? target platforms & build steps (WebGL/Steam/console)?
  frame-budget / GC / object-pooling rules? engine MCP usage (e.g. drive the
  Unity editor via unity-mcp to run tests and inspect the scene)?

## 5. Write the files

- `AGENTS.md` — the filled template (stack · layout · commands · hard rules).
- `CLAUDE.md` — `@AGENTS.md` (+ optional Claude-only notes).
- `RULES.md` — **only** if the user has constraints shared across projects or
  read by humans/CI; otherwise skip it (no empty stubs).

## 6. Health-check

Run `toolbox-lint` on the result and trim what it flags. Keep `AGENTS.md` lean —
durable facts the model can't read from the code, nothing more.
