use crate::Error;
use std::path::Path;

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum Format {
    Zip,
    SevenZ,
    Tar,
    TarGz,
    Rar,
    Cbr,
    Apk,
    Aar,
    Jar,
    War,
    Ear,
    Epub,
    Cbz,
    Xpi,
    Whl,
    Nupkg,
    Ipa,
}

/// The reader that opens a format. ZIP-based containers such as APK or EPUB
/// share the ZIP reader but keep their own `Format` so they stay read-only.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum Container {
    Zip,
    SevenZ,
    Tar,
    TarGz,
    Rar,
}

impl Format {
    pub const WRITABLE: [Self; 4] = [Self::Zip, Self::Tar, Self::TarGz, Self::SevenZ];

    /// Every recognized name suffix, longest first where one ends another.
    pub const SUFFIXES: [(&'static str, Self); 18] = [
        ("zip", Self::Zip),
        ("7z", Self::SevenZ),
        ("tar.gz", Self::TarGz),
        ("tgz", Self::TarGz),
        ("tar", Self::Tar),
        ("rar", Self::Rar),
        ("cbr", Self::Cbr),
        ("apk", Self::Apk),
        ("aar", Self::Aar),
        ("jar", Self::Jar),
        ("war", Self::War),
        ("ear", Self::Ear),
        ("epub", Self::Epub),
        ("cbz", Self::Cbz),
        ("xpi", Self::Xpi),
        ("whl", Self::Whl),
        ("nupkg", Self::Nupkg),
        ("ipa", Self::Ipa),
    ];

    pub fn can_write(self) -> bool {
        Self::WRITABLE.contains(&self)
    }

    pub fn container(self) -> Container {
        match self {
            Self::SevenZ => Container::SevenZ,
            Self::Tar => Container::Tar,
            Self::TarGz => Container::TarGz,
            Self::Rar | Self::Cbr => Container::Rar,
            Self::Zip
            | Self::Apk
            | Self::Aar
            | Self::Jar
            | Self::War
            | Self::Ear
            | Self::Epub
            | Self::Cbz
            | Self::Xpi
            | Self::Whl
            | Self::Nupkg
            | Self::Ipa => Container::Zip,
        }
    }

    pub fn extension(self) -> &'static str {
        Self::SUFFIXES
            .iter()
            .find(|(_, format)| *format == self)
            .map_or("zip", |(suffix, _)| suffix)
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Zip => "ZIP",
            Self::SevenZ => "7z",
            Self::Tar => "TAR",
            Self::TarGz => "TAR.GZ",
            Self::Rar => "RAR",
            Self::Cbr => "CBR",
            Self::Apk => "APK",
            Self::Aar => "AAR",
            Self::Jar => "JAR",
            Self::War => "WAR",
            Self::Ear => "EAR",
            Self::Epub => "EPUB",
            Self::Cbz => "CBZ",
            Self::Xpi => "XPI",
            Self::Whl => "WHL",
            Self::Nupkg => "NUPKG",
            Self::Ipa => "IPA",
        }
    }

    /// The error for any attempt to create or modify an archive in this format.
    pub fn read_only(self) -> Error {
        let label = self.label();
        Error::Unsupported(if self.container() == Container::Rar {
            format!("{label} is read-only; creating or modifying {label} archives is disabled")
        } else {
            format!(
                "{label} is read-only in Arca; rewriting it could break its signature or layout, \
                 so creating or modifying {label} files is disabled"
            )
        })
    }

    /// The format a file name ends in and the name without that suffix.
    pub fn split_name(name: &str) -> Option<(&str, Self)> {
        let lower = name.to_ascii_lowercase();
        Self::SUFFIXES.iter().find_map(|(suffix, format)| {
            let stem = lower.strip_suffix(suffix)?.strip_suffix('.')?;
            Some((&name[..stem.len()], *format))
        })
    }

    pub fn detect(path: &Path) -> Option<Self> {
        Self::split_name(&path.to_string_lossy()).map(|(_, format)| format)
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
        assert_eq!(Format::SevenZ.container(), Container::SevenZ);
        assert!(Format::SevenZ.can_write());
        assert!(!Format::WRITABLE.contains(&Format::Rar));
    }

    #[test]
    fn rar_is_readable_but_never_a_creation_format() {
        for name in ["archive.RAR", "archive.part1.rar"] {
            assert_eq!(Format::detect(Path::new(name)), Some(Format::Rar));
        }
        assert_eq!(Format::detect(Path::new("comic.CbR")), Some(Format::Cbr));
        for format in [Format::Rar, Format::Cbr] {
            assert!(!format.can_write());
            assert_eq!(format.container(), Container::Rar);
        }
        assert!(Format::WRITABLE.iter().all(|format| format.can_write()));
        assert_eq!(Format::detect(Path::new("x.tgz")), Some(Format::TarGz));
    }

    #[test]
    fn zip_containers_are_detected_read_only_and_keep_their_label() {
        let cases = [
            ("app.apk", Format::Apk, "APK"),
            ("lib.aar", Format::Aar, "AAR"),
            ("tool.jar", Format::Jar, "JAR"),
            ("site.war", Format::War, "WAR"),
            ("bundle.ear", Format::Ear, "EAR"),
            ("book.epub", Format::Epub, "EPUB"),
            ("comic.cbz", Format::Cbz, "CBZ"),
            ("addon.xpi", Format::Xpi, "XPI"),
            ("pkg-1.0-py3-none-any.whl", Format::Whl, "WHL"),
            ("Package.1.0.0.nupkg", Format::Nupkg, "NUPKG"),
            ("App.ipa", Format::Ipa, "IPA"),
        ];
        for (name, format, label) in cases {
            assert_eq!(Format::detect(Path::new(name)), Some(format));
            assert_eq!(
                Format::detect(Path::new(&name.to_ascii_uppercase())),
                Some(format)
            );
            assert_eq!(format.container(), Container::Zip);
            assert!(!format.can_write());
            assert_eq!(format.label(), label);
            assert_eq!(format.extension(), label.to_ascii_lowercase());
            assert!(format.read_only().to_string().contains("read-only"));
        }
        assert_eq!(Format::Zip.container(), Container::Zip);
        assert!(Format::Zip.can_write());
    }

    #[test]
    fn split_name_keeps_the_original_case_and_needs_a_dot() {
        assert_eq!(
            Format::split_name("My.Book.EPUB"),
            Some(("My.Book", Format::Epub))
        );
        assert_eq!(
            Format::split_name("backup.TAR.GZ"),
            Some(("backup", Format::TarGz))
        );
        assert_eq!(
            Format::split_name("ñandú.zip"),
            Some(("ñandú", Format::Zip))
        );
        assert_eq!(Format::split_name("unzip"), None);
        assert_eq!(Format::split_name("notes.txt"), None);
        assert_eq!(Format::split_name(".jar"), Some(("", Format::Jar)));
    }
}
