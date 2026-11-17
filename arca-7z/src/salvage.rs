//! Getting back the entries of a damaged 7z that still decode.

use std::{
    fs,
    io::{self, BufReader, Read},
    path::Path,
};

use arca_core::{Error, Level, Result};
use sevenz_rust2::{BlockDecoder, EncoderMethod as M, Password};

use crate::{rewrite_7z, upstream, Change, Phase, Progress, SevenZArchive};

/// What [`salvage_7z`] wrote and what it had to leave out.
#[derive(Default)]
pub struct Salvaged {
    pub kept: usize,
    /// The entries left out, each with the reason.
    pub lost: Vec<(String, String)>,
}

/// Writes at `output` a new 7z holding every entry of `input` that decodes
/// and checks out. Each block is decoded on its own from the start, so damage
/// in one costs nothing in the others; within a block, the first entry that
/// fails and everything after it in the block are lost, since a solid stream
/// cannot be picked up again past a fault. An entry with no checksum of its
/// own is kept only when its whole block decodes and the block's checksum,
/// if it has one, holds.
///
/// The header at the end of the archive is what says where every block and
/// entry is: when it is damaged nothing can be found, and this fails.
/// Survivors are encoded again by [`rewrite_7z`], which reads the new
/// archive back in full. `input` is only read.
pub fn salvage_7z(
    input: &Path,
    output: &Path,
    password: Option<&str>,
    level: Level,
    notify: &mut dyn FnMut(Progress<'_>) -> bool,
) -> Result<Salvaged> {
    if output.exists() && fs::canonicalize(input)? == fs::canonicalize(output)? {
        return Err(Error::Format(
            "the salvaged archive cannot replace the damaged one".into(),
        ));
    }
    let mut source = SevenZArchive::open(BufReader::new(fs::File::open(input)?), password)?;
    if source.encrypted && source.password.is_empty() {
        return Err(Error::PasswordRequired);
    }
    let files = source.archive.files.len();
    let mut keep = vec![true; files];
    let mut report = Salvaged::default();
    for block in 0..source.archive.blocks.len() {
        let first = source.archive.stream_map.block_first_file_index[block];
        let members: Vec<usize> = (first..files)
            .take_while(|&i| source.archive.stream_map.file_block_index[i] == Some(block))
            .filter(|&i| source.archive.files[i].has_stream && source.archive.files[i].size > 0)
            .collect();
        let name = members
            .first()
            .map_or("", |&i| source.archive.files[i].name.as_str());
        if !notify(Progress {
            phase: Phase::Test,
            entries_done: first,
            entries_total: files,
            bytes_done: 0,
            name,
        }) {
            return Err(Error::Cancelled);
        }
        let (lost, why) = check_block(&mut source, block, &members)?;
        for i in lost {
            keep[i] = false;
            report
                .lost
                .push((source.archive.files[i].name.clone(), why.clone()));
        }
    }
    report.kept = keep.iter().filter(|&&k| k).count();
    if report.kept == 0 {
        return Err(Error::Format("no entry of the 7z decodes".into()));
    }
    drop(source);
    let name = |e: &arca_core::Entry| {
        keep.get(e.offset as usize)
            .copied()
            .unwrap_or(false)
            .then(|| e.name.clone())
    };
    let change = Change {
        name: &name,
        add: &[],
        level,
    };
    rewrite_7z(input, output, password, &change, notify)?;
    Ok(report)
}

/// Decodes `block` and checks each of `members` in turn. Gives back the
/// ones lost and why: from the first that fails to the end of the block, and
/// those without a checksum of their own whenever the block does not decode
/// to its end, which is the only thing that vouches for them.
fn check_block<R: Read + io::Seek>(
    source: &mut SevenZArchive<R>,
    block: usize,
    members: &[usize],
) -> Result<(Vec<usize>, String)> {
    let archive = &source.archive;
    let encrypted = archive.blocks[block]
        .coders
        .iter()
        .any(|c| c.encoder_method_id() == M::ID_AES256_SHA256);
    let no_password = Password::empty();
    let password = if encrypted {
        &source.password
    } else {
        &no_password
    };
    let mut reader =
        match BlockDecoder::new(1, block, archive, password, &mut source.source).into_reader() {
            Ok(reader) => reader,
            Err(e) => return Ok((members.to_vec(), upstream(e, encrypted).to_string())),
        };
    let mut unchecked = Vec::new();
    let lost = |unchecked: &[usize], from: usize| {
        let mut lost = unchecked.to_vec();
        lost.extend_from_slice(&members[from..]);
        lost
    };
    let mut buf = vec![0u8; 64 * 1024];
    for (at, &i) in members.iter().enumerate() {
        let file = &archive.files[i];
        let mut hasher = crc32fast::Hasher::new();
        let mut left = file.size;
        while left > 0 {
            let want = buf.len().min(usize::try_from(left).unwrap_or(usize::MAX));
            match reader.read(&mut buf[..want]) {
                Ok(0) => {
                    return Ok((
                        lost(&unchecked, at),
                        "the block ends inside this entry".into(),
                    ))
                }
                Ok(n) => {
                    hasher.update(&buf[..n]);
                    left -= n as u64;
                }
                Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
                Err(e) => return Ok((lost(&unchecked, at), damaged(e, encrypted))),
            }
        }
        if file.has_crc {
            if hasher.finalize() != file.crc as u32 {
                return Ok((lost(&unchecked, at), "the checksum does not match".into()));
            }
        } else {
            unchecked.push(i);
        }
    }
    if !unchecked.is_empty() {
        if let Err(e) = io::copy(&mut reader, &mut io::sink()) {
            return Ok((unchecked, damaged(e, encrypted)));
        }
    }
    Ok((Vec::new(), String::new()))
}

fn damaged(error: io::Error, encrypted: bool) -> String {
    if encrypted {
        Error::PasswordOrCorrupt.to_string()
    } else {
        format!("the block does not decode: {error}")
    }
}
