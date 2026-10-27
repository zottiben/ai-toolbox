import assert from "node:assert/strict";
import { access, mkdtemp, readFile, rm, symlink, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, it } from "node:test";
import { SYSTEM, validatePacket, reviewOpenAI, reviewAnthropic, runProcess } from "../scripts/review.mjs";
import { planReview, PROFILES } from "../scripts/selection.mjs";
import { verifiedSmallChange } from "./fixtures.mjs";

const packet = "Requirement: zero is valid.\n--- a/check.js\n+++ b/check.js\n@@ -1 +1 @@\n-return n >= 0;\n+return n > 0;";
const openai = planReview({ authorModel: "claude-opus-5-5" });
const anthropic = planReview({ authorModel: "gpt-6-astra" });
const usage = { input: 100, output: 20, cacheRead: 0, cacheWrite: 0, totalTokens: 120 };
const openaiResult = { provider: openai.reviewer.provider, model: openai.reviewer.model, stopReason: "stop", content: [{ type: "text", text: "Zero is incorrectly rejected at check.js:1." }], usage };
const claudeAuth = { code: 0, stdout: JSON.stringify({ loggedIn: true, authMethod: "claude.ai", apiProvider: "firstParty" }), stderr: "" };
const claudeResult = { type: "result", subtype: "success", is_error: false, num_turns: 1, result: "Zero is incorrectly rejected at check.js:1.", modelUsage: { [anthropic.reviewer.model]: {} }, usage: { input_tokens: 100, output_tokens: 20 }, total_cost_usd: 0.001 };
const claudeFixture = (response) => async (_command, args) => args.includes("auth") ? claudeAuth : { code: 0, stdout: JSON.stringify(response), stderr: "" };

function runtimeFixture(result, inspect = () => {}, plan = openai) {
  return async () => ({ ModelRuntime: { create: async (options) => {
    assert.equal(options.modelsPath, null);
    assert.equal(options.allowModelNetwork, false);
    return {
      getModel: (provider, model) => {
        assert.equal(provider, plan.reviewer.provider);
        assert.equal(model, plan.reviewer.model);
        return plan.reviewer;
      },
      completeSimple: async (model, context, options) => {
        assert.equal(model, plan.reviewer);
        assert.equal(context.systemPrompt, SYSTEM);
        assert.equal(context.messages.length, 1);
        assert.equal(context.messages[0].content, packet);
        assert.deepEqual(context.tools, []);
        assert.equal(options.maxTokens, plan.limits.requestedOutputTokens);
        assert.equal(options.reasoning, plan.effort);
        assert.equal(options.maxRetries, 0);
        assert.equal(options.timeoutMs, plan.limits.timeoutMs);
        inspect(options);
        return result;
      },
    };
  } } });
}

it("bounds UTF-8 packets by the selected budget without silent truncation", () => {
  for (const limits of Object.values(PROFILES)) {
    assert.equal(validatePacket("a".repeat(limits.inputBytes), limits), limits.inputBytes);
    assert.throws(() => validatePacket("é".repeat(limits.inputBytes), limits), /Narrow the scope/);
  }
  assert.throws(() => validatePacket(" "), /empty/);
});

it("uses each profile's OpenAI reasoning and budgets with no tools or history", async () => {
  for (const assessment of [undefined, verifiedSmallChange]) {
    const plan = planReview({ authorModel: "claude-opus-5-5", assessment });
    let calls = 0;
    const signal = new AbortController().signal;
    const result = await reviewOpenAI(packet, plan, signal, runtimeFixture({ ...openaiResult, model: plan.reviewer.model }, (options) => {
      calls++;
      assert.equal(options.signal, signal);
    }, plan));
    assert.equal(calls, 1);
    assert.equal(result.status, "complete");
    assert.deepEqual(result.usage, usage);
  }
});

it("does not substitute an unavailable default or selected OpenAI model", async () => {
  for (const plan of [openai, planReview({ authorModel: "claude-opus-5-5", model: "gpt-unavailable" })]) {
    const load = async () => ({ ModelRuntime: { create: async () => ({ getModel: (_provider, model) => {
      assert.equal(model, plan.reviewer.model);
      return undefined;
    } }) } });
    await assert.rejects(reviewOpenAI(packet, plan, undefined, load), new RegExp(`${plan.reviewer.model}.*No fallback`));
  }
});

