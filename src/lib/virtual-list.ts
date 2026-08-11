import type { Group } from "./inventory";

export const ROW_H = 40;
export const GROUP_H = 44;
/** Extra pixels above/below the viewport kept mounted. */
export const OVERSCAN_PX = 160;

export type Layout = {
  groupY: Uint32Array;
  itemBase: Uint32Array;
  totalH: number;
  groupCount: number;
};

export function buildLayout(groups: Group[], openAll: boolean): Layout {
  const g = groups.length;
  const groupY = new Uint32Array(g + 1);
  const itemBase = new Uint32Array(g + 1);
  let y = 0;
  let items = 0;
  for (let i = 0; i < g; i++) {
    groupY[i] = y;
    itemBase[i] = items;
    const n = groups[i]!.i.length;
    y += GROUP_H;
    if (openAll) {
      y += n * ROW_H;
      items += n;
    }
  }
  groupY[g] = y;
  itemBase[g] = items;
  return { groupY, itemBase, totalH: y, groupCount: g };
}

function groupAtY(groupY: Uint32Array, y: number, g: number): number {
  let lo = 0;
  let hi = g;
  while (lo < hi) {
    const mid = (lo + hi) >>> 1;
    if (groupY[mid + 1]! <= y) lo = mid + 1;
    else hi = mid;
  }
  return lo;
}

export type VisRow =
  | { t: 0; key: number; y: number; c: string; n: number }
  | {
      t: 1;
      key: number;
      y: number;
      n: number;
      s: string;
      a: string;
      yv: string;
      l: string;
      corpus: string;
    };

export function visibleRows(
  groups: Group[],
  layout: Layout,
  openAll: boolean,
  y0: number,
  y1: number,
): VisRow[] {
  const { groupY, itemBase, groupCount } = layout;
  if (groupCount === 0 || y1 <= y0) return [];

  const out: VisRow[] = [];
  let gi = groupAtY(groupY, y0 > 0 ? y0 : 0, groupCount);

  for (; gi < groupCount; gi++) {
    const gTop = groupY[gi]!;
    if (gTop >= y1) break;
    const g = groups[gi]!;
    const nItems = g.i.length;

    if (gTop + GROUP_H > y0) {
      out.push({ t: 0, key: (gi << 20) | 0xfffff, y: gTop, c: g.c, n: nItems });
    }

    if (!openAll) continue;

    const itemsTop = gTop + GROUP_H;
    const gBottom = groupY[gi + 1]!;
    if (itemsTop >= y1 || gBottom <= y0 || nItems === 0) continue;

    let first = ((y0 - itemsTop) / ROW_H) | 0;
    if (first < 0) first = 0;
    let last = ((y1 - itemsTop + ROW_H - 1) / ROW_H) | 0;
    if (last > nItems) last = nItems;
    const base = itemBase[gi]!;
    for (let li = first; li < last; li++) {
      const it = g.i[li]!;
      out.push({
        t: 1,
        key: (gi << 20) | li,
        y: itemsTop + li * ROW_H,
        n: base + li + 1,
        s: it.s,
        a: it.a,
        yv: it.y,
        l: it.l,
        corpus: it.c,
      });
    }
  }
  return out;
}

export function countItems(groups: Group[]): number {
  let n = 0;
  for (let i = 0; i < groups.length; i++) n += groups[i]!.i.length;
  return n;
}
