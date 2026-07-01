# templates/ — fill-in skeletons

Blank, well-structured files you copy into a repo and fill. Nothing is generated
— once copied, the file is yours (no regeneration pipeline, no drift).

## How to use

| Template | Copy to | Then |
|---|---|---|
| `AGENTS.template.md` | `<repo>/AGENTS.md` | fill stack / layout / commands / hard-rules — or run the `init` skill, which prefills the mechanical parts and interviews you for the rest. Inline relevant `starters/rules/*`. |
| `CLAUDE.template.md` | `<repo>/CLAUDE.md` | usually just `@AGENTS.md` (the Claude Code adapter, so Claude reads your `AGENTS.md`). |
| `RULES.template.md` | `<repo>/RULES.md` | **OPTIONAL** — only for hard constraints shared across projects or read by humans/CI; then `AGENTS.md` imports it. Otherwise skip it. |
| `feature-brief.md` | `<repo>/docs/<feature>.md` | **OPTIONAL** crisp target for a big or fuzzy task. Not a gate. |
| `design/` | `<repo>/design/` | greenfield design guide — see `design/README.md`. |

The fastest path is the **`init` skill** — it does the `AGENTS.md` + `CLAUDE.md`
step for you, stack-aware. Reach for the raw templates when you'd rather write it
by hand.
