//! The disk as a listing: folders read as they are walked into, so the
//! window opens on the computer the way an Explorer does and an archive is
//! one more folder to step into.

use super::*;
use arca_core::Entry;
use std::time::UNIX_EPOCH;

const ATTR_READ_ONLY: u8 = 0x01;
const ATTR_HIDDEN: u8 = 0x02;

#[derive(Default)]
pub(crate) struct DiskState {
    pub(crate) loaded: HashSet<String>,
}

/// Where an archive was stepped into from, so that closing it lands back on
/// the same row of the same folder.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Origin {
    pub(crate) directory: String,
    pub(crate) path: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PlaceKind {
    Home,
    Desktop,
    Documents,
    Downloads,
    Pictures,
    Music,
    Videos,
    Computer,
    Drive,
    Pinned,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Place {
    pub(crate) kind: PlaceKind,
    pub(crate) label: String,
    pub(crate) path: PathBuf,
}

/// The listing name of a folder on disk: forward slashes, a trailing slash,
/// and nothing for the top of the tree. On Windows the top is the list of
/// drives and every path keeps its drive letter.
pub(crate) fn disk_dir(path: &Path) -> String {
    let text = path.to_string_lossy().replace('\\', "/");
    let text = if cfg!(windows) {
        text.trim_start_matches("//?/").to_string()
    } else {
        text
    };
    let trimmed = text.trim_matches('/');
    if trimmed.is_empty() {
        String::new()
    } else {
        format!("{trimmed}/")
    }
}

/// The path on disk behind a listing name, folder or file.
pub(crate) fn disk_path(name: &str) -> PathBuf {
    let name = name.trim_end_matches('/');
    if cfg!(windows) {
        if name.is_empty() {
            return PathBuf::new();
        }
        let mut text = name.replace('/', "\\");
        if text.len() == 2 && text.ends_with(':') {
            text.push('\\');
        }
        PathBuf::from(text)
    } else {
        PathBuf::from(format!("/{name}"))
    }
}

pub(crate) fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .filter(|home| home.is_dir())
}

fn seconds(time: std::io::Result<std::time::SystemTime>) -> Option<i64> {
    let time = time.ok()?;
    match time.duration_since(UNIX_EPOCH) {
        Ok(since) => i64::try_from(since.as_secs()).ok(),
        Err(before) => i64::try_from(before.duration().as_secs()).ok().map(|s| -s),
    }
}

#[cfg(windows)]
fn native_attributes(meta: &fs::Metadata) -> u8 {
    use std::os::windows::fs::MetadataExt;
    let attrs = meta.file_attributes();
    let mut out = 0;
    if attrs & 0x01 != 0 {
        out |= ATTR_READ_ONLY;
    }
    if attrs & 0x02 != 0 {
        out |= ATTR_HIDDEN;
    }
    if attrs & 0x04 != 0 {
        out |= 0x04;
    }
    if attrs & 0x20 != 0 {
        out |= 0x20;
    }
    out
}

#[cfg(not(windows))]
fn native_attributes(meta: &fs::Metadata) -> u8 {
    if meta.permissions().readonly() {
        ATTR_READ_ONLY
    } else {
        0
    }
}

fn disk_entry(name: String, is_dir: bool, meta: Option<&fs::Metadata>, hidden: bool) -> Entry {
    let size = meta.filter(|_| !is_dir).map_or(0, fs::Metadata::len);
    let mut attributes = meta.map_or(0, native_attributes);
    if hidden {
        attributes |= ATTR_HIDDEN;
    }
    Entry {
        raw_name: name.as_bytes().to_vec(),
        name,
        size,
        compressed_size: size,
        method: arca_core::Method::Store,
        crc32: None,
        is_dir,
        mtime: meta.and_then(|m| seconds(m.modified())),
        created: meta.and_then(|m| seconds(m.created())),
        accessed: meta.and_then(|m| seconds(m.accessed())),
        attributes,
        offset: 0,
        utf8: true,
        encrypted: false,
        zipcrypto: false,
    }
}

fn hidden_on_disk(name: &str, meta: Option<&fs::Metadata>) -> bool {
    if cfg!(windows) {
        meta.is_some_and(|m| native_attributes(m) & ATTR_HIDDEN != 0)
    } else {
        name.starts_with('.')
    }
}

#[cfg(windows)]
fn drives() -> Vec<PathBuf> {
    (b'A'..=b'Z')
        .map(|letter| PathBuf::from(format!("{}:\\", letter as char)))
        .filter(|root| fs::metadata(root).is_ok())
        .collect()
}

