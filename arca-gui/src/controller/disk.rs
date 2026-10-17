//! The disk as a listing: folders read as they are walked into, so the
//! window opens on the computer the way an Explorer does and an archive is
//! one more folder to step into.

use super::*;
use crate::archive_ops::{free_name, Answer};
use arca_core::Entry;
use std::collections::HashSet;
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
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
    Removable,
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

#[cfg(unix)]
fn volume_of(path: &Path) -> Option<u64> {
    use std::os::unix::fs::MetadataExt;
    fs::symlink_metadata(path).ok().map(|meta| meta.dev())
}

/// The drive letter stands for the volume. A junction onto another drive
/// gets through, and comes out as a failed rename with a notice.
#[cfg(not(unix))]
fn volume_of(path: &Path) -> Option<u64> {
    let first = path.components().next()?;
    let text = first.as_os_str().to_string_lossy().to_lowercase();
    Some(text.bytes().fold(0u64, |hash, byte| {
        hash.wrapping_mul(31).wrapping_add(u64::from(byte))
    }))
}

/// The worker's side of the name question: it sends the destination and
/// the source, waits for the window, and hands back the answer with the
/// name typed for a Rename. The "all" answers stick for the rest of the run.
/// The window's answer to a taken name, with the name typed for a Rename.
type AskName<'a> = dyn Fn(&Path, &Path) -> (Answer, Option<String>) + 'a;

fn disk_asker<'a>(
    tx: &'a Sender<Message>,
    replies: &'a std::sync::mpsc::Receiver<Answer>,
    chosen: &'a Arc<Mutex<Option<String>>>,
) -> impl Fn(&Path, &Path) -> (Answer, Option<String>) + 'a {
    let sticky = std::cell::Cell::new(None::<Answer>);
    move |to: &Path, from: &Path| {
        if let Some(a) = sticky.get() {
            return (a, None);
        }
        let question =
            Message::ConflictBetween(to.display().to_string(), from.display().to_string());
        if tx.send(question).is_err() {
            return (Answer::Cancel, None);
        }
        let answer = replies.recv().unwrap_or(Answer::Cancel);
        if matches!(
            answer,
            Answer::ReplaceAll | Answer::SkipAll | Answer::RenameAll | Answer::Cancel
        ) {
            sticky.set(Some(answer));
        }
        let name = chosen.lock().ok().and_then(|mut slot| slot.take());
        (answer, name)
    }
}

/// Moves or copies each picked file or folder under `dest`, keeping names
/// unless one is taken and the window says otherwise. The result is how
/// many were carried over, or the message for the one that could not be.
fn transfer_trees(
    picked: &[(PathBuf, String)],
    dest: &Path,
    copy: bool,
    notify: &dyn Fn(usize, usize, &str) -> bool,
    ask: &AskName<'_>,
    s: &'static crate::i18n::Strings,
) -> Result<usize, String> {
    let cancelled = || arca_core::Error::Cancelled.to_string();
    let total = picked.len();
    let mut carried = 0;
    let mut claimed: HashSet<PathBuf> = HashSet::new();
    for (i, (from, leaf)) in picked.iter().enumerate() {
        if !notify(i, total, leaf) {
            return Err(cancelled());
        }
        let mut to = dest.join(leaf);
        if to.symlink_metadata().is_ok() {
            let (answer, name) = ask(&to, from);
            match answer {
                Answer::Cancel => return Err(cancelled()),
                Answer::Skip | Answer::SkipAll => continue,
                Answer::Rename | Answer::RenameAll => {
                    to = match name.filter(|n| !n.is_empty() && !n.contains(['/', '\\'])) {
                        Some(n) if answer == Answer::Rename => dest.join(n),
                        _ => free_name(&to, &claimed),
                    };
                    if to.symlink_metadata().is_ok() {
                        to = free_name(&to, &claimed);
                    }
                }
                Answer::Replace | Answer::ReplaceAll => {
                    let gone = if to.is_dir() {
                        fs::remove_dir_all(&to)
                    } else {
                        fs::remove_file(&to)
                    };
                    gone.map_err(|e| could_not(s, copy, &to, &e))?;
                }
            }
        }
        claimed.insert(to.clone());
        if copy {
            copy_tree(from, &to).map_err(|(path, e)| could_not(s, true, &path, &e))?;
        } else if fs::rename(from, &to).is_err() {
            // Across volumes a rename cannot do it: copy, then take the
            // original away once every file is over.
            copy_tree(from, &to).map_err(|(path, e)| could_not(s, false, &path, &e))?;
            let gone = if from.is_dir() {
                fs::remove_dir_all(from)
            } else {
                fs::remove_file(from)
            };
            gone.map_err(|e| could_not(s, false, from, &e))?;
        }
        carried += 1;
    }
    if !notify(total, total, "") {
        return Err(cancelled());
    }
    Ok(carried)
}

