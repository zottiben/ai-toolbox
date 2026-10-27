import assert from "node:assert/strict";
import { access, mkdtemp, rm, symlink, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, it } from "node:test";
import { LIMITS, SYSTEM, selectReviewer, validatePacket, reviewOpenAI, reviewAnthropic, runProcess } from "../scripts/review.mjs";

const packet = "Requirement: zero is valid.\n--- a/check.js\n+++ b/check.js\n@@ -1 +1 @@\n-return n >= 0;\n+return n > 0;";
const openai = selectReviewer("claude-opus-5-5");
const anthropic = selectReviewer("gpt-6-astra");
const usage = { input: 100, output: 20, cacheRead: 0, cacheWrite: 0, totalTokens: 120 };

function runtimeFixture(result, inspect = () => {}, reviewer = openai) {
  return async () => ({ ModelRuntime: { create: async (options) => {
    assert.equal(options.modelsPath, null);
    assert.equal(options.allowModelNetwork, false);
    return {
      getModel: (provider, model) => {
        assert.equal(provider, reviewer.provider);
        assert.equal(model, reviewer.model);
        return reviewer;
      },
      completeSimple: async (model, context, options) => {
        assert.equal(model, reviewer);
        assert.equal(context.systemPrompt, SYSTEM);
        assert.equal(context.messages.length, 1);
        assert.equal(context.messages[0].content, packet);
        assert.deepEqual(context.tools, []);
        assert.equal(options.maxTokens, LIMITS.requestedOutputTokens);
        assert.equal(options.reasoning, "low");
        assert.equal(options.maxRetries, 0);
        assert.equal(options.timeoutMs, LIMITS.timeoutMs);
        inspect(options);
        return result;
      },
    };
  } } });
}

const openaiResult = { provider: openai.provider, model: openai.model, stopReason: "stop", content: [{ type: "text", text: "Zero is incorrectly rejected at check.js:1." }], usage };
const claudeAuth = { code: 0, stdout: JSON.stringify({ loggedIn: true, authMethod: "claude.ai", apiProvider: "firstParty" }), stderr: "" };
const claudeFixture = (response) => async (_command, args) => args.includes("auth") ? claudeAuth : { code: 0, stdout: JSON.stringify(response), stderr: "" };
const claudeResult = { type: "result", subtype: "success", is_error: false, num_turns: 1, result: "Zero is incorrectly rejected at check.js:1.", modelUsage: { [anthropic.model]: {} }, usage: { input_tokens: 100, output_tokens: 20 }, total_cost_usd: 0.001 };

it("routes the actual author model, including provider-qualified and Bedrock IDs", () => {
  for (const model of ["claude-opus-5-5", "anthropic/claude-sonnet-5-5", "claude-subscription/claude-haiku-5-5", "anthropic.claude-sonnet-4-5"]) assert.deepEqual(selectReviewer(model), openai);
  for (const model of ["gpt-6-astra", "openai-codex/gpt-6-sol", "o3", "o4-mini"]) assert.deepEqual(selectReviewer(model), anthropic);
  for (const model of [undefined, "", "gemini-3", "opus", "my-router"]) assert.throws(() => selectReviewer(model), /Unknown author model/);
});

it("honours explicit reviewer models even for the same vendor or an unknown author", () => {
  for (const author of [undefined, "gemini-3", "gpt-6-astra", "claude-opus-5-5"]) {
    for (const model of ["gpt-6-sol", "openai/gpt-6-sol", "openai-codex/gpt-6-sol", "o3"]) {
      assert.deepEqual(selectReviewer(author, model), { ...openai, model: model.split("/").at(-1) });
    }
    for (const model of ["claude-sonnet-5-5", "anthropic/claude-sonnet-5-5", "claude-code/claude-sonnet-5-5", "claude-subscription/claude-sonnet-5-5"]) {
      assert.deepEqual(selectReviewer(author, model), { ...anthropic, model: "claude-sonnet-5-5" });
    }
  }
});

it("rejects ambiguous, unsupported or mismatched reviewer IDs instead of using a default", () => {
  for (const model of ["", "sonnet", "opus", "gemini-3", "--help", "gpt-*", "openai-codex/claude-sonnet-5-5", "anthropic/gpt-6-sol", "unknown/gpt-6-sol", "extra/openai-codex/gpt-6-sol"]) {
    assert.throws(() => selectReviewer("gpt-6-astra", model), /reviewer model/);
  }
});

it("bounds the packet by UTF-8 bytes without silently truncating it", () => {
  assert.equal(validatePacket("a".repeat(LIMITS.inputBytes)), LIMITS.inputBytes);
  assert.throws(() => validatePacket(" "), /empty/);
  assert.throws(() => validatePacket("é".repeat(LIMITS.inputBytes)), /Narrow the scope/);
});

