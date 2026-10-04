use crate::Error;
use std::path::Path;

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum Format {
    Zip,
    SevenZ,
    Tar,
    TarGz,
    TarXz,
    Xz,
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
    Iso,
}

/// The reader that opens a format. ZIP-based containers such as APK or EPUB
/// share the ZIP reader but keep their own `Format` so they stay read-only.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum Container {
    Zip,
    SevenZ,
    Tar,
    TarGz,
    TarXz,
    /// A single compressed stream. Its one entry is named after the file.
    Xz,
    Rar,
    Iso,
}

impl Format {
    /// Formats whose existing archives Arca can rewrite: add, remove, rename
    /// and password changes. RAR is not one of them.
    pub const WRITABLE: [Self; 6] = [
        Self::Zip,
        Self::Tar,
        Self::TarGz,
        Self::TarXz,
        Self::SevenZ,
        Self::Xz,
    ];

    /// Formats a new archive can be created in. A superset of [`WRITABLE`]:
    /// RAR can be created from scratch but never modified afterwards, and
    /// CBR stays a reading format even though it shares the RAR container.
    /// Like the rest of this type it describes the format, not the build;
    /// clients that compile without the `rar` feature filter RAR themselves.
    pub const CREATABLE: [Self; 7] = [
        Self::Zip,
        Self::Tar,
        Self::TarGz,
        Self::TarXz,
        Self::SevenZ,
        Self::Xz,
        Self::Rar,
    ];

    /// Every recognized name suffix, longest first where one ends another.
    pub const SUFFIXES: [(&'static str, Self); 22] = [
        ("zip", Self::Zip),
        ("7z", Self::SevenZ),
        ("tar.gz", Self::TarGz),
        ("tgz", Self::TarGz),
        ("tar.xz", Self::TarXz),
        ("txz", Self::TarXz),
        ("xz", Self::Xz),
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
        ("iso", Self::Iso),
    ];

    pub fn can_write(self) -> bool {
        Self::WRITABLE.contains(&self)
    }

    pub fn can_create(self) -> bool {
        Self::CREATABLE.contains(&self)
    }

    pub fn container(self) -> Container {
        match self {
            Self::SevenZ => Container::SevenZ,
            Self::Tar => Container::Tar,
            Self::TarGz => Container::TarGz,
            Self::TarXz => Container::TarXz,
            Self::Xz => Container::Xz,
            Self::Rar | Self::Cbr => Container::Rar,
            Self::Iso => Container::Iso,
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
            Self::TarXz => "TAR.XZ",
            Self::Xz => "XZ",
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
            Self::Iso => "ISO",
        }
    }

    /// The error for any attempt to modify an archive in this format, or to
    /// create one when the format cannot be created either.
    pub fn read_only(self) -> Error {
        let label = self.label();
        Error::Unsupported(if self == Self::Rar {
            format!(
                "{label} archives can be created but not modified in Arca; \
                 adding, removing, renaming or re-encrypting entries of an existing \
                 {label} archive is disabled"
            )
        } else {
            match self.container() {
                Container::Rar => {
                    format!("{label} is read-only; creating or modifying {label} archives is disabled")
                }
                Container::Iso => {
                    format!("{label} is read-only; creating or modifying {label} images is disabled")
                }
                _ => format!(
                    "{label} is read-only in Arca; rewriting it could break its signature or layout, \
                     so creating or modifying {label} files is disabled"
                ),
            }
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
        Self::split_name(&path.to_string_lossy())
            .map(|(_, format)| format)
            .or_else(|| is_rar_volume(path).then_some(Self::Rar))
    }
}

fn is_rar_volume(path: &Path) -> bool {
    let Some(ext) = path.extension().and_then(|ext| ext.to_str()) else {
        return false;
    };
    let ext = ext.as_bytes();
    ext.len() == 3
        && (b'r'..=b'z').contains(&ext[0].to_ascii_lowercase())
        && ext[1..].iter().all(u8::is_ascii_digit)
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
        assert!(Format::SevenZ.can_create());
        assert!(!Format::WRITABLE.contains(&Format::Rar));
    }

    #[test]
    fn creatable_formats_extend_the_writable_ones_with_rar_only() {
        assert!(Format::WRITABLE.iter().all(|format| format.can_create()));
        assert!(Format::Rar.can_create());
        assert!(!Format::Rar.can_write());
        assert!(!Format::Cbr.can_create());
        let extra: Vec<_> = Format::CREATABLE
            .iter()
            .filter(|format| !Format::WRITABLE.contains(format))
            .collect();
        assert_eq!(extra, [&Format::Rar]);
        let message = Format::Rar.read_only().to_string();
        assert!(message.contains("can be created"));
        assert!(message.contains("not modified"));
        let message = Format::Cbr.read_only().to_string();
        assert!(message.contains("creating or modifying CBR"));
    }

    #[test]
    fn rar_is_readable_but_never_a_mutation_format() {
        for name in [
            "archive.RAR",
            "archive.part1.rar",
            "archive.part04.rar",
            "archive.r00",
            "archive.R99",
            "archive.s00",
            "archive.z99",
        ] {
            assert_eq!(Format::detect(Path::new(name)), Some(Format::Rar));
        }
        assert_eq!(Format::detect(Path::new("comic.CbR")), Some(Format::Cbr));
        for format in [Format::Rar, Format::Cbr] {
            assert!(!format.can_write());
            assert_eq!(format.container(), Container::Rar);
        }
        assert!(Format::WRITABLE.iter().all(|format| format.can_write()));
        assert_eq!(Format::detect(Path::new("x.tgz")), Some(Format::TarGz));
        for name in ["x.r0", "x.r000", "x.rxx", "x.q00", "x.rev"] {
            assert_eq!(Format::detect(Path::new(name)), None);
        }
    }

    #[test]
    fn iso_is_readable_but_never_a_creation_format() {
        for name in ["image.iso", "DISC.ISO", "ubuntu-24.04-desktop-amd64.iso"] {
            assert_eq!(Format::detect(Path::new(name)), Some(Format::Iso));
        }
        assert_eq!(Format::Iso.container(), Container::Iso);
        assert_eq!(Format::Iso.extension(), "iso");
        assert!(!Format::Iso.can_write());
        assert!(!Format::Iso.can_create());
        assert!(Format::Iso
            .read_only()
            .to_string()
            .contains("ISO is read-only"));
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
    fn xz_suffixes_are_writable_and_tar_xz_wins_over_xz() {
        for (name, format, stem) in [
            ("backup.tar.xz", Format::TarXz, "backup"),
            ("backup.TAR.XZ", Format::TarXz, "backup"),
            ("backup.txz", Format::TarXz, "backup"),
            ("backup.TxZ", Format::TarXz, "backup"),
            ("notes.txt.xz", Format::Xz, "notes.txt"),
            ("NOTES.XZ", Format::Xz, "NOTES"),
        ] {
            assert_eq!(Format::split_name(name), Some((stem, format)));
            assert!(format.can_write());
        }
        assert_eq!(Format::TarXz.extension(), "tar.xz");
        assert_eq!(Format::Xz.extension(), "xz");
        assert_eq!(Format::TarXz.label(), "TAR.XZ");
        assert_eq!(Format::Xz.label(), "XZ");
        assert_eq!(Format::TarXz.container(), Container::TarXz);
        assert_eq!(Format::Xz.container(), Container::Xz);
        assert_eq!(Format::split_name("archive.lzma"), None);
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
