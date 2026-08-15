/**
 * ARUN v3 — the single description of the inventory container.
 *
 * Two programs speak this format: `scripts/build-inventory-bin.mjs` writes it
 * and `src/lib/arun.ts` reads it. They used to hold private copies of the magic,
 * the version and the header size, kept in step by a comment. This module is
 * the copy, so a change lands in both at once.
 *
 * Plain JavaScript on purpose: the writer runs under bare `node` during the
 * data build, and a `.js` module needs no type stripping to be imported there,
 * whatever Node the deployment happens to provide.
 *
 * ```text
 * header 80 B | source | groups 8 B | items 12 B | directories 4 B | pools
 * ```
 *
 * Group (8 B): cth u16, item count u16, first item index u32.
 * Item (12 B): suffix offset u24, suffix length u8, prefix id u8, author u8,
 *              year u8, lang u8, inv u16, corpus u8, pad u8.
 * Directory entry (4 B): pool offset u16, length u16.
 */

/** `ARUN`, little-endian. */
export const ARUN_MAGIC = 0x4e555241;
export const ARUN_VERSION = 3;

/** Bytes per item record. */
export const ITEM = 12;
/** Bytes per group record. */
export const GROUP = 8;
/** Bytes per directory entry. */
export const DIR_ENTRY = 4;

/** Prefix id meaning "this siglum is stored whole, not prefix-compressed". */
export const NO_PREFIX = 255;

/**
 * Header fields in order. Each is a little-endian `u32`, so a field's byte
 * offset is its index times four — which is what [`headerOffset`] returns.
 *
 * `searchLen` is written as 0 and ignored on read: it reserved room for an
 * embedded search index that was never built.
 */
export const HEADER_FIELDS = /** @type {const} */ ([
  "magic",
  "version",
  "manuscripts",
  "nGroups",
  "nItems",
  "nAuth",
  "nYear",
  "nLang",
  "nInv",
  "nCorp",
  "nPrefix",
  "sourceLen",
  "sufPoolLen",
  "authPoolLen",
  "yearPoolLen",
  "langPoolLen",
  "invPoolLen",
  "corpPoolLen",
  "prefixPoolLen",
  "searchLen",
]);

/** Header size in bytes (80). */
export const HEADER = HEADER_FIELDS.length * 4;

/**
 * Byte offset of a header field.
 *
 * @param {(typeof HEADER_FIELDS)[number]} field
 * @returns {number}
 */
export function headerOffset(field) {
  const i = HEADER_FIELDS.indexOf(field);
  if (i < 0) throw new Error(`ARUN: no header field ${field}`);
  return i * 4;
}
