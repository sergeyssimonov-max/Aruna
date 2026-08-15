/**
 * The virtual list: which rows exist, where each one sits, and which of them
 * the viewport can currently see.
 *
 * Kept apart from the component that draws them — the arithmetic is testable
 * without a DOM, and `index.tsx` never computes an offset itself.
 */
import type { Group } from "./inventory";

export const ROW_H = 40;
export const GROUP_H = 44;
/** Extra pixels above/below the viewport kept mounted. */
export const OVERSCAN_PX = 160;

/**
 * Where every group starts, and how many items precede it.
 *
 * Both arrays have one entry more than there are groups: the extra slot holds
 * the totals, so `groupY[g]` is the full height and `itemBase[g]` the item
 * count without a special case at the end.
 */
export type Layout = {
  /** Top offset of each group, in pixels. */
  groupY: Uint32Array;
  /** Running item total before each group — the basis of row numbering. */
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
    const n = groups[i]!.items.length;
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

/** Index of the group covering pixel `y`, by binary search over the offsets. */
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

/**
 * A row to render: either a group heading or one manuscript.
 *
 * `kind` is the discriminant — TypeScript narrows the union on it, so a row
 * reached through `kind === "item"` is known to carry a siglum. `top` and
 * `key` are of the list, not of the data: the pixel offset the row is
 * translated to, and React's identity for it.
 */
export type VisRow =
  | {
      kind: "group";
      key: number;
      top: number;
      cth: string;
      /** Manuscripts in the group, shown beside its label. */
      count: number;
    }
  | {
      kind: "item";
      key: number;
      top: number;
      /** Position in the whole inventory, one-based, unbroken across groups. */
      number: number;
      siglum: string;
      editor: string;
      year: string;
      lang: string;
      corpus: string;
    };

/**
 * The rows intersecting the pixel window `[y0, y1)`, in document order.
 *
 * Nothing outside the window is built at all — the point of the list is that a
 * 24 000-row inventory costs a screenful of objects per frame.
 */
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
    const group = groups[gi]!;
    const nItems = group.items.length;

    if (gTop + GROUP_H > y0) {
      out.push({
        kind: "group",
        key: groupKey(gi),
        top: gTop,
        cth: group.cth,
        count: nItems,
      });
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
      const item = group.items[li]!;
      out.push({
        kind: "item",
        key: itemKey(gi, li),
        top: itemsTop + li * ROW_H,
        number: base + li + 1,
        siglum: item.siglum,
        editor: item.editor,
        year: item.year,
        lang: item.lang,
        corpus: item.corpus,
      });
    }
  }
  return out;
}

/**
 * React keys, packed as one number: the group index in the high bits, the item
 * within it in the low twenty. A heading takes the highest slot of its own
 * group, which no item can occupy, so headings and items never collide.
 */
function groupKey(gi: number): number {
  return (gi << 20) | 0xfffff;
}

function itemKey(gi: number, li: number): number {
  return (gi << 20) | li;
}

export function countItems(groups: Group[]): number {
  let n = 0;
  for (let i = 0; i < groups.length; i++) n += groups[i]!.items.length;
  return n;
}
