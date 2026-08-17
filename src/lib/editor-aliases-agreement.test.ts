/**
 * One person must be one person in both inventories.
 *
 * The site folds an editor's spellings together when it builds its search
 * index (`editor-aliases.ts`); the standalone HTML the CLI writes does the same
 * when it builds the text each row is searched by (`cli/src/html_filter.js`).
 * The two lists cannot import each other — one is a TypeScript module, the
 * other a script embedded in a generated document — so this reads the JavaScript
 * one and checks the two name the same people under the same spellings.
 *
 * The seam is quiet when it breaks: both searches keep working, they just stop
 * finding the same rows. `schwemer` returning 91 manuscripts on the site and 84
 * in the downloaded file is exactly the state this catches — and exactly the
 * state that existed before the JavaScript side had a table at all.
 *
 * Spellings are compared lowercased, and as sets: the HTML side has no use for
 * a spelling's original case, and the order of the two people is nobody's
 * business but the file's.
 */
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { EDITOR_ALIASES } from "./editor-aliases.ts";

const FILTER_JS = "../../cli/src/html_filter.js";
const script = readFileSync(new URL(FILTER_JS, import.meta.url), "utf8");

/** The groups the embedded script carries, as written. */
function scriptGroups(): string[][] {
  const table = /var EDITOR_ALIASES = (\[[\s\S]*?\]);/.exec(script);
  assert.ok(table, `${FILTER_JS} no longer declares an EDITOR_ALIASES table`);

  // The literal is written with double quotes, so it is also JSON — no
  // JavaScript evaluation, which would run a file this test only wants to read.
  let parsed: unknown;
  try {
    parsed = JSON.parse(table[1]!);
  } catch {
    assert.fail(`${FILTER_JS} declares EDITOR_ALIASES in a shape this test cannot read`);
  }
  assert.ok(Array.isArray(parsed), "EDITOR_ALIASES is not a list");
  return parsed as string[][];
}

/** Groups as comparable sets: lowercased, deduplicated, sorted. */
function normalise(groups: readonly (readonly string[])[]): string[][] {
  return groups
    .map((spellings) => [...new Set(spellings.map((s) => s.toLowerCase()))].sort())
    .sort((a, b) => (a[0]! < b[0]! ? -1 : 1));
}

test("both inventories fold the same editors together", () => {
  assert.deepEqual(
    normalise(scriptGroups()),
    normalise(EDITOR_ALIASES.map((alias) => alias.spellings)),
    "a search for one person would reach different rows in the two inventories",
  );
});

test("the embedded table is lowercase, which is what it is compared against", () => {
  // The script lowercases the Editor cell before looking it up, so a spelling
  // written here in mixed case would simply never match — silently, and only
  // for the person it was added for.
  for (const group of scriptGroups()) {
    for (const spelling of group) {
      assert.equal(
        spelling,
        spelling.toLowerCase(),
        `${FILTER_JS} lists ${spelling}, which its own lookup can never match`,
      );
    }
  }
});
