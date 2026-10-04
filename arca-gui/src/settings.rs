//! Persistent user preferences for the Arca GUI.

use crate::i18n::Lang;
use crate::{
    Columns, ThemePreference, CELL_LEAST, CELL_WIDE, NAME_LEAST, NAME_WIDE, SIDEBAR_LEAST,
    SIDEBAR_MOST, SIDEBAR_WIDE,
};
use std::fs;
use std::path::PathBuf;

const PREVIEW_WIDE: f32 = 440.0;
const PREVIEW_LEAST: f32 = 280.0;
const PREVIEW_MOST: f32 = 800.0;
const DIRECTORY_WIDE: f32 = 280.0;
const DIRECTORY_LEAST: f32 = 220.0;
const DIRECTORY_MOST: f32 = 560.0;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum BrowserView {
    #[default]
    Columns,
    Details,
}

fn bounded_width(width: f32, least: f32, most: f32) -> Option<f32> {
    width.is_finite().then(|| width.clamp(least, most))
}

pub(crate) fn config_file() -> Option<PathBuf> {
    let base = if cfg!(windows) {
        std::env::var_os("APPDATA").map(PathBuf::from)
    } else if cfg!(target_os = "macos") {
        std::env::var_os("HOME").map(|h| PathBuf::from(h).join("Library/Application Support"))
    } else {
        std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
    }?;
    Some(base.join("Arca").join("gui.conf"))
}

pub(crate) struct Settings {
    pub(crate) lang: Option<Lang>,
    pub(crate) theme: ThemePreference,
    pub(crate) columns: Columns,
    pub(crate) browser_view: BrowserView,
    pub(crate) sidebar_collapsed: bool,
    pub(crate) preview_visible: bool,
    pub(crate) preview_width: f32,
    pub(crate) directory_width: f32,
    // Whether to ask, once at startup, if there is a newer Arca.
    pub(crate) updates: bool,
    // Every file in the archive at once, instead of one folder at a time.
    pub(crate) flat: bool,
    // El panel de carpetas del archivo, a la izquierda.
    //
    // La clave se llama `folders` y no `tree` a proposito. La vieja existe en
    // los ajustes de todo el mundo puesta a `no`, porque asi venia de fabrica
    // y durante una version entera no la leyo nadie: honrarla ahora escondería
    // el panel a quien nunca pidio esconderlo. Con otro nombre, la vieja se
    // ignora y el panel sigue saliendo.
    pub(crate) folders: bool,
    // Which code page an unflagged zip has its names written in. Only the
    // person looking at the archive can know, so it is remembered: somebody
    // whose archives all come from one machine says it once.
    pub(crate) page: arca_zip::pages::Page,
    // Where the window was left and how big: x, y, width, height. None until it
    // has been opened once.
    pub(crate) window: Option<[f32; 4]>,
    // The archives opened lately, newest first. Paths, so that one that has
    // since been moved can be noticed and dropped rather than opened blind.
    pub(crate) recent: Vec<String>,
    pub(crate) last_folder: Option<String>,
    pub(crate) pinned: Vec<String>,
    pub(crate) show_hidden: bool,
    // How wide each column is: the name first, then the ones that can be
    // turned off, in the order of `Columns::ALL`. A column keeps its width
    // while it is off, so turning one back on does not lose how it was set.
    pub(crate) widths: Vec<f32>,
    // How wide the folder sidebar was left, kept within its pull range in case
    // the file was written by hand.
    pub(crate) sidebar: f32,
}

impl Settings {
    // What a column starts out at, before anybody has pulled on it.
    pub(crate) fn default_widths() -> Vec<f32> {
        std::iter::once(NAME_WIDE)
            .chain(std::iter::repeat_n(CELL_WIDE, Columns::ALL.len()))
            .collect()
    }

