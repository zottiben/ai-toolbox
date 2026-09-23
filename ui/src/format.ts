import type { Counts, Origin, Standing, State, Summary } from "./types";

/** `/Users/me/src/thing` -> `~/src/thing`, which is how people say it. */
export function tilde(path: string, home?: string): string {
  const prefix = home ?? guessHome(path);
  return prefix && path.startsWith(prefix) ? `~${path.slice(prefix.length)}` : path;
}

function guessHome(path: string): string | null {
  const match = /^(\/(?:Users|home)\/[^/]+)\//.exec(path);
  return match?.[1] ?? null;
}

export function basename(path: string): string {
  const parts = path.replace(/\/+$/, "").split("/");
  return parts[parts.length - 1] ?? path;
}

/** Why a repo could not be read, with its own path taken off the front.
 *
 * The error names the file in full, and the row above it already says where the repo is -
 * left whole, the two lines truncate to the same prefix and the part that matters, which
 * file and what is wrong with it, is the part that gets cut off. */
export function problemDetail(summary: Pick<Summary, "repo" | "problem">): string | null {
  if (!summary.problem) return null;
  const prefix = `${summary.repo.path}/`;
  return summary.problem.startsWith(prefix)
    ? summary.problem.slice(prefix.length)
    : summary.problem;
}

export function stateLabel(summary: Pick<Summary, "state" | "exists" | "worktrees_in_step">): string {
  if (!summary.exists) return "gone from disk";
  // A healthy repo whose worktrees disagree is not healthy in the way that matters -
  // half your branches have no hooks.
  if (summary.state === "healthy" && !summary.worktrees_in_step) return "worktrees out of step";
  return {
    unconfigured: "not set up",
    healthy: "healthy",
    attention: "needs a look",
    broken: "broken",
  }[summary.state];
}

export function stateTone(summary: Pick<Summary, "state" | "exists" | "worktrees_in_step">): string {
  if (!summary.exists) return "muted";
  if (summary.state === "healthy" && !summary.worktrees_in_step) return "warn";
  return { unconfigured: "muted", healthy: "ok", attention: "warn", broken: "bad" }[summary.state];
}

export function originLabel(origin: Origin): string {
  switch (origin.state) {
    case "managed":
      return "from the catalogue";
    case "stale":
      return "an older version - safe to update";
    case "modified":
      return "edited here";
    case "local":
      return "not from the catalogue";
    case "broken":
      return origin.why;
  }
}

export function originTone(origin: Origin): string {
  switch (origin.state) {
    case "managed":
      return "ok";
    case "stale":
    case "modified":
      return "warn";
    case "local":
      return "muted";
    case "broken":
      return "bad";
  }
}

export function countsLabel(counts: Counts): string {
  const parts: string[] = [];
  if (counts.managed) parts.push(`${counts.managed} managed`);
  if (counts.stale) parts.push(`${counts.stale} stale`);
  if (counts.modified) parts.push(`${counts.modified} edited`);
  if (counts.local) parts.push(`${counts.local} local`);
  if (counts.broken) parts.push(`${counts.broken} broken`);
  return parts.join(", ");
}

export function standingLabel(standing: Standing): string {
  return {
    reference: "reference",
    // Not damage: a worktree added since the repo was set up has none of this, because
    // none of it is tracked by git.
    unconfigured: "not configured yet",
    "in-step": "in step",
    diverged: "out of step",
    unusable: "nothing on disk",
  }[standing];
}

/** "3 skills, 1 hook" - items, not the files they happen to be made of. */
export function describeFiles(paths: string[]): string {
  const groups = new Map<string, Set<string>>();
  for (const path of paths) {
    const [kind, name] = itemOf(path);
    if (!groups.has(kind)) groups.set(kind, new Set());
    groups.get(kind)?.add(name);
  }
  const parts: string[] = [];
  for (const [kind, names] of groups) {
    if (kind === "file") {
      parts.push([...names].join(", "));
    } else if (names.size === 1) {
      parts.push(`${kind} ${[...names][0]}`);
    } else {
      parts.push(`${names.size} ${kind}s`);
    }
  }
  return parts.join(", ");
}

function itemOf(path: string): [string, string] {
  const skill = /^\.agents\/skills\/([^/]+)/.exec(path);
  if (skill?.[1]) return ["skill", skill[1]];
  const hook = /^\.agents\/hooks\/(.+)\.sh$/.exec(path);
  if (hook?.[1]) return ["hook", hook[1]];
  const helper = /^\.agents\/mcp\/(.+)$/.exec(path);
  if (helper?.[1]) return ["helper", helper[1]];
  return ["file", path];
}

export function pluralise(count: number, one: string, many = `${one}s`): string {
  return `${count} ${count === 1 ? one : many}`;
}

/**
 * A skill's description down to something a list can hold.
 *
 * These are written for a model to match against, not for a person to read - several
 * sentences of "use when…" each. Rendered in full they turn the installed panel into a
 * wall of text, and the one thing somebody is scanning for (the name, and whether it is
 * managed) gets lost in it. The first sentence is the part that says what it is.
 */
export function summarise(text: string | null, width = 110): string | null {
  if (!text) return null;
  const first = text.split(/(?<=\.)\s|\s[-—]\s/)[0]?.trim() ?? text;
  if (first.length <= width) return first;
  const cut = first.slice(0, width);
  const boundary = cut.lastIndexOf(" ");
  return `${boundary > 40 ? cut.slice(0, boundary) : cut}…`;
}

export const STATES: State[] = ["broken", "attention", "healthy", "unconfigured"];
