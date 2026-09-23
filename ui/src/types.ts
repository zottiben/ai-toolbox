// The API's shapes, as the board reads them. Mirrors the `serde` output of
// `ai-toolbox-core` - anything renamed there has to be renamed here, which is the price
// of not generating this file and the reason each name below matches its Rust field.

export type Harness = "claude" | "codex" | "pi";

export type State = "unconfigured" | "healthy" | "attention" | "broken";

export type Severity = "broken" | "warning" | "note";

export type ItemKind = "hook" | "skill" | "server" | "helper";

/** The five states of an installed item. `broken` carries why. */
export type Origin =
  | { state: "managed" }
  | { state: "modified" }
  | { state: "stale" }
  | { state: "local" }
  | { state: "broken"; why: string };

export interface Item {
  kind: ItemKind;
  name: string;
  catalogue_key: string | null;
  origin: Origin;
  path: string;
  description: string | null;
  hash: string;
}

export interface Counts {
  managed: number;
  modified: number;
  stale: number;
  local: number;
  broken: number;
}

export interface Repo {
  id: number;
  path: string;
  first_seen: string;
  last_seen: string;
  /** Null for a repo that was found rather than set up. */
  last_configured: string | null;
}

export interface Summary {
  repo: Repo;
  exists: boolean;
  state: State;
  harnesses: Harness[];
  counts: Counts;
  stack: string[];
  worktrees: number;
  worktrees_in_step: boolean;
  /** Why this repo could not be read, when it could not be. Null is the normal case. */
  problem: string | null;
}

export interface Finding {
  severity: Severity;
  code: string;
  what: string;
  advice: string | null;
  path: string;
  repairable: boolean;
}

export type SkillsLink =
  | { state: "missing" }
  | { state: "link"; target: string; resolves: boolean }
  | { state: "own-copy"; skills: number };

export interface Inventory {
  repo: string;
  agents_md: boolean;
  claude_md: boolean;
  skills: { name: string; path: string }[];
  hooks: { name: string; path: string; executable: boolean }[];
  helpers: { name: string }[];
  servers: { name: string; helpers: string[] }[];
  claude: { settings: boolean; skills: SkillsLink; wired: WiredHook[] };
  codex: { config: boolean; servers: string[]; wired: WiredHook[] };
  pi: { overrides: { name: string }[] };
  legacy: string[];
}

export interface WiredHook {
  event: string;
  matcher: string | null;
  command: string;
  script: string;
}

export interface Available {
  hooks: string[];
  skills: string[];
  presets: string[];
}

export type Standing = "reference" | "unconfigured" | "in-step" | "diverged" | "unusable";

export interface WorktreeState {
  worktree: {
    path: string;
    branch: string | null;
    is_main: boolean;
    prunable: string | null;
  };
  standing: Standing;
  diff: { missing: string[]; different: string[]; extra: string[] };
}

export interface Comparison {
  reference: string;
  worktrees: WorktreeState[];
}

export interface Secret {
  name: string;
  source: "environment" | "dot-env";
}

export interface Recommendation {
  detected: string[];
  hooks: string[];
  mcp: string[];
  skills: string[];
  rules: string[];
  notes: string[];
}

/** One project in full. `exists: false` means the directory has gone. */
export interface Detail {
  repo: Repo;
  exists: boolean;
  state?: State;
  harnesses?: Harness[];
  inventory?: Inventory;
  items?: Item[];
  available?: Available;
  recommendation?: Recommendation;
  findings: Finding[];
  worktrees: Comparison | null;
  secrets: Secret[];
}

export interface CatalogueHook {
  name: string;
  summary: string | null;
}

export interface CataloguePreset {
  name: string;
  servers: Record<string, unknown>;
  secrets: Secret[];
}

export interface CatalogueSkill {
  key: string;
  name: string;
  group: string | null;
  description: string | null;
}

export interface Catalogue {
  root: string;
  hooks: CatalogueHook[];
  presets: CataloguePreset[];
  skills: CatalogueSkill[];
  rules: { name: string }[];
  helpers: { name: string }[];
}

export interface MachineInfo {
  machine: {
    charters: { harness: Harness; path: string; installed: boolean }[];
    pi_mcp: "ready" | "missing" | "no-pi";
  };
  catalogue_root: string;
  version: string;
  harnesses: Harness[];
  bundle: { embedded: boolean; files: number };
}

export interface PlanAction {
  summary: string;
  path: string;
  noop: boolean;
}

export interface PlanView {
  actions: PlanAction[];
  changes: number;
  warnings: string[];
  secrets: Secret[];
}

export interface Applied {
  applied: number;
  skipped: number;
  stale: string[];
}

export interface ScanResult {
  added: string[];
  known: string[];
  missing_roots: string[];
}

/** What the board asks the server to do. Planned first, then applied. */
export type Request =
  | {
      kind: "install";
      hooks?: string[];
      presets?: string[];
      skills?: string[];
      harnesses?: Harness[];
      scaffold?: boolean;
    }
  | { kind: "recommended"; harnesses?: Harness[]; scaffold?: boolean }
  | { kind: "repair" }
  | { kind: "converge"; worktree?: string };