    // The least a column can be pulled down to, by its place in `widths`.
    pub(crate) fn least(slot: usize) -> f32 {
        if slot == 0 {
            NAME_LEAST
        } else {
            CELL_LEAST
        }
    }
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            lang: None,
            theme: ThemePreference::Dark,
            columns: Columns::default(),
            browser_view: BrowserView::default(),
            sidebar_collapsed: false,
            preview_visible: true,
            preview_width: PREVIEW_WIDE,
            directory_width: DIRECTORY_WIDE,
            updates: true,
            flat: false,
            folders: true,
            page: arca_zip::pages::Page::default(),
            window: None,
            recent: Vec::new(),
            last_folder: None,
            pinned: Vec::new(),
            show_hidden: false,
            widths: Settings::default_widths(),
            sidebar: SIDEBAR_WIDE.min(200.0).clamp(SIDEBAR_LEAST, SIDEBAR_MOST),
        }
    }
}

impl Settings {
    pub(crate) fn load() -> Self {
        let Some(p) = config_file() else {
            return Settings::default();
        };
        let Ok(text) = fs::read_to_string(p) else {
            return Settings::default();
        };
        Settings::parse(&text)
    }

    /// The settings file read back out of text.
    ///
    /// Apart from `load` so that what `text` writes can be read back and
    /// compared without going near a disk.
    pub(crate) fn parse(text: &str) -> Settings {
        let mut s = Settings::default();
        for line in text.lines() {
            let Some((k, v)) = line.split_once('=') else {
                continue;
            };
            match (k.trim(), v.trim()) {
                ("lang", "system") => s.lang = None,
                ("lang", other) => s.lang = Lang::from_code(other),
                ("theme", "light") => s.theme = ThemePreference::Light,
                ("theme", "dark") => s.theme = ThemePreference::Dark,
                ("theme", _) => s.theme = ThemePreference::System,
                ("browser_view", "columns") => s.browser_view = BrowserView::Columns,
                ("browser_view", "details") => s.browser_view = BrowserView::Details,
                ("sidebar_collapsed", "yes") => s.sidebar_collapsed = true,
                ("sidebar_collapsed", "no") => s.sidebar_collapsed = false,
                ("preview_visible", "yes") => s.preview_visible = true,
                ("preview_visible", "no") => s.preview_visible = false,
                ("preview_width", v) => {
                    if let Some(width) = v
                        .parse::<f32>()
                        .ok()
                        .and_then(|w| bounded_width(w, PREVIEW_LEAST, PREVIEW_MOST))
                    {
                        s.preview_width = width;
                    }
                }
                ("directory_width", v) => {
                    if let Some(width) = v
                        .parse::<f32>()
                        .ok()
                        .and_then(|w| bounded_width(w, DIRECTORY_LEAST, DIRECTORY_MOST))
                    {
                        s.directory_width = width;
                    }
                }
                ("flat", v) => s.flat = v == "yes",
                ("folders", v) => s.folders = v == "yes",
                ("updates", v) => s.updates = v != "no",
                ("page", v) => {
                    if let Some(p) = arca_zip::pages::Page::from_code(v) {
                        s.page = p;
                    }
                }
                // One line each, because a path can hold anything a filename
                // can and there is no separator left that it could not.
                ("recent", p) if !p.is_empty() => s.recent.push(p.to_string()),
                ("pinned", p) if !p.is_empty() => s.pinned.push(p.to_string()),
                ("last_folder", p) if !p.is_empty() => s.last_folder = Some(p.to_string()),
                ("show_hidden", v) => s.show_hidden = v == "yes",
                ("window", v) => {
                    let Some(n) = v
                        .split(',')
                        .map(|x| x.trim().parse::<f32>().ok().filter(|n| n.is_finite()))
                        .collect::<Option<Vec<_>>>()
                    else {
                        continue;
                    };
                    if let [x, y, w, h] = n[..] {
                        // A window smaller than the minimum, or one left on a
                        // screen that is no longer plugged in, is not a window
                        // anybody can use.
                        if w >= 720.0 && h >= 320.0 {
                            s.window = Some([x, y, w, h]);
                        }
                    }
                }
                // Written since the columns could first be turned off and read
                // by nobody, so every window opened with the six of them
                // showing however they had been left.
                ("columns", list) => {
                    let mut c = Columns::default();
                    for (which, _) in Columns::ALL {
                        c.set(which, false);
                    }
                    for name in list.split(',').map(str::trim) {
                        if let Some((which, _)) = Columns::ALL.iter().find(|(_, n)| *n == name) {
                            c.set(*which, true);
                        }
                    }
                    s.columns = c;
                }
                // All of them or none: a line with a column missing from it
                // belongs to a different set of columns than this one has, and
                // guessing which is which would put the widths on the wrong
                // ones. Each is held above its floor in case the file was
                // written by hand.
                ("widths", list) => {
                    let Some(read) = list
                        .split(',')
                        .map(|n| n.trim().parse::<f32>().ok().filter(|w| w.is_finite()))
                        .collect::<Option<Vec<_>>>()
                    else {
                        continue;
                    };
                    if read.len() == s.widths.len() {
                        s.widths = read
                            .into_iter()
                            .enumerate()
                            .map(|(i, w)| w.max(Settings::least(i)))
                            .collect();
                    }
                }
                ("sidebar", v) => {
                    if let Some(width) = v
                        .parse::<f32>()
                        .ok()
                        .and_then(|w| bounded_width(w, SIDEBAR_LEAST, SIDEBAR_MOST))
                    {
                        s.sidebar = width;
                    }
                }
                _ => {}
            }
        }
        // Flat browsing requires Details, regardless of persisted key order.
        if s.flat {
            s.browser_view = BrowserView::Details;
        }
        s
    }

