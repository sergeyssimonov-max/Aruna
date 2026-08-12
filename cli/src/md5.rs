//! MD5 (RFC 1321), streaming, no dependencies.
//!
//! Zenodo publishes an MD5 for every file in a record, so that is the digest a
//! downloaded archive can be checked against — the choice is the registry's,
//! not ours. Nothing here is security-sensitive: MD5 answers "did the bytes
//! arrive intact", not "did an adversary swap them".
//!
//! Hashing is incremental so the 71 MiB archive is digested as it streams to
//! disk, without a second pass over the file.

/// Per-round left-rotation amounts.
const SHIFTS: [u32; 64] = [
    7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, //
    5, 9, 14, 20, 5, 9, 14, 20, 5, 9, 14, 20, 5, 9, 14, 20, //
    4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, //
    6, 10, 15, 21, 6, 10, 15, 21, 6, 10, 15, 21, 6, 10, 15, 21,
];

/// Round constants: `floor(abs(sin(i + 1)) * 2^32)`.
const SINE: [u32; 64] = [
    0xd76aa478, 0xe8c7b756, 0x242070db, 0xc1bdceee, 0xf57c0faf, 0x4787c62a, 0xa8304613, 0xfd469501,
    0x698098d8, 0x8b44f7af, 0xffff5bb1, 0x895cd7be, 0x6b901122, 0xfd987193, 0xa679438e, 0x49b40821,
    0xf61e2562, 0xc040b340, 0x265e5a51, 0xe9b6c7aa, 0xd62f105d, 0x02441453, 0xd8a1e681, 0xe7d3fbc8,
    0x21e1cde6, 0xc33707d6, 0xf4d50d87, 0x455a14ed, 0xa9e3e905, 0xfcefa3f8, 0x676f02d9, 0x8d2a4c8a,
    0xfffa3942, 0x8771f681, 0x6d9d6122, 0xfde5380c, 0xa4beea44, 0x4bdecfa9, 0xf6bb4b60, 0xbebfbc70,
    0x289b7ec6, 0xeaa127fa, 0xd4ef3085, 0x04881d05, 0xd9d4d039, 0xe6db99e5, 0x1fa27cf8, 0xc4ac5665,
    0xf4292244, 0x432aff97, 0xab9423a7, 0xfc93a039, 0x655b59c3, 0x8f0ccc92, 0xffeff47d, 0x85845dd1,
    0x6fa87e4f, 0xfe2ce6e0, 0xa3014314, 0x4e0811a1, 0xf7537e82, 0xbd3af235, 0x2ad7d2bb, 0xeb86d391,
];

/// Incremental MD5 state.
pub struct Md5 {
    state: [u32; 4],
    /// Total message length in bytes; the padding encodes it in bits.
    len: u64,
    /// Bytes of the block currently being filled.
    block: [u8; 64],
    filled: usize,
}

impl Default for Md5 {
    fn default() -> Self {
        Self::new()
    }
}

impl Md5 {
    pub fn new() -> Self {
        Md5 {
            state: [0x6745_2301, 0xefcd_ab89, 0x98ba_dcfe, 0x1032_5476],
            len: 0,
            block: [0; 64],
            filled: 0,
        }
    }

    /// Feed the next slice of the message. Any chunking gives the same digest.
    pub fn update(&mut self, mut data: &[u8]) {
        self.len = self.len.wrapping_add(data.len() as u64);

        if self.filled > 0 {
            let need = 64 - self.filled;
            let take = need.min(data.len());
            self.block[self.filled..self.filled + take].copy_from_slice(&data[..take]);
            self.filled += take;
            data = &data[take..];
            if self.filled < 64 {
                // Still short of a block, and `data` is now empty — keep the
                // partial block for the next call rather than falling through
                // to the tail below, which would overwrite it.
                return;
            }
            let block = self.block;
            self.compress(&block);
            self.filled = 0;
        }

        let mut chunks = data.chunks_exact(64);
        for chunk in &mut chunks {
            let mut block = [0u8; 64];
            block.copy_from_slice(chunk);
            self.compress(&block);
        }

        let rest = chunks.remainder();
        self.block[..rest.len()].copy_from_slice(rest);
        self.filled = rest.len();
    }

