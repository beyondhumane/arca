use crate::{app::Screen, engine::Mode, i18n::Lang};
use std::path::PathBuf;

#[derive(Debug, PartialEq)]
pub struct Options {
    pub mode: Mode,
    pub silent: bool,
    pub update: bool,
    pub relaunched: bool,
    pub preview: bool,
    pub files_only: bool,
    pub dir: Option<PathBuf>,
    pub lang: Option<Lang>,
    pub screen: Option<Screen>,
    pub percent: Option<f32>,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            mode: Mode::Install,
            silent: false,
            update: false,
            relaunched: false,
            preview: false,
            files_only: false,
            dir: None,
            lang: None,
            screen: None,
            percent: None,
        }
    }
}

pub fn parse(args: impl Iterator<Item = String>) -> Options {
    let mut options = Options::default();
    for arg in args {
        let Some(flag) = arg.strip_prefix("--").or_else(|| arg.strip_prefix('/')) else {
            continue;
        };
        let (name, value) = match flag.split_once('=') {
            Some((name, value)) => (name, Some(value)),
            None => (flag, None),
        };
        match (name.to_ascii_lowercase().as_str(), value) {
            ("uninstall", _) => options.mode = Mode::Uninstall,
            ("verysilent" | "silent", _) => options.silent = true,
            ("update", value) => options.update = value.is_some_and(|v| v != "0"),
            ("relaunched", _) => options.relaunched = true,
            ("preview", _) => options.preview = true,
            ("files-only", _) => options.files_only = true,
            ("dir", Some(value)) if !value.is_empty() => {
                options.dir = Some(PathBuf::from(value.trim_matches('"')));
            }
            ("lang", Some(code)) => options.lang = Lang::from_code(code),
            ("screen", Some(name)) => options.screen = Screen::from_name(name),
            ("percent", Some(value)) => {
                options.percent = value.parse::<f32>().ok().map(|p| p.clamp(0., 100.));
            }
            _ => {}
        }
    }
    options.preview |= options.screen.is_some() || options.percent.is_some();
    options
}

#[cfg(test)]
mod tests {
    use super::*;

    fn options(list: &[&str]) -> Options {
        parse(list.iter().map(|arg| arg.to_string()))
    }

    #[test]
    fn the_updater_command_line_is_understood() {
        let parsed = options(&[
            "/VERYSILENT",
            "/NOCANCEL",
            "/NORESTART",
            "/NORESTARTAPPLICATIONS",
            "/update=1",
        ]);
        assert!(parsed.silent);
        assert!(parsed.update);
        assert_eq!(parsed.mode, Mode::Install);
        assert!(!parsed.preview);
    }

    #[test]
    fn flags_are_read_with_either_prefix_and_any_case() {
        assert!(options(&["--verysilent"]).silent);
        assert!(options(&["/SILENT"]).silent);
        assert_eq!(options(&["--UNINSTALL"]).mode, Mode::Uninstall);
        assert!(!options(&["/update=0"]).update);
        assert!(!options(&["/update"]).update);
    }

    #[test]
    fn the_folder_is_read_with_or_without_quotes() {
        assert_eq!(
            options(&["/DIR=C:\\Tools\\Arca"]).dir,
            Some(PathBuf::from("C:\\Tools\\Arca"))
        );
        assert_eq!(
            options(&["/DIR=\"C:\\Program Files\\Arca\""]).dir,
            Some(PathBuf::from("C:\\Program Files\\Arca"))
        );
        assert_eq!(options(&["/DIR="]).dir, None);
    }

    #[test]
    fn a_screen_or_a_percent_turns_on_the_preview() {
        let parsed = options(&["--screen=installing", "--percent=68", "--lang=es"]);
        assert_eq!(parsed.screen, Some(Screen::Installing));
        assert_eq!(parsed.percent, Some(68.));
        assert_eq!(parsed.lang, Some(Lang::Es));
        assert!(parsed.preview);
        assert!(options(&["--percent=5"]).preview);
        assert!(!options(&[]).preview);
    }

    #[test]
    fn nonsense_is_ignored() {
        let parsed = options(&[
            "--screen=nowhere",
            "--percent=lots",
            "--lang=xx",
            "stray",
            "/",
        ]);
        assert_eq!(parsed, Options::default());
    }

    #[test]
    fn the_percent_is_held_between_zero_and_a_hundred() {
        assert_eq!(options(&["--percent=250"]).percent, Some(100.));
        assert_eq!(options(&["--percent=-4"]).percent, Some(0.));
    }
}
