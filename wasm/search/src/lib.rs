//! Compact inventory search (TLH2) — production matcher selection.
//!
//! ## Why this hybrid (from release benches on short haystacks ≈ inventory sigla)
//! | Needle | Winner | Reason |
//! |--------|--------|--------|
//! | m = 1  | memchr-style | LLVM/byte scan beats automata setup |
//! | m = 2..=3 | unrolled window | SO preprocess not amortized on n≲100 |
//! | m ≥ 4  | **Boyer–Moore–Horspool** | 3–7× over naive; GS full-BM loses on short n |
//!
//! Full Boyer–Moore (δ₁+good-suffix) was measured **slower** than Horspool alone
//! for our haystack lengths (≤104 B): GS build cost is never paid back.
//!
//! ## Index-level accelerators (kept — dominate end-to-end search)
//! 1. Auth / year `u64` bitsets (pools ≤64)
//! 2. Per-sig character presence bloom (`u64`)
//! 3. TLH2 compact pools (deduped sigla, u8 auth/year)
//!
//! # Index layout (TLH2, little-endian)
//! ```text
//! header 32 B | groups 8 B | items 8 B | auth_dir 4 B | year_dir 4 B | pools
//! ```

#![allow(clippy::missing_safety_doc)]

use std::sync::Mutex;

const MAGIC: u32 = 0x3248_4C54; // TLH2
const HEADER: usize = 32;
const GROUP_STRIDE: usize = 8;
const ITEM_STRIDE: usize = 8;
const DIR_STRIDE: usize = 4;
const RESULT_STRIDE: usize = 12;

static INDEX: Mutex<Option<IndexView>> = Mutex::new(None);

struct IndexView {
    bytes: Vec<u8>,
    n_groups: u32,
    n_items: u32,
    n_auth: u32,
    n_year: u32,
    groups_off: usize,
    items_off: usize,
    auth_dir_off: usize,
    year_dir_off: usize,
    sig_pool_off: usize,
    auth_pool_off: usize,
    year_pool_off: usize,
    /// Presence bloom per item (`1 << (b & 63)`).
    sig_masks: Vec<u64>,
}

impl IndexView {
    fn parse(bytes: Vec<u8>) -> Result<Self, ()> {
        if bytes.len() < HEADER {
            return Err(());
        }
        let magic = u32_le(&bytes, 0)?;
        if magic != MAGIC {
            return Err(());
        }
        let n_groups = u32_le(&bytes, 4)?;
        let n_items = u32_le(&bytes, 8)?;
        let n_auth = u32_le(&bytes, 12)?;
        let n_year = u32_le(&bytes, 16)?;
        let sig_pool_len = u32_le(&bytes, 20)? as usize;
        let auth_pool_len = u32_le(&bytes, 24)? as usize;
        let year_pool_len = u32_le(&bytes, 28)? as usize;

        if n_auth > 64 || n_year > 64 {
            return Err(());
        }

        let groups_off = HEADER;
        let items_off = groups_off + n_groups as usize * GROUP_STRIDE;
        let auth_dir_off = items_off + n_items as usize * ITEM_STRIDE;
        let year_dir_off = auth_dir_off + n_auth as usize * DIR_STRIDE;
        let sig_pool_off = year_dir_off + n_year as usize * DIR_STRIDE;
        let auth_pool_off = sig_pool_off + sig_pool_len;
        let year_pool_off = auth_pool_off + auth_pool_len;
        let need = year_pool_off + year_pool_len;
        if bytes.len() < need {
            return Err(());
        }

        let mut view = Self {
            bytes,
            n_groups,
            n_items,
            n_auth,
            n_year,
            groups_off,
            items_off,
            auth_dir_off,
            year_dir_off,
            sig_pool_off,
            auth_pool_off,
            year_pool_off,
            sig_masks: Vec::new(),
        };
        view.sig_masks = view.build_sig_masks();
        Ok(view)
    }

