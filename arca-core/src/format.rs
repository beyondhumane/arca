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
    TarBz2,
    Bz2,
    TarZst,
    Zst,
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
    Cpio,
    Ar,
    Deb,
    Rpm,
    Cab,
    Lzh,
    Vhd,
    Vhdx,
    Vmdk,
    Dmg,
    Img,
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
    TarBz2,
    Bz2,
    TarZst,
    Zst,
    Rar,
    Iso,
    /// The read-only formats of arca-extra: CPIO, AR, DEB, RPM, CAB and LZH.
    Extra,
    /// The disk images of arca-disk: VHD, VHDX, VMDK, DMG and raw images.
    Disk,
}

/// Whether a desktop install offers to open a format with Arca. It never makes
/// Arca the default on its own: the user picks it in the installer, and the
/// system's default stays where it was.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum Association {
    /// Offered with the rest of the archive types.
    Offered,
    /// Offered only when the user asks for it: disc images usually belong to
    /// the system's own mounter.
    OnRequest,
    /// Never claimed. ZIP-based documents and packages, CBR comics, DEB and
    /// RPM packages and static libraries belong to the applications and
    /// package managers made for them, and Arca only reads them. Disk images
    /// belong to the system that mounts them: Windows mounts VHD, VHDX and
    /// IMG, macOS mounts DMG, and VMDK belongs to the hypervisors.
    Never,
}

impl Format {
    /// Formats whose existing archives Arca can rewrite: add, remove, rename
    /// and password changes. RAR is not one of them.
    pub const WRITABLE: [Self; 10] = [
        Self::Zip,
        Self::Tar,
        Self::TarGz,
        Self::TarXz,
        Self::SevenZ,
        Self::Xz,
        Self::TarBz2,
        Self::Bz2,
        Self::TarZst,
        Self::Zst,
    ];

    /// Formats a new archive can be created in. A superset of [`WRITABLE`]:
    /// RAR can be created from scratch but never modified afterwards, and
    /// CBR stays a reading format even though it shares the RAR container.
    /// Like the rest of this type it describes the format, not the build;
    /// clients that compile without the `rar` feature filter RAR themselves.
    pub const CREATABLE: [Self; 11] = [
        Self::Zip,
        Self::Tar,
        Self::TarGz,
        Self::TarXz,
        Self::SevenZ,
        Self::Xz,
        Self::TarBz2,
        Self::Bz2,
        Self::TarZst,
        Self::Zst,
        Self::Rar,
    ];

    /// Every recognized name suffix, longest first where one ends another.
    pub const SUFFIXES: [(&'static str, Self); 43] = [
        ("zip", Self::Zip),
        ("7z", Self::SevenZ),
        ("tar.gz", Self::TarGz),
        ("tgz", Self::TarGz),
        ("tar.xz", Self::TarXz),
        ("txz", Self::TarXz),
        ("xz", Self::Xz),
        ("tar.bz2", Self::TarBz2),
        ("tbz2", Self::TarBz2),
        ("tbz", Self::TarBz2),
        ("bz2", Self::Bz2),
        ("tar.zst", Self::TarZst),
        ("tzst", Self::TarZst),
        ("zst", Self::Zst),
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
        ("cpio", Self::Cpio),
        ("ar", Self::Ar),
        ("a", Self::Ar),
        ("deb", Self::Deb),
        ("udeb", Self::Deb),
        ("rpm", Self::Rpm),
        ("cab", Self::Cab),
        ("lzh", Self::Lzh),
        ("lha", Self::Lzh),
        ("vhd", Self::Vhd),
        ("vhdx", Self::Vhdx),
        ("vmdk", Self::Vmdk),
        ("dmg", Self::Dmg),
        ("img", Self::Img),
    ];

    pub fn can_write(self) -> bool {
        Self::WRITABLE.contains(&self)
    }

    pub fn can_create(self) -> bool {
        Self::CREATABLE.contains(&self)
    }

    /// For a single compressed file such as `.xz`, the format that compresses
    /// a TAR the same way. `None` for everything else.
    pub fn stream_tar(self) -> Option<Self> {
        match self {
            Self::Xz => Some(Self::TarXz),
            Self::Bz2 => Some(Self::TarBz2),
            Self::Zst => Some(Self::TarZst),
            _ => None,
        }
    }

    pub fn association(self) -> Association {
        match self {
            Self::Zip
            | Self::SevenZ
            | Self::Tar
            | Self::TarGz
            | Self::TarXz
            | Self::Xz
            | Self::TarBz2
            | Self::Bz2
            | Self::TarZst
            | Self::Zst
            | Self::Rar
            | Self::Cpio
            | Self::Cab
            | Self::Lzh => Association::Offered,
            Self::Iso => Association::OnRequest,
            Self::Cbr
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
            | Self::Ipa
            | Self::Ar
            | Self::Deb
            | Self::Rpm
            | Self::Vhd
            | Self::Vhdx
            | Self::Vmdk
            | Self::Dmg
            | Self::Img => Association::Never,
        }
    }

    /// The freedesktop shared-mime-info types for a format Arca offers to
    /// open, empty for the rest. Several names where shared-mime-info renamed
    /// the type between versions, so that old and new systems both match.
    pub fn mime_types(self) -> &'static [&'static str] {
        match self {
            Self::Zip => &["application/zip"],
            Self::SevenZ => &["application/x-7z-compressed"],
            Self::Tar => &["application/x-tar"],
            Self::TarGz => &["application/x-compressed-tar"],
            Self::TarXz => &["application/x-xz-compressed-tar"],
            Self::Xz => &["application/x-xz"],
            Self::TarBz2 => &[
                "application/x-bzip2-compressed-tar",
                "application/x-bzip-compressed-tar",
                "application/x-bzip1-compressed-tar",
            ],
            Self::Bz2 => &["application/x-bzip2", "application/x-bzip"],
            Self::TarZst => &["application/x-zstd-compressed-tar"],
            Self::Zst => &["application/zstd"],
            Self::Rar => &["application/vnd.rar", "application/x-rar"],
            Self::Iso => &["application/x-cd-image", "application/vnd.efi.iso"],
            Self::Cpio => &["application/x-cpio"],
            Self::Cab => &["application/vnd.ms-cab-compressed"],
            Self::Lzh => &["application/x-lha", "application/x-lzh-compressed"],
            _ => &[],
        }
    }

