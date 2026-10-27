export const PROFILES = Object.freeze({
  light: Object.freeze({ effort: "medium", inputBytes: 65536, requestedOutputTokens: 4096, timeoutMs: 120000, claudeBudgetUsd: 0.25 }),
  strong: Object.freeze({ effort: "high", inputBytes: 262144, requestedOutputTokens: 16384, timeoutMs: 600000, claudeBudgetUsd: 2 }),
});

const MODELS = {
  light: { openai: "gpt-6-luna", anthropic: "claude-haiku-5-5" },
  strong: { openai: "gpt-6-sol", anthropic: "claude-opus-5-5" },
};
const CHECKS = ["requirements", "tests", "coverage", "familiarity"];
const LEVELS = ["low", "medium", "high", "unknown"];

function vendorOf(model) {
  if (/^claude-[a-z0-9]/.test(model ?? "")) return "anthropic";
  if (/^(?:gpt-[a-z0-9]|o[1-9](?:-|$))/.test(model ?? "")) return "openai";
}

function parseReviewer(value) {
  const match = /^(?:([a-z-]+)\/)?([a-z0-9][a-z0-9.-]*)$/.exec(value);
  const [, provider, model] = match ?? [];
  const vendor = vendorOf(model);
  if (vendor === "anthropic" && [undefined, "anthropic", "claude-code", "claude-subscription"].includes(provider)) {
    return { vendor, provider: "claude-code", model };
  }
  if (vendor === "openai" && [undefined, "openai", "openai-codex"].includes(provider)) {
    return { vendor, provider: "openai-codex", model };
  }
  throw new Error("Unsupported reviewer model. Use an exact GPT/o-series or Claude model ID with a matching provider, not a moving alias. No fallback was started.");
}

function checkObject(value, keys, label) {
  if (!value || typeof value !== "object" || Array.isArray(value) || Object.keys(value).some((key) => !keys.includes(key))) {
    throw new Error(`${label} must be an object containing only: ${keys.join(", ")}.`);
  }
}

export function assessChange(assessment = {}) {
  checkObject(assessment, ["complexity", "risk", "rationale", "checks"], "Assessment");
  const { complexity = "unknown", risk = "unknown", rationale = "", checks = {} } = assessment;
  if (!LEVELS.includes(complexity) || !LEVELS.includes(risk)) throw new Error("Assessment complexity and risk must be low, medium, high or unknown.");
  if (typeof rationale !== "string" || ((complexity !== "unknown" || risk !== "unknown") && !rationale.trim())) {
    throw new Error("Assessment requires a rationale supporting the complexity and risk ratings.");
  }
  checkObject(checks, CHECKS, "Assessment checks");
  const evidence = {};
  for (const key of CHECKS) {
    if (checks[key] === undefined) {
      evidence[key] = { passed: null, evidence: "Not supplied" };
      continue;
    }
    checkObject(checks[key], ["passed", "evidence"], `Assessment check ${key}`);
    const { passed, evidence: detail = "" } = checks[key];
    if (![true, false, null].includes(passed) || typeof detail !== "string" || (passed === true && !detail.trim())) {
      throw new Error(`Assessment check ${key} requires passed (true, false or null) and concrete evidence for a passed check.`);
    }
    evidence[key] = { passed, evidence: detail };
  }
  const score = Object.values(evidence).filter((check) => check.passed === true).length;
  return { complexity, risk, rationale, confidence: { score, max: CHECKS.length, basis: "Caller-supplied evidence checks, not a probability or independently verified result", checks: evidence } };
}

export function planReview({ authorModel, model, projectModel, globalModel, assessment, inputBytes = 0 }) {
  const authorId = authorModel?.split("/").at(-1)?.toLowerCase().replace(/^(?:[a-z]+\.)?anthropic\./, "");
  const authorVendor = vendorOf(authorId);
  if (!authorVendor) throw new Error("Unknown author model. Supply --author-model with the actual OpenAI or Anthropic model ID; cross-vendor routing cannot be inferred from a reviewer preference.");
  const vendor = authorVendor === "anthropic" ? "openai" : "anthropic";
  const assessed = assessChange(assessment);
  const reasons = [];
  if (assessed.complexity !== "low") reasons.push(`Implementation complexity is ${assessed.complexity}.`);
  if (assessed.risk !== "low") reasons.push(`Change risk is ${assessed.risk}.`);
  if (assessed.confidence.score !== assessed.confidence.max) reasons.push(`Confidence evidence supports ${assessed.confidence.score}/${assessed.confidence.max} checks.`);
  if (inputBytes > PROFILES.light.inputBytes) reasons.push("The packet exceeds the light review input budget.");
  let profile = reasons.length ? "strong" : "light";
  if (profile === "light") reasons.push("Low complexity, low risk and all four confidence checks are supported by evidence.");

  const preference = [["task", model], ["project", projectModel], ["global", globalModel]].find(([, value]) => value !== undefined);
  const source = preference?.[0] ?? "automatic";
  let reviewer;
  if (preference) {
    const choices = preference[1].split(",").map((value) => parseReviewer(value.trim()));
    if (choices.length > 2 || new Set(choices.map((choice) => choice.vendor)).size !== choices.length) {
      throw new Error("A reviewer preference must contain at most one model per vendor.");
    }
    reviewer = choices.find((choice) => choice.vendor === vendor);
    if (!reviewer) throw new Error(`Cross-vendor review requires the opposing vendor (${vendor}). Supply its model or a comma-separated OpenAI,Anthropic pair; no fallback was started.`);
    reasons.push(`The ${source} preference selected ${reviewer.model} from the opposing vendor.`);
    if (profile === "light" && reviewer.model !== MODELS.light[vendor]) {
      profile = "strong";
      reasons.push("The selected model receives the high-reasoning review budget rather than being limited to lightweight settings.");
    }
    if (profile === "strong" && reviewer.model === MODELS.light[vendor]) {
      reasons.push("The explicit preference selects a lightweight model despite a strong review assessment; high effort and larger budgets are retained.");
    }
  } else {
    reviewer = parseReviewer(MODELS[profile][vendor]);
  }
  const { effort, ...limits } = PROFILES[profile];
  return {
    reviewer, effort, limits,
    selection: { source, authorModel, authorVendor, profile, ...assessed, reasons },
  };
}