    fn build_sig_masks(&self) -> Vec<u64> {
        let n = self.n_items as usize;
        let mut masks = vec![0u64; n];
        for i in 0..n {
            let (off, len, _, _) = self.item(i);
            masks[i] = char_mask(self.sig(off, len));
        }
        masks
    }

    #[inline]
    fn group(&self, gi: usize) -> (u16, u16, u32) {
        let o = self.groups_off + gi * GROUP_STRIDE;
        let b = &self.bytes;
        (
            u16::from_le_bytes([b[o], b[o + 1]]),
            u16::from_le_bytes([b[o + 2], b[o + 3]]),
            u32::from_le_bytes([b[o + 4], b[o + 5], b[o + 6], b[o + 7]]),
        )
    }

    #[inline]
    fn item(&self, ii: usize) -> (u32, u8, u8, u8) {
        let o = self.items_off + ii * ITEM_STRIDE;
        let b = &self.bytes;
        (
            u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]]),
            b[o + 4],
            b[o + 5],
            b[o + 6],
        )
    }

    #[inline]
    fn auth_entry(&self, i: usize) -> (u16, u16) {
        let o = self.auth_dir_off + i * DIR_STRIDE;
        let b = &self.bytes;
        (
            u16::from_le_bytes([b[o], b[o + 1]]),
            u16::from_le_bytes([b[o + 2], b[o + 3]]),
        )
    }

    #[inline]
    fn year_entry(&self, i: usize) -> (u16, u16) {
        let o = self.year_dir_off + i * DIR_STRIDE;
        let b = &self.bytes;
        (
            u16::from_le_bytes([b[o], b[o + 1]]),
            u16::from_le_bytes([b[o + 2], b[o + 3]]),
        )
    }

    #[inline]
    fn sig(&self, off: u32, len: u8) -> &[u8] {
        let s = self.sig_pool_off + off as usize;
        &self.bytes[s..s + len as usize]
    }

    #[inline]
    fn auth_str(&self, i: u8) -> &[u8] {
        let (off, len) = self.auth_entry(i as usize);
        let s = self.auth_pool_off + off as usize;
        &self.bytes[s..s + len as usize]
    }

    #[inline]
    fn year_str(&self, i: u8) -> &[u8] {
        let (off, len) = self.year_entry(i as usize);
        let s = self.year_pool_off + off as usize;
        &self.bytes[s..s + len as usize]
    }
}

#[inline]
fn u32_le(b: &[u8], o: usize) -> Result<u32, ()> {
    b.get(o..o + 4)
        .and_then(|s| s.try_into().ok())
        .map(u32::from_le_bytes)
        .ok_or(())
}

#[inline]
fn char_mask(s: &[u8]) -> u64 {
    let mut m = 0u64;
    for &b in s {
        m |= 1u64 << (b & 63);
    }
    m
}

#[inline]
fn write_cth_label(n: u16, buf: &mut [u8; 16]) -> usize {
    buf[0] = b'c';
    buf[1] = b't';
    buf[2] = b'h';
    buf[3] = b' ';
    let mut x = n as u32;
    let mut tmp = [0u8; 5];
    let mut nd = 0;
    if x == 0 {
        tmp[0] = b'0';
        nd = 1;
    } else {
        while x > 0 {
            tmp[nd] = b'0' + (x % 10) as u8;
            x /= 10;
            nd += 1;
        }
    }
    for i in 0..nd {
        buf[4 + i] = tmp[nd - 1 - i];
    }
    4 + nd
}

// ── exports ─────────────────────────────────────────────────────

#[no_mangle]
pub extern "C" fn alloc(n: usize) -> *mut u8 {
    if n == 0 {
        return core::ptr::null_mut();
    }
    let mut v = Vec::<u8>::with_capacity(n);
    let ptr = v.as_mut_ptr();
    core::mem::forget(v);
    ptr
}

#[no_mangle]
pub unsafe extern "C" fn dealloc(ptr: *mut u8, n: usize) {
    if ptr.is_null() || n == 0 {
        return;
    }
    let _ = Vec::from_raw_parts(ptr, n, n);
}

