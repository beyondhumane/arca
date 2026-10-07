use crate::assets::FONT_FILES;
use crate::ThemePreference;
use eframe::egui::{
    self, Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Stroke, TextStyle,
    Visuals,
};
use std::sync::Arc;

pub(crate) const BLUE: u32 = 0x0066FF;
pub(crate) const BLUE_HOVER: u32 = 0x0057DB;
pub(crate) const BLUE_ACTIVE: u32 = 0x004CC2;
pub(crate) const WHITE: u32 = 0xFFFFFF;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Palette {
    pub background: u32,
    pub surface: u32,
    pub raised: u32,
    pub border: u32,
    pub text: u32,
    pub muted: u32,
    pub selected: u32,
    pub focus: u32,
    pub link: u32,
    pub spark: u32,
    pub danger: u32,
    pub warning: u32,
    pub success: u32,
    pub state_foreground: u32,
}

// Tokyo Night, the dark one Arca opens with.
pub(crate) const DARK: Palette = Palette {
    background: 0x16161E,
    surface: 0x1A1B26,
    raised: 0x232637,
    border: 0x2F344D,
    text: 0xC0CAF5,
    muted: 0xA9B1D6,
    selected: 0x2A3459,
    focus: 0x7AA2F7,
    link: 0x7AA2F7,
    spark: 0xFF9E64,
    danger: 0xF7768E,
    warning: 0xE0AF68,
    success: 0x9ECE6A,
    state_foreground: 0x16161E,
};

pub(crate) const LIGHT: Palette = Palette {
    background: 0xFFFFFF,
    surface: 0xF4F7FF,
    raised: 0xEAF2FF,
    border: 0xC3CFE3,
    text: 0x0E1628,
    muted: 0x4F5D77,
    selected: 0xDCEAFF,
    focus: 0x0066FF,
    link: 0x0057DB,
    spark: 0xC45100,
    danger: 0xC12035,
    warning: 0x9A4800,
    success: 0x147D58,
    state_foreground: 0xFFFFFF,
};

pub(crate) fn hex(value: u32) -> Color32 {
    Color32::from_rgb((value >> 16) as u8, (value >> 8) as u8, value as u8)
}

pub(crate) fn alpha(value: u32, amount: f32) -> Color32 {
    let [r, g, b, _] = hex(value).to_array();
    Color32::from_rgba_unmultiplied(r, g, b, (amount.clamp(0.0, 1.0) * 255.0) as u8)
}

pub(crate) fn palette_of(ctx: &egui::Context) -> Palette {
    if ctx.global_style().visuals.dark_mode {
        DARK
    } else {
        LIGHT
    }
}

pub(crate) const DISPLAY: &str = "Sora";

pub(crate) fn fonts() -> FontDefinitions {
    let mut fonts = FontDefinitions::default();
    fonts.font_data.insert(
        DISPLAY.into(),
        Arc::new(FontData::from_static(FONT_FILES[0])),
    );
    fonts.font_data.insert(
        "Inter".into(),
        Arc::new(FontData::from_static(FONT_FILES[1])),
    );
    if let Some(family) = fonts.families.get_mut(&FontFamily::Proportional) {
        family.insert(0, "Inter".into());
    }
    egui_phosphor::add_to_fonts(&mut fonts, egui_phosphor::Variant::Regular);
    fonts
        .families
        .insert(FontFamily::Name("phosphor".into()), vec!["phosphor".into()]);
    let mut display = vec![DISPLAY.to_string()];
    display.extend(
        fonts
            .families
            .get(&FontFamily::Proportional)
            .cloned()
            .unwrap_or_default(),
    );
    fonts
        .families
        .insert(FontFamily::Name(DISPLAY.into()), display);
    fonts
}

pub(crate) fn icon_font(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("phosphor".into()))
}

pub(crate) fn icon_text(glyph: &str) -> egui::RichText {
    egui::RichText::new(glyph).family(FontFamily::Name("phosphor".into()))
}

pub(crate) fn display(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(DISPLAY.into()))
}