/// Copies a file, or a folder and everything under it, to `to`. The error
/// is the path that could not be read or written, and why.
pub(crate) fn copy_tree(
    from: &Path,
    to: &Path,
) -> std::result::Result<(), (PathBuf, std::io::Error)> {
    let at = |path: &Path| {
        let path = path.to_path_buf();
        move |e| (path, e)
    };
    let meta = fs::symlink_metadata(from).map_err(at(from))?;
    if meta.is_dir() {
        fs::create_dir_all(to).map_err(at(to))?;
        let items = fs::read_dir(from).map_err(at(from))?;
        for item in items.flatten() {
            copy_tree(&item.path(), &to.join(item.file_name()))?;
        }
        Ok(())
    } else {
        if let Some(parent) = to.parent() {
            fs::create_dir_all(parent).map_err(at(parent))?;
        }
        fs::copy(from, to).map(|_| ()).map_err(at(from))
    }
}

pub(crate) fn could_not(
    s: &'static crate::i18n::Strings,
    copy: bool,
    path: &Path,
    error: &std::io::Error,
) -> String {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| path.display().to_string());
    let what = fill(
        if copy { s.cannot_copy } else { s.cannot_move },
        &[("name", &name)],
    );
    format!("{what}: {error}")
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

pub(crate) fn hidden_on_disk(name: &str, meta: Option<&fs::Metadata>) -> bool {
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
        if leaf.is_empty() || leaf.contains(['/', '\\']) {
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

/// The outcome a device job in the background leaves for the window.
pub(crate) type DeviceJob = Arc<Mutex<Option<Result<String, String>>>>;

/// A storage volume the sidebar can offer: a partition with a file system
/// that is not part of the running system.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Volume {
    /// `/dev/sdb1`; empty where the platform gives no device node.
    pub(crate) device: String,
    pub(crate) label: String,
    pub(crate) model: String,
    pub(crate) size: String,
    pub(crate) fs: String,
    pub(crate) removable: bool,
    pub(crate) mount: Option<PathBuf>,
}

impl Volume {
    pub(crate) fn name(&self, volume_word: &str) -> String {
        if !self.label.is_empty() {
            self.label.clone()
        } else if !self.model.is_empty() {
            format!("{} {}", self.model, self.size)
        } else if let Some(name) = self
            .mount
            .as_ref()
            .and_then(|m| m.file_name())
            .map(|n| n.to_string_lossy().to_string())
        {
            name
        } else {
            format!("{} {volume_word}", self.size)
        }
    }

    /// Where the sidebar sends a click: the mount point, or the device node
    /// until it is mounted.
    pub(crate) fn place(&self) -> PathBuf {
        self.mount
            .clone()
            .unwrap_or_else(|| PathBuf::from(&self.device))
    }
}

/// Mount points that belong to the system rather than to a volume someone
/// plugged in or set aside.
fn system_mount(mount: &str) -> bool {
    mount == "/"
        || mount == "[SWAP]"
        || [
            "/boot", "/snap/", "/usr", "/var", "/home", "/opt", "/srv", "/tmp", "/nix", "/efi",
        ]
        .iter()
        .any(|base| mount == base.trim_end_matches('/') || mount.starts_with(base))
}

#[cfg(target_os = "linux")]
fn lsblk_unescape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\\' && chars.peek() == Some(&'x') {
            chars.next();
            let hex: String = chars.by_ref().take(2).collect();
            if let Ok(code) = u8::from_str_radix(&hex, 16) {
                out.push(code as char);
                continue;
            }
            out.push_str("\\x");
            out.push_str(&hex);
        } else {
            out.push(c);
        }
    }
    out
}

/// The volumes `lsblk -P` describes, one `KEY="value"` line per device.
#[cfg(target_os = "linux")]
pub(crate) fn volumes_from_lsblk(text: &str) -> Vec<Volume> {
    let mut out = Vec::new();
    for line in text.lines() {
        let mut fields: HashMap<&str, String> = HashMap::new();
        let mut rest = line.trim();
        while let Some((key, after)) = rest.split_once("=\"") {
            let Some((value, tail)) = after.split_once('"') else {
                break;
            };
            fields.insert(key.trim(), lsblk_unescape(value));
            rest = tail.trim_start();
        }
        let field = |key: &str| fields.get(key).cloned().unwrap_or_default();
        let path = field("PATH");
        let fs = field("FSTYPE");
        let kind = field("TYPE");
        let mount = field("MOUNTPOINT");
        let removable = field("RM") == "1" || field("HOTPLUG") == "1";
        let virtual_node = ["/dev/loop", "/dev/zram", "/dev/ram", "/dev/sr"]
            .iter()
            .any(|base| path.starts_with(base));
        let no_filesystem = fs.is_empty()
            || matches!(
                fs.as_str(),
                "swap" | "squashfs" | "LVM2_member" | "crypto_LUKS" | "linux_raid_member"
            );
        if virtual_node
            || no_filesystem
            || !matches!(kind.as_str(), "part" | "disk" | "crypt")
            || (!mount.is_empty() && system_mount(&mount))
        {
            continue;
        }
        let set_aside = ["/media/", "/run/media/", "/mnt/"]
            .iter()
            .any(|base| mount.starts_with(base) || mount == base.trim_end_matches('/'));
        if !removable && !mount.is_empty() && !set_aside {
            continue;
        }
        out.push(Volume {
            device: path,
            label: field("LABEL"),
            model: field("MODEL").trim().to_string(),
            size: field("SIZE"),
            fs,
            removable,
            mount: (!mount.is_empty()).then(|| PathBuf::from(mount)),
        });
    }
    out
}

