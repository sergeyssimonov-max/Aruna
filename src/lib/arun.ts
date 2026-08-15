/**
 * ARUN v3 reader — display inventory and search wire from the binary container.
 *
 * The layout itself lives in `./arun-format.js`, shared with the writer.
 *
 * Reading happens twice over the same bytes and the two readers want different
 * things: the page wants strings to render ([`parseInventory`]), the worker
 * wants pool ids to index with ([`parseWire`]). Both start from [`openArun`],
 * which validates the container once and hands back where everything is.
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

/**
 * A walk through the container's sections, in file order.
 *
 * Every section begins where the previous one ended, so the offsets are not
 * independent facts to be maintained — they are this sequence of calls, and
 * `end` is where the file must reach.
 */
function sections(start: number) {
  let at = start;
  return {
    /** Offset of the next section, `size` bytes long. */
    take(size: number): number {
      const off = at;
      at += size;
      return off;
    },
    get end() {
      return at;
    },
  };
}

/** An opened container: validated header, and where each section begins. */
type Container = {
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

/**
 * Validate the header and resolve every section, or throw.
 *
 * Nothing downstream re-checks anything: an item read through the result is
 * inside the buffer because the walk below said the buffer was long enough.
 */
function openArun(buf: ArrayBuffer): Container {
  if (buf.byteLength < HEADER) throw new Error("ARUN: truncated header");
  const v = new DataView(buf);
  const bytes = new Uint8Array(buf);
  const field = (name: Parameters<typeof headerOffset>[0]) => u32(v, headerOffset(name));

  if (field("magic") !== ARUN_MAGIC) throw new Error("ARUN: bad magic");
  if (field("version") !== ARUN_VERSION) {
    throw new Error(`ARUN: unsupported version ${field("version")}`);
  }

  const nGroups = field("nGroups");
  const nItems = field("nItems");
  const nAuth = field("nAuth");
  const nYear = field("nYear");
  const nLang = field("nLang");
  const nInv = field("nInv");
  const nCorp = field("nCorp");
  const nPrefix = field("nPrefix");
  const sufPoolLen = field("sufPoolLen");
  const authPoolLen = field("authPoolLen");
  const yearPoolLen = field("yearPoolLen");
  const langPoolLen = field("langPoolLen");
  const invPoolLen = field("invPoolLen");
  const corpPoolLen = field("corpPoolLen");
  const prefixPoolLen = field("prefixPoolLen");

  // In file order: the source line, the records, the six directories, then the
  // pools they point into.
  const at = sections(HEADER);
  const sourceOff = at.take(field("sourceLen"));
  const groupsOff = at.take(nGroups * GROUP);
  const itemsOff = at.take(nItems * ITEM);
  const authDirOff = at.take(nAuth * DIR_ENTRY);
  const yearDirOff = at.take(nYear * DIR_ENTRY);
  const langDirOff = at.take(nLang * DIR_ENTRY);
  const invDirOff = at.take(nInv * DIR_ENTRY);
  const corpDirOff = at.take(nCorp * DIR_ENTRY);
  const prefixDirOff = at.take(nPrefix * DIR_ENTRY);
  const sufPoolOff = at.take(sufPoolLen);
  const authPoolOff = at.take(authPoolLen);
  const yearPoolOff = at.take(yearPoolLen);
  const langPoolOff = at.take(langPoolLen);
  const invPoolOff = at.take(invPoolLen);
  const corpPoolOff = at.take(corpPoolLen);
  const prefixPoolOff = at.take(prefixPoolLen);
  if (at.end > buf.byteLength) throw new Error("ARUN: truncated body");

  return {
    v,
    bytes,
    manuscripts: field("manuscripts"),
    nGroups,
    nItems,
    source: strAt(bytes, sourceOff, field("sourceLen")),
    groupsOff,
    itemsOff,
    auths: readDir(v, bytes, authDirOff, nAuth, authPoolOff, authPoolLen, "author"),
    years: readDir(v, bytes, yearDirOff, nYear, yearPoolOff, yearPoolLen, "year"),
    langs: readDir(v, bytes, langDirOff, nLang, langPoolOff, langPoolLen, "lang"),
    invs: readDir(v, bytes, invDirOff, nInv, invPoolOff, invPoolLen, "inv"),
    corps: readDir(v, bytes, corpDirOff, nCorp, corpPoolOff, corpPoolLen, "corpus"),
    prefixes: readDir(
      v,
      bytes,
      prefixDirOff,
      nPrefix,
      prefixPoolOff,
      prefixPoolLen,
      "prefix",
    ),
    sufPoolOff,
    sufPoolLen,
  };
}

/** One group record: its CTH number and the run of items belonging to it. */
function groupAt(c: Container, index: number): { cth: number; start: number; count: number } {
  const at = c.groupsOff + index * GROUP;
  return {
    cth: c.v.getUint16(at, true),
    count: c.v.getUint16(at + 2, true),
    start: c.v.getUint32(at + 4, true),
  };
}

/** One item record: its siglum, rebuilt from the prefix table, and its ids. */
function itemAt(
  c: Container,
  index: number,
): {
  sig: string;
  auth: number;
  year: number;
  lang: number;
  inv: number;
  corpus: number;
} {
  const ib = c.itemsOff + index * ITEM;
  const b = c.bytes;
  const sufOff = b[ib]! | (b[ib + 1]! << 8) | (b[ib + 2]! << 16);
  const sufLen = b[ib + 3]!;
  const pref = b[ib + 4]!;
  if (sufOff + sufLen > c.sufPoolLen) {
    throw new Error(`ARUN: item ${index} siglum runs past the suffix pool`);
  }
  const suf = strAt(b, c.sufPoolOff + sufOff, sufLen);
  // A prefix id other than NO_PREFIX must name a real prefix; silently dropping
  // it would hand back a truncated siglum that reads as a genuine one.
  if (pref !== NO_PREFIX && pref >= c.prefixes.length) {
    throw new Error(`ARUN: item ${index} names prefix ${pref}, pool holds ${c.prefixes.length}`);
  }
  return {
    sig: pref === NO_PREFIX ? suf : c.prefixes[pref]! + suf,
    auth: b[ib + 5]!,
    year: b[ib + 6]!,
    lang: b[ib + 7]!,
    inv: c.v.getUint16(ib + 8, true),
    corpus: b[ib + 10]!,
  };
}

/** Parse ARUN → display inventory (no inv column; kept only for search in worker). */
export function parseInventory(buf: ArrayBuffer): Inventory {
  const c = openArun(buf);
  const groups = new Array(c.nGroups);
  for (let gi = 0; gi < c.nGroups; gi++) {
    const { cth, start, count } = groupAt(c, gi);
    const items: Item[] = new Array(count);
    for (let li = 0; li < count; li++) {
      const it = itemAt(c, start + li);
      items[li] = {
        siglum: it.sig,
        editor: c.auths[it.auth] ?? "—",
        year: c.years[it.year] ?? "—",
        lang: c.langs[it.lang] ?? "—",
        corpus: c.corps[it.corpus] ?? "—",
      };
    }
    groups[gi] = { cth: `CTH ${cth}`, items };
  }
  return { source: c.source, manuscripts: c.manuscripts, groups };
}

/**
 * Full wire for search-index builder (worker only).
 *
 * The five metadata pools are concatenated into one, so a row can name any of
 * them with a single index — which is what the wire's tuples carry. The bases
 * below are where each pool starts inside that concatenation.
 */
export function parseWire(buf: ArrayBuffer): Wire {
  const c = openArun(buf);
  const pool: string[] = c.auths.concat(c.years, c.langs, c.invs, c.corps);
  const yearBase = c.auths.length;
  const langBase = yearBase + c.years.length;
  const invBase = langBase + c.langs.length;
  const corpBase = invBase + c.invs.length;

  const g: Wire["g"] = new Array(c.nGroups);
  for (let gi = 0; gi < c.nGroups; gi++) {
    const { cth, start, count } = groupAt(c, gi);
    const rows: Wire["g"][0][1] = new Array(count);
    for (let li = 0; li < count; li++) {
      const it = itemAt(c, start + li);
      rows[li] = [
        it.sig,
        it.auth,
        yearBase + it.year,
        langBase + it.lang,
        invBase + it.inv,
        corpBase + it.corpus,
      ];
    }
    g[gi] = [`CTH ${cth}`, rows];
  }
  return { s: c.source, m: c.manuscripts, p: pool, g, v: 2 };
}

export function isArun(buf: ArrayBuffer): boolean {
  return buf.byteLength >= 4 && new DataView(buf).getUint32(0, true) === ARUN_MAGIC;
}
