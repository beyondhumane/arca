const BLOCK_BYTES: usize = 64;
const OUT_BYTES: usize = 32;
const PARALLELISM: usize = 8;
const BUFFER_BYTES: usize = BLOCK_BYTES * PARALLELISM;

const IV: [u32; 8] = [
    0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19,
];

const SIGMA: [[usize; 16]; 10] = [
    [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15],
    [14, 10, 4, 8, 9, 15, 13, 6, 1, 12, 0, 2, 11, 7, 5, 3],
    [11, 8, 12, 0, 5, 2, 15, 13, 10, 14, 3, 6, 7, 1, 9, 4],
    [7, 9, 3, 1, 13, 12, 11, 14, 2, 6, 5, 10, 4, 0, 15, 8],
    [9, 0, 5, 7, 2, 4, 10, 15, 14, 1, 11, 12, 6, 8, 3, 13],
    [2, 12, 6, 10, 0, 11, 8, 3, 4, 13, 7, 5, 15, 14, 1, 9],
    [12, 5, 1, 15, 14, 13, 4, 10, 0, 7, 6, 3, 9, 2, 8, 11],
    [13, 11, 7, 14, 12, 1, 3, 9, 5, 0, 15, 4, 8, 6, 2, 10],
    [6, 15, 14, 9, 11, 3, 0, 8, 12, 2, 13, 7, 1, 4, 10, 5],
    [10, 2, 8, 4, 7, 6, 1, 5, 15, 11, 9, 14, 3, 12, 13, 0],
];

struct Blake2s {
    h: [u32; 8],
    t: u64,
    buf: [u8; BLOCK_BYTES],
    buflen: usize,
    last_node: bool,
}

impl Blake2s {
    fn new(node_offset: u64, node_depth: u8, last_node: bool) -> Self {
        let mut param = [0u8; 32];
        param[0] = OUT_BYTES as u8;
        param[2] = PARALLELISM as u8;
        param[3] = 2;
        param[8..14].copy_from_slice(&node_offset.to_le_bytes()[..6]);
        param[14] = node_depth;
        param[15] = OUT_BYTES as u8;

        let mut h = IV;
        for (word, chunk) in h.iter_mut().zip(param.as_chunks::<4>().0.iter()) {
            *word ^= u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
        }

        Self {
            h,
            t: 0,
            buf: [0; BLOCK_BYTES],
            buflen: 0,
            last_node,
        }
    }

    fn update(&mut self, mut input: &[u8]) {
        // Stripe updates are one 64-byte block, final leaf tails are 1..=64
        // bytes, and the root consumes one 32-byte leaf digest at a time.
        debug_assert!(!input.is_empty() && input.len() <= BLOCK_BYTES);
        if self.buflen > 0 {
            let fill = BLOCK_BYTES - self.buflen;
            if input.len() > fill {
                self.buf[self.buflen..].copy_from_slice(&input[..fill]);
                self.t = self.t.wrapping_add(BLOCK_BYTES as u64);
                let block = self.buf;
                self.compress(&block, false);
                self.buflen = 0;
                input = &input[fill..];
            } else {
                self.buf[self.buflen..self.buflen + input.len()].copy_from_slice(input);
                self.buflen += input.len();
                return;
            }
        }
        self.buf[..input.len()].copy_from_slice(input);
        self.buflen = input.len();
    }

    fn finalize(mut self) -> [u8; OUT_BYTES] {
        self.t = self.t.wrapping_add(self.buflen as u64);
        self.buf[self.buflen..].fill(0);
        let block = self.buf;
        self.compress(&block, true);

        let mut out = [0u8; OUT_BYTES];
        for (chunk, word) in out.as_chunks_mut::<4>().0.iter_mut().zip(self.h) {
            chunk.copy_from_slice(&word.to_le_bytes());
        }
        out
    }