/// The children of one folder as listing entries, read once and cached by
/// the caller. Hidden files stay out unless asked for; what is hidden is
/// the dot on Unix and the attribute on Windows.
pub(crate) fn list_directory(dir: &str, show_hidden: bool) -> std::io::Result<Vec<Entry>> {
    #[cfg(windows)]
    if dir.is_empty() {
        return Ok(drives()
            .into_iter()
            .map(|root| {
                disk_entry(
                    disk_dir(&root),
                    true,
                    fs::metadata(&root).ok().as_ref(),
                    false,
                )
            })
            .collect());
    }
    let mut entries = Vec::new();
    for item in fs::read_dir(disk_path(dir))? {
        let item = item?;
        let leaf = item.file_name().to_string_lossy().to_string();
        if leaf.is_empty() || leaf.contains('/') {
            continue;
        }
        let meta = fs::metadata(item.path()).or_else(|_| item.metadata()).ok();
        let hidden = hidden_on_disk(&leaf, meta.as_ref());
        if hidden && !show_hidden {
            continue;
        }
        let is_dir = meta.as_ref().is_some_and(fs::Metadata::is_dir);
        let name = if is_dir {
            format!("{dir}{leaf}/")
        } else {
            format!("{dir}{leaf}")
        };
        entries.push(disk_entry(name, is_dir, meta.as_ref(), hidden));
    }
    entries.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(entries)
}

/// Every folder on the way down to `dir`, the top included and `dir` itself
/// last, so each can be listed before the one under it is looked for.
pub(crate) fn ancestors_of(dir: &str) -> Vec<String> {
    let mut out = vec![String::new()];
    let mut walked = String::new();
    for part in dir.split('/').filter(|part| !part.is_empty()) {
        walked.push_str(part);
        walked.push('/');
        out.push(walked.clone());
    }
    out
}

#[cfg(all(unix, not(target_os = "macos")))]
fn user_dirs(home: &Path) -> HashMap<&'static str, PathBuf> {
    let mut out = HashMap::new();
    let Ok(text) = fs::read_to_string(home.join(".config/user-dirs.dirs")) else {
        return out;
    };
    for line in text.lines() {
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let value = value.trim().trim_matches('"');
        let value = value.replace("$HOME", &home.to_string_lossy());
        let key: &'static str = match key.trim() {
            "XDG_DESKTOP_DIR" => "Desktop",
            "XDG_DOCUMENTS_DIR" => "Documents",
            "XDG_DOWNLOAD_DIR" => "Downloads",
            "XDG_PICTURES_DIR" => "Pictures",
            "XDG_MUSIC_DIR" => "Music",
            "XDG_VIDEOS_DIR" => "Videos",
            _ => continue,
        };
        out.insert(key, PathBuf::from(value));
    }
    out
}

#[cfg(not(all(unix, not(target_os = "macos"))))]
fn user_dirs(_home: &Path) -> HashMap<&'static str, PathBuf> {
    HashMap::new()
}

fn mounted_volumes() -> Vec<PathBuf> {
    let mut roots: Vec<PathBuf> = Vec::new();
    for base in ["/media", "/run/media", "/mnt", "/Volumes"] {
        let base = Path::new(base);
        let Ok(items) = fs::read_dir(base) else {
            continue;
        };
        for item in items.flatten() {
            let path = item.path();
            if !path.is_dir() {
                continue;
            }
            // /media and /run/media hold one folder per user with the
            // volumes under it; everything else mounts straight underneath.
            if base.ends_with("media") && path.join("..").is_dir() {
                if let Ok(nested) = fs::read_dir(&path) {
                    let found: Vec<PathBuf> = nested
                        .flatten()
                        .map(|n| n.path())
                        .filter(|p| p.is_dir())
                        .collect();
                    if !found.is_empty() {
                        roots.extend(found);
                        continue;
                    }
                }
            }
            roots.push(path);
        }
    }
    roots.sort();
    roots.dedup();
    roots
}

impl AppController {
    pub(crate) fn writable(&self) -> bool {
        self.state.archive.is_some() && self.state.format == Format::Zip
    }

    pub(crate) fn on_disk(&self) -> bool {
        self.state.archive.is_none() && self.state.disk.is_some()
    }

