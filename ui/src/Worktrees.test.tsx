import { render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { Worktrees } from "./Worktrees";
import type { Comparison } from "./types";

function comparison(overrides: Partial<Comparison> = {}): Comparison {
  return {
    reference: "/src/thing",
    worktrees: [
      {
        worktree: { path: "/src/thing", branch: "main", is_main: true, prunable: null },
        standing: "reference",
        diff: { missing: [], different: [], extra: [] },
      },
    ],
    ...overrides,
  };
}

describe("Worktrees", () => {
  it("says there is nothing to compare rather than looking broken", () => {
    render(<Worktrees comparison={null} onConverge={vi.fn()} />);
    expect(screen.getByText(/not a git repository/i)).toBeDefined();
  });

  it("calls a fresh worktree unconfigured, not damaged", () => {
    // The distinction that matters: none of this is tracked by git, so a worktree added
    // after the repo was set up starts empty. Listing seventeen faults would be wrong.
    const data = comparison();
    data.worktrees.push({
      worktree: { path: "/src/feature", branch: "feature", is_main: false, prunable: null },
      standing: "unconfigured",
      diff: { missing: [".mcp.json"], different: [], extra: [] },
    });

    render(<Worktrees comparison={data} onConverge={vi.fn()} />);
    expect(screen.getByText("not configured yet")).toBeDefined();
    expect(screen.getByText(/Nothing from the toolkit is here yet/)).toBeDefined();
  });

  it("describes drift as items and marks extras as left alone", () => {
    const data = comparison();
    data.worktrees.push({
      worktree: { path: "/src/feature", branch: "feature", is_main: false, prunable: null },
      standing: "diverged",
      diff: {
        missing: [".agents/skills/pre-pr/SKILL.md"],
        different: [".mcp.json"],
        extra: [".agents/skills/branch-only/SKILL.md"],
      },
    });

    render(<Worktrees comparison={data} onConverge={vi.fn()} />);
    expect(screen.getByText("missing skill pre-pr")).toBeDefined();
    expect(screen.getByText(".mcp.json differ")).toBeDefined();
    expect(screen.getByText(/skill branch-only only here - left alone/)).toBeDefined();
  });

  it("offers to converge only the ones that are out of step", () => {
    const data = comparison();
    data.worktrees.push(
      {
        worktree: { path: "/src/a", branch: "a", is_main: false, prunable: null },
        standing: "in-step",
        diff: { missing: [], different: [], extra: [] },
      },
      {
        worktree: { path: "/src/b", branch: "b", is_main: false, prunable: null },
        standing: "unconfigured",
        diff: { missing: [], different: [], extra: [] },
      },
    );

    render(<Worktrees comparison={data} onConverge={vi.fn()} />);
    expect(screen.getByText("Bring 1 into step")).toBeDefined();
    // One Sync button, on the one that needs it.
    expect(screen.getAllByText("Sync")).toHaveLength(1);
  });

  it("says every worktree matches when they do", () => {
    const data = comparison();
    data.worktrees.push({
      worktree: { path: "/src/a", branch: "a", is_main: false, prunable: null },
      standing: "in-step",
      diff: { missing: [], different: [], extra: [] },
    });

    render(<Worktrees comparison={data} onConverge={vi.fn()} />);
    expect(screen.getByText("Every worktree matches.")).toBeDefined();
    expect(screen.queryByText(/Bring/)).toBeNull();
  });
});
