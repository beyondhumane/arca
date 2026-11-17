//! Debian packages: an `ar` holding `debian-binary`, `control.tar.*` and
//! `data.tar.*`. The listing is what `dpkg-deb -R` unpacks: the files the
//! package installs, and its control files under `DEBIAN/`.

use crate::stream::{self, Layout, Spec};
use crate::{ar, check_count, clean, file_len, skipped_notice, Data, Kind, Member, Parsed, BUF};
use arca_core::{Error, Result};
use std::fs::File;
use std::io::{BufReader, Read, Seek, SeekFrom};
use std::path::Path;

pub(crate) const CONTROL: &str = "DEBIAN/";

pub(crate) fn open(path: &Path, carry_on: &dyn Fn(&str) -> bool) -> Result<Parsed> {
    let file = File::open(path)?;
    let len = file_len(&file)?;
    let mut file = BufReader::with_capacity(BUF, file);
    let members = ar::members(&mut file, len, carry_on)?;
    let not_a_package = || Error::Format("not a Debian package: debian-binary is missing".into());
    let first = members.first().ok_or_else(not_a_package)?;
    if first.name != b"debian-binary" {
        return Err(not_a_package());
    }
    let mut version = Vec::new();
    file.seek(SeekFrom::Start(first.start))?;
    (&mut file)
        .take(first.size.min(16))
        .read_to_end(&mut version)?;
    if !version.starts_with(b"2.") {
        return Err(Error::Unsupported(format!(
            "Debian package format {}",
            String::from_utf8_lossy(&version).trim()
        )));
    }
    let find = |prefix: &[u8]| members.iter().find(|m| m.name.starts_with(prefix));
    let data = find(b"data.tar")
        .ok_or_else(|| Error::Format("the Debian package has no data.tar member".into()))?;
    let mut parsed = Parsed::default();
    let mut special = 0;
    for (part, prefix) in [(Some(data), ""), (find(b"control.tar"), CONTROL)] {
        let Some(part) = part else {
            continue;
        };
        let magic = stream::magic(&mut file, part.start, part.size)?;
        let spec = Spec {
            start: part.start,
            len: part.size,
            codec: stream::sniff(&magic)?,
            layout: Layout::Tar,
        };
        let source = stream::open(path, &spec)?;
        special += list(
            source,
            parsed.streams.len(),
            prefix,
            &mut parsed.members,
            carry_on,
        )?;
        parsed.streams.push(spec);
    }
    if special > 0 {
        parsed.notices.push(skipped_notice(special));
    }
    Ok(parsed)
}

fn list(
    source: Box<dyn Read + Send>,
    stream: usize,
    prefix: &str,
    members: &mut Vec<Member>,
    carry_on: &dyn Fn(&str) -> bool,
) -> Result<usize> {
    let mut reader = arca_tar::TarReader::new(source);
    let mut ordinal = 0u64;
    let mut special = 0;
    while let Some(e) = reader.next_entry()? {
        let this = ordinal;
        ordinal += 1;
        let bare = clean(&e.entry.name);
        reader.skip_data(&e)?;
        if bare.is_empty() {
            continue;
        }
        let name = format!("{prefix}{bare}");
        if !carry_on(&name) {
            return Err(Error::Cancelled);
        }
        let kind = if e.entry.is_dir {
            Kind::Dir
        } else if let Some(target) = e.symlink() {
            Kind::Symlink(target.to_string())
        } else if let Some(target) = e.hard_link() {
            Kind::Hard(format!("{prefix}{}", clean(target)))
        } else if e.is_plain() {
            Kind::File
        } else {
            special += 1;
            continue;
        };
        check_count(members)?;
        let mut entry = e.entry.clone();
        entry.raw_name = name.as_bytes().to_vec();
        entry.name = name;
        if kind != Kind::File {
            entry.size = 0;
            entry.compressed_size = 0;
        }
        members.push(Member {
            entry,
            data: if kind == Kind::File {
                Data::Stream {
                    stream,
                    ordinal: this,
                }
            } else {
                Data::None
            },
            kind,
            mode: e.mode,
            sum: None,
        });
    }
    Ok(special)
}