#[no_mangle]
pub unsafe extern "C" fn init(ptr: *const u8, len: usize) -> u32 {
    if ptr.is_null() || len == 0 {
        return 0;
    }
    let slice = core::slice::from_raw_parts(ptr, len);
    match IndexView::parse(slice.to_vec()) {
        Ok(view) => {
            if let Ok(mut g) = INDEX.lock() {
                *g = Some(view);
                1
            } else {
                0
            }
        }
        Err(()) => 0,
    }
}

#[no_mangle]
pub extern "C" fn reset() {
    if let Ok(mut g) = INDEX.lock() {
        *g = None;
    }
}

#[no_mangle]
pub unsafe extern "C" fn stats(out_ptr: *mut u8) -> u32 {
    if out_ptr.is_null() {
        return 0;
    }
    let Ok(guard) = INDEX.lock() else {
        return 0;
    };
    let Some(index) = guard.as_ref() else {
        return 0;
    };
    let out = core::slice::from_raw_parts_mut(out_ptr, 16);
    out[0..4].copy_from_slice(&index.n_groups.to_le_bytes());
    out[4..8].copy_from_slice(&index.n_items.to_le_bytes());
    out[8..12].copy_from_slice(&index.n_auth.to_le_bytes());
    out[12..16].copy_from_slice(&index.n_year.to_le_bytes());
    1
}

/// Matcher plan for one query — tables built once, reused over all items.
enum Plan {
    Empty,
    Byte(u8),
    Short2([u8; 2]),
    Short3([u8; 3]),
    /// Horspool: pattern + compact u8 skip table.
    Bmh { needle: *const u8, len: usize, skip: [u8; 256] },
}

// needle pointer in Bmh is only valid for the duration of search() — we store
// the query slice lifetime on the stack of search and never return Plan.
#[no_mangle]
pub unsafe extern "C" fn search(
    q_ptr: *const u8,
    q_len: usize,
    out_ptr: *mut u8,
    out_cap: usize,
) -> u32 {
    if out_ptr.is_null() || out_cap < 4 {
        return 0;
    }
    let Ok(guard) = INDEX.lock() else {
        return 0;
    };
    let Some(index) = guard.as_ref() else {
        return 0;
    };

    let out = core::slice::from_raw_parts_mut(out_ptr, out_cap);
    let max_entries = (out_cap - 4) / RESULT_STRIDE;

    if q_len == 0 || q_ptr.is_null() {
        let n = (index.n_groups as usize).min(max_entries);
        out[0..4].copy_from_slice(&(n as u32).to_le_bytes());
        for gi in 0..n {
            write_entry(out, gi, gi as u32, 0, 0);
        }
        return n as u32;
    }

    let query = core::slice::from_raw_parts(q_ptr, q_len);
    let q_mask = char_mask(query);
    let qlen = query.len();

    // Auth / year bitsets — O(pool), not O(items).
    let mut auth_bits: u64 = 0;
    for i in 0..index.n_auth as u32 {
        if contains(index.auth_str(i as u8), query) {
            auth_bits |= 1u64 << i;
        }
    }
    let mut year_bits: u64 = 0;
    for i in 0..index.n_year as u32 {
        if contains(index.year_str(i as u8), query) {
            year_bits |= 1u64 << i;
        }
    }
    let any_meta = auth_bits != 0 || year_bits != 0;

    // Build matcher once.
    let plan = match qlen {
        0 => Plan::Empty,
        1 => Plan::Byte(query[0]),
        2 => Plan::Short2([query[0], query[1]]),
        3 => Plan::Short3([query[0], query[1], query[2]]),
        _ if qlen <= 255 => Plan::Bmh {
            needle: query.as_ptr(),
            len: qlen,
            skip: bmh_skip_table(query),
        },
        _ => Plan::Bmh {
            // Extremely long query: still Horspool but skip table saturates at 255.
            needle: query.as_ptr(),
            len: qlen,
            skip: bmh_skip_table(&query[..255.min(qlen)]),
        },
    };

    let mut count = 0usize;
    let mut cth_buf = [0u8; 16];
    let masks = index.sig_masks.as_slice();

    for gi in 0..index.n_groups as usize {
        if count >= max_entries {
            break;
        }
        let (cth, item_count, item_start) = index.group(gi);
        let lab_len = write_cth_label(cth, &mut cth_buf);
        if match_plan(&cth_buf[..lab_len], query, &plan) {
            write_entry(out, count, gi as u32, 0, 0);
            count += 1;
            continue;
        }

        let start = item_start as usize;
        let end = start + item_count as usize;
        for ii in start..end {
            if count >= max_entries {
                break;
            }
            let (sig_off, sig_len, auth, year) = index.item(ii);

            if any_meta {
                let a_hit = (auth_bits >> auth) & 1 != 0;
                let y_hit = (year_bits >> year) & 1 != 0;
                if a_hit || y_hit {
                    write_entry(out, count, gi as u32, 1, (ii - start) as u32);
                    count += 1;
                    continue;
                }
            }

            // Character-presence prefilter.
            let sm = unsafe { *masks.get_unchecked(ii) };
            if sm & q_mask != q_mask {
                continue;
            }

            let sig = index.sig(sig_off, sig_len);
            if sig.len() < qlen {
                continue;
            }
            if match_plan(sig, query, &plan) {
                write_entry(out, count, gi as u32, 1, (ii - start) as u32);
                count += 1;
            }
        }
    }

    out[0..4].copy_from_slice(&(count as u32).to_le_bytes());
    count as u32
}

