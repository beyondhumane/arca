//! Experimental, opt-in RAR reading. No archive-writing API is exposed.

use arca_core::{Entry, Error, Result};
use std::path::Path;

#[cfg(feature = "rar")]
mod reader;

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
    inner: rars::Archive,
}

impl RarArchive {
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
    arca_core::Format::Rar.read_only()
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
