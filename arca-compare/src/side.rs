use crate::Notify;
use arca_core::{Container, Entry, Error, Format, Result};
use std::collections::BTreeMap;
use std::fs::{self, File};
use std::path::{Path, PathBuf};

// A link is only told apart where the format says so, which is a TAR and the
// disk; anywhere else it reads as a file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Kind {
    File,
    Folder,
    Link(String),
}

impl Kind {
    pub(crate) fn same_type(&self, other: &Kind) -> bool {
        std::mem::discriminant(self) == std::mem::discriminant(other)
    }
}

#[derive(Clone, Debug)]
pub(crate) enum Origin {
    Entry(usize),
    Disk(PathBuf),
    Implied,
}

#[derive(Clone, Debug)]
pub(crate) struct Item {
    pub(crate) kind: Kind,
    pub(crate) size: Option<u64>,
    pub(crate) crc32: Option<u32>,
    pub(crate) mtime: Option<i64>,
    pub(crate) origin: Origin,
}

// A hard link in a TAR is the file it names, read from that entry; devices
// and pipes have nothing to compare and are left out, as on the disk.
#[derive(Clone)]
enum TarKind {
    Plain,
    Symlink(String),
    Hard(String),
    Special,
}

pub(crate) enum Source {
    Archive {
        path: PathBuf,
        format: Format,
        entries: Vec<Entry>,
        password: Option<String>,
    },
    Folder,
}

pub struct Side {
    pub(crate) items: BTreeMap<String, Item>,
    pub(crate) source: Source,
}

// Names are keyed without the slash a folder ends in, so a folder `x/` and a
// file `x` meet under one name and read as a change of type.
pub(crate) fn key(name: &str) -> String {
    name.replace('\\', "/")
        .split('/')
        .filter(|part| !part.is_empty() && *part != ".")
        .collect::<Vec<_>>()
        .join("/")
}

fn mtime_of(meta: &fs::Metadata) -> Option<i64> {
    let at = meta.modified().ok()?;
    Some(match at.duration_since(std::time::UNIX_EPOCH) {
        Ok(after) => after.as_secs() as i64,
        Err(before) => -(before.duration().as_secs() as i64),
    })
}

impl Side {
    pub fn open(path: &Path, password: Option<&str>, notify: Notify<'_>) -> Result<Side> {
        if fs::metadata(path)?.is_dir() {
            return Side::folder(path, notify);
        }
        let format = arca_stream::identify(path).ok_or_else(|| {
            Error::Unsupported(format!("'{}' is not an archive Arca reads", path.display()))
        })?;
        Side::archive(path, format, password, notify)
    }