for (const change of [
  { stopReason: "error", errorMessage: "Quota exceeded" }, { stopReason: "length" },
  { stopReason: "aborted" }, { model: "gpt-6-astra" }, { provider: "unexpected" },
  { content: [] }, { content: [{ type: "toolCall", name: "bash", arguments: {} }] },
]) {
  it(`does not certify an incomplete OpenAI review: ${JSON.stringify(change)}`, async () => {
    const result = await reviewOpenAI(packet, openai, undefined, runtimeFixture({ ...openaiResult, ...change }));
    assert.equal(result.status, "incomplete");
    assert.deepEqual(result.usage, usage);
  });
}

it("uses each profile's Claude effort and budgets with no tools, configuration or sessions", async () => {
  for (const assessment of [undefined, verifiedSmallChange]) {
    const plan = planReview({ authorModel: "gpt-6-astra", assessment });
    let directory;
    let calls = 0;
    const signal = new AbortController().signal;
    const result = await reviewAnthropic(packet, plan, signal, async (command, args, options) => {
      assert.equal(options.signal, signal);
      if (args.includes("auth")) {
        assert.deepEqual(args, ["--safe-mode", "--setting-sources", "", "auth", "status", "--json"]);
        return claudeAuth;
      }
      calls++;
      directory = options.cwd;
      assert.equal(command, "claude");
      for (const flag of ["--safe-mode", "--strict-mcp-config", "--disable-slash-commands", "--no-session-persistence"]) assert.ok(args.includes(flag));
      for (const [flag, value] of [["--tools", ""], ["--setting-sources", ""], ["--mcp-config", '{"mcpServers":{}}'], ["--model", plan.reviewer.model], ["--effort", plan.effort], ["--max-turns", "1"], ["--max-budget-usd", String(plan.limits.claudeBudgetUsd)], ["--system-prompt", SYSTEM]]) assert.equal(args[args.indexOf(flag) + 1], value);
      assert.ok(!args.includes("--fallback-model"));
      assert.ok(!args.includes("--resume"));
      assert.equal(options.input, packet);
      assert.equal(options.env.CLAUDE_CODE_MAX_OUTPUT_TOKENS, String(plan.limits.requestedOutputTokens));
      assert.equal(options.env.ENABLE_CLAUDEAI_MCP_SERVERS, "0");
      await access(directory);
      return { code: 0, stdout: JSON.stringify({ ...claudeResult, modelUsage: { [plan.reviewer.model]: {} } }), stderr: "" };
    });
    assert.equal(calls, 1);
    assert.equal(result.status, "complete");
    assert.deepEqual(result.usage, claudeResult.usage);
    assert.equal(result.estimatedUsd, 0.001);
    await assert.rejects(access(directory), { code: "ENOENT" });
  }
});

for (const change of [
  { subtype: "error_max_turns" }, { is_error: true }, { num_turns: 2 },
  { modelUsage: { "claude-sonnet-5-5": {} } }, { modelUsage: {} },
  { stop_reason: "max_tokens" }, { result: "" },
  { usage: { output_tokens: PROFILES.strong.requestedOutputTokens } }, { usage: undefined },
]) {
  it(`does not certify an incomplete Claude review: ${JSON.stringify(change)}`, async () => {
    const result = await reviewAnthropic(packet, anthropic, undefined, claudeFixture({ ...claudeResult, ...change }));
    assert.equal(result.status, "incomplete");
  });
}

it("refuses API billing, cloud providers and missing authentication before starting a model", async () => {
  for (const change of [{ authMethod: "api_key" }, { apiProvider: "bedrock" }, { loggedIn: false }]) {
    let calls = 0;
    await assert.rejects(reviewAnthropic(packet, anthropic, undefined, async (_command, args) => {
      calls++;
      assert.ok(args.includes("auth"));
      return { ...claudeAuth, stdout: JSON.stringify({ ...JSON.parse(claudeAuth.stdout), ...change }) };
    }), /requires a first-party subscription login/);
    assert.equal(calls, 1);
  }
});

