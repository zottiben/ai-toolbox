import { useState } from "react";
import { summarise } from "./format";
import type { Available, Catalogue as CatalogueData, Harness, Request } from "./types";

interface Props {
  catalogue: CatalogueData;
  available: Available;
  harnesses: Harness[];
  onInstall: (request: Request, title: string) => void;
}

const ALL: Harness[] = ["claude", "codex", "pi"];

/**
 * What this repo could add.
 *
 * Only things it does not already have are offered - what is installed is shown by
 * `Installed`, and listing every item twice would make the useful half harder to find.
 */
export function Catalogue({ catalogue, available, harnesses, onInstall }: Props) {
  const [chosen, setChosen] = useState<Set<string>>(new Set());
  // Defaults to the harnesses this repo already uses, which is nearly always right.
  const [targets, setTargets] = useState<Harness[]>(harnesses.length ? harnesses : ["claude"]);

  const toggle = (key: string) =>
    setChosen((current) => {
      const next = new Set(current);
      if (!next.delete(key)) next.add(key);
      return next;
    });

  const toggleHarness = (harness: Harness) =>
    setTargets((current) =>
      current.includes(harness)
        ? current.filter((h) => h !== harness)
        : [...current, harness],
    );

  function install() {
    const request: Request = {
      kind: "install",
      hooks: [...chosen].filter((k) => k.startsWith("hook:")).map((k) => k.slice(5)),
      presets: [...chosen].filter((k) => k.startsWith("preset:")).map((k) => k.slice(7)),
      skills: [...chosen].filter((k) => k.startsWith("skill:")).map((k) => k.slice(6)),
      harnesses: targets,
    };
    onInstall(request, `Install ${chosen.size} item(s)`);
    setChosen(new Set());
  }

  const hooks = catalogue.hooks.filter((h) => available.hooks.includes(h.name));
  const presets = catalogue.presets.filter((p) => available.presets.includes(p.name));
  const skills = catalogue.skills.filter((s) => available.skills.includes(s.key));
  const nothingLeft = !hooks.length && !presets.length && !skills.length;

  return (
    <section className="panel">
      <header className="panel-head">
        <h2>Add from the catalogue</h2>
        {chosen.size > 0 && (
          <button className="primary" onClick={install}>
            Install {chosen.size}
          </button>
        )}
      </header>

      {nothingLeft ? (
        <p className="muted">Everything in the catalogue is already installed here.</p>
      ) : (
        <>
          <div className="harness-picker">
            <span className="muted small">for</span>
            {ALL.map((harness) => (
              <button
                key={harness}
                className={targets.includes(harness) ? "chip on" : "chip"}
                onClick={() => toggleHarness(harness)}
              >
                {harness}
              </button>
            ))}
          </div>

          <Group title="Hooks">
            {hooks.map((hook) => (
              <Pick
                key={hook.name}
                id={`hook:${hook.name}`}
                name={hook.name}
                description={hook.summary}
                chosen={chosen}
                onToggle={toggle}
              />
            ))}
          </Group>

          <Group title="MCP servers">
            {presets.map((preset) => (
              <Pick
                key={preset.name}
                id={`preset:${preset.name}`}
                name={preset.name}
                description={describePreset(preset.servers, preset.name)}
                chosen={chosen}
                onToggle={toggle}
                secrets={preset.secrets.map((s) => s.name)}
              />
            ))}
          </Group>

          <Group title="Skills">
            {skills.map((skill) => (
              <Pick
                key={skill.key}
                id={`skill:${skill.key}`}
                name={skill.key}
                description={skill.description}
                chosen={chosen}
                onToggle={toggle}
              />
            ))}
          </Group>
        </>
      )}
    </section>
  );
}

function Group({ title, children }: { title: string; children: React.ReactNode[] }) {
  if (children.length === 0) return null;
  return (
    <div className="group">
      <h3>{title}</h3>
      <ul className="items pickable">{children}</ul>
    </div>
  );
}

function Pick({
  id,
  name,
  description,
  chosen,
  onToggle,
  secrets = [],
}: {
  id: string;
  name: string;
  description: string | null;
  chosen: Set<string>;
  onToggle: (id: string) => void;
  secrets?: string[];
}) {
  const on = chosen.has(id);
  return (
    <li>
      <label className={on ? "pick on" : "pick"}>
        <input type="checkbox" checked={on} onChange={() => onToggle(id)} />
        <span className="name">{name}</span>
        {description && (
          <span className="muted small desc" title={description}>
            {summarise(description)}
          </span>
        )}
        {secrets.length > 0 && (
          <span className="muted small">needs {secrets.join(", ")}</span>
        )}
      </label>
    </li>
  );
}

/** A preset does not always install a server of its own name - say so when it does not. */
function describePreset(servers: Record<string, unknown>, name: string): string | null {
  const names = Object.keys(servers);
  if (names.length === 1 && names[0] === name) return null;
  return names.join(", ");
}
