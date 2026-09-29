use crate::assets::FONT_FILES;
use gpui::{rgb, App, Rgba, SharedString};
use std::{borrow::Cow, sync::OnceLock};

pub const BLUE: u32 = 0x0066FF;
pub const BLUE_2: u32 = 0x3B9CFF;
pub const BLUE_PRESSED: u32 = 0x0057DB;
pub const INK: u32 = 0x0E1628;
pub const SLATE: u32 = 0x4F5D77;
pub const MUTED: u32 = 0x74839C;
pub const FOG: u32 = 0xF4F7FF;
pub const PALE: u32 = 0xEAF2FF;
pub const PALE_HOVER: u32 = 0xDCEAFF;
pub const LINE: u32 = 0xCFE0FF;
pub const TRACK: u32 = 0xDCE7FA;
pub const SWITCH_OFF: u32 = 0xC3CFE3;
pub const WHITE: u32 = 0xFFFFFF;

pub fn color(hex: u32) -> Rgba {
    rgb(hex)
}

const SYSTEM: &str = ".SystemUIFont";

struct Fonts {
    display: SharedString,
    text: SharedString,
}

static FONTS: OnceLock<Fonts> = OnceLock::new();

pub fn resolve_fonts(cx: &App) {
    let _ = cx
        .text_system()
        .add_fonts(FONT_FILES.iter().map(|font| Cow::Borrowed(*font)).collect());
    let installed = cx.text_system().all_font_names();
    let pick = |wanted: &str| {
        if installed.iter().any(|name| name == wanted) {
            SharedString::from(wanted.to_string())
        } else {
            SharedString::from(SYSTEM)
        }
    };
    let _ = FONTS.set(Fonts {
        display: pick("Sora"),
        text: pick("Inter"),
    });
}

pub fn display_font() -> SharedString {
    FONTS
        .get()
        .map_or_else(|| SharedString::from(SYSTEM), |f| f.display.clone())
}

pub fn text_font() -> SharedString {
    FONTS
        .get()
        .map_or_else(|| SharedString::from(SYSTEM), |f| f.text.clone())
}
