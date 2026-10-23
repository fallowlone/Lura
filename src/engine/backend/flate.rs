//! Minimal zlib/Deflate encoder for PDF `FlateDecode` streams.
//!
//! One fixed-Huffman block with LZ77 hash-chain matching. It trades some ratio against a
//! dynamic-Huffman encoder for ~100 lines and no dependency; fonts and content streams still
//! shrink to roughly half or better.

const WINDOW: usize = 32768;
const MIN_MATCH: usize = 3;
const MAX_MATCH: usize = 258;
const MAX_CHAIN: usize = 64;
const HASH_BITS: u32 = 15;
const NONE: u32 = u32::MAX;

const LEN_BASE: [u16; 29] = [
    3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31, 35, 43, 51, 59, 67, 83, 99, 115, 131,
    163, 195, 227, 258,
];
const LEN_EXTRA: [u8; 29] = [
    0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0,
];
const DIST_BASE: [u16; 30] = [
    1, 2, 3, 4, 5, 7, 9, 13, 17, 25, 33, 49, 65, 97, 129, 193, 257, 385, 513, 769, 1025, 1537,
    2049, 3073, 4097, 6145, 8193, 12289, 16385, 24577,
];
const DIST_EXTRA: [u8; 30] = [
    0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 13,
    13,
];

/// Compress `data` as a zlib stream (RFC 1950) holding one fixed-Huffman Deflate block.
pub fn zlib_compress(data: &[u8]) -> Vec<u8> {
    let mut w = BitWriter::default();
    w.out.extend_from_slice(&[0x78, 0x01]);
    w.put(1, 1); // BFINAL
    w.put(1, 2); // BTYPE = fixed Huffman

    let n = data.len();
    let mut head = vec![NONE; 1 << HASH_BITS];
    let mut prev = vec![NONE; n];
    let mut i = 0;
    while i < n {
        let mut best = (0usize, 0usize);
        if i + MIN_MATCH <= n {
            let limit = MAX_MATCH.min(n - i);
            let mut cand = head[hash(data, i)];
            let mut chain = 0;
            while cand != NONE && i - cand as usize <= WINDOW && chain < MAX_CHAIN {
                let c = cand as usize;
                let len = data[c..].iter().zip(&data[i..i + limit]).take_while(|(a, b)| a == b).count();
                if len > best.0 {
                    best = (len, i - c);
                    if len == limit {
                        break;
                    }
                }
                cand = prev[c];
                chain += 1;
            }
        }
        if best.0 >= MIN_MATCH {
            w.put_match(best.0, best.1);
            for p in i..i + best.0 {
                insert(&mut head, &mut prev, data, p);
            }
            i += best.0;
        } else {
            w.put_literal(u32::from(data[i]));
            insert(&mut head, &mut prev, data, i);
            i += 1;
        }
    }
    w.put_symbol(256); // end of block
    let mut out = w.finish();
    out.extend_from_slice(&adler32(data).to_be_bytes());
    out
}

fn hash(data: &[u8], p: usize) -> usize {
    let h = (u32::from(data[p]) << 10) ^ (u32::from(data[p + 1]) << 5) ^ u32::from(data[p + 2]);
    (h as usize) & ((1 << HASH_BITS) - 1)
}

/// Link position `p` into the hash chain of its 3-byte prefix.
fn insert(head: &mut [u32], prev: &mut [u32], data: &[u8], p: usize) {
    if p + MIN_MATCH <= data.len() {
        let h = hash(data, p);
        prev[p] = head[h];
        head[h] = p as u32;
    }
}

pub fn adler32(data: &[u8]) -> u32 {
    const MOD: u32 = 65521;
    let (mut a, mut b) = (1u32, 0u32);
    for chunk in data.chunks(5552) {
        for &byte in chunk {
            a += u32::from(byte);
            b += a;
        }
        a %= MOD;
        b %= MOD;
    }
    (b << 16) | a
}

#[derive(Default)]
struct BitWriter {
    out: Vec<u8>,
    acc: u64,
    bits: u32,
}

impl BitWriter {
    /// Append `count` bits of `value`, least significant bit first.
    fn put(&mut self, value: u32, count: u32) {
        self.acc |= u64::from(value) << self.bits;
        self.bits += count;
        while self.bits >= 8 {
            self.out.push(self.acc as u8);
            self.acc >>= 8;
            self.bits -= 8;
        }
    }

    /// Huffman codes are packed most significant bit first.
    fn put_code(&mut self, code: u32, len: u32) {
        self.put(code.reverse_bits() >> (32 - len), len);
    }