    fn compress(&mut self, block: &[u8; BLOCK_BYTES], last_block: bool) {
        let mut m = [0u32; 16];
        for (word, chunk) in m.iter_mut().zip(block.as_chunks::<4>().0.iter()) {
            *word = u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
        }

        let mut v = [0u32; 16];
        v[..8].copy_from_slice(&self.h);
        v[8..].copy_from_slice(&IV);
        v[12] ^= self.t as u32;
        v[13] ^= (self.t >> 32) as u32;
        if last_block {
            v[14] = !v[14];
        }
        if last_block && self.last_node {
            v[15] = !v[15];
        }

        for s in SIGMA {
            g(&mut v, 0, 4, 8, 12, m[s[0]], m[s[1]]);
            g(&mut v, 1, 5, 9, 13, m[s[2]], m[s[3]]);
            g(&mut v, 2, 6, 10, 14, m[s[4]], m[s[5]]);
            g(&mut v, 3, 7, 11, 15, m[s[6]], m[s[7]]);
            g(&mut v, 0, 5, 10, 15, m[s[8]], m[s[9]]);
            g(&mut v, 1, 6, 11, 12, m[s[10]], m[s[11]]);
            g(&mut v, 2, 7, 8, 13, m[s[12]], m[s[13]]);
            g(&mut v, 3, 4, 9, 14, m[s[14]], m[s[15]]);
        }

        for i in 0..8 {
            self.h[i] ^= v[i] ^ v[i + 8];
        }
    }
}

pub(crate) struct Hasher {
    leaves: [Blake2s; PARALLELISM],
    buffer: [u8; BUFFER_BYTES],
    buffer_len: usize,
}

impl Hasher {
    pub(crate) fn new() -> Self {
        Self {
            leaves: [
                Blake2s::new(0, 0, false),
                Blake2s::new(1, 0, false),
                Blake2s::new(2, 0, false),
                Blake2s::new(3, 0, false),
                Blake2s::new(4, 0, false),
                Blake2s::new(5, 0, false),
                Blake2s::new(6, 0, false),
                Blake2s::new(7, 0, true),
            ],
            buffer: [0; BUFFER_BYTES],
            buffer_len: 0,
        }
    }

    pub(crate) fn update(&mut self, mut input: &[u8]) {
        if input.is_empty() {
            return;
        }

        if self.buffer_len > 0 {
            let fill = BUFFER_BYTES - self.buffer_len;
            if input.len() >= fill {
                self.buffer[self.buffer_len..].copy_from_slice(&input[..fill]);
                let group = self.buffer;
                self.update_group(&group);
                self.buffer_len = 0;
                input = &input[fill..];
            } else {
                self.buffer[self.buffer_len..self.buffer_len + input.len()].copy_from_slice(input);
                self.buffer_len += input.len();
                return;
            }
        }

        let (groups, tail) = input.as_chunks::<BUFFER_BYTES>();
        for group in groups {
            self.update_group(group);
        }
        self.buffer[..tail.len()].copy_from_slice(tail);
        self.buffer_len = tail.len();
    }

    pub(crate) fn finalize(mut self) -> [u8; OUT_BYTES] {
        if self.buffer_len > 0 {
            let tail = &self.buffer[..self.buffer_len];
            for (leaf_index, leaf) in self.leaves.iter_mut().enumerate() {
                let start = leaf_index * BLOCK_BYTES;
                if tail.len() > start {
                    let end = (start + BLOCK_BYTES).min(tail.len());
                    leaf.update(&tail[start..end]);
                }
            }
        }

        let mut root = Blake2s::new(0, 1, true);
        for leaf in self.leaves {
            root.update(&leaf.finalize());
        }
        root.finalize()
    }

    fn update_group(&mut self, group: &[u8]) {
        // Keep each leaf's final block pending, just as Blake2s::update does.
        // A new full stripe proves the previous stripe is not final, so its
        // eight independent compression states can run together.
        if self.leaves[0].buflen == BLOCK_BYTES {
            debug_assert!(self.leaves.iter().all(|leaf| leaf.buflen == BLOCK_BYTES));
            self.compress_leaf_group();
            for (leaf, block) in self
                .leaves
                .iter_mut()
                .zip(group.as_chunks::<BLOCK_BYTES>().0.iter())
            {
                leaf.buf.copy_from_slice(block);
            }
            return;
        }
        for (leaf, block) in self
            .leaves
            .iter_mut()
            .zip(group.as_chunks::<BLOCK_BYTES>().0.iter())
        {
            leaf.update(block);
        }
    }

