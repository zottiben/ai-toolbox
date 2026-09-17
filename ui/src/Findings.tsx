import type { Finding } from "./types";
import { pluralise } from "./format";

interface Props {
  findings: Finding[];
  onRepair: () => void;
}

/**
 * What is wrong, worst first.
 *
 * Each line is the consequence rather than the symptom, because that is what the server
 * sends - "Claude Code sees no skills here", not "symlink does not resolve". The ones
 * `--fix` will not touch say why, so nobody is left wondering whether Repair missed them.
 */
export function Findings({ findings, onRepair }: Props) {
  if (findings.length === 0) {
    return (
      <section className="panel">
        <h2>Health</h2>
        <p className="ok">Nothing to report.</p>
      </section>
    );
  }

  const repairable = findings.filter((f) => f.repairable).length;

  return (
    <section className="panel">
      <header className="panel-head">
        <h2>Health</h2>
        {repairable > 0 && (
          <button className="primary" onClick={onRepair}>
            Repair {pluralise(repairable, "finding")}
          </button>
        )}
      </header>

      <ul className="findings">
        {findings.map((finding) => (
          <li key={finding.code + finding.path} className={finding.severity}>
            <span className={`badge ${tone(finding.severity)}`}>{finding.severity}</span>
            <div>
              <p>{finding.what}</p>
              {finding.advice && <p className="muted small">{finding.advice}</p>}
              {!finding.repairable && !finding.advice && (
                <p className="muted small">Left alone - Repair will not touch this.</p>
              )}
            </div>
          </li>
        ))}
      </ul>
    </section>
  );
}

function tone(severity: Finding["severity"]): string {
  return { broken: "bad", warning: "warn", note: "muted" }[severity];
}