    /// Finish the message and return the digest as lowercase hex.
    pub fn finish_hex(mut self) -> String {
        let bit_len = self.len.wrapping_mul(8);

        // Padding: a single 1 bit, zeroes, then the length in bits (LE).
        self.pad_byte(0x80);
        while self.filled != 56 {
            self.pad_byte(0x00);
        }
        let len_bytes = bit_len.to_le_bytes();
        for b in len_bytes {
            self.pad_byte(b);
        }
        debug_assert_eq!(self.filled, 0, "padding must end on a block boundary");

        let mut hex = String::with_capacity(32);
        for word in self.state {
            for byte in word.to_le_bytes() {
                hex.push(nibble(byte >> 4));
                hex.push(nibble(byte & 0x0f));
            }
        }
        hex
    }

    /// Append one padding byte, compressing whenever a block completes.
    ///
    /// Padding is not part of the message, so `len` must not move here.
    fn pad_byte(&mut self, byte: u8) {
        self.block[self.filled] = byte;
        self.filled += 1;
        if self.filled == 64 {
            let block = self.block;
            self.compress(&block);
            self.filled = 0;
        }
    }

    fn compress(&mut self, block: &[u8; 64]) {
        let mut m = [0u32; 16];
        for (word, bytes) in m.iter_mut().zip(block.chunks_exact(4)) {
            *word = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
        }

        let [mut a, mut b, mut c, mut d] = self.state;
        for i in 0..64 {
            let (mix, index) = match i / 16 {
                0 => ((b & c) | (!b & d), i),
                1 => ((d & b) | (!d & c), (5 * i + 1) % 16),
                2 => (b ^ c ^ d, (3 * i + 5) % 16),
                _ => (c ^ (b | !d), (7 * i) % 16),
            };
            let mix = mix
                .wrapping_add(a)
                .wrapping_add(SINE[i])
                .wrapping_add(m[index]);
            a = d;
            d = c;
            c = b;
            b = b.wrapping_add(mix.rotate_left(SHIFTS[i]));
        }

        self.state[0] = self.state[0].wrapping_add(a);
        self.state[1] = self.state[1].wrapping_add(b);
        self.state[2] = self.state[2].wrapping_add(c);
        self.state[3] = self.state[3].wrapping_add(d);
    }
}

fn nibble(v: u8) -> char {
    char::from(if v < 10 { b'0' + v } else { b'a' + (v - 10) })
}

/// Digest a whole slice at once.
pub fn md5_hex(data: &[u8]) -> String {
    let mut h = Md5::new();
    h.update(data);
    h.finish_hex()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The test suite from RFC 1321, appendix A.5.
    #[test]
    fn rfc1321_vectors() {
        assert_eq!(md5_hex(b""), "d41d8cd98f00b204e9800998ecf8427e");
        assert_eq!(md5_hex(b"a"), "0cc175b9c0f1b6a831c399e269772661");
        assert_eq!(md5_hex(b"abc"), "900150983cd24fb0d6963f7d28e17f72");
        assert_eq!(
            md5_hex(b"message digest"),
            "f96b697d7cb7938d525a2f31aaf161d0"
        );
        assert_eq!(
            md5_hex(b"abcdefghijklmnopqrstuvwxyz"),
            "c3fcd3d76192e4007dfb496cca67e13b"
        );
        assert_eq!(
            md5_hex(b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789"),
            "d174ab98d277d9f5a5611c2c9f419d9f"
        );
        assert_eq!(
            md5_hex(
                b"12345678901234567890123456789012345678901234567890123456789012345678901234567890"
            ),
            "57edf4a22be3c955ac49da2e2107b67a"
        );
    }

    /// Chunking must not change the digest — the download feeds 64 KiB reads,
    /// which never line up with the 64-byte block boundary.
    #[test]
    fn chunking_does_not_change_digest() {
        let data: Vec<u8> = (0..10_000u32).map(|i| (i % 251) as u8).collect();
        let one_shot = md5_hex(&data);

        for chunk in [1usize, 7, 63, 64, 65, 1000] {
            let mut h = Md5::new();
            for part in data.chunks(chunk) {
                h.update(part);
            }
            assert_eq!(h.finish_hex(), one_shot, "chunk size {chunk}");
        }
    }

    /// Lengths around the padding boundary (55/56/57 mod 64) are where naive
    /// implementations break.
    #[test]
    fn padding_boundaries() {
        for len in [54usize, 55, 56, 57, 63, 64, 65, 119, 120, 128] {
            let data = vec![b'x'; len];
            let mut h = Md5::new();
            for part in data.chunks(9) {
                h.update(part);
            }
            assert_eq!(h.finish_hex(), md5_hex(&data), "length {len}");
        }
    }
}
