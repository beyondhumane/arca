use crate::{assets::FONT_FILES, ThemePreference};
use gpui::{px, App, Hsla, SharedString, Window};
use gpui_component::scroll::ScrollbarMode;
use gpui_component::theme::{Theme, ThemeMode, ThemeTokens};
use std::{borrow::Cow, sync::OnceLock};

const BLUE: u32 = 0x0066FF;
const BLUE_HOVER: u32 = 0x0057DB;
const BLUE_ACTIVE: u32 = 0x004CC2;
const WHITE: u32 = 0xFFFFFF;

struct Palette {
    background: u32,
    surface: u32,
    raised: u32,
    border: u32,
    text: u32,
    muted: u32,
    selected: u32,
    focus: u32,
    link: u32,
    spark: u32,
    danger: u32,
    warning: u32,
    success: u32,
    state_foreground: u32,
}

const DARK: Palette = Palette {
    background: 0x070C1E,
    surface: 0x0A1126,
    raised: 0x131D3A,
    border: 0x293A5C,
    text: 0xF4F7FF,
    muted: 0x9AAEC9,
    selected: 0x1B3559,
    focus: 0x3B9CFF,
    link: 0x5FAEFF,
    spark: 0xFF8A3D,
    danger: 0xFF7A8A,
    warning: 0xFFB366,
    success: 0x4ADEA5,
    state_foreground: 0x0E1628,
};

const LIGHT: Palette = Palette {
    background: WHITE,
    surface: 0xF4F7FF,
    raised: 0xEAF2FF,
    border: 0xC3CFE3,
    text: 0x0E1628,
    muted: 0x4F5D77,
    selected: 0xDCEAFF,
    focus: BLUE,
    link: BLUE_HOVER,
    spark: 0xC45100,
    danger: 0xC12035,
    warning: 0x9A4800,
    success: 0x147D58,
    state_foreground: WHITE,
};

struct Fonts {
    display: SharedString,
    text: SharedString,
}

static FONTS: OnceLock<Fonts> = OnceLock::new();

fn hex(value: u32) -> Hsla {
    gpui::rgb(value).into()
}

fn alpha(value: u32, a: f32) -> Hsla {
    let mut color = hex(value);
    color.a = a;
    color
}

