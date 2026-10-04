use gpui::{AssetSource, Result, SharedString};
use std::borrow::Cow;

const CUSTOM: [&str; 13] = [
    "brand/mark.svg",
    "icons/clipboard-paste.svg",
    "icons/file-output.svg",
    "icons/lock.svg",
    "icons/scissors.svg",
    "icons/trash-2.svg",
    "icons/house.svg",
    "icons/download.svg",
    "icons/image.svg",
    "icons/music.svg",
    "icons/film.svg",
    "icons/pin.svg",
    "icons/monitor.svg",
];

pub const FONT_FILES: [&[u8]; 2] = [
    include_bytes!("../../arca-setup/assets/fonts/Sora-SemiBold.ttf").as_slice(),
    include_bytes!("../../arca-setup/assets/fonts/Inter-Regular.otf").as_slice(),
];

pub struct Assets;

impl AssetSource for Assets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        let data: &'static [u8] = match path {
            "brand/mark.svg" => include_bytes!("../../brand/arca-monolito.svg"),
            "icons/clipboard-paste.svg" => include_bytes!("../assets/icons/clipboard-paste.svg"),
            "icons/file-output.svg" => include_bytes!("../assets/icons/file-output.svg"),
            "icons/lock.svg" => include_bytes!("../assets/icons/lock.svg"),
            "icons/scissors.svg" => include_bytes!("../assets/icons/scissors.svg"),
            "icons/trash-2.svg" => include_bytes!("../assets/icons/trash-2.svg"),
            "icons/house.svg" => include_bytes!("../assets/icons/house.svg"),
            "icons/download.svg" => include_bytes!("../assets/icons/download.svg"),
            "icons/image.svg" => include_bytes!("../assets/icons/image.svg"),
            "icons/music.svg" => include_bytes!("../assets/icons/music.svg"),
            "icons/film.svg" => include_bytes!("../assets/icons/film.svg"),
            "icons/pin.svg" => include_bytes!("../assets/icons/pin.svg"),
            "icons/monitor.svg" => include_bytes!("../assets/icons/monitor.svg"),
            _ => return gpui::AssetSource::load(&gpui_kit_assets::Assets, path),
        };
        Ok(Some(Cow::Borrowed(data)))
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        let mut assets = gpui::AssetSource::list(&gpui_kit_assets::Assets, path)?;
        assets.extend(
            CUSTOM
                .into_iter()
                .filter(|asset| asset.starts_with(path))
                .map(Into::into),
        );
        Ok(assets)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn brand_and_custom_icons_load_alongside_the_kit() {
        let assets = Assets;
        for path in CUSTOM {
            let data = assets.load(path).unwrap().unwrap();
            assert!(data.starts_with(b"<svg"), "invalid SVG asset: {path}");
        }
        assert!(assets.load("icons/copy.svg").unwrap().is_some());
        assert_eq!(
            assets.list("brand/").unwrap(),
            vec![SharedString::from("brand/mark.svg")]
        );
    }

    #[test]
    fn brand_fonts_are_embedded() {
        assert!(FONT_FILES[0].starts_with(b"\x00\x01\x00\x00"));
        assert!(FONT_FILES[1].starts_with(b"OTTO"));
    }
}