    pub(crate) fn root_label(&self) -> &'static str {
        if self.on_disk() {
            self.s().computer_word
        } else {
            self.s().archive_root
        }
    }

    pub(crate) fn folder_total(&self) -> usize {
        if self.on_disk() {
            children_of(&self.state.entries, &self.state.current_dir).len()
        } else {
            self.state.entries.len()
        }
    }

    /// The archive being read came off the disk the window was browsing.
    pub(crate) fn can_close_archive(&self) -> bool {
        self.state.archive.is_some() && self.state.disk.is_some()
    }

    pub(crate) fn can_go_up(&self) -> bool {
        !self.state.current_dir.is_empty() || self.can_close_archive()
    }

    pub(crate) fn go_up(&mut self) {
        if !self.state.current_dir.is_empty() {
            self.go_to(parent_of(&self.state.current_dir));
        } else if self.can_close_archive() {
            self.close_archive();
        }
    }

    /// Where the window opens when nothing was asked for: the folder it was
    /// last in if it still exists, otherwise home, otherwise the top.
    pub(crate) fn start_browsing(&mut self) {
        let remembered = self
            .state
            .settings
            .last_folder
            .as_deref()
            .map(PathBuf::from)
            .filter(|path| path.is_dir());
        let path = remembered
            .or_else(home_dir)
            .unwrap_or_else(|| disk_path(""));
        self.browse_disk(&path);
    }

    /// Shows a folder on disk, closing whatever archive was open.
    pub(crate) fn browse_disk(&mut self, path: &Path) {
        if self.state.busy && !self.state.listing {
            return;
        }
        self.state.listing = false;
        self.state.busy = false;
        self.cancel_preview();
        self.state.archive = None;
        self.state.origin = None;
        self.state.entries.clear();
        self.state.checked.clear();
        self.state.cursor = None;
        self.state.folders = tree::folders_of(&[]);
        self.state.history.clear();
        self.state.here = 0;
        self.state.undo = None;
        self.state.renaming = None;
        self.state.archive_password = None;
        self.state.waiting_on_password = None;
        self.state.password_input.clear();
        self.state.password_wrong = false;
        self.state.format = Format::Zip;
        self.state.disk = Some(DiskState::default());
        let dir = if path.as_os_str().is_empty() {
            String::new()
        } else {
            disk_dir(path)
        };
        self.reset_browser_panes();
        self.navigate_panes(dir);
        self.record_pane_history();
        self.refresh_disk_title();
    }

    pub(crate) fn refresh_disk_title(&mut self) {
        if !self.on_disk() {
            return;
        }
        let leaf = self
            .state
            .current_dir
            .trim_end_matches('/')
            .rsplit('/')
            .next()
            .unwrap_or("")
            .to_string();
        self.state.window_title = if leaf.is_empty() {
            "Arca".to_string()
        } else {
            format!("{leaf} - Arca")
        };
    }

    /// Lists every folder on the way to `dir` that has not been listed yet.
    /// A folder that cannot be read is remembered as read so that the window
    /// does not ask again on every repaint, and says so once.
    pub(crate) fn load_disk_ancestors(&mut self, dir: &str) {
        if !self.on_disk() {
            return;
        }
        let show_hidden = self.state.settings.show_hidden;
        for ancestor in ancestors_of(dir) {
            let already = self
                .state
                .disk
                .as_ref()
                .is_some_and(|disk| disk.loaded.contains(&ancestor));
            if already {
                continue;
            }
            match list_directory(&ancestor, show_hidden) {
                Ok(entries) => self.state.entries.extend(entries),
                Err(error) => {
                    let name = ancestor
                        .trim_end_matches('/')
                        .rsplit('/')
                        .next()
                        .unwrap_or("");
                    self.state.notice =
                        format!("{}: {error}", if name.is_empty() { "/" } else { name });
                    self.state.error = true;
                }
            }
            if let Some(disk) = self.state.disk.as_mut() {
                disk.loaded.insert(ancestor);
            }
        }
        self.state.checked.resize(self.state.entries.len(), false);
    }

    /// Reads the folders being shown again, for a disk that changed under
    /// the window.
    pub(crate) fn refresh_disk(&mut self) {
        if !self.on_disk() {
            return;
        }
        let dir = self.state.current_dir.clone();
        self.state.entries.clear();
        self.state.checked.clear();
        self.state.disk = Some(DiskState::default());
        self.load_disk_ancestors(&dir);
        self.navigate_panes(dir);
    }

    pub(crate) fn remember_folder(&mut self) {
        if !self.on_disk() {
            return;
        }
        let path = disk_path(&self.state.current_dir);
        let text = (!path.as_os_str().is_empty()).then(|| path.to_string_lossy().to_string());
        if self.state.settings.last_folder != text {
            self.state.settings.last_folder = text;
            self.state.settings.save();
        }
    }

    /// A row on disk, opened: a folder is walked into, an archive is read in
    /// place, and anything else is handed to whatever the system opens it
    /// with.
    pub(crate) fn open_disk_entry(&mut self, index: usize) {
        let Some(entry) = self.state.entries.get(index).cloned() else {
            return;
        };
        let path = disk_path(&entry.name);
        if entry.is_dir {
            self.go_to(entry.name);
            return;
        }
        if detect(&path).is_some() {
            let origin = Origin {
                directory: self.state.current_dir.clone(),
                path: entry.name.clone(),
            };
            self.open(path);
            self.state.origin = Some(origin);
            return;
        }
        match launch_with_system(&path) {
            Ok(()) => {
                self.state.notice.clear();
                self.state.error = false;
            }
            Err(error) => {
                self.state.notice = error.to_string();
                self.state.error = true;
            }
        }
    }

    /// Steps back out of the archive onto the folder it was opened from.
    pub(crate) fn close_archive(&mut self) {
        if self.state.busy && !self.state.listing {
            return;
        }
        let origin = self.state.origin.take();
        let error = self.state.error;
        let notice = if error {
            std::mem::take(&mut self.state.notice)
        } else {
            String::new()
        };
        let target = origin
            .as_ref()
            .map(|origin| disk_path(&origin.directory))
            .or_else(|| {
                self.state
                    .settings
                    .last_folder
                    .as_deref()
                    .map(PathBuf::from)
                    .filter(|path| path.is_dir())
            })
            .or_else(home_dir)
            .unwrap_or_else(|| disk_path(""));
        self.browse_disk(&target);
        self.state.notice = notice;
        self.state.error = error;
        if let Some(origin) = origin {
            if let Some(cursor) = self
                .visible_rows()
                .iter()
                .position(|row| !row.up && row.path == origin.path)
            {
                self.set_pane_cursor(self.state.browser.active, Some(cursor));
            }
        }
    }

    /// The paths on disk behind the ticked rows, for compressing them.
    pub(crate) fn selected_disk_paths(&self) -> Vec<PathBuf> {
        if !self.on_disk() {
            return Vec::new();
        }
        self.selected_roots()
            .iter()
            .map(|root| disk_path(root))
            .collect()
    }

    pub(crate) fn places(&self) -> Vec<Place> {
        let s = self.s();
        let mut out = Vec::new();
        if let Some(home) = home_dir() {
            out.push(Place {
                kind: PlaceKind::Home,
                label: s.home_word.to_string(),
                path: home.clone(),
            });
            let known = user_dirs(&home);
            for (kind, name, label) in [
                (PlaceKind::Desktop, "Desktop", s.desktop_word),
                (PlaceKind::Documents, "Documents", s.documents_word),
                (PlaceKind::Downloads, "Downloads", s.downloads_word),
                (PlaceKind::Pictures, "Pictures", s.pictures_word),
                (PlaceKind::Music, "Music", s.music_word),
                (PlaceKind::Videos, "Videos", s.videos_word),
            ] {
                let path = known.get(name).cloned().unwrap_or_else(|| home.join(name));
                if path.is_dir() && path != home {
                    out.push(Place {
                        kind,
                        label: label.to_string(),
                        path,
                    });
                }
            }
        }
        out
    }

    pub(crate) fn pinned_places(&self) -> Vec<Place> {
        self.state
            .settings
            .pinned
            .iter()
            .map(|text| {
                let path = PathBuf::from(text);
                let label = path
                    .file_name()
                    .map(|name| name.to_string_lossy().to_string())
                    .unwrap_or_else(|| text.clone());
                Place {
                    kind: PlaceKind::Pinned,
                    label,
                    path,
                }
            })
            .collect()
    }

    pub(crate) fn devices(&self) -> Vec<Place> {
        let s = self.s();
        let mut out = Vec::new();
        if cfg!(windows) {
            out.push(Place {
                kind: PlaceKind::Computer,
                label: s.computer_word.to_string(),
                path: PathBuf::new(),
            });
            #[cfg(windows)]
            for root in drives() {
                out.push(Place {
                    kind: PlaceKind::Drive,
                    label: root.to_string_lossy().trim_end_matches('\\').to_string(),
                    path: root,
                });
            }
        } else {
            out.push(Place {
                kind: PlaceKind::Computer,
                label: s.computer_word.to_string(),
                path: PathBuf::from("/"),
            });
            for root in mounted_volumes() {
                let label = root
                    .file_name()
                    .map(|name| name.to_string_lossy().to_string())
                    .unwrap_or_default();
                out.push(Place {
                    kind: PlaceKind::Drive,
                    label,
                    path: root,
                });
            }
        }
        out
    }

    pub(crate) fn is_pinned(&self, path: &Path) -> bool {
        let text = path.to_string_lossy();
        self.state.settings.pinned.iter().any(|p| *p == text)
    }

    pub(crate) fn toggle_pinned(&mut self, path: &Path) {
        let text = path.to_string_lossy().to_string();
        if self.is_pinned(path) {
            self.state.settings.pinned.retain(|p| *p != text);
        } else {
            self.state.settings.pinned.push(text);
        }
        self.state.settings.save();
    }

    pub(crate) fn set_show_hidden(&mut self, show: bool) {
        if self.state.settings.show_hidden == show {
            return;
        }
        self.state.settings.show_hidden = show;
        self.state.settings.save();
        self.refresh_disk();
    }
}