    fn archive(
        path: &Path,
        format: Format,
        password: Option<&str>,
        notify: Notify<'_>,
    ) -> Result<Side> {
        let mut side = Side {
            items: BTreeMap::new(),
            source: Source::Folder,
        };
        let mut tar_kinds: Vec<TarKind> = Vec::new();
        let entries = match format.container() {
            Container::Zip => arca_zip::ZipArchive::open(File::open(path)?)?
                .entries()
                .to_vec(),
            Container::SevenZ => arca_7z::SevenZArchive::open(File::open(path)?, password)?
                .entries()
                .to_vec(),
            Container::Rar => arca_rar::RarArchive::open_with_progress(path, password, notify)?
                .entries()
                .to_vec(),
            Container::Iso => arca_iso::IsoArchive::open_with_progress(path, notify)?
                .entries()
                .to_vec(),
            Container::Xz | Container::Bz2 | Container::Zst => {
                vec![arca_stream::entry(path, format)?]
            }
            Container::Tar
            | Container::TarGz
            | Container::TarXz
            | Container::TarBz2
            | Container::TarZst => {
                let mut r = arca_tar::TarReader::new(crate::digest::tar_source(path, format)?);
                let mut entries = Vec::new();
                while let Some(e) = r.next_entry()? {
                    if !notify(entries.len(), 0, &e.entry.name) {
                        return Err(Error::Cancelled);
                    }
                    tar_kinds.push(match (e.symlink(), e.hard_link()) {
                        (Some(target), _) => TarKind::Symlink(target.to_string()),
                        (_, Some(target)) => TarKind::Hard(target.to_string()),
                        _ if e.is_plain() => TarKind::Plain,
                        _ => TarKind::Special,
                    });
                    entries.push(e.entry.clone());
                    r.skip_data(&e)?;
                }
                arca_stream::drain(&mut r.into_inner())?;
                entries
            }
        };
        let stream = matches!(
            format.container(),
            Container::Xz | Container::Bz2 | Container::Zst
        );
        for (i, e) in entries.iter().enumerate() {
            let name = key(&e.name);
            if name.is_empty() {
                continue;
            }
            let item = match tar_kinds.get(i).cloned().unwrap_or(TarKind::Plain) {
                TarKind::Special => continue,
                TarKind::Symlink(target) => Item {
                    kind: Kind::Link(target),
                    size: None,
                    crc32: None,
                    mtime: e.mtime,
                    origin: Origin::Entry(i),
                },
                TarKind::Hard(target) => match side.items.get(&key(&target)) {
                    Some(earlier) if earlier.kind == Kind::File => Item {
                        mtime: e.mtime,
                        ..earlier.clone()
                    },
                    _ => continue,
                },
                TarKind::Plain if e.is_dir => Item {
                    kind: Kind::Folder,
                    size: None,
                    crc32: None,
                    mtime: e.mtime,
                    origin: Origin::Entry(i),
                },
                TarKind::Plain => Item {
                    kind: Kind::File,
                    size: (!stream).then_some(e.size),
                    crc32: e.crc32.filter(|&crc| !(e.encrypted && crc == 0)),
                    mtime: e.mtime,
                    origin: Origin::Entry(i),
                },
            };
            side.items.insert(name, item);
        }
        side.imply_folders();
        side.source = Source::Archive {
            path: path.to_path_buf(),
            format,
            entries,
            password: password.map(str::to_string),
        };
        Ok(side)
    }

    fn folder(root: &Path, notify: Notify<'_>) -> Result<Side> {
        let mut side = Side {
            items: BTreeMap::new(),
            source: Source::Folder,
        };
        let mut pending = vec![(root.to_path_buf(), String::new())];
        while let Some((dir, prefix)) = pending.pop() {
            if !notify(side.items.len(), 0, &prefix) {
                return Err(Error::Cancelled);
            }
            for child in fs::read_dir(&dir)? {
                let child = child?;
                let path = child.path();
                let name = format!("{prefix}{}", child.file_name().to_string_lossy());
                let meta = fs::symlink_metadata(&path)?;
                let kind = if meta.file_type().is_symlink() {
                    Kind::Link(fs::read_link(&path)?.to_string_lossy().replace('\\', "/"))
                } else if meta.is_dir() {
                    pending.push((path.clone(), format!("{name}/")));
                    Kind::Folder
                } else if meta.is_file() {
                    Kind::File
                } else {
                    continue;
                };
                let size = (kind == Kind::File).then_some(meta.len());
                side.items.insert(
                    name,
                    Item {
                        kind,
                        size,
                        crc32: None,
                        mtime: mtime_of(&meta),
                        origin: Origin::Disk(path),
                    },
                );
            }
        }
        Ok(side)
    }

    fn imply_folders(&mut self) {
        let mut implied = Vec::new();
        for name in self.items.keys() {
            let mut at = name.len();
            while let Some(slash) = name[..at].rfind('/') {
                let parent = &name[..slash];
                if self.items.contains_key(parent) {
                    break;
                }
                implied.push(parent.to_string());
                at = slash;
            }
        }
        for parent in implied {
            self.items.entry(parent).or_insert(Item {
                kind: Kind::Folder,
                size: None,
                crc32: None,
                mtime: None,
                origin: Origin::Implied,
            });
        }
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
}
