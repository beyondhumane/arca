use super::{reader, Progress};
use arca_core::{Error, Result};
use rars::{Archive, ArchiveMember, ArchiveMemberDetail, ArchiveReader, ReadCancellation};
use std::collections::BTreeMap;
use std::fs::{self, OpenOptions};
use std::path::{Path, PathBuf};

const MAX_VOLUMES: usize = 256;
const MAX_DIRECTORY_ENTRIES: usize = 100_000;
const MAX_HEADERS: u64 = 100_000;
const MAX_HEADER_BYTES: u64 = 64 * 1024 * 1024;

pub(super) struct Volumes {
    pub archives: Vec<Archive>,
    paths: Vec<PathBuf>,
}

enum Naming {
    Numbered { stem: String, width: usize },
    Legacy { stem: String },
}

impl Naming {
    fn from_name(name: &str) -> Option<Self> {
        let lower = name.to_ascii_lowercase();
        if let Some(stem) = lower.strip_suffix(".rar") {
            if let Some((prefix, digits)) = stem.rsplit_once(".part") {
                if !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()) {
                    return Some(Self::Numbered {
                        stem: name[..prefix.len()].into(),
                        width: digits.len(),
                    });
                }
            }
            return Some(Self::Legacy {
                stem: name[..stem.len()].into(),
            });
        }
        let (stem, ext) = lower.rsplit_once('.')?;
        legacy_index(ext)?;
        Some(Self::Legacy {
            stem: name[..stem.len()].into(),
        })
    }

    fn index(&self, name: &str) -> Option<usize> {
        let lower = name.to_ascii_lowercase();
        match self {
            Self::Numbered { stem, .. } => {
                let (prefix, digits) = lower.strip_suffix(".rar")?.rsplit_once(".part")?;
                if !prefix.eq_ignore_ascii_case(stem)
                    || digits.is_empty()
                    || !digits.bytes().all(|b| b.is_ascii_digit())
                {
                    return None;
                }
                Some(digits.parse().unwrap_or(usize::MAX))
            }
            Self::Legacy { stem } => {
                let (prefix, ext) = lower.rsplit_once('.')?;
                if !prefix.eq_ignore_ascii_case(stem) {
                    return None;
                }
                if ext == "rar" {
                    Some(1)
                } else {
                    legacy_index(ext)
                }
            }
        }
    }

    fn name(&self, index: usize) -> String {
        match self {
            Self::Numbered { stem, width } => format!("{stem}.part{index:0width$}.rar"),
            Self::Legacy { stem } if index == 1 => format!("{stem}.rar"),
            Self::Legacy { stem } => {
                let n = index - 2;
                format!("{stem}.{}{:02}", (b'r' + (n / 100) as u8) as char, n % 100)
            }
        }
    }
}

fn legacy_index(ext: &str) -> Option<usize> {
    let bytes = ext.as_bytes();
    if bytes.len() == 3
        && (b'r'..=b'z').contains(&bytes[0])
        && bytes[1..].iter().all(u8::is_ascii_digit)
    {
        Some(2 + usize::from(bytes[0] - b'r') * 100 + ext[1..].parse::<usize>().ok()?)
    } else {
        None
    }
}

fn check(token: &ReadCancellation) -> Result<()> {
    if token.is_cancelled() {
        Err(Error::Cancelled)
    } else {
        Ok(())
    }
}

fn invalid(path: &Path, message: &str) -> Error {
    Error::Format(format!("RAR volume '{}': {message}", path.display()))
}