#[inline]
fn write_entry(out: &mut [u8], idx: usize, gi: u32, kind: u32, extra: u32) {
    let o = 4 + idx * RESULT_STRIDE;
    out[o..o + 4].copy_from_slice(&gi.to_le_bytes());
    out[o + 4..o + 8].copy_from_slice(&kind.to_le_bytes());
    out[o + 8..o + 12].copy_from_slice(&extra.to_le_bytes());
}

// ── matchers (optimal hybrid) ───────────────────────────────────

#[inline]
fn match_plan(hay: &[u8], needle: &[u8], plan: &Plan) -> bool {
    match *plan {
        Plan::Empty => true,
        Plan::Byte(b) => hay.contains(&b),
        Plan::Short2(p) => contains_2(hay, p),
        Plan::Short3(p) => contains_3(hay, p),
        Plan::Bmh {
            needle: nptr,
            len,
            ref skip,
        } => {
            // Reconstruct needle slice from the plan pointer (valid during search).
            let n = unsafe { core::slice::from_raw_parts(nptr, len) };
            // If we truncated skip build for absurd lengths, fall back.
            if len > 255 {
                return baseline_contains(hay, needle);
            }
            debug_assert_eq!(n, needle);
            bmh_search(hay, n, skip)
        }
    }
}

/// Public routing used by auth/year prefilter and tests.
#[inline]
fn contains(hay: &[u8], needle: &[u8]) -> bool {
    let m = needle.len();
    if m == 0 {
        return true;
    }
    if m > hay.len() {
        return false;
    }
    match m {
        1 => hay.contains(&needle[0]),
        2 => contains_2(hay, [needle[0], needle[1]]),
        3 => contains_3(hay, [needle[0], needle[1], needle[2]]),
        _ if m <= 255 => {
            let skip = bmh_skip_table(needle);
            bmh_search(hay, needle, &skip)
        }
        _ => baseline_contains(hay, needle),
    }
}

#[inline]
fn contains_2(hay: &[u8], p: [u8; 2]) -> bool {
    if hay.len() < 2 {
        return false;
    }
    let end = hay.len() - 1;
    let mut i = 0;
    while i < end {
        if hay[i] == p[0] && hay[i + 1] == p[1] {
            return true;
        }
        i += 1;
    }
    false
}

