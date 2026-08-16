#!/usr/bin/env node
/**
 * Build the ARUN v3 container the site downloads, from the catalog the CLI
 * emits: `src/data/inventory.json` → `inventory.bin` + `inventory.bin.gz`.
 *
 * Four stages, in the order the format forces — the header declares every
 * length, so nothing can be written until all of them are known:
 *
 *   choosePrefixes  which siglum prefixes are worth storing once
 *   collect         the catalog as ids into pools, with the strings deduplicated
 *   packStrings     each pool laid out end to end, with its directory
 *   write           the container
 *
 * The layout is `src/lib/arun-format.js`, imported here and by the reader in
 * `src/lib/arun.ts`, so the two cannot disagree about it. Plain JavaScript
 * because this runs under bare `node` during the data build.
 */
import { readFileSync, writeFileSync } from "node:fs";
import { gzipSync } from "node:zlib";
import { resolve, dirname } from "node:path";
import { fileURLToPath } from "node:url";
import {
  ARUN_MAGIC,
  ARUN_VERSION,
  DIR_ENTRY,
  GROUP,
  HEADER_FIELDS,
  HEADER,
  ITEM,
  NO_PREFIX,
} from "../src/lib/arun-format.js";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const jsonPath = resolve(root, "src/data/inventory.json");
const binPath = resolve(root, "src/data/inventory.bin");
const gzPath = resolve(root, "src/data/inventory.bin.gz");

const te = new TextEncoder();

/** A suffix offset is a `u24`; a pool string's length is a `u16`. */
const MAX_SUFFIX_POOL = 0xffffff;
const MAX_POOL_STRING = 0xffff;
/** Metadata ids are `u8`s in the item record — except the inventory number. */
const MAX_META_ID = 255;
const MAX_INV_ID = 0xffff;
/** A suffix's length is a `u8`. */
const MAX_SUFFIX = 255;

/** Deduplicating string table: id of each string, in first-seen order. */
function interner() {
  const ids = new Map();
  const list = [];
  return {
    intern(s) {
      let i = ids.get(s);
      if (i === undefined) {
        i = list.length;
        ids.set(s, i);
        list.push(s);
      }
      return i;
    },
    list,
  };
}

/**
 * Choose the shared siglum prefixes worth storing once.
 *
 * Sigla repeat heavily (`KBo `, `KUB 2`, …), so the container keeps up to 64
 * prefixes and each item stores only the id of its prefix plus what is left.
 * A candidate has to appear at least 25 times and save more than 64 bytes
 * overall, which is what keeps the table from filling with prefixes that pay
 * for themselves and nothing more.
 *
 * Longest first, so `splitSiglum` below takes the most it can; ties broken
 * alphabetically so the same catalog always produces the same bytes.
 */
function choosePrefixes(sigs, maxN = 64) {
  const freq = new Map();
  for (const s of sigs) {
    for (let n = 3; n <= 10; n++) {
      if (s.length > n) {
        const p = s.slice(0, n);
        freq.set(p, (freq.get(p) || 0) + 1);
      }
    }
  }
  const scored = [];
  for (const [p, c] of freq) {
    if (c < 25) continue;
    const save = c * (p.length - 1) - p.length;
    if (save > 64) scored.push({ p, c, save });
  }
  scored.sort((a, b) => b.save - a.save || b.p.length - a.p.length);
  const picked = [];
  const seen = new Set();
  for (const { p } of scored) {
    if (seen.has(p)) continue;
    seen.add(p);
    picked.push(p);
    if (picked.length >= maxN) break;
  }
  picked.sort((a, b) => b.length - a.length || a.localeCompare(b));
  return picked;
}

/** Split a siglum into a prefix id and the rest, or store it whole. */
function splitSiglum(s, prefixes) {
  for (let i = 0; i < prefixes.length; i++) {
    if (s.startsWith(prefixes[i])) return { pref: i, suf: s.slice(prefixes[i].length) };
  }
  return { pref: NO_PREFIX, suf: s };
}