fn discover(
    path: &Path,
    progress: &Progress<'_>,
    token: &ReadCancellation,
) -> Result<(Vec<PathBuf>, Option<PathBuf>)> {
    check(token)?;
    if !progress(0, 0, "RAR volume discovery") {
        return Err(Error::Cancelled);
    }
    let Some(naming) = path
        .file_name()
        .and_then(|n| n.to_str())
        .and_then(Naming::from_name)
    else {
        return Ok((vec![path.to_owned()], None));
    };
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let mut candidates = BTreeMap::new();
    for (count, entry) in fs::read_dir(parent)?.enumerate() {
        check(token)?;
        if count >= MAX_DIRECTORY_ENTRIES {
            return Err(Error::Limit(format!(
                "RAR discovery in '{}' exceeds {MAX_DIRECTORY_ENTRIES} directory entries",
                parent.display()
            )));
        }
        let entry = entry?;
        let Some(index) = entry.file_name().to_str().and_then(|n| naming.index(n)) else {
            continue;
        };
        if index == 0 || index > MAX_VOLUMES {
            return Err(Error::Limit(format!(
                "RAR volume '{}' exceeds the {MAX_VOLUMES}-volume limit or has index zero",
                entry.path().display()
            )));
        }
        if let Some(previous) = candidates.insert(index, entry.path()) {
            return Err(invalid(
                &entry.path(),
                &format!(
                    "duplicate volume {index}, also named '{}'",
                    previous.display()
                ),
            ));
        }
    }
    // The selected path must exist, even if a differently cased alias exists.
    fs::symlink_metadata(path).map_err(|e| reader::at_path(e.into(), path))?;
    let mut paths = Vec::new();
    let selected_index = path
        .file_name()
        .and_then(|n| n.to_str())
        .and_then(|n| naming.index(n));
    for index in 1..=candidates.keys().next_back().copied().unwrap_or(1) {
        let expected = parent.join(naming.name(index));
        let candidate = candidates
            .remove(&index)
            .ok_or_else(|| invalid(&expected, "missing volume"))?;
        paths.push(if Some(index) == selected_index {
            path.to_owned()
        } else {
            candidate
        });
    }
    let next = parent.join(naming.name(paths.len() + 1));
    Ok((paths, Some(next)))
}

#[derive(PartialEq, Eq)]
struct Properties {
    volume: bool,
    solid: bool,
    encrypted_headers: bool,
    new_numbering: Option<bool>,
}

fn properties(archive: &Archive) -> Result<Properties> {
    let (volume, solid, encrypted_headers) = match archive {
        Archive::Rar13(a) => (a.main.is_volume(), a.main.is_solid(), false),
        Archive::Rar15To40(a) => (
            a.main.is_volume(),
            a.main.is_solid(),
            a.main.has_encrypted_headers(),
        ),
        Archive::Rar50Plus(a) => (
            a.main.is_volume(),
            a.main.is_solid(),
            a.main.encrypted_headers,
        ),
        _ => return Err(Error::Unsupported("unknown RAR family".into())),
    };
    Ok(Properties {
        volume,
        solid,
        encrypted_headers,
        new_numbering: match archive {
            Archive::Rar15To40(a) => Some(a.main.uses_new_numbering()),
            _ => None,
        },
    })
}

fn next_volume(archive: &Archive) -> Result<bool> {
    let split = archive
        .members()
        .last()
        .is_some_and(|m| m.meta.is_split_after);
    match archive {
        Archive::Rar50Plus(a) => match a.blocks.last() {
            Some(rars::rar50::Block::End(end)) => {
                if split && !end.has_next_volume() {
                    return Err(Error::Format(
                        "split member has no next-volume end flag".into(),
                    ));
                }
                Ok(end.has_next_volume())
            }
            _ => Err(Error::Format(
                "RAR5 end header is missing (truncated archive)".into(),
            )),
        },
        Archive::Rar15To40(a) => match a.blocks.last() {
            Some(rars::rar15_40::Block::End(end)) => {
                if split && end.flags & 1 == 0 {
                    return Err(Error::Format(
                        "split member has no next-volume end flag".into(),
                    ));
                }
                Ok(end.flags & 1 != 0)
            }
            // Old RAR generations have no end header; only split flags signal continuation.
            _ => Ok(split),
        },
        Archive::Rar13(_) => Ok(split),
        _ => Err(Error::Unsupported("unknown RAR family".into())),
    }
}

