/**
 * The CLI's HTML inventory and the site show the same table, and each side has
 * its own list of what the columns are: `COLUMNS` in `cli/src/html.rs` and in
 * `src/lib/columns.ts`. They cannot import each other — one is Rust, the other
 * TypeScript — so this reads the Rust list and checks the two describe the same
 * six columns, in the same order, with the same words.
 *
 * Within each side the list is already single: the Rust one generates the
 * legend, the `<colgroup>` and the `<thead>`, and the TypeScript one the legend
 * and the heading row. What nothing covered is the seam between the two
 * outputs, and the failure there is quiet — both pages render, they just stop
 * describing the same table. A column renamed in one place and not the other is
 * exactly what this catches.
 *
 * The class names are deliberately not compared. They are `c-num` in the
 * standalone HTML and `vl-c-num` in the app because each has its own stylesheet
 * that owns those names; requiring them to match would fail on a styling change
 * that no reader can see.
 */
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { COLUMNS } from "./columns.ts";

const HTML_RS = "../../cli/src/html.rs";
const rust = readFileSync(new URL(HTML_RS, import.meta.url), "utf8");

/** The `Column { head: …, class: …, legend: … }` items of the Rust table. */
function rustColumns(): { head: string; legend: string }[] {
  const table = /const COLUMNS: \[Column; \d+\] = \[([\s\S]*?)\n\];/.exec(rust);
  assert.ok(table, `${HTML_RS} no longer declares a COLUMNS table`);

  const item = /Column \{\s*head: "([^"]*)",\s*class: "[^"]*",\s*legend: "([^"]*)",\s*\}/g;
  const out: { head: string; legend: string }[] = [];
  for (const m of table[1]!.matchAll(item)) out.push({ head: m[1]!, legend: m[2]! });

  assert.ok(out.length > 0, `${HTML_RS} declares COLUMNS in a shape this test cannot read`);
  return out;
}

test("the CLI's table and the site's describe the same columns", () => {
  const fromRust = rustColumns();

  assert.deepEqual(
    COLUMNS.map((c) => ({ head: c.head, legend: c.legend })),
    fromRust,
    "the two inventories would show tables that disagree about their own columns",
  );
});

test("every column the Rust table declares was read, not silently skipped", () => {
  // The count is stated in the Rust type — `[Column; 6]` — so a column added
  // there without a matching entry here cannot pass as "no entries matched".
  const declared = /const COLUMNS: \[Column; (\d+)\]/.exec(rust);
  assert.ok(declared, `${HTML_RS} no longer declares COLUMNS with a fixed length`);
  assert.equal(rustColumns().length, Number(declared[1]), "not every Column item parsed");
});
