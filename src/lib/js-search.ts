/**
 * The engine behind the binary one: a plain scan over lowercased strings.
 *
 * It answers whenever the WASM module cannot — the inventory not fitting the
 * container, the module not loading, a trap mid-query — and it has to answer
 * identically, because the page never says which engine produced a result list,
 * only which one is running.
 *
 * It lives here rather than inside `search.worker.ts` so that "identically" is
 * something a test can check against the code that ships. A worker module
 * cannot be imported outside a worker — it touches `self` as it loads — so
 * while these two functions sat in it, the only way to test them was to copy
 * them, and a copy agreeing with the module proves nothing about the original.
 * `engine-parity.test.ts` runs this against the real WASM module.
 */
import { searchableEditor } from "./editor-aliases.ts";
import type { SearchMatch, Wire } from "./inventory";

/** Position of the editor in a wire row: siglum, editor, year, lang, inv, corpus. */
const EDITOR_COLUMN = 1;

/** What the catalog writes where a document says nothing. */
const MISSING = "—";

/** One group, folded to lowercase once so a query can be a plain `includes`. */
export type JsGroup = {
  /** The group's own label, e.g. `cth 786`. */
  label: string;
  /** Per item: siglum and metadata joined, the text a query is tested against. */
  haystacks: string[];
};

/** Read the inventory into the lowercased strings a query is tested against. */
export function buildJsIndex(w: Wire): JsGroup[] {
  const pool = w.p;
  const groups: JsGroup[] = new Array(w.g.length);
  for (let gi = 0; gi < w.g.length; gi++) {
    const [label, rows] = w.g[gi]!;
    const haystacks: string[] = new Array(rows.length);
    for (let ri = 0; ri < rows.length; ri++) {
      const row = rows[ri]!;
      let hay = row[0]!;
      for (let k = 1; k < row.length; k++) {
        let part = pool[row[k] as number] ?? MISSING;
        // Column 1 is the editor: search it under every spelling of the same
        // person, as the WASM index does — the two engines must answer the
        // same query the same way.
        if (k === EDITOR_COLUMN) part = searchableEditor(part);
        // The marker stands for a value the document does not give, so it is
        // not searchable text. The binary index leaves the same fields empty
        // for the same reason — see `pooled` in `search-index.ts`.
        if (part && part !== MISSING) hay += `\n${part}`;
      }
      haystacks[ri] = hay.toLowerCase();
    }
    groups[gi] = { label: label.toLowerCase(), haystacks };
  }
  return groups;
}

/**
 * Every group with something matching `q`, which must already be lowercased.
 *
 * The shape mirrors what the module writes: a group whose own label matches is
 * one entry standing for all of its manuscripts, and a group matched through
 * its rows carries the indices of those rows.
 */
export function searchJs(index: JsGroup[] | null, q: string): SearchMatch[] {
  if (!index) return [];
  const matches: SearchMatch[] = [];
  for (let group = 0; group < index.length; group++) {
    const g = index[group]!;
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