fn header_usage(archive: &Archive, length: u64) -> Result<(u64, u64)> {
    let count = match archive {
        Archive::Rar13(a) => a.entries.len() as u64 + 1,
        Archive::Rar15To40(a) => a.blocks.len() as u64 + 1,
        Archive::Rar50Plus(a) => a.blocks.len() as u64 + 1 + u64::from(a.main.encrypted_headers),
        _ => return Err(Error::Unsupported("unknown RAR family".into())),
    };
    // Conservatively charge all non-file bytes, including encrypted-header
    // padding, SFX prefixes, service data and trailing data, across the set.
    let bytes = archive.members().try_fold(length, |remaining, m| {
        remaining
            .checked_sub(m.meta.packed_size)
            .ok_or_else(|| Error::Format("RAR packed ranges exceed volume size".into()))
    })?;
    Ok((count, bytes))
}

#[cfg(any(windows, test))]
fn is_reparse_point(attributes: u32) -> bool {
    attributes & 0x400 != 0
}

fn regular_input(path: &Path, meta: &fs::Metadata) -> Result<()> {
    let regular = meta.file_type().is_file();
    #[cfg(windows)]
    let regular = {
        use std::os::windows::fs::MetadataExt;
        regular && !is_reparse_point(meta.file_attributes())
    };
    if !regular {
        return Err(invalid(
            path,
            "expected a regular file in the selected directory, not a link or reparse point",
        ));
    }
    Ok(())
}

fn read_archive(
    path: &Path,
    password: Option<&str>,
    token: &ReadCancellation,
    headers: &mut u64,
    bytes: &mut u64,
) -> Result<Archive> {
    check(token)?;
    let meta = fs::symlink_metadata(path).map_err(|e| reader::at_path(e.into(), path))?;
    regular_input(path, &meta)?;
    let mut open = OpenOptions::new();
    open.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
        open.custom_flags(FILE_FLAG_OPEN_REPARSE_POINT);
    }
    let file = open
        .open(path)
        .map_err(|e| reader::at_path(e.into(), path))?;
    let meta = file
        .metadata()
        .map_err(|e| reader::at_path(e.into(), path))?;
    regular_input(path, &meta)?;
    let archive = ArchiveReader::read_reader_with_options(
        file,
        reader::options(password, token)
            .with_max_header_count(*headers)
            .with_max_header_bytes(*bytes),
    )
    .map_err(|e| reader::at_path(reader::error(e), path))?;
    let (count, size) = header_usage(&archive, meta.len()).map_err(|e| reader::at_path(e, path))?;
    *headers = headers.checked_sub(count).ok_or_else(|| {
        Error::Limit(format!(
            "RAR headers exceed {MAX_HEADERS} across the set at '{}'",
            path.display()
        ))
    })?;
    *bytes = bytes.checked_sub(size).ok_or_else(|| {
        Error::Limit(format!(
            "RAR non-file bytes exceed 64 MiB across the set at '{}'",
            path.display()
        ))
    })?;
    Ok(archive)
}