/**
 * Read the catalog into ids: one pool per column, plus the suffix pool.
 *
 * Every field of the display table is drawn from a handful of distinct values
 * — 44 editors, a few dozen years, five languages — so the rows carry ids and
 * the strings are stored once.
 */
function collect(wire, prefixes) {
  const pool = wire.p;
  const suffixes = interner();
  const auths = interner();
  const years = interner();
  const langs = interner();
  const invs = interner();
  const corps = interner();

  const groups = wire.g.map(([label, rows]) => {
    const m = /^CTH\s*(\d+)/i.exec(label);
    const cth = m ? parseInt(m[1], 10) : 0;
    // A group keeps only this number: the reader rebuilds the label as
    // `CTH ${cth}`. Every one of the 663 groups in this corpus is exactly that,
    // so the round trip is currently lossless — but nothing made it stay so. A
    // future release naming a group `CTH 12.1` would be shown as `CTH 12`, one
    // with no CTH at all as `CTH 0`, and a number past u16 would wrap silently
    // in setUint16 into a different, entirely plausible group. Refuse instead:
    // a label the container cannot carry has to be a format decision, not a
    // quiet substitution nobody sees.
    if (cth > 0xffff) throw new Error(`group "${label}": CTH number exceeds u16`);
    if (`CTH ${cth}` !== label) {
      throw new Error(
        `group "${label}" would be published as "CTH ${cth}" — ARUN carries the number only`,
      );
    }

    const items = rows.map((row) => {
      const [siglum, ai, yi, li = 0, ii = 0, ci = 0] = row;
      const auth = auths.intern(pool[ai] ?? "—");
      const year = years.intern(pool[yi] ?? "—");
      const lang = langs.intern(pool[li] ?? "—");
      const inv = invs.intern(pool[ii] ?? "—");
      const corpus = corps.intern(pool[ci] ?? "—");
      if (auth > MAX_META_ID || year > MAX_META_ID || lang > MAX_META_ID || corpus > MAX_META_ID) {
        throw new Error("meta pool exceeds u8");
      }
      if (inv > MAX_INV_ID) throw new Error("inv pool exceeds u16");

      const { pref, suf } = splitSiglum(siglum, prefixes);
      return { pref, suf: suffixes.intern(suf), auth, year, lang, inv, corpus };
    });

    return { cth, items };
  });

  return { groups, suffixes, auths, years, langs, invs, corps };
}

/** Encode a pool: the bytes end to end, and where each string sits in them. */
function packStrings(list) {
  const parts = [];
  const dir = [];
  let len = 0;
  for (const s of list) {
    const b = te.encode(s);
    if (b.length > MAX_POOL_STRING) throw new Error("string > u16");
    dir.push({ off: len, len: b.length });
    parts.push(b);
    len += b.length;
  }
  return { parts, dir, len };
}

/**
 * Write the container: header, source, groups, items, directories, pools.
 *
 * The order here is the order `openArun` in `src/lib/arun.ts` walks, and the
 * final size check is the two halves agreeing about it.
 */