#[cfg(target_os = "linux")]
pub(crate) fn volumes() -> Vec<Volume> {
    let listed = std::process::Command::new("lsblk")
        .args([
            "-P",
            "-n",
            "-o",
            "NAME,PATH,LABEL,MOUNTPOINT,RM,HOTPLUG,FSTYPE,SIZE,TYPE,MODEL",
        ])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| volumes_from_lsblk(&String::from_utf8_lossy(&o.stdout)));
    match listed {
        Some(volumes) => volumes,
        None => mounted_volumes()
            .into_iter()
            .map(|mount| Volume {
                mount: Some(mount),
                ..Volume::default()
            })
            .collect(),
    }
}

#[cfg(not(target_os = "linux"))]
pub(crate) fn volumes() -> Vec<Volume> {
    mounted_volumes()
        .into_iter()
        .map(|mount| Volume {
            mount: Some(mount),
            ..Volume::default()
        })
        .collect()
}

#[cfg(target_os = "linux")]
fn udisks_object(device: &str) -> String {
    let name = device.trim_start_matches("/dev/");
    let mut out = String::from("/org/freedesktop/UDisks2/block_devices/");
    for c in name.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c);
        } else {
            out.push_str(&format!("_{:02x}", c as u32));
        }
    }
    out
}

#[cfg(target_os = "linux")]
fn run_device_command(program: &str, args: &[&str]) -> Result<String, String> {
    let output = std::process::Command::new(program)
        .args(args)
        .output()
        .map_err(|e| e.to_string())?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        let text = String::from_utf8_lossy(&output.stderr);
        let text = text.trim();
        let text = text.rsplit_once(": ").map_or(text, |(_, last)| last);
        Err(text.trim_end_matches('.').to_string())
    }
}

/// Mounts the volume through UDisks and tells where it landed.
#[cfg(target_os = "linux")]
pub(crate) fn mount_device(device: &str) -> Result<PathBuf, String> {
    let said = run_device_command("udisksctl", &["mount", "-b", device])?;
    said.rsplit_once(" at ")
        .map(|(_, at)| PathBuf::from(at.trim_end_matches('.')))
        .ok_or(said)
}

#[cfg(target_os = "linux")]
pub(crate) fn unmount_device(device: &str) -> Result<(), String> {
    run_device_command("udisksctl", &["unmount", "-b", device]).map(|_| ())
}

#[cfg(target_os = "linux")]
pub(crate) fn label_device(device: &str, label: &str) -> Result<(), String> {
    let object = udisks_object(device);
    run_device_command(
        "gdbus",
        &[
            "call",
            "--system",
            "--dest",
            "org.freedesktop.UDisks2",
            "--object-path",
            &object,
            "--method",
            "org.freedesktop.UDisks2.Filesystem.SetLabel",
            label,
            "{}",
        ],
    )
    .map(|_| ())
}

/// The file systems the format box offers, in the order it shows them.
pub(crate) const FORMATS: [&str; 4] = ["ext4", "exfat", "vfat", "ntfs"];

#[cfg(target_os = "linux")]
pub(crate) fn format_device(device: &str, fs: &str, label: &str) -> Result<(), String> {
    let object = udisks_object(device);
    let options = if label.is_empty() {
        "{}".to_string()
    } else {
        format!(
            "{{'label': <'{}'>}}",
            label.replace('\\', "\\\\").replace('\'', "\\'")
        )
    };
    run_device_command(
        "gdbus",
        &[
            "call",
            "--system",
            "--dest",
            "org.freedesktop.UDisks2",
            "--object-path",
            &object,
            "--method",
            "org.freedesktop.UDisks2.Block.Format",
            fs,
            &options,
        ],
    )
    .map(|_| ())
}

#[cfg(not(target_os = "linux"))]
pub(crate) fn mount_device(_device: &str) -> Result<PathBuf, String> {
    Err(String::new())
}

#[cfg(not(target_os = "linux"))]
pub(crate) fn unmount_device(_device: &str) -> Result<(), String> {
    Err(String::new())
}

#[cfg(not(target_os = "linux"))]
pub(crate) fn label_device(_device: &str, _label: &str) -> Result<(), String> {
    Err(String::new())
}

#[cfg(not(target_os = "linux"))]
pub(crate) fn format_device(_device: &str, _fs: &str, _label: &str) -> Result<(), String> {
    Err(String::new())
}

impl AppController {
    pub(crate) fn writable(&self) -> bool {
        self.state.archive.is_some() && self.state.format == Format::Zip
    }

    pub(crate) fn on_disk(&self) -> bool {
        self.state.archive.is_none() && self.state.disk.is_some()
    }

    /// Whether what is shown can be renamed, removed or added to: a zip, or
    /// a folder on the disk.
    pub(crate) fn can_change(&self) -> bool {
        self.writable() || self.on_disk()
    }

    pub(crate) fn rename_on_disk(&mut self, from: &str, to: &str) {
        let s = self.s();
        let (a, b) = (disk_path(from), disk_path(to));
        if b.exists() {
            let name = b
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            self.state.notice = fill(s.name_taken, &[("name", &name)]);
            self.state.error = true;
            return;
        }
        match fs::rename(&a, &b) {
            Ok(()) => {
                self.state.notice.clear();
                self.state.error = false;
                self.refresh_disk();
            }
            Err(e) => {
                self.state.notice = e.to_string();
                self.state.error = true;
            }
        }
    }