#[inline]
fn contains_3(hay: &[u8], p: [u8; 3]) -> bool {
    if hay.len() < 3 {
        return false;
    }
    let end = hay.len() - 2;
    let mut i = 0;
    while i < end {
        if hay[i] == p[0] && hay[i + 1] == p[1] && hay[i + 2] == p[2] {
            return true;
        }
        i += 1;
    }
    false
}

// ── Boyer–Moore–Horspool (primary for m ≥ 4) ────────────────────

/// Compact **256-byte** skip table (not usize×256).
#[inline]
fn bmh_skip_table(needle: &[u8]) -> [u8; 256] {
    let m = needle.len();
    debug_assert!(m >= 1 && m <= 255);
    let m_u8 = m as u8;
    let mut skip = [m_u8; 256];
    let last = m - 1;
    for (i, &c) in needle[..last].iter().enumerate() {
        unsafe {
            *skip.get_unchecked_mut(c as usize) = (last - i) as u8;
        }
    }
    skip
}

#[inline]
fn bmh_search(hay: &[u8], needle: &[u8], skip: &[u8; 256]) -> bool {
    let m = needle.len();
    let n = hay.len();
    if m == 0 {
        return true;
    }
    if m > n {
        return false;
    }
    let last = m - 1;
    let last_c = unsafe { *needle.get_unchecked(last) };
    let mut i = last;
    while i < n {
        let hc = unsafe { *hay.get_unchecked(i) };
        if hc == last_c {
            let mut j = last;
            loop {
                if j == 0 {
                    return true;
                }
                j -= 1;
                if unsafe { *hay.get_unchecked(i - (last - j)) }
                    != unsafe { *needle.get_unchecked(j) }
                {
                    break;
                }
            }
        }
        i += unsafe { *skip.get_unchecked(hc as usize) as usize };
    }
    false
}

// ── baseline oracle ─────────────────────────────────────────────

#[inline]
fn baseline_contains(hay: &[u8], needle: &[u8]) -> bool {
    let m = needle.len();
    if m == 0 {
        return true;
    }
    if m > hay.len() {
        return false;
    }
    if m == 1 {
        return hay.contains(&needle[0]);
    }
    hay.windows(m).any(|w| w == needle)
}

#[inline]
#[cfg(test)]
fn path_bmh(hay: &[u8], needle: &[u8]) -> bool {
    if needle.is_empty() {
        return true;
    }
    if needle.len() > hay.len() || needle.len() > 255 {
        return baseline_contains(hay, needle);
    }
    let skip = bmh_skip_table(needle);
    bmh_search(hay, needle, &skip)
}

