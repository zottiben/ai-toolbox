---
name: cross-vendor-review
description: Get a bounded, read-only second opinion during code review, branch or PR review, and pre-PR checks. Use for normal review work as well as explicit second-opinion requests. Supports a user-selected reviewer model; otherwise uses a low-cost model from the other vendor.
compatibility: Node 22.19+; OpenAI reviews need npm-installed Pi 1.4+ with Codex login; Anthropic reviews need Claude Code with safe-mode and the selected model; macOS or Linux.
---

# Cross-vendor review

Normal automatic skill selection is allowed, subject to the user's harness policy.
Run one pass per review scope. Do not start review loops or let the reviewer delegate.

## 1. Select the reviewer

The user can say "Review this with claude-sonnet-5-5" or pass `--model`/`-m` in
skill arguments. Forward that choice to the helper's `--model` option. Use an
exact model ID, not a moving alias such as `sonnet` or `opus`.

Selection order: `--model`/`-m`, then `CROSS_VENDOR_REVIEW_MODEL`, then the defaults:

| Author model | Default reviewer |
|---|---|
| Anthropic Claude | OpenAI `openai-codex/gpt-6-luna` |
| OpenAI GPT or o-series | Anthropic `claude-haiku-5-5` through Claude Code |

An override can select either supported vendor, including the author's vendor.
Without an override, read the actual author model from `PI_MODEL`, or pass it with
`--author-model` outside Pi. Do not infer the vendor from the editor. Invalid IDs,
missing credentials, unavailable models and quota errors stop without fallback.
Both routes use subscription authentication, not an automatic switch to API billing.

## 2. Prepare a small, explicit packet

Establish the base/head or uncommitted scope. Include requirements, relevant rules,
the complete scoped diff (including relevant untracked files), line-numbered source
excerpts and verified test results. Write a private temporary file outside the repo.
Exclude secrets, customer data, conversation history and unrelated skills.
The UTF-8 limit is 64 KiB. If it does not fit, narrow the scope with the user;
do not truncate or split into extra model calls. The reviewer cannot read files
or run tests, so report partial coverage as partial coverage.

## 3. Run the bundled helper

Resolve `scripts/review.mjs` relative to this skill directory. Dry-run the resolved
choice first; this example assumes the user selected Sonnet:

```sh
node /absolute/skill/path/scripts/review.mjs --input /private/packet.md -m claude-sonnet-5-5 --dry-run
node /absolute/skill/path/scripts/review.mjs --input /private/packet.md -m claude-sonnet-5-5
```

Omit `-m` to use the environment preference or automatic opposing-vendor defaults.
For a persistent preference, set `CROSS_VENDOR_REVIEW_MODEL` in the harness environment.
OpenAI uses Pi's model API, not an agent session or extensions. Claude has no tools,
skills, MCP servers, hooks or session persistence. Both have fresh context, low
effort and a 90-second deadline. The output target is 2,048 tokens; Codex does not
enforce that setting, so its deadline is the hard bound. Claude also has a one-turn
limit and a $0.25 API budget. Reported cost is an estimate, not subscription billing.

## 4. Report, then stop

Require exit code zero and `status: complete`. Errors, timeouts and truncation are
incomplete, never "no findings". Report the model, scope, findings, limits and usage;
missing usage is unknown, not zero. Verify findings before proposing fixes.
Do not edit, post comments, commit, push or approve on this skill's behalf.
Remove the temporary packet.
