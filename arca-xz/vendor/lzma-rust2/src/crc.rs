const CRC32_POLY: u32 = 0xEDB88320;

// Arca: slicing-by-8 tables. `CRC32_TABLES[0]` is the classic byte table, and
// `CRC32_TABLES[k][i]` is the CRC of byte `i` followed by `k` zero bytes, so
// eight bytes fold in with eight independent lookups instead of a chain of
// eight dependent ones.
const fn make_crc32_tables() -> [[u32; 256]; 8] {
    let mut tables = [[0; 256]; 8];
    let mut i = 0;
    while i < 256 {
        let mut crc = i as u32;
        let mut j = 0;
        while j < 8 {
            if crc & 1 != 0 {
                crc = (crc >> 1) ^ CRC32_POLY;
            } else {
                crc >>= 1;
            }
            j += 1;
        }
        tables[0][i] = crc;
        i += 1;
    }
    let mut k = 1;
    while k < 8 {
        let mut i = 0;
        while i < 256 {
            let prev = tables[k - 1][i];
            tables[k][i] = (prev >> 8) ^ tables[0][(prev & 0xFF) as usize];
            i += 1;
        }
        k += 1;
    }
    tables
}

const CRC32_TABLES: [[u32; 256]; 8] = make_crc32_tables();

/// CRC_32_ISO_HDLC
#[derive(Clone, Copy)]
#[repr(transparent)]
pub(crate) struct Crc32 {
    state: u32,
}

impl Crc32 {
    pub(crate) fn new() -> Self {
        Self { state: 0xFFFFFFFF }
    }

    pub(crate) fn update(&mut self, data: &[u8]) {
        let t = &CRC32_TABLES;
        let mut crc = self.state;
        let mut chunks = data.chunks_exact(8);
        for chunk in &mut chunks {
            let lo = u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]) ^ crc;
            let hi = u32::from_le_bytes([chunk[4], chunk[5], chunk[6], chunk[7]]);
            crc = t[7][(lo & 0xFF) as usize]
                ^ t[6][((lo >> 8) & 0xFF) as usize]
                ^ t[5][((lo >> 16) & 0xFF) as usize]
                ^ t[4][(lo >> 24) as usize]
                ^ t[3][(hi & 0xFF) as usize]
                ^ t[2][((hi >> 8) & 0xFF) as usize]
                ^ t[1][((hi >> 16) & 0xFF) as usize]
                ^ t[0][(hi >> 24) as usize];
        }
        for &byte in chunks.remainder() {
            crc = (crc >> 8) ^ t[0][((crc ^ byte as u32) & 0xFF) as usize];
        }
        self.state = crc;
    }

    pub(crate) fn finalize(self) -> u32 {
        self.state ^ 0xFFFFFFFF
    }

    pub(crate) fn checksum(data: &[u8]) -> u32 {
        let mut crc = Self::new();
        crc.update(data);
        crc.finalize()
    }
}

const CRC64_POLY: u64 = 0xC96C5795D7870F42;

// Arca: slicing-by-8, as for CRC32 above.
const fn make_crc64_tables() -> [[u64; 256]; 8] {
    let mut tables = [[0; 256]; 8];
    let mut i = 0;
    while i < 256 {
        let mut crc = i as u64;
        let mut j = 0;
        while j < 8 {
            if crc & 1 != 0 {
                crc = (crc >> 1) ^ CRC64_POLY;
            } else {
                crc >>= 1;
            }
            j += 1;
        }
        tables[0][i] = crc;
        i += 1;
    }
    let mut k = 1;
    while k < 8 {
        let mut i = 0;
        while i < 256 {
            let prev = tables[k - 1][i];
            tables[k][i] = (prev >> 8) ^ tables[0][(prev & 0xFF) as usize];
            i += 1;
        }
        k += 1;
    }
    tables
}

const CRC64_TABLES: [[u64; 256]; 8] = make_crc64_tables();