    pub(crate) fn create_disk_folder(&mut self, name: &str) -> Option<String> {
        let s = self.s();
        let dir = format!("{}{name}/", self.state.active_pane().directory);
        let path = disk_path(&dir);
        if path.exists() {
            self.state.notice = fill(s.name_taken, &[("name", name)]);
            self.state.error = true;
            return None;
        }
        match fs::create_dir(&path) {
            Ok(()) => {
                self.state.notice.clear();
                self.state.error = false;
                self.refresh_disk();
                Some(dir)
            }
            Err(e) => {
                self.state.notice = e.to_string();
                self.state.error = true;
                None
            }
        }
    }

    /// Removes what is picked, for good or into the Trash, as a task.
    pub(crate) fn remove_on_disk(&mut self, roots: &[String], trash: bool) {
        let s = self.s();
        let picked: Vec<PathBuf> = roots.iter().map(|root| disk_path(root)).collect();
        if picked.is_empty() {
            return;
        }
        let subject = match picked.as_slice() {
            [one] => one
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default(),
            many => fill(s.items_carried, &[("n", &many.len().to_string())]),
        };
        let here = disk_path(&self.state.active_pane().directory);
        let sources = picked.clone();
        let task = Task::new(
            if trash {
                s.trashing_files
            } else {
                s.deleting_files
            },
            subject,
            Box::new(move |tx, _reply_rx, stop, hold| {
                let notify = worker_notify(&tx, &stop, &hold);
                let mut count = 0;
                for (i, path) in picked.iter().enumerate() {
                    let name = path
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_default();
                    if !notify(i, picked.len(), &name) {
                        let _ = tx.send(Message::Failed(arca_core::Error::Cancelled.to_string()));
                        return;
                    }
                    let gone = if trash {
                        trash_item(path)
                    } else {
                        remove_item(path).map_err(|e| e.to_string())
                    };
                    if gone.is_err() {
                        let word = if trash {
                            s.could_not_trash
                        } else {
                            s.could_not_remove
                        };
                        let _ = tx.send(Message::Failed(fill(word, &[("name", &name)])));
                        return;
                    }
                    count += 1;
                }
                let word = if trash { s.trashed_n } else { s.removed_n };
                let _ = tx.send(Message::Done(fill(word, &[("n", &count.to_string())])));
            }),
        )
        .reading(sources)
        .filling([here]);
        self.enqueue(task);
    }

    /// Copies what is picked next to itself as "name copy", "name copy 2"…
    pub(crate) fn duplicate_on_disk(&mut self, roots: &[String]) {
        let s = self.s();
        let picked: Vec<PathBuf> = roots.iter().map(|root| disk_path(root)).collect();
        if picked.is_empty() {
            return;
        }
        let subject = match picked.as_slice() {
            [one] => one
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default(),
            many => fill(s.items_carried, &[("n", &many.len().to_string())]),
        };
        let here = disk_path(&self.state.active_pane().directory);
        let sources = picked.clone();
        let task = Task::new(
            s.duplicating_files,
            subject,
            Box::new(move |tx, _reply_rx, stop, hold| {
                let notify = worker_notify(&tx, &stop, &hold);
                let mut count = 0;
                for (i, path) in picked.iter().enumerate() {
                    let name = path
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_default();
                    if !notify(i, picked.len(), &name) {
                        let _ = tx.send(Message::Failed(arca_core::Error::Cancelled.to_string()));
                        return;
                    }
                    let Some(to) = duplicate_target(path) else {
                        continue;
                    };
                    if let Err((_, e)) = copy_tree(path, &to) {
                        let _ = tx.send(Message::Failed(could_not(s, true, path, &e)));
                        return;
                    }
                    count += 1;
                }
                let _ = tx.send(Message::Done(fill(
                    s.duplicated_n,
                    &[("n", &count.to_string())],
                )));
            }),
        )
        .reading(sources)
        .filling([here]);
        self.enqueue(task);
    }

    /// Hands the file to a given application, or to the system's chooser.
    pub(crate) fn open_with(&mut self, path: &Path, app: Option<&DesktopApp>) {
        let s = self.s();
        let result = match app {
            Some(app) => app.launch(path),
            None => open_with_chooser(path),
        };
        match result {
            Ok(()) => {
                self.state.notice.clear();
                self.state.error = false;
            }
            Err(e) => {
                self.state.notice = if e.is_empty() {
                    s.no_opener.to_string()
                } else {
                    e
                };
                self.state.error = true;
            }
        }
    }

    /// Whether rows dragged inside the window can land on a folder here: a
    /// zip takes a rewrite, and the disk takes a rename.
    pub(crate) fn can_drop(&self) -> bool {
        self.writable() || self.on_disk()
    }

    /// Whether every carried root sits on the same volume as the target, so
    /// that a move is a rename and not a copy followed by a delete.
    pub(crate) fn same_volume(&self, roots: &[String], target: &str) -> bool {
        let into = volume_of(&disk_path(target));
        roots
            .iter()
            .all(|root| volume_of(&disk_path(root.trim_end_matches('/'))) == into)
    }

