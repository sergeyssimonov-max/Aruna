import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { gunzipSync } from "node:zlib";
import { parseWire } from "./arun.ts";
import type { Wire } from "./inventory.ts";
import { buildSearchIndex } from "./search-index.ts";

/**
 * The limits the WASM module enforces on its side. Kept here so a change to
 * either half fails a test rather than degrading search silently — the builder
 * once guarded the pools at 255 while the module rejected anything over 64, and
 * the only symptom was a quiet fallback to the JavaScript scan.
 */
const MAX_POOL = 64;
const TLH2_MAGIC = 0x32484c54;
const HEADER = 32;

function realWire(): Wire {
  const gz = readFileSync(new URL("../../public/data/inventory.bin.gz", import.meta.url));
  const raw = gunzipSync(gz);
  const buf = raw.buffer.slice(raw.byteOffset, raw.byteOffset + raw.byteLength) as ArrayBuffer;
  return parseWire(buf);
}

function wireOf(rows: { sig: string; auth: string; year: string }[]): Wire {
  const pool: string[] = [];
  const intern = (s: string) => {
    const i = pool.indexOf(s);
    return i >= 0 ? i : pool.push(s) - 1;
  };
  // Lang, inv and corpus are folded into the searchable text; point them all at
  // the same pooled dash so the rows stay well-formed.
  const dash = intern("—");
  const g: Wire["g"] = [
    ["CTH 1", rows.map((r) => [r.sig, intern(r.auth), intern(r.year), dash, dash, dash])],
  ];
  return { s: "test", m: rows.length, p: pool, g, v: 2 };
}

test("the shipped catalog builds an index the module will accept", () => {
  const blob = buildSearchIndex(realWire());
  assert.ok(blob, "the real inventory must fit the binary format");

  const v = new DataView(blob);
  assert.equal(v.getUint32(0, true), TLH2_MAGIC, "magic");
  const nAuth = v.getUint32(12, true);
  const nYear = v.getUint32(16, true);
  assert.ok(nAuth > 0 && nAuth <= MAX_POOL, `authors ${nAuth} within the bitset`);
  assert.ok(nYear > 0 && nYear <= MAX_POOL, `years ${nYear} within the bitset`);

  // Every id an item carries has to address a real pool entry; ids past the
  // pool are what the module refuses to load.
  const nGroups = v.getUint32(4, true);
  const nItems = v.getUint32(8, true);
  const itemsOff = HEADER + nGroups * 8;
  const bytes = new Uint8Array(blob);
  for (let i = 0; i < nItems; i++) {
    const at = itemsOff + i * 8;
    assert.ok(bytes[at + 5]! < nAuth, `item ${i} author id`);
    assert.ok(bytes[at + 6]! < nYear, `item ${i} year id`);
  }
});

test("sections are sized exactly as the header claims", () => {
  const blob = buildSearchIndex(realWire())!;
  const v = new DataView(blob);
  const [nGroups, nItems, nAuth, nYear, sigLen, authLen, yearLen] = [4, 8, 12, 16, 20, 24, 28].map(
    (o) => v.getUint32(o, true),
  ) as number[];
  const expected =
    HEADER + nGroups! * 8 + nItems! * 8 + nAuth! * 4 + nYear! * 4 + sigLen! + authLen! + yearLen!;
  assert.equal(blob.byteLength, expected, "no slack, no shortfall");
});

test("more distinct authors than the bitset holds is declined, not thrown", () => {
  const rows = Array.from({ length: MAX_POOL + 1 }, (_, i) => ({
    sig: `kbo ${i}`,
    auth: `editor ${i}`,
    year: "2021",
  }));
  // A throw here would reach the worker and blank the page; null means "use the
  // JavaScript engine", which has no such limit.
  assert.equal(buildSearchIndex(wireOf(rows)), null);
});

test("exactly as many authors as the bitset holds still builds", () => {
  const rows = Array.from({ length: MAX_POOL }, (_, i) => ({
    sig: `kbo ${i}`,
    auth: `editor ${i}`,
    year: "2021",
  }));
  const blob = buildSearchIndex(wireOf(rows));
  assert.ok(blob, "64 authors is the limit, not one past it");
  assert.equal(new DataView(blob).getUint32(12, true), MAX_POOL);
});

test("a siglum too long for the length field is declined", () => {
  const blob = buildSearchIndex(wireOf([{ sig: "k".repeat(300), auth: "ed", year: "2021" }]));
  assert.equal(blob, null);
});

test("authors are pooled case-insensitively", () => {
  const blob = buildSearchIndex(
    wireOf([
      { sig: "a", auth: "Otten", year: "2021" },
      { sig: "b", auth: "OTTEN", year: "2021" },
    ]),
  )!;
  assert.equal(new DataView(blob).getUint32(12, true), 1, "one author, not two");
});
