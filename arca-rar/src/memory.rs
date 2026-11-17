use arca_core::{Error, Level, Result};
use std::sync::Arc;

#[cfg(feature = "rar")]
use super::{create_limits as limits, reader, writer};
#[cfg(feature = "rar")]
use rars::rar50::{
    write_streaming_archive_with_progress, ArchiveEntry, ArchiveExtras, FilterPolicy, WriterOptions,
};
#[cfg(feature = "rar")]
use rars::{ArchiveReader, ArchiveVersion, EntrySource, FeatureSet, WriterResources};

/// Packs `members` into a new non-solid RAR5 archive held in memory, named
/// `0`, `1`, ... in order, with the compressor [`super::create_rar`] uses.
/// Payloads stay in memory up to 1 MiB each and 128 MiB together; past that
/// they spool to the current directory. Members are compressed side by side
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
            .with_max_spool_memory_bytes(writer::RESIDENT_SPOOL_BYTES);
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
            ArchiveReader::read(bytes)
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
                .read_member_at(index, None)
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
