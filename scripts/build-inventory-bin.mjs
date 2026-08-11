#!/usr/bin/env node
/**
 * Build ARUN v3 binary (+ gzip).
 * Item (12 B): suf_off:u24, suf_len:u8, prefix:u8, auth:u8, year:u8, lang:u8, inv:u16, corpus:u8, pad:u8
 */
import { readFileSync, writeFileSync } from "node:fs";
import { gzipSync } from "node:zlib";
import { resolve, dirname } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const jsonPath = resolve(root, "public/data/inventory.json");
const binPath = resolve(root, "public/data/inventory.bin");
const gzPath = resolve(root, "public/data/inventory.bin.gz");

const ARUN_MAGIC = 0x4e555241;
const ARUN_VERSION = 3;
const NO_PREFIX = 255;
const te = new TextEncoder();
const HEADER = 60;

const wire = JSON.parse(readFileSync(jsonPath, "utf8"));
const pool = wire.p;

const allSigs = [];
for (const [, rows] of wire.g) for (const [s] of rows) allSigs.push(s);
const uniqueSigs = [...new Set(allSigs)];

function buildPrefixes(sigs, maxN = 64) {
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

const prefixes = buildPrefixes(uniqueSigs);
const splitSig = (s) => {
  for (let i = 0; i < prefixes.length; i++) {
    if (s.startsWith(prefixes[i])) return { pref: i, suf: s.slice(prefixes[i].length) };
  }
  return { pref: NO_PREFIX, suf: s };
};

// Separate small vocab pools for meta fields (from wire pool indices)
function buildMetaPools() {
  const auths = []; const aMap = new Map();
  const years = []; const yMap = new Map();
  const langs = []; const lMap = new Map();
  const invs = []; const iMap = new Map();
  const corps = []; const cMap = new Map();
  const intern = (map, list, s) => {
    let i = map.get(s);
    if (i === undefined) { i = list.length; map.set(s, i); list.push(s); }
    return i;
  };
  const groups = [];
  for (const [c, rows] of wire.g) {
    const m = /^CTH\s*(\d+)/i.exec(c);
    const cth = m ? parseInt(m[1], 10) : 0;
    const items = rows.map((row) => {
      const [s, ai, yi, li = 0, ii = 0, ci = 0] = row;
      const auth = intern(aMap, auths, pool[ai] ?? "—");
      const year = intern(yMap, years, pool[yi] ?? "—");
      const lang = intern(lMap, langs, pool[li] ?? "—");
      const inv = intern(iMap, invs, pool[ii] ?? "—");
      const corpus = intern(cMap, corps, pool[ci] ?? "—");
      if (auth > 255 || year > 255 || lang > 255 || corpus > 255) {
        throw new Error("meta pool exceeds u8");
      }
      if (inv > 0xffff) throw new Error("inv pool exceeds u16");
      const { pref, suf } = splitSig(s);
      return { pref, suf, auth, year, lang, inv, corpus };
    });
    groups.push({ cth, items });
  }
  return { groups, auths, years, langs, invs, corps };
}

const { groups, auths, years, langs, invs, corps } = buildMetaPools();

const sufList = [];
const sufMap = new Map();
const internSuf = (suf) => {
  let i = sufMap.get(suf);
  if (i === undefined) { i = sufList.length; sufMap.set(suf, i); sufList.push(suf); }
  return i;
};
for (const g of groups) for (const it of g.items) it.sufIdx = internSuf(it.suf);

function packStrings(list) {
  const parts = []; const dir = []; let len = 0;
  for (const s of list) {
    const b = te.encode(s);
    if (b.length > 0xffff) throw new Error("string > u16");
    dir.push({ off: len, len: b.length });
    parts.push(b);
    len += b.length;
  }
  return { parts, dir, len };
}

const sufP = packStrings(sufList);
const authP = packStrings(auths);
const yearP = packStrings(years);
const langP = packStrings(langs);
const invP = packStrings(invs);
const corpP = packStrings(corps);
const prefP = packStrings(prefixes);
if (sufP.len > 0xffffff) throw new Error("suffix pool > u24");

const sourceBytes = te.encode(wire.s);
const nGroups = groups.length;
const nItems = groups.reduce((a, g) => a + g.items.length, 0);
const ITEM = 12;

// header 60: magic ver m nG nI nAuth nYear nLang nInv nCorp nPref srcL sufL authL yearL
// Wait - need more fields. Expand header to 72 (18 u32).
const HEADER_V3 = 72;
// magic, ver, m, nG, nI, nAuth, nYear, nLang, nInv, nCorp, nPref, srcL, sufL, authL, yearL, langL, invL, corpL, prefL, searchL = 20 u32 = 80
const H = 80;

const total =
  H +
  sourceBytes.length +
  nGroups * 8 +
  nItems * ITEM +
  auths.length * 4 + years.length * 4 + langs.length * 4 + invs.length * 4 + corps.length * 4 + prefixes.length * 4 +
  sufP.len + authP.len + yearP.len + langP.len + invP.len + corpP.len + prefP.len;

const out = new ArrayBuffer(total);
const view = new DataView(out);
const u8 = new Uint8Array(out);
let h = 0;
const pu = (x) => { view.setUint32(h, x >>> 0, true); h += 4; };
pu(ARUN_MAGIC); pu(ARUN_VERSION); pu(wire.m); pu(nGroups); pu(nItems);
pu(auths.length); pu(years.length); pu(langs.length); pu(invs.length); pu(corps.length); pu(prefixes.length);
pu(sourceBytes.length); pu(sufP.len); pu(authP.len); pu(yearP.len); pu(langP.len); pu(invP.len); pu(corpP.len); pu(prefP.len); pu(0);
if (h !== H) throw new Error(`header ${h} != ${H}`);

let o = H;
u8.set(sourceBytes, o); o += sourceBytes.length;
let ic = 0;
for (const g of groups) {
  view.setUint16(o, g.cth, true);
  view.setUint16(o + 2, g.items.length, true);
  view.setUint32(o + 4, ic, true);
  o += 8; ic += g.items.length;
}
for (const g of groups) {
  for (const it of g.items) {
    const meta = sufP.dir[it.sufIdx];
    u8[o] = meta.off & 0xff;
    u8[o + 1] = (meta.off >>> 8) & 0xff;
    u8[o + 2] = (meta.off >>> 16) & 0xff;
    u8[o + 3] = meta.len;
    if (meta.len > 255) throw new Error("suf len");
    u8[o + 4] = it.pref;
    u8[o + 5] = it.auth;
    u8[o + 6] = it.year;
    u8[o + 7] = it.lang;
    view.setUint16(o + 8, it.inv, true);
    u8[o + 10] = it.corpus;
    u8[o + 11] = 0;
    o += ITEM;
  }
}
const writeDir = (dir) => {
  for (const d of dir) {
    view.setUint16(o, d.off, true);
    view.setUint16(o + 2, d.len, true);
    o += 4;
  }
};
writeDir(authP.dir); writeDir(yearP.dir); writeDir(langP.dir); writeDir(invP.dir); writeDir(corpP.dir); writeDir(prefP.dir);
const writeParts = (parts) => { for (const p of parts) { u8.set(p, o); o += p.length; } };
writeParts(sufP.parts); writeParts(authP.parts); writeParts(yearP.parts);
writeParts(langP.parts); writeParts(invP.parts); writeParts(corpP.parts); writeParts(prefP.parts);
if (o !== total) throw new Error(`size ${o} vs ${total}`);

writeFileSync(binPath, u8);
const gz = gzipSync(u8, { level: 9, memLevel: 9 });
writeFileSync(gzPath, gz);
const kb = (n) => (n / 1024).toFixed(1);
console.log(`[build-inventory-bin] ARUNv3 ${kb(u8.length)} KB → gzip ${kb(gz.length)} KB`);
console.log(`  meta pools auth=${auths.length} year=${years.length} lang=${langs.length} inv=${invs.length} corpus=${corps.length}`);