    pub(crate) fn save(&self) {
        let Some(p) = config_file() else { return };
        if let Some(dir) = p.parent() {
            let _ = fs::create_dir_all(dir);
        }
        let _ = fs::write(p, self.text());
    }

    /// The settings file as text.
    ///
    /// Apart from `save` so that what is written can be read back and compared
    /// without going near a disk. That is not tidiness: six settings were being
    /// read at startup and never written, because the line that builds this had
    /// quietly stopped mentioning them -- `flat`, `tree`, `page`, `window` and
    /// `recent` were all built into a string that was then thrown away. A round
    /// trip that never touches a file is the only way that stays fixed.
    pub(crate) fn text(&self) -> String {
        let lang = self.lang.map(|l| l.code()).unwrap_or("system");
        let theme = match self.theme {
            ThemePreference::Light => "light",
            ThemePreference::Dark => "dark",
            ThemePreference::System => "system",
        };
        let yes = |b: bool| if b { "yes" } else { "no" };
        let browser_view = match (self.flat, self.browser_view) {
            (false, BrowserView::Columns) => "columns",
            _ => "details",
        };
        let preview_width =
            bounded_width(self.preview_width, PREVIEW_LEAST, PREVIEW_MOST).unwrap_or(PREVIEW_WIDE);
        let directory_width = bounded_width(self.directory_width, DIRECTORY_LEAST, DIRECTORY_MOST)
            .unwrap_or(DIRECTORY_WIDE);
        let columns: Vec<&str> = Columns::ALL
            .iter()
            .filter(|(which, _)| self.columns.on(*which))
            .map(|(_, name)| *name)
            .collect();
        let widths: Vec<String> = self.widths.iter().map(|w| format!("{w:.1}")).collect();

        let mut out = String::new();
        out.push_str(&format!("lang = {lang}\n"));
        out.push_str(&format!("theme = {theme}\n"));
        out.push_str(&format!("flat = {}\n", yes(self.flat)));
        out.push_str(&format!("browser_view = {browser_view}\n"));
        out.push_str(&format!("folders = {}\n", yes(self.folders)));
        out.push_str(&format!(
            "sidebar_collapsed = {}\n",
            yes(self.sidebar_collapsed)
        ));
        out.push_str(&format!(
            "preview_visible = {}\n",
            yes(self.preview_visible)
        ));
        out.push_str(&format!("preview_width = {preview_width:.1}\n"));
        out.push_str(&format!("directory_width = {directory_width:.1}\n"));
        out.push_str(&format!("updates = {}\n", yes(self.updates)));
        out.push_str(&format!("page = {}\n", self.page.code()));
        out.push_str(&format!("columns = {}\n", columns.join(",")));
        out.push_str(&format!("widths = {}\n", widths.join(",")));
        out.push_str(&format!("sidebar = {:.1}\n", self.sidebar));
        if let Some([x, y, w, h]) = self.window {
            out.push_str(&format!("window = {x:.0},{y:.0},{w:.0},{h:.0}\n"));
        }
        // One line each: a path can hold anything a filename can and there is
        // no separator left that it could not.
        for path in &self.recent {
            out.push_str(&format!("recent = {path}\n"));
        }
        for path in &self.pinned {
            out.push_str(&format!("pinned = {path}\n"));
        }
        if let Some(path) = &self.last_folder {
            out.push_str(&format!("last_folder = {path}\n"));
        }
        out.push_str(&format!("show_hidden = {}\n", yes(self.show_hidden)));
        out
    }

