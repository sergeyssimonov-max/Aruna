/** Display model + search match types (ARUN v3 / wire schema v2). */

export type Item = {
  s: string;
  a: string;
  y: string;
  l: string;
  c: string;
};

export type Group = {
  c: string;
  i: Item[];
};

export type Inventory = {
  source: string;
  manuscripts: number;
  groups: Group[];
};

/** Legacy wire shape used only when building TLH2 offline / tests. */
export type Wire = {
  s: string;
  m: number;
  p: string[];
  g: [string, [string, number, number, number, number, number][]][];
  v?: number;
};

export type SearchMatch = {
  gi: number;
  /** null = whole CTH group matches */
  ii: number[] | null;
};

/** Project search hits onto inventory groups (share item object refs). */
export function applyMatches(inv: Inventory, matches: SearchMatch[]): Group[] {
  const out: Group[] = new Array(matches.length);
  for (let i = 0; i < matches.length; i++) {
    const m = matches[i]!;
    const g = inv.groups[m.gi]!;
    if (m.ii === null) {
      out[i] = g;
    } else {
      const items: Item[] = new Array(m.ii.length);
      for (let j = 0; j < m.ii.length; j++) items[j] = g.i[m.ii[j]!]!;
      out[i] = { c: g.c, i: items };
    }
  }
  return out;
}
