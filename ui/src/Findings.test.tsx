import { render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { Findings } from "./Findings";
import type { Finding } from "./types";

const broken: Finding = {
  severity: "broken",
  code: "skills-link-dangling",
  what: "Claude Code sees no skills here: .claude/skills points at ../.agents/skills, which does not exist",
  advice: null,
  path: "/src/thing/.claude/skills",
  repairable: true,
};

const yours: Finding = {
  severity: "note",
  code: "item-modified",
  what: "skill pre-pr has been edited here and is no version the catalogue ever shipped",
  advice: "left alone - this is your edit. Reinstall it to take the catalogue's copy",
  path: "/src/thing/.agents/skills/pre-pr",
  repairable: false,
};

describe("Findings", () => {
  it("says nothing is wrong when nothing is", () => {
    render(<Findings findings={[]} onRepair={vi.fn()} />);
    expect(screen.getByText("Nothing to report.")).toBeDefined();
  });

  it("offers to repair only what is repairable", () => {
    render(<Findings findings={[broken, yours]} onRepair={vi.fn()} />);
    expect(screen.getByText("Repair 1 finding")).toBeDefined();
  });

  it("does not offer a repair when nothing can be repaired", () => {
    render(<Findings findings={[yours]} onRepair={vi.fn()} />);
    expect(screen.queryByText(/^Repair/)).toBeNull();
  });

  it("shows the consequence, and why an untouched finding was untouched", () => {
    render(<Findings findings={[broken, yours]} onRepair={vi.fn()} />);
    expect(screen.getByText(/sees no skills here/)).toBeDefined();
    expect(screen.getByText(/this is your edit/)).toBeDefined();
  });
});
