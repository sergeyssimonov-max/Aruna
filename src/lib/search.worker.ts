/// <reference lib="webworker" />
import { parseWire } from "./arun";
import type { SearchMatch } from "./inventory";
import { buildJsIndex, searchJs, type JsGroup } from "./js-search";
import type { FallbackReason, WorkerIn, WorkerOut } from "./search-protocol";
import { buildSearchIndex } from "./search-index";
import { fetchWasmModule } from "./wasm-module";
import { WasmSearch } from "./wasm-search";

let wasm: WasmSearch | null = null;
let jsIndex: JsGroup[] | null = null;
let engine: "wasm" | "js" = "js";
/** Why `engine` is `"js"`; null while the binary index is in use. */
let fallback: FallbackReason | null = null;
let nGroups = 0;
let nManuscripts = 0;

function runSearch(q: string): SearchMatch[] {
  if (!q) {
    const all: SearchMatch[] = new Array(nGroups);
    for (let i = 0; i < nGroups; i++) all[i] = { group: i, items: null };
    return all;
  }
  if (engine === "wasm" && wasm) {
    try {
      return wasm.search(q);
    } catch {
      // A trap poisons the instance: every later call would throw too. Letting
      // that reach the caller turns one bad query into a fatal error and takes
      // the whole inventory off the screen, even though the JavaScript index is
      // built and sitting right here. Switch to it for good and answer the
      // query that just failed.
      engine = "js";
      fallback = "trapped";
      wasm = null;
      // Said out loud: the page shows which engine is answering, and until now
      // this switch happened behind its back.
      ctx.postMessage({ type: "engine", engine: "js", reason: fallback } satisfies WorkerOut);
    }
  }
  return searchJs(jsIndex, q);
}

const ctx: DedicatedWorkerGlobalScope = self as unknown as DedicatedWorkerGlobalScope;

ctx.onmessage = async (ev: MessageEvent<WorkerIn>) => {
  const msg = ev.data;
  try {
    if (msg.type === "init") {
      wasm?.dispose();
      wasm = null;
      const wire = parseWire(msg.arun);
      nManuscripts = wire.m;
      nGroups = wire.g.length;
      jsIndex = buildJsIndex(wire);
      // Two different failures, kept apart because the page reports them to the
      // reader: the inventory not fitting the container, and the module not
      // being usable. Either way the JavaScript index covers it, so this is a
      // downgrade rather than a failure.
      const blob = buildSearchIndex(wire);
      const module = blob ? await fetchWasmModule() : null;
      wasm = blob && module ? await WasmSearch.fromBytes(module, blob) : null;
      engine = wasm ? "wasm" : "js";
      fallback = wasm ? null : blob ? "unavailable" : "unsupported";
      ctx.postMessage({
        type: "ready",
        manuscripts: nManuscripts,
        groups: nGroups,
        engine,
        ...(fallback ? { reason: fallback } : {}),
      } satisfies WorkerOut);
      return;
    }
    if (msg.type === "search") {
      ctx.postMessage({
        type: "result",
        id: msg.id,
        matches: runSearch(msg.q),
      } satisfies WorkerOut);
    }
  } catch (e) {
    ctx.postMessage({
      type: "error",
      message: e instanceof Error ? e.message : "Worker error",
    } satisfies WorkerOut);
  }
};

export {};
