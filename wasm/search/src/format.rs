//! TLH2 — the layout of the search index, and nothing else.
//!
//! Two programs speak this format and they cannot share a module: the builder
//! is TypeScript (`src/lib/search-index.ts`) and the reader is this crate. What
//! they can share is one file to read the constants out of, which is what
//! `src/lib/tlh2-agreement.test.ts` does — it parses the `const` items below
//! and fails the build if the builder disagrees with any of them. Keep them
//! here, spelled the way that test expects: `const NAME: type = value;`.
//!
//! It is the counterpart of `src/lib/arun-format.js`, which the ARUN writer and
//! reader import directly because both are JavaScript.
//!
//! # Layout (little-endian)
//!
//! ```text
//! header 32 B | groups 8 B each | items 8 B each | auth_dir 4 B | year_dir 4 B | pools
//! ```
//!
//! Header: magic, n_groups, n_items, n_auth, n_year, sig_pool_len,
//! auth_pool_len, year_pool_len — eight `u32`s.
//! Group: cth `u16`, item count `u16`, first item index `u32`.
//! Item: siglum offset `u32`, siglum length `u8`, auth id `u8`, year id `u8`, pad.
//! Directory entry: pool offset `u16`, length `u16`.
//!
//! Result buffer, written back by `search`: a `u32` count, then that many
//! 12-byte entries of group index, kind, item index within the group.

/// `TLH2`, little-endian.
pub const MAGIC: u32 = 0x3248_4C54;

/// Header size in bytes.
pub const HEADER: usize = 32;

/// Bytes per group record.
pub const GROUP_STRIDE: usize = 8;

/// Bytes per item record.
pub const ITEM_STRIDE: usize = 8;

/// Bytes per directory entry.
pub const DIR_STRIDE: usize = 4;

/// Bytes per entry in the result buffer.
pub const RESULT_STRIDE: usize = 12;

/// Entries a metadata pool may hold: as many as the id field can address.
///
/// An item names its author and year with a `u8` each, so 255 is what the
/// container can express; ids run `0..=254`.
///
/// This used to be 64, the width of the `u64` the matcher used as a bitset —
/// a limit belonging to one implementation detail rather than to the format.
/// It cost twice: the builder once guarded at 255 and produced indexes the
/// module refused, which showed up as search silently falling back to the
/// JavaScript scan; and the ceiling sat 19 authors above a corpus that gains a
/// few every release. The matcher now sizes its bitset to the pool, so the
/// only limit left is the one the bytes impose.
pub const MAX_POOL: u32 = 255;

/// Read a little-endian `u32`.
pub fn u32_le(bytes: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]])
}

/// Read a little-endian `u16`.
pub fn u16_le(bytes: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([bytes[at], bytes[at + 1]])
}