    /// Dragging rows onto a folder with a copy asked for, or across volumes.
    pub(crate) fn copy_on_disk(&mut self, roots: &[String], target: &str) {
        self.transfer_on_disk(roots, target, true);
    }

    /// Dragging rows onto a folder on the disk: a rename on the same volume,
    /// a copy and a delete across one.
    pub(crate) fn move_on_disk(&mut self, roots: &[String], target: &str) {
        self.transfer_on_disk(roots, target, false);
    }

    /// Moving or copying picked rows under `target` runs as a task with the
    /// usual progress row. A name already taken there is put to the window
    /// -- overwrite, skip, rename or stop -- the way Dolphin asks, and the
    /// answer can stick for the rest of the run.
    fn transfer_on_disk(&mut self, roots: &[String], target: &str, copy: bool) {
        let s = self.s();
        let into = disk_path(target);
        let mut picked: Vec<(PathBuf, String)> = Vec::new();
        for root in roots {
            let from = root.trim_end_matches('/');
            let Some(leaf) = from.rsplit('/').next().filter(|leaf| !leaf.is_empty()) else {
                continue;
            };
            let parent = from.rfind('/').map_or("", |cut| &from[..=cut]);
            if parent == target || target.starts_with(&format!("{from}/")) {
                continue;
            }
            picked.push((disk_path(from), leaf.to_string()));
        }
        if picked.is_empty() {
            return;
        }
        let subject = match picked.as_slice() {
            [(_, leaf)] => leaf.clone(),
            many => fill(s.items_carried, &[("n", &many.len().to_string())]),
        };
        let sources: Vec<PathBuf> = picked.iter().map(|(path, _)| path.clone()).collect();
        let dest = into.clone();
        let rename_to: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
        let chosen = Arc::clone(&rename_to);
        let verb = if copy {
            s.copying_files
        } else {
            s.moving_files
        };
        let mut task = Task::new(
            verb,
            subject,
            Box::new(move |tx, reply_rx, stop, hold| {
                let notify = worker_notify(&tx, &stop, &hold);
                let ask = disk_asker(&tx, &reply_rx, &chosen);
                let result = transfer_trees(&picked, &dest, copy, &notify, &ask, s);
                let _ = tx.send(match result {
                    Ok(count) => Message::Done(fill(
                        if copy { s.copied_into } else { s.moved_into },
                        &[
                            ("n", &count.to_string()),
                            ("dest", &dest.display().to_string()),
                        ],
                    )),
                    Err(e) => Message::Failed(e),
                });
            }),
        )
        .reading(sources)
        .filling([into]);
        task.rename_to = rename_to;
        task.stop_leaves_files = true;
        self.enqueue(task);
    }