/// CRC_64_XZ
#[derive(Clone, Copy)]
#[repr(transparent)]
pub(crate) struct Crc64 {
    state: u64,
}

impl Crc64 {
    pub(crate) fn new() -> Self {
        Self {
            state: 0xFFFFFFFFFFFFFFFF,
        }
    }

    pub(crate) fn update(&mut self, data: &[u8]) {
        let t = &CRC64_TABLES;
        let mut crc = self.state;
        let mut chunks = data.chunks_exact(8);
        for chunk in &mut chunks {
            let mut word = [0; 8];
            word.copy_from_slice(chunk);
            let v = u64::from_le_bytes(word) ^ crc;
            crc = t[7][(v & 0xFF) as usize]
                ^ t[6][((v >> 8) & 0xFF) as usize]
                ^ t[5][((v >> 16) & 0xFF) as usize]
                ^ t[4][((v >> 24) & 0xFF) as usize]
                ^ t[3][((v >> 32) & 0xFF) as usize]
                ^ t[2][((v >> 40) & 0xFF) as usize]
                ^ t[1][((v >> 48) & 0xFF) as usize]
                ^ t[0][(v >> 56) as usize];
        }
        for &byte in chunks.remainder() {
            crc = (crc >> 8) ^ t[0][((crc ^ byte as u64) & 0xFF) as usize];
        }
        self.state = crc;
    }

    pub(crate) fn finalize(self) -> u64 {
        self.state ^ 0xFFFFFFFFFFFFFFFF
    }

    pub(crate) fn checksum(data: &[u8]) -> u64 {
        let mut crc = Self::new();
        crc.update(data);
        crc.finalize()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SIMPLE_DATA: &[u8] = &[1, 2, 3, 7, 16, 31, 64, 255];

    #[test]
    fn crc32_empty() {
        assert_eq!(Crc32::checksum(&[]), 0);
    }

    #[test]
    fn crc64_empty() {
        assert_eq!(Crc64::checksum(&[]), 0);
    }

    #[test]
    fn crc32_simple_data() {
        assert_eq!(Crc32::checksum(SIMPLE_DATA), 2428203834);
    }

    #[test]
    fn crc64_simple_data() {
        assert_eq!(Crc64::checksum(SIMPLE_DATA), 11721292222009571391);
    }

    // Arca: the sliced tables against the bit-at-a-time definition, for every
    // length around the eight-byte stride and for updates split anywhere.
    #[test]
    fn sliced_matches_bitwise() {
        fn bitwise32(data: &[u8]) -> u32 {
            let mut crc = !0u32;
            for &b in data {
                crc ^= b as u32;
                for _ in 0..8 {
                    crc = if crc & 1 != 0 { (crc >> 1) ^ CRC32_POLY } else { crc >> 1 };
                }
            }
            !crc
        }
        fn bitwise64(data: &[u8]) -> u64 {
            let mut crc = !0u64;
            for &b in data {
                crc ^= b as u64;
                for _ in 0..8 {
                    crc = if crc & 1 != 0 { (crc >> 1) ^ CRC64_POLY } else { crc >> 1 };
                }
            }
            !crc
        }
        assert_eq!(Crc32::checksum(b"123456789"), 0xCBF43926);
        assert_eq!(Crc64::checksum(b"123456789"), 0x995DC9BBDF1939FA);
        let data: Vec<u8> = (0..300u32).map(|i| (i.wrapping_mul(2654435761) >> 13) as u8).collect();
        for len in 0..data.len() {
            let part = &data[..len];
            assert_eq!(Crc32::checksum(part), bitwise32(part));
            assert_eq!(Crc64::checksum(part), bitwise64(part));
            let split = len / 3;
            let mut c32 = Crc32::new();
            c32.update(&part[..split]);
            c32.update(&part[split..]);
            assert_eq!(c32.finalize(), bitwise32(part));
            let mut c64 = Crc64::new();
            c64.update(&part[..split]);
            c64.update(&part[split..]);
            assert_eq!(c64.finalize(), bitwise64(part));
        }
    }
}
