import { test } from "node:test";
import assert from "node:assert/strict";
import type { Group } from "./inventory.ts";
import { GROUP_H, ROW_H, buildLayout, countItems, visibleRows } from "./virtual-list.ts";

function groups(...sizes: number[]): Group[] {
  return sizes.map((n, gi) => ({
    c: `CTH ${gi + 1}`,
    i: Array.from({ length: n }, (_, li) => ({
      s: `sig ${gi}.${li}`,
      a: "ed",
      y: "2021",
      l: "Hit",
      c: "TLH",
    })),
  }));
}

test("layout heights follow the row constants", () => {
  const g = groups(2, 0, 3);
  const open = buildLayout(g, true);
  assert.equal(open.groupCount, 3);
  assert.equal(open.totalH, 3 * GROUP_H + 5 * ROW_H);
  assert.equal(open.itemBase[3], 5, "running item totals end at the item count");

  const closed = buildLayout(g, false);
  assert.equal(closed.totalH, 3 * GROUP_H, "collapsed: headers only");
  assert.equal(closed.itemBase[3], 0);
});

test("an empty inventory lays out and renders nothing", () => {
  const layout = buildLayout([], true);
  assert.equal(layout.totalH, 0);
  assert.deepEqual(visibleRows([], layout, true, 0, 800), []);
  assert.equal(countItems([]), 0);
});

test("only rows inside the window are produced", () => {
  const g = groups(100);
  const layout = buildLayout(g, true);
  const rows = visibleRows(g, layout, true, 0, 200);

  assert.ok(rows.length > 0, "the window is not empty");
  assert.ok(rows.length < 100, "and it is not the whole group either");
  for (const row of rows) {
    assert.ok(row.y < 200, `row at ${row.y} starts before the window ends`);
    assert.ok(row.y + ROW_H > 0, "and ends after it begins");
  }
});

test("scrolling far down still finds the right rows", () => {
  const g = groups(1000);
  const layout = buildLayout(g, true);
  const y0 = GROUP_H + 500 * ROW_H;
  const rows = visibleRows(g, layout, true, y0, y0 + 3 * ROW_H);

  const items = rows.filter((r) => r.t === 1);
  assert.ok(items.length > 0, "found items");
  // Row numbering is one-based and continuous across the whole inventory.
  assert.equal(items[0]!.n, 501, "first visible row is #501");
  assert.equal(items[0]!.s, "sig 0.500");
});

test("numbering runs unbroken across groups", () => {
  const g = groups(2, 3);
  const layout = buildLayout(g, true);
  const rows = visibleRows(g, layout, true, 0, layout.totalH);
  const numbers = rows.filter((r) => r.t === 1).map((r) => r.n);
  assert.deepEqual(numbers, [1, 2, 3, 4, 5]);
});

test("keys are unique within a window", () => {
  const g = groups(5, 5, 5);
  const layout = buildLayout(g, true);
  const rows = visibleRows(g, layout, true, 0, layout.totalH);
  const keys = rows.map((r) => r.key);
  assert.equal(new Set(keys).size, keys.length, "React would drop duplicates");
});

test("collapsed groups show headers and no items", () => {
  const g = groups(4, 4);
  const layout = buildLayout(g, false);
  const rows = visibleRows(g, layout, false, 0, layout.totalH);
  assert.equal(rows.length, 2);
  assert.ok(
    rows.every((r) => r.t === 0),
    "headers only",
  );
});

test("a zero-height window yields nothing", () => {
  const g = groups(10);
  const layout = buildLayout(g, true);
  assert.deepEqual(visibleRows(g, layout, true, 100, 100), []);
  assert.deepEqual(visibleRows(g, layout, true, 100, 50), []);
});

test("countItems totals every group", () => {
  assert.equal(countItems(groups(1, 2, 3)), 6);
  assert.equal(countItems(groups(0, 0)), 0);
});
