#[cfg(windows)]
mod install;
mod payload;
#[cfg(windows)]
mod system;
#[cfg(windows)]
mod uninstall;

use std::path::{Path, PathBuf};

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const SHELL_DLL: &str = "arca_shell.dll";
pub const GUI_EXE: &str = "arca-gui.exe";
pub const UNINSTALLER: &str = "unins000.exe";
pub const MANIFEST: &str = "AppxManifest.xml";
pub const INNO_LEFTOVERS: [&str; 2] = ["unins000.dat", "unins000.msg"];

pub const KNOWN_FILES: [&str; 9] = [
    "arca.exe",
    "arca-gui.exe",
    "arca_shell.dll",
    "AppxManifest.xml",
    "LICENSE",
    "README.md",
    "Assets/Square150x150Logo.png",
    "Assets/Square44x44Logo.png",
    "Assets/StoreLogo.png",
];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mode {
    Install,
    Uninstall,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Choices {
    pub menu: bool,
    pub assoc: bool,
    pub path: bool,
}

impl Default for Choices {
    fn default() -> Self {
        Choices {
            menu: true,
            assoc: true,
            path: true,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Job {
    pub mode: Mode,
    pub dir: PathBuf,
    pub choices: Choices,
    pub quiet_update: bool,
    pub files_only: bool,
}

#[derive(Clone, PartialEq, Debug)]
pub enum Event {
    Progress(f32),
    Finished,
    Failed(String),
}

pub fn overall(step: usize, fraction: f32) -> f32 {
    25. * (step as f32 + fraction.clamp(0., 1.))
}

pub fn run(job: &Job, report: &mut dyn FnMut(Event)) -> Result<(), String> {
    #[cfg(windows)]
    {
        match job.mode {
            Mode::Install => install::run(job, report),
            Mode::Uninstall => uninstall::run(job, report),
        }
    }
    #[cfg(not(windows))]
    {
        let _ = (job, report);
        Err("the installer only works on Windows".to_string())
    }
}

pub fn default_location() -> PathBuf {
    #[cfg(windows)]
    {
        system::existing_location().unwrap_or_else(system::default_location)
    }
    #[cfg(not(windows))]
    {
        PathBuf::from("Arca")
    }
}

pub fn initial_choices(dir: &Path) -> Choices {
    #[cfg(windows)]
    {
        if system::existing_location().is_some() {
            system::installed_choices(dir)
        } else {
            Choices::default()
        }
    }
    #[cfg(not(windows))]
    {
        let _ = dir;
        Choices::default()
    }
}

pub fn uninstall_location() -> PathBuf {
    let beside = std::env::current_exe().ok().and_then(|exe| {
        let named = exe
            .file_name()
            .is_some_and(|name| name.to_string_lossy().eq_ignore_ascii_case(UNINSTALLER));
        if named {
            exe.parent().map(PathBuf::from)
        } else {
            None
        }
    });
    beside.unwrap_or_else(default_location)
}

pub fn relaunch_for_uninstall(args: &[String]) -> Result<(), String> {
    #[cfg(windows)]
    {
        system::relaunch_from_temp(args)
    }
    #[cfg(not(windows))]
    {
        let _ = args;
        Err("the installer only works on Windows".to_string())
    }
}

pub fn launch_app(dir: &Path) {
    #[cfg(windows)]
    system::launch(&dir.join(GUI_EXE), dir);
    #[cfg(not(windows))]
    let _ = dir;
}

pub fn path_with(current: &str, dir: &str) -> String {
    if path_has(current, dir) {
        return current.to_string();
    }
    let trimmed = current.trim_matches(';');
    if trimmed.is_empty() {
        dir.to_string()
    } else {
        format!("{trimmed};{dir}")
    }
}

pub fn path_without(current: &str, dir: &str) -> String {
    current
        .split(';')
        .filter(|part| !part.is_empty() && !same_dir(part, dir))
        .collect::<Vec<_>>()
        .join(";")
}

pub fn path_has(current: &str, dir: &str) -> bool {
    current.split(';').any(|part| same_dir(part, dir))
}

fn same_dir(a: &str, b: &str) -> bool {
    let clean = |text: &str| text.trim().trim_end_matches(['\\', '/']).to_lowercase();
    let (a, b) = (clean(a), clean(b));
    !a.is_empty() && a == b
}

pub fn stamp_manifest(text: &str, version: &str) -> String {
    text.replace("Version=\"0.0.0.0\"", &format!("Version=\"{version}.0\""))
}

pub fn base64(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = (u32::from(chunk[0]) << 16)
            | (u32::from(*chunk.get(1).unwrap_or(&0)) << 8)
            | u32::from(*chunk.get(2).unwrap_or(&0));
        out.push(TABLE[(n >> 18) as usize & 63] as char);
        out.push(TABLE[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 {
            TABLE[(n >> 6) as usize & 63] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            TABLE[n as usize & 63] as char
        } else {
            '='
        });
    }
    out
}

pub fn encoded_command(script: &str) -> String {
    let utf16: Vec<u8> = script
        .encode_utf16()
        .flat_map(|unit| unit.to_le_bytes())
        .collect();
    base64(&utf16)
}

pub fn quote_ps(text: &str) -> String {
    format!("'{}'", text.replace('\'', "''"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_matches_the_reference_vectors() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foob"), "Zm9vYg==");
        assert_eq!(base64(b"fooba"), "Zm9vYmE=");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn a_powershell_script_travels_as_utf16() {
        assert_eq!(encoded_command("dir"), "ZABpAHIA");
    }

    #[test]
    fn single_quotes_are_doubled_for_powershell() {
        assert_eq!(quote_ps("C:\\it's"), "'C:\\it''s'");
    }

    #[test]
    fn the_directory_is_added_once() {
        let dir = "C:\\Users\\a\\Programs\\Arca";
        let added = path_with("C:\\Windows;C:\\Tools", dir);
        assert_eq!(added, "C:\\Windows;C:\\Tools;C:\\Users\\a\\Programs\\Arca");
        assert_eq!(path_with(&added, dir), added);
        assert_eq!(path_with(&added, &dir.to_uppercase()), added);
        assert_eq!(path_with(&added, &format!("{dir}\\")), added);
    }

    #[test]
    fn an_empty_path_becomes_the_directory() {
        assert_eq!(path_with("", "C:\\Arca"), "C:\\Arca");
        assert_eq!(path_with(";;", "C:\\Arca"), "C:\\Arca");
    }

    #[test]
    fn removing_touches_only_that_directory() {
        let current = "C:\\Windows;C:\\Arca;C:\\ArcaOther;C:\\Tools";
        assert_eq!(
            path_without(current, "c:\\arca\\"),
            "C:\\Windows;C:\\ArcaOther;C:\\Tools"
        );
        assert_eq!(path_without("C:\\Arca", "C:\\Arca"), "");
        assert_eq!(path_without("C:\\Windows", "C:\\Arca"), "C:\\Windows");
    }

    #[test]
    fn a_prefix_of_another_directory_is_not_a_match() {
        assert!(!path_has("C:\\ArcaOther", "C:\\Arca"));
        assert!(path_has("C:\\x;C:\\Arca\\", "C:\\Arca"));
        assert!(!path_has("", "C:\\Arca"));
    }

    #[test]
    fn the_manifest_gets_the_release_version() {
        let text = "<Identity Name=\"x\" Version=\"0.0.0.0\"/>";
        assert_eq!(
            stamp_manifest(text, "0.7.1"),
            "<Identity Name=\"x\" Version=\"0.7.1.0\"/>"
        );
        assert_eq!(stamp_manifest("nothing here", "0.7.1"), "nothing here");
    }

    #[test]
    fn steps_split_the_bar_in_quarters() {
        assert_eq!(overall(0, 0.), 0.);
        assert_eq!(overall(1, 0.5), 37.5);
        assert_eq!(overall(3, 1.), 100.);
        assert_eq!(overall(3, 7.), 100.);
    }
}
