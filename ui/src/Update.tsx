import { useState } from "react";
import { createPortal } from "react-dom";
import { api } from "./api";
import type { UpdateStatus, Updated } from "./types";

/** Checking is explicit. Opening the board does not contact GitHub or install anything. */
export function Update() {
  const [status, setStatus] = useState<UpdateStatus | null>(null);
  const [installed, setInstalled] = useState<Updated | null>(null);
  const [busy, setBusy] = useState<"checking" | "installing" | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [open, setOpen] = useState(false);

  async function check() {
    setBusy("checking");
    setError(null);
    try {
      const result = await api.checkUpdate();
      setStatus(result);
      setInstalled(result.installed);
      setOpen(true);
    } catch (err) {
      setError((err as Error).message);
    } finally {
      setBusy(null);
    }
  }

  async function install() {
    if (!status) return;
    setBusy("installing");
    setError(null);
    try {
      setInstalled(await api.update(status.tag));
    } catch (err) {
      setError((err as Error).message);
    } finally {
      setBusy(null);
    }
  }

  return (
    <section className="update-panel" aria-label="ai-toolbox updates">
      <button className="ghost small" disabled={busy !== null} onClick={installed ? () => setOpen(true) : check}>
        {busy === "checking" ? "Checking…" : installed ? "Update installed — restart needed" : "Check for updates"}
      </button>
      {error && !open && <p className="bad small" role="alert">{error}</p>}
      {open && status && createPortal(
        <div className="scrim">
          <div className="dialog" role="dialog" aria-modal="true" aria-label="Update ai-toolbox">
            <header>
              <h2>Update ai-toolbox</h2>
              <button className="ghost" aria-label="Close" disabled={busy !== null} onClick={() => setOpen(false)}>✕</button>
            </header>
            <div className="dialog-body update-body">
              <p>Running {status.current} · latest stable {status.latest}</p>
              {error && <p className="bad" role="alert">{error}</p>}
              {installed ? (
                <>
                  <p className="ok" role="status">Installed ai-toolbox {installed.version}.</p>
                  <p>Quit and reopen the desktop app, or stop and restart <code>ai-toolbox ui</code>. Reloading this page alone will not load the new version.</p>
                  <details><summary>Installer output</summary><pre>{installed.output}</pre></details>
                </>
              ) : status.available ? (
                <>
                  <p>A new release is available. Download and verify it, then replace:</p>
                  <ul className="actions">
                    {status.installation.binary && <li>{status.installation.binary}</li>}
                    {status.installation.app && <li>{status.installation.app}</li>}
                    {status.installation.catalogue && <li>Fast-forward catalogue: {status.installation.catalogue} (local edits preserved)</li>}
                  </ul>
                  {status.installation.notes.map((note) => <p className="muted small" key={note}>{note}</p>)}
                  <p className="small">Restart the application after the update finishes.</p>
                  {status.installation.blocked && <p className="warn">{status.installation.blocked}</p>}
                  <a href={status.release_url} target="_blank" rel="noreferrer">Release notes</a>
                </>
              ) : <p className="ok">No newer release available.</p>}
              {busy === "installing" && <p role="status">Downloading and installing… Keep this window open.</p>}
            </div>
            <footer>
              <button className="ghost" disabled={busy !== null} onClick={() => setOpen(false)}>{installed ? "Close" : "Cancel"}</button>
              {!installed && status.available && !status.installation.blocked && (
                <button className="primary" disabled={busy !== null} onClick={install}>
                  {busy === "installing" ? "Installing…" : `Install ${status.latest}`}
                </button>
              )}
            </footer>
          </div>
        </div>, document.body,
      )}
    </section>
  );
}
