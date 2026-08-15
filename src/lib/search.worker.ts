/// <reference lib="webworker" />
import { parseWire } from "./arun";
import type { SearchMatch, Wire } from "./inventory";
import type { WorkerIn, WorkerOut } from "./search-protocol";
import { buildSearchIndex } from "./search-index";
import { WasmSearch } from "./wasm-search";

/** One group, folded to lowercase once so a query can be a plain `includes`. */
type JsGroup = {
  /** The group's own label, e.g. `cth 786`. */
  label: string;
  /** Per item: siglum and metadata joined, the text a query is tested against. */
  haystacks: string[];
};

let wasm: WasmSearch | null = null;
let jsIndex: JsGroup[] | null = null;
let engine: "wasm" | "js" = "js";
let nGroups = 0;
let nManuscripts = 0;

function buildJsIndex(w: Wire): JsGroup[] {
  const pool = w.p;
  const groups: JsGroup[] = new Array(w.g.length);
  for (let gi = 0; gi < w.g.length; gi++) {
    const [label, rows] = w.g[gi]!;
    const haystacks: string[] = new Array(rows.length);
    for (let ri = 0; ri < rows.length; ri++) {
      const row = rows[ri]!;
      let hay = row[0]!;
      for (let k = 1; k < row.length; k++) {
        const part = pool[row[k] as number] ?? "—";
        if (part && part !== "—") hay += `\n${part}`;
      }
      haystacks[ri] = hay.toLowerCase();
    }
    groups[gi] = { label: label.toLowerCase(), haystacks };
  }
  return groups;
}

function searchJs(q: string): SearchMatch[] {
  if (!jsIndex) return [];
  const matches: SearchMatch[] = [];
  for (let group = 0; group < jsIndex.length; group++) {
    const g = jsIndex[group]!;
    // A group whose label matches stands for all of its manuscripts.
    if (g.label.includes(q)) {
      matches.push({ group, items: null });
      continue;
    }
    const items: number[] = [];
    for (let i = 0; i < g.haystacks.length; i++) {
      if (g.haystacks[i]!.includes(q)) items.push(i);
    }
    if (items.length) matches.push({ group, items });
  }
  return matches;
}

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
