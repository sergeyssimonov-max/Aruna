/**
 * Build the compact TLH2 binary index the WASM module searches.
 *
 * Three stages, in the order the format forces: read the inventory into ids
 * ([`collect`]), lay the deduplicated strings out into pools ([`packPool`]),
 * then write the container ([`write`]). Nothing is written before every length
 * is known, because the header declares them all.
 *
 * The layout itself lives in `wasm/search/src/format.rs` — the reader is Rust
 * and cannot share a module with this one, so `tlh2-agreement.test.ts` parses
 * the constants there and fails if the two disagree.
 */
import { searchableEditor } from "./editor-aliases.ts";
import type { Wire } from "./inventory";

/** TLH2 magic — must match wasm/search. */
const MAGIC = 0x32484c54;
/** Header: eight little-endian `u32`s. */
const HEADER = 32;
/** cth u16, item count u16, first item u32. */
const GROUP_STRIDE = 8;
/** siglum offset u32, length u8, auth u8, year u8, pad. */
const ITEM_STRIDE = 8;
/** pool offset u16, length u16. */
const DIR_STRIDE = 4;
/**
 * Entries an author or year pool may hold — as many as the `u8` id in an item
 * record can address (`MAX_POOL` in wasm/search/src/format.rs, checked against
 * this constant by tlh2-agreement.test.ts).
 *
 * It was 64 on both sides for a while: the width of the `u64` the module used
 * as a bitset. Before that the two disagreed — this file guarded at 255 and the
 * module refused anything past 64, which showed up only as search quietly
 * falling back to the JavaScript scan. The module now sizes its bitset to the
 * pool, so the limit is the format's own. The corpus currently fills 45
 * entries: 46 spellings, two of which are one person under `editor-aliases`
 * and share an entry.
 */
const MAX_POOL = 255;
/** A siglum's length is a `u8` in the item record. */
const MAX_SIGLUM_BYTES = 255;
/** Directory entries address their pool with `u16`s. */
const MAX_SMALL_POOL_BYTES = 0xffff;
/** What the catalog writes where a document says nothing. */
const MISSING = "—";

const te = new TextEncoder();

/**
 * Build the compact TLH2 binary index for `src/wasm/search.wasm`.
 *
 * Returns null when the inventory does not fit the format — too many distinct
 * authors or years, a siglum past 255 bytes, a CTH past `u16`. The caller then
 * uses the JavaScript engine, which has no such limits.
 *
 * Null rather than an exception on purpose: this runs inside the worker on
 * every load, and a throw here would surface as a fatal error and replace the
 * inventory with an error screen, when a slower but working search was
 * available all along.
 */
export function buildSearchIndex(wire: Wire): ArrayBuffer | null {
  const collected = collect(wire);
  if (!collected) return null;

  const sigs = packPool(collected.sigs.list, MAX_SIGLUM_BYTES, Infinity);
  const auths = packPool(collected.auths.list, MAX_SMALL_POOL_BYTES, MAX_SMALL_POOL_BYTES);
  const years = packPool(collected.years.list, MAX_SMALL_POOL_BYTES, MAX_SMALL_POOL_BYTES);
  if (!sigs || !auths || !years) return null;

  return write(collected.groups, sigs, auths, years);
}

/**
 * A field's searchable form: what it says, or nothing at all.
 *
 * Every item record names an author and a year, so a document that gives
 * neither still needs an entry in each pool — and the entry it used to get was
 * the dash the table displays. That made `—` a query matching the 14 349
 * manuscripts with no editor, while the JavaScript engine, which drops the
 * marker from its haystack outright, answered the same query with nothing. The
 * marker stands for the absence of a value; it is not a value to search for, so
 * the pool holds an empty string and no query reaches it.
 */
function pooled(value: string): string {
  return value === MISSING ? "" : value;
}

/** A pooled string table: first-seen order, deduplicated, lowercase. */
type Interner = {
  /** Id of `s` in the pool, adding it if this is its first occurrence. */
  intern(s: string): number;
  readonly list: string[];
};

function interner(): Interner {
  const ids = new Map<string, number>();
  const list: string[] = [];
  return {
    intern(s: string) {
      const key = s.toLowerCase();
      let id = ids.get(key);
      if (id === undefined) {
        id = list.length;
        ids.set(key, id);
        list.push(key);
      }
      return id;
    },
    list,
  };
}

/** One manuscript, as ids into the three pools. */
type IndexItem = { sig: number; auth: number; year: number };
type IndexGroup = { cth: number; items: IndexItem[] };

type Collected = {
  groups: IndexGroup[];
  sigs: Interner;
  auths: Interner;
  years: Interner;
};

/**
 * Read the inventory into ids, deduplicating as it goes.
 *
 * Returns null as soon as something will not fit the container: an id past the
 * bitset the module matches with, or a CTH past the `u16` the group record
 * holds.
 */
