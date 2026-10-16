use super::theme::{Palette, DARK, LIGHT};
use std::path::PathBuf;

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Theme {
    pub(crate) name: String,
    pub(crate) dark: bool,
    pub(crate) palette: Palette,
    pub(crate) custom: bool,
}

const FILE_LIMIT: u64 = 16 * 1024;

const BUILTIN: &[(&str, bool, Palette)] = &[
    ("Tokyo Night", true, DARK),
    ("Arca Light", false, LIGHT),
    (
        "Catppuccin Mocha",
        true,
        Palette {
            background: 0x181825,
            surface: 0x1E1E2E,
            raised: 0x313244,
            border: 0x45475A,
            text: 0xCDD6F4,
            muted: 0xA6ADC8,
            selected: 0x313244,
            focus: 0x89B4FA,
            link: 0x89B4FA,
            spark: 0xFAB387,
            danger: 0xF38BA8,
            warning: 0xF9E2AF,
            success: 0xA6E3A1,
            state_foreground: 0x11111B,
        },
    ),
    (
        "Catppuccin Latte",
        false,
        Palette {
            background: 0xEFF1F5,
            surface: 0xE6E9EF,
            raised: 0xDCE0E8,
            border: 0xBCC0CC,
            text: 0x4C4F69,
            muted: 0x54576B,
            selected: 0xCCD0DA,
            focus: 0x1E66F5,
            link: 0x0A54E6,
            spark: 0xC74801,
            danger: 0xC30F36,
            warning: 0x85580F,
            success: 0x2A701F,
            state_foreground: 0xEFF1F5,
        },
    ),
    (
        "Nord",
        true,
        Palette {
            background: 0x2E3440,
            surface: 0x3B4252,
            raised: 0x434C5E,
            border: 0x4C566A,
            text: 0xECEFF4,
            muted: 0xD8DEE9,
            selected: 0x434C5E,
            focus: 0x88C0D0,
            link: 0x90C4D2,
            spark: 0xD08770,
            danger: 0xDEB1B3,
            warning: 0xEBCB8B,
            success: 0xACC495,
            state_foreground: 0x2E3440,
        },
    ),
    (
        "Gruvbox Dark",
        true,
        Palette {
            background: 0x1D2021,
            surface: 0x282828,
            raised: 0x3C3836,
            border: 0x504945,
            text: 0xEBDBB2,
            muted: 0xBDAE93,
            selected: 0x3C3836,
            focus: 0x83A598,
            link: 0x89A99C,
            spark: 0xFE8019,
            danger: 0xFB7D70,
            warning: 0xFABD2F,
            success: 0xB8BB26,
            state_foreground: 0x1D2021,
        },
    ),
    (
        "Gruvbox Light",
        false,
        Palette {
            background: 0xFBF1C7,
            surface: 0xF2E5BC,
            raised: 0xEBDBB2,
            border: 0xD5C4A1,
            text: 0x3C3836,
            muted: 0x504945,
            selected: 0xE0CFA9,
            focus: 0x076678,
            link: 0x076678,
            spark: 0xAF3A03,
            danger: 0x9D0006,
            warning: 0x83580E,
            success: 0x65640B,
            state_foreground: 0xFBF1C7,
        },
    ),
    (
        "Dracula",
        true,
        Palette {
            background: 0x21222C,
            surface: 0x282A36,
            raised: 0x343746,
            border: 0x44475A,
            text: 0xF8F8F2,
            muted: 0xBFC3D9,
            selected: 0x44475A,
            focus: 0xBD93F9,
            link: 0x8BE9FD,
            spark: 0xFFB86C,
            danger: 0xFF7878,
            warning: 0xF1FA8C,
            success: 0x50FA7B,
            state_foreground: 0x21222C,
        },
    ),
    (
        "One Dark",
        true,
        Palette {
            background: 0x21252B,
            surface: 0x282C34,
            raised: 0x2C313A,
            border: 0x3E4451,
            text: 0xABB2BF,
            muted: 0x9DA5B4,
            selected: 0x2C313A,
            focus: 0x61AFEF,
            link: 0x61AFEF,
            spark: 0xD19A66,
            danger: 0xE37881,
            warning: 0xE5C07B,
            success: 0x98C379,
            state_foreground: 0x21252B,
        },
    ),
    (
        "Solarized Light",
        false,
        Palette {
            background: 0xFDF6E3,
            surface: 0xEEE8D5,
            raised: 0xE6DFC8,
            border: 0xD3CBB7,
            text: 0x073642,
            muted: 0x506269,
            selected: 0xE3DCC5,
            focus: 0x2382C6,
            link: 0x19649E,
            spark: 0xCB4B16,
            danger: 0xBF221F,
            warning: 0x7E5D00,
            success: 0x5B6700,
            state_foreground: 0xFDF6E3,
        },
    ),
    (
        "Rose Pine",
        true,
        Palette {
            background: 0x191724,
            surface: 0x1F1D2E,
            raised: 0x26233A,
            border: 0x403D52,
            text: 0xE0DEF4,
            muted: 0x938FAC,
            selected: 0x2A283E,
            focus: 0xC4A7E7,
            link: 0x9CCFD8,
            spark: 0xF6C177,
            danger: 0xEB6F92,
            warning: 0xF6C177,
            success: 0x3D98BF,
            state_foreground: 0x191724,
        },
    ),
    (
        "GitHub Light",
        false,
        Palette {
            background: 0xFFFFFF,
            surface: 0xF6F8FA,
            raised: 0xEAEEF2,
            border: 0xD0D7DE,
            text: 0x1F2328,
            muted: 0x59636E,
            selected: 0xDDF4FF,
            focus: 0x0969DA,
            link: 0x0967D5,
            spark: 0xBC4C00,
            danger: 0xCF222E,
            warning: 0x906100,
            success: 0x197B35,
            state_foreground: 0xFFFFFF,
        },
    ),
    (
        "Everforest Dark",
        true,
        Palette {
            background: 0x272E33,
            surface: 0x2E383C,
            raised: 0x374145,
            border: 0x414B50,
            text: 0xD3C6AA,
            muted: 0xACB3AF,
            selected: 0x3A464C,
            focus: 0xA7C080,
            link: 0x7FBBB3,
            spark: 0xE69875,
            danger: 0xEB9294,
            warning: 0xDBBC7F,
            success: 0xA7C080,
            state_foreground: 0x272E33,
        },
    ),
];