it("calls OpenAI once with no tools or history and returns its usage", async () => {
  let calls = 0;
  const signal = new AbortController().signal;
  const result = await reviewOpenAI(packet, openai, signal, runtimeFixture(openaiResult, (options) => {
    calls++;
    assert.equal(options.signal, signal);
  }));
  assert.equal(calls, 1);
  assert.equal(result.status, "complete");
  assert.deepEqual(result.usage, usage);
});

it("passes a selected model to each backend and still verifies the returned model", async () => {
  const selectedOpenAI = selectReviewer(undefined, "gpt-6-sol");
  const openAIResponse = { ...openaiResult, model: selectedOpenAI.model };
  const openAIReview = await reviewOpenAI(packet, selectedOpenAI, undefined, runtimeFixture(openAIResponse, () => {}, selectedOpenAI));
  assert.equal(openAIReview.status, "complete");
  assert.deepEqual(openAIReview.reviewer, selectedOpenAI);
  const wrongOpenAI = await reviewOpenAI(packet, selectedOpenAI, undefined, runtimeFixture(openaiResult, () => {}, selectedOpenAI));
  assert.equal(wrongOpenAI.status, "incomplete");

  const selectedClaude = selectReviewer(undefined, "claude-sonnet-5-5");
  const claudeResponse = { ...claudeResult, modelUsage: { [selectedClaude.model]: {} } };
  const run = async (command, args, options) => {
    if (!args.includes("auth")) assert.equal(args[args.indexOf("--model") + 1], selectedClaude.model);
    return claudeFixture(claudeResponse)(command, args, options);
  };
  const claudeReview = await reviewAnthropic(packet, selectedClaude, undefined, run);
  assert.equal(claudeReview.status, "complete");
  assert.deepEqual(claudeReview.reviewer, selectedClaude);
  const wrongClaude = await reviewAnthropic(packet, selectedClaude, undefined, claudeFixture(claudeResult));
  assert.equal(wrongClaude.status, "incomplete");
});

it("does not substitute an unavailable default or selected OpenAI model", async () => {
  for (const reviewer of [openai, selectReviewer(undefined, "gpt-unavailable")]) {
    const load = async () => ({ ModelRuntime: { create: async () => ({ getModel: (_provider, model) => {
      assert.equal(model, reviewer.model);
      return undefined;
    } }) } });
    await assert.rejects(reviewOpenAI(packet, reviewer, undefined, load), new RegExp(`${reviewer.model}.*No fallback`));
  }
});

for (const change of [
  { stopReason: "error", errorMessage: "Quota exceeded" },
  { stopReason: "length" },
  { stopReason: "aborted" },
  { model: "gpt-6-astra" },
  { provider: "unexpected" },
  { content: [] },
  { content: [{ type: "toolCall", name: "bash", arguments: {} }] },
]) {
  it(`does not certify an incomplete OpenAI review: ${JSON.stringify(change)}`, async () => {
    const result = await reviewOpenAI(packet, openai, undefined, runtimeFixture({ ...openaiResult, ...change }));
    assert.equal(result.status, "incomplete");
    assert.deepEqual(result.usage, usage);
  });
}

it("isolates Claude with no tools, configuration, sessions, or fallback", async () => {
  let directory;
  let calls = 0;
  const result = await reviewAnthropic(packet, anthropic, undefined, async (command, args, options) => {
    if (args.includes("auth")) {
      assert.deepEqual(args, ["--safe-mode", "--setting-sources", "", "auth", "status", "--json"]);
      return claudeAuth;
    }
    calls++;
    directory = options.cwd;
    assert.equal(command, "claude");
    for (const flag of ["--safe-mode", "--strict-mcp-config", "--disable-slash-commands", "--no-session-persistence"]) assert.ok(args.includes(flag));
    for (const [flag, value] of [["--tools", ""], ["--setting-sources", ""], ["--mcp-config", '{"mcpServers":{}}'], ["--model", anthropic.model], ["--effort", "low"], ["--max-turns", "1"], ["--max-budget-usd", "0.25"], ["--system-prompt", SYSTEM]]) assert.equal(args[args.indexOf(flag) + 1], value);
    assert.ok(!args.includes("--fallback-model"));
    assert.ok(!args.includes("--resume"));
    assert.equal(options.input, packet);
    assert.equal(options.env.CLAUDE_CODE_MAX_OUTPUT_TOKENS, String(LIMITS.requestedOutputTokens));
    assert.equal(options.env.ENABLE_CLAUDEAI_MCP_SERVERS, "0");
    await access(directory);
    return { code: 0, stdout: JSON.stringify(claudeResult), stderr: "" };
  });
  assert.equal(calls, 1);
  assert.equal(result.status, "complete");
  assert.deepEqual(result.usage, claudeResult.usage);
  assert.equal(result.estimatedUsd, 0.001);
  await assert.rejects(access(directory), { code: "ENOENT" });
});

