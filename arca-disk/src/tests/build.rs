// Tiny images built byte by byte, as the specifications lay them out.

pub(crate) fn long_content() -> Vec<u8> {
    (0..3000u32).map(|i| (i * 7 % 251) as u8).collect()
}

pub(crate) const DOS_DATE: u16 = ((2024 - 1980) << 9) | (5 << 5) | 17;
pub(crate) const DOS_TIME: u16 = (12 << 11) | (30 << 5) | 5;

pub(crate) struct DirEnt {
    pub short: [u8; 11],
    pub long: Option<&'static str>,
    pub attr: u8,
    pub case: u8,
    pub start: u32,
    pub size: u32,
}

impl DirEnt {
    fn file(
        short: &[u8; 11],
        long: Option<&'static str>,
        case: u8,
        start: u32,
        size: usize,
    ) -> Self {
        DirEnt {
            short: *short,
            long,
            attr: 0x20,
            case,
            start,
            size: size as u32,
        }
    }

    fn dir(short: &[u8; 11], long: Option<&'static str>, start: u32) -> Self {
        DirEnt {
            short: *short,
            long,
            attr: 0x10,
            case: 0,
            start,
            size: 0,
        }
    }
}

fn short_sum(short: &[u8; 11]) -> u8 {
    short
        .iter()
        .fold(0u8, |sum, &b| sum.rotate_right(1).wrapping_add(b))
}

fn encode(entries: &[DirEnt]) -> Vec<u8> {
    let mut out = Vec::new();
    for e in entries {
        if let Some(long) = e.long {
            let mut units: Vec<u16> = long.encode_utf16().collect();
            if !units.len().is_multiple_of(13) {
                units.push(0);
            }
            while !units.len().is_multiple_of(13) {
                units.push(0xffff);
            }
            let parts = units.len() / 13;
            for ord in (1..=parts).rev() {
                let mut l = [0u8; 32];
                l[0] = ord as u8 | if ord == parts { 0x40 } else { 0 };
                l[11] = 0x0f;
                l[13] = short_sum(&e.short);
                let chunk = &units[(ord - 1) * 13..ord * 13];
                for (i, at) in [1, 3, 5, 7, 9, 14, 16, 18, 20, 22, 24, 28, 30]
                    .iter()
                    .enumerate()
                {
                    l[*at..*at + 2].copy_from_slice(&chunk[i].to_le_bytes());
                }
                out.extend_from_slice(&l);
            }
        }
        let mut s = [0u8; 32];
        s[..11].copy_from_slice(&e.short);
        s[11] = e.attr;
        s[12] = e.case;
        s[14..16].copy_from_slice(&DOS_TIME.to_le_bytes());
        s[16..18].copy_from_slice(&DOS_DATE.to_le_bytes());
        s[18..20].copy_from_slice(&DOS_DATE.to_le_bytes());
        s[20..22].copy_from_slice(&((e.start >> 16) as u16).to_le_bytes());
        s[22..24].copy_from_slice(&DOS_TIME.to_le_bytes());
        s[24..26].copy_from_slice(&DOS_DATE.to_le_bytes());
        s[26..28].copy_from_slice(&(e.start as u16).to_le_bytes());
        s[28..32].copy_from_slice(&e.size.to_le_bytes());
        out.extend_from_slice(&s);
    }
    out
}

struct Fat {
    width: u32,
    reserved: u64,
    root_entries: u64,
    total: u64,
    fat_sectors: u64,
    data_at: u64,
    image: Vec<u8>,
    table: Vec<u32>,
    next: u32,
}

impl Fat {
    fn new(width: u32) -> Fat {
        let (total, reserved, root_entries, fat_sectors): (u64, u64, u64, u64) = match width {
            12 => (128, 1, 32, 2),
            16 => (4300, 1, 32, 17),
            _ => (66600, 32, 0, 513),
        };
        let data_at = reserved + 2 * fat_sectors + (root_entries * 32).div_ceil(512);
        let clusters = total - data_at;
        Fat {
            width,
            reserved,
            root_entries,
            total,
            fat_sectors,
            data_at,
            image: vec![0u8; (total * 512) as usize],
            table: vec![0; clusters as usize + 2],
            next: 2,
        }
    }

    fn end(&self) -> u32 {
        match self.width {
            12 => 0xfff,
            16 => 0xffff,
            _ => 0x0fff_ffff,
        }
    }

    fn alloc(&mut self, bytes: usize, spread: bool) -> u32 {
        if bytes == 0 {
            return 0;
        }
        let count = bytes.div_ceil(512);
        let mut clusters = Vec::new();
        for _ in 0..count {
            clusters.push(self.next);
            self.next += if spread { 2 } else { 1 };
        }
        for pair in clusters.windows(2) {
            self.table[pair[0] as usize] = pair[1];
        }
        let last = *clusters.last().unwrap();
        self.table[last as usize] = self.end();
        clusters[0]
    }