pub fn init(cx: &mut App) {
    gpui_component::init(cx);
    let _ = cx
        .text_system()
        .add_fonts(FONT_FILES.iter().map(|font| Cow::Borrowed(*font)).collect());
    let installed = cx.text_system().all_font_names();
    let pick = |wanted: &str| {
        if installed.iter().any(|name| name == wanted) {
            SharedString::from(wanted.to_string())
        } else {
            system_ui().into()
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
        .map_or_else(|| system_ui().into(), |fonts| fonts.display.clone())
}

fn text_font() -> SharedString {
    FONTS
        .get()
        .map_or_else(|| system_ui().into(), |fonts| fonts.text.clone())
}

pub fn apply(preference: ThemePreference, window: Option<&mut Window>, cx: &mut App) {
    let mode = match preference {
        ThemePreference::Light => ThemeMode::Light,
        ThemePreference::Dark => ThemeMode::Dark,
        ThemePreference::System => window
            .as_ref()
            .map(|window| window.appearance())
            .unwrap_or_else(|| cx.window_appearance())
            .into(),
    };

    // Theme::change resets the colors; sync_base must see our overrides.
    Theme::change(mode, window, cx);
    paint(mode, Theme::global_mut(cx));
    Theme::sync_base(cx);
    Theme::set_scrollbar_mode(ScrollbarMode::Hover, cx);
}

fn paint(mode: ThemeMode, theme: &mut Theme) {
    let p = if mode.is_dark() { DARK } else { LIGHT };

    theme.radius = px(6.);
    theme.radius_lg = px(12.);
    theme.font_size = px(16.);
    theme.mono_font_size = px(13.);
    theme.font_family = text_font();
    theme.mono_font_family = system_mono().into();

    theme.background = hex(p.background);
    theme.foreground = hex(p.text);
    theme.border = hex(p.border);
    theme.muted = hex(p.raised);
    theme.muted_foreground = hex(p.muted);
    theme.transparent = alpha(p.background, 0.0);
    theme.ring = hex(p.focus);
    theme.selection = hex(p.selected);
    theme.caret = hex(p.focus);

    theme.primary = hex(BLUE);
    theme.primary_foreground = hex(WHITE);
    theme.primary_hover = hex(BLUE_HOVER);
    theme.primary_active = hex(BLUE_ACTIVE);
    theme.secondary = hex(p.surface);
    theme.secondary_foreground = hex(p.text);
    theme.secondary_hover = hex(p.raised);
    theme.secondary_active = hex(p.selected);

    theme.button = hex(p.surface);
    theme.button_foreground = hex(p.text);
    theme.button_hover = hex(p.raised);
    theme.button_active = hex(p.selected);
    theme.button_primary = theme.primary;
    theme.button_primary_foreground = theme.primary_foreground;
    theme.button_primary_hover = theme.primary_hover;
    theme.button_primary_active = theme.primary_active;
    theme.button_secondary = theme.secondary;
    theme.button_secondary_foreground = theme.secondary_foreground;
    theme.button_secondary_hover = theme.secondary_hover;
    theme.button_secondary_active = theme.secondary_active;

    theme.accent = hex(p.raised);
    theme.accent_foreground = hex(p.text);
    theme.popover = hex(p.surface);
    theme.popover_foreground = hex(p.text);
    theme.overlay = alpha(0x070C1E, if mode.is_dark() { 0.72 } else { 0.36 });
    theme.input = hex(p.border);

    theme.table = hex(p.background);
    theme.table_head = hex(p.surface);
    theme.table_head_foreground = hex(p.muted);
    theme.table_foot = hex(p.surface);
    theme.table_foot_foreground = hex(p.muted);
    theme.table_even = alpha(p.focus, 0.025);
    theme.table_hover = hex(p.raised);
    theme.table_active = hex(p.selected);
    theme.table_active_border = hex(p.focus);
    theme.table_row_border = alpha(p.background, 0.0);
    theme.colors.list = hex(p.background);
    theme.list_head = hex(p.surface);
    theme.list_even = theme.table_even;
    theme.list_hover = theme.table_hover;
    theme.list_active = theme.table_active;
    theme.list_active_border = theme.table_active_border;

    theme.scrollbar = alpha(p.background, 0.0);
    theme.scrollbar_thumb = alpha(p.muted, 0.4);
    theme.scrollbar_thumb_hover = alpha(p.muted, 0.65);
    theme.title_bar = hex(p.surface);
    theme.title_bar_border = hex(p.border);
    theme.status_bar = hex(p.surface);
    theme.status_bar_border = hex(p.border);
    theme.window_border = hex(p.border);

    theme.sidebar = hex(p.surface);
    theme.sidebar_foreground = hex(p.text);
    theme.sidebar_border = hex(p.border);
    theme.sidebar_accent = hex(p.raised);
    theme.sidebar_accent_foreground = hex(p.text);
    theme.sidebar_primary = hex(BLUE);
    theme.sidebar_primary_foreground = hex(WHITE);

    theme.tab = hex(p.surface);
    theme.tab_bar = hex(p.surface);
    theme.tab_bar_segmented = hex(p.raised);
    theme.tab_foreground = hex(p.muted);
    theme.tab_active = hex(p.selected);
    theme.tab_active_foreground = hex(p.text);
    theme.accordion = hex(p.surface);
    theme.group_box = hex(p.surface);
    theme.group_box_foreground = hex(p.text);
    theme.description_list_label = hex(p.surface);
    theme.description_list_label_foreground = hex(p.muted);
    theme.skeleton = hex(p.raised);
    theme.tiles = hex(p.surface);

    theme.switch = hex(p.border);
    theme.switch_thumb = hex(WHITE);
    theme.slider_bar = hex(p.border);
    theme.slider_thumb = hex(p.focus);
    theme.progress_bar = hex(p.spark);
    theme.drag_border = hex(p.focus);
    theme.drop_target = alpha(p.focus, 0.12);
    theme.link = hex(p.link);
    theme.link_hover = hex(p.focus);
    theme.link_active = hex(p.link);

    theme.info = hex(p.link);
    theme.info_foreground = hex(p.state_foreground);
    theme.info_hover = theme.info;
    theme.info_active = theme.info;
    theme.button_info = theme.info;
    theme.button_info_foreground = theme.info_foreground;
    theme.button_info_hover = theme.info_hover;
    theme.button_info_active = theme.info_active;

    theme.success = hex(p.success);
    theme.success_foreground = hex(p.state_foreground);
    theme.success_hover = theme.success;
    theme.success_active = theme.success;
    theme.button_success = theme.success;
    theme.button_success_foreground = theme.success_foreground;
    theme.button_success_hover = theme.success_hover;
    theme.button_success_active = theme.success_active;

    theme.danger = hex(p.danger);
    theme.danger_foreground = hex(p.state_foreground);
    theme.danger_hover = theme.danger;
    theme.danger_active = theme.danger;
    theme.button_danger = theme.danger;
    theme.button_danger_foreground = theme.danger_foreground;
    theme.button_danger_hover = theme.danger_hover;
    theme.button_danger_active = theme.danger_active;

    theme.warning = hex(p.warning);
    theme.warning_foreground = hex(p.state_foreground);
    theme.warning_hover = theme.warning;
    theme.warning_active = theme.warning;
    theme.button_warning = theme.warning;
    theme.button_warning_foreground = theme.warning_foreground;
    theme.button_warning_hover = theme.warning_hover;
    theme.button_warning_active = theme.warning_active;

    // Kit components read tokens, while Arca's custom views read colors.
    theme.tokens = ThemeTokens::from(&theme.colors);
}

fn system_ui() -> &'static str {
    if cfg!(windows) {
        "Segoe UI"
    } else {
        ".SystemUIFont"
    }
}

fn system_mono() -> &'static str {
    if cfg!(windows) {
        "Consolas"
    } else {
        "monospace"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn luminance(value: u32) -> f32 {
        let channel = |shift: u32| {
            let c = ((value >> shift) & 0xFF) as f32 / 255.0;
            if c <= 0.04045 {
                c / 12.92
            } else {
                ((c + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * channel(16) + 0.7152 * channel(8) + 0.0722 * channel(0)
    }

    fn contrast(a: u32, b: u32) -> f32 {
        let (x, y) = (luminance(a), luminance(b));
        (x.max(y) + 0.05) / (x.min(y) + 0.05)
    }

    #[test]
    fn text_and_selection_meet_wcag_aa_in_both_modes() {
        for (name, p) in [("dark", DARK), ("light", LIGHT)] {
            for ground in [p.background, p.surface, p.raised, p.selected] {
                for ink in [p.text, p.muted] {
                    assert!(
                        contrast(ink, ground) >= 4.5,
                        "{name}: {ink:06x} on {ground:06x}"
                    );
                }
                assert!(
                    contrast(p.focus, ground) >= 3.0,
                    "{name}: focus on {ground:06x}"
                );
                assert!(
                    contrast(p.spark, ground) >= 3.0,
                    "{name}: progress on {ground:06x}"
                );
            }
        }
    }

    #[test]
    fn actions_and_semantic_states_remain_legible() {
        for primary in [BLUE, BLUE_HOVER, BLUE_ACTIVE] {
            assert!(contrast(WHITE, primary) >= 4.5);
        }
        for (name, p) in [("dark", DARK), ("light", LIGHT)] {
            for state in [p.link, p.danger, p.warning, p.success] {
                for ground in [p.background, p.surface, p.raised] {
                    assert!(
                        contrast(state, ground) >= 4.5,
                        "{name}: {state:06x} on {ground:06x}"
                    );
                }
                assert!(
                    contrast(p.state_foreground, state) >= 4.5,
                    "{name}: state label on {state:06x}"
                );
            }
        }
    }

    #[test]
    fn switching_modes_repaints_custom_and_kit_tokens() {
        let mut theme = Theme::default();
        for (mode, p) in [
            (ThemeMode::Light, LIGHT),
            (ThemeMode::Dark, DARK),
            (ThemeMode::Light, LIGHT),
        ] {
            paint(mode, &mut theme);
            assert_eq!(theme.background, hex(p.background));
            assert_eq!(theme.tokens.background.color, theme.background);
            assert_eq!(theme.tokens.button_primary.color, hex(BLUE));
            assert_eq!(theme.tokens.button_primary_foreground.color, hex(WHITE));
            assert_eq!(theme.tokens.table_active.color, hex(p.selected));
            assert_eq!(theme.tokens.list_active.color, theme.table_active);
            assert_eq!(theme.tokens.ring.color, hex(p.focus));
            assert_eq!(theme.tokens.progress_bar.color, hex(p.spark));
            assert_eq!(theme.tokens.danger.color, hex(p.danger));
            assert_eq!(theme.tokens.popover.color, hex(p.surface));
        }
    }
}