pub(crate) fn builtin() -> Vec<Theme> {
    BUILTIN
        .iter()
        .map(|(name, dark, palette)| Theme {
            name: name.to_string(),
            dark: *dark,
            palette: *palette,
            custom: false,
        })
        .collect()
}

pub(crate) fn folder() -> Option<PathBuf> {
    crate::settings::config_file()?
        .parent()
        .map(|dir| dir.join("themes"))
}

fn color(value: &str) -> Option<u32> {
    let digits = value.strip_prefix('#')?;
    if digits.len() != 6 {
        return None;
    }
    u32::from_str_radix(digits, 16).ok()
}

pub(crate) fn parse(text: &str) -> Option<Theme> {
    let pairs: Vec<(&str, &str)> = text
        .lines()
        .filter_map(|line| line.split_once('='))
        .map(|(k, v)| (k.trim(), v.trim()))
        .collect();
    let dark = !pairs.iter().any(|(k, v)| *k == "kind" && *v == "light");
    let mut palette = if dark { DARK } else { LIGHT };
    let mut name = None;
    for (key, value) in pairs {
        if key == "name" {
            let clean: String = value.chars().filter(|c| !c.is_control()).take(48).collect();
            name = Some(clean.trim().to_string()).filter(|n| !n.is_empty());
        } else if let (Some(slot), Some(c)) = (palette.slot(key), color(value)) {
            *slot = c;
        }
    }
    Some(Theme {
        name: name?,
        dark,
        palette,
        custom: true,
    })
}

pub(crate) fn text(theme: &Theme) -> String {
    let mut out = format!(
        "name = {}\nkind = {}\n",
        theme.name,
        if theme.dark { "dark" } else { "light" }
    );
    for key in Palette::KEYS {
        if let Some(value) = theme.palette.get(key) {
            out.push_str(&format!("{key} = #{value:06x}\n"));
        }
    }
    out
}

