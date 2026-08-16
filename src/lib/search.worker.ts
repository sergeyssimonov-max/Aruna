/// <reference lib="webworker" />
import { parseWire } from "./arun";
import { searchableEditor } from "./editor-aliases.ts";
import type { SearchMatch, Wire } from "./inventory";
import type { FallbackReason, WorkerIn, WorkerOut } from "./search-protocol";
import { buildSearchIndex } from "./search-index";
import { WasmSearch } from "./wasm-search";

/** Position of the editor in a wire row: siglum, editor, year, lang, inv, corpus. */
const EDITOR_COLUMN = 1;

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
/** Why `engine` is `"js"`; null while the binary index is in use. */
let fallback: FallbackReason | null = null;
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
        let part = pool[row[k] as number] ?? "—";
        // Column 1 is the editor: search it under every spelling of the same
        // person, as the WASM index does — the two engines must answer the
        // same query the same way.
        if (k === EDITOR_COLUMN) part = searchableEditor(part);
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
      fallback = "trapped";
      wasm = null;
      // Said out loud: the page shows which engine is answering, and until now
      // this switch happened behind its back.
      ctx.postMessage({ type: "engine", engine: "js", reason: fallback } satisfies WorkerOut);
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
      // Two different failures, kept apart because the page reports them to the
      // reader: the inventory not fitting the container, and the module not
      // being usable. Either way the JavaScript index covers it, so this is a
      // downgrade rather than a failure.
      const blob = buildSearchIndex(wire);
      wasm = blob ? await WasmSearch.create(blob) : null;
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
