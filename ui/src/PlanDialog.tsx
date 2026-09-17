import { useEffect, useState } from "react";
import { api, ApiError } from "./api";
import type { Applied, PlanView, Request } from "./types";
import { pluralise } from "./format";

interface Props {
  project: number;
  request: Request;
  title: string;
  onClose: () => void;
  onApplied: (applied: Applied) => void;
}

/**
 * Plan, show, then apply (D3).
 *
 * Nothing in the board writes without this. A button that says "Repair (4 changes)" and
 * lists them before it touches anything is a different product from one that says
 * "Repair" and hopes - and it is the only way a person can tell that a repair is about to
 * replace a file they edited.
 */
export function PlanDialog({
  project,
  request,
  title,
  onClose,
  onApplied,
}: Props) {
  const [plan, setPlan] = useState<PlanView | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [applying, setApplying] = useState(false);

  useEffect(() => {
    let live = true;
    api
      .plan(project, request)
      .then((result) => live && setPlan(result))
      .catch((err: ApiError) => live && setError(err.message));
    return () => {
      live = false;
    };
    // The request is rebuilt by the parent on every render, so its identity is not a
    // dependency worth tracking - the dialog is opened once per action.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [project]);

  async function apply() {
    setApplying(true);
    try {
      onApplied(await api.apply(project, request));
      onClose();
    } catch (err) {
      setError((err as Error).message);
      setApplying(false);
    }
  }

  const changes = plan?.actions.filter((a) => !a.noop) ?? [];
  const unchanged = plan?.actions.filter((a) => a.noop) ?? [];

  return (
    <div className="scrim" onClick={onClose} role="presentation">
      <div
        className="dialog"
        onClick={(e) => e.stopPropagation()}
        role="dialog"
        aria-label={title}
      >
        <header>
          <h2>{title}</h2>
          <button className="ghost" onClick={onClose} aria-label="Close">
            ✕
          </button>
        </header>

        {error && <p className="bad">{error}</p>}
        {!plan && !error && (
          <p className="muted">Working out what would change…</p>
        )}

        {plan && (
          <>
            {/* Only the list scrolls. A plan can be forty lines long, and the button
                that applies it must not disappear below them. */}
            <div className="dialog-body">
              {changes.length === 0 ? (
                <p className="muted">Everything here is already in place.</p>
              ) : (
                <ul className="actions">
                  {changes.map((action) => (
                    <li key={action.path + action.summary}>{action.summary}</li>
                  ))}
                </ul>
              )}

              {unchanged.length > 0 && (
                <p className="muted small">
                  {pluralise(unchanged.length, "item")} already in place.
                </p>
              )}

              {plan.warnings.map((warning) => (
                <p className="warn small" key={warning}>
                  {warning}
                </p>
              ))}

              {plan.secrets.length > 0 && (
                <div className="secrets">
                  <h3>Secrets these need</h3>
                  {plan.secrets.map((secret) => (
                    <p key={secret.name} className="small">
                      <code>{secret.name}</code>{" "}
                      <span className="muted">
                        {secret.source === "environment"
                          ? "export it before starting the harness"
                          : "put it in the repo's .env - no export needed"}
                      </span>
                    </p>
                  ))}
                </div>
              )}
            </div>

            <footer>
              <button className="ghost" onClick={onClose}>
                Cancel
              </button>
              <button
                className="primary"
                onClick={apply}
                disabled={applying || changes.length === 0}
              >
                {applying
                  ? "Applying…"
                  : `Apply ${pluralise(changes.length, "change")}`}
              </button>
            </footer>
          </>
        )}
      </div>
    </div>
  );
}