    fn compress_leaf_group(&mut self) {
        // Word-major lanes expose independent leaf work to the optimizer
        // without CPU-specific intrinsics or changing the persistent state.
        let mut message = [[0u32; PARALLELISM]; 16];
        let mut state = [[0u32; PARALLELISM]; 16];
        for (lane, leaf) in self.leaves.iter_mut().enumerate() {
            leaf.t = leaf.t.wrapping_add(BLOCK_BYTES as u64);
            for (word, bytes) in leaf.buf.as_chunks::<4>().0.iter().enumerate() {
                message[word][lane] = u32::from_le_bytes(*bytes);
            }
            for word in 0..8 {
                state[word][lane] = leaf.h[word];
                state[word + 8][lane] = IV[word];
            }
            state[12][lane] ^= leaf.t as u32;
            state[13][lane] ^= (leaf.t >> 32) as u32;
        }
        for schedule in SIGMA {
            g_lanes::<0, 4, 8, 12>(&mut state, &message[schedule[0]], &message[schedule[1]]);
            g_lanes::<1, 5, 9, 13>(&mut state, &message[schedule[2]], &message[schedule[3]]);
            g_lanes::<2, 6, 10, 14>(&mut state, &message[schedule[4]], &message[schedule[5]]);
            g_lanes::<3, 7, 11, 15>(&mut state, &message[schedule[6]], &message[schedule[7]]);
            g_lanes::<0, 5, 10, 15>(&mut state, &message[schedule[8]], &message[schedule[9]]);
            g_lanes::<1, 6, 11, 12>(&mut state, &message[schedule[10]], &message[schedule[11]]);
            g_lanes::<2, 7, 8, 13>(&mut state, &message[schedule[12]], &message[schedule[13]]);
            g_lanes::<3, 4, 9, 14>(&mut state, &message[schedule[14]], &message[schedule[15]]);
        }
        for (lane, leaf) in self.leaves.iter_mut().enumerate() {
            for word in 0..8 {
                leaf.h[word] ^= state[word][lane] ^ state[word + 8][lane];
            }
        }
    }
}

#[inline(always)]
fn g_lanes<const A: usize, const B: usize, const C: usize, const D: usize>(
    state: &mut [[u32; PARALLELISM]; 16],
    x: &[u32; PARALLELISM],
    y: &[u32; PARALLELISM],
) {
    for lane in 0..PARALLELISM {
        state[A][lane] = state[A][lane]
            .wrapping_add(state[B][lane])
            .wrapping_add(x[lane]);
        state[D][lane] = (state[D][lane] ^ state[A][lane]).rotate_right(16);
        state[C][lane] = state[C][lane].wrapping_add(state[D][lane]);
        state[B][lane] = (state[B][lane] ^ state[C][lane]).rotate_right(12);
        state[A][lane] = state[A][lane]
            .wrapping_add(state[B][lane])
            .wrapping_add(y[lane]);
        state[D][lane] = (state[D][lane] ^ state[A][lane]).rotate_right(8);
        state[C][lane] = state[C][lane].wrapping_add(state[D][lane]);
        state[B][lane] = (state[B][lane] ^ state[C][lane]).rotate_right(7);
    }
}

pub(crate) fn hash(input: &[u8]) -> [u8; OUT_BYTES] {
    let mut hasher = Hasher::new();
    hasher.update(input);
    hasher.finalize()
}

fn g(v: &mut [u32; 16], a: usize, b: usize, c: usize, d: usize, x: u32, y: u32) {
    v[a] = v[a].wrapping_add(v[b]).wrapping_add(x);
    v[d] = (v[d] ^ v[a]).rotate_right(16);
    v[c] = v[c].wrapping_add(v[d]);
    v[b] = (v[b] ^ v[c]).rotate_right(12);
    v[a] = v[a].wrapping_add(v[b]).wrapping_add(y);
    v[d] = (v[d] ^ v[a]).rotate_right(8);
    v[c] = v[c].wrapping_add(v[d]);
    v[b] = (v[b] ^ v[c]).rotate_right(7);
}

