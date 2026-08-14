import type { Wire } from "./inventory";

/** TLH2 magic — must match wasm/search. */
const MAGIC = 0x32484c54;
/**
 * Author and year pools are matched through `u64` bitsets in the WASM module,
 * which caps each at 64 entries (`MAX_POOL` in wasm/search/src/lib.rs).
 *
 * This used to be guarded as `> 255`, the width of the id field rather than the
 * width of the bitset. Between 65 and 255 authors the builder was happy and the
 * module rejected the result, so search fell back to the JavaScript scan with
 * nothing said. The corpus currently holds 44 distinct authors.
 */
const MAX_POOL = 64;
const te = new TextEncoder();

/**
 * Build the compact TLH2 binary index for `public/wasm/search.wasm`.
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
  const pool = wire.p;

  // ── collect unique lowercase strings ──────────────────────────
  const authMap = new Map<string, number>();
  const yearMap = new Map<string, number>();
  const auths: string[] = [];
  const years: string[] = [];

  const internAuth = (s: string) => {
    const k = s.toLowerCase();
    let i = authMap.get(k);
    if (i === undefined) {
      i = auths.length;
      authMap.set(k, i);
      auths.push(k);
    }
    return i;
  };
  const internYear = (s: string) => {
    const k = s.toLowerCase();
    let i = yearMap.get(k);
    if (i === undefined) {
      i = years.length;
      yearMap.set(k, i);
      years.push(k);
    }
    return i;
  };

  type ItemRec = { sig: string; auth: number; year: number };
  type GroupRec = { cth: number; items: ItemRec[] };

  const groups: GroupRec[] = [];
  const sigSet = new Map<string, number>(); // sig -> first occurrence order for pool
  const sigList: string[] = [];

  const internSig = (s: string) => {
    const k = s.toLowerCase();
    let i = sigSet.get(k);
    if (i === undefined) {
      i = sigList.length;
      sigSet.set(k, i);
      sigList.push(k);
    }
    return k; // store string; offsets assigned later
  };

  for (const [c, rows] of wire.g) {
    const m = /^CTH\s*(\d+)/i.exec(c);
    const cth = m ? parseInt(m[1]!, 10) : 0;
    const items: ItemRec[] = new Array(rows.length);
    for (let ri = 0; ri < rows.length; ri++) {
      const row = rows[ri]!;
      const s = row[0]!;
      const ai = row[1]!;
      const yi = row[2]!;
      const li = row[3];
      const ii = row[4];
      const ci = row[5];
      // Searchable blob: siglum + lang/inv/corpus (fits u8 length).
      let hay = s.toLowerCase();
      for (const idx of [li, ii, ci]) {
        if (idx === undefined) continue;
        const extra = (pool[idx] ?? "—").toLowerCase();
        if (!extra || extra === "—") continue;
        if (hay.length + 1 + extra.length <= 255) hay += `\n${extra}`;
      }
      const sig = internSig(hay);
      const auth = internAuth(pool[ai] ?? "—");
      const year = internYear(pool[yi] ?? "—");
      if (auth >= MAX_POOL || year >= MAX_POOL) return null;
      items[ri] = { sig, auth, year };
    }
    if (cth > 0xffff) return null;
    groups.push({ cth, items });
  }

  // ── build pools ───────────────────────────────────────────────
  const sigMeta: { off: number; len: number }[] = [];
  let sigPoolLen = 0;
  const sigParts: Uint8Array[] = [];
  const sigOffByStr = new Map<string, { off: number; len: number }>();
  for (const s of sigList) {
    const b = te.encode(s);
    if (b.length > 255) return null;
    const meta = { off: sigPoolLen, len: b.length };
    sigOffByStr.set(s, meta);
    sigMeta.push(meta);
    sigParts.push(b);
    sigPoolLen += b.length;
  }

  const packSmallPool = (list: string[]) => {
    const parts: Uint8Array[] = [];
    const dir: { off: number; len: number }[] = [];
    let len = 0;
    for (const s of list) {
      const b = te.encode(s);
      if (b.length > 0xffff || len > 0xffff) return null;
      dir.push({ off: len, len: b.length });
      parts.push(b);
      len += b.length;
    }
    return { parts, dir, len };
  };

  const authP = packSmallPool(auths);
  const yearP = packSmallPool(years);
  if (!authP || !yearP) return null;

  // ── layout ────────────────────────────────────────────────────
  const nGroups = groups.length;
  const nItems = groups.reduce((a, g) => a + g.items.length, 0);
  const header = 32;
  const groupsBytes = nGroups * 8;
  const itemsBytes = nItems * 8;
  const authDirBytes = auths.length * 4;
  const yearDirBytes = years.length * 4;
  const total =
    header +
    groupsBytes +
    itemsBytes +
    authDirBytes +
    yearDirBytes +
    sigPoolLen +
    authP.len +
    yearP.len;

  const buf = new ArrayBuffer(total);
  const view = new DataView(buf);
  const u8 = new Uint8Array(buf);

  view.setUint32(0, MAGIC, true);
  view.setUint32(4, nGroups, true);
  view.setUint32(8, nItems, true);
  view.setUint32(12, auths.length, true);
  view.setUint32(16, years.length, true);
  view.setUint32(20, sigPoolLen, true);
  view.setUint32(24, authP.len, true);
  view.setUint32(28, yearP.len, true);

  let o = header;
  let itemCursor = 0;
  for (const g of groups) {
    view.setUint16(o, g.cth, true);
    view.setUint16(o + 2, g.items.length, true);
    view.setUint32(o + 4, itemCursor, true);
    o += 8;
    itemCursor += g.items.length;
  }

  for (const g of groups) {
    for (const it of g.items) {
      const meta = sigOffByStr.get(it.sig)!;
      view.setUint32(o, meta.off, true);
      u8[o + 4] = meta.len;
      u8[o + 5] = it.auth;
      u8[o + 6] = it.year;
      u8[o + 7] = 0;
      o += 8;
    }
  }

  for (const d of authP.dir) {
    view.setUint16(o, d.off, true);
    view.setUint16(o + 2, d.len, true);
    o += 4;
  }
  for (const d of yearP.dir) {
    view.setUint16(o, d.off, true);
    view.setUint16(o + 2, d.len, true);
    o += 4;
  }

  for (const part of sigParts) {
    u8.set(part, o);
    o += part.length;
  }
  for (const part of authP.parts) {
    u8.set(part, o);
    o += part.length;
  }
  for (const part of yearP.parts) {
    u8.set(part, o);
    o += part.length;
  }

  return buf;
}

