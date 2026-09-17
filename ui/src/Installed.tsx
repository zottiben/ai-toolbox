import { originLabel, originTone, summarise } from "./format";
import type { Item, ItemKind } from "./types";

interface Props {
  items: Item[];
}

const GROUPS: { kind: ItemKind; title: string }[] = [
  { kind: "hook", title: "Hooks" },
  { kind: "skill", title: "Skills" },
  { kind: "server", title: "MCP servers" },
  { kind: "helper", title: "Launchers" },
];

/**
 * What this repo has, and where each piece came from.
 *
 * `local` is shown as plainly as `managed`, because a hand-written skill is a thing
 * people have rather than a fault - the board should name it, not flag it.
 */
export function Installed({ items }: Props) {
  if (items.length === 0) {
    return (
      <section className="panel">
        <h2>Installed</h2>
        <p className="muted">Nothing from the toolkit is here yet.</p>
      </section>
    );
  }

  return (
    <section className="panel">
      <h2>Installed</h2>
      {GROUPS.map(({ kind, title }) => {
        const group = items.filter((item) => item.kind === kind);
        if (group.length === 0) return null;
        return (
          <div key={kind} className="group">
            <h3>{title}</h3>
            <ul className="items">
              {group.map((item) => (
                <li key={`${item.kind}-${item.name}`}>
                  <span className="name">{item.name}</span>
                  <span className={`badge ${originTone(item.origin)}`}>
                    {item.origin.state}
                  </span>
                  <span className="muted small">{originLabel(item.origin)}</span>
                  {item.description && (
                    <p className="muted small desc" title={item.description}>
                      {summarise(item.description)}
                    </p>
                  )}
                </li>
              ))}
            </ul>
          </div>
        );
      })}
    </section>
  );
}
