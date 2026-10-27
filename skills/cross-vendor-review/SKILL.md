---
name: cross-vendor-review
description: Get a bounded, read-only opposing-vendor second opinion during code review, branch or PR review, and pre-PR checks. Use for normal review work and explicit second-opinion requests. Honour model-pair preferences; otherwise select review strength from implementation complexity, risk and evidence-backed confidence.
compatibility: Node 22.19+; OpenAI reviews need npm-installed Pi 1.4+ with Codex login; Anthropic reviews need Claude Code with safe-mode and the selected model; macOS or Linux.
---

# Cross-vendor review

Normal automatic selection is allowed, subject to harness policy. Run one pass per
review scope, not a review loop. The reviewer cannot use tools or delegate.

## 1. Establish scope and preferences

Identify the actual author model (`PI_MODEL` in Pi, `--author-model` elsewhere).
Always select the opposing vendor, even when a model preference is supplied.
Accept an exact model ID or a comma-separated pair, in either order:
`gpt-6-sol,claude-opus-5-5`. A single model must belong to the opposing vendor;
duplicate vendors, unknown authors and unavailable models stop without fallback.

Preference order: task `--model`/`-m`, project `--project-model` or
`CROSS_VENDOR_REVIEW_PROJECT_MODEL`, global `CROSS_VENDOR_REVIEW_MODEL`, automatic.
Read project instructions for a preferred pair and pass it as `--project-model`.
Use native harness environment settings for persistent preferences; do not add a
configuration loader or silently merge lower-priority preferences into a bad pair.

## 2. Assess the whole implementation

Read [the assessment rubric](references/assessment.md). Inspect requirements,
behaviour changes, affected boundaries, tests, coverage and unresolved assumptions.
Write a private assessment JSON file with concrete evidence, not a guessed percentage.
The helper counts four supported checks: requirements, tests, coverage and familiarity.
This is a 0-4 evidence score, not a probability that the code is correct.

Automatic light review requires low complexity, low risk and all four checks.
Otherwise use strong review, including when evidence is missing or uncertain.
A tiny permission or financial change can be high risk; diff size alone is not enough.
Explicit preferences choose the model, but do not force its reasoning budget down.

| Profile | Automatic pair | Effort | Input / output target / deadline |
|---|---|---|---|
| Light | GPT-6 Luna / Haiku 5.5 | medium | 64 KiB / 4,096 tokens / 2 minutes |
| Strong | GPT-6 Sol / Opus 5.5 | high | 256 KiB / 16,384 tokens / 10 minutes |

## 3. Prepare the packet and run

Include requirements, relevant rules, the complete scoped diff including untracked
files, line-numbered source excerpts and verified test results in a private temporary
packet outside the repo. Exclude secrets, customer data, history and unrelated skills.
Do not truncate or split into automatic extra calls; agree on a smaller scope if needed.
The assessment controls routing only, not the reviewer's judgment of the code.

Resolve the helper relative to this skill. Dry-run first and inspect its rationale.
Set the harness tool timeout above `limits.timeoutMs` plus 30 seconds, or use the
harness's background/polling support. Do not let a shorter outer timeout kill it:

```sh
node /absolute/skill/path/scripts/review.mjs --input /private/packet.md --assessment /private/assessment.json --dry-run
node /absolute/skill/path/scripts/review.mjs --input /private/packet.md --assessment /private/assessment.json
```

Append `-m gpt-6-sol,claude-opus-5-5` for a task preference. Both routes use fresh
context and subscription auth, not an API billing fallback. Codex does not enforce
the output-token setting; its deadline is the hard execution bound. See the rubric
for budget details. Reported costs are estimates, not subscription billing.

## 4. Verify findings and report

Require exit zero and `status: complete`; failures and truncation are incomplete,
never "no findings". Report scope, chosen model, preference source, profile, confidence
evidence, selection rationale, limits and usage. Missing usage is unknown, not zero.
Verify findings against the code before proposing changes; uncertainty is not a defect.
Do not edit, post comments, commit, push or approve on this skill's behalf.
Remove the temporary packet and assessment after reporting.
