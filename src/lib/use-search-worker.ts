import { useCallback, useEffect, useRef, useState } from "react";
import type { SearchMatch } from "./inventory";
import type { WorkerIn, WorkerOut } from "./search-protocol";

type Status = "boot" | "ready" | "error";

/** Search off the main thread. Init with a copy of the ARUN buffer. */
export function useSearchWorker(arun: ArrayBuffer | null) {
  const workerRef = useRef<Worker | null>(null);
  const reqId = useRef(0);
  const [status, setStatus] = useState<Status>("boot");
  const [error, setError] = useState<string | null>(null);
  const [matches, setMatches] = useState<SearchMatch[] | null>(null);

  useEffect(() => {
    if (!arun) return;
    let alive = true;
    const worker = new Worker(new URL("./search.worker.ts", import.meta.url), {
      type: "module",
      name: "tlhdig-search",
    });
    workerRef.current = worker;

    const onMessage = (ev: MessageEvent<WorkerOut>) => {
      if (!alive) return;
      const msg = ev.data;
      if (msg.type === "ready") {
        setStatus("ready");
        return;
      }
      if (msg.type === "result") {
        if (msg.id !== reqId.current) return;
        setMatches(msg.matches);
        return;
      }
      if (msg.type === "error") {
        setStatus("error");
        setError(msg.message);
      }
    };
    const onError = (e: ErrorEvent) => {
      if (!alive) return;
      setStatus("error");
      setError(e.message || "Search worker failed");
    };

    worker.addEventListener("message", onMessage);
    worker.addEventListener("error", onError);

    // Transfer a detachable copy so the main thread's buffer stays valid.
    const copy = arun.slice(0);
    worker.postMessage({ type: "init", arun: copy } satisfies WorkerIn, [copy]);

    return () => {
      alive = false;
      worker.removeEventListener("message", onMessage);
      worker.removeEventListener("error", onError);
      worker.terminate();
      workerRef.current = null;
      setStatus("boot");
      setMatches(null);
    };
  }, [arun]);

  const search = useCallback(
    (q: string) => {
      const trimmed = q.trim().toLowerCase();
      if (!trimmed) {
        reqId.current += 1;
        setMatches(null);
        return;
      }
      const w = workerRef.current;
      if (!w || status !== "ready") return;
      const id = ++reqId.current;
      w.postMessage({ type: "search", id, q: trimmed } satisfies WorkerIn);
    },
    [status],
  );

  return { status, error, matches, search };
}
