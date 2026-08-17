/**
 * The two search engines must answer every query the same way.
 *
 * The page has one search box and two things behind it: the compact binary
 * index the WASM module walks, and a scan over strings when the module is out
 * of the picture. Which one is answering depends on the browser, the network
 * and whether the catalog still fits the container — none of which a reader
 * chooses. So the fallback is only honest if it is invisible in the results,
 * and until this test there was nothing checking that it was.
 *
 * It was not. `—`, the marker the table prints where a document says nothing,
 * matched 14 349 manuscripts through the module and none through the scan: the
 * builder pooled the marker as though it were an author's name, while the scan
 * dropped it. Neither answer was wrong so much as unrepeatable.
 *
 * Run against the committed catalog and the committed module — the pair that
 * ships — rather than a fixture, because the disagreement above depended on
 * what the corpus happens to contain.
 */
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { parseWire } from "./arun.ts";
import { buildJsIndex, searchJs } from "./js-search.ts";
import type { SearchMatch } from "./inventory";
import { buildSearchIndex } from "./search-index.ts";
import { WasmSearch } from "./wasm-search.ts";

/** A committed file, as its own ArrayBuffer. */
function read(path: string): ArrayBuffer {
  const buf = readFileSync(new URL(path, import.meta.url));
  return buf.buffer.slice(buf.byteOffset, buf.byteOffset + buf.byteLength) as ArrayBuffer;
}

const wire = parseWire(read("../data/inventory.bin"));
const jsIndex = buildJsIndex(wire);

/**
 * Queries chosen to reach every part of both indexes: the pooled fields, the
 * group labels, the metadata appended to a siglum, the alias table, and the
 * shapes a person actually types.
 */
const QUERIES = [
  // the marker that started this
  "—",
  // sigla, whole and partial
  "kbo",
  "kub 35.99",
  "kbo 17.86+",
  "İk 174-66",
  "chds",
  // group labels, including the prefixes that overlap
  "cth 786",
  "cth 16",
  "cth 316",
  "cth 999",
  // editors, both spellings of the two people the alias table knows
  "schwemer",
  "daniel schwemer",
  "ds",
  "fuscagni",
  "ff",
  "burgin",
  "jb",
  "oğuz soysal",
  // years
  "2016",
  "2026",
  "1999",
  // languages and corpora, which ride along with the siglum
  "hit",
  "hur",
  "luw",
  "hfr",
  "kultinv",
  // inventory numbers
  "vat",
  "bo ",
  // single characters, punctuation and things that are in no document
  "a",
  "z",
  "'",
  '"',
  "<b>",
  ".",
  "+",
  "qqqqqzzz",
  "𒀀",
];

/** Comparable, and readable when it fails. */
function shape(matches: SearchMatch[]) {
  return matches.map((m) => `${m.group}:${m.items === null ? "*" : m.items.join(",")}`);
}

test("the binary index and the string scan answer every query alike", async () => {
  const blob = buildSearchIndex(wire);
  assert.ok(blob, "the shipped catalog no longer builds an index the module accepts");

  const wasm = await WasmSearch.fromBytes(read("../wasm/search.wasm"), blob);
  assert.ok(wasm, "the committed module would not instantiate");

  try {
    for (const q of QUERIES) {
      const fromWasm = shape(wasm.search(q));
      const fromJs = shape(searchJs(jsIndex, q));
      assert.deepEqual(
        fromWasm,
        fromJs,
        `query ${JSON.stringify(q)}: the module found ${fromWasm.length} groups, ` +
          `the scan ${fromJs.length} — a reader would get different results ` +
          `depending on whether the module happened to load`,
      );
    }
  } finally {
    wasm.dispose();
  }
});

test("the missing-value marker is not searchable text in either engine", () => {
  // Stated on its own because the agreement above would also be satisfied by
  // both engines matching the marker, and matching it is not what is wanted:
  // `—` says the document gives no editor, and a row is not "about" the fact
  // that something is absent from it.
  assert.deepEqual(searchJs(jsIndex, "—"), [], "the scan matched the marker");
});
