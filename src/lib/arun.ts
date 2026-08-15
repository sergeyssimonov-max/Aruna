/**
 * ARUN v3 reader — display inventory and search wire from the binary container.
 *
 * The layout itself lives in `./arun-format.js`, shared with the writer.
 */
import type { Inventory, Item, Wire } from "./inventory";
import {
  ARUN_MAGIC,
  ARUN_VERSION,
  DIR_ENTRY,
  GROUP,
  HEADER,
  ITEM,
  NO_PREFIX,
  headerOffset,
} from "./arun-format.js";

export { ARUN_MAGIC, ARUN_VERSION };

const td = new TextDecoder("utf-8");

function u32(v: DataView, o: number) {
  return v.getUint32(o, true);
}

function strAt(bytes: Uint8Array, off: number, len: number): string {
  return td.decode(bytes.subarray(off, off + len));
}

/**
 * Read a directory of `(offset, length)` pairs into strings.
 *
 * Each entry is checked against its own pool. `subarray` clamps out-of-range
 * indices instead of throwing, so an entry reaching past the pool used to
 * produce a short or empty string that looked like real data — and one reaching
 * into the *next* pool produced a plausible wrong one. Failing here means a
 * damaged file is reported as damaged.
 */
function readDir(
  v: DataView,
  bytes: Uint8Array,
  dirOff: number,
  n: number,
  poolOff: number,
  poolLen: number,
  what: string,
): string[] {
  const out = new Array<string>(n);
  for (let i = 0; i < n; i++) {
    const base = dirOff + i * 4;
    const off = v.getUint16(base, true);
    const len = v.getUint16(base + 2, true);
    if (off + len > poolLen) {
      throw new Error(`ARUN: ${what} entry ${i} runs past its pool`);
    }
    out[i] = strAt(bytes, poolOff + off, len);
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
  sufPoolLen: number;
};

function openArun(buf: ArrayBuffer): Layout {
  if (buf.byteLength < HEADER) throw new Error("ARUN: truncated header");
  const v = new DataView(buf);
  const bytes = new Uint8Array(buf);
  const field = (name: Parameters<typeof headerOffset>[0]) => u32(v, headerOffset(name));

  if (field("magic") !== ARUN_MAGIC) throw new Error("ARUN: bad magic");
  if (field("version") !== ARUN_VERSION) {
    throw new Error(`ARUN: unsupported version ${field("version")}`);
  }

  const manuscripts = field("manuscripts");
  const nGroups = field("nGroups");
  const nItems = field("nItems");
  const nAuth = field("nAuth");
  const nYear = field("nYear");
  const nLang = field("nLang");
  const nInv = field("nInv");
  const nCorp = field("nCorp");
  const nPrefix = field("nPrefix");
  const sourceLen = field("sourceLen");
  const sufPoolLen = field("sufPoolLen");
  const authPoolLen = field("authPoolLen");
  const yearPoolLen = field("yearPoolLen");
  const langPoolLen = field("langPoolLen");
  const invPoolLen = field("invPoolLen");
  const corpPoolLen = field("corpPoolLen");
  const prefixPoolLen = field("prefixPoolLen");

  let o = HEADER;
  const source = strAt(bytes, o, sourceLen);
  o += sourceLen;
  const groupsOff = o;
  o += nGroups * GROUP;
  const itemsOff = o;
  o += nItems * ITEM;
  const authDirOff = o;
  o += nAuth * DIR_ENTRY;
  const yearDirOff = o;
  o += nYear * DIR_ENTRY;
  const langDirOff = o;
  o += nLang * DIR_ENTRY;
  const invDirOff = o;
  o += nInv * DIR_ENTRY;
  const corpDirOff = o;
  o += nCorp * DIR_ENTRY;
  const prefixDirOff = o;
  o += nPrefix * DIR_ENTRY;
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
    auths: readDir(v, bytes, authDirOff, nAuth, authPoolOff, authPoolLen, "author"),
    years: readDir(v, bytes, yearDirOff, nYear, yearPoolOff, yearPoolLen, "year"),
    langs: readDir(v, bytes, langDirOff, nLang, langPoolOff, langPoolLen, "lang"),
    invs: readDir(v, bytes, invDirOff, nInv, invPoolOff, invPoolLen, "inv"),
    corps: readDir(v, bytes, corpDirOff, nCorp, corpPoolOff, corpPoolLen, "corpus"),
    prefixes: readDir(v, bytes, prefixDirOff, nPrefix, prefixPoolOff, prefixPoolLen, "prefix"),
    sufPoolOff,
    sufPoolLen,
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
  if (sufOff + sufLen > L.sufPoolLen) {
    throw new Error(`ARUN: item ${index} siglum runs past the suffix pool`);
  }
  const suf = strAt(b, L.sufPoolOff + sufOff, sufLen);
  // A prefix id other than NO_PREFIX must name a real prefix; silently dropping
  // it would hand back a truncated siglum that reads as a genuine one.
  if (pref !== NO_PREFIX && pref >= L.prefixes.length) {
    throw new Error(`ARUN: item ${index} names prefix ${pref}, pool holds ${L.prefixes.length}`);
  }
  const sig = pref === NO_PREFIX ? suf : L.prefixes[pref]! + suf;
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
    const gb = L.groupsOff + gi * GROUP;
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
    const gb = L.groupsOff + gi * GROUP;
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
