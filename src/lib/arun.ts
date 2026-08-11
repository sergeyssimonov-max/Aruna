/**
 * ARUN v3 — single binary inventory container.
 * Header 80 B · items 12 B · prefix-compressed sigla · Lang/Inv/Corpus pools.
 */
import type { Inventory, Item, Wire } from "./inventory";

export const ARUN_MAGIC = 0x4e555241;
export const ARUN_VERSION = 3;
const HEADER = 80;
const ITEM = 12;
const NO_PREFIX = 255;
const td = new TextDecoder("utf-8");

function u32(v: DataView, o: number) {
  return v.getUint32(o, true);
}

function strAt(bytes: Uint8Array, off: number, len: number): string {
  return td.decode(bytes.subarray(off, off + len));
}

function readDir(
  v: DataView,
  bytes: Uint8Array,
  dirOff: number,
  n: number,
  poolOff: number,
): string[] {
  const out = new Array<string>(n);
  for (let i = 0; i < n; i++) {
    const base = dirOff + i * 4;
    out[i] = strAt(bytes, poolOff + v.getUint16(base, true), v.getUint16(base + 2, true));
  }
  return out;
}

type Layout = {
  v: DataView;
  bytes: Uint8Array;
  manuscripts: number;
  nGroups: number;
  nItems: number;
  source: string;
  groupsOff: number;
  itemsOff: number;
  auths: string[];
  years: string[];
  langs: string[];
  invs: string[];
  corps: string[];
  prefixes: string[];
  sufPoolOff: number;
};

function openArun(buf: ArrayBuffer): Layout {
  if (buf.byteLength < HEADER) throw new Error("ARUN: truncated header");
  const v = new DataView(buf);
  const bytes = new Uint8Array(buf);
  if (u32(v, 0) !== ARUN_MAGIC) throw new Error("ARUN: bad magic");
  if (u32(v, 4) !== ARUN_VERSION) throw new Error(`ARUN: unsupported version ${u32(v, 4)}`);

  const manuscripts = u32(v, 8);
  const nGroups = u32(v, 12);
  const nItems = u32(v, 16);
  const nAuth = u32(v, 20);
  const nYear = u32(v, 24);
  const nLang = u32(v, 28);
  const nInv = u32(v, 32);
  const nCorp = u32(v, 36);
  const nPrefix = u32(v, 40);
  const sourceLen = u32(v, 44);
  const sufPoolLen = u32(v, 48);
  const authPoolLen = u32(v, 52);
  const yearPoolLen = u32(v, 56);
  const langPoolLen = u32(v, 60);
  const invPoolLen = u32(v, 64);
  const corpPoolLen = u32(v, 68);
  const prefixPoolLen = u32(v, 72);
  // search_len at 76 ignored (always 0 in current builds)

  let o = HEADER;
  const source = strAt(bytes, o, sourceLen);
  o += sourceLen;
  const groupsOff = o;
  o += nGroups * 8;
  const itemsOff = o;
  o += nItems * ITEM;
  const authDirOff = o;
  o += nAuth * 4;
  const yearDirOff = o;
  o += nYear * 4;
  const langDirOff = o;
  o += nLang * 4;
  const invDirOff = o;
  o += nInv * 4;
  const corpDirOff = o;
  o += nCorp * 4;
  const prefixDirOff = o;
  o += nPrefix * 4;
  const sufPoolOff = o;
  o += sufPoolLen;
  const authPoolOff = o;
  o += authPoolLen;
  const yearPoolOff = o;
  o += yearPoolLen;
  const langPoolOff = o;
  o += langPoolLen;
  const invPoolOff = o;
  o += invPoolLen;
  const corpPoolOff = o;
  o += corpPoolLen;
  const prefixPoolOff = o;
  o += prefixPoolLen;
  if (o > buf.byteLength) throw new Error("ARUN: truncated body");

  return {
    v,
    bytes,
    manuscripts,
    nGroups,
    nItems,
    source,
    groupsOff,
    itemsOff,
    auths: readDir(v, bytes, authDirOff, nAuth, authPoolOff),
    years: readDir(v, bytes, yearDirOff, nYear, yearPoolOff),
    langs: readDir(v, bytes, langDirOff, nLang, langPoolOff),
    invs: readDir(v, bytes, invDirOff, nInv, invPoolOff),
    corps: readDir(v, bytes, corpDirOff, nCorp, corpPoolOff),
    prefixes: readDir(v, bytes, prefixDirOff, nPrefix, prefixPoolOff),
    sufPoolOff,
  };
}

