use arca_core::{Error, Level, Result};
use std::sync::Arc;

#[cfg(feature = "rar")]
use super::{create_limits as limits, reader, writer};
#[cfg(feature = "rar")]
use rars::rar50::{
    write_streaming_archive_with_progress, ArchiveEntry, ArchiveExtras, FilterPolicy, WriterOptions,
};
#[cfg(feature = "rar")]
use rars::{
    ArchiveReader, ArchiveVersion, EntrySource, FeatureSet, ReadCancellation, WriterResources,
};

/// Packs `members` into a new non-solid RAR5 archive held in memory, named
/// `0`, `1`, ... in order, with the compressor [`super::create_rar`] uses.
/// Payloads stay in memory up to 1 MiB each and 128 MiB together; past that
/// they spool to the system temporary directory, within the bounds
/// [`super::create_rar`] sets. Members are compressed side by side
/// on the threads of the current rayon pool.
pub fn pack_in_memory(members: &[Arc<[u8]>], level: Level) -> Result<Vec<u8>> {
    #[cfg(feature = "rar")]
    {
        let entries = members
            .iter()
            .enumerate()
            .map(|(i, data)| {
                let name = rars::validate_entry_name(i.to_string().into_bytes())
                    .map_err(writer::write_error)?;
                Ok(
                    ArchiveEntry::new(name, EntrySource::from_bytes(Arc::clone(data)))
                        .with_attributes(0x20),
                )
            })
            .collect::<Result<Vec<_>>>()?;
        let options = WriterOptions::new(ArchiveVersion::Rar50, FeatureSet::store_only())
            .with_compression_level(writer::rar_level(level));
        let extras = ArchiveExtras::default().with_filter_policy(FilterPolicy::None);
        let resources = WriterResources::new(limits::WRITER_MEMORY_BYTES)
            .with_max_memory_bytes(limits::WRITER_MEMORY_BYTES)
            .with_max_spool_bytes(limits::MAX_SPOOL_BYTES)
            .with_max_spool_memory_bytes(writer::RESIDENT_SPOOL_BYTES)
            .with_temp_dir(std::env::temp_dir());
        let mut out = Vec::new();
        write_streaming_archive_with_progress(
            &entries, options, extras, &resources, None, &mut out,
        )
        .map_err(writer::write_error)?;
        Ok(out)
    }
    #[cfg(not(feature = "rar"))]
    {
        let _ = (members, level);
        Err(Error::Unsupported(
            "RAR creation is disabled in this build; rebuild with --features rar".into(),
        ))
    }
}

/// A RAR archive read from memory, whose members are decoded one at a
/// time, each checked against its stored checksum.
pub struct MemoryArchive {
    #[cfg(feature = "rar")]
    inner: rars::Archive,
}

impl MemoryArchive {
    pub fn open(bytes: &[u8]) -> Result<Self> {
        #[cfg(feature = "rar")]
        {
            ArchiveReader::read_with_options(bytes, reader::options(None, &ReadCancellation::new()))
                .map(|inner| Self { inner })
                .map_err(reader::error)
        }
        #[cfg(not(feature = "rar"))]
        {
            let _ = bytes;
            Err(super::disabled())
        }
    }

    /// The decoded bytes of the member at `index`, in archive order.
    pub fn member(&self, index: usize) -> Result<Vec<u8>> {
        #[cfg(feature = "rar")]
        {
            self.inner
                .read_member_at_with_options(index, reader::options(None, &ReadCancellation::new()))
                .map_err(reader::error)?
                .ok_or_else(|| Error::Format(format!("RAR: there is no member {index}")))
        }
        #[cfg(not(feature = "rar"))]
        {
            let _ = index;
            Err(Error::Format(format!("RAR: there is no member {index}")))
        }
    }
}

#[cfg(all(test, feature = "rar"))]
mod tests {
    use super::*;

    #[test]
    fn members_come_back_as_they_went_in() {
        let text: Arc<[u8]> = b"arca ".repeat(20_000).into();
        let empty: Arc<[u8]> = Vec::new().into();
        let packed =
            pack_in_memory(&[Arc::clone(&text), Arc::clone(&empty)], Level::NORMAL).unwrap();
        assert!(packed.len() < text.len() / 10);
        let archive = MemoryArchive::open(&packed).unwrap();
        assert_eq!(archive.member(0).unwrap(), &text[..]);
        assert!(archive.member(1).unwrap().is_empty());
        assert!(archive.member(2).is_err());
    }

    fn vint(data: &[u8], at: &mut usize) -> u64 {
        let mut value = 0;
        for shift in (0..63).step_by(7) {
            let byte = data[*at];
            *at += 1;
            value |= u64::from(byte & 0x7f) << shift;
            if byte & 0x80 == 0 {
                break;
            }
        }
        value
    }

    fn encoded(mut value: u64) -> Vec<u8> {
        let mut bytes = Vec::new();
        while value >= 0x80 {
            bytes.push(value as u8 | 0x80);
            value >>= 7;
        }
        bytes.push(value as u8);
        bytes
    }

    fn claiming(packed: &[u8], unpacked: u64) -> Vec<u8> {
        let mut out = packed[..8].to_vec();
        let mut cursor = 8;
        while cursor < packed.len() {
            cursor += 4;
            let size = vint(packed, &mut cursor) as usize;
            let end = cursor + size;
            let mut head = packed[cursor..end].to_vec();
            let mut at = 0;
            let kind = vint(&head, &mut at);
            let flags = vint(&head, &mut at);
            if flags & 1 != 0 {
                vint(&head, &mut at);
            }
            let data_len = if flags & 2 != 0 {
                vint(&head, &mut at) as usize
            } else {
                0
            };
            if kind == 2 {
                vint(&head, &mut at);
                let start = at;
                vint(&head, &mut at);
                head.splice(start..at, encoded(unpacked));
            }
            let mut header = encoded(head.len() as u64);
            header.extend(head);
            out.extend(arca_core::crc32(&header).to_le_bytes());
            out.extend(header);
            out.extend_from_slice(&packed[end..end + data_len]);
            cursor = end + data_len;
        }
        out
    }

    #[test]
    fn members_past_the_read_limits_are_refused() {
        let data: Arc<[u8]> = vec![7; 1000].into();
        let packed = pack_in_memory(&[data], Level::STORE).unwrap();
        assert_eq!(
            MemoryArchive::open(&claiming(&packed, 1000))
                .unwrap()
                .member(0)
                .unwrap(),
            vec![7; 1000]
        );
        let result = MemoryArchive::open(&claiming(&packed, 5 << 30)).and_then(|a| a.member(0));
        assert!(matches!(result, Err(Error::Limit(_))), "{result:?}");
    }

    #[test]
    fn a_truncated_archive_is_an_error() {
        let data: Arc<[u8]> = b"arca ".repeat(1000).into();
        let packed = pack_in_memory(&[data], Level::NORMAL).unwrap();
        for cut in [0, 7, packed.len() / 2, packed.len() - 1] {
            let opened = MemoryArchive::open(&packed[..cut]);
            assert!(opened.is_err() || opened.unwrap().member(0).is_err());
        }
    }
}

#[cfg(all(test, not(feature = "rar")))]
mod disabled_tests {
    use super::*;

    #[test]
    fn without_the_feature_nothing_is_packed_or_read() {
        assert!(matches!(
            pack_in_memory(&[], Level::NORMAL),
            Err(Error::Unsupported(_))
        ));
        assert!(matches!(
            MemoryArchive::open(b"Rar!"),
            Err(Error::Unsupported(_))
        ));
    }
}
