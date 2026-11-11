//! Files on the system clipboard, in both directions.
//!
//! Copying names as text was never what Ctrl+C means in a file list: the
//! Explorer pastes files, not a list of words. What it actually wants is
//! CF_HDROP, a list of paths on disk, so the entries have to exist as real
//! files before the clipboard can carry them. That is why every copy from
//! here extracts first, into a folder under the temporary directory, and puts
//! *those* paths on the clipboard.
//!
//! Alongside CF_HDROP goes "Preferred DropEffect", a private format the shell
//! reads to tell a cut from a copy. Without it a cut still pastes, but the
//! Explorer treats it as a copy and leaves the temporary file behind.
//!
//! On Linux the same idea is text/uri-list on the CLIPBOARD selection, which
//! every file manager reads and arboard already speaks on X11 (and on Wayland
//! through XWayland). What Linux lacks is one agreed way of saying "cut": GNOME
//! and Thunar have theirs, KDE another, and arboard writes neither, so to the
//! other programs a cut here is a copy. Within the window a cut still moves,
//! because the window remembers what it cut. Other platforms get a stub, and
//! the window turns the menu entries off.

use std::path::PathBuf;

/// Whether this build can put files on the clipboard at all. The menu asks so
/// it can leave the entries out rather than offer something that only fails.
pub const AVAILABLE: bool = cfg!(any(windows, target_os = "linux"));

/// Whether other programs can tell a cut from a copy. Cutting out of an archive
/// only takes the entries out once whoever pastes has moved the files, so
/// where nobody can be told to move them there is no cut to offer.
pub const CUTS: bool = cfg!(windows);

/// A path the way the shell reads one. `fs::canonicalize` hands back paths
/// with the `\\?\` prefix on Windows, and the Explorer takes a CF_HDROP made of
/// those as a list with nothing in it: Ctrl+V does nothing at all.
pub(crate) fn shell_path(path: PathBuf) -> PathBuf {
    let Some(text) = path.to_str() else {
        return path;
    };
    if let Some(share) = text.strip_prefix(r"\\?\UNC\") {
        PathBuf::from(format!(r"\\{share}"))
    } else if let Some(local) = text.strip_prefix(r"\\?\") {
        PathBuf::from(local)
    } else {
        path
    }
}

