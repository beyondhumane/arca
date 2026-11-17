use arca_core::{limits, Entry, Error, Method, Result};

use crate::device::{le16, Volume};

pub(crate) const MAX_DEPTH: usize = 128;
const MAX_RUNS: usize = 4 * 1024 * 1024;
pub(crate) const MAX_FOLDER_BYTES: u64 = 1024 * 1024 * 1024;
const CHAIN_SLACK: u64 = 64 * 1024;
const CHAIN_CHECK: u64 = 4096;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Run {
    Disk { at: u64, len: u64 },
    Zero(u64),
    Inline(Vec<u8>),
}

impl Run {
    pub fn len(&self) -> u64 {
        match self {
            Run::Disk { len, .. } | Run::Zero(len) => *len,
            Run::Inline(bytes) => bytes.len() as u64,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    File,
    Dir,
    Symlink(String),
}

#[derive(Clone, Debug)]
pub(crate) struct Member {
    pub entry: Entry,
    pub kind: Kind,
    pub runs: Vec<Run>,
}

#[derive(Default)]
pub(crate) struct Listing {
    pub members: Vec<Member>,
    pub notices: Vec<String>,
}

impl Listing {
    pub fn push(&mut self, member: Member) -> Result<()> {
        if self.members.len() as u64 >= limits::MAX_ENTRIES {
            return Err(Error::Limit("too many entries".into()));
        }
        self.members.push(member);
        Ok(())
    }
}

/// Counts the data runs of a whole listing, so that a badly fragmented or a
/// crafted volume cannot make the index outgrow memory.
#[derive(Default)]
pub(crate) struct Runs {
    total: usize,
}

impl Runs {
    pub fn add(&mut self, runs: &mut Vec<Run>, run: Run) -> Result<()> {
        if run.len() == 0 {
            return Ok(());
        }
        match (runs.last_mut(), &run) {
            (
                Some(Run::Disk { at, len }),
                Run::Disk {
                    at: next,
                    len: more,
                },
            ) if at.checked_add(*len) == Some(*next) => {
                *len += more;
                return Ok(());
            }
            (Some(Run::Zero(len)), Run::Zero(more)) => {
                *len += more;
                return Ok(());
            }
            _ => {}
        }
        self.total += 1;
        if self.total > MAX_RUNS {
            return Err(Error::Limit(
                "the files are split into too many pieces".into(),
            ));
        }
        runs.push(run);
        Ok(())
    }
}

/// The cluster-chain steps the files of one volume take between them. Each
/// cluster of a sound volume belongs to one file at most, so the chains of
/// all its files add up to no more steps than it has clusters; past that,
/// chains cross, and the files left are cut short.
pub(crate) struct Chains<'a> {
    left: u64,
    taken: u64,
    carry_on: &'a dyn Fn() -> bool,
    looped: usize,
    spent: bool,
}

impl<'a> Chains<'a> {
    pub fn new(clusters: u32, carry_on: &'a dyn Fn() -> bool) -> Self {
        Chains {
            left: u64::from(clusters) + CHAIN_SLACK,
            taken: 0,
            carry_on,
            looped: 0,
            spent: false,
        }
    }

    /// Takes one more step: false once the volume's steps are spent.
    pub fn step(&mut self) -> Result<bool> {
        self.taken += 1;
        if self.taken.is_multiple_of(CHAIN_CHECK) && !(self.carry_on)() {
            return Err(Error::Cancelled);
        }
        if self.left == 0 {
            self.spent = true;
            return Ok(false);
        }
        self.left -= 1;
        Ok(true)
    }

    pub fn looped(&mut self) {
        self.looped += 1;
    }

    pub fn notices(&self, notices: &mut Vec<String>) {
        if self.looped > 0 {
            notices.push(format!(
                "{} files have a cluster chain that loops; they are cut where it loops",
                self.looped
            ));
        }
        if self.spent {
            notices.push(
                "the cluster chains of the files cross one another; the files whose chains ran past the volume's clusters are cut short"
                    .into(),
            );
        }
    }
}

/// Brent's cycle detection over the clusters of one chain, in constant
/// memory: a chain that loops is caught within a few turns of its loop.
pub(crate) struct Cycle {
    mark: u32,
    power: u64,
    lap: u64,
}

impl Cycle {
    pub fn new(first: u32) -> Self {
        Cycle {
            mark: first,
            power: 1,
            lap: 0,
        }
    }

