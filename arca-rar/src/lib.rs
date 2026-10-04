//! RAR support: reading any family, and creating new single-volume RAR5
//! archives. Existing archives are never modified.

use arca_core::{Entry, Error, Level, Result};
use std::path::{Path, PathBuf};

#[cfg(feature = "rar")]
mod reader;
#[cfg(feature = "rar")]
mod volumes;
#[cfg(feature = "rar")]
mod writer;

/// Ceilings enforced by [`create_rar`], chosen to match what the reader
/// accepts so every archive Arca writes is one Arca can open again.
/// They describe the API, not the build, so clients can show them even
/// when the `rar` feature is off.
pub mod create_limits {
    const MIB: u64 = 1024 * 1024;
    /// Members per archive, the same as the reader's header-count limit.
    pub const MAX_ENTRIES: usize = 100_000;
    /// UTF-8 bytes in one member name, after backslashes become slashes.
    pub const MAX_NAME_BYTES: usize = arca_core::limits::MAX_NAME;
    /// Bytes in one source file.
    pub const MAX_MEMBER_BYTES: u64 = 4 * 1024 * MIB;
    /// Bytes across all source files.
    pub const MAX_TOTAL_BYTES: u64 = 16 * 1024 * MIB;
    /// Estimated header bytes: names plus a fixed allowance per member.
    pub const MAX_HEADER_BYTES: u64 = 64 * MIB;
    /// Managed writer memory: compression workspace, retained headers and
    /// spool bookkeeping. Not a process-RAM limit; see `rars::WriterResources`.
    pub const WRITER_MEMORY_BYTES: u64 = 256 * MIB;
    /// Logical bytes the writer may spool to disk beside the output.
    pub const MAX_SPOOL_BYTES: u64 = 16 * 1024 * MIB;
}

pub type Progress<'a> = dyn Fn(usize, usize, &str) -> bool + Sync + 'a;

#[derive(Clone, Copy, Debug)]
pub enum Conflict {
    Overwrite,
    Skip,
    Rename,
    Cancel,
}

pub struct RarArchive {
    entries: Vec<Entry>,
    #[cfg(feature = "rar")]
    inner: volumes::Volumes,
}

impl RarArchive {
    /// Opens a standalone archive or a complete set from any volume. Siblings
    /// are resolved only in the selected directory when the selected archive's
    /// metadata requires volumes. Numbered and legacy sets resolve to volume one.
    ///
    /// Available volume numbers/flags and split metadata are checked; checksums
    /// are verified while decoding. RAR has no universal set identifier, so
    /// unrelated unsplit volumes with matching metadata cannot always be
    /// distinguished. Older families also lack reliable volume ordinals.
    pub fn open(path: &Path, password: Option<&str>) -> Result<Self> {
        Self::open_with_progress(path, password, &|_, _, _| true)
    }

    pub fn open_with_progress(
        path: &Path,
        password: Option<&str>,
        progress: &Progress<'_>,
    ) -> Result<Self> {
        #[cfg(feature = "rar")]
        return reader::open(path, password, progress);
        #[cfg(not(feature = "rar"))]
        {
            let _ = (path, password, progress);
            Err(disabled())
        }
    }

    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    pub fn test(&self, password: Option<&str>, progress: &Progress<'_>) -> Result<()> {
        #[cfg(feature = "rar")]
        return reader::test(self, password, progress);
        #[cfg(not(feature = "rar"))]
        {
            let _ = (password, progress);
            Err(disabled())
        }
    }

    pub fn read_entry(&self, index: usize, password: Option<&str>) -> Result<Vec<u8>> {
        #[cfg(feature = "rar")]
        return reader::read_entry(self, index, password);
        #[cfg(not(feature = "rar"))]
        {
            let _ = (index, password);
            Err(disabled())
        }
    }

