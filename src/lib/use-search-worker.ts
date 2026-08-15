import { useCallback, useEffect, useRef, useState } from "react";
import type { SearchMatch } from "./inventory";
import type { WorkerIn, WorkerOut } from "./search-protocol";

type Status = "boot" | "ready" | "error";

/** Which engine answered: the WASM index, or the JavaScript scan behind it. */
type Engine = "wasm" | "js";

/** Search off the main thread. Init with a copy of the ARUN buffer. */
export function useSearchWorker(arun: ArrayBuffer | null) {
  const workerRef = useRef<Worker | null>(null);
  const reqId = useRef(0);
  /** Query of the request currently in flight — the one `reqId` refers to. */
  const sentQuery = useRef("");
  const [status, setStatus] = useState<Status>("boot");
  const [engine, setEngine] = useState<Engine | null>(null);
  const [error, setError] = useState<string | null>(null);
  // A result carries the query it answers. Matches alone cannot say whether
  // they are still current, so a caller holding them would go on drawing the
  // previous query's hits for as long as the next search is in flight.
  const [result, setResult] = useState<{
    query: string;
    matches: SearchMatch[];
  } | null>(null);

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
        // Which engine the worker settled on. The message has carried this
        // since the fallback existed and nothing read it, so the page could not
        // tell a fast search from a slow one — see `engine` in the return value.
        setEngine(msg.engine);
        return;
      }
      if (msg.type === "result") {
        if (msg.id !== reqId.current) return;
        setResult({ query: sentQuery.current, matches: msg.matches });
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
      setEngine(null);
      setResult(null);
    };
  }, [arun]);

  const search = useCallback(
    (q: string) => {
      const trimmed = q.trim().toLowerCase();
      const id = ++reqId.current;
      sentQuery.current = trimmed;
      if (!trimmed) {
        // An empty query is answered here: the caller shows everything.
        setResult({ query: "", matches: [] });
        return;
      }
      const w = workerRef.current;
      if (!w || status !== "ready") return;
      w.postMessage({ type: "search", id, q: trimmed } satisfies WorkerIn);
    },
    [status],
  );

  return {
    status,
    /**
     * `"js"` means the binary index was refused and the worker is scanning
     * strings instead: still correct, measurably slower, and until now
     * indistinguishable from the fast path. The usual cause is the corpus
     * outgrowing the container — more than 64 distinct editors or years, which
     * the WASM module matches through `u64` bitsets.
     */
    engine,
    error,
    matches: result?.matches ?? null,
    /** Query `matches` answers; compare with the live one before trusting it. */
    matchesQuery: result?.query ?? null,
    search,
  };
}
