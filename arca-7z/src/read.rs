use std::io::{self, Read, Seek, SeekFrom};

use arca_core::{Entry, Error, Method, Result};
use sevenz_rust2::{Archive, BlockDecoder, EncoderMethod as M, Password};

use crate::{entry_path, upstream};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Validate,
    Extract,
    Test,
    Create,
}

#[derive(Clone, Copy, Debug)]
pub struct Progress<'a> {
    pub phase: Phase,
    pub entries_done: usize,
    pub entries_total: usize,
    pub bytes_done: u64,
    pub name: &'a str,
}

pub struct SevenZArchive<R: Read + Seek> {
    source: R,
    archive: Archive,
    entries: Vec<Entry>,
    password: Password,
    encrypted: bool,
}

impl<R: Read + Seek> SevenZArchive<R> {
    pub fn open(mut source: R, password: Option<&str>) -> Result<Self> {
        let len = source.seek(SeekFrom::End(0))?;
        source.seek(SeekFrom::Start(0))?;
        let mut header = [0u8; 32];
        source.read_exact(&mut header)?;
        if &header[..6] != b"7z\xbc\xaf\x27\x1c" || header[6] != 0 {
            return Err(Error::Format("invalid 7z signature or version".into()));
        }
        let mut fields = arca_core::Cursor::at(&header, 8)?;
        let crc = fields.u32le("7z start CRC")?;
        let offset = fields.u64le("7z next header offset")?;
        let size = fields.u64le("7z next header size")?;
        let end = offset.checked_add(32).and_then(|x| x.checked_add(size));
        if size == 0 || end.is_none_or(|end| end > len) {
            return Err(Error::Format(
                "truncated or invalid 7z header bounds".into(),
            ));
        }
        if crc32fast::hash(&header[12..]) != crc {
            return Err(Error::Format("7z start header CRC mismatch".into()));
        }
        let password = Password::from(password.unwrap_or(""));
        let archive = Archive::read(&mut source, &password).map_err(|e| upstream(e, false))?;
        let packed_end = archive.pack_sizes().iter().try_fold(
            archive
                .pack_pos()
                .checked_add(32)
                .ok_or_else(|| Error::Format("7z offset overflow".into()))?,
            |sum, &size| {
                sum.checked_add(size)
                    .ok_or_else(|| Error::Format("7z packed size overflow".into()))
            },
        )?;
        if packed_end > len {
            return Err(Error::Format("truncated 7z packed streams".into()));
        }
        let encrypted_blocks: Vec<bool> = archive
            .blocks
            .iter()
            .map(|b| {
                b.coders
                    .iter()
                    .any(|c| c.encoder_method_id() == M::ID_AES256_SHA256)
            })
            .collect();
        let mut entries = Vec::with_capacity(archive.files.len());
        let mut names = std::collections::HashSet::new();
        let mut total_size = 0u64;
        for (index, file) in archive.files.iter().enumerate() {
            let path = entry_path(&file.name)?;
            if !names.insert(path) {
                return Err(Error::Format("duplicate normalized 7z entry name".into()));
            }
            if file.is_anti_item
                || file.has_windows_attributes
                    && ((file.windows_attributes >> 16) & 0xf000 == 0xa000
                        || file.windows_attributes & 0x400 != 0)
            {
                return Err(Error::Unsupported(
                    "7z anti-items, links and reparse points".into(),
                ));
            }
            total_size = total_size
                .checked_add(file.size)
                .ok_or_else(|| Error::Limit("7z total size overflow".into()))?;
            let block = archive.stream_map.file_block_index[index];
            let encrypted = block.is_some_and(|b| encrypted_blocks[b]);
            if encrypted
                && file.has_stream
                && !file.has_crc
                && !archive.blocks[block.unwrap()].has_crc
            {
                return Err(Error::Unsupported(
                    "encrypted 7z streams without a CRC cannot be validated".into(),
                ));
            }
            let method = block
                .map(|b| method(&archive.blocks[b]))
                .unwrap_or(Method::Store);
            let compressed_size = block
                .filter(|&b| archive.stream_map.block_first_file_index[b] == index)
                .map(|b| {
                    archive.pack_sizes()[archive.stream_map.block_first_pack_stream_index()[b]]
                })
                .unwrap_or(0);
            entries.push(Entry {
                name: file.name.clone(),
                size: file.size,
                compressed_size,
                method,
                crc32: file.crc as u32,
                is_dir: file.is_directory,
                mtime: file
                    .has_last_modified_date
                    .then(|| unix(file.last_modified_date)),
                created: file.has_creation_date.then(|| unix(file.creation_date)),
                accessed: file.has_access_date.then(|| unix(file.access_date)),
                attributes: file.windows_attributes as u8,
                offset: index as u64,
                raw_name: file.name.as_bytes().to_vec(),
                utf8: true,
                encrypted,
                zipcrypto: false,
            });
        }
        let encrypted = archive.header_encrypted || encrypted_blocks.iter().any(|&e| e);
        Ok(Self {
            source,
            archive,
            entries,
            password,
            encrypted,
        })
    }

    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }
    pub fn len(&self) -> usize {
        self.entries.len()
    }
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
    pub fn is_solid(&self) -> bool {
        self.archive.is_solid
    }
    pub fn has_encrypted(&self) -> bool {
        self.encrypted
    }
    pub fn has_encrypted_header(&self) -> bool {
        self.archive.header_encrypted
    }

    pub fn validate_encrypted(
        &mut self,
        notify: &mut dyn FnMut(Progress<'_>) -> bool,
    ) -> Result<()> {
        if self.encrypted {
            let selected = vec![true; self.len()];
            self.run(&selected, Phase::Validate, &mut |_, _, _| Ok(()), notify)?;
        }
        Ok(())
    }

    pub fn test(&mut self, notify: &mut dyn FnMut(Progress<'_>) -> bool) -> Result<()> {
        let selected = vec![true; self.len()];
        self.run(&selected, Phase::Test, &mut |_, _, _| Ok(()), notify)
    }

    /// Calls `each` only after validating every encrypted stream and every entry path.
    /// The callback owns destination/conflict policy. It must not prepare destinations
    /// before calling this method. Unread bytes are drained with bounded memory.
    pub fn extract(
        &mut self,
        indices: &[usize],
        each: &mut dyn FnMut(usize, &Entry, &mut dyn Read) -> Result<()>,
        notify: &mut dyn FnMut(Progress<'_>) -> bool,
    ) -> Result<()> {
        let mut selected = vec![false; self.len()];
        for &index in indices {
            *selected
                .get_mut(index)
                .ok_or_else(|| Error::Format("7z entry index out of range".into()))? = true;
        }
        self.validate_encrypted(notify)?;
        self.run(&selected, Phase::Extract, each, notify)
    }

    pub fn read_entry(
        &mut self,
        index: usize,
        max_bytes: u64,
        notify: &mut dyn FnMut(Progress<'_>) -> bool,
    ) -> Result<Vec<u8>> {
        let entry = self
            .entries
            .get(index)
            .ok_or_else(|| Error::Format("7z entry index out of range".into()))?;
        if entry.size > max_bytes {
            return Err(Error::Limit("7z preview size".into()));
        }
        let mut data = Vec::new();
        self.extract(
            &[index],
            &mut |_, _, reader| {
                reader.read_to_end(&mut data)?;
                Ok(())
            },
            notify,
        )?;
        Ok(data)
    }

    fn run(
        &mut self,
        selected: &[bool],
        phase: Phase,
        each: &mut dyn FnMut(usize, &Entry, &mut dyn Read) -> Result<()>,
        notify: &mut dyn FnMut(Progress<'_>) -> bool,
    ) -> Result<()> {
        if self.encrypted && self.password.is_empty() {
            return Err(Error::PasswordRequired);
        }
        if self.encrypted
            && !self.archive.header_encrypted
            && !self.entries.iter().any(|e| e.encrypted && e.size > 0)
        {
            return Err(Error::Unsupported(
                "encrypted empty streams cannot validate a password without encrypted headers"
                    .into(),
            ));
        }
        let total = selected.iter().filter(|&&yes| yes).count();
        let mut done = 0;
        let mut bytes = 0;
        let mut blocks = vec![false; self.archive.blocks.len()];
        for (index, &yes) in selected.iter().enumerate() {
            if let Some(block) = self.archive.stream_map.file_block_index[index] {
                blocks[block] |= yes;
            }
        }
        let mut visit = |index: usize, reader: &mut dyn Read| -> Result<()> {
            let entry = &self.entries[index];
            let progress = Progress {
                phase,
                entries_done: done,
                entries_total: total,
                bytes_done: bytes,
                name: &entry.name,
            };
            if !notify(progress) {
                return Err(Error::Cancelled);
            }
            let mut stream = Stream {
                reader,
                notify,
                progress,
                consumed: 0,
                cancelled: false,
                read_error: false,
            };
            let outcome = (|| {
                if selected[index] {
                    each(index, entry, &mut stream)?;
                }
                io::copy(&mut stream, &mut io::sink())?;
                Ok(())
            })();
            if stream.cancelled {
                return Err(Error::Cancelled);
            }
            outcome.map_err(|e| match e {
                Error::Io(_) if entry.encrypted && stream.read_error => Error::PasswordOrCorrupt,
                _ => e,
            })?;
            if stream.consumed != entry.size {
                return Err(if entry.encrypted {
                    Error::PasswordOrCorrupt
                } else {
                    Error::Format("truncated 7z entry".into())
                });
            }
            bytes += stream.consumed;
            if selected[index] {
                done += 1;
            }
            Ok(())
        };
        for (block, &needed) in blocks.iter().enumerate() {
            if !needed {
                continue;
            }
            let mut index = self.archive.stream_map.block_first_file_index[block];
            let mut callback_error = None;
            let result =
                BlockDecoder::new(1, block, &self.archive, &self.password, &mut self.source)
                    .for_each_entries(&mut |_, reader| {
                        if let Err(error) = visit(index, reader) {
                            callback_error = Some(error);
                            return Ok(false);
                        }
                        index += 1;
                        Ok(true)
                    });
            if let Some(error) = callback_error {
                return Err(error);
            }
            result.map_err(|e| upstream(e, self.encrypted))?;
        }
        for (index, &yes) in selected.iter().enumerate() {
            if yes && self.archive.stream_map.file_block_index[index].is_none() {
                visit(index, &mut io::empty())?;
            }
        }
        if !notify(Progress {
            phase,
            entries_done: done,
            entries_total: total,
            bytes_done: bytes,
            name: "",
        }) {
            return Err(Error::Cancelled);
        }
        Ok(())
    }
}

struct Stream<'a, 'b> {
    reader: &'a mut dyn Read,
    notify: &'a mut dyn FnMut(Progress<'_>) -> bool,
    progress: Progress<'b>,
    consumed: u64,
    cancelled: bool,
    read_error: bool,
}

impl Read for Stream<'_, '_> {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        self.progress.bytes_done += self.consumed;
        let keep_going = (self.notify)(self.progress);
        self.progress.bytes_done -= self.consumed;
        if !keep_going {
            self.cancelled = true;
            return Err(io::Error::other("cancelled"));
        }
        let len = buffer.len().min(64 * 1024);
        let size = self
            .reader
            .read(&mut buffer[..len])
            .inspect_err(|_| self.read_error = true)?;
        self.consumed += size as u64;
        Ok(size)
    }
}

fn unix(time: sevenz_rust2::NtTime) -> i64 {
    (u64::from(time) / 10_000_000) as i64 - 11_644_473_600
}

fn method(block: &sevenz_rust2::Block) -> Method {
    block
        .coders
        .iter()
        .find_map(|coder| match coder.encoder_method_id() {
            M::ID_LZMA => Some(Method::Lzma),
            M::ID_LZMA2 => Some(Method::Lzma2),
            M::ID_DEFLATE => Some(Method::Deflate),
            M::ID_BZIP2 => Some(Method::Bzip2),
            M::ID_ZSTD => Some(Method::Zstd),
            M::ID_COPY => Some(Method::Store),
            _ => None,
        })
        .unwrap_or(Method::Other7z)
}
