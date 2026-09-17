import { useState } from "react";
import { api } from "./api";
import { Catalogue } from "./Catalogue";
import { Findings } from "./Findings";
import { Installed } from "./Installed";
import { PlanDialog } from "./PlanDialog";
import { Worktrees } from "./Worktrees";
import { basename, stateLabel, stateTone, tilde } from "./format";
import { useLoad } from "./hooks";
import type { Catalogue as CatalogueData, Detail, Request } from "./types";

interface Props {
  id: number;
  detail: Detail;
  catalogue: CatalogueData;
  onChanged: () => void;
  onForgotten: () => void;
}

interface Pending {
  request: Request;
  title: string;
}

export function Project({ id, detail, catalogue, onChanged, onForgotten }: Props) {
  const [pending, setPending] = useState<Pending | null>(null);
  const [note, setNote] = useState<string | null>(null);

  if (!detail.exists) {
    return (
      <main className="detail">
        <header className="detail-head">
          <h1>{basename(detail.repo.path)}</h1>
          <span className="badge muted">gone from disk</span>
        </header>
        <p className="muted">{tilde(detail.repo.path)} is no longer there.</p>
        <button
          className="ghost"
          onClick={async () => {
            await api.forget(id);
            onForgotten();
          }}
        >
          Forget it
        </button>
      </main>
    );
  }

  const summary = {
    state: detail.state ?? "unconfigured",
    exists: true,
    worktrees_in_step: !detail.worktrees
      ? true
      : detail.worktrees.worktrees.every(
          (w) => w.standing !== "diverged" && w.standing !== "unconfigured",
        ),
  };

  const ask = (request: Request, title: string) => setPending({ request, title });

  return (
    <main className="detail">
      <header className="detail-head">
        <div>
          <h1>{basename(detail.repo.path)}</h1>
          <p className="path">{tilde(detail.repo.path)}</p>
        </div>
        <span className={`badge ${stateTone(summary)}`}>{stateLabel(summary)}</span>
      </header>

      {note && <p className="note">{note}</p>}

      {detail.state === "unconfigured" && detail.recommendation && (
        <section className="panel highlight">
          <h2>Set this up</h2>
          <p className="muted">
            {detail.recommendation.detected.length > 0
              ? `Looks like ${detail.recommendation.detected.join(", ")}.`
              : "No stack detected, so this is the generic set."}{" "}
            That calls for {detail.recommendation.hooks.length} hooks,{" "}
            {detail.recommendation.mcp.length} MCP servers and{" "}
            {detail.recommendation.skills.length} skills.
          </p>
          <button
            className="primary"
            onClick={() =>
              ask(
                { kind: "recommended", scaffold: true },
                "Install what this stack calls for",
              )
            }
          >
            Install the recommended set
          </button>
        </section>
      )}

      <Findings
        findings={detail.findings}
        onRepair={() => ask({ kind: "repair" }, "Repair what can be repaired")}
      />

      <Worktrees
        comparison={detail.worktrees}
        onConverge={(worktree) =>
          ask(
            { kind: "converge", worktree },
            worktree ? "Bring this worktree into step" : "Bring every worktree into step",
          )
        }
      />

      <Installed items={detail.items ?? []} />

      {detail.available && (
        <Catalogue
          catalogue={catalogue}
          available={detail.available}
          harnesses={detail.harnesses ?? []}
          onInstall={ask}
        />
      )}

      {pending && (
        <PlanDialog
          project={id}
          request={pending.request}
          title={pending.title}
          onClose={() => setPending(null)}
          onApplied={(applied) => {
            setNote(
              applied.stale.length > 0
                ? `Applied ${applied.applied}. ${applied.stale.length} file(s) changed while this was being prepared and were left alone.`
                : `Applied ${applied.applied} change(s).`,
            );
            onChanged();
          }}
        />
      )}
    </main>
  );
}

/** Loads a project and keeps it fresh. Split out so `App` stays about layout. */
export function useProject(id: number | null) {
  return useLoad<Detail | null>(
    () => (id === null ? Promise.resolve(null) : api.project(id)),
    [id],
  );
}
