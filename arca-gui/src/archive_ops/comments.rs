//! The comments an archive carries, read once when it is listed.

use crate::{container_of, Container};
use arca_zip::ZipArchive;
use std::collections::HashMap;
use std::path::Path;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Comments {
    pub(crate) archive: String,
    /// By the entry's `offset`, which tells ZIP entries apart.
    pub(crate) entries: HashMap<u64, String>,
}

/// The comments of a ZIP, or the archive comment of a RAR. Nothing for the
/// formats that keep none, or when they cannot be read: a comment is never a
/// reason for an archive not to open.
pub(crate) fn read_comments(archive: &Path, password: Option<&str>) -> Option<Comments> {
    match container_of(archive)? {
        Container::Zip => {
            let a = ZipArchive::open(arca_core::volumes::open(archive).ok()?).ok()?;
            let entries = a
                .entries()
                .iter()
                .enumerate()
                .filter_map(|(i, e)| Some((e.offset, a.entry_comment(i)?)))
                .collect();
            Some(Comments {
                archive: a.comment(),
                entries,
            })
        }
        Container::Rar => {
            let a = arca_rar::RarArchive::open(archive, password).ok()?;
            Some(Comments {
                archive: a.comment(password).ok()??,
                entries: HashMap::new(),
            })
        }
        _ => None,
    }
}

/// A comment as one line for the status bar: control characters and runs of
/// white space become one space, and a long one is cut.
pub(crate) fn notice_line(comment: &str, label: &str) -> Option<String> {
    const SHOWN: usize = 160;
    let line = comment
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    if line.is_empty() {
        return None;
    }
    let mut shown: String = line.chars().take(SHOWN).collect();
    if line.chars().count() > SHOWN {
        shown.push('…');
    }
    Some(format!("{label}: {shown}"))
}
