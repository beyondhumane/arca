#![forbid(unsafe_code)]

use egui::{Color32, FontData, FontDefinitions};
use std::sync::Arc;

pub const BLUE: u32 = 0x0066FF;
pub const BLUE_HOVER: u32 = 0x0057DB;
pub const BLUE_PRESSED: u32 = 0x004CC2;
pub const WHITE: u32 = 0xFFFFFF;

pub const SORA: &str = "Sora";
pub const INTER: &str = "Inter";

const SORA_FILE: &[u8] = include_bytes!("../fonts/Sora-SemiBold.ttf");
const INTER_FILE: &[u8] = include_bytes!("../fonts/Inter-Regular.otf");

pub fn hex(value: u32) -> Color32 {
    Color32::from_rgb((value >> 16) as u8, (value >> 8) as u8, value as u8)
}

pub fn alpha(value: u32, amount: f32) -> Color32 {
    let [r, g, b, _] = hex(value).to_array();
    Color32::from_rgba_unmultiplied(r, g, b, (amount.clamp(0.0, 1.0) * 255.0).round() as u8)
}

/// Adds the Sora and Inter faces under [`SORA`] and [`INTER`]; placing them
/// in font families is left to each window.
pub fn register_brand_fonts(fonts: &mut FontDefinitions) {
    fonts
        .font_data
        .insert(SORA.into(), Arc::new(FontData::from_static(SORA_FILE)));
    fonts
        .font_data
        .insert(INTER.into(), Arc::new(FontData::from_static(INTER_FILE)));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colors_keep_their_channels() {
        assert_eq!(hex(BLUE), Color32::from_rgb(0x00, 0x66, 0xFF));
        assert_eq!(hex(BLUE_HOVER), Color32::from_rgb(0x00, 0x57, 0xDB));
        assert_eq!(alpha(WHITE, 0.5).a(), 128);
        assert_eq!(alpha(WHITE, 2.0).a(), 255);
        assert_eq!(alpha(WHITE, -1.0).a(), 0);
    }

    #[test]
    fn both_faces_are_registered_under_their_names() {
        let mut fonts = FontDefinitions::empty();
        register_brand_fonts(&mut fonts);
        assert_eq!(fonts.font_data.len(), 2);
        assert!(fonts.font_data.contains_key(SORA));
        assert!(fonts.font_data.contains_key(INTER));
    }
}
