# starters/agents/

Generic, reusable subagent starters — and, more importantly, guidance on **when
NOT to make one.**

## First rule: don't rebuild native capability

Your harness already ships strong agents/skills — `/code-review`,
`/security-review`, `/verify`, `/run`, plus native subagents for search and
planning. A custom agent is only worth it when it's a **genuinely distinct lens**
or a **project-specific specialist** native tooling doesn't cover. If a native
tool does the job, use the native tool.

## When a custom agent earns its place

- It needs an **isolated context** — a big, noisy investigation you don't want
  polluting the main thread.
- It's a **distinct review lens** native review doesn't apply (e.g. "does this diff
  obey THIS project's `AGENTS.md` rules?").
- It's a **project-specific specialist** — a domain the model must be pointed at.

## What's here

- `AGENT.template.md` — a skeleton for authoring a subagent.
- `convention-reviewer.md` — one worked example: reviews a diff against the
  project's own `AGENTS.md` hard rules (distinct from native bug-focused review).

Keep this folder small. A roster of agents is how frameworks rot.
