use arca_core::{Error, Result};
use std::collections::HashSet;

use crate::device::{le32, le64, Device};
use crate::fs;

const MAX_LOGICAL: usize = 128;
const MAX_GPT_ENTRIES: u64 = 4096;

#[derive(Clone, Debug)]
pub(crate) struct Partition {
    pub start: u64,
    pub len: u64,
}

/// The partitions of a disk, from its GPT or its MBR, or `None` when the
/// disk has no partition table: a file system that starts at its first
/// byte, or nothing Arca knows.
pub(crate) fn scan(
    dev: &mut dyn Device,
    notices: &mut Vec<String>,
) -> Result<Option<Vec<Partition>>> {
    if dev.len() < 512 {
        return Ok(None);
    }
    let mut mbr = [0u8; 512];
    dev.read_at(0, &mut mbr)?;
    if mbr[510..512] != [0x55, 0xaa] || fs::boot_sector_kind(&mbr).is_some() {
        return gpt(dev, notices);
    }
    let slots: Vec<&[u8]> = (0..4).map(|i| &mbr[446 + 16 * i..462 + 16 * i]).collect();
    if slots.iter().any(|s| s[0] != 0 && s[0] != 0x80) {
        return gpt(dev, notices);
    }
    if slots.iter().any(|s| s[4] == 0xee) {
        if let Some(found) = gpt(dev, notices)? {
            return Ok(Some(found));
        }
    }
    let mut found = Vec::new();
    for s in &slots {
        let (kind, start, count) = (s[4], u64::from(le32(s, 8)), u64::from(le32(s, 12)));
        if kind == 0 || count == 0 || kind == 0xee {
            continue;
        }
        if matches!(kind, 0x05 | 0x0f | 0x85) {
            logical(dev, start, &mut found, notices)?;
        } else {
            push(dev, &mut found, start * 512, count * 512, notices);
        }
    }
    Ok(Some(found))
}

fn push(
    dev: &dyn Device,
    found: &mut Vec<Partition>,
    start: u64,
    len: u64,
    notices: &mut Vec<String>,
) {
    let size = dev.len();
    if start >= size {
        notices.push(format!(
            "a partition starts past the end of the disk, at byte {start}, and is not listed"
        ));
        return;
    }
    let len = if start.saturating_add(len) > size {
        notices.push(format!(
            "partition {} runs past the end of the disk and is cut where the disk ends",
            found.len()
        ));
        size - start
    } else {
        len
    };
    found.push(Partition { start, len });
}

// Each extended boot record holds one logical partition, placed from the
// record itself, and a link to the next record, placed from the start of the
// extended partition.
fn logical(
    dev: &mut dyn Device,
    base: u64,
    found: &mut Vec<Partition>,
    notices: &mut Vec<String>,
) -> Result<()> {
    let mut next = base;
    let mut seen = HashSet::new();
    for _ in 0..MAX_LOGICAL {
        if !seen.insert(next) {
            notices.push("the chain of logical partitions loops and is cut there".into());
            return Ok(());
        }
        let Some(at) = next
            .checked_mul(512)
            .filter(|at| at.saturating_add(512) <= dev.len())
        else {
            notices.push("a logical partition record lies past the end of the disk".into());
            return Ok(());
        };
        let mut ebr = [0u8; 512];
        dev.read_at(at, &mut ebr)?;
        if ebr[510..512] != [0x55, 0xaa] {
            notices.push(
                "a logical partition record is damaged; the ones after it are not listed".into(),
            );
            return Ok(());
        }
        let (own, link) = (&ebr[446..462], &ebr[462..478]);
        let count = u64::from(le32(own, 12));
        if own[4] != 0 && count != 0 {
            let start = next.saturating_add(u64::from(le32(own, 8)));
            push(dev, found, start.saturating_mul(512), count * 512, notices);
        }
        if link[4] == 0 || le32(link, 12) == 0 {
            return Ok(());
        }
        next = base.saturating_add(u64::from(le32(link, 8)));
    }
    notices.push(format!(
        "only the first {MAX_LOGICAL} logical partitions are listed"
    ));
    Ok(())
}

fn gpt(dev: &mut dyn Device, notices: &mut Vec<String>) -> Result<Option<Vec<Partition>>> {
    let mut sizes = vec![dev.sector()];
    for s in [512, 4096] {
        if !sizes.contains(&s) {
            sizes.push(s);
        }
    }
    for sector in sizes {
        if let Some(found) = gpt_at(dev, sector, sector, notices)? {
            return Ok(Some(found));
        }
        let last = (dev.len() / sector).saturating_sub(1);
        if last > 1 {
            if let Some(found) = gpt_at(dev, sector, last * sector, notices)? {
                notices.push("the main GPT header is damaged; the backup copy was read".into());
                return Ok(Some(found));
            }
        }
    }
    Ok(None)
}

fn gpt_at(
    dev: &mut dyn Device,
    sector: u64,
    at: u64,
    notices: &mut Vec<String>,
) -> Result<Option<Vec<Partition>>> {
    if at.saturating_add(sector) > dev.len() || sector < 512 {
        return Ok(None);
    }
    let mut header = vec![0u8; sector as usize];
    dev.read_at(at, &mut header)?;
    if &header[..8] != b"EFI PART" {
        return Ok(None);
    }
    let size = le32(&header, 12) as usize;
    if !(92..=header.len()).contains(&size) {
        return Ok(None);
    }
    let stored = le32(&header, 16);
    let mut copy = header[..size].to_vec();
    copy[16..20].fill(0);
    if crc32fast::hash(&copy) != stored {
        return Ok(None);
    }
    let first = le64(&header, 72);
    let count = u64::from(le32(&header, 80));
    let entry = u64::from(le32(&header, 84));
    if !(128..=4096).contains(&entry) || entry % 8 != 0 {
        return Err(Error::Format(format!("GPT entries of {entry} bytes")));
    }
    if count > MAX_GPT_ENTRIES {
        return Err(Error::Limit(format!("a GPT of {count} entries")));
    }
    let table_len = count * entry;
    let table_at = first
        .checked_mul(sector)
        .filter(|start| start.saturating_add(table_len) <= dev.len())
        .ok_or_else(|| Error::Format("the GPT entries lie past the end of the disk".into()))?;
    let mut table = vec![0u8; table_len as usize];
    dev.read_at(table_at, &mut table)?;
    if crc32fast::hash(&table) != le32(&header, 88) {
        return Ok(None);
    }
    let mut found = Vec::new();
    for e in table.chunks_exact(entry as usize) {
        if e[..16].iter().all(|&b| b == 0) {
            continue;
        }
        let (lo, hi) = (le64(e, 32), le64(e, 40));
        if hi < lo {
            notices.push("a GPT entry ends before it starts and is not listed".into());
            continue;
        }
        let start = lo.saturating_mul(sector);
        let len = (hi - lo + 1).saturating_mul(sector);
        push(dev, &mut found, start, len, notices);
    }
    Ok(Some(found))
}
