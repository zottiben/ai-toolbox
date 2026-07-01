# hooks/ — the functional guardrail layer

Hooks are where the toolkit stops being advisory. Your `AGENTS.md` *tells* the
model the rules; a hook *enforces* them — deterministically, every time, even
when the model would otherwise ignore them. **This is the enforcement layer for
the knowledge layer** — the harvested rules become hook specs.

These are **Claude Code** hooks (configured in `settings.json`). The shell logic
is reusable; other harnesses wire it differently (Codex has its own model).

## What's here

| Script | Event | Does |
|---|---|---|
| `format-on-edit.sh` | PostToolUse (Write\|Edit) | formats the edited file with the project's own prettier/eslint/gofmt/ruff. Never blocks. |
| `guard-irreversible.sh` | PreToolUse (Bash) | blocks release-tag pushes (billed builds), force-push, `rm -rf`, secret-file reads. |
| `protect-generated.sh` | PreToolUse (Write\|Edit) | blocks edits to generated files (regenerate from source instead). |
| `conventional-commit.sh` | PreToolUse (Bash) | enforces Conventional Commits on `git commit -m`. |
| `session-context.sh` | SessionStart | injects branch / uncommitted count / last commit so a session starts oriented. |

Blocking hooks use **exit code 2** (stderr is fed back to the model as the
reason). Context hooks emit `hookSpecificOutput.additionalContext` on stdout.

## Install (per project)

1. Copy the scripts in: `cp hooks/*.sh <project>/.claude/hooks/ && chmod +x <project>/.claude/hooks/*.sh`
2. Merge `settings.hooks.json` into `<project>/.claude/settings.json` (commit it to
   share with the team) — or `~/.claude/settings.json` for all projects.
3. Requires `jq` on `PATH`. Run `/hooks` in Claude Code to see them registered.

## Tune them

The patterns are **starters** — edit per project:
- `guard-irreversible.sh`'s release-tag block maps to the "never push `v*` — it
  triggers a billed EAS build" rule; it's opt-in, drop it where it doesn't apply.
- `protect-generated.sh` reads `GENERATED_GLOBS` (pipe-separated regex) to override
  the default generated-file patterns.

## Safety

Hooks run arbitrary shell with your permissions on every matching action. Read a
script before enabling it, and keep them fast — they're in the critical path.
