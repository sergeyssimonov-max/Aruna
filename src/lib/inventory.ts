/**
 * What the page holds in memory: the inventory as it is displayed.
 *
 * The names here are spelled out, and the wire formats' are not, on purpose.
 * `Wire` below and the ARUN container are storage — bytes on a server and in a
 * `postMessage`, where a short key is a smaller file. This model is built once
 * per load by `arun.ts` and read on every render, every search and every test;
 * nothing about it is serialised, so the abbreviations bought nothing and cost
 * the reader the difference between a group's `c` (its CTH) and an item's `c`
 * (its corpus), and between an item's `y` (its year) and a row's `y` (its
 * pixel offset).
 */

/** One manuscript, as it appears in a row. */
export type Item = {
  /** Publication siglum, e.g. `KBo 3.22`. */
  siglum: string;
  /** Transliteration / edition author. */
  editor: string;
  /** Edition year. */
  year: string;
  /** Dominant language code (Hit, Hur, Akk…). */
  lang: string;
  /** Edition series (HFR, TLH, HAnn…). */
  corpus: string;
};

/** A CTH catalogue group: the tablet family and its manuscripts. */
export type Group = {
  /** Group label, always `CTH <number>`. */
  cth: string;
  items: Item[];
};

export type Inventory = {
  source: string;
  manuscripts: number;
  groups: Group[];
};

/**
 * Legacy wire shape used only when building TLH2 offline / tests.
 *
 * Deliberately still terse: this one is a serialisation format, and its keys
 * are written and read by `arun.ts`, `search-index.ts` and the worker as a
 * unit — `s` source, `m` manuscripts, `p` string pool, `g` groups, `v` version.
 */
export type Wire = {
  s: string;
  m: number;
  p: string[];
  g: [string, [string, number, number, number, number, number][]][];
  v?: number;
};

/** Where a query hit: a group, and which of its items — `null` = all of them. */
export type SearchMatch = {
  group: number;
  /** null = the group's own label matched, so every item counts as a hit. */
  items: number[] | null;
};

/** Project search hits onto inventory groups (share item object refs). */
export function applyMatches(inv: Inventory, matches: SearchMatch[]): Group[] {
  const out: Group[] = new Array(matches.length);
  for (let i = 0; i < matches.length; i++) {
    const m = matches[i]!;
    const g = inv.groups[m.group]!;
    if (m.items === null) {
      out[i] = g;
    } else {
      const items: Item[] = new Array(m.items.length);
      for (let j = 0; j < m.items.length; j++) items[j] = g.items[m.items[j]!]!;
      out[i] = { cth: g.cth, items };
    }
  }
  return out;
}
