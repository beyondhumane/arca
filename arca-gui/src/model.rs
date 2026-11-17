//! Pure application model, settings, and projections shared by the controller and the egui view.

use crate::i18n::Strings;
use crate::tree::Row;
use std::path::Path;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ThemePreference {
    System,
    Light,
    Dark,
}

pub(crate) use arca_core::{Container, Format};

pub(crate) fn detect(p: &Path) -> Option<Format> {
    arca_stream::identify(p)
}

pub(crate) fn container_of(p: &Path) -> Option<Container> {
    detect(p).map(Format::container)
}

// GTK matches filter patterns case-sensitively, so APP.APK needs its own entry.
pub(crate) fn open_filter() -> Vec<String> {
    let mut extensions = Vec::new();
    for (suffix, format) in Format::SUFFIXES {
        if format.container() == Container::Rar && !cfg!(feature = "rar") {
            continue;
        }
        let extension = suffix.rsplit('.').next().unwrap_or(suffix);
        for variant in [extension.to_string(), extension.to_ascii_uppercase()] {
            if !extensions.contains(&variant) {
                extensions.push(variant);
            }
        }
    }
    // The first part of a split set, which opens the whole set.
    extensions.push("001".into());
    extensions
}

pub(crate) fn read_only_suffix(format: Option<Format>) -> String {
    match format {
        Some(f) if !f.can_write() => format!(" ({}: read-only)", f.label()),
        _ => String::new(),
    }
}

/// Whether a format compresses the archive as one stream rather than entry by
/// entry, so that no entry has a compressed size of its own and only the
/// file's size says how much was saved.
pub(crate) fn whole_stream(format: Format) -> bool {
    matches!(
        format.container(),
        arca_core::Container::Tar
            | arca_core::Container::TarGz
            | arca_core::Container::TarXz
            | arca_core::Container::TarBz2
            | arca_core::Container::TarZst
    )
}

pub(crate) fn saved_of(r: &Row) -> f64 {
    if r.size == 0 {
        0.0
    } else {
        1.0 - r.packed as f64 / r.size as f64
    }
}

/// A Unix timestamp as the date and time the clock here showed then, the way
/// the Explorer and every file manager shows them.
pub(crate) fn when(mtime: Option<i64>) -> String {
    match mtime.filter(|t| *t > 0) {
        Some(t) => civil(arca_core::to_local_wall(t)),
        None => String::new(),
    }
}

/// Seconds counted as if the wall clock were UTC, as a date and a time.
pub(crate) fn civil(t: i64) -> String {
    let days = t.div_euclid(86_400);
    let secs = t.rem_euclid(86_400);
    // Days since 1970 to a civil date, by Howard Hinnant's method: shift the
    // epoch to March so the leap day lands at the end of the year and the
    // month lengths follow one formula.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = era * 400 + yoe + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02} {:02}:{:02}",
        secs / 3600,
        (secs % 3600) / 60
    )
}

pub(crate) fn human(n: u64) -> String {
    const U: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut v = n as f64;
    let mut i = 0;
    while v >= 1024.0 && i < U.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    if i == 0 {
        format!("{n} B")
    } else {
        format!("{v:.1} {}", U[i])
    }
}

pub(crate) fn archive_stem(p: &Path) -> String {
    let p = arca_core::volumes::whole_name(p);
    let name = p
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    Format::split_name(&name).map_or_else(|| name.clone(), |(stem, _)| stem.to_string())
}

#[derive(PartialEq, Eq, Clone, Copy)]
pub(crate) enum SortColumn {
    Name,
    Size,
    Packed,
    Method,
    Saved,
    Modified,
    Crc,
    Type,
    Path,
    Created,
    Accessed,
    Attributes,
}

// Which columns the list shows. Name is not here: a list of nothing but sizes
// would be a strange thing to allow.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct Columns {
    size: bool,
    packed: bool,
    method: bool,
    saved: bool,
    modified: bool,
    crc: bool,
    type_: bool,
    path: bool,
    created: bool,
    accessed: bool,
    attributes: bool,
}

