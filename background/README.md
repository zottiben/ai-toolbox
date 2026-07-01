# background/ — recurring & unattended automation

The "set it and forget it" layer. Most of this is **built into Claude Code** —
the job here is to point you at it and encode the one rule that keeps it safe.

## The built-ins (use these; don't rebuild them)

| Feature | What | Good for |
|---|---|---|
| `/loop [interval] [prompt]` | re-runs a prompt on an interval in-session (backed by cron tools, **7-day auto-expiry**). Can call skills: `/loop 20m /babysit-pr 1234`. Bare `/loop` runs a maintenance prompt (customize via `.claude/loop.md`). | poll CI, tend a PR, periodic checks — while the terminal is open |
| `/goal <condition>` | keeps working turn-after-turn until a condition holds. **Always cap it:** "… stop after N turns". | "all tests pass and lint clean" |
| Background Bash (Ctrl-B / `run_in_background`) | move dev servers / builds / watchers off the main thread; completion lands as a notification next turn | long-running processes, no polling |
| GitHub Action (`anthropics/claude-code-action@v1`) | the battle-tested unattended agent — PR review, `@claude` mentions, scheduled reports, CI autofix | true unattended, runs in CI not your laptop |
| Routines (managed cloud cron) | scheduled agents that run with your laptop closed (min 1h). Research preview — a green run ≠ success. | nightly triage, dependency bumps, docs-drift PRs |

## The one rule: bound the loop

Every reliable setup engineers against the runaway-agent failure mode: turn caps
(`/goal … stop after N`), `/loop`'s 7-day expiry, `--max-turns` in CI, and
**sandboxing for any `--dangerously-skip-permissions` run.** Never wire an
unbounded auto-fix loop.

## What's here

- `claude-github-action.yml` — a drop-in workflow (PR review + `@claude`), capped.
- `loop.md.template` — customize what bare `/loop` does per repo.
- The **`babysit-pr`** skill (`skills/babysit-pr/`) — a *triaging* PR watcher (the
  research's key lesson: assess each item as fix/dismiss/escalate, don't blindly auto-apply).

## How to use

- **`/loop` and `/goal`** are built into Claude Code — just type them; nothing to
  install. E.g. `/loop 10m /babysit-pr 1234`, or `/goal all tests pass, stop after 8 turns`.
- **GitHub Action:** `cp background/claude-github-action.yml <repo>/.github/workflows/claude.yml`,
  add an `ANTHROPIC_API_KEY` repo secret, and verify the action's inputs against its README.
- **`.claude/loop.md`:** `cp background/loop.md.template <repo>/.claude/loop.md` to
  customize what a bare `/loop` does in that repo.
- **`babysit-pr`:** install it like any skill — see `skills/README.md`.
