# starters/

Reusable, **opt-in** best-practice content. Nothing here is mandatory or
always-on — it's a library you pull from when scaffolding a project, and then
edit to taste. You stay the author.

| Path | What it is | How it's used |
|---|---|---|
| `base-charter.md` | Universal engineering norms (the one always-on artifact) | imported into each harness's *global* config, once per machine |
| `rules/` | Rule snippets by stack (`go.md`, `react.md`, `sql.md`, `aws.md`, …) | composed into a project's `AGENTS.md` at scaffold time |
| `agents/` | A few high-quality, generic agent definitions + an authoring template | dropped into a project's agent dir and customized |

## Rules of the road for this folder

- **Snippets are seeds, not gospel.** They capture common footguns for a stack;
  you delete what doesn't apply and add what's specific to your project.
- **Never rebuild native capability.** Agent starters must not duplicate what the
  harness already ships (`/code-review`, `/security-review`, `/verify`, `/run`,
  `/simplify`). They're distinct lenses or project-specific specialists only.
- **Keep each snippet short.** If a rule file grows past a screen, it's carrying
  process, not knowledge — cut it.