pub(crate) fn file_name(name: &str) -> String {
    let mut slug = String::new();
    for c in name.chars().take(48) {
        if c.is_ascii_alphanumeric() {
            slug.push(c.to_ascii_lowercase());
        } else if !slug.ends_with('-') {
            slug.push('-');
        }
    }
    let slug = slug.trim_matches('-');
    format!("{}.conf", if slug.is_empty() { "theme" } else { slug })
}

pub(crate) fn custom() -> Vec<Theme> {
    let Some(dir) = folder() else {
        return Vec::new();
    };
    let Ok(read) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut out: Vec<Theme> = read
        .flatten()
        .filter(|e| e.path().extension().is_some_and(|x| x == "conf"))
        .filter(|e| {
            e.metadata()
                .is_ok_and(|m| m.is_file() && m.len() <= FILE_LIMIT)
        })
        .filter_map(|e| std::fs::read_to_string(e.path()).ok())
        .filter_map(|t| parse(&t))
        .collect();
    out.sort_by_key(|t| t.name.to_lowercase());
    out
}

pub(crate) fn all() -> Vec<Theme> {
    let mut out = builtin();
    for theme in custom() {
        if !out.iter().any(|t| t.name == theme.name) {
            out.push(theme);
        }
    }
    out
}

/// Whether a built-in theme already uses `name`, ignoring case: a custom theme
/// by that name would be hidden behind it.
pub(crate) fn builtin_name(name: &str) -> bool {
    builtin().iter().any(|t| t.name.eq_ignore_ascii_case(name))
}

pub(crate) fn save(theme: &Theme) -> std::io::Result<()> {
    if builtin_name(theme.name.trim()) {
        return Err(std::io::ErrorKind::AlreadyExists.into());
    }
    let dir = folder().ok_or(std::io::ErrorKind::NotFound)?;
    std::fs::create_dir_all(&dir)?;
    std::fs::write(dir.join(file_name(&theme.name)), text(theme))
}

pub(crate) fn delete(name: &str) -> std::io::Result<()> {
    let dir = folder().ok_or(std::io::ErrorKind::NotFound)?;
    std::fs::remove_file(dir.join(file_name(name)))
}

pub(crate) fn palette(themes: &[Theme], name: &str, dark: bool) -> Palette {
    themes
        .iter()
        .find(|t| t.dark == dark && t.name == name)
        .map(|t| t.palette)
        .unwrap_or(if dark { DARK } else { LIGHT })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_theme_file_reads_back_what_was_written() {
        let mut palette = LIGHT;
        palette.focus = 0x123456;
        let theme = Theme {
            name: "Paper".into(),
            dark: false,
            palette,
            custom: true,
        };
        assert_eq!(parse(&text(&theme)), Some(theme));
    }

    #[test]
    fn broken_theme_files_fall_back_instead_of_failing() {
        assert_eq!(parse("kind = dark\n"), None, "a theme needs a name");
        let t = parse("name = X\nfocus = #zzzzzz\ntext = 123\nnonsense\n").unwrap();
        assert_eq!(t.palette, DARK);
        assert_eq!(file_name("../../etc/passwd"), "etc-passwd.conf");
        assert_eq!(file_name("***"), "theme.conf");
    }

    #[test]
    fn builtin_names_are_unique_and_both_kinds_exist() {
        let all = builtin();
        for (i, t) in all.iter().enumerate() {
            assert!(all[i + 1..].iter().all(|u| u.name != t.name), "{}", t.name);
        }
        assert!(all.iter().any(|t| t.dark) && all.iter().any(|t| !t.dark));
        assert_eq!(palette(&all, "Tokyo Night", true), DARK);
        assert_eq!(palette(&all, "missing", false), LIGHT);
    }

    #[test]
    fn built_in_names_cannot_be_saved_over() {
        assert!(builtin_name("nord"));
        assert!(builtin_name("Tokyo Night"));
        assert!(!builtin_name("My Nord"));
        let mut theme = builtin().remove(0);
        theme.custom = true;
        assert!(save(&theme).is_err());
    }
}