#[cfg(test)]
mod tests {
    use super::{hash, Blake2s, Hasher, BLOCK_BYTES, PARALLELISM};

    #[test]
    fn matches_public_blake2sp_vectors() {
        assert_eq!(
            hex(&hash(b"")),
            "dd0e891776933f43c7d032b08a917e25741f8aa9a12c12e1cac8801500f2ca4f"
        );
        assert_eq!(
            hex(&hash(b"abc")),
            "70f75b58f1fecab821db43c88ad84edde5a52600616cd22517b7bb14d440a7d5"
        );
    }

    #[test]
    fn block_and_stripe_boundaries_match_independent_tree_hash_vectors() {
        // Python hashlib.blake2s oracle: distribute consecutive 64-byte
        // blocks over eight leaves, fanout=8, depth=2, inner_size=32,
        // node_offset=leaf index, node_depth=0, last_node on leaf 7. Hash
        // their concatenated digests at node_depth=1, last_node=true.
        // The same oracle reproduces both public vectors above.
        for (length, expected) in [
            (
                1,
                "a6b9eecc25227ad788c99d3f236debc8da408849e9a5178978727a81457f7239",
            ),
            (
                63,
                "1024c940be7341449b5010522b509f65bbdc1287b455c2bb7f72b2c92fd0d189",
            ),
            (
                64,
                "52603b6cbfad4966cb044cb267568385cf35f21e6c45cf30aed19832cb51e9f5",
            ),
            (
                65,
                "fff24d3cc729d395daf978b0157306cb495797e6c8dca1731d2f6f81b849baae",
            ),
            (
                127,
                "a626543c271fccc3e4450b48d66bc9cbdeb25e5d077a6213cd90cbbd0fd22076",
            ),
            (
                511,
                "8e1e8ee1ffa0a01028fff3bff0ae9df2565a82e55a04e9541bb78b9c4778336f",
            ),
            (
                512,
                "8d9e357863298dd8364b7caf4234317f8a49f180d788b7abffb521925f1e1ff1",
            ),
            (
                513,
                "8a4bc3330497e681f15daf24fc496044a1c32bf0a837a210399e1ae4af7e92be",
            ),
            (
                1023,
                "0db3cb64828effe5b2aa6be5e865121f03226cca6423b8841b1019cfad09ac09",
            ),
            (
                1024,
                "48467549502e2d3f422870bfb1d09bce71a065735763bf654582cf46a5112793",
            ),
            (
                1025,
                "04e03e65b8f19a5f46288802b2a515bab73363262caa300ae75c0eb29c016e5a",
            ),
            (
                4097,
                "055d3f5f5440fe6c8dd0dcceb503b93caa03a533b71b1e47b4fdcb5577b8762a",
            ),
        ] {
            let input: Vec<_> = (0..length).map(|i| (i % 251) as u8).collect();
            assert_eq!(hex(&hash(&input)), expected, "length={length}");
            for chunk_size in [1, 31, 32, 63, 64, 65, 127, 511, 512, 513] {
                let mut hasher = Hasher::new();
                hasher.update(&[]);
                for chunk in input.chunks(chunk_size) {
                    hasher.update(chunk);
                    hasher.update(&[]);
                }
                assert_eq!(
                    hex(&hasher.finalize()),
                    expected,
                    "length={length}, chunk_size={chunk_size}"
                );
            }
        }
    }

    #[test]
    fn streaming_hasher_matches_one_shot_hash() {
        let input: Vec<u8> = (0..4097).map(|i| (i % 251) as u8).collect();
        let mut hasher = Hasher::new();
        for chunk in input.chunks(37) {
            hasher.update(chunk);
        }
        assert_eq!(hasher.finalize(), hash(&input));
    }