impl Volumes {
    pub fn open(
        path: &Path,
        password: Option<&str>,
        progress: &Progress<'_>,
        token: &ReadCancellation,
    ) -> Result<Self> {
        let mut headers = MAX_HEADERS;
        let mut bytes = MAX_HEADER_BYTES;
        if !progress(0, 0, &path.to_string_lossy()) {
            return Err(Error::Cancelled);
        }
        let selected = read_archive(path, password, token, &mut headers, &mut bytes)?;
        let needs_volumes = properties(&selected)?.volume
            || selected
                .members()
                .any(|m| m.meta.is_split_before || m.meta.is_split_after)
            || next_volume(&selected).map_err(|e| reader::at_path(e, path))?;
        let (paths, next) = if needs_volumes {
            discover(path, progress, token)?
        } else {
            (vec![path.to_owned()], None)
        };
        let selected_path = path;
        let mut selected = Some(selected);
        let mut archives: Vec<Archive> = Vec::new();
        for (index, path) in paths.iter().enumerate() {
            check(token)?;
            if !progress(index, paths.len(), &path.to_string_lossy()) {
                return Err(Error::Cancelled);
            }
            let archive = if path == selected_path {
                selected
                    .take()
                    .ok_or_else(|| invalid(path, "duplicate selected volume"))?
            } else {
                read_archive(path, password, token, &mut headers, &mut bytes)?
            };
            let props = properties(&archive)?;
            if let Some(first) = archives.first() {
                if archive.family() != first.family() || props != properties(first)? {
                    return Err(invalid(path, "archive family or volume/solid/encryption flags do not match the first volume"));
                }
            }
            if paths.len() > 1 && !props.volume {
                return Err(invalid(path, "not marked as part of a volume set"));
            }
            match &archive {
                Archive::Rar50Plus(a)
                    if props.volume && a.main.volume_number.unwrap_or(0) != index as u64 =>
                {
                    return Err(invalid(path, "volume number is out of order"));
                }
                Archive::Rar15To40(a)
                    if props.volume
                        && ((index > 0 && a.main.is_first_volume())
                            || (index == 0
                                && a.main.uses_new_numbering()
                                && !a.main.is_first_volume())) =>
                {
                    return Err(invalid(path, "first-volume flag is out of order"));
                }
                _ => {}
            }
            let has_next = next_volume(&archive).map_err(|e| reader::at_path(e, path))?;
            if has_next && index + 1 == paths.len() {
                return Err(invalid(
                    next.as_deref().unwrap_or(path),
                    &format!("missing volume required after '{}'", path.display()),
                ));
            }
            if !has_next && index + 1 < paths.len() {
                return Err(invalid(
                    &paths[index + 1],
                    &format!("unexpected volume after final volume '{}'", path.display()),
                ));
            }
            archives.push(archive);
        }
        Ok(Self { archives, paths })
    }

    pub fn members(&self, token: &ReadCancellation) -> Result<Vec<ArchiveMember>> {
        let mut result: Vec<ArchiveMember> = Vec::new();
        let mut pending = false;
        let mut previous_volume = 0;
        let mut encryption = None;
        let mut dictionary = None;
        for (volume, archive) in self.archives.iter().enumerate() {
            let mut legacy_files = match archive {
                Archive::Rar15To40(a) => Some(a.files()),
                _ => None,
            };
            let mut rar5_files = match archive {
                Archive::Rar50Plus(a) => Some(a.files()),
                _ => None,
            };
            for (index, member) in archive.members().enumerate() {
                check(token)?;
                let fail = |message| invalid(&self.paths[volume], message);
                let window = legacy_files
                    .as_mut()
                    .and_then(|files| files.next())
                    .map(|f| f.block.flags & 0x00e0);
                if member.meta.is_split_before != pending {
                    return Err(fail(
                        "split chain is missing its first part or is interrupted",
                    ));
                }
                let mut crypt = rar5_files
                    .as_mut()
                    .and_then(|files| files.next())
                    .and_then(|f| f.encryption.clone());
                if let Some(crypt) = &mut crypt {
                    // The terminal checksum may be keyed while intermediate
                    // packed-fragment checksums are not. Keys and IV must match.
                    crypt.flags &= !2;
                }
                if pending {
                    if index != 0 || volume != previous_volume + 1 {
                        return Err(fail("split continuation must start the next volume"));
                    }
                    let previous = result
                        .last_mut()
                        .ok_or_else(|| fail("missing first split member"))?;
                    validate_fragment(previous, &member)
                        .map_err(|e| reader::at_path(e, &self.paths[volume]))?;
                    if crypt != encryption {
                        return Err(fail("split encryption parameters changed"));
                    }
                    if window != dictionary {
                        return Err(fail("split dictionary flags changed"));
                    }
                    let packed = previous
                        .meta
                        .packed_size
                        .checked_add(member.meta.packed_size)
                        .ok_or_else(|| fail("split packed size overflow"))?;
                    // Terminal headers carry the file checksum; earlier fragments
                    // can carry packed-fragment checksums instead.
                    *previous = member.clone();
                    previous.meta.packed_size = packed;
                    previous.meta.is_split_before = false;
                } else {
                    encryption = crypt;
                    dictionary = window;
                    result.push(member.clone());
                }
                pending = member.meta.is_split_after;
                previous_volume = volume;
                if (member.meta.is_split_before || pending) && member.meta.is_directory {
                    return Err(fail("directory cannot be split"));
                }
            }
        }
        if pending {
            return Err(invalid(
                self.paths.last().unwrap(),
                "incomplete split member",
            ));
        }
        Ok(result)
    }

