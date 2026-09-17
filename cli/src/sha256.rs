//! SHA-256 (FIPS 180-4), streaming, no dependencies.
//!
//! The second digest this crate carries, and it exists for a different question
//! than [`crate::md5`]. MD5 answers Zenodo: the registry publishes an MD5 per
//! file, so that is what a downloaded archive is compared against, and the
//! choice is the registry's. SHA-256 answers the fonts: `docs/FONTS.md` records
//! one per shipped file, taken from the release each font came out of, and
//! those are the digests a reader can check for themselves against upstream.
//!
//! Written here rather than added as a dependency for the same reason MD5 was:
//! the algorithm is a page of arithmetic with published test vectors, and a
//! font-integrity check is not a place to widen the supply chain (5.2). The
//! vectors of FIPS 180-4 and the seven digests of `docs/FONTS.md` are both
//! checked below, so the implementation is held by something that did not come
//! from it.
//!
//! Hashing is incremental: a 838 KiB font is digested as it is read, and the
//! same code serves a byte slice compiled into the binary.

/// Bytes per compression block, as SHA-256 defines it.
const BLOCK: usize = 64;

/// Where the message length goes in the final block: the last eight bytes.
const LENGTH_AT: usize = BLOCK - 8;

/// Round constants: the first 32 bits of the fractional parts of the cube roots
/// of the first sixty-four primes.
const K: [u32; 64] = [
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
    0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
    0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
    0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
    0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
    0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
    0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
    0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
];

/// A SHA-256 in progress: the eight-word state, the bytes not yet a full block,
/// and how many bytes have been fed in altogether.
pub struct Sha256 {
    state: [u32; 8],
    buffer: [u8; BLOCK],
    buffered: usize,
    length: u64,
}

impl Default for Sha256 {
    fn default() -> Self {
        Self::new()
    }
}

impl Sha256 {
    /// The initial state: the first 32 bits of the fractional parts of the
    /// square roots of the first eight primes.
    pub fn new() -> Self {
        Sha256 {
            state: [
                0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
                0x5be0cd19,
            ],
            buffer: [0; BLOCK],
            buffered: 0,
            length: 0,
        }
    }

    /// Feed bytes in. Any number, any number of times.
    pub fn update(&mut self, mut data: &[u8]) {
        self.length = self.length.wrapping_add(data.len() as u64);

        if self.buffered > 0 {
            let want = BLOCK - self.buffered;
            let take = want.min(data.len());
            self.buffer[self.buffered..self.buffered + take].copy_from_slice(&data[..take]);
            self.buffered += take;
            data = &data[take..];
            if self.buffered < BLOCK {
                // The call did not fill the block, so it had nothing left over
                // and the partial block below must not be overwritten — which
                // is exactly what feeding this hasher one byte at a time does.
                return;
            }
            let block = self.buffer;
            self.compress(&block);
            self.buffered = 0;
        }

        let mut chunks = data.chunks_exact(BLOCK);
        for chunk in &mut chunks {
            let mut block = [0u8; BLOCK];
            block.copy_from_slice(chunk);
            self.compress(&block);
        }

        let rest = chunks.remainder();
        self.buffer[..rest.len()].copy_from_slice(rest);
        self.buffered = rest.len();
    }

    /// Close the message and return the digest as lowercase hexadecimal.
    pub fn finish_hex(mut self) -> String {
        // The padding SHA-256 prescribes: a single one bit, then zeroes, then
        // the message length in bits as a big-endian 64-bit number.
        let bits = self.length.wrapping_mul(8);
        let mut tail = [0u8; BLOCK * 2];
        tail[0] = 0x80;
        // The one bit is a byte of its own, so the zeroes are counted from
        // after it: enough of them to land the length field at `LENGTH_AT` of
        // some block. Counting from before it puts the length on top of the
        // one bit when a message ends exactly `LENGTH_AT` bytes into a block.
        let zeroes = (BLOCK + LENGTH_AT - (self.buffered + 1) % BLOCK) % BLOCK;
        let pad = 1 + zeroes;
        tail[pad..pad + 8].copy_from_slice(&bits.to_be_bytes());
        let tail = &tail[..pad + 8];

        // `update` would add these bytes to the length; feed them to the
        // compression function through the same buffering by hand instead.
        let mut buffered = self.buffered;
        let mut buffer = self.buffer;
        for byte in tail {
            buffer[buffered] = *byte;
            buffered += 1;
            if buffered == BLOCK {
                self.compress(&buffer);
                buffered = 0;
            }
        }

        let mut hex = String::with_capacity(64);
        for word in self.state {
            use std::fmt::Write as _;
            let _ = write!(hex, "{word:08x}");
        }
        hex
    }

    /// One compression round over a full block.
    fn compress(&mut self, block: &[u8; BLOCK]) {
        let mut w = [0u32; 64];
        for (i, word) in w.iter_mut().take(16).enumerate() {
            let at = i * 4;
            *word = u32::from_be_bytes([block[at], block[at + 1], block[at + 2], block[at + 3]]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }

        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut h] = self.state;
        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let t1 = h
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[i])
                .wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let t2 = s0.wrapping_add(maj);

            h = g;
            g = f;
            f = e;
            e = d.wrapping_add(t1);
            d = c;
            c = b;
            b = a;
            a = t1.wrapping_add(t2);
        }

        for (word, delta) in self.state.iter_mut().zip([a, b, c, d, e, f, g, h]) {
            *word = word.wrapping_add(delta);
        }
    }
}

/// Digest a file, reading it in blocks rather than into memory.
pub fn sha256_file(path: &std::path::Path) -> std::io::Result<String> {
    let file = std::fs::File::open(path)?;
    sha256_stream(std::io::BufReader::new(file))
}

/// Digest everything a reader yields.
pub fn sha256_stream<R: std::io::Read>(mut reader: R) -> std::io::Result<String> {
    let mut hasher = Sha256::new();
    let mut chunk = [0u8; 8192];
    loop {
        let read = reader.read(&mut chunk)?;
        if read == 0 {
            break;
        }
        hasher.update(&chunk[..read]);
    }
    Ok(hasher.finish_hex())
}

/// Digest a slice already in memory.
pub fn sha256_hex(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    hasher.finish_hex()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The published vectors, which is what says this is SHA-256 and not
    /// something that merely looks like it.
    #[test]
    fn the_published_vectors_come_out_right() {
        assert_eq!(
            sha256_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(
            sha256_hex(b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"),
            "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1"
        );
    }

    /// A message that crosses the block boundary, and one that lands exactly on
    /// it: the padding branch is the part of this an implementation gets wrong.
    #[test]
    fn padding_is_right_at_and_around_the_block_boundary() {
        for length in [55usize, 56, 63, 64, 65, 119, 120, 127, 128] {
            let data = vec![b'a'; length];
            let at_once = sha256_hex(&data);
            let mut hasher = Sha256::new();
            for byte in &data {
                hasher.update(std::slice::from_ref(byte));
            }
            assert_eq!(
                at_once,
                hasher.finish_hex(),
                "{length} bytes digest differently in one call and byte by byte"
            );
        }
        assert_eq!(
            sha256_hex(&vec![b'a'; 1_000_000]),
            "cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0"
        );
    }
}
