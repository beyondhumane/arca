//! Arca: in-memory storage for native member spools.
//! Capacity is admitted before allocation; a reallocation counts the old and
//! replacement buffers together while one is copied into the other.
use super::CapacityCharge;
use std::io::{self, Read, Seek, SeekFrom, Write};

pub(super) struct ResidentSpool {
    bytes: Vec<u8>,
    pos: u64,
    limit: usize,
    // Drop the payload before releasing its shared charge.
    charge: CapacityCharge,
}

impl ResidentSpool {
    pub(super) fn new(charge: CapacityCharge, limit: usize) -> Self {
        Self {
            bytes: Vec::new(),
            pos: 0,
            limit,
            charge,
        }
    }

    pub(super) fn contents(&self) -> &[u8] {
        &self.bytes
    }

    /// Makes room for `end` bytes. False means the bytes belong on disk: the
    /// spool would pass its own limit, the shared ledgers refused the
    /// capacity, or the allocator did. Nothing changes when it is false.
    pub(super) fn reserve(&mut self, end: u64) -> bool {
        let Ok(end) = usize::try_from(end) else {
            return false;
        };
        if end > self.limit {
            return false;
        }
        let old = self.bytes.capacity();
        if end <= old {
            return true;
        }
        let target = end.max(old.saturating_mul(2)).min(self.limit);
        if self.charge.grow_to(old as u64 + target as u64).is_err() {
            return false;
        }
        let mut replacement = Vec::new();
        let admitted = replacement.try_reserve_exact(target).is_ok()
            && self
                .charge
                .grow_to(old as u64 + replacement.capacity() as u64)
                .is_ok();
        if !admitted {
            drop(replacement);
            self.charge.shrink_to(old as u64);
            return false;
        }
        replacement.extend_from_slice(&self.bytes);
        self.bytes = replacement;
        self.charge.shrink_to(self.bytes.capacity() as u64);
        true
    }
}

impl Write for ResidentSpool {
    fn write(&mut self, data: &[u8]) -> io::Result<usize> {
        if data.is_empty() {
            return Ok(0);
        }
        let end = usize::try_from(self.pos)
            .ok()
            .and_then(|pos| pos.checked_add(data.len()))
            .filter(|&end| end <= self.bytes.capacity())
            .ok_or_else(|| io::Error::other("resident spool write past its admitted capacity"))?;
        let pos = end - data.len();
        if self.bytes.len() < pos {
            self.bytes.resize(pos, 0);
        }
        let overlap = (self.bytes.len() - pos).min(data.len());
        self.bytes[pos..pos + overlap].copy_from_slice(&data[..overlap]);
        self.bytes.extend_from_slice(&data[overlap..]);
        self.pos = end as u64;
        Ok(data.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl Read for ResidentSpool {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        let Some(available) = usize::try_from(self.pos)
            .ok()
            .and_then(|pos| self.bytes.get(pos..))
        else {
            return Ok(0);
        };
        let count = available.len().min(out.len());
        out[..count].copy_from_slice(&available[..count]);
        self.pos += count as u64;
        Ok(count)
    }
}

impl Seek for ResidentSpool {
    fn seek(&mut self, from: SeekFrom) -> io::Result<u64> {
        let pos = match from {
            SeekFrom::Start(pos) => Some(pos),
            SeekFrom::End(offset) => (self.bytes.len() as u64).checked_add_signed(offset),
            SeekFrom::Current(offset) => self.pos.checked_add_signed(offset),
        }
        .ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "invalid resident spool seek")
        })?;
        self.pos = pos;
        Ok(pos)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::streaming::WriterResources;

    fn spool(resources: &WriterResources, limit: usize) -> ResidentSpool {
        ResidentSpool::new(resources.resident_spool_charge().unwrap(), limit)
    }

    fn used(resources: &WriterResources) -> u64 {
        *resources
            .spool_memory_budget
            .as_ref()
            .unwrap()
            .used
            .lock()
            .unwrap()
    }

    #[test]
    fn growth_charges_capacity_and_refuses_without_changing_anything() {
        let resources = WriterResources::default().with_max_spool_memory_bytes(64);
        let mut spool = spool(&resources, 1024);
        assert_eq!(used(&resources), 0);
        assert!(spool.reserve(5));
        spool.write_all(b"hello").unwrap();
        assert_eq!(used(&resources), 5);
        assert!(spool.reserve(20));
        assert_eq!(used(&resources), 20);
        assert!(!spool.reserve(65), "the shared cap counts old and new");
        assert_eq!(used(&resources), 20);
        assert_eq!(spool.contents(), b"hello");
        drop(spool);
        assert_eq!(used(&resources), 0);
    }

    #[test]
    fn the_per_spool_limit_refuses_before_any_charge() {
        let resources = WriterResources::default().with_max_spool_memory_bytes(1 << 20);
        let mut spool = spool(&resources, 8);
        assert!(spool.reserve(8));
        assert!(!spool.reserve(9));
        assert!(!spool.reserve(u64::MAX));
        assert_eq!(used(&resources), 8);
    }

    #[test]
    fn writes_overwrite_extend_and_fill_holes_with_zeros() {
        let resources = WriterResources::default().with_max_spool_memory_bytes(1 << 20);
        let mut spool = spool(&resources, 1024);
        assert!(spool.reserve(6));
        spool.write_all(b"abcdef").unwrap();
        spool.seek(SeekFrom::Start(1)).unwrap();
        spool.write_all(b"XY").unwrap();
        spool.seek(SeekFrom::Start(9)).unwrap();
        assert!(spool.reserve(10));
        spool.write_all(b"Z").unwrap();
        assert_eq!(spool.contents(), b"aXYdef\0\0\0Z");
        spool.seek(SeekFrom::End(-2)).unwrap();
        let mut tail = Vec::new();
        spool.read_to_end(&mut tail).unwrap();
        assert_eq!(tail, b"\0Z");
        spool.seek(SeekFrom::Start(100)).unwrap();
        assert_eq!(spool.read(&mut [0; 4]).unwrap(), 0);
        assert!(spool.seek(SeekFrom::Current(-200)).is_err());
    }

    #[test]
    fn unadmitted_writes_fail_instead_of_allocating() {
        let resources = WriterResources::default().with_max_spool_memory_bytes(1 << 20);
        let mut spool = spool(&resources, 1024);
        assert!(spool.write(b"x").is_err());
        spool.seek(SeekFrom::Start(u64::MAX)).unwrap();
        assert!(spool.write(b"x").is_err());
        assert_eq!(spool.write(b"").unwrap(), 0);
        assert_eq!(used(&resources), 0);
    }

    #[test]
    fn the_execution_ledger_is_charged_alongside_the_cap() {
        let ledger = crate::codec::workspace::Allowance::limited(16);
        let resources = WriterResources::default()
            .with_execution_allowance(ledger.clone())
            .with_max_spool_memory_bytes(1 << 20);
        let mut spool = spool(&resources, 1024);
        assert!(spool.reserve(16));
        assert_eq!(ledger.used(), 16);
        assert!(!spool.reserve(17), "the ledger refuses the replacement");
        assert_eq!(ledger.used(), 16);
        assert_eq!(used(&resources), 16);
        drop(spool);
        assert_eq!(ledger.used(), 0);
        assert_eq!(used(&resources), 0);
    }
}
