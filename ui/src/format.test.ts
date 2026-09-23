import { describe, expect, it } from "vitest";
import {
  countsLabel,
  describeFiles,
  originLabel,
  originTone,
  problemDetail,
  stateLabel,
  stateTone,
  summarise,
  tilde,
} from "./format";

describe("problemDetail", () => {
  const repo = { path: "/Users/me/src/thing" };

  it("drops the repo path the row already shows", () => {
    expect(
      problemDetail({
        repo,
        problem: "/Users/me/src/thing/.mcp.json: key must be a string at line 6 column 5",
      } as never),
    ).toBe(".mcp.json: key must be a string at line 6 column 5");
  });

  it("leaves a message that is not about a file in the repo alone", () => {
    expect(problemDetail({ repo, problem: "not a git repository" } as never)).toBe(
      "not a git repository",
    );
  });

  it("is null when there is no problem", () => {
    expect(problemDetail({ repo, problem: null } as never)).toBeNull();
  });
});

describe("stateLabel", () => {
  const base = { state: "healthy", exists: true, worktrees_in_step: true } as const;

  it("calls a healthy repo healthy", () => {
    expect(stateLabel(base)).toBe("healthy");
    expect(stateTone(base)).toBe("ok");
  });

  it("does not call a repo healthy when half its branches have no hooks", () => {
    const drifted = { ...base, worktrees_in_step: false };
    expect(stateLabel(drifted)).toBe("worktrees out of step");
    expect(stateTone(drifted)).toBe("warn");
  });

  it("separates a repo nobody set up from one that is broken", () => {
    expect(stateLabel({ ...base, state: "unconfigured" })).toBe("not set up");
    expect(stateTone({ ...base, state: "unconfigured" })).toBe("muted");
    expect(stateTone({ ...base, state: "broken" })).toBe("bad");
  });

  it("says when the directory has gone, whatever the last known state was", () => {
    expect(stateLabel({ ...base, exists: false })).toBe("gone from disk");
  });
});

describe("originLabel", () => {
  it("distinguishes an old copy from somebody's edit", () => {
    // These mean opposite things: one is safe to update, the other is work.
    expect(originLabel({ state: "stale" })).toContain("safe to update");
    expect(originLabel({ state: "modified" })).toBe("edited here");
    expect(originTone({ state: "stale" })).toBe("warn");
    expect(originTone({ state: "modified" })).toBe("warn");
  });

  it("treats a hand-written item as a fact rather than a fault", () => {
    expect(originTone({ state: "local" })).toBe("muted");
  });

  it("repeats the reason a broken item is broken", () => {
    expect(originLabel({ state: "broken", why: "not executable" })).toBe("not executable");
  });
});

describe("describeFiles", () => {
  it("counts items rather than the files they are made of", () => {
    const text = describeFiles([
      ".agents/skills/pre-pr/SKILL.md",
      ".agents/skills/pre-pr/extra.md",
      ".agents/skills/gh/SKILL.md",
      ".agents/hooks/format-on-edit.sh",
      ".mcp.json",
    ]);
    expect(text).toContain("2 skills");
    expect(text).toContain("hook format-on-edit");
    expect(text).toContain(".mcp.json");
    expect(text).not.toContain("SKILL.md");
  });

  it("names a single item instead of counting it", () => {
    expect(describeFiles([".agents/skills/pre-pr/SKILL.md"])).toBe("skill pre-pr");
  });
});

describe("countsLabel", () => {
  it("leaves out what is zero", () => {
    expect(countsLabel({ managed: 11, modified: 0, stale: 1, local: 0, broken: 0 })).toBe(
      "11 managed, 1 stale",
    );
  });

  it("is empty when there is nothing installed", () => {
    expect(countsLabel({ managed: 0, modified: 0, stale: 0, local: 0, broken: 0 })).toBe("");
  });
});

describe("summarise", () => {
  it("keeps the first sentence, which is the part that says what it is", () => {
    const skill =
      "Capture a just-learned project gotcha into AGENTS.md. Use when the user corrects you, or when a non-obvious footgun surfaces mid-task.";
    expect(summarise(skill)).toBe("Capture a just-learned project gotcha into AGENTS.md.");
  });

  it("splits on a dash too, which is how most of these are written", () => {
    expect(summarise("Operate GitHub via the gh CLI - PRs, issues, CI checks, runs")).toBe(
      "Operate GitHub via the gh CLI",
    );
  });

  it("breaks a long first sentence on a word", () => {
    const long = `${"alpha bravo charlie delta echo foxtrot ".repeat(6)}end.`;
    const short = summarise(long);
    expect(short?.endsWith("\u2026")).toBe(true);
    expect(short?.length).toBeLessThan(120);
    expect(short).not.toContain("  ");
  });

  it("leaves a short description alone and passes null through", () => {
    expect(summarise("Short one.")).toBe("Short one.");
    expect(summarise(null)).toBeNull();
  });
});

describe("tilde", () => {
  it("shortens a home path the way people say it", () => {
    expect(tilde("/Users/ben/src/thing")).toBe("~/src/thing");
    expect(tilde("/home/ben/src/thing")).toBe("~/src/thing");
  });

  it("leaves a path outside home alone", () => {
    expect(tilde("/opt/work/thing")).toBe("/opt/work/thing");
  });
});
