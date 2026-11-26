use sha1::Digest;
use std::io;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Algorithm {
    Crc32,
    Sha1,
    Sha256,
}

impl Algorithm {
    pub const ALL: [Algorithm; 3] = [Algorithm::Crc32, Algorithm::Sha1, Algorithm::Sha256];

    pub fn label(self) -> &'static str {
        match self {
            Algorithm::Crc32 => "CRC32",
            Algorithm::Sha1 => "SHA1",
            Algorithm::Sha256 => "SHA256",
        }
    }

    pub fn hex_len(self) -> usize {
        match self {
            Algorithm::Crc32 => 8,
            Algorithm::Sha1 => 40,
            Algorithm::Sha256 => 64,
        }
    }

    /// The kind a checksum of `len` hex digits is. Each one has its own length.
    pub fn from_hex_len(len: usize) -> Option<Algorithm> {
        Self::ALL.into_iter().find(|a| a.hex_len() == len)
    }
}

enum State {
    Crc32(crc32fast::Hasher),
    Sha1(sha1::Sha1),
    Sha256(sha2::Sha256),
}

impl State {
    fn new(algorithm: Algorithm) -> Self {
        match algorithm {
            Algorithm::Crc32 => State::Crc32(crc32fast::Hasher::new()),
            Algorithm::Sha1 => State::Sha1(sha1::Sha1::new()),
            Algorithm::Sha256 => State::Sha256(sha2::Sha256::new()),
        }
    }

    fn update(&mut self, data: &[u8]) {
        match self {
            State::Crc32(h) => h.update(data),
            State::Sha1(h) => h.update(data),
            State::Sha256(h) => h.update(data),
        }
    }

    fn finish(self) -> String {
        match self {
            State::Crc32(h) => hex(&h.finalize().to_be_bytes()),
            State::Sha1(h) => hex(&h.finalize()),
            State::Sha256(h) => hex(&h.finalize()),
        }
    }
}

fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        out.push(DIGITS[usize::from(b >> 4)] as char);
        out.push(DIGITS[usize::from(b & 0x0f)] as char);
    }
    out
}

/// Several checksums of one stream, worked out in a single pass over it. Write
/// the bytes in, then `finish` gives the lowercase hex of each, in the order
/// the algorithms were asked for.
pub struct Hasher {
    states: Vec<(Algorithm, State)>,
    bytes: u64,
}

impl Hasher {
    pub fn new(algorithms: &[Algorithm]) -> Self {
        Hasher {
            states: algorithms.iter().map(|&a| (a, State::new(a))).collect(),
            bytes: 0,
        }
    }

    pub fn update(&mut self, data: &[u8]) {
        for (_, state) in &mut self.states {
            state.update(data);
        }
        self.bytes += data.len() as u64;
    }

    pub fn bytes(&self) -> u64 {
        self.bytes
    }

    pub fn finish(self) -> Vec<(Algorithm, String)> {
        self.states
            .into_iter()
            .map(|(a, state)| (a, state.finish()))
            .collect()
    }
}

impl io::Write for Hasher {
    fn write(&mut self, data: &[u8]) -> io::Result<usize> {
        self.update(data);
        Ok(data.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn of(algorithm: Algorithm, data: &[u8]) -> String {
        let mut h = Hasher::new(&[algorithm]);
        h.update(data);
        h.finish().remove(0).1
    }

    #[test]
    fn known_vectors() {
        assert_eq!(of(Algorithm::Crc32, b"123456789"), "cbf43926");
        assert_eq!(
            of(Algorithm::Sha1, b"abc"),
            "a9993e364706816aba3e25717850c26c9cd0d89d"
        );
        assert_eq!(
            of(Algorithm::Sha256, b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(of(Algorithm::Crc32, b""), "00000000");
    }

    #[test]
    fn one_pass_gives_every_checksum_and_split_writes_change_nothing() {
        let mut h = Hasher::new(&Algorithm::ALL);
        for piece in [&b"12345"[..], b"", b"6789"] {
            h.update(piece);
        }
        assert_eq!(h.bytes(), 9);
        let sums = h.finish();
        assert_eq!(sums[0], (Algorithm::Crc32, "cbf43926".to_string()));
        assert_eq!(sums[1].1, of(Algorithm::Sha1, b"123456789"));
        assert_eq!(sums[2].1, of(Algorithm::Sha256, b"123456789"));
    }

    #[test]
    fn the_length_of_a_checksum_says_its_kind() {
        for a in Algorithm::ALL {
            assert_eq!(Algorithm::from_hex_len(a.hex_len()), Some(a));
        }
        assert_eq!(Algorithm::from_hex_len(7), None);
    }
}
