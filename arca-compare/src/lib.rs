//! Checksums of the entries of any archive Arca reads, worked out by
//! streaming them through the hasher instead of extracting them.

mod digest;

pub use digest::{digest_entries, digest_file, tar_source, Digest};

/// Told how far along the work is, `(done, total, name)`, and asked whether
/// to carry on. A zero total means the end is not known yet.
pub type Notify<'a> = &'a (dyn Fn(usize, usize, &str) -> bool + Sync);