for (const change of [
  { subtype: "error_max_turns" }, { is_error: true }, { num_turns: 2 },
  { modelUsage: { "claude-opus-5-5": {} } }, { modelUsage: {} },
  { stop_reason: "max_tokens" }, { result: "" },
  { usage: { output_tokens: LIMITS.requestedOutputTokens } }, { usage: undefined },
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
  it("accepts --model/-m, with CLI taking precedence over environment and automatic routing", async () => {
    const directory = await mkdtemp(join(tmpdir(), "review-model-test-"));
    const input = join(directory, "packet.md");
    const expected = { ...anthropic, model: "claude-sonnet-5-5" };
    try {
      await writeFile(input, packet);
      for (const flag of ["--model", "-m"]) {
        const result = await runProcess(process.execPath, [script, "--input", input, flag, expected.model, "--dry-run"], {
          env: { PATH: "", PI_MODEL: "unknown", CROSS_VENDOR_REVIEW_MODEL: "invalid-model" },
        });
        assert.equal(result.code, 0, result.stdout);
        assert.deepEqual(JSON.parse(result.stdout).reviewer, expected);
      }
      const configured = await runProcess(process.execPath, [script, "--input", input, "--dry-run"], {
        env: { PATH: "", PI_MODEL: "unknown", CROSS_VENDOR_REVIEW_MODEL: expected.model },
      });
      assert.equal(configured.code, 0, configured.stdout);
      assert.deepEqual(JSON.parse(configured.stdout).reviewer, expected);
      for (const model of ["", "invalid-model"]) {
        const invalid = await runProcess(process.execPath, [script, "--input", input, "--dry-run"], {
          env: { PATH: "", PI_MODEL: "gpt-6-astra", CROSS_VENDOR_REVIEW_MODEL: model },
        });
        assert.equal(invalid.code, 1);
        assert.match(invalid.stdout, /reviewer model/);
      }
    } finally {
      await rm(directory, { recursive: true, force: true });
    }
  });

  it("runs the explicitly selected backend through the real CLI", async () => {
    const directory = await mkdtemp(join(tmpdir(), "review-backend-test-"));
    const input = join(directory, "packet.md");
    const model = "claude-sonnet-5-5";
    const response = { ...claudeResult, modelUsage: { [model]: {} } };
    try {
      await writeFile(input, packet);
      await writeFile(join(directory, "claude"), `#!${process.execPath}\nconst assert = require('node:assert/strict');\nconst args = process.argv.slice(2);\nif (args.includes('auth')) console.log(${JSON.stringify(claudeAuth.stdout)});\nelse {\nassert.equal(args[args.indexOf('--model') + 1], ${JSON.stringify(model)});\nassert.equal(args[args.indexOf('--tools') + 1], '');\nprocess.stdin.resume();\nprocess.stdin.on('end', () => console.log(${JSON.stringify(JSON.stringify(response))}));\n}\n`, { mode: 0o755 });
      const result = await runProcess(process.execPath, [script, "--input", input, "-m", model], {
        env: { PATH: directory, PI_MODEL: "claude-opus-5-5", CROSS_VENDOR_REVIEW_MODEL: "gpt-6-sol" },
        signal: AbortSignal.timeout(10000),
      });
      assert.equal(result.code, 0, result.stdout);
      const review = JSON.parse(result.stdout);
      assert.equal(review.status, "complete");
      assert.equal(review.reviewer.model, model);
      assert.deepEqual(review.usage, response.usage);
    } finally {
      await rm(directory, { recursive: true, force: true });
    }
  });

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

  it("dry-runs both routes and rejects unknown models or oversized packets without model access", async () => {
    const directory = await mkdtemp(join(tmpdir(), "review-test-"));
    const input = join(directory, "packet.md");
    try {
      await writeFile(input, packet);
      for (const [author, expected] of [["claude-opus-5-5", openai], ["gpt-6-astra", anthropic]]) {
        const result = await runProcess(process.execPath, [script, "--input", input, "--author-model", author, "--dry-run"], { env: { PATH: "" } });
        assert.equal(result.code, 0, result.stdout);
        assert.deepEqual(JSON.parse(result.stdout).reviewer, expected);
      }
      const unknown = await runProcess(process.execPath, [script, "--input", input, "--author-model", "unknown", "--dry-run"], { env: { PATH: "" } });
      assert.equal(unknown.code, 1);
      await writeFile(input, "x".repeat(LIMITS.inputBytes + 1));
      const large = await runProcess(process.execPath, [script, "--input", input, "--author-model", "gpt-6-astra"], { env: { PATH: "" } });
      assert.equal(large.code, 1);
      assert.match(large.stdout, /Narrow the scope/);
    } finally {
      await rm(directory, { recursive: true, force: true });
    }
  });
});