    pub fn error(&self, error: rars::Error) -> Error {
        let mut source = &error;
        while let rars::Error::AtEntry { source: inner, .. }
        | rars::Error::AtArchiveOffset { source: inner, .. } = source
        {
            source = inner;
        }
        if let rars::Error::InVolume { number, .. } = source {
            if let Some(path) = number.checked_sub(1).and_then(|n| self.paths.get(n)) {
                return reader::at_path(reader::error(error), path);
            }
        }
        let name = error.entry_context().map(|(name, _)| name);
        let paths = self
            .archives
            .iter()
            .zip(&self.paths)
            .filter(|(a, _)| name.is_none_or(|name| a.members().any(|m| m.meta.name == name)))
            .map(|(_, p)| p.display().to_string())
            .collect::<Vec<_>>()
            .join("', '");
        reader::at_path(reader::error(error), Path::new(&paths))
    }
}

fn validate_fragment(first: &ArchiveMember, next: &ArchiveMember) -> Result<()> {
    let a = &first.meta;
    let b = &next.meta;
    if a.name != b.name
        || a.unpacked_size != b.unpacked_size
        || a.file_attr != b.file_attr
        || a.host_os != b.host_os
        || a.is_encrypted != b.is_encrypted
        || a.is_redirection != b.is_redirection
    {
        return Err(Error::Format(
            "split member name, size, attributes or encryption changed".into(),
        ));
    }
    let same = match (&first.detail, &next.detail) {
        (
            ArchiveMemberDetail::Rar13 {
                method: am,
                unpack_version: av,
                ..
            },
            ArchiveMemberDetail::Rar13 {
                method: bm,
                unpack_version: bv,
                ..
            },
        ) => am == bm && av == bv,
        (
            ArchiveMemberDetail::Rar15To40 {
                method: am,
                unpack_version: av,
                salt: asalt,
                solid: asolid,
                unicode_name: an,
                ..
            },
            ArchiveMemberDetail::Rar15To40 {
                method: bm,
                unpack_version: bv,
                salt: bsalt,
                solid: bsolid,
                unicode_name: bn,
                ..
            },
        ) => am == bm && av == bv && asalt == bsalt && an == bn && asolid == bsolid,
        (
            ArchiveMemberDetail::Rar50Plus {
                compression_info: ac,
                redirection: ar,
                ..
            },
            ArchiveMemberDetail::Rar50Plus {
                compression_info: bc,
                redirection: br,
                ..
            },
        ) => ac == bc && ar == br,
        _ => false,
    };
    if !same {
        return Err(Error::Format(
            "split member codec or encryption metadata changed".into(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn rejects_all_windows_reparse_tags_not_only_symlinks() {
        assert!(!super::is_reparse_point(0x20));
        assert!(super::is_reparse_point(0x420));
        assert!(super::is_reparse_point(0x410));
    }
}
