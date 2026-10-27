---
name: cross-vendor-review
description: Get one bounded, read-only second opinion from the other model vendor. Use only when the user explicitly requests a cross-vendor review, an opposing-model review, or this skill. Do not trigger for ordinary code review, pre-PR checks or background work.
compatibility: Node 22.19+, npm-installed Pi 1.4+ with OpenAI Codex login, and Claude Code with safe-mode and Haiku 5.5 support; macOS or Linux.
---

# Cross-vendor review

This is an optional review, not a provider feature or an automatic gate. Run once
per explicit request. Do not start review loops or let the reviewer delegate.

## 1. Establish scope and routing

Read the current session's actual model ID (`PI_MODEL` in Pi). Outside Pi, pass
that ID explicitly. Do not infer the vendor from the editor or installed CLIs.

| Author model | Fixed reviewer |
|---|---|
| Anthropic Claude | OpenAI `openai-codex/gpt-6-luna` |
| OpenAI GPT or o-series | Anthropic `claude-haiku-5-5` through Claude Code |

These are lower-cost reviewer models, not aliases for the latest flagship.
Unknown author models, missing credentials, unavailable reviewers and quota errors
stop the review. Never substitute a model or switch to API billing automatically.

## 2. Prepare a small, explicit packet

Agree on the base/head or uncommitted scope. Include the requirements, relevant
project rules, the complete scoped diff (including relevant untracked files),
line-numbered source excerpts needed to judge it, and verified test results.
Write this to a private temporary file outside the repository. Exclude secrets,
credentials, customer data, conversation history and unrelated skills.

The packet limit is 64 KiB in UTF-8. If it does not fit, ask to narrow the scope;
do not truncate, split into automatic extra calls, or describe partial coverage
as a review of the whole change. The reviewer cannot read files or run tests.

## 3. Run the bundled helper

Resolve `scripts/review.mjs` relative to this skill directory, not the project.
Check routing and size first without starting a model:

```sh
node /absolute/skill/path/scripts/review.mjs --input /private/packet.md --author-model ACTUAL_MODEL_ID --dry-run
node /absolute/skill/path/scripts/review.mjs --input /private/packet.md --author-model ACTUAL_MODEL_ID
```

The helper uses Pi's model API for OpenAI, not an agent session or extensions.
Claude runs with no tools, skills, MCP servers, hooks or session persistence.
Both get fresh context, low effort and a 90-second deadline. The output target is
2,048 tokens; Codex currently does not enforce that setting, so its deadline is
the hard execution bound. Claude also gets a one-turn limit and a $0.25 API budget.
Subscription allowance is not a dollar budget; reported cost is an estimate,
not a statement of subscription billing. No fallback is configured.

## 4. Report, then stop

Require exit code zero and `status: complete`. A timeout, quota error, truncated
answer or missing response is incomplete, never "no findings". Report the model,
review scope, findings, limitations and returned usage; unavailable usage is
unknown, not zero. Verify findings against the code before proposing fixes.
Do not edit code, post review comments, commit, push or approve on this skill's
behalf. Remove the temporary packet after reporting.