    /// Verifies the entire stream in private staging before touching destinations.
    /// Publication errors can leave earlier verified files in place; it is not a
    /// transaction over the destination directory. Solid predecessors are decoded
    /// and count against limits even when they are not selected.
    pub fn extract(
        &self,
        dest: &Path,
        wanted: &[bool],
        password: Option<&str>,
        progress: &Progress<'_>,
        conflict: &dyn Fn(&Path) -> Conflict,
    ) -> Result<u64> {
        #[cfg(feature = "rar")]
        return reader::extract(self, dest, wanted, password, progress, conflict);
        #[cfg(not(feature = "rar"))]
        {
            let _ = (dest, wanted, password, progress, conflict);
            Err(disabled())
        }
    }
}

/// The error for modifying an existing RAR archive, which stays unsupported.
pub fn read_only() -> Error {
    arca_core::Format::Rar.read_only()
}

/// One member of a new archive: the file or directory to read and the name
/// it gets inside the archive. Directories are stored as single empty entries;
/// walking a tree is the caller's job, as it is for the other writers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Source {
    pub path: PathBuf,
    pub name: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CreateOptions {
    pub level: Level,
}

impl Default for CreateOptions {
    fn default() -> Self {
        Self {
            level: Level::Normal,
        }
    }
}

/// Creates a new single-volume, non-solid, unencrypted RAR5 archive at
/// `output` from `sources`, in the order given.
///
/// Policy:
/// - `output` must not exist, not even as a dangling link; nothing is ever
///   replaced. Its parent directory must exist. A `.cbr` output is refused.
/// - Names use `/` (backslashes are converted), must be relative, free of
///   `..`, drive letters, control characters, `:<>"|?*`, Windows device names
///   and trailing dots or spaces, and unique without regard to case. A file
///   may not share a name with a directory implied by another member.
/// - Sources must be regular files or directories; symbolic links and special
///   files are refused rather than followed. Sizes are checked against
///   [`create_limits`] up front and again while reading: a source that changes
///   size, modification time or identity during the write fails the write.
/// - Everything is written to a uniquely named `.arca-*.rar.part` file in the
///   output directory, with any spooling beside it, then reopened with the
///   reader and fully decoded. Only an archive that passes is published, via a
///   hard link so the name appears complete or not at all. On filesystems
///   without hard links the name is reserved exclusively and the staged file
///   is renamed over the reservation. Cancellation, write errors and failed
///   verification leave no file behind at `output` or in its directory.
///
/// `progress` receives `(members done, members total, member name)` during
/// the source scan, the write (including compression progress inside large
/// members) and the verification pass; returning `false` cancels and yields
/// [`Error::Cancelled`]. The last call precedes publication, so a `false`
/// there still leaves nothing behind; nothing is reported once the archive
/// has its name.
pub fn create_rar(
    output: &Path,
    sources: &[Source],
    options: &CreateOptions,
    progress: &Progress<'_>,
) -> Result<()> {
    #[cfg(feature = "rar")]
    return writer::create(output, sources, options, progress);
    #[cfg(not(feature = "rar"))]
    {
        let _ = (output, sources, options, progress);
        Err(Error::Unsupported(
            "RAR creation is disabled in this build; rebuild with --features rar".into(),
        ))
    }
}

#[cfg(not(feature = "rar"))]
fn disabled() -> Error {
    Error::Unsupported("RAR reading is disabled in this build; rebuild with --features rar".into())
}

#[cfg(all(test, not(feature = "rar")))]
mod tests {
    use super::*;

    #[test]
    fn disabled_reader_never_opens_the_file() {
        assert!(
            matches!(RarArchive::open(Path::new("not-present.rar"), None), Err(Error::Unsupported(message)) if message.contains("--features rar"))
        );
    }

    #[test]
    fn disabled_writer_touches_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let output = dir.path().join("new.rar");
        let sources = [Source {
            path: PathBuf::from("not-present.txt"),
            name: "not-present.txt".into(),
        }];
        let result = create_rar(&output, &sources, &CreateOptions::default(), &|_, _, _| {
            true
        });
        assert!(
            matches!(result, Err(Error::Unsupported(message)) if message.contains("RAR creation") && message.contains("--features rar"))
        );
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
    }
}
