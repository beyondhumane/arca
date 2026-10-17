use eframe::egui;

macro_rules! bundle {
    ($($path:literal),+ $(,)?) => {
        #[cfg(test)]
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
);

pub const FONT_FILES: [&[u8]; 2] = [
    include_bytes!("../assets/fonts/Sora-SemiBold.ttf").as_slice(),
    include_bytes!("../assets/fonts/Inter-Regular.otf").as_slice(),
];

pub const ICON: &[u8] = include_bytes!("../../brand/arca-monolito-256.png");

pub fn image(path: &'static str) -> egui::Image<'static> {
    egui::Image::from_bytes(format!("bytes://{path}"), bytes(path).unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_bundled_picture_is_an_svg_with_content() {
        for path in PATHS {
            let data = bytes(path).expect("bundled");
            assert!(String::from_utf8_lossy(data).contains("<svg"), "{path}");
        }
        assert!(bytes("missing.svg").is_none());
    }

    #[test]
    fn the_window_icon_is_a_png() {
        assert!(ICON.starts_with(b"\x89PNG"));
    }
}