function itemAt(L: Layout, index: number): {
  sig: string;
  auth: number;
  year: number;
  lang: number;
  inv: number;
  corpus: number;
} {
  const ib = L.itemsOff + index * ITEM;
  const b = L.bytes;
  const sufOff = b[ib]! | (b[ib + 1]! << 8) | (b[ib + 2]! << 16);
  const sufLen = b[ib + 3]!;
  const pref = b[ib + 4]!;
  const suf = strAt(b, L.sufPoolOff + sufOff, sufLen);
  const sig = pref === NO_PREFIX ? suf : (L.prefixes[pref] ?? "") + suf;
  return {
    sig,
    auth: b[ib + 5]!,
    year: b[ib + 6]!,
    lang: b[ib + 7]!,
    inv: L.v.getUint16(ib + 8, true),
    corpus: b[ib + 10]!,
  };
}

/** Parse ARUN → display inventory (no inv column; kept only for search in worker). */
export function parseInventory(buf: ArrayBuffer): Inventory {
  const L = openArun(buf);
  const groups = new Array(L.nGroups);
  for (let gi = 0; gi < L.nGroups; gi++) {
    const gb = L.groupsOff + gi * 8;
    const cth = L.v.getUint16(gb, true);
    const count = L.v.getUint16(gb + 2, true);
    const start = L.v.getUint32(gb + 4, true);
    const items: Item[] = new Array(count);
    for (let li = 0; li < count; li++) {
      const it = itemAt(L, start + li);
      items[li] = {
        s: it.sig,
        a: L.auths[it.auth] ?? "—",
        y: L.years[it.year] ?? "—",
        l: L.langs[it.lang] ?? "—",
        c: L.corps[it.corpus] ?? "—",
      };
    }
    groups[gi] = { c: `CTH ${cth}`, i: items };
  }
  return { source: L.source, manuscripts: L.manuscripts, groups };
}

/** Full wire for search-index builder (worker only). */
export function parseWire(buf: ArrayBuffer): Wire {
  const L = openArun(buf);
  const pool: string[] = L.auths.concat(L.years, L.langs, L.invs, L.corps);
  const yBase = L.auths.length;
  const lBase = yBase + L.years.length;
  const iBase = lBase + L.langs.length;
  const cBase = iBase + L.invs.length;

  const g: Wire["g"] = new Array(L.nGroups);
  for (let gi = 0; gi < L.nGroups; gi++) {
    const gb = L.groupsOff + gi * 8;
    const cth = L.v.getUint16(gb, true);
    const count = L.v.getUint16(gb + 2, true);
    const start = L.v.getUint32(gb + 4, true);
    const rows: Wire["g"][0][1] = new Array(count);
    for (let li = 0; li < count; li++) {
      const it = itemAt(L, start + li);
      rows[li] = [
        it.sig,
        it.auth,
        yBase + it.year,
        lBase + it.lang,
        iBase + it.inv,
        cBase + it.corpus,
      ];
    }
    g[gi] = [`CTH ${cth}`, rows];
  }
  return { s: L.source, m: L.manuscripts, p: pool, g, v: 2 };
}

export function isArun(buf: ArrayBuffer): boolean {
  return buf.byteLength >= 4 && new DataView(buf).getUint32(0, true) === ARUN_MAGIC;
}