impl Default for Columns {
    fn default() -> Self {
        // What was on screen before any of this was a choice, plus the date,
        // which both WinRAR and NanaZip show and which people look for.
        Columns {
            size: true,
            packed: true,
            method: true,
            saved: true,
            modified: true,
            crc: false,
            type_: false,
            path: false,
            created: false,
            accessed: false,
            attributes: false,
        }
    }
}

impl Columns {
    pub(crate) const ALL: [(SortColumn, &'static str); 11] = [
        (SortColumn::Size, "size"),
        (SortColumn::Packed, "packed"),
        (SortColumn::Method, "method"),
        (SortColumn::Saved, "saved"),
        (SortColumn::Modified, "modified"),
        (SortColumn::Crc, "crc"),
        (SortColumn::Type, "type"),
        (SortColumn::Path, "path"),
        (SortColumn::Created, "created"),
        (SortColumn::Accessed, "accessed"),
        (SortColumn::Attributes, "attributes"),
    ];

    pub(crate) fn on(&self, which: SortColumn) -> bool {
        match which {
            SortColumn::Size => self.size,
            SortColumn::Packed => self.packed,
            SortColumn::Method => self.method,
            SortColumn::Saved => self.saved,
            SortColumn::Modified => self.modified,
            SortColumn::Crc => self.crc,
            SortColumn::Type => self.type_,
            SortColumn::Path => self.path,
            SortColumn::Created => self.created,
            SortColumn::Accessed => self.accessed,
            SortColumn::Attributes => self.attributes,
            SortColumn::Name => true,
        }
    }

    pub(crate) fn set(&mut self, which: SortColumn, value: bool) {
        match which {
            SortColumn::Size => self.size = value,
            SortColumn::Packed => self.packed = value,
            SortColumn::Method => self.method = value,
            SortColumn::Saved => self.saved = value,
            SortColumn::Modified => self.modified = value,
            SortColumn::Crc => self.crc = value,
            SortColumn::Type => self.type_ = value,
            SortColumn::Path => self.path = value,
            SortColumn::Created => self.created = value,
            SortColumn::Accessed => self.accessed = value,
            SortColumn::Attributes => self.attributes = value,
            SortColumn::Name => {}
        }
    }

    pub(crate) fn label(which: SortColumn, s: &Strings) -> &'static str {
        match which {
            SortColumn::Size => s.col_size,
            SortColumn::Packed => s.col_packed,
            SortColumn::Method => s.col_method,
            SortColumn::Saved => s.col_saved,
            SortColumn::Modified => s.col_modified,
            SortColumn::Crc => s.col_crc,
            SortColumn::Type => s.col_type,
            SortColumn::Path => s.col_path,
            SortColumn::Created => s.col_created,
            SortColumn::Accessed => s.col_accessed,
            SortColumn::Attributes => s.col_attributes,
            SortColumn::Name => s.col_name,
        }
    }
}

/// The sizes the compress box offers to split a new archive into, the way
/// 7-Zip's list has them: in MiB, with FAT32's largest file rounded down to
/// a whole number of them.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum VolumeChoice {
    None,
    Mb100,
    Mb700,
    Fat32,
    Custom,
}

impl VolumeChoice {
    pub(crate) const ALL: [Self; 5] = [
        Self::None,
        Self::Mb100,
        Self::Mb700,
        Self::Fat32,
        Self::Custom,
    ];

    pub(crate) fn bytes(self) -> Option<u64> {
        match self {
            Self::None | Self::Custom => None,
            Self::Mb100 => Some(100 << 20),
            Self::Mb700 => Some(700 << 20),
            Self::Fat32 => Some(4092 << 20),
        }
    }

    pub(crate) fn label(self, s: &Strings) -> &'static str {
        match self {
            Self::None => s.volume_none,
            Self::Mb100 => s.volume_100m,
            Self::Mb700 => s.volume_700m,
            Self::Fat32 => s.volume_fat32,
            Self::Custom => s.volume_custom,
        }
    }
}
