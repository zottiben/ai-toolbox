import { useMemo, useState } from "react";
import { api } from "./api";
import {
  basename,
  countsLabel,
  pluralise,
  problemDetail,
  stateLabel,
  stateTone,
  tilde,
} from "./format";
import type { State, Summary } from "./types";

interface Props {
  summaries: Summary[];
  selected: number | null;
  onSelect: (id: number) => void;
  onScanned: () => void;
}

const FILTERS: { key: State | "all"; label: string }[] = [
  { key: "all", label: "All" },
  { key: "broken", label: "Broken" },
  { key: "attention", label: "Needs a look" },
  { key: "healthy", label: "Healthy" },
  { key: "unconfigured", label: "Not set up" },
];

export function ProjectList({ summaries, selected, onSelect, onScanned }: Props) {
  const [filter, setFilter] = useState<State | "all">("all");
  const [scanning, setScanning] = useState(false);
  const [message, setMessage] = useState<string | null>(null);

  const counts = useMemo(() => {
    const out = new Map<State | "all", number>([["all", summaries.length]]);
    for (const summary of summaries) {
      out.set(summary.state, (out.get(summary.state) ?? 0) + 1);
    }
    return out;
  }, [summaries]);

  const shown = summaries.filter((s) => filter === "all" || s.state === filter);

  async function scan() {
    setScanning(true);
    setMessage(null);
    try {
      const result = await api.scan();
      setMessage(
        result.added.length
          ? `Found ${pluralise(result.added.length, "new repo")}.`
          : `Nothing new - ${pluralise(result.known.length, "repo")} already known.`,
      );
      onScanned();
    } catch (err) {
      setMessage((err as Error).message);
    } finally {
      setScanning(false);
    }
  }

  return (
    <aside className="sidebar">
      <header className="sidebar-head">
        <h1>ai-toolbox</h1>
        <button className="ghost" onClick={scan} disabled={scanning}>
          {scanning ? "Scanning…" : "Scan"}
        </button>
      </header>

      <nav className="filters">
        {FILTERS.map(({ key, label }) => (
          <button
            key={key}
            className={key === filter ? "chip on" : "chip"}
            onClick={() => setFilter(key)}
            disabled={key !== "all" && !counts.get(key)}
          >
            {label}
            <span className="count">{counts.get(key) ?? 0}</span>
          </button>
        ))}
      </nav>

      {message && <p className="small muted pad">{message}</p>}

      <ul className="projects">
        {shown.map((summary) => (
          <li key={summary.repo.id}>
            <button
              className={summary.repo.id === selected ? "project on" : "project"}
              onClick={() => onSelect(summary.repo.id)}
            >
              <span className="name">{basename(summary.repo.path)}</span>
              <span className={`badge ${stateTone(summary)}`}>{stateLabel(summary)}</span>
              <span className="path">{tilde(summary.repo.path)}</span>
              <span className="meta">
                {problemDetail(summary) ?? (
                  <>
                    {summary.harnesses.join(" + ") || "no harness"}
                    {countsLabel(summary.counts) && ` · ${countsLabel(summary.counts)}`}
                    {summary.worktrees > 1 && ` · ${pluralise(summary.worktrees, "worktree")}`}
                  </>
                )}
              </span>
            </button>
          </li>
        ))}
        {shown.length === 0 && (
          <li className="pad muted small">
            {summaries.length === 0
              ? "No projects yet. Scan to find the repos on this machine."
              : "Nothing in this filter."}
          </li>
        )}
      </ul>
    </aside>
  );
}
