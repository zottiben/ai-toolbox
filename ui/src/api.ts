import type {
  Applied,
  Catalogue,
  Detail,
  MachineInfo,
  PlanView,
  Request,
  ScanResult,
  Summary,
} from "./types";

// The token arrives in the URL query, because a freshly opened tab has no other channel.
// It is moved into a header for every call after that, so it stops appearing in the
// address bar and in anything that logs a URL.
const token = new URLSearchParams(window.location.search).get("t") ?? "";

if (token && window.history.replaceState) {
  const clean = new URL(window.location.href);
  clean.searchParams.delete("t");
  window.history.replaceState({}, "", clean.toString());
}

export function streamUrl(project?: number): string {
  const params = new URLSearchParams({ t: token });
  if (project !== undefined) params.set("project", String(project));
  return `/api/events?${params}`;
}

export class ApiError extends Error {
  constructor(
    message: string,
    readonly code: string,
    readonly status: number,
  ) {
    super(message);
  }
}

async function call<T>(path: string, init?: RequestInit): Promise<T> {
  const response = await fetch(`/api${path}`, {
    ...init,
    headers: {
      "x-toolbox-token": token,
      ...(init?.body ? { "content-type": "application/json" } : {}),
      ...init?.headers,
    },
  });
  if (!response.ok) {
    // The server sends a structured error; fall back to the status only when it did not,
    // which means something failed before any handler ran.
    const body = await response.json().catch(() => null);
    throw new ApiError(
      body?.error ?? `${response.status} ${response.statusText}`,
      body?.code ?? "unknown",
      response.status,
    );
  }
  return (await response.json()) as T;
}

const post = <T,>(path: string, body: unknown): Promise<T> =>
  call<T>(path, { method: "POST", body: JSON.stringify(body) });

export const api = {
  machine: () => call<MachineInfo>("/machine"),
  catalogue: () => call<Catalogue>("/catalogue"),
  projects: () => call<Summary[]>("/projects"),
  project: (id: number) => call<Detail>(`/projects/${id}`),

  scan: (roots: string[] = []) => post<ScanResult>("/projects/scan", { roots }),
  add: (path: string) => post<{ id: number }>("/projects/add", { path }),
  forget: (id: number) => post<boolean>(`/projects/${id}/forget`, {}),

  /** Always called before `apply`, so the board can show what it is about to do (D3). */
  plan: (id: number, request: Request) => post<PlanView>(`/projects/${id}/plan`, request),
  apply: (id: number, request: Request) => post<Applied>(`/projects/${id}/apply`, request),
};