    /// Fixed literal/length code (RFC 1951 3.2.6).
    fn put_symbol(&mut self, sym: u32) {
        match sym {
            0..=143 => self.put_code(0x30 + sym, 8),
            144..=255 => self.put_code(0x190 + sym - 144, 9),
            256..=279 => self.put_code(sym - 256, 7),
            _ => self.put_code(0xC0 + sym - 280, 8),
        }
    }

    fn put_literal(&mut self, byte: u32) {
        self.put_symbol(byte);
    }

    fn put_match(&mut self, len: usize, dist: usize) {
        let li = LEN_BASE.partition_point(|&b| usize::from(b) <= len) - 1;
        self.put_symbol(257 + li as u32);
        self.put((len - usize::from(LEN_BASE[li])) as u32, u32::from(LEN_EXTRA[li]));
        let di = DIST_BASE.partition_point(|&b| usize::from(b) <= dist) - 1;
        self.put_code(di as u32, 5);
        self.put((dist - usize::from(DIST_BASE[di])) as u32, u32::from(DIST_EXTRA[di]));
    }

    fn finish(mut self) -> Vec<u8> {
        if self.bits > 0 {
            self.out.push(self.acc as u8);
        }
        self.out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Inflate for exactly what the encoder emits: zlib header, one fixed-Huffman block, Adler-32.
    fn inflate(z: &[u8]) -> Vec<u8> {
        assert_eq!(&z[..2], &[0x78, 0x01]);
        let body = &z[2..z.len() - 4];
        let mut pos = 0usize;
        let mut bit = || {
            let b = (body[pos / 8] >> (pos % 8)) & 1;
            pos += 1;
            u32::from(b)
        };
        let read = |n: u32, bit: &mut dyn FnMut() -> u32| (0..n).fold(0, |v, i| v | (bit() << i));
        assert_eq!(read(3, &mut bit), 0b011, "BFINAL=1, BTYPE=fixed");
        let mut out: Vec<u8> = Vec::new();
        loop {
            let mut code = 0;
            for _ in 0..7 {
                code = (code << 1) | bit();
            }
            let sym = if code <= 0b001_0111 {
                256 + code
            } else {
                code = (code << 1) | bit();
                if (0x30..=0xBF).contains(&code) {
                    code - 0x30
                } else if (0xC0..=0xC7).contains(&code) {
                    280 + code - 0xC0
                } else {
                    code = (code << 1) | bit();
                    144 + code - 0x190
                }
            };
            match sym {
                0..=255 => out.push(sym as u8),
                256 => break,
                _ => {
                    let li = (sym - 257) as usize;
                    let len = usize::from(LEN_BASE[li]) + read(u32::from(LEN_EXTRA[li]), &mut bit) as usize;
                    let mut di = 0;
                    for _ in 0..5 {
                        di = (di << 1) | bit() as usize;
                    }
                    let dist = usize::from(DIST_BASE[di]) + read(u32::from(DIST_EXTRA[di]), &mut bit) as usize;
                    for _ in 0..len {
                        out.push(out[out.len() - dist]);
                    }
                }
            }
        }
        assert_eq!(&z[z.len() - 4..], &adler32(&out).to_be_bytes());
        out
    }

    fn pseudo_random(n: usize) -> Vec<u8> {
        let mut x = 0x2545_F491u32;
        (0..n)
            .map(|_| {
                x ^= x << 13;
                x ^= x >> 17;
                x ^= x << 5;
                (x >> 24) as u8
            })
            .collect()
    }

    #[test]
    fn adler32_matches_reference_vector() {
        assert_eq!(adler32(b"Wikipedia"), 0x11E6_0398);
        assert_eq!(adler32(b""), 1);
    }

    #[test]
    fn round_trips_edge_cases() {
        let long_zeros = vec![0u8; 100_000];
        let text = "the quick brown fox ".repeat(500).into_bytes();
        for data in [&b""[..], b"a", b"abc", b"aaaaaaaaaaaaaaaaaaaaaaaa", &long_zeros, &text] {
            assert_eq!(inflate(&zlib_compress(data)), data);
        }
    }

    #[test]
    fn round_trips_incompressible_and_font_data() {
        let noise = pseudo_random(20_000);
        assert_eq!(inflate(&zlib_compress(&noise)), noise);
        let font = include_bytes!("../../../assets/fonts/LiberationMono-Regular.ttf");
        assert_eq!(inflate(&zlib_compress(font)), &font[..]);
    }

    #[test]
    fn repetitive_data_shrinks_a_lot() {
        let text = "the quick brown fox ".repeat(500).into_bytes();
        assert!(zlib_compress(&text).len() < text.len() / 20);
    }
}