    /// What a drag out of a disk folder carries: every picked file, and the
    /// files under every picked folder with their path from that folder, so
    /// the shell can rebuild the tree where it drops.
    #[cfg(windows)]
    pub(crate) fn disk_drag_items(&self) -> Vec<(PathBuf, String, u64, Option<i64>)> {
        const MOST: usize = 50_000;
        let mut out = Vec::new();
        for root in self.selected_roots() {
            let trimmed = root.trim_end_matches('/');
            let leaf = trimmed.rsplit('/').next().unwrap_or(trimmed).to_string();
            let mut stack = vec![(disk_path(trimmed), leaf)];
            while let Some((path, name)) = stack.pop() {
                if out.len() >= MOST {
                    return out;
                }
                let Ok(meta) = fs::metadata(&path) else {
                    continue;
                };
                if meta.is_dir() {
                    let Ok(items) = fs::read_dir(&path) else {
                        continue;
                    };
                    for item in items.flatten() {
                        let child = item.file_name().to_string_lossy().to_string();
                        stack.push((item.path(), format!("{name}\\{child}")));
                    }
                } else {
                    let mtime = meta
                        .modified()
                        .ok()
                        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                        .map(|d| d.as_secs() as i64);
                    out.push((path, name, meta.len(), mtime));
                }
            }
        }
        out
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
            children_of(&self.state.entries, &self.state.active_pane().directory).len()
        } else {
            self.state.entries.len()
        }
    }

    /// The archive being read came off the disk the window was browsing.
    pub(crate) fn can_close_archive(&self) -> bool {
        self.state.archive.is_some() && self.state.disk.is_some()
    }

    pub(crate) fn can_go_up(&self) -> bool {
        !self.state.active_pane().directory.is_empty() || self.can_close_archive()
    }

    pub(crate) fn go_up(&mut self) {
        if !self.state.active_pane().directory.is_empty() {
            self.go_to(parent_of(&self.state.active_pane().directory));
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
        self.state.active_pane_mut().checked.clear();
        self.state.active_pane_mut().cursor = None;
        self.state.folders = tree::folders_of(&[]);
        self.state.history.clear();
        self.state.here = 0;
        self.state.undo = None;
        self.state.renaming = None;
        self.state.archive_password = None;
        self.state.waiting_on_password = None;
        self.state.password_input.clear();
        self.state.password_wrong = false;
        self.state.format = Self::default_format(&self.state.settings);
        self.state.disk = Some(DiskState::default());
        let dir = if path.as_os_str().is_empty() {
            String::new()
        } else {
            disk_dir(path)
        };
        // The panes still point into the archive that was open; rebuilding
        // them from that path would try to read it off the disk.
        self.state.active_pane_mut().directory.clear();
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
            .active_pane()
            .directory
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
        let entries = self.state.entries.len();
        self.state.active_pane_mut().checked.resize(entries, false);
    }

    /// Reads the folders being shown again, for a disk that changed under
    /// the window.
    pub(crate) fn refresh_disk(&mut self) {
        if !self.on_disk() {
            return;
        }
        let dir = self.state.active_pane().directory.clone();
        let remembered: Vec<(Vec<String>, Option<String>)> = (0..self.state.browser.panes.len())
            .map(|pane| {
                let rows = self.pane_rows(pane);
                let state = &self.state.browser.panes[pane];
                let selected = state
                    .checked
                    .iter()
                    .enumerate()
                    .filter(|(_, on)| **on)
                    .filter_map(|(i, _)| self.state.entries.get(i))
                    .map(|entry| entry.name.clone())
                    .collect();
                let cursor = state
                    .cursor
                    .and_then(|i| rows.get(i))
                    .map(|row| row.path.clone());
                (selected, cursor)
            })
            .collect();
        self.state.entries.clear();
        self.state.active_pane_mut().checked.clear();
        self.state.disk = Some(DiskState::default());
        self.load_disk_ancestors(&dir);
        let by_name: HashMap<String, usize> = self
            .state
            .entries
            .iter()
            .enumerate()
            .map(|(i, entry)| (entry.name.clone(), i))
            .collect();
        let entries = self.state.entries.len();
        for (pane, (selected, cursor)) in remembered.into_iter().enumerate() {
            let rows = self.pane_rows(pane);
            let picked: HashSet<usize> = selected
                .iter()
                .filter_map(|name| by_name.get(name).copied())
                .collect();
            let state = &mut self.state.browser.panes[pane];
            state.checked = (0..entries).map(|i| picked.contains(&i)).collect();
            state.cursor = cursor.and_then(|path| rows.iter().position(|row| row.path == path));
        }
        self.navigate_panes(dir);
    }

    pub(crate) fn remember_folder(&mut self) {
        if !self.on_disk() {
            return;
        }
        let path = disk_path(&self.state.active_pane().directory);
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
                directory: self.state.active_pane().directory.clone(),
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

    /// Re-reads the volumes every few seconds and collects what a format
    /// job in the background has to say.
    pub(crate) fn refresh_volumes(&mut self) {
        let due = self
            .volumes_at
            .is_none_or(|at| at.elapsed() >= std::time::Duration::from_secs(3));
        if due {
            self.volumes = volumes();
            self.volumes_at = Some(std::time::Instant::now());
        }
        let finished = self
            .volume_job
            .as_ref()
            .and_then(|job| job.lock().ok().and_then(|mut slot| slot.take()));
        if let Some(outcome) = finished {
            self.volume_job = None;
            self.volumes_at = None;
            self.report_device(outcome);
        }
    }

    pub(crate) fn volume_at(&self, path: &Path) -> Option<&Volume> {
        self.volumes
            .iter()
            .find(|v| v.mount.as_deref() == Some(path) || Path::new(&v.device) == path)
    }

    fn report_device(&mut self, outcome: Result<String, String>) {
        let s = self.s();
        match outcome {
            Ok(said) => {
                self.state.notice = said;
                self.state.error = false;
            }
            Err(e) => {
                self.state.notice = if e.is_empty() {
                    s.no_device_tool.to_string()
                } else {
                    fill(s.device_failed, &[("error", &e)])
                };
                self.state.error = true;
            }
        }
    }

    /// Mounts the volume and tells where it landed, so the caller can go there.
    pub(crate) fn mount_volume(&mut self, volume: &Volume) -> Option<PathBuf> {
        let s = self.s();
        let name = volume.name(s.volume_word);
        let outcome = mount_device(&volume.device);
        self.volumes_at = None;
        match outcome {
            Ok(at) => {
                self.report_device(Ok(fill(
                    s.mounted_at,
                    &[("name", &name), ("path", &at.display().to_string())],
                )));
                Some(at)
            }
            Err(e) => {
                self.report_device(Err(e));
                None
            }
        }
    }

    pub(crate) fn unmount_volume(&mut self, volume: &Volume) {
        let s = self.s();
        let name = volume.name(s.volume_word);
        let outcome =
            unmount_device(&volume.device).map(|()| fill(s.unmounted, &[("name", &name)]));
        self.volumes_at = None;
        self.report_device(outcome);
    }

    pub(crate) fn label_volume(&mut self, volume: &Volume, label: &str) {
        let s = self.s();
        let name = volume.name(s.volume_word);
        let outcome = label_device(&volume.device, label)
            .map(|()| fill(s.labeled, &[("name", &name), ("label", label)]));
        self.volumes_at = None;
        self.report_device(outcome);
    }

    /// Formats in the background; `refresh_volumes` reports the outcome.
    pub(crate) fn format_volume(&mut self, volume: &Volume, fs: &str, label: &str) {
        let s = self.s();
        let name = volume.name(s.volume_word);
        self.state.notice = fill(s.formatting, &[("name", &name)]);
        self.state.error = false;
        let slot: DeviceJob = Arc::new(Mutex::new(None));
        self.volume_job = Some(slot.clone());
        let (device, fs, label) = (volume.device.clone(), fs.to_string(), label.to_string());
        let done = fill(s.formatted, &[("name", &name)]);
        std::thread::spawn(move || {
            let outcome = format_device(&device, &fs, &label).map(|()| done);
            if let Ok(mut slot) = slot.lock() {
                *slot = Some(outcome);
            }
        });
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
            for volume in &self.volumes {
                out.push(Place {
                    kind: if volume.removable {
                        PlaceKind::Removable
                    } else {
                        PlaceKind::Drive
                    },
                    label: volume.name(s.volume_word),
                    path: volume.place(),
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

fn remove_item(path: &Path) -> std::io::Result<()> {
    let meta = fs::symlink_metadata(path)?;
    if meta.is_dir() {
        fs::remove_dir_all(path)
    } else {
        fs::remove_file(path)
    }
}

/// "name copy", "name copy 2"… beside the original, keeping the extension of
/// a file where it is.
pub(crate) fn duplicate_target(path: &Path) -> Option<PathBuf> {
    let parent = path.parent()?;
    let leaf = path.file_name()?.to_string_lossy().into_owned();
    let (stem, ext) = if path.is_dir() {
        (leaf.clone(), String::new())
    } else {
        match leaf.rsplit_once('.') {
            Some((stem, ext)) if !stem.is_empty() => (stem.to_string(), format!(".{ext}")),
            _ => (leaf.clone(), String::new()),
        }
    };
    (1..10_000)
        .map(|n| {
            let name = if n == 1 {
                format!("{stem} copy{ext}")
            } else {
                format!("{stem} copy {n}{ext}")
            };
            parent.join(name)
        })
        .find(|candidate| !candidate.exists())
}

/// Moves a file or folder to the Trash the way the desktop does it.
#[cfg(all(unix, not(target_os = "macos")))]
pub(crate) fn trash_item(path: &Path) -> Result<(), String> {
    let path = fs::canonicalize(path).map_err(|e| e.to_string())?;
    let data = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| home_dir().map(|h| h.join(".local/share")))
        .ok_or_else(|| "no home".to_string())?;
    let trash = data.join("Trash");
    let (files, info) = (trash.join("files"), trash.join("info"));
    fs::create_dir_all(&files).map_err(|e| e.to_string())?;
    fs::create_dir_all(&info).map_err(|e| e.to_string())?;
    let leaf = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let (target, sheet) = (1..10_000)
        .map(|n| {
            let name = if n == 1 {
                leaf.clone()
            } else {
                format!("{leaf}.{n}")
            };
            (files.join(&name), info.join(format!("{name}.trashinfo")))
        })
        .find(|(a, b)| !a.exists() && !b.exists())
        .ok_or_else(|| "trash is full of this name".to_string())?;
    let encoded: String = path
        .to_string_lossy()
        .bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'/' | b'-' | b'_' | b'.' | b'~' => {
                (b as char).to_string()
            }
            other => format!("%{other:02X}"),
        })
        .collect();
    let when = std::time::SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let stamp = iso_utc(when);
    fs::write(
        &sheet,
        format!("[Trash Info]\nPath={encoded}\nDeletionDate={stamp}\n"),
    )
    .map_err(|e| e.to_string())?;
    if fs::rename(&path, &target).is_ok() {
        return Ok(());
    }
    let _ = fs::remove_file(&sheet);
    // Another volume: let the desktop's own tool work out that trash.
    let out = std::process::Command::new("gio")
        .arg("trash")
        .arg(&path)
        .output()
        .map_err(|e| e.to_string())?;
    if out.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}

#[cfg(target_os = "macos")]
pub(crate) fn trash_item(path: &Path) -> Result<(), String> {
    let script = format!(
        "tell application \"Finder\" to delete POSIX file \"{}\"",
        path.display()
            .to_string()
            .replace('\\', "\\\\")
            .replace('"', "\\\"")
    );
    let out = std::process::Command::new("osascript")
        .args(["-e", &script])
        .output()
        .map_err(|e| e.to_string())?;
    if out.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}

#[cfg(windows)]
pub(crate) fn trash_item(path: &Path) -> Result<(), String> {
    let what = if path.is_dir() {
        "DeleteDirectory"
    } else {
        "DeleteFile"
    };
    let script = format!(
        "Add-Type -AssemblyName Microsoft.VisualBasic; [Microsoft.VisualBasic.FileIO.FileSystem]::{what}('{}', 'OnlyErrorDialogs', 'SendToRecycleBin')",
        path.display().to_string().replace('\'', "''")
    );
    let out = std::process::Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .output()
        .map_err(|e| e.to_string())?;
    if out.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}

/// An application from a .desktop file that takes a kind of file.
#[derive(Clone, Debug)]
pub(crate) struct DesktopApp {
    pub(crate) name: String,
    pub(crate) exec: String,
    pub(crate) terminal: bool,
}

impl DesktopApp {
    pub(crate) fn launch(&self, path: &Path) -> Result<(), String> {
        let file = path.display().to_string();
        let mut argv: Vec<String> = Vec::new();
        let mut placed = false;
        for word in split_exec(&self.exec) {
            match word.as_str() {
                "%f" | "%F" | "%u" | "%U" => {
                    argv.push(file.clone());
                    placed = true;
                }
                w if w.starts_with('%') && w.len() == 2 => {}
                _ => argv.push(word),
            }
        }
        if !placed {
            argv.push(file);
        }
        let Some((program, rest)) = argv.split_first() else {
            return Err(String::new());
        };
        let mut command = if self.terminal {
            let mut c = std::process::Command::new("xdg-terminal-exec");
            c.args(&argv);
            c
        } else {
            let mut c = std::process::Command::new(program);
            c.args(rest);
            c
        };
        command
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .map(|_| ())
            .map_err(|e| e.to_string())
    }
}

/// Splits an Exec= line on spaces, honouring double quotes.
pub(crate) fn split_exec(exec: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut word = String::new();
    let mut quoted = false;
    let mut chars = exec.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' => quoted = !quoted,
            '\\' if quoted => {
                if let Some(next) = chars.next() {
                    word.push(next);
                }
            }
            ' ' if !quoted => {
                if !word.is_empty() {
                    words.push(std::mem::take(&mut word));
                }
            }
            other => word.push(other),
        }
    }
    if !word.is_empty() {
        words.push(word);
    }
    words
}

/// The applications on this desktop that declare the file's MIME type,
/// read from the .desktop files in the XDG data directories.
#[cfg(target_os = "linux")]
pub(crate) fn apps_for(path: &Path) -> Vec<DesktopApp> {
    let mime = std::process::Command::new("xdg-mime")
        .args(["query", "filetype"])
        .arg(path)
        .output()
        .ok()
        .filter(|out| out.status.success())
        .map(|out| String::from_utf8_lossy(&out.stdout).trim().to_string())
        .unwrap_or_default();
    if mime.is_empty() {
        return Vec::new();
    }
    let mut dirs: Vec<PathBuf> = Vec::new();
    if let Some(home) = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| home_dir().map(|h| h.join(".local/share")))
    {
        dirs.push(home.join("applications"));
    }
    let system = std::env::var("XDG_DATA_DIRS")
        .ok()
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| "/usr/local/share:/usr/share".to_string());
    dirs.extend(
        system
            .split(':')
            .map(|d| PathBuf::from(d).join("applications")),
    );
    let mut seen = HashSet::new();
    let mut apps = Vec::new();
    for dir in dirs {
        let Ok(items) = fs::read_dir(&dir) else {
            continue;
        };
        for item in items.flatten() {
            let file = item.path();
            if file.extension().and_then(|e| e.to_str()) != Some("desktop") {
                continue;
            }
            let id = item.file_name();
            if !seen.insert(id) {
                continue;
            }
            let Ok(text) = fs::read_to_string(&file) else {
                continue;
            };
            if let Some(app) = desktop_app(&text, &mime) {
                apps.push(app);
            }
        }
    }
    apps.sort_by_key(|a| a.name.to_lowercase());
    apps.dedup_by(|a, b| a.name == b.name && a.exec == b.exec);
    apps
}

