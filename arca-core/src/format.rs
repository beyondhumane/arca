use std::path::Path;

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum Format {
    Zip,
    SevenZ,
    Tar,
    TarGz,
    Rar,
}

impl Format {
    pub const WRITABLE: [Self; 4] = [Self::Zip, Self::Tar, Self::TarGz, Self::SevenZ];

    pub fn can_write(self) -> bool {
        Self::WRITABLE.contains(&self)
    }

    pub fn extension(self) -> &'static str {
        match self {
            Self::Zip => "zip",
            Self::SevenZ => "7z",
            Self::Tar => "tar",
            Self::TarGz => "tar.gz",
            Self::Rar => "rar",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Zip => "ZIP",
            Self::SevenZ => "7z",
            Self::Tar => "TAR",
            Self::TarGz => "TAR.GZ",
            Self::Rar => "RAR (experimental, read-only)",
        }
    }

    pub fn detect(path: &Path) -> Option<Self> {
        let name = path.to_string_lossy().to_ascii_lowercase();
        if name.ends_with(".zip") {
            Some(Self::Zip)
        } else if name.ends_with(".7z") {
            Some(Self::SevenZ)
        } else if name.ends_with(".tar.gz") || name.ends_with(".tgz") {
            Some(Self::TarGz)
        } else if name.ends_with(".tar") {
            Some(Self::Tar)
        } else if name.ends_with(".rar") || name.ends_with(".cbr") {
            Some(Self::Rar)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sevenz_is_a_shared_writable_format() {
        assert_eq!(
            Format::detect(Path::new("archive.7Z")),
            Some(Format::SevenZ)
        );
        assert_eq!(Format::SevenZ.extension(), "7z");
        assert_eq!(Format::SevenZ.label(), "7z");
        assert!(Format::SevenZ.can_write());
        assert!(!Format::WRITABLE.contains(&Format::Rar));
    }

    #[test]
    fn rar_is_readable_but_never_a_creation_format() {
        for name in ["archive.RAR", "comic.CbR", "archive.part1.rar"] {
            assert_eq!(Format::detect(Path::new(name)), Some(Format::Rar));
        }
        assert!(!Format::Rar.can_write());
        assert!(Format::WRITABLE.iter().all(|format| format.can_write()));
        assert_eq!(Format::detect(Path::new("x.tgz")), Some(Format::TarGz));
    }
}
