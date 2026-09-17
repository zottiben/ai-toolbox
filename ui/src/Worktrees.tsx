import { describeFiles, basename, standingLabel } from "./format";
import type { Comparison, WorktreeState } from "./types";

interface Props {
  comparison: Comparison | null;
  onConverge: (worktree?: string) => void;
}

/**
 * Every worktree, against the main one.
 *
 * The note about git is not decoration. Everything the toolkit writes is in the user's
 * global gitignore, so `git worktree add` brings none of it and a new worktree starts
 * empty - which looks alarming until you know it is the default rather than damage.
 */
export function Worktrees({ comparison, onConverge }: Props) {
  if (!comparison || comparison.worktrees.length === 0) {
    return (
      <section className="panel">
        <h2>Worktrees</h2>
        <p className="muted">Not a git repository, so there is nothing to compare.</p>
      </section>
    );
  }

  const outOfStep = comparison.worktrees.filter(
    (w) => w.standing === "unconfigured" || w.standing === "diverged",
  );

  return (
    <section className="panel">
      <header className="panel-head">
        <h2>Worktrees</h2>
        {outOfStep.length > 0 && (
          <button className="primary" onClick={() => onConverge()}>
            Bring {outOfStep.length} into step
          </button>
        )}
      </header>

      {comparison.worktrees.length === 1 ? (
        <p className="muted small">One worktree, so nothing can be out of step.</p>
      ) : (
        outOfStep.length === 0 && <p className="ok small">Every worktree matches.</p>
      )}

      <ul className="worktrees">
        {comparison.worktrees.map((state) => (
          <Row key={state.worktree.path} state={state} onConverge={onConverge} />
        ))}
      </ul>

      {outOfStep.length > 0 && (
        <p className="muted small">
          Nothing here is tracked by git, so a worktree added after the repo was set up
          starts with none of it. Bringing it into step copies from{" "}
          <code>{basename(comparison.reference)}</code> and never deletes.
        </p>
      )}
    </section>
  );
}

function Row({ state, onConverge }: { state: WorktreeState; onConverge: (path?: string) => void }) {
  const { diff, standing, worktree } = state;
  const label = worktree.branch ?? basename(worktree.path);

  return (
    <li>
      <div className="worktree-head">
        <span className="name">{label}</span>
        <span className={`badge ${tone(standing)}`}>{standingLabel(standing)}</span>
        {(standing === "unconfigured" || standing === "diverged") && (
          <button className="ghost small" onClick={() => onConverge(worktree.path)}>
            Sync
          </button>
        )}
      </div>
      {standing === "diverged" && (
        <ul className="diff">
          {diff.missing.length > 0 && <li>missing {describeFiles(diff.missing)}</li>}
          {diff.different.length > 0 && <li>{describeFiles(diff.different)} differ</li>}
          {diff.extra.length > 0 && (
            <li className="muted">{describeFiles(diff.extra)} only here - left alone</li>
          )}
        </ul>
      )}
      {standing === "unconfigured" && (
        <p className="muted small">Nothing from the toolkit is here yet.</p>
      )}
    </li>
  );
}

function tone(standing: WorktreeState["standing"]): string {
  return {
    reference: "muted",
    "in-step": "ok",
    unconfigured: "warn",
    diverged: "warn",
    unusable: "muted",
  }[standing];
}
