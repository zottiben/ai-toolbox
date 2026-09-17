import { useCallback, useEffect, useRef, useState } from "react";
import { streamUrl } from "./api";

export interface Loaded<T> {
  data: T | null;
  error: string | null;
  loading: boolean;
  reload: () => void;
}

/**
 * Fetch something, and expose a way to fetch it again.
 *
 * The stale-response guard matters here: clicking through three projects quickly fires
 * three requests, and without it the slowest one wins and the page shows a project you
 * have already navigated away from.
 */
export function useLoad<T>(load: () => Promise<T>, deps: unknown[]): Loaded<T> {
  const [data, setData] = useState<T | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [nonce, setNonce] = useState(0);
  const generation = useRef(0);

  useEffect(() => {
    const mine = ++generation.current;
    setLoading(true);
    load()
      .then((result) => {
        if (generation.current !== mine) return;
        setData(result);
        setError(null);
      })
      .catch((err: Error) => {
        if (generation.current !== mine) return;
        setError(err.message);
      })
      .finally(() => {
        if (generation.current === mine) setLoading(false);
      });
    // `load` is rebuilt every render, so the caller's deps are the real dependency list.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [...deps, nonce]);

  const reload = useCallback(() => setNonce((n) => n + 1), []);
  return { data, error, loading, reload };
}

/**
 * Re-run `onChange` when the server says a repo moved.
 *
 * The server polls only the project this stream names, so opening one is what starts it
 * being watched and closing the tab is what stops it.
 */
export function useLiveRefresh(project: number | undefined, onChange: () => void): void {
  const handler = useRef(onChange);
  handler.current = onChange;

  useEffect(() => {
    const source = new EventSource(streamUrl(project));
    const listener = () => handler.current();
    source.addEventListener("changed", listener);
    // EventSource reconnects on its own; this only stops the noise in the console.
    source.onerror = () => {};
    return () => {
      source.removeEventListener("changed", listener);
      source.close();
    };
  }, [project]);
}
