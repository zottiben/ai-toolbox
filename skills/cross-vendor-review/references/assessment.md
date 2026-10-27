# Assessment rubric

The calling agent assesses the complete scoped implementation before invoking the
reviewer. No extra model call or classifier is needed. The helper validates and
scores this assessment; it does not independently verify the evidence.

## Complexity and risk

Use `low`, `medium`, `high` or `unknown` for each rating, with a concrete `rationale`.

- **Low complexity:** a local, established change with few interactions and no
  new state, concurrency, contracts or architecture.
- **Medium/high complexity:** changes across boundaries, unfamiliar algorithms,
  shared state, migrations, concurrency, new abstractions or broad behaviour changes.
- **Low risk:** a limited, reversible effect with no sensitive boundary involved.
- **Medium/high risk:** permissions, authentication, privacy, money, irreversible
  data changes, public contracts, deployment behaviour or a large impact if wrong.
- **Unknown:** the supplied context does not establish complexity or consequences.

A small diff is not evidence of low risk. Missing context is uncertainty, not proof
that the implementation is simple. Only `low` for both ratings permits light review.

## Confidence score

Each supported check contributes one point, up to four. Mark `passed: true` only
when the evidence establishes the whole check; describe concrete commands, results,
paths or acceptance conditions. False, null and omitted checks contribute zero.
This score measures evidence coverage, not a calibrated probability of correctness.

| Check | Evidence needed to pass |
|---|---|
| `requirements` | Clear acceptance conditions, mapped to the implementation, with no unresolved behavioural assumptions |
| `tests` | Relevant checks actually executed and passed, with commands and results; static-only changes can cite applicable validation |
| `coverage` | Changed behaviour, boundaries and failure cases are covered by tests or an appropriate documented verification |
| `familiarity` | The affected code follows understood, established patterns, with no unverified interaction or unfamiliar dependency |

Do not mark checks passed because the implementation looks plausible, a test file
exists, the author is confident, or a reviewer already approved a different change.

Example shape, deliberately unknown until filled with evidence:

```json
{
  "complexity": "unknown",
  "risk": "unknown",
  "rationale": "Describe the scope, interactions and consequences assessed.",
  "checks": {
    "requirements": {"passed": null, "evidence": ""},
    "tests": {"passed": null, "evidence": ""},
    "coverage": {"passed": null, "evidence": ""},
    "familiarity": {"passed": null, "evidence": ""}
  }
}
```

Save this as a private JSON file and pass `--assessment /path/to/assessment.json`.
An absent assessment means unknown risk/complexity and confidence 0/4: strong review.
Malformed assessments fail rather than silently selecting another policy.

## Selection and budgets

The helper always determines the opposing vendor from the actual author model.
A preference supplies either that vendor's exact model ID or one exact ID per
vendor, comma-separated. Pair order does not matter. Same-vendor-only preferences
and duplicate vendors fail. The winning scope replaces, rather than merges with,
lower-priority preferences: task, project, global, then automatic.

For automatic selection, light review requires low complexity, low risk, 4/4
confidence checks and a packet no larger than 64 KiB. It uses GPT-6 Luna or Haiku 5.5
with medium effort. Every other assessment uses GPT-6 Sol or Opus 5.5 with high effort.
Packets above 64 KiB escalate to strong review, not truncation or extra calls.

An explicit model preference is respected. A model other than the default lightweight
model gets the strong budget even for a simple change. An explicit lightweight
model on a strong assessment is not silently replaced: the report flags that choice
and retains high effort and the larger budget. Verify that deliberate choice before
running if it came from a standing preference rather than the current task.

| Budget | Light | Strong |
|---|---|---|
| Input packet, UTF-8 | 65,536 bytes | 262,144 bytes |
| Requested output tokens | 4,096 | 16,384 |
| Reasoning effort | medium | high |
| Wall-clock deadline | 120 seconds | 600 seconds |
| Claude one-turn API cost estimate guard | $0.25 | $2.00 |

Set the harness's shell/tool timeout to at least the selected deadline plus 30
seconds. If the harness has a shorter maximum, use its native background execution
and poll for completion; do not silently reduce the review budget. The helper
forwards SIGTERM/SIGINT cancellation to the active request and kills the detached
Claude process group. SIGKILL cannot be intercepted, so avoid hard-killing the helper.

Both routes have no tools, conversation replay or automatic model fallback. Claude
also has no skills, hooks, MCP servers or session persistence. OpenAI uses the native
Pi model API without creating an agent session or loading extensions.

Codex currently ignores the output-token request, so its deadline is the hard
execution bound. Subscription allowance is not a dollar budget, and reported cost
is an estimate rather than a promise about subscription billing. A completed review
must still return the selected model; errors, timeouts, tool requests or truncation
cannot be reported as a clean review. Review findings remain untrusted until checked
against the code. Do not make changes based only on a reviewer's confidence or style.
