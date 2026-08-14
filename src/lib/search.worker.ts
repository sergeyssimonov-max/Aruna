/// <reference lib="webworker" />
import { parseWire } from "./arun";
import type { SearchMatch, Wire } from "./inventory";
import type { WorkerIn, WorkerOut } from "./search-protocol";
import { buildSearchIndex } from "./search-index";
import { WasmSearch } from "./wasm-search";

type JsGroup = { cl: string; h: string[] };

let wasm: WasmSearch | null = null;
let jsIndex: JsGroup[] | null = null;
let engine: "wasm" | "js" = "js";
let nGroups = 0;
let nManuscripts = 0;

function buildJsIndex(w: Wire): JsGroup[] {
  const pool = w.p;
  const groups: JsGroup[] = new Array(w.g.length);
  for (let gi = 0; gi < w.g.length; gi++) {
    const [c, rows] = w.g[gi]!;
    const h: string[] = new Array(rows.length);
    for (let ri = 0; ri < rows.length; ri++) {
      const row = rows[ri]!;
      let s = row[0]!;
      for (let k = 1; k < row.length; k++) {
        const part = pool[row[k] as number] ?? "—";
        if (part && part !== "—") s += `\n${part}`;
      }
      h[ri] = s.toLowerCase();
    }
    groups[gi] = { cl: c.toLowerCase(), h };
  }
  return groups;
}

function searchJs(q: string): SearchMatch[] {
  if (!jsIndex) return [];
  const matches: SearchMatch[] = [];
  for (let gi = 0; gi < jsIndex.length; gi++) {
    const g = jsIndex[gi]!;
    if (g.cl.includes(q)) {
      matches.push({ gi, ii: null });
      continue;
    }
    const ii: number[] = [];
    for (let i = 0; i < g.h.length; i++) {
      if (g.h[i]!.includes(q)) ii.push(i);
    }
    if (ii.length) matches.push({ gi, ii });
  }
  return matches;
}

function runSearch(q: string): SearchMatch[] {
  if (!q) {
    const all: SearchMatch[] = new Array(nGroups);
    for (let i = 0; i < nGroups; i++) all[i] = { gi: i, ii: null };
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
      wasm = null;
    }
  }
  return searchJs(q);
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
      // Null when the inventory outgrew the binary format; the JavaScript index
      // covers that case, so it is a downgrade rather than a failure.
      const blob = buildSearchIndex(wire);
      wasm = blob ? await WasmSearch.create(blob) : null;
      engine = wasm ? "wasm" : "js";
      ctx.postMessage({
        type: "ready",
        manuscripts: nManuscripts,
        groups: nGroups,
        engine,
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