    pub(crate) fn effective_lang(&self) -> Lang {
        self.lang.unwrap_or_else(Lang::from_system)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_use_columns_and_visible_expanded_panels() {
        let s = Settings::parse("");
        assert_eq!(BrowserView::default(), BrowserView::Columns);
        assert_eq!(s.browser_view, BrowserView::Columns);
        assert!(!s.flat);
        assert!(s.folders);
        assert!(!s.sidebar_collapsed);
        assert_eq!(s.sidebar, 200.0);
        assert!(s.preview_visible);
        assert_eq!(s.preview_width, 440.0);
        assert_eq!(s.directory_width, 280.0);
        assert_eq!(s.text(), Settings::default().text());
    }

    #[test]
    fn legacy_files_preserve_every_existing_preference() {
        let text = "lang = es\ntheme = dark\nflat = yes\nfolders = no\nupdates = no\n\
            page = cp1252\ncolumns = size,crc,path\n\
            widths = 321,80,81,82,83,84,85,86,87,88,89,90\nsidebar = 311\n\
            window = -100,34,1000,700\n\
            recent = C:\\one.zip\nrecent = D:\\two, with=punctuation.zip\n";
        let s = Settings::parse(text);
        assert_eq!(s.lang, Some(Lang::Es));
        assert_eq!(s.theme, ThemePreference::Dark);
        assert!(s.flat);
        assert!(!s.folders);
        assert!(!s.updates);
        assert_eq!(s.page, arca_zip::pages::Page::Cp1252);
        for (which, name) in Columns::ALL {
            assert_eq!(s.columns.on(which), matches!(name, "size" | "crc" | "path"));
        }
        assert_eq!(
            s.widths,
            vec![321.0, 80.0, 81.0, 82.0, 83.0, 84.0, 85.0, 86.0, 87.0, 88.0, 89.0, 90.0]
        );
        assert_eq!(s.sidebar, 311.0);
        assert_eq!(s.window, Some([-100.0, 34.0, 1000.0, 700.0]));
        assert_eq!(s.recent, ["C:\\one.zip", "D:\\two, with=punctuation.zip"]);
        assert_eq!(s.browser_view, BrowserView::Details);
        assert!(!s.sidebar_collapsed);
        assert!(s.preview_visible);
        assert_eq!(s.preview_width, 440.0);
        assert_eq!(s.directory_width, 280.0);
        assert_eq!(Settings::parse(&s.text()).text(), s.text());
        assert_eq!(
            Settings::parse("flat = no").browser_view,
            BrowserView::Columns
        );
        assert!(Settings::parse("tree = no").folders);
    }

    #[test]
    fn layout_preferences_round_trip_independently() {
        for browser_view in [BrowserView::Columns, BrowserView::Details] {
            for flat in [false, true] {
                for folders in [false, true] {
                    for sidebar_collapsed in [false, true] {
                        for preview_visible in [false, true] {
                            let s = Settings {
                                browser_view,
                                flat,
                                folders,
                                sidebar_collapsed,
                                preview_visible,
                                sidebar: 257.5,
                                preview_width: 512.5,
                                directory_width: 297.5,
                                ..Settings::default()
                            };
                            let text = s.text();
                            let after = Settings::parse(&text);
                            assert_eq!(
                                after.browser_view,
                                if flat {
                                    BrowserView::Details
                                } else {
                                    browser_view
                                }
                            );
                            assert_eq!(after.flat, flat);
                            assert_eq!(after.folders, folders);
                            assert_eq!(after.sidebar_collapsed, sidebar_collapsed);
                            assert_eq!(after.preview_visible, preview_visible);
                            assert_eq!(after.sidebar, 257.5);
                            assert_eq!(after.preview_width, 512.5);
                            assert_eq!(after.directory_width, 297.5);
                            assert_eq!(after.text(), text);
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn flat_mode_selects_details_regardless_of_key_order() {
        for view in ["columns", "details", "unknown"] {
            for flat in ["yes", "no"] {
                let lines = [
                    format!("browser_view = {view}"),
                    format!("flat = {flat}"),
                    "sidebar_collapsed = yes".into(),
                    "folders = no".into(),
                    "sidebar = 300".into(),
                    "preview_visible = no".into(),
                    "preview_width = 480".into(),
                    "directory_width = 260".into(),
                ];
                let forward = Settings::parse(&lines.join("\n"));
                let reverse =
                    Settings::parse(&lines.iter().rev().cloned().collect::<Vec<_>>().join("\n"));
                let expected = if flat == "yes" || view == "details" {
                    BrowserView::Details
                } else {
                    BrowserView::Columns
                };
                assert_eq!(forward.browser_view, expected);
                assert_eq!(forward.flat, flat == "yes");
                assert_eq!(forward.text(), reverse.text());
            }
        }
    }

    #[test]
    fn unknown_layout_values_are_ignored() {
        let invalid = "browser_view = future\nsidebar_collapsed = maybe\npreview_visible = maybe";
        assert_eq!(Settings::parse(invalid).text(), Settings::default().text());
        let known = "browser_view = details\nsidebar_collapsed = yes\npreview_visible = no";
        assert_eq!(
            Settings::parse(&format!("{known}\n{invalid}")).text(),
            Settings::parse(known).text()
        );
        assert_eq!(
            Settings::parse(&format!("{invalid}\nflat = yes")).browser_view,
            BrowserView::Details
        );
    }

    #[test]
    fn finite_layout_widths_are_clamped_to_usable_bounds() {
        for (value, directory, preview, sidebar) in [
            ("-100", 220.0, 280.0, SIDEBAR_LEAST),
            ("0", 220.0, 280.0, SIDEBAR_LEAST),
            ("220", 220.0, 280.0, 220.0),
            ("320.5", 320.5, 320.5, 320.5),
            ("560", 560.0, 560.0, SIDEBAR_MOST),
            ("800", 560.0, 800.0, SIDEBAR_MOST),
            ("1e30", 560.0, 800.0, SIDEBAR_MOST),
        ] {
            let s = Settings::parse(&format!(
                "directory_width = {value}\npreview_width = {value}\nsidebar = {value}"
            ));
            assert_eq!(s.directory_width, directory, "{value}");
            assert_eq!(s.preview_width, preview, "{value}");
            assert_eq!(s.sidebar, sidebar, "{value}");
        }
    }

    #[test]
    fn invalid_layout_widths_do_not_replace_defaults_or_valid_values() {
        for value in [
            "NaN", "inf", "-inf", "Infinity", "1e99", "-1e99", "bad", "", "300px",
        ] {
            let invalid =
                format!("directory_width = {value}\npreview_width = {value}\nsidebar = {value}");
            assert_eq!(Settings::parse(&invalid).text(), Settings::default().text());
            let valid = "directory_width = 350\npreview_width = 510\nsidebar = 300";
            assert_eq!(
                Settings::parse(&format!("{valid}\n{invalid}")).text(),
                Settings::parse(valid).text()
            );
        }
    }

    #[test]
    fn serialization_normalizes_new_widths_without_mutating_preferences() {
        for (value, directory, preview) in [
            (f32::NAN, DIRECTORY_WIDE, PREVIEW_WIDE),
            (f32::INFINITY, DIRECTORY_WIDE, PREVIEW_WIDE),
            (f32::NEG_INFINITY, DIRECTORY_WIDE, PREVIEW_WIDE),
            (-10.0, DIRECTORY_LEAST, PREVIEW_LEAST),
            (1000.0, DIRECTORY_MOST, PREVIEW_MOST),
        ] {
            let s = Settings {
                directory_width: value,
                preview_width: value,
                ..Settings::default()
            };
            let text = s.text();
            let after = Settings::parse(&text);
            assert_eq!(after.directory_width, directory);
            assert_eq!(after.preview_width, preview);
            assert_eq!(after.text(), text);
            assert_eq!(s.directory_width.to_bits(), value.to_bits());
            assert_eq!(s.preview_width.to_bits(), value.to_bits());
        }
    }

    #[test]
    fn malformed_legacy_width_lists_are_rejected_as_a_whole() {
        let valid = Settings::default()
            .widths
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>();
        for value in ["NaN", "inf", "-inf", "1e99", "bad", ""] {
            let mut malformed = valid.clone();
            malformed[2] = value.into();
            let s = Settings::parse(&format!("widths = {}", malformed.join(",")));
            assert_eq!(s.widths, Settings::default_widths());
            malformed.insert(0, "400".into());
            let s = Settings::parse(&format!("widths = {}", malformed.join(",")));
            assert_eq!(s.widths, Settings::default_widths());
        }
        for len in [1, valid.len() - 1, valid.len() + 1] {
            let s = Settings::parse(&format!("widths = {}", vec!["400"; len].join(",")));
            assert_eq!(s.widths, Settings::default_widths());
        }
        let s = Settings::parse(&format!("widths = {}", vec!["-10"; valid.len()].join(",")));
        for (slot, width) in s.widths.iter().enumerate() {
            assert_eq!(*width, Settings::least(slot));
        }
    }

    #[test]
    fn invalid_window_geometry_does_not_replace_valid_geometry() {
        let valid = "window = -100,20,1000,700";
        for value in ["NaN", "inf", "-inf", "1e99", "bad", ""] {
            for slot in 0..4 {
                let mut parts = ["10", "20", "1000", "700"];
                parts[slot] = value;
                let invalid = format!("window = {}", parts.join(","));
                assert_eq!(Settings::parse(&invalid).window, None);
                assert_eq!(
                    Settings::parse(&format!("{valid}\n{invalid}")).window,
                    Settings::parse(valid).window
                );
            }
        }
        for value in [
            "1,2,719,700",
            "1,2,1000,319",
            "1,2,1000",
            "bad,1,2,1000,700",
        ] {
            assert_eq!(Settings::parse(&format!("window = {value}")).window, None);
        }
    }

    #[test]
    fn unknown_keys_and_content_are_not_persisted() {
        let s = Settings::parse(
            "password = not-a-real-password\npreview_contents = some-content\nfuture = yes\n\
             malformed line\n = value\n# comment\n",
        );
        assert_eq!(s.text(), Settings::default().text());
    }

    #[test]
    fn disk_preferences_round_trip() {
        let before = Settings {
            last_folder: Some("/home/someone/Documents".into()),
            pinned: vec!["/srv/share".into(), "/home/someone/Projects".into()],
            show_hidden: true,
            ..Default::default()
        };
        let after = Settings::parse(&before.text());
        assert_eq!(after.last_folder, before.last_folder);
        assert_eq!(after.pinned, before.pinned);
        assert!(after.show_hidden);
        let defaults = Settings::parse(&Settings::default().text());
        assert_eq!(defaults.last_folder, None);
        assert!(defaults.pinned.is_empty());
        assert!(!defaults.show_hidden);
    }
}
