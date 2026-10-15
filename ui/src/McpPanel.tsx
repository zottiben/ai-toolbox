import { useEffect, useState } from "react";
import { api } from "./api";
import type { McpApplied, McpOptions, McpPreview, McpReport } from "./types";

export function McpPanel({ id, onChanged }: { id: number; onChanged: () => void }) {
  const [report, setReport] = useState<McpReport | null>(null);
  const [options, setOptions] = useState<McpOptions>({ scope: "project", keep_direct: [], keep_tools: {} });
  const [preview, setPreview] = useState<McpPreview | null>(null);
  const [applied, setApplied] = useState<McpApplied | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [revision, refresh] = useState(0);
  useEffect(() => {
    let active = true;
    api.mcpReport(id).then((data) => { if (active) setReport(data); }).catch((e: Error) => { if (active) setError(e.message); });
    return () => { active = false; };
  }, [id, revision]);
  function choose(next: McpOptions) { setOptions(next); setPreview(null); }
  async function run(action: () => Promise<void>) {
    setBusy(true); setError(null);
    try { await action(); } catch (e) { setError(e instanceof Error ? e.message : String(e)); }
    finally { setBusy(false); }
  }
  return <section className="panel" aria-label="MCP optimisation">
    <h2>MCP exposure</h2>
    <p className="muted">Offline inspection. No servers, models or credential commands are started.</p>
    {error && <p role="alert" className="note">{error}</p>}
    {report ? <>
      {!report.complete && <p className="note">Partial inventory: unresolved configuration is listed below. These counts are not a complete total.</p>}
      <p>{report.backend === "native" ? "Native Pi" : "Pi MCP adapter"} · {report.known_direct_tools} known configured eager tools · {report.unknown_catalogues} unknown catalogues</p>
      <p className="muted">Counts describe file configuration before Pi trust and session overrides, not live availability. Changed project definitions require reapproval; headless/RPC sessions may block those tools until approved interactively.</p>
      {report.warnings.map((warning) => <p key={warning} className="note">{warning}</p>)}
      <fieldset disabled={busy}>
        <legend>Keep these servers or tools always loaded</legend>
        {report.servers.map((server) => <details key={server.name}>
          <summary>{server.name} — {server.exposure}, {server.direct_tools ?? "unknown"} eager tools</summary>
          <p className="path">Exposure from {server.exposure_source}</p>
          <p className="muted">{server.cache_note}</p>
          <p className="path">Layers: {server.sources.join(" → ")}</p>
          <label><input type="checkbox" checked={options.keep_direct.includes(server.name)}
            onChange={(event) => choose({ ...options, keep_direct: event.target.checked ? [...options.keep_direct, server.name] : options.keep_direct.filter((name) => name !== server.name) })} /> Keep {server.name} unchanged</label>
          {server.tool_names.length > 0 && <details><summary>Choose individual cached tools</summary>
            {server.tool_names.map((tool) => <label key={tool} style={{ display: "block" }}><input type="checkbox"
              disabled={options.keep_direct.includes(server.name)}
              checked={options.keep_tools[server.name]?.includes(tool) ?? false}
              onChange={(event) => {
                const selected = options.keep_tools[server.name] ?? [];
                const next = event.target.checked ? [...selected, tool] : selected.filter((name) => name !== tool);
                const keep_tools = { ...options.keep_tools };
                if (next.length) keep_tools[server.name] = next; else delete keep_tools[server.name];
                choose({ ...options, keep_tools });
              }} />{tool}</label>)}
          </details>}
        </details>)}
        <label>Scope <select value={options.scope} onChange={(event) => {
          const scope = event.target.value;
          if (scope === "project" || scope === "worktrees" || scope === "all" || scope === "machine") choose({ ...options, scope });
        }}>
          <option value="project">This project only</option>
          <option value="worktrees">This project and all registered Git worktrees</option>
          <option value="all">All scanned/registered projects and their worktrees</option>
          <option value="machine">Global Pi configuration + all scanned projects/worktrees</option>
        </select></label>
        <p className="muted">Project scopes edit project files, which may be Git-tracked; each selected worktree can become dirty. Scan projects first to include repositories outside the registry.</p>
        <button className="ghost" onClick={() => run(async () => { setPreview(await api.mcpPreview(id, options)); })}>Preview optimisation</button>
      </fieldset>
    </> : <p className="muted">Reading configuration…</p>}
    {preview && <section aria-label="Exact MCP preview">
      <h3>{preview.changes.length} exposure changes across {preview.reports.length} scopes</h3>
      {preview.changes.map((change) => <p key={`${change.path}:${change.server}`}><code>{change.path}</code> · {change.server}: {JSON.stringify(change.before)} → {JSON.stringify(change.after)}</p>)}
      {preview.warnings.map((warning) => <p key={warning} className="note">{warning}</p>)}
      <p>Credentials, permission policy, filters and lifecycle remain unchanged. Reapprove changed adapter definitions interactively before headless/RPC use. Originals are saved in a private backup; later edits block rollback.</p>
      <button className="primary" disabled={busy || preview.changes.length === 0} onClick={() => run(async () => {
        setApplied(await api.mcpApply(preview.id)); setPreview(null); refresh((n) => n + 1); onChanged();
      })}>Apply this exact preview</button>{" "}
      <button className="ghost" disabled={busy} onClick={() => setPreview(null)}>Cancel preview</button>
    </section>}
    {applied && <section aria-label="MCP backup">
      <p>Updated {applied.files} file(s). Reload/restart Pi and reapprove changed adapter definitions interactively before headless/RPC use.</p>
      <p className="path">Private backup: {applied.backup}</p>
      <button className="ghost" disabled={busy} onClick={() => run(async () => {
        await api.mcpRollback(applied.id); setApplied(null); setPreview(null); refresh((n) => n + 1); onChanged();
      })}>Roll back this optimisation</button>
    </section>}
  </section>;
}
