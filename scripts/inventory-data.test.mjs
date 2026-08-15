/**
 * The three committed data files describe one catalog — check they agree.
 *
 * `public/data/inventory.json` is the parser's output, `inventory.bin` is the
 * ARUN container built from it, and `inventory.bin.gz` is the file the browser
 * actually downloads (`load-inventory.ts` asks for the gzip first and only falls
 * back to the plain binary). CI rebuilds the binary from the catalog and fails
 * on a difference — but that job needs the 71 MiB Zenodo archive, and it never
 * looks at the gzip at all. A stale `.gz` committed beside a fresh `.bin` would
 * therefore serve every visitor the old catalog with the build green.
 *
 * These assertions need no archive and no network: they read what is committed.
 * The corpus job answers "does the catalog match the corpus"; this answers "do
 * the files we ship match the catalog", which is the half that was missing.
 */
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { gunzipSync } from "node:zlib";
import { resolve, dirname } from "node:path";
import { fileURLToPath } from "node:url";

import {
  ARUN_MAGIC,
  ARUN_VERSION,
  GROUP,
  HEADER,
  ITEM,
  headerOffset,
} from "../src/lib/arun-format.js";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const read = (name) => readFileSync(resolve(root, "public/data", name));

const catalog = JSON.parse(read("inventory.json").toString("utf8"));
const bin = read("inventory.bin");
const gz = read("inventory.bin.gz");

const view = new DataView(bin.buffer, bin.byteOffset, bin.byteLength);
const header = (field) => view.getUint32(headerOffset(field), true);

test("the gzip the browser downloads is the committed binary", () => {
  // Byte-for-byte on the decompressed bytes rather than on the gzip itself:
  // the container carries a timestamp and depends on the zlib build, so
  // comparing compressed bytes would fail for reasons that never reach a user.
  assert.deepEqual(
    gunzipSync(gz),
    bin,
    "inventory.bin.gz does not decompress to inventory.bin — run 'npm run build:data' and commit both",
  );
});

test("the binary is ARUN v3", () => {
  assert.equal(header("magic"), ARUN_MAGIC);
  assert.equal(header("version"), ARUN_VERSION);
});

test("the binary counts the manuscripts the catalog holds", () => {
  const rows = catalog.g.reduce((n, [, items]) => n + items.length, 0);

  assert.equal(header("manuscripts"), catalog.m, "header manuscript count");
  assert.equal(header("nItems"), rows, "items written");
  assert.equal(header("nGroups"), catalog.g.length, "CTH groups written");
  // The catalog's own summary line is part of what the site shows, so a
  // mismatch here is a wrong number on the page, not just an internal one.
  assert.equal(catalog.m, rows, "catalog summary vs its own rows");
});

test("the binary is exactly as long as its header says", () => {
  // Every pool length is declared in the header, so the total is computable —
  // and a truncated commit (a partial write, a mangled merge) is caught here
  // rather than by a reader running off the end in someone's browser.
  const declared =
    HEADER +
    header("sourceLen") +
    header("nGroups") * GROUP +
    header("nItems") * ITEM +
    (header("nAuth") +
      header("nYear") +
      header("nLang") +
      header("nInv") +
      header("nCorp") +
      header("nPrefix")) *
      4 +
    header("sufPoolLen") +
    header("authPoolLen") +
    header("yearPoolLen") +
    header("langPoolLen") +
    header("invPoolLen") +
    header("corpPoolLen") +
    header("prefixPoolLen");

  assert.equal(bin.byteLength, declared);
});

test("the binary names the source the catalog names", () => {
  const source = bin.subarray(HEADER, HEADER + header("sourceLen")).toString("utf8");
  assert.equal(source, catalog.s);
});
