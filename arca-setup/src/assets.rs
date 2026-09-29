use gpui::{AssetSource, Result, SharedString};
use std::borrow::Cow;

macro_rules! bundle {
    ($($path:literal),+ $(,)?) => {
        const PATHS: &[&str] = &[$($path),+];

        fn bytes(path: &str) -> Option<&'static [u8]> {
            match path {
                $($path => Some(include_bytes!(concat!("../assets/", $path)).as_slice()),)+
                _ => None,
            }
        }
    };
}

bundle!(
    "instalador-fondo-bienvenida.svg",
    "instalador-fondo-progreso.svg",
    "instalador-fondo-listo.svg",
    "logo-horizontal.svg",
    "mark-sm.svg",
    "mark-lg.svg",
    "icons/arrow-right.svg",
    "icons/check.svg",
    "icons/chevron-left.svg",
    "icons/folder.svg",
    "icons/loader.svg",
    "icons/minus.svg",
    "icons/x.svg",
);

pub const FONT_FILES: [&[u8]; 2] = [
    include_bytes!("../assets/fonts/Sora-SemiBold.ttf").as_slice(),
    include_bytes!("../assets/fonts/Inter-Regular.otf").as_slice(),
];

pub struct Assets;

impl AssetSource for Assets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        Ok(bytes(path).map(Cow::Borrowed))
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        Ok(PATHS
            .iter()
            .filter(|asset| asset.starts_with(path))
            .map(|asset| SharedString::from(*asset))
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_bundled_asset_is_an_svg() {
        for path in PATHS {
            let data = Assets.load(path).unwrap().unwrap();
            assert!(data.starts_with(b"<svg"), "not an SVG: {path}");
        }
    }

    #[test]
    fn a_missing_asset_is_none() {
        assert!(Assets.load("nope.svg").unwrap().is_none());
    }
}
