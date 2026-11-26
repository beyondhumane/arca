//! Telling Windows that something it keeps in memory has changed.
//!
//! Writing `PATH` or a file association into the registry is not enough: the
//! Explorer reads both once and keeps them, so a terminal opened a second later
//! still would not find `arca`, and the "Open with" list would still be the old
//! one. Both need a broadcast, and a broadcast is a raw Win32 call, which is why
//! this is a crate: `arca-setup` forbids unsafe code, and this is two unsafe
//! calls and nothing else.
//!
//! On anything that is not Windows these functions do nothing.

#[cfg(windows)]
mod win;

#[cfg(windows)]
pub use win::{associations_changed, environment_changed};

#[cfg(not(windows))]
pub fn environment_changed() {}

#[cfg(not(windows))]
pub fn associations_changed() {}