it("surfaces quota errors even when Claude reports subtype success", async () => {
  const response = { ...claudeResult, is_error: true, result: "You've hit your weekly limit" };
  const result = await reviewAnthropic(packet, anthropic, undefined, claudeFixture(response));
  assert.equal(result.status, "incomplete");
  assert.equal(result.error, response.result);
});

it("reports Claude failures and cleans up without retrying", async () => {
  let directory;
  await assert.rejects(reviewAnthropic(packet, anthropic, undefined, async (_command, _args, options) => {
    directory = options.cwd;
    return { code: 1, stdout: "", stderr: "Unknown option --safe-mode" };
  }), /Unknown option/);
  await assert.rejects(access(directory), { code: "ENOENT" });
});

it("bounds process output and kills the process at the deadline", async () => {
  await assert.rejects(runProcess(process.execPath, ["-e", "setInterval(()=>{},1000)"], { signal: AbortSignal.timeout(100) }), /timed out/);
  await assert.rejects(runProcess(process.execPath, ["-e", 'process.stdout.write("x".repeat(2*1024*1024))']), /output limit/);
  await assert.rejects(runProcess("nonexistent-reviewer-binary", []), { code: "ENOENT" });
});

it("cancels the whole process group, not only the reviewer parent", async () => {
  const directory = await mkdtemp(join(tmpdir(), "review-process-test-"));
  const ready = join(directory, "ready");
  const leaked = join(directory, "leaked");
  const controller = new AbortController();
  const child = `const fs=require('node:fs');fs.writeFileSync(${JSON.stringify(ready)},'');setTimeout(()=>fs.writeFileSync(${JSON.stringify(leaked)},''),500)`;
  const parent = `require('node:child_process').spawn(process.execPath,['-e',${JSON.stringify(child)}],{stdio:'ignore'});setInterval(()=>{},1000)`;
  const stopped = assert.rejects(runProcess(process.execPath, ["-e", parent], { signal: controller.signal }), /cancelled/);
  try {
    for (let attempt = 0; attempt < 100; attempt++) {
      try { await access(ready); break; }
      catch (error) { if (error.code !== "ENOENT") throw error; }
      await new Promise((resolve) => setTimeout(resolve, 20));
    }
    await access(ready);
    controller.abort();
    await stopped;
    await new Promise((resolve) => setTimeout(resolve, 600));
    await assert.rejects(access(leaked), { code: "ENOENT" });
  } finally {
    controller.abort();
    await stopped;
    await rm(directory, { recursive: true, force: true });
  }
});