    /// The final extension of every suffix of a format a desktop install
    /// offers, each once, with the format it names. Windows and Finder match
    /// on the final extension alone, so `.tar.gz` is `.gz` to them; an
    /// extension that is also a whole suffix names that format, so `.xz` is
    /// XZ rather than TAR.XZ.
    pub fn desktop_extensions() -> Vec<(&'static str, Self)> {
        let mut found: Vec<(&'static str, Self)> = Vec::new();
        for (suffix, format) in Self::SUFFIXES {
            if format.association() == Association::Never {
                continue;
            }
            let last = suffix.rsplit('.').next().unwrap_or(suffix);
            match found.iter_mut().find(|(ext, _)| *ext == last) {
                Some(entry) if suffix == last => entry.1 = format,
                Some(_) => {}
                None => found.push((last, format)),
            }
        }
        found
    }

    pub fn container(self) -> Container {
        match self {
            Self::SevenZ => Container::SevenZ,
            Self::Tar => Container::Tar,
            Self::TarGz => Container::TarGz,
            Self::TarXz => Container::TarXz,
            Self::Xz => Container::Xz,
            Self::TarBz2 => Container::TarBz2,
            Self::Bz2 => Container::Bz2,
            Self::TarZst => Container::TarZst,
            Self::Zst => Container::Zst,
            Self::Rar | Self::Cbr => Container::Rar,
            Self::Iso => Container::Iso,
            Self::Cpio | Self::Ar | Self::Deb | Self::Rpm | Self::Cab | Self::Lzh => {
                Container::Extra
            }
            Self::Vhd | Self::Vhdx | Self::Vmdk | Self::Dmg | Self::Img => Container::Disk,
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
            Self::TarBz2 => "TAR.BZ2",
            Self::Bz2 => "BZ2",
            Self::TarZst => "TAR.ZST",
            Self::Zst => "ZST",
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
            Self::Cpio => "CPIO",
            Self::Ar => "AR",
            Self::Deb => "DEB",
            Self::Rpm => "RPM",
            Self::Cab => "CAB",
            Self::Lzh => "LZH",
            Self::Vhd => "VHD",
            Self::Vhdx => "VHDX",
            Self::Vmdk => "VMDK",
            Self::Dmg => "DMG",
            Self::Img => "IMG",
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
                Container::Rar | Container::Extra => {
                    format!("{label} is read-only; creating or modifying {label} archives is disabled")
                }
                Container::Iso | Container::Disk => {
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

    /// The read-only format a file starting with `magic` is in, for a file
    /// whose name says nothing: CPIO, AR, DEB, RPM, CAB, LZH, or a disk image
    /// that starts with its signature: a dynamic VHD, a VHDX or a VMDK. 32
    /// bytes are enough to tell; a DEB is an AR whose first member is
    /// `debian-binary`. A fixed VHD, a DMG and a raw image keep nothing
    /// telling at the start, so only their names say what they are.
    pub fn sniff(magic: &[u8]) -> Option<Self> {
        let at = |from: usize, bytes: &[u8]| magic.get(from..from + bytes.len()) == Some(bytes);
        if at(0, b"!<arch>\n") {
            Some(if at(8, b"debian-binary") {
                Self::Deb
            } else {
                Self::Ar
            })
        } else if at(0, &[0xed, 0xab, 0xee, 0xdb]) {
            Some(Self::Rpm)
        } else if at(0, b"MSCF\0\0\0\0") {
            Some(Self::Cab)
        } else if at(0, b"070701") || at(0, b"070702") || at(0, b"070707") {
            Some(Self::Cpio)
        } else if magic.len() >= 7
            && magic[2] == b'-'
            && magic[6] == b'-'
            && matches!(&magic[3..5], b"lh" | b"lz" | b"pm")
        {
            Some(Self::Lzh)
        } else if at(0, b"conectix") {
            Some(Self::Vhd)
        } else if at(0, b"vhdxfile") {
            Some(Self::Vhdx)
        } else if at(0, b"KDMV") || at(0, b"# Disk DescriptorFile") {
            Some(Self::Vmdk)
        } else {
            None
        }
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
    fn bzip2_and_zstd_suffixes_mirror_xz() {
        for (name, format, stem) in [
            ("backup.tar.bz2", Format::TarBz2, "backup"),
            ("backup.TBZ2", Format::TarBz2, "backup"),
            ("backup.tbz", Format::TarBz2, "backup"),
            ("notes.txt.bz2", Format::Bz2, "notes.txt"),
            ("backup.Tar.Zst", Format::TarZst, "backup"),
            ("backup.tzst", Format::TarZst, "backup"),
            ("notes.txt.ZST", Format::Zst, "notes.txt"),
        ] {
            assert_eq!(Format::split_name(name), Some((stem, format)));
            assert!(format.can_write() && format.can_create());
        }
        for (single, tar, ext, label) in [
            (Format::Xz, Format::TarXz, "xz", "XZ"),
            (Format::Bz2, Format::TarBz2, "bz2", "BZ2"),
            (Format::Zst, Format::TarZst, "zst", "ZST"),
        ] {
            assert_eq!(single.stream_tar(), Some(tar));
            assert_eq!(tar.stream_tar(), None);
            assert_eq!(single.extension(), ext);
            assert_eq!(tar.extension(), format!("tar.{ext}"));
            assert_eq!(single.label(), label);
            assert_eq!(tar.label(), format!("TAR.{label}"));
        }
        assert_eq!(Format::Bz2.container(), Container::Bz2);
        assert_eq!(Format::TarZst.container(), Container::TarZst);
        assert_eq!(Format::TarGz.stream_tar(), None);
        assert_eq!(Format::split_name("archive.bz"), None);
    }

    #[test]
    fn desktops_are_offered_archives_and_never_zip_containers() {
        let offered: Vec<&str> = Format::desktop_extensions()
            .iter()
            .map(|(ext, _)| *ext)
            .collect();
        assert_eq!(
            offered,
            [
                "zip", "7z", "gz", "tgz", "xz", "txz", "bz2", "tbz2", "tbz", "zst", "tzst", "tar",
                "rar", "iso", "cpio", "cab", "lzh", "lha"
            ]
        );
        let named: Vec<(&str, Format)> = Format::desktop_extensions();
        assert!(named.contains(&("gz", Format::TarGz)));
        assert!(named.contains(&("xz", Format::Xz)));
        assert!(named.contains(&("bz2", Format::Bz2)));
        assert!(named.contains(&("zst", Format::Zst)));
        for (suffix, format) in Format::SUFFIXES {
            let claimed = format.association() != Association::Never;
            assert_eq!(claimed, !format.mime_types().is_empty(), "{suffix}");
            if format.container() == Container::Zip && format != Format::Zip {
                assert!(!claimed, "{suffix} belongs to its own application");
            }
        }
        assert_eq!(Format::Cbr.association(), Association::Never);
        assert_eq!(Format::Iso.association(), Association::OnRequest);
        assert_eq!(Format::Rar.association(), Association::Offered);
    }

    #[test]
    fn the_extra_formats_are_read_only_and_packages_are_never_claimed() {
        for (name, format, label) in [
            ("initrd.cpio", Format::Cpio, "CPIO"),
            ("libz.a", Format::Ar, "AR"),
            ("objects.ar", Format::Ar, "AR"),
            ("hello_1.0_amd64.deb", Format::Deb, "DEB"),
            ("installer.udeb", Format::Deb, "DEB"),
            ("hello-1.0-1.x86_64.rpm", Format::Rpm, "RPM"),
            ("driver.CAB", Format::Cab, "CAB"),
            ("game.lzh", Format::Lzh, "LZH"),
            ("game.LHA", Format::Lzh, "LZH"),
        ] {
            assert_eq!(Format::detect(Path::new(name)), Some(format), "{name}");
            assert_eq!(format.container(), Container::Extra);
            assert_eq!(format.label(), label);
            assert!(!format.can_write() && !format.can_create());
            assert!(format
                .read_only()
                .to_string()
                .contains(&format!("{label} is read-only")));
        }
        for format in [Format::Ar, Format::Deb, Format::Rpm] {
            assert_eq!(format.association(), Association::Never);
        }
        for format in [Format::Cpio, Format::Cab, Format::Lzh] {
            assert_eq!(format.association(), Association::Offered);
        }
        assert_eq!(Format::split_name("x.rar"), Some(("x", Format::Rar)));
        assert_eq!(Format::split_name("x.tar"), Some(("x", Format::Tar)));
        assert_eq!(Format::split_name("data"), None);
    }

    #[test]
    fn the_extra_formats_are_told_by_their_first_bytes() {
        let mut deb = b"!<arch>\ndebian-binary   ".to_vec();
        deb.resize(32, b' ');
        assert_eq!(Format::sniff(&deb), Some(Format::Deb));
        assert_eq!(
            Format::sniff(b"!<arch>\n/               "),
            Some(Format::Ar)
        );
        assert_eq!(
            Format::sniff(&[0xed, 0xab, 0xee, 0xdb, 3, 0]),
            Some(Format::Rpm)
        );
        assert_eq!(Format::sniff(b"MSCF\0\0\0\0\x10"), Some(Format::Cab));
        assert_eq!(Format::sniff(b"MSCF\x01\0\0\0"), None);
        for magic in [&b"070701"[..], b"070702", b"070707"] {
            assert_eq!(Format::sniff(magic), Some(Format::Cpio));
        }
        assert_eq!(Format::sniff(b"\x1f\x20-lh5-"), Some(Format::Lzh));
        assert_eq!(Format::sniff(b"\x1f\x20-pm2-"), Some(Format::Lzh));
        assert_eq!(Format::sniff(b"\x1f\x20-xx5-"), None);
        assert_eq!(Format::sniff(b"PK\x03\x04"), None);
        assert_eq!(Format::sniff(b""), None);
    }

    #[test]
    fn disk_images_are_read_only_never_claimed_and_sniffed_by_signature() {
        for (name, format, label) in [
            ("server.vhd", Format::Vhd, "VHD"),
            ("Server.VHDX", Format::Vhdx, "VHDX"),
            ("guest-disk1.vmdk", Format::Vmdk, "VMDK"),
            ("App-1.0.dmg", Format::Dmg, "DMG"),
            ("sdcard.img", Format::Img, "IMG"),
        ] {
            assert_eq!(Format::detect(Path::new(name)), Some(format), "{name}");
            assert_eq!(format.container(), Container::Disk);
            assert_eq!(format.label(), label);
            assert_eq!(format.association(), Association::Never);
            assert!(format.mime_types().is_empty());
            assert!(!format.can_write() && !format.can_create());
            assert!(format
                .read_only()
                .to_string()
                .contains(&format!("{label} is read-only")));
        }
        assert_eq!(Format::split_name("a.vhdx"), Some(("a", Format::Vhdx)));
        assert_eq!(Format::sniff(b"conectix\0\0\0\x02"), Some(Format::Vhd));
        assert_eq!(Format::sniff(b"vhdxfile"), Some(Format::Vhdx));
        assert_eq!(Format::sniff(b"KDMV\x01\0\0\0"), Some(Format::Vmdk));
        assert_eq!(
            Format::sniff(b"# Disk DescriptorFile\nversion=1"),
            Some(Format::Vmdk)
        );
        assert_eq!(Format::sniff(b"koly"), None);
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