    /// Whether `next`, the cluster after the last one, closes a loop.
    pub fn closes(&mut self, next: u32) -> bool {
        if next == self.mark {
            return true;
        }
        self.lap += 1;
        if self.lap == self.power {
            self.mark = next;
            self.power *= 2;
            self.lap = 0;
        }
        false
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Fs {
    Fat,
    Exfat,
    Ntfs,
    Hfs,
    Iso,
}

impl Fs {
    pub fn tag(self) -> &'static str {
        match self {
            Fs::Fat => "fat",
            Fs::Exfat => "exfat",
            Fs::Ntfs => "ntfs",
            Fs::Hfs => "hfs",
            Fs::Iso => "iso",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Fs::Fat => "FAT",
            Fs::Exfat => "exFAT",
            Fs::Ntfs => "NTFS",
            Fs::Hfs => "HFS+",
            Fs::Iso => "ISO 9660",
        }
    }
}

/// The file system a first sector says it holds: NTFS, exFAT or FAT.
pub(crate) fn boot_sector_kind(b: &[u8]) -> Option<Fs> {
    if b.len() < 512 {
        return None;
    }
    if &b[3..11] == b"NTFS    " {
        return Some(Fs::Ntfs);
    }
    if &b[3..11] == b"EXFAT   " {
        return Some(Fs::Exfat);
    }
    crate::fat::Bpb::parse(b).ok().map(|_| Fs::Fat)
}

/// The file system on a volume, from its first sector, the HFS+ volume
/// header at byte 1024 or the ISO 9660 descriptor at byte 32769.
pub(crate) fn detect(vol: &mut Volume<'_>) -> Result<Option<Fs>> {
    if vol.len >= 512 {
        let mut first = [0u8; 512];
        vol.read(0, &mut first)?;
        if let Some(fs) = boot_sector_kind(&first) {
            return Ok(Some(fs));
        }
    }
    if vol.len >= 1536 {
        let mut header = [0u8; 4];
        vol.read(1024, &mut header)?;
        if matches!(&header, b"H+\0\x04" | b"HX\0\x05") {
            return Ok(Some(Fs::Hfs));
        }
    }
    if vol.len >= 16 * 2048 + 2048 {
        let mut id = [0u8; 5];
        vol.read(16 * 2048 + 1, &mut id)?;
        if &id == b"CD001" {
            return Ok(Some(Fs::Iso));
        }
    }
    Ok(None)
}

/// The listing of one file system. `runs` is the budget of data runs the
/// whole disk shares, whatever partition the runs are in.
pub(crate) fn list(
    fs: Fs,
    vol: &mut Volume<'_>,
    runs: &mut Runs,
    carry_on: &dyn Fn() -> bool,
) -> Result<Listing> {
    match fs {
        Fs::Fat => crate::fat::list(vol, runs, carry_on),
        Fs::Exfat => crate::exfat::list(vol, runs, carry_on),
        Fs::Ntfs => crate::ntfs::list(vol, runs, carry_on),
        Fs::Hfs => crate::hfs::list(vol, runs, carry_on),
        Fs::Iso => iso(vol, runs, carry_on),
    }
}

fn iso(vol: &mut Volume<'_>, runs: &mut Runs, carry_on: &dyn Fn() -> bool) -> Result<Listing> {
    let base = vol.base;
    let mut reader = crate::device::VolumeReader {
        volume: vol,
        pos: 0,
    };
    let image = arca_iso::index(&mut reader, carry_on)?;
    let mut listing = Listing {
        notices: image.notices,
        ..Listing::default()
    };
    for (entry, extents) in image.entries.into_iter().zip(image.extents) {
        let mut data = Vec::new();
        for (start, len) in extents {
            runs.add(
                &mut data,
                Run::Disk {
                    at: base.saturating_add(start),
                    len,
                },
            )?;
        }
        let kind = if entry.is_dir { Kind::Dir } else { Kind::File };
        listing.push(Member {
            entry,
            kind,
            runs: data,
        })?;
    }
    Ok(listing)
}

pub(crate) fn entry(name: String, size: u64, is_dir: bool, mtime: Option<i64>) -> Entry {
    Entry {
        raw_name: name.as_bytes().to_vec(),
        name,
        size: if is_dir { 0 } else { size },
        compressed_size: if is_dir { 0 } else { size },
        method: Method::Store,
        crc32: None,
        is_dir,
        mtime,
        created: None,
        accessed: None,
        attributes: 0,
        offset: 0,
        utf8: true,
        encrypted: false,
        zipcrypto: false,
    }
}

/// Whether a name read from a directory can stand as one path component.
pub(crate) fn usable(name: &str) -> bool {
    !name.is_empty()
        && name != "."
        && name != ".."
        && name.len() <= limits::MAX_NAME
        && !name.contains(['/', '\0'])
}

pub(crate) fn join(parent: &str, name: &str) -> String {
    if parent.is_empty() {
        name.to_string()
    } else {
        format!("{parent}/{name}")
    }
}

pub(crate) fn utf16(units: &[u16]) -> String {
    char::decode_utf16(units.iter().copied())
        .map(|c| c.unwrap_or(char::REPLACEMENT_CHARACTER))
        .collect()
}

pub(crate) fn utf16_le(bytes: &[u8]) -> String {
    let units: Vec<u16> = (0..bytes.len() / 2).map(|i| le16(bytes, i * 2)).collect();
    utf16(&units)
}

/// Windows FILETIME, 100 ns since 1601, as Unix seconds.
pub(crate) fn filetime(ft: u64) -> Option<i64> {
    if ft == 0 {
        return None;
    }
    Some((ft / 10_000_000) as i64 - 11_644_473_600)
}