fn visuals(p: Palette, dark: bool) -> Visuals {
    let mut v = if dark {
        Visuals::dark()
    } else {
        Visuals::light()
    };
    let radius = CornerRadius::same(6);
    v.override_text_color = Some(hex(p.text));
    v.panel_fill = hex(p.background);
    v.window_fill = hex(p.surface);
    v.extreme_bg_color = hex(p.surface);
    v.faint_bg_color = hex(p.surface);
    v.code_bg_color = hex(p.raised);
    v.window_stroke = Stroke::new(1.0, hex(p.border));
    v.window_corner_radius = CornerRadius::same(10);
    v.menu_corner_radius = CornerRadius::same(8);
    v.hyperlink_color = hex(p.link);
    v.warn_fg_color = hex(p.warning);
    v.error_fg_color = hex(p.danger);
    v.selection.bg_fill = hex(p.selected);
    v.selection.stroke = Stroke::new(1.0, hex(p.focus));
    v.text_cursor.stroke = Stroke::new(2.0, hex(p.focus));
    for (w, fill, stroke) in [
        (&mut v.widgets.noninteractive, p.background, p.border),
        (&mut v.widgets.inactive, p.raised, p.border),
        (&mut v.widgets.hovered, p.selected, p.focus),
        (&mut v.widgets.active, p.selected, p.focus),
        (&mut v.widgets.open, p.raised, p.border),
    ] {
        w.bg_fill = hex(fill);
        w.weak_bg_fill = hex(fill);
        w.bg_stroke = Stroke::new(1.0, hex(stroke));
        w.corner_radius = radius;
        w.fg_stroke = Stroke::new(1.0, hex(p.text));
    }
    v.widgets.noninteractive.weak_bg_fill = Color32::TRANSPARENT;
    v.widgets.noninteractive.fg_stroke = Stroke::new(1.0, hex(p.text));
    v.widgets.inactive.weak_bg_fill = Color32::TRANSPARENT;
    v.widgets.inactive.bg_stroke = Stroke::NONE;
    v.widgets.hovered.weak_bg_fill = alpha(p.focus, 0.12);
    v.striped = false;
    v
}

/// Fonts, spacing and both palettes, installed once.
pub(crate) fn install(ctx: &egui::Context, preference: ThemePreference) {
    ctx.set_fonts(fonts());
    ctx.set_visuals_of(egui::Theme::Dark, visuals(DARK, true));
    ctx.set_visuals_of(egui::Theme::Light, visuals(LIGHT, false));
    ctx.all_styles_mut(|style| {
        style.spacing.item_spacing = egui::vec2(6.0, 4.0);
        style.spacing.button_padding = egui::vec2(8.0, 3.0);
        style.spacing.interact_size.y = 24.0;
        style.spacing.menu_margin = egui::Margin::same(6);
        style.interaction.selectable_labels = false;
        style
            .text_styles
            .insert(TextStyle::Body, FontId::proportional(13.0));
        style
            .text_styles
            .insert(TextStyle::Button, FontId::proportional(13.0));
        style
            .text_styles
            .insert(TextStyle::Small, FontId::proportional(11.0));
        style
            .text_styles
            .insert(TextStyle::Monospace, FontId::monospace(12.0));
        style.text_styles.insert(TextStyle::Heading, display(17.0));
    });
    apply(ctx, preference);
}

pub(crate) fn apply(ctx: &egui::Context, preference: ThemePreference) {
    ctx.options_mut(|options| {
        options.theme_preference = match preference {
            ThemePreference::System => egui::ThemePreference::System,
            ThemePreference::Light => egui::ThemePreference::Light,
            ThemePreference::Dark => egui::ThemePreference::Dark,
        };
        options.fallback_theme = egui::Theme::Dark;
    });
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
    fn icons_are_drawn_from_phosphor_and_not_from_the_text_font() {
        let fonts = fonts();
        let icons = &fonts.families[&FontFamily::Name("phosphor".into())];
        assert_eq!(icons, &vec!["phosphor".to_string()]);
        assert_eq!(fonts.families[&FontFamily::Proportional][0], "Inter");
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
                assert!(contrast(p.focus, ground) >= 3.0, "{name}: focus");
                assert!(contrast(p.spark, ground) >= 3.0, "{name}: progress");
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
                    assert!(contrast(state, ground) >= 4.5, "{name}: {state:06x}");
                }
                assert!(contrast(p.state_foreground, state) >= 4.5, "{name}");
            }
        }
    }

    #[test]
    fn the_brand_fonts_and_icons_are_installed() {
        let fonts = fonts();
        assert!(fonts.font_data.contains_key("Sora"));
        assert!(fonts.font_data.contains_key("Inter"));
        assert_eq!(fonts.families[&FontFamily::Proportional][0], "Inter");
        assert_eq!(fonts.families[&FontFamily::Name(DISPLAY.into())][0], "Sora");
        assert!(fonts.font_data.len() > 3, "phosphor glyphs are missing");
    }
}