#[cfg(windows)]
pub fn set_files(paths: &[PathBuf], cut: bool) -> Result<(), String> {
    use clipboard_win::{formats, raw, Clipboard, Setter};

    // DROPEFFECT_COPY and DROPEFFECT_MOVE, out of the shell's own header.
    const COPY: u32 = 1;
    const MOVE: u32 = 2;

    let list: Vec<String> = paths
        .iter()
        .map(|p| shell_path(p.clone()).display().to_string())
        .collect();
    if list.is_empty() {
        return Err("nothing to copy".into());
    }
    // The clipboard is one global lock: another program may be holding it for
    // the moment it takes to read something. Retrying is what every clipboard
    // library does, and is cheaper than telling the user to press it again.
    let _clip = Clipboard::new_attempts(10).map_err(|e| e.to_string())?;
    raw::empty().map_err(|e| e.to_string())?;
    formats::FileList
        .write_clipboard(&list)
        .map_err(|e| e.to_string())?;
    // After the list, and without clearing: this is a second format on the
    // same clipboard contents, not a replacement for them.
    if let Some(fmt) = raw::register_format("Preferred DropEffect") {
        let effect = if cut { MOVE } else { COPY };
        raw::set_without_clear(fmt.get(), &effect.to_le_bytes()).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// The files the clipboard is carrying right now, if it is carrying any.
#[cfg(windows)]
pub fn files() -> Vec<PathBuf> {
    taken().0
}

/// The files on the clipboard, and whether whoever put them there cut them.
///
/// The Explorer says which in "Preferred DropEffect": a cut is a move, and
/// pasting it has to take the files away from where they were.
#[cfg(windows)]
pub fn taken() -> (Vec<PathBuf>, bool) {
    use clipboard_win::{formats, raw, Clipboard, Getter};

    const MOVE: u32 = 2;
    let Ok(_clip) = Clipboard::new_attempts(10) else {
        return (Vec::new(), false);
    };
    let mut out: Vec<String> = Vec::new();
    if formats::FileList.read_clipboard(&mut out).is_err() {
        return (Vec::new(), false);
    }
    let mut effect = Vec::new();
    let cut = raw::register_format("Preferred DropEffect")
        .and_then(|fmt| raw::get_vec(fmt.get(), &mut effect).ok())
        .and_then(|_| effect.get(..4))
        .map(|bytes| u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
        .is_some_and(|effect| effect & MOVE != 0);
    (out.into_iter().map(PathBuf::from).collect(), cut)
}

/// Whether there are files to paste, asked without opening the clipboard, so
/// a menu can ask it every time it is drawn.
#[cfg(windows)]
pub fn has_files() -> bool {
    clipboard_win::raw::is_format_avail(clipboard_win::formats::CF_HDROP)
}

/// A number that changes every time anything is put on the clipboard, by this
/// program or any other.
#[cfg(windows)]
pub fn sequence() -> Option<u32> {
    clipboard_win::raw::seq_num().map(|n| n.get())
}

/// Empties the clipboard, the way the Explorer does once a cut has been pasted:
/// the files have moved, and the paths it held lead nowhere.
#[cfg(windows)]
pub fn clear() {
    if let Ok(_clip) = clipboard_win::Clipboard::new_attempts(10) {
        let _ = clipboard_win::raw::empty();
    }
}

#[cfg(target_os = "linux")]
mod linux {
    use super::PathBuf;
    use std::sync::{Mutex, OnceLock};

    // On X11 the program that set the clipboard is the one that hands it out
    // when somebody pastes, so the handle that set it has to outlive the call.
    fn with<T>(use_it: impl FnOnce(&mut arboard::Clipboard) -> T) -> Result<T, String> {
        static HANDLE: OnceLock<Mutex<Option<arboard::Clipboard>>> = OnceLock::new();
        let mut held = HANDLE
            .get_or_init(|| Mutex::new(None))
            .lock()
            .map_err(|e| e.to_string())?;
        if held.is_none() {
            *held = Some(arboard::Clipboard::new().map_err(|e| e.to_string())?);
        }
        match held.as_mut() {
            Some(clipboard) => Ok(use_it(clipboard)),
            None => Err("no clipboard".into()),
        }
    }

    // What this window last cut, since the clipboard itself cannot say.
    fn last_cut() -> &'static Mutex<Option<Vec<PathBuf>>> {
        static CUT: OnceLock<Mutex<Option<Vec<PathBuf>>>> = OnceLock::new();
        CUT.get_or_init(|| Mutex::new(None))
    }

    pub fn set_files(paths: &[PathBuf], cut: bool) -> Result<(), String> {
        if paths.is_empty() {
            return Err("nothing to copy".into());
        }
        with(|clipboard| clipboard.set().file_list(paths).map_err(|e| e.to_string()))??;
        if let Ok(mut last) = last_cut().lock() {
            *last = cut.then(|| paths.to_vec());
        }
        Ok(())
    }

    pub fn files() -> Vec<PathBuf> {
        with(|clipboard| clipboard.get().file_list().unwrap_or_default()).unwrap_or_default()
    }

    pub fn taken() -> (Vec<PathBuf>, bool) {
        let files = files();
        let cut = !files.is_empty()
            && last_cut()
                .lock()
                .is_ok_and(|last| last.as_ref() == Some(&files));
        (files, cut)
    }

    pub fn has_files() -> bool {
        !files().is_empty()
    }

    pub fn sequence() -> Option<u32> {
        None
    }

    pub fn clear() {
        let _ = with(|clipboard| clipboard.clear());
        if let Ok(mut last) = last_cut().lock() {
            *last = None;
        }
    }
}

#[cfg(target_os = "linux")]
pub use linux::*;

#[cfg(not(any(windows, target_os = "linux")))]
pub fn set_files(_paths: &[PathBuf], _cut: bool) -> Result<(), String> {
    Err("putting files on the clipboard is not available on this system yet".into())
}

#[cfg(not(any(windows, target_os = "linux")))]
pub fn files() -> Vec<PathBuf> {
    Vec::new()
}

#[cfg(not(any(windows, target_os = "linux")))]
pub fn taken() -> (Vec<PathBuf>, bool) {
    (Vec::new(), false)
}

#[cfg(not(any(windows, target_os = "linux")))]
pub fn has_files() -> bool {
    false
}

#[cfg(not(any(windows, target_os = "linux")))]
pub fn sequence() -> Option<u32> {
    None
}

#[cfg(not(any(windows, target_os = "linux")))]
pub fn clear() {}

#[cfg(test)]
mod tests {
    use super::shell_path;
    use std::path::PathBuf;

    #[test]
    fn the_shell_gets_paths_without_the_verbatim_prefix() {
        let plain = |p: &str| shell_path(PathBuf::from(p));
        assert_eq!(
            plain(r"\\?\C:\Temp\Arca\a.txt"),
            PathBuf::from(r"C:\Temp\Arca\a.txt")
        );
        assert_eq!(
            plain(r"\\?\UNC\server\share\a"),
            PathBuf::from(r"\\server\share\a")
        );
        assert_eq!(plain(r"C:\Temp\a.txt"), PathBuf::from(r"C:\Temp\a.txt"));
        assert_eq!(plain("/tmp/a.txt"), PathBuf::from("/tmp/a.txt"));
    }
}
