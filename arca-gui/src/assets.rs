pub const FONT_FILES: [&[u8]; 2] = [
    include_bytes!("../../arca-setup/assets/fonts/Sora-SemiBold.ttf").as_slice(),
    include_bytes!("../../arca-setup/assets/fonts/Inter-Regular.otf").as_slice(),
];

pub const MARK: &[u8] = include_bytes!("../../brand/arca-monolito.svg");
pub const ICON: &[u8] = include_bytes!("../../brand/arca-monolito-256.png");
