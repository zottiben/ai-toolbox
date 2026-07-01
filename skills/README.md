# skills/ — the toolkit's skills

Portable markdown skills (`<name>/SKILL.md` — YAML frontmatter + a short body).
Each becomes a `/slash-command` and is auto-selected by its `description`.

## What's here

| Skill | Does |
|---|---|
| `init` | scaffold a project's `AGENTS.md` (stack-aware, greenfield + brownfield) |
| `capture` | turn a correction into a permanent `AGENTS.md` rule (the flywheel) |
| `lint` | health-check a knowledge file for drift/bloat |
| `pre-pr` | run the project's real checks + native review before you push |
| `audit` | self-audit the whole toolbox for drift |
| `babysit-pr` | shepherd a PR to green, triaging each item |
| `cli/*` | CLI-wrapper skills — `fly`, `gh`, `supabase`, `eas`, `aws` |

## How to use

- **Everywhere (recommended for the toolkit skills):** copy or symlink into your
  user skills dir — `cp -r skills/<name> ~/.claude/skills/`. Then `/<name>` works
  in every repo. (Symlinking keeps them updated when you `git pull` the toolbox.)
- **Per repo (share with the team):** `cp -r skills/<name> <repo>/.claude/skills/`
  and commit it.
- **CLI skills:** `cp -r skills/cli/* ~/.claude/skills/` (or into a repo's
  `.claude/skills/`).
- **Codex:** it reads the same `SKILL.md` from `~/.codex/skills/` — symlink to use
  the skill in both harnesses (Claude-only frontmatter is ignored by Codex).

Invoke a skill by name (`/pre-pr`) or just describe the task and let the model
pick it by the `description`.
