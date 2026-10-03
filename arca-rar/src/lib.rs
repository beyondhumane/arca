//! Experimental, opt-in RAR reading. No archive-writing API is exposed.

use arca_core::{Entry, Error, Result};
use std::path::Path;

#[cfg(feature = "rar")]
mod reader;
#[cfg(feature = "rar")]
mod volumes;

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

pub fn read_only() -> Error {
    Error::Unsupported("RAR is read-only; creating or modifying RAR archives is disabled".into())
}

#[cfg(not(feature = "rar"))]
fn disabled() -> Error {
    Error::Unsupported("experimental RAR reading is disabled; rebuild with --features rar".into())
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
}
