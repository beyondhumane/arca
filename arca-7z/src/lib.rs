#![forbid(unsafe_code)]

mod read;
mod write;

pub use read::{Phase, Progress, SevenZArchive};
pub use write::{create_7z, CreateOptions, Source};

use arca_core::{Error, Result};

pub fn password_required(error: &Error) -> bool {
    matches!(error, Error::PasswordRequired | Error::PasswordOrCorrupt)
}

fn upstream(error: sevenz_rust2::Error, encrypted: bool) -> Error {
    use sevenz_rust2::Error as E;
    match error {
        E::PasswordRequired => Error::PasswordRequired,
        E::MaybeBadPassword(_) => Error::PasswordOrCorrupt,
        E::Unsupported(s) => Error::Unsupported(s.to_string()),
        E::UnsupportedCompressionMethod(s) => Error::Unsupported(s),
        E::MaxMemLimited { .. } => Error::Limit("7z decoder memory".into()),
        _ if encrypted => Error::PasswordOrCorrupt,
        _ => Error::Format(format!("7z: {error}")),
    }
}

fn entry_path(name: &str) -> Result<std::path::PathBuf> {
    if name.starts_with(['/', '\\']) {
        return Err(Error::Format("absolute 7z entry path".into()));
    }
    let path = arca_core::safe_name(name)?;
    for part in name.split(['/', '\\']) {
        let stem = part.split('.').next().unwrap_or("").to_ascii_uppercase();
        if part.contains(':')
            || part.ends_with([' ', '.']) && part != "."
            || matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
            || (stem.len() == 4
                && (stem.starts_with("COM") || stem.starts_with("LPT"))
                && matches!(stem.as_bytes()[3], b'1'..=b'9'))
        {
            return Err(Error::Format("non-portable 7z entry path".into()));
        }
    }
    Ok(path)
}