    fn offset(&self, cluster: u32) -> usize {
        ((self.data_at + u64::from(cluster) - 2) * 512) as usize
    }

    fn write(&mut self, start: u32, data: &[u8]) {
        let mut cluster = start;
        for chunk in data.chunks(512) {
            let at = self.offset(cluster);
            self.image[at..at + chunk.len()].copy_from_slice(chunk);
            cluster = self.table[cluster as usize];
        }
    }

    fn file(&mut self, data: &[u8], spread: bool) -> u32 {
        let start = self.alloc(data.len(), spread);
        self.write(start, data);
        start
    }

    fn folder(&mut self, parent: u32, entries: Vec<DirEnt>) -> u32 {
        let count = entries
            .iter()
            .map(|e| {
                1 + e
                    .long
                    .map_or(0, |l| (l.encode_utf16().count() + 1).div_ceil(13))
            })
            .sum::<usize>()
            + 2;
        let start = self.alloc(count * 32, false);
        let mut all = vec![
            DirEnt::dir(b".          ", None, start),
            DirEnt::dir(b"..         ", None, parent),
        ];
        all.extend(entries);
        let bytes = encode(&all);
        self.write(start, &bytes);
        start
    }

    fn finish(mut self, root: Vec<DirEnt>) -> Vec<u8> {
        let mut root_cluster = 0;
        let bytes = encode(&root);
        if self.width == 32 {
            root_cluster = self.alloc(bytes.len().max(512), false);
            self.write(root_cluster, &bytes);
        } else {
            let at = ((self.reserved + 2 * self.fat_sectors) * 512) as usize;
            self.image[at..at + bytes.len()].copy_from_slice(&bytes);
        }
        let b = &mut self.image[..512];
        b[0] = 0xeb;
        b[1] = 0x3c;
        b[2] = 0x90;
        b[3..11].copy_from_slice(b"ARCATEST");
        b[11..13].copy_from_slice(&512u16.to_le_bytes());
        b[13] = 1;
        b[14..16].copy_from_slice(&(self.reserved as u16).to_le_bytes());
        b[16] = 2;
        b[17..19].copy_from_slice(&(self.root_entries as u16).to_le_bytes());
        if self.total < 65536 {
            b[19..21].copy_from_slice(&(self.total as u16).to_le_bytes());
        } else {
            b[32..36].copy_from_slice(&(self.total as u32).to_le_bytes());
        }
        b[21] = 0xf8;
        if self.width == 32 {
            b[36..40].copy_from_slice(&(self.fat_sectors as u32).to_le_bytes());
            b[44..48].copy_from_slice(&root_cluster.to_le_bytes());
        } else {
            b[22..24].copy_from_slice(&(self.fat_sectors as u16).to_le_bytes());
        }
        b[510] = 0x55;
        b[511] = 0xaa;
        self.table[0] = 0x0fff_fff8 & self.end();
        self.table[1] = self.end();
        let mut fat = vec![0u8; (self.fat_sectors * 512) as usize];
        for (n, &v) in self.table.iter().enumerate() {
            match self.width {
                12 => {
                    let at = n + n / 2;
                    if n % 2 == 0 {
                        fat[at] = v as u8;
                        fat[at + 1] = (fat[at + 1] & 0xf0) | ((v >> 8) as u8 & 0x0f);
                    } else {
                        fat[at] = (fat[at] & 0x0f) | ((v as u8 & 0x0f) << 4);
                        fat[at + 1] = (v >> 4) as u8;
                    }
                }
                16 => fat[n * 2..n * 2 + 2].copy_from_slice(&(v as u16).to_le_bytes()),
                _ => fat[n * 4..n * 4 + 4].copy_from_slice(&v.to_le_bytes()),
            }
        }
        for copy in 0..2 {
            let at = ((self.reserved + copy * self.fat_sectors) * 512) as usize;
            self.image[at..at + fat.len()].copy_from_slice(&fat);
        }
        self.image
    }
}