#[cfg(not(target_os = "linux"))]
pub(crate) fn apps_for(_path: &Path) -> Vec<DesktopApp> {
    Vec::new()
}

/// The entry of a .desktop file if it is shown and takes the MIME type.
pub(crate) fn desktop_app(text: &str, mime: &str) -> Option<DesktopApp> {
    let mut name = None;
    let mut exec = None;
    let mut terminal = false;
    let mut takes = false;
    let mut in_entry = false;
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_entry = line == "[Desktop Entry]";
            continue;
        }
        if !in_entry {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        match key.trim() {
            "Name" => name = Some(value.trim().to_string()),
            "Exec" => exec = Some(value.trim().to_string()),
            "Terminal" => terminal = value.trim() == "true",
            "NoDisplay" | "Hidden" if value.trim() == "true" => return None,
            "Type" if value.trim() != "Application" => return None,
            "MimeType" => {
                let family = mime.split_once('/').map(|(f, _)| format!("{f}/*"));
                takes = value
                    .split(';')
                    .map(str::trim)
                    .any(|m| m == mime || family.as_deref() == Some(m));
            }
            _ => {}
        }
    }
    if !takes {
        return None;
    }
    Some(DesktopApp {
        name: name?,
        exec: exec?,
        terminal,
    })
}

#[cfg(windows)]
fn open_with_chooser(path: &Path) -> Result<(), String> {
    std::process::Command::new("rundll32.exe")
        .arg("shell32.dll,OpenAs_RunDLL")
        .arg(path)
        .spawn()
        .map(|_| ())
        .map_err(|e| e.to_string())
}

#[cfg(not(windows))]
fn open_with_chooser(_path: &Path) -> Result<(), String> {
    Err(String::new())
}

/// "2026-10-16T19:27:00" for a Unix time, the shape .trashinfo wants.
fn iso_utc(secs: i64) -> String {
    let days = secs.div_euclid(86_400);
    let rest = secs.rem_euclid(86_400);
    // Civil-from-days (Howard Hinnant).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}",
        rest / 3600,
        rest % 3600 / 60,
        rest % 60
    )
}