function collect(wire: Wire): Collected | null {
  const pool = wire.p;
  const sigs = interner();
  const auths = interner();
  const years = interner();
  const groups: IndexGroup[] = [];

  for (const [label, rows] of wire.g) {
    const m = /^CTH\s*(\d+)/i.exec(label);
    const cth = m ? parseInt(m[1]!, 10) : 0;
    if (cth > 0xffff) return null;

    const items: IndexItem[] = new Array(rows.length);
    for (let ri = 0; ri < rows.length; ri++) {
      const row = rows[ri]!;
      // The editor is pooled with the person's other spellings appended, so a
      // surname reaches the rows that carry only initials — see editor-aliases.
      const auth = auths.intern(pooled(searchableEditor(pool[row[1]!] ?? MISSING)));
      const year = years.intern(pooled(pool[row[2]!] ?? MISSING));
      if (auth >= MAX_POOL || year >= MAX_POOL) return null;
      items[ri] = { sig: sigs.intern(searchableText(row, pool)), auth, year };
    }
    groups.push({ cth, items });
  }

  return { groups, sigs, auths, years };
}

/**
 * The text a query is matched against: the siglum, plus lang, inventory number
 * and corpus when there is room for them.
 *
 * They share one string because the item record has one offset and one length.
 * The budget is that length field — a `u8` — so metadata is appended only while
 * it fits, and a document keeps its siglum searchable either way.
 */
function searchableText(row: Wire["g"][0][1][0], pool: string[]): string {
  let hay = row[0]!.toLowerCase();
  for (const id of [row[3], row[4], row[5]]) {
    if (id === undefined) continue;
    const extra = (pool[id] ?? MISSING).toLowerCase();
    if (!extra || extra === MISSING) continue;
    if (hay.length + 1 + extra.length <= MAX_SIGLUM_BYTES) hay += `\n${extra}`;
  }
  return hay;
}

/** Pooled strings laid out end to end, with where each one starts. */
type PackedPool = {
  bytes: Uint8Array;
  entries: { off: number; len: number }[];
};

/**
 * Encode `list` into one buffer, refusing a pool the format cannot address.
 *
 * `maxEntry` is the width of the length field that will describe an entry;
 * `maxTotal` the width of the offsets pointing into the pool.
 */
function packPool(list: string[], maxEntry: number, maxTotal: number): PackedPool | null {
  const parts: Uint8Array[] = [];
  const entries: { off: number; len: number }[] = [];
  let len = 0;
  for (const s of list) {
    const encoded = te.encode(s);
    if (encoded.length > maxEntry || len > maxTotal) return null;
    entries.push({ off: len, len: encoded.length });
    parts.push(encoded);
    len += encoded.length;
  }

  const bytes = new Uint8Array(len);
  let at = 0;
  for (const part of parts) {
    bytes.set(part, at);
    at += part.length;
  }
  return { bytes, entries };
}

/** Write the container: header, groups, items, directories, pools. */
function write(
  groups: IndexGroup[],
  sigs: PackedPool,
  auths: PackedPool,
  years: PackedPool,
): ArrayBuffer {
  const nGroups = groups.length;
  const nItems = groups.reduce((n, g) => n + g.items.length, 0);
  const nAuth = auths.entries.length;
  const nYear = years.entries.length;

  const total =
    HEADER +
    nGroups * GROUP_STRIDE +
    nItems * ITEM_STRIDE +
    (nAuth + nYear) * DIR_STRIDE +
    sigs.bytes.length +
    auths.bytes.length +
    years.bytes.length;

  const buf = new ArrayBuffer(total);
  const view = new DataView(buf);
  const u8 = new Uint8Array(buf);

  view.setUint32(0, MAGIC, true);
  view.setUint32(4, nGroups, true);
  view.setUint32(8, nItems, true);
  view.setUint32(12, nAuth, true);
  view.setUint32(16, nYear, true);
  view.setUint32(20, sigs.bytes.length, true);
  view.setUint32(24, auths.bytes.length, true);
  view.setUint32(28, years.bytes.length, true);

  let o = HEADER;

  // Groups, each naming the run of items that follows the previous group's.
  let firstItem = 0;
  for (const g of groups) {
    view.setUint16(o, g.cth, true);
    view.setUint16(o + 2, g.items.length, true);
    view.setUint32(o + 4, firstItem, true);
    o += GROUP_STRIDE;
    firstItem += g.items.length;
  }

  // Items, in the same order the groups just claimed them.
  for (const g of groups) {
    for (const item of g.items) {
      const sig = sigs.entries[item.sig]!;
      view.setUint32(o, sig.off, true);
      u8[o + 4] = sig.len;
      u8[o + 5] = item.auth;
      u8[o + 6] = item.year;
      u8[o + 7] = 0; // pad
      o += ITEM_STRIDE;
    }
  }

  for (const pool of [auths, years]) {
    for (const entry of pool.entries) {
      view.setUint16(o, entry.off, true);
      view.setUint16(o + 2, entry.len, true);
      o += DIR_STRIDE;
    }
  }

  for (const pool of [sigs, auths, years]) {
    u8.set(pool.bytes, o);
    o += pool.bytes.length;
  }

  return buf;
}
