# starters/rules/ — per-stack rule snippets

Lean lists of the non-obvious footguns per stack. These are **not** files you
install — you **inline** the relevant ones into a project's `AGENTS.md`.

Available: `typescript` · `go` · `react` · `expo` · `supabase` · `bun` · `vue` ·
`laravel` · `aws` · `unity`.

## How to use

- The **`init` skill** pulls the snippets matching a repo's detected stack into its
  `AGENTS.md` automatically — that's the normal path.
- By hand: open the snippet(s) for your stack and paste the applicable bullets into
  the repo's `AGENTS.md` (under Hard rules / Commands), then **delete what doesn't
  apply and add what's project-specific**.

Seeds, not gospel. Keep each short — if a snippet grows past a screen it's carrying
process, not knowledge.
