import { useCallback, useEffect, useState } from "react";
import { api } from "./api";
import { Project, useProject } from "./Project";
import { ProjectList } from "./ProjectList";
import { useLiveRefresh, useLoad } from "./hooks";

export function App() {
  const [selected, setSelected] = useState<number | null>(initialSelection());

  const catalogue = useLoad(() => api.catalogue(), []);
  const projects = useLoad(() => api.projects(), []);
  const project = useProject(selected);

  // Reloading both is what makes a change in one place show up in the other: installing
  // a hook changes the project *and* its line in the list.
  const refresh = useCallback(() => {
    projects.reload();
    project.reload();
    // The two reload callbacks are stable, so this never re-fires on its own.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [projects.reload, project.reload]);

  // The server watches only the project this stream names, so opening one is what starts
  // it being watched - and a repo an agent edits in a terminal updates here on its own.
  useLiveRefresh(selected ?? undefined, refresh);

  useEffect(() => {
    const url = new URL(window.location.href);
    if (selected === null) url.searchParams.delete("project");
    else url.searchParams.set("project", String(selected));
    window.history.replaceState({}, "", url.toString());
  }, [selected]);

  if (catalogue.error || projects.error) {
    return (
      <div className="fatal">
        <h1>The board could not start</h1>
        <p>{catalogue.error ?? projects.error}</p>
      </div>
    );
  }

  return (
    <div className="app">
      <ProjectList
        summaries={projects.data ?? []}
        selected={selected}
        onSelect={setSelected}
        onScanned={projects.reload}
      />

      {selected !== null && project.data && catalogue.data ? (
        <Project
          id={selected}
          detail={project.data}
          catalogue={catalogue.data}
          onChanged={refresh}
          onForgotten={() => {
            setSelected(null);
            projects.reload();
          }}
        />
      ) : (
        <main className="detail empty">
          {projects.loading ? (
            <p className="muted">Reading the repos on this machine…</p>
          ) : (
            <>
              <h1>Pick a project</h1>
              <p className="muted">
                Everything here is read off disk when you ask for it, so what you see is
                what is actually installed - not what was installed when something last
                cached it.
              </p>
            </>
          )}
        </main>
      )}
    </div>
  );
}

/** A project in the URL survives a reload, which is the point of it having one. */
function initialSelection(): number | null {
  const raw = new URLSearchParams(window.location.search).get("project");
  const id = raw === null ? Number.NaN : Number(raw);
  return Number.isInteger(id) ? id : null;
}
