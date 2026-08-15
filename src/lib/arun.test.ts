import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { gunzipSync } from "node:zlib";
import { ARUN_MAGIC, isArun, parseInventory, parseWire } from "./arun.ts";

/** The catalog the app actually ships, as the browser receives it. */
function realArun(): ArrayBuffer {
  const gz = readFileSync(new URL("../../public/data/inventory.bin.gz", import.meta.url));
  const raw = gunzipSync(gz);
  return raw.buffer.slice(raw.byteOffset, raw.byteOffset + raw.byteLength) as ArrayBuffer;
}

test("isArun recognises the container and nothing else", () => {
  assert.equal(isArun(realArun()), true);
  assert.equal(isArun(new ArrayBuffer(0)), false, "empty");
  assert.equal(isArun(new ArrayBuffer(2)), false, "shorter than the magic");
  assert.equal(isArun(new ArrayBuffer(64)), false, "zeroed, so the magic is wrong");
});

test("the shipped catalog parses into a coherent inventory", () => {
  const inv = parseInventory(realArun());
  assert.ok(inv.groups.length > 0, "has groups");
  assert.ok(inv.manuscripts > 0, "has a manuscript count");
  assert.match(inv.source, /Zenodo/, "carries its provenance");

  const counted = inv.groups.reduce((n, g) => n + g.items.length, 0);
  assert.equal(counted, inv.manuscripts, "the header count matches the rows present");

  for (const g of inv.groups) {
    assert.match(g.cth, /^CTH \d+$/, `group label ${JSON.stringify(g.cth)}`);
  }

  // Every field is either real text or the missing-value dash — never empty,
  // never undefined, which is what the table renders directly.
  for (const item of inv.groups.flatMap((g) => g.items)) {
    for (const [field, value] of Object.entries(item)) {
      assert.equal(typeof value, "string", `${field} is a string`);
      assert.notEqual(value, "", `${field} is not blank`);
    }
  }
});

test("parseWire agrees with parseInventory about the same catalog", () => {
  const buf = realArun();
  const inv = parseInventory(buf);
  const wire = parseWire(buf);

  assert.equal(wire.g.length, inv.groups.length, "same number of groups");
  assert.equal(wire.m, inv.manuscripts, "same manuscript count");

  // The wire form stores metadata as pool indices; resolving them must give
  // back exactly what the display form shows.
  for (let gi = 0; gi < wire.g.length; gi++) {
    const [label, rows] = wire.g[gi]!;
    const group = inv.groups[gi]!;
    assert.equal(label, group.cth);
    assert.equal(rows.length, group.items.length);
    for (let ri = 0; ri < rows.length; ri++) {
      const row = rows[ri]!;
      const item = group.items[ri]!;
      assert.equal(row[0], item.siglum, "siglum");
      assert.equal(wire.p[row[1] as number], item.editor, "author");
      assert.equal(wire.p[row[2] as number], item.year, "year");
    }
  }
});

test("a truncated or foreign buffer is rejected, not half-read", () => {
  assert.throws(() => parseInventory(new ArrayBuffer(8)), /truncated header/);

  const notArun = new ArrayBuffer(128);
  assert.throws(() => parseInventory(notArun), /bad magic/);

  const wrongVersion = new ArrayBuffer(128);
  const v = new DataView(wrongVersion);
  v.setUint32(0, ARUN_MAGIC, true);
  v.setUint32(4, 99, true);
  assert.throws(() => parseInventory(wrongVersion), /unsupported version 99/);

  const short = realArun().slice(0, 200);
  assert.throws(() => parseInventory(short), /truncated body/);
});

test("a directory entry pointing past its pool is reported", () => {
  // `subarray` clamps rather than throwing, so without the bounds check this
  // produced a plausible-looking short string instead of an error.
  const buf = realArun();
  const v = new DataView(buf);
  const nGroups = v.getUint32(12, true);
  const nItems = v.getUint32(16, true);
  const nAuth = v.getUint32(20, true);
  const sourceLen = v.getUint32(44, true);

  const authDirOff = 80 + sourceLen + nGroups * 8 + nItems * 12;
  assert.ok(nAuth > 0, "the catalog has authors to corrupt");
  // Give author 0 a length that cannot fit in the pool.
  v.setUint16(authDirOff + 2, 0xffff, true);
  assert.throws(() => parseInventory(buf), /author entry 0 runs past its pool/);
});