// ── tests ───────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    fn build_tiny() -> Vec<u8> {
        let sigs = [b"kbo 3.22".as_slice(), b"kbo 22.5".as_slice()];
        let auth_pool = b"ls";
        let year_pool = b"2021";

        let mut sig_pool = Vec::new();
        let s0_off = 0u32;
        sig_pool.extend_from_slice(sigs[0]);
        let s1_off = sig_pool.len() as u32;
        sig_pool.extend_from_slice(sigs[1]);

        let mut buf = Vec::new();
        buf.extend_from_slice(&MAGIC.to_le_bytes());
        buf.extend_from_slice(&1u32.to_le_bytes());
        buf.extend_from_slice(&2u32.to_le_bytes());
        buf.extend_from_slice(&1u32.to_le_bytes());
        buf.extend_from_slice(&1u32.to_le_bytes());
        buf.extend_from_slice(&(sig_pool.len() as u32).to_le_bytes());
        buf.extend_from_slice(&(auth_pool.len() as u32).to_le_bytes());
        buf.extend_from_slice(&(year_pool.len() as u32).to_le_bytes());
        buf.extend_from_slice(&1u16.to_le_bytes());
        buf.extend_from_slice(&2u16.to_le_bytes());
        buf.extend_from_slice(&0u32.to_le_bytes());
        buf.extend_from_slice(&s0_off.to_le_bytes());
        buf.push(sigs[0].len() as u8);
        buf.push(0);
        buf.push(0);
        buf.push(0);
        buf.extend_from_slice(&s1_off.to_le_bytes());
        buf.push(sigs[1].len() as u8);
        buf.push(0);
        buf.push(0);
        buf.push(0);
        buf.extend_from_slice(&0u16.to_le_bytes());
        buf.extend_from_slice(&(auth_pool.len() as u16).to_le_bytes());
        buf.extend_from_slice(&0u16.to_le_bytes());
        buf.extend_from_slice(&(year_pool.len() as u16).to_le_bytes());
        buf.extend_from_slice(&sig_pool);
        buf.extend_from_slice(auth_pool);
        buf.extend_from_slice(year_pool);
        buf
    }

    fn corpus() -> Vec<(Vec<u8>, Vec<u8>)> {
        let hays: &[&[u8]] = &[
            b"",
            b"a",
            b"kbo 3.22",
            b"kbo 22.5",
            b"KBo 26.25 (sumerisch-akkadisch-hethitisch)",
            b"xxxsumerisch-akkadischyyy",
            b"the quick brown fox jumps over the lazy dog",
            b"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            b"ababababababababababab",
        ];
        let needles: &[&[u8]] = &[
            b"",
            b"a",
            b"bo",
            b"kbo",
            b"3.22",
            b"22.5",
            b"fox",
            b"sumerisch-akkadisch",
            b"zzzz",
            b"not-in-any-haystack-at-all!!",
        ];
        let mut out = Vec::new();
        for h in hays {
            for n in needles {
                out.push((h.to_vec(), n.to_vec()));
            }
            out.push((h.to_vec(), h.to_vec()));
            if h.len() >= 2 {
                out.push((h.to_vec(), h[..h.len() / 2].to_vec()));
                out.push((h.to_vec(), h[h.len() / 2..].to_vec()));
            }
        }
        out
    }

    #[test]
    fn optimised_matches_baseline() {
        for (hay, needle) in corpus() {
            let want = baseline_contains(&hay, &needle);
            let got = contains(&hay, &needle);
            assert_eq!(got, want, "hay={hay:?} needle={needle:?}");
            if !needle.is_empty() && needle.len() <= 255 {
                assert_eq!(path_bmh(&hay, &needle), want);
            }
        }
    }

    #[test]
    fn bmh_skip_rightmost_excluding_last() {
        let n = b"abca";
        let skip = bmh_skip_table(n);
        assert_eq!(skip[b'a' as usize], 3);
        assert_eq!(skip[b'b' as usize], 2);
        assert_eq!(skip[b'c' as usize], 1);
        assert_eq!(skip[b'z' as usize], 4);
    }

    #[test]
    fn short_paths() {
        assert!(contains(b"abc", b"a"));
        assert!(contains(b"abc", b"bc"));
        assert!(contains(b"abc", b"abc"));
        assert!(!contains(b"abc", b"x"));
        assert!(contains(b"kbo 3.22", b"3.22"));
        assert!(contains(
            b"xxsumerisch-akkadischyy",
            b"sumerisch-akkadisch"
        ));
    }

    #[test]
    fn finds_by_siglum() {
        let blob = build_tiny();
        reset();
        unsafe {
            assert_eq!(init(blob.as_ptr(), blob.len()), 1);
            let q = b"3.22";
            let mut out = vec![0u8; 4 + 12 * 8];
            let n = search(q.as_ptr(), q.len(), out.as_mut_ptr(), out.len());
            assert_eq!(n, 1);
            assert_eq!(u32::from_le_bytes(out[8..12].try_into().unwrap()), 1);
        }
    }

    #[test]
    fn finds_by_auth_bitset() {
        let blob = build_tiny();
        reset();
        unsafe {
            assert_eq!(init(blob.as_ptr(), blob.len()), 1);
            let q = b"ls";
            let mut out = vec![0u8; 4 + 12 * 8];
            let n = search(q.as_ptr(), q.len(), out.as_mut_ptr(), out.len());
            assert_eq!(n, 2);
        }
    }

    #[test]
    fn finds_group_by_cth() {
        let blob = build_tiny();
        reset();
        unsafe {
            assert_eq!(init(blob.as_ptr(), blob.len()), 1);
            let q = b"cth 1";
            let mut out = vec![0u8; 4 + 12 * 8];
            let n = search(q.as_ptr(), q.len(), out.as_mut_ptr(), out.len());
            assert_eq!(n, 1);
            assert_eq!(u32::from_le_bytes(out[8..12].try_into().unwrap()), 0);
        }
    }

    #[test]
    fn full_search_matches_baseline_scan() {
        let blob = build_tiny();
        reset();
        unsafe {
            assert_eq!(init(blob.as_ptr(), blob.len()), 1);
        }
        let items: &[&[u8]] = &[b"kbo 3.22", b"kbo 22.5"];
        let queries: &[&[u8]] = &[
            b"3.22", b"22.5", b"kbo", b"ls", b"2021", b"cth 1", b"zzz", b"2",
        ];
        for q in queries {
            let mut out = vec![0u8; 4 + 12 * 16];
            let n = unsafe { search(q.as_ptr(), q.len(), out.as_mut_ptr(), out.len()) };
            let mut base_hits = 0u32;
            if baseline_contains(b"cth 1", q) {
                base_hits = 1;
            } else {
                for sig in items {
                    if baseline_contains(sig, q)
                        || baseline_contains(b"ls", q)
                        || baseline_contains(b"2021", q)
                    {
                        base_hits += 1;
                    }
                }
            }
            assert_eq!(n, base_hits, "query={q:?}");
        }
    }

    /// Release: `cargo test --release bench_optimal_vs_baseline -- --nocapture`
    #[test]
    fn bench_optimal_vs_baseline() {
        let mut hays: Vec<Vec<u8>> = Vec::new();
        for i in 0..5000 {
            hays.push(format!("kbo {}.{} (fragment-note-{i})", i % 50, i % 30).into_bytes());
        }
        for i in 0..2000 {
            hays.push(format!("kub {}.{i}", i % 100).into_bytes());
        }
        for i in 0..200 {
            hays.push(
                format!("kbo {i}.25 (sumerisch-akkadisch-hethitisch; note-{i})").into_bytes(),
            );
        }

        let needles: &[&[u8]] = &[
            b"a",
            b"kbo",
            b"3.22",
            b"fragment",
            b"sumerisch-akkadisch",
            b"zzzz-miss-xx",
        ];

        fn time_ms(iters: u32, mut f: impl FnMut()) -> f64 {
            for _ in 0..2 {
                f();
            }
            let t0 = Instant::now();
            for _ in 0..iters {
                f();
            }
            t0.elapsed().as_secs_f64() * 1000.0 / iters as f64
        }

        println!();
        println!(
            "{:<22} {:>10} {:>10} {:>8}",
            "needle", "baseline", "optimal", "speedup"
        );
        println!("{}", "-".repeat(54));

        for needle in needles {
            let mut sink = 0u64;
            let base = time_ms(12, || {
                for h in &hays {
                    if baseline_contains(h, needle) {
                        sink = sink.wrapping_add(1);
                    }
                }
            });
            let opt = time_ms(12, || {
                for h in &hays {
                    if contains(h, needle) {
                        sink = sink.wrapping_add(1);
                    }
                }
            });
            let speedup = if opt > 0.0 { base / opt } else { 0.0 };
            println!(
                "{:<22} {:>8.3}ms {:>8.3}ms {:>7.2}x",
                String::from_utf8_lossy(needle),
                base,
                opt,
                speedup
            );
            for h in hays.iter().step_by(97) {
                assert_eq!(contains(h, needle), baseline_contains(h, needle));
            }
            let _ = sink;
        }
        println!("{}", "-".repeat(54));
    }
}