fn sample(width: u32, looping: bool) -> Vec<u8> {
    let mut f = Fat::new(width);
    let readme = f.file(b"hello", false);
    let long = f.file(&long_content(), true);
    let note = f.file(b"deep down", false);
    let inner = f.file(&[7], false);
    let docs_parent = 0;
    let deeper_entries = vec![DirEnt::file(b"NOTE    TXT", None, 0x18, note, 9)];
    let deeper = f.folder(0, deeper_entries);
    let docs = f.folder(
        docs_parent,
        vec![
            DirEnt::file(b"INNER   BIN", None, 0x18, inner, 1),
            DirEnt::dir(b"DEEPER     ", Some("Deeper"), deeper),
        ],
    );
    if looping {
        f.table[docs as usize] = docs;
    }
    let mut label = DirEnt::file(b"TESTVOL    ", None, 0, 0, 0);
    label.attr = 0x08;
    let mut deleted = DirEnt::file(b"GONE    TXT", None, 0, readme, 5);
    deleted.short[0] = 0xe5;
    f.finish(vec![
        label,
        DirEnt::file(b"README  TXT", None, 0, readme, 5),
        deleted,
        DirEnt::file(b"ALONGF~1TXT", Some("A long file name.txt"), 0, long, 3000),
        DirEnt::file(b"EMPTY   DAT", None, 0x18, 0, 0),
        DirEnt::dir(b"DOCS       ", Some("Docs"), docs),
    ])
}

pub(crate) fn sample_fat(width: u32) -> Vec<u8> {
    sample(width, false)
}

pub(crate) fn sample_fat_looping() -> Vec<u8> {
    sample(12, true)
}

pub(crate) fn pad(bytes: &[u8]) -> Vec<u8> {
    let mut out = bytes.to_vec();
    out.resize(bytes.len().div_ceil(512) * 512, 0);
    out
}

fn mbr_slot(sector: &mut [u8], slot: usize, kind: u8, start: u64, count: u64) {
    let e = &mut sector[446 + 16 * slot..462 + 16 * slot];
    e[4] = kind;
    e[8..12].copy_from_slice(&(start as u32).to_le_bytes());
    e[12..16].copy_from_slice(&(count as u32).to_le_bytes());
}

/// An MBR disk: the primary partitions in order, then, when given, one
/// logical partition in an extended partition. Each starts 64 sectors on.
pub(crate) fn mbr(primaries: &[(u8, &[u8])], logical: Option<&[u8]>) -> Vec<u8> {
    let mut disk = vec![0u8; 64 * 512];
    disk[510] = 0x55;
    disk[511] = 0xaa;
    for (slot, (kind, data)) in primaries.iter().enumerate() {
        let start = (disk.len() / 512) as u64;
        let data = pad(data);
        mbr_slot(&mut disk, slot, *kind, start, (data.len() / 512) as u64);
        disk.extend_from_slice(&data);
        disk.resize(disk.len().div_ceil(64 * 512) * 64 * 512, 0);
    }
    if let Some(data) = logical {
        let base = (disk.len() / 512) as u64;
        let data = pad(data);
        let mut ebr = vec![0u8; 64 * 512];
        ebr[510] = 0x55;
        ebr[511] = 0xaa;
        mbr_slot(&mut ebr, 0, 0x06, 64, (data.len() / 512) as u64);
        let count = 64 + (data.len() / 512) as u64;
        mbr_slot(&mut disk, primaries.len(), 0x0f, base, count);
        disk.extend_from_slice(&ebr);
        disk.extend_from_slice(&data);
    }
    disk
}

pub(crate) fn guid_bytes() -> [u8; 16] {
    [
        0xa2, 0xa0, 0xd0, 0xeb, 0xe5, 0xb9, 0x33, 0x44, 0x87, 0xc0, 0x68, 0xb6, 0xb7, 0x26, 0x99,
        0xc7,
    ]
}

fn gpt_header(total: u64, mine: u64, other: u64, table: u64, entries_crc: u32) -> Vec<u8> {
    let mut h = vec![0u8; 512];
    h[..8].copy_from_slice(b"EFI PART");
    h[8..12].copy_from_slice(&0x0001_0000u32.to_le_bytes());
    h[12..16].copy_from_slice(&92u32.to_le_bytes());
    h[24..32].copy_from_slice(&mine.to_le_bytes());
    h[32..40].copy_from_slice(&other.to_le_bytes());
    h[40..48].copy_from_slice(&34u64.to_le_bytes());
    h[48..56].copy_from_slice(&(total - 34).to_le_bytes());
    h[72..80].copy_from_slice(&table.to_le_bytes());
    h[80..84].copy_from_slice(&128u32.to_le_bytes());
    h[84..88].copy_from_slice(&128u32.to_le_bytes());
    h[88..92].copy_from_slice(&entries_crc.to_le_bytes());
    let crc = crc32fast::hash(&h[..92]);
    h[16..20].copy_from_slice(&crc.to_le_bytes());
    h
}