function write(wire, collected) {
  const { groups, suffixes, auths, years, langs, invs, corps } = collected;

  const sufP = packStrings(suffixes.list);
  const authP = packStrings(auths.list);
  const yearP = packStrings(years.list);
  const langP = packStrings(langs.list);
  const invP = packStrings(invs.list);
  const corpP = packStrings(corps.list);
  const prefP = packStrings(collected.prefixes);
  if (sufP.len > MAX_SUFFIX_POOL) throw new Error("suffix pool > u24");

  const sourceBytes = te.encode(wire.s);
  const nGroups = groups.length;
  const nItems = groups.reduce((a, g) => a + g.items.length, 0);
  const dirs = [authP, yearP, langP, invP, corpP, prefP];
  const pools = [sufP, ...dirs];

  const total =
    HEADER +
    sourceBytes.length +
    nGroups * GROUP +
    nItems * ITEM +
    dirs.reduce((n, p) => n + p.dir.length * DIR_ENTRY, 0) +
    pools.reduce((n, p) => n + p.len, 0);

  const out = new ArrayBuffer(total);
  const view = new DataView(out);
  const u8 = new Uint8Array(out);

  // Written by name against the shared field list, so the writer cannot drift out
  // of the order the reader expects — an off-by-one here would have silently
  // shifted every pool length by four bytes.
  const headerValues = {
    magic: ARUN_MAGIC,
    version: ARUN_VERSION,
    manuscripts: wire.m,
    nGroups,
    nItems,
    nAuth: authP.dir.length,
    nYear: yearP.dir.length,
    nLang: langP.dir.length,
    nInv: invP.dir.length,
    nCorp: corpP.dir.length,
    nPrefix: prefP.dir.length,
    sourceLen: sourceBytes.length,
    sufPoolLen: sufP.len,
    authPoolLen: authP.len,
    yearPoolLen: yearP.len,
    langPoolLen: langP.len,
    invPoolLen: invP.len,
    corpPoolLen: corpP.len,
    prefixPoolLen: prefP.len,
    searchLen: 0,
  };
  HEADER_FIELDS.forEach((name, i) => {
    if (!(name in headerValues)) throw new Error(`no value for header field ${name}`);
    view.setUint32(i * 4, headerValues[name] >>> 0, true);
  });

  let o = HEADER;
  u8.set(sourceBytes, o);
  o += sourceBytes.length;

  // Groups, each naming the run of items that follows the previous group's.
  let firstItem = 0;
  for (const g of groups) {
    view.setUint16(o, g.cth, true);
    view.setUint16(o + 2, g.items.length, true);
    view.setUint32(o + 4, firstItem, true);
    o += GROUP;
    firstItem += g.items.length;
  }

  for (const g of groups) {
    for (const it of g.items) {
      const suf = sufP.dir[it.suf];
      if (suf.len > MAX_SUFFIX) throw new Error("suf len");
      // The suffix offset is three bytes; there is no setUint24.
      u8[o] = suf.off & 0xff;
      u8[o + 1] = (suf.off >>> 8) & 0xff;
      u8[o + 2] = (suf.off >>> 16) & 0xff;
      u8[o + 3] = suf.len;
      u8[o + 4] = it.pref;
      u8[o + 5] = it.auth;
      u8[o + 6] = it.year;
      u8[o + 7] = it.lang;
      view.setUint16(o + 8, it.inv, true);
      u8[o + 10] = it.corpus;
      u8[o + 11] = 0; // pad
      o += ITEM;
    }
  }

  // All six directories, then the pools they point into — the suffix pool
  // first, because items address it directly rather than through a directory.
  for (const pool of dirs) {
    for (const entry of pool.dir) {
      view.setUint16(o, entry.off, true);
      view.setUint16(o + 2, entry.len, true);
      o += DIR_ENTRY;
    }
  }
  for (const pool of pools) {
    for (const part of pool.parts) {
      u8.set(part, o);
      o += part.length;
    }
  }

  if (o !== total) throw new Error(`size ${o} vs ${total}`);
  return { bytes: u8, pools: collected };
}

function main() {
  const wire = JSON.parse(readFileSync(jsonPath, "utf8"));

  const sigs = [];
  for (const [, rows] of wire.g) for (const [s] of rows) sigs.push(s);
  const prefixes = choosePrefixes([...new Set(sigs)]);

  const collected = { ...collect(wire, prefixes), prefixes };
  const { bytes } = write(wire, collected);

  writeFileSync(binPath, bytes);
  const gz = gzipSync(bytes, { level: 9, memLevel: 9 });
  writeFileSync(gzPath, gz);

  const kb = (n) => (n / 1024).toFixed(1);
  console.log(`[build-inventory-bin] ARUNv3 ${kb(bytes.length)} KB → gzip ${kb(gz.length)} KB`);
  console.log(
    `  meta pools auth=${collected.auths.list.length} year=${collected.years.list.length}` +
      ` lang=${collected.langs.list.length} inv=${collected.invs.list.length}` +
      ` corpus=${collected.corps.list.length}`,
  );
}

main();
