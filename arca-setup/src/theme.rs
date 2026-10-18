pub use arca_brand::{alpha, hex, BLUE, BLUE_HOVER, BLUE_PRESSED, WHITE};
use arca_brand::{INTER, SORA};
use eframe::egui::{self, FontDefinitions, FontFamily, FontId};

pub const BLUE_2: u32 = 0x3B9CFF;
pub const INK: u32 = 0x0E1628;
pub const SLATE: u32 = 0x4F5D77;
pub const MUTED: u32 = 0x74839C;
pub const FOG: u32 = 0xF4F7FF;
pub const PALE: u32 = 0xEAF2FF;
pub const PALE_HOVER: u32 = 0xDCEAFF;
pub const LINE: u32 = 0xCFE0FF;
pub const TRACK: u32 = 0xDCE7FA;
pub const SWITCH_OFF: u32 = 0xC3CFE3;

const DISPLAY: &str = "display";

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
        style.visuals.panel_fill = hex(FOG);
        style.visuals.override_text_color = Some(hex(INK));
        style.visuals.selection.stroke.color = hex(BLUE);
        style.visuals.widgets.noninteractive.fg_stroke.color = hex(INK);
        style.interaction.selectable_labels = false;
    });
}

fn fonts() -> FontDefinitions {
    let mut fonts = FontDefinitions::default();
    arca_brand::register_brand_fonts(&mut fonts);
    let fallback = fonts
        .families
        .get(&FontFamily::Proportional)
        .cloned()
        .unwrap_or_default();
    fonts
        .families
        .entry(FontFamily::Proportional)
        .or_default()
        .insert(0, INTER.into());
    let mut display = vec![SORA.to_string(), INTER.to_string()];
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
        assert_eq!(fonts.families[&FontFamily::Proportional][0], INTER);
        assert_eq!(
            fonts.families[&FontFamily::Name(DISPLAY.into())][..2],
            [SORA.to_string(), INTER.to_string()]
        );
    }
}