    #[test]
    fn batched_stripes_match_independent_long_tree_hash_vectors() {
        // Same independent hashlib tree parameters as the boundary vectors,
        // with enough complete stripes to repeatedly enter the batched path.
        for (length, expected) in [
            (
                65_535,
                "ab141a58697b388f28149af484e466beea9f28dbd5aed8e649f02fa8d0afcb0d",
            ),
            (
                65_536,
                "f31ebaad05d2ead9e476110facfaaf38141a58d53da972a09be1d9e22e0a94ad",
            ),
            (
                65_537,
                "60e42e849f6f22218ae9faeb17cf9eaf00fd9bc140a4b392109d6378557a4192",
            ),
            (
                1_048_576,
                "65f05ea2b52b252474eadbcc69d159009c3b7b98e2c630a3056561269d78656e",
            ),
        ] {
            let input: Vec<_> = (0..length).map(|i| (i % 251) as u8).collect();
            assert_eq!(hex(&hash(&input)), expected, "length={length}");
            for chunk_size in [63, 64, 65, 511, 512, 513, 4097, 65_536] {
                let mut hasher = Hasher::new();
                for chunk in input.chunks(chunk_size) {
                    hasher.update(chunk);
                }
                assert_eq!(
                    hex(&hasher.finalize()),
                    expected,
                    "length={length}, chunk={chunk_size}"
                );
            }
        }
    }

    #[test]
    fn batched_updates_match_scalar_leaves_for_varied_data_and_tails() {
        let mut random = 0x1234_5678u32;
        let input: Vec<_> = (0..65_537)
            .map(|_| {
                random = random.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                (random >> 24) as u8
            })
            .collect();
        for length in [
            0, 1, 63, 64, 65, 511, 512, 513, 1023, 1024, 1025, 4095, 4096, 4097, 65_535, 65_536,
            65_537,
        ] {
            let data = &input[..length];
            let mut leaves: [Blake2s; PARALLELISM] =
                std::array::from_fn(|lane| Blake2s::new(lane as u64, 0, lane == PARALLELISM - 1));
            for (block_index, block) in data.chunks(BLOCK_BYTES).enumerate() {
                leaves[block_index % PARALLELISM].update(block);
            }
            let mut root = Blake2s::new(0, 1, true);
            for leaf in leaves {
                root.update(&leaf.finalize());
            }
            let expected = root.finalize();
            assert_eq!(hash(data), expected, "length={length}");
            for chunk_size in [1, 67, 511, 512, 513, 4097] {
                let mut hasher = Hasher::new();
                for chunk in data.chunks(chunk_size) {
                    hasher.update(chunk);
                }
                assert_eq!(
                    hasher.finalize(),
                    expected,
                    "length={length}, chunk={chunk_size}"
                );
            }
        }
    }

    #[test]
    fn batched_compression_keeps_each_leaf_counter_and_wrapping_arithmetic() {
        for counter in [0, 1, u64::MAX - 64, u64::MAX - 63, u64::MAX - 32] {
            let prepare = || {
                let mut hasher = Hasher::new();
                for (lane, leaf) in hasher.leaves.iter_mut().enumerate() {
                    leaf.t = counter.wrapping_add(lane as u64);
                    leaf.buflen = BLOCK_BYTES;
                    for (byte_index, byte) in leaf.buf.iter_mut().enumerate() {
                        *byte = (byte_index * 17 + lane * 41) as u8;
                    }
                    for (word, value) in leaf.h.iter_mut().enumerate() {
                        *value ^= u32::MAX.wrapping_sub((word * 17 + lane * 41) as u32);
                    }
                }
                hasher
            };
            let mut actual = prepare();
            let mut expected = prepare();
            actual.compress_leaf_group();
            for leaf in &mut expected.leaves {
                leaf.t = leaf.t.wrapping_add(BLOCK_BYTES as u64);
                let block = leaf.buf;
                leaf.compress(&block, false);
            }
            for (actual, expected) in actual.leaves.iter().zip(expected.leaves) {
                assert_eq!(actual.h, expected.h);
                assert_eq!(actual.t, expected.t);
                assert_eq!(actual.buf, expected.buf);
                assert_eq!(actual.buflen, expected.buflen);
            }
        }
    }

    fn hex(bytes: &[u8]) -> String {
        const HEX: &[u8; 16] = b"0123456789abcdef";
        let mut out = String::with_capacity(bytes.len() * 2);
        for &byte in bytes {
            out.push(HEX[(byte >> 4) as usize] as char);
            out.push(HEX[(byte & 0x0f) as usize] as char);
        }
        out
    }
}