describe("CLI", () => {
  const script = fileURLToPath(new URL("../scripts/review.mjs", import.meta.url));

  it("runs the opposite vendor with the selected pair and high-effort budget", async () => {
    const directory = await mkdtemp(join(tmpdir(), "review-backend-test-"));
    const input = join(directory, "packet.md");
    try {
      await writeFile(input, packet);
      await writeFile(join(directory, "claude"), `#!${process.execPath}\nconst assert = require('node:assert/strict');\nconst args = process.argv.slice(2);\nif (args.includes('auth')) console.log(${JSON.stringify(claudeAuth.stdout)});\nelse {\nassert.equal(args[args.indexOf('--model') + 1], 'claude-opus-5-5');\nassert.equal(args[args.indexOf('--effort') + 1], 'high');\nassert.equal(args[args.indexOf('--tools') + 1], '');\nassert.equal(process.env.CLAUDE_CODE_MAX_OUTPUT_TOKENS, '16384');\nprocess.stdin.resume();\nprocess.stdin.on('end', () => console.log(${JSON.stringify(JSON.stringify(claudeResult))}));\n}\n`, { mode: 0o755 });
      const result = await runProcess(process.execPath, [script, "--input", input, "-m", "gpt-6-sol,claude-opus-5-5"], {
        env: { PATH: directory, PI_MODEL: "gpt-6-astra", CROSS_VENDOR_REVIEW_MODEL: "gpt-6-luna,claude-haiku-5-5" },
        signal: AbortSignal.timeout(10000),
      });
      assert.equal(result.code, 0, result.stdout);
      const review = JSON.parse(result.stdout);
      assert.equal(review.status, "complete");
      assert.equal(review.reviewer.model, "claude-opus-5-5");
      assert.equal(review.selection.source, "task");
      assert.equal(review.selection.profile, "strong");
      assert.deepEqual(review.usage, claudeResult.usage);
    } finally {
      await rm(directory, { recursive: true, force: true });
    }
  });

  for (const signal of ["SIGTERM", "SIGINT"]) {
    it(`cancels a detached reviewer when the harness interrupts the helper with ${signal}`, async () => {
      const directory = await mkdtemp(join(tmpdir(), "review-interrupt-test-"));
      const input = join(directory, "packet.md");
      const ready = join(directory, "ready.json");
      const leaked = join(directory, "leaked");
      let reviewerPid;
      try {
        await writeFile(input, packet);
        await writeFile(join(directory, "claude"), `#!${process.execPath}\nif (process.argv.includes('auth')) console.log(${JSON.stringify(claudeAuth.stdout)});\nelse {\nconst fs = require('node:fs');\nfs.writeFileSync(${JSON.stringify(ready)}, JSON.stringify({reviewer:process.pid,helper:process.ppid}));\nsetTimeout(()=>fs.writeFileSync(${JSON.stringify(leaked)}, 'still running'), 500);\nsetInterval(()=>{},1000);\n}\n`, { mode: 0o755 });
        const finished = runProcess(process.execPath, [script, "--input", input, "--author-model", "gpt-6-astra"], {
          env: { PATH: directory }, signal: AbortSignal.timeout(10000),
        });
        for (let attempt = 0; attempt < 100; attempt++) {
          try { await access(ready); break; }
          catch (error) { if (error.code !== "ENOENT") throw error; }
          await new Promise((resolve) => setTimeout(resolve, 20));
        }
        const pids = JSON.parse(await readFile(ready, "utf8"));
        reviewerPid = pids.reviewer;
        process.kill(pids.helper, signal);
        const result = await finished;
        await new Promise((resolve) => setTimeout(resolve, 600));
        await assert.rejects(access(leaked), { code: "ENOENT" });
        assert.equal(result.code, 1, result.stdout);
        const response = JSON.parse(result.stdout);
        assert.equal(response.status, "incomplete");
        assert.equal(response.selection.profile, "strong");
      } finally {
        if (reviewerPid) {
          try { process.kill(-reviewerPid, "SIGKILL"); }
          catch (error) { if (error.code !== "ESRCH") throw error; }
        }
        await rm(directory, { recursive: true, force: true });
      }
    });
  }

  it("runs when the installed skill path contains a symlink", async () => {
    const directory = await mkdtemp(join(tmpdir(), "review-symlink-test-"));
    try {
      const linked = join(directory, "review.mjs");
      const input = join(directory, "packet.md");
      await symlink(script, linked);
      await writeFile(input, packet);
      const result = await runProcess(process.execPath, [linked, "--input", input, "--author-model", "gpt-6-astra", "--dry-run"], { env: { PATH: "" } });
      assert.equal(result.code, 0, result.stdout);
      assert.match(result.stdout, /"status": "dry-run"/);
    } finally {
      await rm(directory, { recursive: true, force: true });
    }
  });

  it("reports selection and unknown usage when the backend process cannot start", async () => {
    const directory = await mkdtemp(join(tmpdir(), "review-failure-test-"));
    try {
      const input = join(directory, "packet.md");
      await writeFile(input, packet);
      const result = await runProcess(process.execPath, [script, "--input", input, "--author-model", "gpt-6-astra"], { env: { PATH: "" } });
      assert.equal(result.code, 1);
      const response = JSON.parse(result.stdout);
      assert.equal(response.status, "incomplete");
      assert.equal(response.selection.profile, "strong");
      assert.equal(response.reviewer.model, "claude-opus-5-5");
      assert.equal(response.usage, null);
      assert.match(response.error, /ENOENT/);
    } finally {
      await rm(directory, { recursive: true, force: true });
    }
  });
});