/// A GPT disk with 512-byte sectors, its partitions 64 sectors apart, and
/// the backup header and table at the end.
pub(crate) fn gpt(parts: &[&[u8]]) -> Vec<u8> {
    let mut disk = vec![0u8; 64 * 512];
    let mut table = vec![0u8; 128 * 128];
    for (i, data) in parts.iter().enumerate() {
        let start = (disk.len() / 512) as u64;
        let data = pad(data);
        let last = start + (data.len() / 512) as u64 - 1;
        let e = &mut table[i * 128..(i + 1) * 128];
        e[..16].copy_from_slice(&guid_bytes());
        e[16] = i as u8 + 1;
        e[32..40].copy_from_slice(&start.to_le_bytes());
        e[40..48].copy_from_slice(&last.to_le_bytes());
        for (n, u) in "data".encode_utf16().enumerate() {
            e[56 + 2 * n..58 + 2 * n].copy_from_slice(&u.to_le_bytes());
        }
        disk.extend_from_slice(&data);
        disk.resize(disk.len().div_ceil(64 * 512) * 64 * 512, 0);
    }
    disk.resize(disk.len() + 33 * 512, 0);
    let total = (disk.len() / 512) as u64;
    let crc = crc32fast::hash(&table);
    mbr_slot(&mut disk, 0, 0xee, 1, total - 1);
    disk[510] = 0x55;
    disk[511] = 0xaa;
    disk[512..1024].copy_from_slice(&gpt_header(total, 1, total - 1, 2, crc));
    disk[1024..1024 + table.len()].copy_from_slice(&table);
    let backup_table = (total - 33) as usize * 512;
    disk[backup_table..backup_table + table.len()].copy_from_slice(&table);
    let backup = (total - 1) as usize * 512;
    disk[backup..backup + 512].copy_from_slice(&gpt_header(total, total - 1, 1, total - 33, crc));
    disk
}

pub(crate) fn fix_vhd_checksum(footer: &mut [u8]) {
    let sum = footer
        .iter()
        .enumerate()
        .filter(|(i, _)| !(64..68).contains(i))
        .fold(0u32, |sum, (_, &b)| sum.wrapping_add(u32::from(b)));
    footer[64..68].copy_from_slice(&(!sum).to_be_bytes());
}

fn vhd_footer(size: u64, kind: u32, data_offset: u64) -> Vec<u8> {
    let mut f = vec![0u8; 512];
    f[..8].copy_from_slice(b"conectix");
    f[8..12].copy_from_slice(&2u32.to_be_bytes());
    f[12..16].copy_from_slice(&0x0001_0000u32.to_be_bytes());
    f[16..24].copy_from_slice(&data_offset.to_be_bytes());
    f[28..32].copy_from_slice(b"arca");
    f[36..40].copy_from_slice(b"Wi2k");
    f[40..48].copy_from_slice(&size.to_be_bytes());
    f[48..56].copy_from_slice(&size.to_be_bytes());
    f[60..64].copy_from_slice(&kind.to_be_bytes());
    f[68..84].copy_from_slice(&[0x42; 16]);
    fix_vhd_checksum(&mut f);
    f
}

pub(crate) fn vhd_fixed(disk: &[u8]) -> Vec<u8> {
    let mut out = disk.to_vec();
    out.extend_from_slice(&vhd_footer(disk.len() as u64, 2, u64::MAX));
    out
}

pub(crate) fn vhd_dynamic(disk: &[u8]) -> Vec<u8> {
    const BLOCK: usize = 4096;
    let size = disk.len() as u64;
    let footer = vhd_footer(size, 3, 512);
    let blocks = disk.len().div_ceil(BLOCK);
    let mut header = vec![0u8; 1024];
    header[..8].copy_from_slice(b"cxsparse");
    header[8..16].copy_from_slice(&u64::MAX.to_be_bytes());
    header[16..24].copy_from_slice(&1536u64.to_be_bytes());
    header[24..28].copy_from_slice(&0x0001_0000u32.to_be_bytes());
    header[28..32].copy_from_slice(&(blocks as u32).to_be_bytes());
    header[32..36].copy_from_slice(&(BLOCK as u32).to_be_bytes());
    let sum = header
        .iter()
        .enumerate()
        .filter(|(i, _)| !(36..40).contains(i))
        .fold(0u32, |sum, (_, &b)| sum.wrapping_add(u32::from(b)));
    header[36..40].copy_from_slice(&(!sum).to_be_bytes());
    let mut out = footer.clone();
    out.extend_from_slice(&header);
    let table_at = out.len();
    out.resize(table_at + (blocks * 4).div_ceil(512) * 512, 0xff);
    for b in 0..blocks {
        let mut block = disk[b * BLOCK..disk.len().min((b + 1) * BLOCK)].to_vec();
        if block.iter().all(|&x| x == 0) {
            continue;
        }
        block.resize(BLOCK, 0);
        let sector = (out.len() / 512) as u32;
        out[table_at + 4 * b..table_at + 4 * b + 4].copy_from_slice(&sector.to_be_bytes());
        out.extend_from_slice(&[0xff; 512]);
        out.extend_from_slice(&block);
    }
    out.extend_from_slice(&footer);
    out
}
