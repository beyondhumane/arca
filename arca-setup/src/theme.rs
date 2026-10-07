use crate::assets::FONT_FILES;
use eframe::egui::{self, Color32, FontData, FontDefinitions, FontFamily, FontId};
use std::sync::Arc;

pub const BLUE: u32 = 0x0066FF;
pub const BLUE_2: u32 = 0x3B9CFF;
pub const BLUE_HOVER: u32 = 0x1A78FF;
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

const DISPLAY: &str = "display";

pub fn color(hex: u32) -> Color32 {
    Color32::from_rgb((hex >> 16) as u8, (hex >> 8) as u8, hex as u8)
}

pub fn alpha(hex: u32, amount: f32) -> Color32 {
    let [r, g, b, _] = color(hex).to_array();
    Color32::from_rgba_unmultiplied(r, g, b, (amount.clamp(0., 1.) * 255.).round() as u8)
}

pub fn display(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(DISPLAY.into()))
}

pub fn text(size: f32) -> FontId {
    FontId::proportional(size)
}

pub fn install(ctx: &egui::Context) {
    ctx.set_fonts(fonts());
    ctx.set_theme(egui::Theme::Light);
    ctx.style_mut_of(egui::Theme::Light, |style| {
        style.visuals.panel_fill = color(FOG);
        style.visuals.override_text_color = Some(color(INK));
        style.visuals.selection.stroke.color = color(BLUE);
        style.visuals.widgets.noninteractive.fg_stroke.color = color(INK);
        style.interaction.selectable_labels = false;
    });
}

fn fonts() -> FontDefinitions {
    let mut fonts = FontDefinitions::default();
    fonts.font_data.insert(
        "Sora".into(),
        Arc::new(FontData::from_static(FONT_FILES[0])),
    );
    fonts.font_data.insert(
        "Inter".into(),
        Arc::new(FontData::from_static(FONT_FILES[1])),
    );
    let fallback = fonts
        .families
        .get(&FontFamily::Proportional)
        .cloned()
        .unwrap_or_default();
    fonts
        .families
        .entry(FontFamily::Proportional)
        .or_default()
        .insert(0, "Inter".into());
    let mut display = vec!["Sora".to_string(), "Inter".to_string()];
    display.extend(fallback);
    fonts
        .families
        .insert(FontFamily::Name(DISPLAY.into()), display);
    fonts
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn both_brand_faces_are_installed_ahead_of_the_fallbacks() {
        let fonts = fonts();
        assert_eq!(fonts.families[&FontFamily::Proportional][0], "Inter");
        assert_eq!(
            fonts.families[&FontFamily::Name(DISPLAY.into())][..2],
            ["Sora".to_string(), "Inter".to_string()]
        );
    }

    #[test]
    fn colors_keep_their_channels() {
        assert_eq!(color(BLUE), Color32::from_rgb(0x00, 0x66, 0xFF));
        assert_eq!(alpha(WHITE, 0.5).a(), 128);
    }
}
