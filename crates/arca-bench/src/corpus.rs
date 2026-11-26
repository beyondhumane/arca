use rayon::prelude::*;
use std::sync::Arc;

/// Bytes in one chunk: the unit each thread compresses on its own.
pub const CHUNK: usize = 1 << 20;

/// Bytes in one stretch of a mixed chunk. Every fourth stretch is random.
pub const STRETCH: usize = 64 << 10;

/// The seed every published rating uses. Changing it, or anything in this
/// file, makes earlier ratings incomparable.
pub const SEED: u64 = 0x4172_6361_4265_6e63;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// Three stretches of text to one of random bytes, all through each chunk.
    Mixed,
    Text,
    Random,
}

pub(crate) struct Rng(u64);

impl Rng {
    pub(crate) fn new(seed: u64) -> Self {
        Rng(seed)
    }

    pub(crate) fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }

    fn fill(&mut self, out: &mut [u8]) {
        for block in out.chunks_mut(8) {
            let word = self.next().to_le_bytes();
            block.copy_from_slice(&word[..block.len()]);
        }
    }
}

const SYLLABLES: [&str; 48] = [
    "a", "ar", "ca", "lo", "ne", "ti", "ra", "mo", "se", "du", "ver", "in", "con", "pre", "ex",
    "ta", "ri", "on", "al", "es", "de", "la", "me", "no", "po", "sa", "tu", "vi", "ber", "gan",
    "ing", "ment", "er", "ly", "or", "ble", "ter", "the", "and", "it", "um", "o", "is", "ous",
    "an", "el", "st", "ch",
];

const WORDS: usize = 2048;

fn vocabulary() -> Vec<String> {
    let mut rng = Rng::new(0x766f_6361_6275_6c61);
    (0..WORDS)
        .map(|i| {
            let syllables = 1 + usize::from(i >= 64) + usize::from(i >= 512) + rng.below(2);
            (0..syllables)
                .map(|_| SYLLABLES[rng.below(SYLLABLES.len())])
                .collect()
        })
        .collect()
}

/// Prose-like text: words of a fixed vocabulary picked with a heavy skew
/// toward the common ones, in sentences and wrapped lines, with the odd
/// number. Compresses about as well as ordinary writing does.
struct Text<'a> {
    rng: Rng,
    words: &'a [String],
    column: usize,
    sentence: usize,
}

impl<'a> Text<'a> {
    fn push_word(&mut self, out: &mut Vec<u8>) {
        let word = if self.rng.below(24) == 0 {
            self.rng.below(100_000).to_string()
        } else {
            {
                let r = self.rng.next();
                let (a, b, c) = (r & 0x7ff, (r >> 11) & 0x7ff, (r >> 22) & 0x7ff);
                self.words[(a * b * c / (WORDS as u64 * WORDS as u64)) as usize].clone()
            }
        };
        if self.column + word.len() + 1 > 72 {
            out.push(b'\n');
            self.column = 0;
        } else if self.column > 0 {
            out.push(b' ');
            self.column += 1;
        }
        let start = out.len();
        out.extend_from_slice(word.as_bytes());
        if self.sentence == 0 {
            out[start] = out[start].to_ascii_uppercase();
        }
        self.column += word.len();
        self.sentence += 1;
        let end = self.sentence >= 4 && self.rng.below(12) == 0;
        if end {
            out.push(if self.rng.below(12) == 0 { b'?' } else { b'.' });
            self.column += 1;
            self.sentence = 0;
            if self.rng.below(8) == 0 {
                out.extend_from_slice(b"\n\n");
                self.column = 0;
            }
        } else if self.rng.below(10) == 0 {
            out.push(b',');
            self.column += 1;
        }
    }

    fn fill(&mut self, out: &mut Vec<u8>, len: usize) {
        let goal = out.len() + len;
        while out.len() < goal {
            self.push_word(out);
        }
        out.truncate(goal);
    }
}

fn chunk(kind: Kind, seed: u64, index: usize, words: &[String]) -> Arc<[u8]> {
    let mut rng = Rng::new(seed ^ (index as u64).wrapping_mul(0xd6e8_feb8_6659_fd93));
    let mut text = Text {
        rng: Rng::new(rng.next()),
        words,
        column: 0,
        sentence: 0,
    };
    let mut out = Vec::with_capacity(CHUNK + 80);
    for stretch in 0..CHUNK / STRETCH {
        let random = match kind {
            Kind::Mixed => stretch % 4 == 3,
            Kind::Text => false,
            Kind::Random => true,
        };
        if random {
            let at = out.len();
            out.resize(at + STRETCH, 0);
            rng.fill(&mut out[at..]);
        } else {
            text.fill(&mut out, STRETCH);
        }
    }
    out.into()
}

/// `chunks` chunks of [`CHUNK`] bytes each. The same arguments give the same
/// bytes on every machine, and chunk `i` does not depend on how many follow.
pub fn generate(kind: Kind, seed: u64, chunks: usize) -> Vec<Arc<[u8]>> {
    let words = vocabulary();
    (0..chunks)
        .into_par_iter()
        .map(|i| chunk(kind, seed, i, &words))
        .collect()
}
