import assert from "node:assert/strict";
import { mkdtemp, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { it } from "node:test";
import { runProcess } from "../scripts/review.mjs";
import { assessChange, planReview, PROFILES } from "../scripts/selection.mjs";
import { verifiedSmallChange } from "./fixtures.mjs";

const script = fileURLToPath(new URL("../scripts/review.mjs", import.meta.url));
const pair = "gpt-6-sol,claude-opus-5-5";

async function dryRun(args, env = {}, assessment, packet = "Review the supplied change only.") {
  const directory = await mkdtemp(join(tmpdir(), "review-selection-"));
  try {
    const input = join(directory, "packet.md");
    await writeFile(input, packet);
    const options = [...args];
    if (assessment !== undefined) {
      const path = join(directory, "assessment.json");
      await writeFile(path, JSON.stringify(assessment));
      options.push("--assessment", path);
    }
    const result = await runProcess(process.execPath, [script, "--input", input, "--dry-run", ...options], { env: { PATH: "", ...env } });
    return { code: result.code, body: JSON.parse(result.stdout) };
  } finally {
    await rm(directory, { recursive: true, force: true });
  }
}

it("recognises author model IDs without using the reviewer to guess the author", () => {
  for (const authorModel of ["claude-opus-5-5", "anthropic/claude-sonnet-5-5", "claude-subscription/claude-haiku-5-5", "anthropic.claude-sonnet-4-5", "us.anthropic.claude-sonnet-4-5"]) {
    assert.equal(planReview({ authorModel }).reviewer.vendor, "openai");
  }
  for (const authorModel of ["gpt-6-astra", "openai-codex/gpt-6-sol", "o3", "o4-mini"]) {
    assert.equal(planReview({ authorModel }).reviewer.vendor, "anthropic");
  }
  for (const authorModel of [undefined, "", "gemini-3", "opus", "my-router"]) {
    assert.throws(() => planReview({ authorModel, model: pair }), /Unknown author model/);
  }
});

it("uses a light profile only for simple, low-risk changes with all confidence checks", () => {
  for (const [authorModel, model] of [["claude-opus-5-5", "gpt-6-luna"], ["gpt-6-astra", "claude-haiku-5-5"]]) {
    const plan = planReview({ authorModel, assessment: verifiedSmallChange });
    assert.equal(plan.reviewer.model, model);
    assert.equal(plan.selection.profile, "light");
    assert.equal(plan.effort, "medium");
    assert.equal(plan.selection.confidence.score, 4);
    assert.equal(plan.selection.confidence.max, 4);
    assert.deepEqual(plan.selection.confidence.checks, verifiedSmallChange.checks);
    assert.match(plan.selection.confidence.basis, /not a probability/);
    assert.equal(plan.limits.inputBytes, PROFILES.light.inputBytes);
    assert.equal(plan.limits.requestedOutputTokens, PROFILES.light.requestedOutputTokens);
    assert.equal(plan.limits.timeoutMs, PROFILES.light.timeoutMs);
  }
});

it("escalates every missing or unsupported confidence check", () => {
  for (const key of Object.keys(verifiedSmallChange.checks)) {
    for (const passed of [false, null, undefined]) {
      const assessment = structuredClone(verifiedSmallChange);
      if (passed === undefined) delete assessment.checks[key];
      else assessment.checks[key].passed = passed;
      const plan = planReview({ authorModel: "gpt-6-astra", assessment });
      assert.equal(plan.selection.confidence.score, 3);
      assert.equal(plan.reviewer.model, "claude-opus-5-5");
      assert.equal(plan.effort, "high");
      assert.match(plan.selection.reasons.join(" "), /3\/4/);
    }
  }
});

it("escalates risk, complexity and packet size independently of confidence", () => {
  for (const key of ["risk", "complexity"]) {
    for (const value of ["medium", "high", "unknown"]) {
      const assessment = { ...verifiedSmallChange, [key]: value };
      const plan = planReview({ authorModel: "claude-opus-5-5", assessment });
      assert.equal(plan.selection.confidence.score, 4);
      assert.equal(plan.reviewer.model, "gpt-6-sol");
      assert.equal(plan.selection.profile, "strong");
    }
  }
  const large = planReview({ authorModel: "claude-opus-5-5", assessment: verifiedSmallChange, inputBytes: PROFILES.light.inputBytes + 1 });
  assert.equal(large.selection.profile, "strong");
  assert.match(large.selection.reasons.join(" "), /packet exceeds/);
});

it("rejects malformed assessments, unsupported scores and evidence-free claims", () => {
  for (const assessment of [null, [], "high", { confidence: 100 }, { risk: "safe" }, { complexity: "low" }, { rationale: 3 }, { checks: [] }, { checks: { typo: {} } }, { checks: { tests: { passed: "yes", evidence: "passed" } } }, { checks: { tests: { passed: true, evidence: " " } } }, { checks: { tests: { passed: false, evidence: 3 } } }]) {
    assert.throws(() => assessChange(assessment), /Assessment/);
  }
});

it("supports matching provider prefixes and rejects malformed or duplicate-vendor preferences", () => {
  for (const model of ["gpt-6-sol", "openai/gpt-6-sol", "openai-codex/gpt-6-sol", "o3"]) {
    assert.equal(planReview({ authorModel: "claude-opus-5-5", model }).reviewer.model, model.split("/").at(-1));
  }
  for (const model of ["claude-sonnet-5-5", "anthropic/claude-sonnet-5-5", "claude-code/claude-sonnet-5-5", "claude-subscription/claude-sonnet-5-5"]) {
    assert.equal(planReview({ authorModel: "gpt-6-astra", model }).reviewer.model, "claude-sonnet-5-5");
  }
  for (const model of ["", "sonnet", "opus", "gemini-3", "--help", "gpt-*", "openai-codex/claude-sonnet-5-5", "anthropic/gpt-6-sol", "unknown/gpt-6-sol", "extra/openai-codex/gpt-6-sol", `${pair},`, "gpt-6-sol,gpt-6-luna", "claude-opus-5-5,claude-sonnet-5-5", `${pair},gpt-6-astra`]) {
    assert.throws(() => planReview({ authorModel: "gpt-6-astra", model }), /model|vendor/);
  }
});

it("honours explicit models without quietly reducing or overriding the review budget", () => {
  const strong = planReview({ authorModel: "gpt-6-astra", model: pair, assessment: verifiedSmallChange });
  assert.equal(strong.reviewer.model, "claude-opus-5-5");
  assert.equal(strong.effort, "high");
  assert.equal(strong.selection.profile, "strong");
  const light = planReview({ authorModel: "gpt-6-astra", model: "claude-haiku-5-5" });
  assert.equal(light.reviewer.model, "claude-haiku-5-5");
  assert.equal(light.effort, "high");
  assert.match(light.selection.reasons.join(" "), /lightweight model despite a strong/);
});

it("enforces task, project, global and automatic precedence without merging invalid preferences", async () => {
  const globalModel = "gpt-6-luna,claude-haiku-5-5";
  const projectModel = "gpt-6-astra,claude-sonnet-5-5";
  const env = { PI_MODEL: "gpt-6-astra", CROSS_VENDOR_REVIEW_MODEL: globalModel, CROSS_VENDOR_REVIEW_PROJECT_MODEL: projectModel };
  for (const flag of ["--model", "-m"]) {
    const task = await dryRun([flag, pair], { ...env, CROSS_VENDOR_REVIEW_PROJECT_MODEL: "invalid" });
    assert.equal(task.code, 0, task.body.error);
    assert.equal(task.body.reviewer.model, "claude-opus-5-5");
    assert.equal(task.body.selection.source, "task");
  }
  const project = await dryRun([], env);
  assert.equal(project.code, 0, project.body.error);
  assert.equal(project.body.reviewer.model, "claude-sonnet-5-5");
  assert.equal(project.body.selection.source, "project");
  const projectFlag = await dryRun(["--project-model", pair], env);
  assert.equal(projectFlag.code, 0, projectFlag.body.error);
  assert.equal(projectFlag.body.reviewer.model, "claude-opus-5-5");
  assert.equal(projectFlag.body.selection.source, "project");
  for (const model of ["", "invalid", "gpt-6-sol"]) {
    const rejected = await dryRun([], { ...env, CROSS_VENDOR_REVIEW_PROJECT_MODEL: model });
    assert.equal(rejected.code, 1);
  }
});

it("uses assessment files for automatic selection and refuses oversize or malformed input", async () => {
  const light = await dryRun(["--author-model", "gpt-6-astra"], {}, verifiedSmallChange);
  assert.equal(light.code, 0, light.body.error);
  assert.equal(light.body.reviewer.model, "claude-haiku-5-5");
  assert.equal(light.body.selection.source, "automatic");
  assert.equal(light.body.effort, "medium");
  const large = await dryRun(["--author-model", "gpt-6-astra"], {}, verifiedSmallChange, "a".repeat(PROFILES.light.inputBytes + 1));
  assert.equal(large.code, 0, large.body.error);
  assert.equal(large.body.reviewer.model, "claude-opus-5-5");
  const oversized = await dryRun(["--author-model", "gpt-6-astra"], {}, undefined, "é".repeat(PROFILES.strong.inputBytes));
  assert.equal(oversized.code, 1);
  assert.match(oversized.body.error, /Narrow the scope/);
  const malformed = await dryRun(["--author-model", "gpt-6-astra"], {}, { confidence: 100 });
  assert.equal(malformed.code, 1);
});

it("routes a persistent comma-separated pair to the opposing vendor in either order", async () => {
  for (const preference of [pair, " claude-opus-5-5 , gpt-6-sol "]) {
    for (const [author, expected] of [["claude-sonnet-5-5", "gpt-6-sol"], ["gpt-6-astra", "claude-opus-5-5"]]) {
      const result = await dryRun([], { PI_MODEL: author, CROSS_VENDOR_REVIEW_MODEL: preference });
      assert.equal(result.code, 0, result.body.error);
      assert.equal(result.body.reviewer.model, expected);
      assert.equal(result.body.selection.source, "global");
      assert.equal(result.body.effort, "high");
    }
  }
});

it("refuses same-vendor and unknown-author requests before starting a reviewer", async () => {
  for (const author of ["claude-sonnet-5-5", "unknown"]) {
    const result = await dryRun(["-m", "claude-opus-5-5", "--author-model", author]);
    assert.equal(result.code, 1);
    assert.match(result.body.error, /opposing vendor|Unknown author/);
  }
});

it("uses strong defaults and budgets when confidence evidence is missing", async () => {
  for (const [author, expected] of [["claude-sonnet-5-5", "gpt-6-sol"], ["gpt-6-astra", "claude-opus-5-5"]]) {
    const result = await dryRun(["--author-model", author]);
    assert.equal(result.code, 0, result.body.error);
    assert.equal(result.body.reviewer.model, expected);
    assert.equal(result.body.effort, "high");
    assert.equal(result.body.selection.confidence.score, 0);
    assert.equal(result.body.selection.confidence.max, 4);
    assert.ok(result.body.limits.requestedOutputTokens > 2048);
    assert.ok(result.body.limits.timeoutMs > 90000);
  }
});
