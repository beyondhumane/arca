use arca_core::extraction::{Conflict, Destination};
use arca_core::{Codec, Container, Error, Format, Level, Result};
use arca_tar::{TarReader, TarWriter};
use arca_zip::{ExtractionReader, ZipArchive};
use clap::{Parser, Subcommand, ValueEnum};
use rayon::prelude::*;
use std::collections::HashSet;
use std::fs::{self, File};
use std::io::{self, BufReader, BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use std::time::Instant;

mod bench;
mod comment;
mod diff;
mod hash;
mod rar;
mod repair;
mod sevenz;

const BUF: usize = 256 * 1024;

#[derive(Parser)]
#[command(
    name = "arca",
    version,
    about = "Fast, safe archiver",
    long_about = "Arca compresses and extracts archives. The container parsers are written in\nsafe Rust: a malformed file produces an error, never memory corruption."
)]
struct Cli {
    #[command(subcommand)]
    command: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    #[command(visible_alias = "c", about = "Create an archive")]
    Create {
        #[arg(
            help = "Output archive (.zip, .7z, .rar, .tar, .tar.gz, .tar.xz, .xz, .tar.bz2, .bz2, .tar.zst, .zst)"
        )]
        out: PathBuf,
        #[arg(required = true, help = "Files or directories to include")]
        inputs: Vec<PathBuf>,
        #[arg(short, long, default_value = "normal", value_parser = clap::value_parser!(Level),
              help = "Compression level: 0-9, or store, fast, normal, best")]
        level: Level,
        #[arg(short, long, value_enum, default_value_t = CodecArg::Auto,
              help = "Compressor. 'auto' uses deflate in .zip, LZMA2 in .7z and the RAR compressor in .rar, which accepts only auto or store. .7z also takes lzma, ppmd, bzip2, deflate and store")]
        codec: CodecArg,
        #[arg(
            short = 'j',
            long,
            default_value_t = 0,
            help = "Threads to use. 0 means every core. .xz and .tar.xz use as many as fit in 2 GiB; .zst and .tar.zst use as many as asked; .bz2 and .tar.bz2 use one; .7z and .rar always use every core their memory budget allows"
        )]
        threads: usize,
        #[arg(
            short = 'p',
            long,
            help = "Encrypt with AES-256 (.zip and .7z). Other tools will ask for it to open the archive"
        )]
        password: Option<String>,
        #[arg(
            long,
            help = "Encrypt file names too (.7z only; requires a nonempty password)"
        )]
        hide_names: bool,
        #[arg(
            short = 'v',
            long,
            value_name = "SIZE",
            value_parser = arca_core::volumes::parse_size,
            help = "Split the archive into parts of SIZE bytes, or k, m or g (KiB, MiB, GiB): out.zip.001, out.zip.002, ... as 7-Zip does. Not for .rar"
        )]
        volume: Option<u64>,
        #[arg(
            long,
            conflicts_with_all = ["volume", "password", "hide_names"],
            help = "Make a self-extracting program (out.exe) instead: the arca-sfx program found next to arca, followed by a ZIP. Run it to choose a folder and extract there"
        )]
        sfx: bool,
        #[arg(
            long,
            requires = "sfx",
            help = "Make the self-extracting program ask and report in the console instead of in windows"
        )]
        console: bool,
    },
    #[command(
        visible_alias = "l",
        about = "List the contents without extracting them"
    )]
    List {
        #[arg(help = "Archive to read")]
        archive: PathBuf,
        #[arg(short, long, help = "Report how long it took")]
        time: bool,
        #[arg(short = 'p', long, help = "Password to read encrypted 7z/RAR headers")]
        password: Option<String>,
    },
    #[command(visible_alias = "x", about = "Extract the contents")]
    Extract {
        #[arg(help = "Archive to extract")]
        archive: PathBuf,
        #[arg(short = 'o', long, default_value = ".", help = "Destination directory")]
        dest: PathBuf,
        #[arg(long, value_enum, default_value_t = OnConflict::Overwrite,
              help = "What to do when the file is already in the destination")]
        on_conflict: OnConflict,
        #[arg(
            short = 'j',
            long,
            default_value_t = 0,
            help = "Threads to use. 0 means every core. Only .zip can go parallel"
        )]
        threads: usize,
        #[arg(short = 'p', long, help = "Password of an encrypted archive")]
        password: Option<String>,
    },
    #[command(visible_alias = "t", about = "Check integrity without writing to disk")]
    Test {
        #[arg(help = "Archive to check")]
        archive: PathBuf,
        #[arg(short = 'p', long, help = "Password of an encrypted archive")]
        password: Option<String>,
    },
    #[command(about = "Print or check checksums of files, in the sha256sum format")]
    Hash {
        #[arg(
            required_unless_present = "check",
            help = "Files or directories (read recursively)"
        )]
        paths: Vec<PathBuf>,
        #[arg(
            short,
            long,
            value_enum,
            help = "Checksum to compute [default: sha256]; with --check, taken from each line unless given"
        )]
        algorithm: Option<hash::Kind>,
        #[arg(
            short,
            long,
            value_name = "SUMS",
            conflicts_with = "paths",
            help = "Verify the files listed in SUMS, a file in the sha256sum format"
        )]
        check: Option<PathBuf>,
    },
    #[command(
        about = "Add, change or remove the password of an existing archive",
        long_about = "Rewrites the archive with a different password, or with none.\n\nThe entries are not compressed again: WinZip AES encrypts the already\ncompressed bytes, so taking the encryption off gives back the same stream\nthat was there before."
    )]
    Password {
        #[arg(help = "Archive to rewrite (.zip)")]
        archive: PathBuf,
        #[arg(short = 'p', long, help = "Current password, if the archive has one")]
        password: Option<String>,
        #[arg(long, help = "New password. Leave it out to remove the encryption")]
        new: Option<String>,
        #[arg(
            short = 'o',
            long,
            help = "Write here instead of replacing the archive in place"
        )]
        out: Option<PathBuf>,
    },
    #[command(
        about = "Show or change the comment of an archive or of one entry",
        long_about = "With no --set, --file or --clear, prints the comment: a ZIP or RAR archive's own,\nor with --entry the one on that entry of a ZIP or a TAR (pax). A TAR has no\ncomment of its own, so it lists the entries that carry one.\n\nOnly ZIP comments can be changed. The archive is written again beside itself,\nevery entry copied unchanged, read back, and then put in place of the old one;\nno password is needed, even for encrypted entries."
    )]
    Comment {
        #[arg(help = "Archive (.zip to change; .zip, .rar or .tar to read)")]
        archive: PathBuf,
        #[arg(long, value_name = "TEXT", conflicts_with_all = ["file", "clear"], help = "Set the comment to TEXT")]
        set: Option<String>,
        #[arg(
            long,
            value_name = "FILE",
            conflicts_with = "clear",
            help = "Set the comment to the contents of FILE (- reads standard input)"
        )]
        file: Option<PathBuf>,
        #[arg(long, help = "Remove the comment")]
        clear: bool,
        #[arg(
            long,
            value_name = "NAME",
            help = "The entry whose comment to show or change, by its path inside the archive"
        )]
        entry: Option<String>,
        #[arg(
            short = 'o',
            long,
            help = "Write here instead of replacing the archive in place"
        )]
        out: Option<PathBuf>,
        #[arg(
            short = 'p',
            long,
            help = "Password of a RAR archive whose headers are encrypted"
        )]
        password: Option<String>,
    },
    #[command(
        about = "Rebuild a damaged archive from what can still be read",
        long_about = "Writes a new archive of the same format; the damaged one is only read.\n\nZIP: scans for local headers, checks every entry it finds and copies the sound\nones unchanged. Encrypted entries get a structural check only: without the\npassword their contents cannot be checked.\n\nTAR (.tar, .tar.gz, .tar.xz, .tar.bz2, .tar.zst): keeps every entry whose header\nchecks out and whose data is all there, and after a damaged header carries on\nat the next valid one. TAR has no checksum over data. Damage inside the\ncompressed stream ends the repair there, with what came before it.\n\n7z: decodes each block on its own and keeps the entries that check out, up to\nthe first fault in their block. A damaged header cannot be repaired.\n\nRAR: rebuilds the archive from its recovery record, which it must have been\nmade with. Single volumes only; damaged headers cannot be repaired."
    )]
    Repair {
        #[arg(help = "Damaged archive")]
        archive: PathBuf,
        #[arg(
            short = 'o',
            long,
            help = "Where to write the rebuilt archive, in the same format"
        )]
        out: PathBuf,
        #[arg(short = 'p', long, help = "Password, for an encrypted 7z or RAR")]
        password: Option<String>,
    },
    #[command(
        about = "Compare two archives, or an archive and a folder",
        long_about = "Prints one line for each name the two sides hold differently:\n  + only in the second    - only in the first\n  M contents differ       T a file on one side, a folder or link on the other\n  m only the modification time differs (with --times)\n\nA folder is compared by what it holds, not by its own name. Contents are\ncompared by size and then by SHA-256, reading the entries without extracting\nthem.\n\nExit status: 0 if both hold the same, 1 if they differ, 2 on an error."
    )]
    Diff {
        #[arg(help = "First archive or folder")]
        a: PathBuf,
        #[arg(help = "Second archive or folder")]
        b: PathBuf,
        #[arg(
            long,
            help = "Compare only sizes, and the CRC32 where both sides store one, without reading contents"
        )]
        quick: bool,
        #[arg(long, help = "Also report files that differ only in modification time")]
        times: bool,
        #[arg(
            short = 'p',
            long,
            help = "Password of the first archive, and of the second unless --password-b is given"
        )]
        password: Option<String>,
        #[arg(long, value_name = "PASSWORD", help = "Password of the second archive")]
        password_b: Option<String>,
    },
    #[command(
        about = "Measure every codec on generated data (--codecs), or the R1 and R2 requirements on an archive"
    )]
    Bench {
        #[arg(
            required_unless_present = "codecs",
            conflicts_with = "codecs",
            help = "Archive to measure the R1 and R2 requirements against"
        )]
        archive: Option<PathBuf>,
        #[arg(
            long,
            help = "Compress and decompress data generated in memory with every codec, on one thread and on --threads, and print the speeds, the ratios and a rating. Nothing is read from or written to the disk"
        )]
        codecs: bool,
        #[arg(
            short = 'j',
            long,
            default_value_t = 0,
            requires = "codecs",
            help = "Threads for the second measurement. 0 means every core"
        )]
        threads: usize,
        #[arg(
            long,
            default_value_t = arca_bench::Options::SECONDS,
            value_parser = bench::parse_seconds,
            requires = "codecs",
            help = "Seconds each measurement keeps repeating its runs; at least one run is always made"
        )]
        seconds: f64,
        #[arg(
            long,
            value_name = "MIB",
            default_value_t = arca_bench::Options::SIZE_MIB,
            value_parser = clap::builder::RangedU64ValueParser::<usize>::new().range(1..=4096),
            requires = "codecs",
            help = "Size of the generated data in MiB; raised to 1 MiB per thread"
        )]
        size: usize,
        #[arg(long, requires = "codecs", help = "Print the result as JSON")]
        json: bool,
    },
}

#[derive(Copy, Clone, PartialEq, Eq, ValueEnum)]
enum CodecArg {
    #[value(help = "Deflate in .zip, LZMA2 in .7z")]
    Auto,
    Store,
    #[value(help = "Deflate (.zip, .7z)")]
    Deflate,
    #[value(help = "Zstandard (.zip; .zst and .tar.zst always use it)")]
    Zstd,
    #[value(name = "lzma2", help = "LZMA2 (.7z; .xz and .tar.xz always use it)")]
    Lzma2,
    #[value(help = "LZMA (.7z)")]
    Lzma,
    #[value(help = "PPMd (.7z), the smallest for text")]
    Ppmd,
    #[value(help = "BZip2 (.7z; .bz2 and .tar.bz2 always use it)")]
    Bzip2,
}

#[derive(Copy, Clone, PartialEq, Eq, ValueEnum)]
enum OnConflict {
    #[value(help = "Replace the file that is already there")]
    Overwrite,
    #[value(help = "Leave the file that is already there and move on")]
    Skip,
    #[value(help = "Write it next to the other one as name (1).ext")]
    Rename,
}

fn detect(p: &Path) -> Result<Format> {
    Format::detect(p).ok_or_else(|| unrecognized(p))
}

// For reading: an XZ, bzip2 or Zstandard file is looked inside, so a TAR in a
// `.xz`, or one with no suffix, still opens as TAR.XZ.
fn detect_archive(p: &Path) -> Result<Format> {
    arca_stream::identify(p).ok_or_else(|| unrecognized(p))
}

fn unrecognized(p: &Path) -> Error {
    let whole = arca_core::volumes::whole_name(p);
    if let Some(format) = Format::detect(&whole).filter(|f| {
        whole != p
            && matches!(
                f.container(),
                Container::Rar | Container::Iso | Container::Extra
            )
    }) {
        return Error::Unsupported(format!(
            "'{}' is part of a split {}; Arca joins split parts only for ZIP, 7z, TAR and the compressed streams",
            p.display(),
            format.label()
        ));
    }
    {
        let known: Vec<String> = Format::SUFFIXES
            .iter()
            .map(|(suffix, _)| format!(".{suffix}"))
            .collect();
        Error::Unsupported(format!(
            "unrecognized extension in '{}' (recognized: {})",
            p.display(),
            known.join(", ")
        ))
    }
}

fn split_is_read_only() -> Error {
    Error::Unsupported(
        "a split archive is read-only: extract it and create it again to change it".into(),
    )
}

fn tar_source(archive: &Path, format_kind: Format) -> Result<Box<dyn Read>> {
    Ok(match format_kind {
        f if arca_stream::handles(f) => arca_stream::open(archive, f)?,
        _ => {
            let f = BufReader::with_capacity(BUF, arca_core::volumes::open(archive)?);
            if format_kind == Format::TarGz {
                Box::new(flate2::read::MultiGzDecoder::new(f))
            } else {
                Box::new(f)
            }
        }
    })
}

fn stream_codec(format_kind: Format) -> Option<(CodecArg, &'static str)> {
    match format_kind.container() {
        Container::Xz | Container::TarXz => Some((CodecArg::Lzma2, "LZMA2")),
        Container::Bz2 | Container::TarBz2 => Some((CodecArg::Bzip2, "bzip2")),
        Container::Zst | Container::TarZst => Some((CodecArg::Zstd, "Zstandard")),
        _ => None,
    }
}

fn conflict(policy: OnConflict) -> Conflict {
    match policy {
        OnConflict::Overwrite => Conflict::Overwrite,
        OnConflict::Skip => Conflict::Skip,
        OnConflict::Rename => Conflict::Rename,
    }
}

// Built under a temporary name beside `out` and renamed only once complete, so
// a failed or interrupted run never leaves a half-written archive behind.
fn staged_path(out: &Path) -> PathBuf {
    let mut name = out.file_name().unwrap_or_default().to_os_string();
    name.push(".arca-new");
    out.with_file_name(name)
}

// Built under a temporary name beside `out`, or as staged parts when split,
// and renamed only once complete, so a failed or interrupted run never leaves
// a half-written archive behind. Answers with the bytes written in all.
fn staged<T>(
    out: &Path,
    volume: Option<u64>,
    write: impl FnOnce(BufWriter<arca_core::volumes::Output>) -> Result<T>,
) -> Result<(T, u64)> {
    arca_core::volumes::staged(out, volume, false, |o| {
        write(BufWriter::with_capacity(BUF, o))
    })
}

fn main() {
    let cli = Cli::parse();
    if let Err(e) = run(cli) {
        eprintln!("arca: {e}");
        std::process::exit(1);
    }
}

fn run(cli: Cli) -> Result<()> {
    match cli.command {
        Cmd::Create {
            out,
            inputs,
            level,
            codec,
            threads,
            password,
            hide_names,
            volume,
            sfx,
            console,
        } => create(
            &out,
            &inputs,
            level,
            codec,
            threads,
            (password.as_deref(), hide_names),
            match (volume, sfx) {
                (_, true) => Shape::Sfx { console },
                (Some(size), false) => Shape::Split(size),
                (None, false) => Shape::Plain,
            },
        ),
        Cmd::List {
            archive,
            time,
            password,
        } => list(&archive, time, password.as_deref()),
        Cmd::Extract {
            archive,
            dest,
            on_conflict,
            threads,
            password,
        } => extract(&archive, &dest, on_conflict, threads, password.as_deref()),
        Cmd::Test { archive, password } => test_archive(&archive, password.as_deref()),
        Cmd::Hash {
            paths,
            algorithm,
            check,
        } => {
            if !hash::run(&paths, algorithm, check.as_deref())? {
                std::process::exit(1);
            }
            Ok(())
        }
        Cmd::Password {
            archive,
            password,
            new,
            out,
        } => change_password(
            &archive,
            out.as_deref(),
            password.as_deref(),
            new.as_deref(),
        ),
        Cmd::Comment {
            archive,
            set,
            file,
            clear,
            entry,
            out,
            password,
        } => comment::run(comment::Request {
            archive,
            set,
            file,
            clear,
            entry,
            out,
            password,
        }),
        Cmd::Repair {
            archive,
            out,
            password,
        } => repair::run(&archive, &out, password.as_deref()),
        Cmd::Diff {
            a,
            b,
            quick,
            times,
            password,
            password_b,
        } => std::process::exit(diff::run(
            &a,
            &b,
            arca_compare::Options { quick, times },
            password.as_deref(),
            password_b.as_deref().or(password.as_deref()),
        )),
        Cmd::Bench {
            archive,
            codecs,
            threads,
            seconds,
            size,
            json,
        } => match archive {
            Some(archive) if !codecs => bench(&archive),
            _ => bench::codecs(threads, seconds, size, json),
        },
    }
}

/// A symbolic link to store as one: its name, what it holds, its time.
type Link = (String, String, i64);
type Collected = (Vec<(PathBuf, String, fs::Metadata)>, Vec<Link>);

/// The regular files under `inputs`, each with what the folder listing said
/// about it, and the symbolic links among them, which are never followed.
/// Only a TAR keeps the links.
fn collect_files(inputs: &[PathBuf]) -> Result<Collected> {
    let mut v = Vec::new();
    let mut links = Vec::new();
    for e in inputs {
        let base = e.parent().unwrap_or(Path::new(""));
        walk(e, fs::symlink_metadata(e)?, base, &mut v, &mut links)?;
    }
    Ok((v, links))
}

fn walk(
    p: &Path,
    meta: fs::Metadata,
    base: &Path,
    out: &mut Vec<(PathBuf, String, fs::Metadata)>,
    links: &mut Vec<Link>,
) -> Result<()> {
    let rel = p.strip_prefix(base).unwrap_or(p);
    let name = rel.to_string_lossy().replace('\\', "/");
    if meta.is_dir() {
        let mut children: Vec<_> = fs::read_dir(p)?.collect::<io::Result<Vec<_>>>()?;
        children.sort_by_key(|d| d.file_name());
        for h in children {
            walk(&h.path(), h.metadata()?, base, out, links)?;
        }
    } else if meta.is_file() {
        out.push((p.to_path_buf(), name, meta));
    } else if let Some(target) = arca_tar::link_target(p, &meta) {
        links.push((name, target, mtime_of(&meta)));
    }
    Ok(())
}

/// The symbolic links of a new TAR, after its files.
fn add_links<W: Write>(w: &mut TarWriter<W>, links: &[Link]) -> Result<()> {
    for (name, target, mtime) in links {
        w.add_link(name, target, *mtime, b'2')?;
    }
    Ok(())
}

fn mtime_of(m: &fs::Metadata) -> i64 {
    m.modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn resolve_threads(requested: usize) -> usize {
    if requested > 0 {
        return requested;
    }
    std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1)
}

fn resolve_codec(c: CodecArg, format_kind: Format) -> Result<Codec> {
    Ok(match c {
        CodecArg::Store => Codec::Store,
        CodecArg::Deflate => Codec::Deflate,
        CodecArg::Zstd => Codec::Zstd,
        CodecArg::Lzma2 => return Err(Error::Unsupported("LZMA2 creation requires .7z".into())),
        CodecArg::Lzma => return Err(Error::Unsupported("LZMA creation requires .7z".into())),
        CodecArg::Ppmd => return Err(Error::Unsupported("PPMd creation requires .7z".into())),
        CodecArg::Bzip2 => {
            return Err(Error::Unsupported(
                "bzip2 creation requires .7z, .bz2 or .tar.bz2".into(),
            ))
        }
        CodecArg::Auto => match format_kind {
            Format::Zip => Codec::Deflate,
            _ => Codec::Deflate,
        },
    })
}

#[derive(Clone, Copy)]
enum Shape {
    Plain,
    Split(u64),
    Sfx { console: bool },
}

/// The self-extractor program and the flags for its trailer. It is the
/// arca-sfx built alongside arca and installed next to it; ARCA_SFX_STUB
/// names another one. With `console` a Windows program is made a console
/// one, so it can talk through the console it was started from.
fn sfx_stub(console: bool) -> Result<(Vec<u8>, u32)> {
    let path = match std::env::var_os("ARCA_SFX_STUB") {
        Some(path) => PathBuf::from(path),
        None => std::env::current_exe()?
            .with_file_name(format!("arca-sfx{}", std::env::consts::EXE_SUFFIX)),
    };
    let mut program = fs::read(&path).map_err(|e| {
        Error::Unsupported(format!(
            "self-extracting programs need arca-sfx, which was not found at '{}' ({e}); \
             it comes with Arca on Windows, and `cargo build -p arca-sfx` builds it",
            path.display()
        ))
    })?;
    if arca_core::sfx::Trailer::read(&mut io::Cursor::new(&program)).is_ok() {
        return Err(Error::Unsupported(format!(
            "'{}' already carries an archive; point ARCA_SFX_STUB at the plain arca-sfx",
            path.display()
        )));
    }
    let mut flags = 0;
    if console {
        if program.starts_with(b"MZ") {
            arca_core::sfx::console_subsystem(&mut program)?;
        }
        flags |= arca_core::sfx::CONSOLE;
    }
    Ok((program, flags))
}

fn create(
    out: &Path,
    inputs: &[PathBuf],
    level: Level,
    codec_arg: CodecArg,
    requested_threads: usize,
    (password, hide_names): (Option<&str>, bool),
    shape: Shape,
) -> Result<()> {
    let volume = match shape {
        Shape::Split(size) => Some(size),
        _ => None,
    };
    let whole = volume.map(|_| arca_core::volumes::whole_name(out));
    let out = whole.as_deref().unwrap_or(out);
    let stub = match shape {
        Shape::Sfx { console } => Some(sfx_stub(console)?),
        _ => None,
    };
    if stub.is_some()
        && !matches!(
            codec_arg,
            CodecArg::Auto | CodecArg::Deflate | CodecArg::Store
        )
    {
        return Err(Error::Unsupported(
            "a self-extracting program carries a ZIP compressed with deflate or stored; use -c auto, deflate or store".into(),
        ));
    }
    let format_kind = if stub.is_some() {
        Format::Zip
    } else {
        detect(out)?
    };
    if !format_kind.can_create() {
        return Err(format_kind.read_only());
    }
    if format_kind == Format::SevenZ {
        return sevenz::create(
            out,
            inputs,
            level,
            codec_arg,
            (password, hide_names),
            volume,
        );
    }
    if format_kind == Format::Rar && volume.is_some() {
        return Err(Error::Unsupported(
            "-v does not split .rar archives; RAR has volumes of its own that Arca does not write yet. Split a .zip or .7z instead".into(),
        ));
    }
    if format_kind == Format::Rar {
        return rar::create(
            out,
            inputs,
            level,
            codec_arg,
            requested_threads,
            password,
            hide_names,
        );
    }
    if hide_names {
        return Err(Error::Unsupported(
            "--hide-names requires .7z and a nonempty password".into(),
        ));
    }
    if password.is_some() && format_kind != Format::Zip {
        return Err(Error::Unsupported(format!(
            "encryption requires .zip or .7z; {} has no place to put it",
            format_kind.label()
        )));
    }
    let stream = arca_stream::handles(format_kind);
    if let Some((accepted, name)) = stream_codec(format_kind) {
        if codec_arg != CodecArg::Auto && codec_arg != accepted {
            return Err(Error::Unsupported(format!(
                ".{} always compresses with {name}",
                format_kind.extension()
            )));
        }
    }
    if let Some(tar) = format_kind.stream_tar() {
        let (label, tar) = (format_kind.label(), tar.extension());
        if inputs.len() != 1 {
            return Err(Error::Unsupported(format!(
                "{label} holds exactly one file, and {} were given; make a .{tar} to keep several",
                inputs.len()
            )));
        }
        if !fs::metadata(&inputs[0])?.is_file() {
            return Err(Error::Unsupported(format!(
                "{label} holds exactly one file, and '{}' is not a regular file; make a .{tar} for a folder",
                inputs[0].display()
            )));
        }
    }
    let codec = resolve_codec(if stream { CodecArg::Auto } else { codec_arg }, format_kind)?;
    let mut threads = resolve_threads(requested_threads);
    let own = arca_core::extraction::OwnOutput::new(out);
    let (mut raw_list, mut links) = collect_files(inputs)?;
    raw_list.retain(|(path, _, _)| !own.is_own(path));
    if !matches!(
        format_kind.container(),
        Container::Tar | Container::TarGz | Container::TarXz
    ) {
        links.clear();
    }
    if raw_list.is_empty() && links.is_empty() {
        return Err(Error::Format("there is nothing to add".into()));
    }

    let mut files: Vec<arca_zip::Source> = Vec::with_capacity(raw_list.len());
    let mut total = 0u64;
    for (path, name, m) in raw_list {
        total += m.len();
        files.push(arca_zip::Source {
            path,
            name,
            size: m.len(),
            mtime: mtime_of(&m),
            codec,
            level,
        });
    }

    let t0 = Instant::now();
    let final_size = match format_kind.container() {
        Container::SevenZ => unreachable!(),
        // Nothing to report while it runs -- the summary is printed at the end
        // -- so the answer to "carry on?" is always yes.
        Container::Rar | Container::Iso | Container::Extra => return Err(format_kind.read_only()),
        Container::Zip => {
            staged(out, volume, |w| {
                let go = &|_: usize, _: usize, _: &str| true;
                match &stub {
                    Some((program, flags)) => arca_core::sfx::build(w, program, *flags, |w| {
                        arca_zip::create_zip_into(w, &files, threads, None, go)
                    })?
                    .flush()?,
                    None => arca_zip::create_zip_into(w, &files, threads, password, go)?.flush()?,
                }
                Ok(())
            })?
            .1
        }
        Container::Tar | Container::TarGz => {
            staged(out, volume, |f| {
                let dest: Box<dyn Write> = if format_kind == Format::TarGz {
                    Box::new(flate2::write::GzEncoder::new(
                        f,
                        flate2::Compression::new(level.to_flate2()),
                    ))
                } else {
                    Box::new(f)
                };
                let mut w = TarWriter::new(dest);
                for s in &files {
                    let file = File::open(&s.path)?;
                    let mode = arca_tar::mode_of(&file.metadata()?);
                    let entrada = BufReader::with_capacity(BUF, file);
                    w.add(&s.name, s.size, s.mtime, mode, entrada)?;
                }
                add_links(&mut w, &links)?;
                w.finish()?.flush()?;
                Ok(())
            })?
            .1
        }
        Container::TarXz | Container::TarBz2 | Container::TarZst => {
            let (used, written) = staged(out, volume, |f| {
                let e = arca_stream::encoder(f, format_kind, level, requested_threads, None)?;
                let used = e.workers();
                let mut w = TarWriter::new(e);
                for s in &files {
                    let file = File::open(&s.path)?;
                    let mode = arca_tar::mode_of(&file.metadata()?);
                    let entrada = BufReader::with_capacity(BUF, file);
                    w.add(&s.name, s.size, s.mtime, mode, entrada)?;
                }
                add_links(&mut w, &links)?;
                w.finish()?.finish()?.flush()?;
                Ok(used)
            })?;
            threads = used;
            written
        }
        Container::Xz | Container::Bz2 | Container::Zst => {
            let (used, written) = staged(out, volume, |f| {
                let mut e = arca_stream::encoder(
                    f,
                    format_kind,
                    level,
                    requested_threads,
                    Some(files[0].size),
                )?;
                let used = e.workers();
                io::copy(
                    &mut BufReader::with_capacity(BUF, File::open(&files[0].path)?),
                    &mut e,
                )?;
                e.finish()?.flush()?;
                Ok(used)
            })?;
            threads = used;
            written
        }
    };

    let dt = t0.elapsed();
    let program = stub
        .as_ref()
        .map_or(0, |(p, _)| p.len() as u64 + arca_core::sfx::TRAILER_LEN);
    let packed = final_size.saturating_sub(program);
    let note = if program > 0 {
        format!(
            " \u{b7} plus the {} self-extracting program",
            human(program)
        )
    } else {
        parts_note(final_size, volume)
    };
    let ratio = if total > 0 {
        100.0 * (1.0 - packed as f64 / total as f64)
    } else {
        0.0
    };
    let mbs = if dt.as_secs_f64() > 0.0 {
        total as f64 / 1_048_576.0 / dt.as_secs_f64()
    } else {
        0.0
    };
    println!(
        "{}: {} files, {} -> {} ({:.1}% smaller) in {:.3} s \u{b7} {:.0} MiB/s \u{b7} {} threads{}",
        arca_core::volumes::first_output(out, volume).display(),
        files.len(),
        human(total),
        human(packed),
        ratio,
        dt.as_secs_f64(),
        mbs,
        threads,
        note
    );
    Ok(())
}

fn parts_note(written: u64, volume: Option<u64>) -> String {
    match volume {
        Some(size) => format!(" \u{b7} {} parts", written.div_ceil(size).max(1)),
        None => String::new(),
    }
}

fn list(archive: &Path, time: bool, password: Option<&str>) -> Result<()> {
    let t0 = Instant::now();
    let format_kind = detect_archive(archive)?;
    let mut n = 0u64;
    let mut bytes = 0u64;

    let mut notes: Vec<String> = Vec::new();
    let mut out = BufWriter::new(io::stdout().lock());
    match format_kind.container() {
        Container::SevenZ => {
            let a = arca_7z::SevenZArchive::open(arca_core::volumes::open(archive)?, password)?;
            for e in a.entries() {
                writeln!(
                    out,
                    "{:>12}  {:>7}  {:>5.1}%  {}",
                    e.size,
                    e.method.name(),
                    e.ratio() * 100.0,
                    e.name
                )?;
                n += 1;
                bytes += e.size;
            }
        }
        Container::Rar => {
            let a = arca_rar::RarArchive::open(archive, password)?;
            for e in a.entries() {
                writeln!(
                    out,
                    "{:>12}  {:>7}  {:>5.1}%  {}",
                    e.size,
                    e.method.name(),
                    e.ratio() * 100.0,
                    e.name
                )?;
                n += 1;
                bytes += e.size;
            }
            comment::notes_for_rar(&a, password, &mut notes);
        }
        Container::Iso => {
            let a = open_iso(archive)?;
            for e in a.entries() {
                writeln!(
                    out,
                    "{:>12}  {:>7}  {:>5.1}%  {}",
                    e.size,
                    e.method.name(),
                    e.ratio() * 100.0,
                    e.name
                )?;
                n += 1;
                bytes += e.size;
            }
        }
        Container::Extra => {
            let a = open_extra(archive, format_kind)?;
            for (i, e) in a.entries().iter().enumerate() {
                let link = match a.link(i) {
                    Some(arca_extra::Link::Symbolic(target)) => format!(" -> {target}"),
                    Some(arca_extra::Link::Hard(target)) => format!(" link to {target}"),
                    None => String::new(),
                };
                writeln!(
                    out,
                    "{:>12}  {:>7}  {:>5.1}%  {}{link}",
                    e.size,
                    e.method.name(),
                    e.ratio() * 100.0,
                    e.name
                )?;
                n += 1;
                bytes += e.size;
            }
        }
        Container::Zip => {
            let a = ZipArchive::open(arca_core::volumes::open(archive)?)?;
            for (i, e) in a.entries().iter().enumerate() {
                writeln!(
                    out,
                    "{:>12}  {:>7}  {:>5.1}%  {}",
                    e.size,
                    e.method.name(),
                    e.ratio() * 100.0,
                    e.name
                )?;
                n += 1;
                bytes += e.size;
                if let Some(c) = a.entry_comment(i) {
                    notes.push(format!(
                        "comment on {}: {}",
                        printable(&e.name).replace('\n', " "),
                        printable(&c).replace('\n', " ")
                    ));
                }
            }
            let comment = a.comment();
            if !comment.is_empty() {
                notes.push("archive comment:".into());
                notes.extend(printable(&comment).lines().map(|l| format!("  {l}")));
            }
        }
        Container::Xz | Container::Bz2 | Container::Zst => {
            let e = arca_stream::entry(archive, format_kind)?;
            writeln!(
                out,
                "{:>12}  {:>7}  {:>5.1}%  {}",
                e.size,
                e.method.name(),
                e.ratio() * 100.0,
                e.name
            )?;
            n = 1;
            bytes = e.size;
        }
        Container::Tar
        | Container::TarGz
        | Container::TarXz
        | Container::TarBz2
        | Container::TarZst => {
            let mut r = TarReader::new(tar_source(archive, format_kind)?);
            while let Some(e) = r.next_entry()? {
                let link = match (e.symlink(), e.hard_link()) {
                    (Some(target), _) => format!(" -> {target}"),
                    (_, Some(target)) => format!(" link to {target}"),
                    _ => String::new(),
                };
                writeln!(
                    out,
                    "{:>12}  {:>7}  {:>5}   {}{link}",
                    e.entry.size, "store", "", e.entry.name
                )?;
                n += 1;
                bytes += e.entry.size;
                comment::note_for_tar(&e, &mut notes);
                r.skip_data(&e)?;
            }
        }
    }
    out.flush()?;
    drop(out);
    for note in notes {
        eprintln!("{note}");
    }

    if time {
        eprintln!(
            "{n} entries, {} uncompressed, listed in {:.1} ms",
            human(bytes),
            t0.elapsed().as_secs_f64() * 1000.0
        );
    }
    Ok(())
}

// `claimed` holds the names this run has already handed out. Extraction decides
// every destination before it writes anything, so `exists()` alone would give
// two entries with the same name the same free name.
fn free_name(path: &Path, claimed: &HashSet<PathBuf>) -> PathBuf {
    let dir = path.parent().map(PathBuf::from).unwrap_or_default();
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    let ext = path
        .extension()
        .map(|s| format!(".{}", s.to_string_lossy()))
        .unwrap_or_default();
    for n in 1..10_000u32 {
        let candidate = dir.join(format!("{stem} ({n}){ext}"));
        if !candidate.exists() && !claimed.contains(&arca_core::extraction::claim_key(&candidate)) {
            return candidate;
        }
    }
    path.to_path_buf()
}

// Small entries are decompressed into one reused buffer and written with a
// single call. The bound keeps an entry that lies about its size from growing
// the buffer past the size its header declared.
struct Bounded<'a> {
    buf: &'a mut Vec<u8>,
    left: usize,
}

impl Write for Bounded<'_> {
    fn write(&mut self, data: &[u8]) -> io::Result<usize> {
        if data.len() > self.left {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "entry produced more bytes than its header declared",
            ));
        }
        self.left -= data.len();
        self.buf.extend_from_slice(data);
        Ok(data.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn resolve_conflict(
    path: PathBuf,
    policy: OnConflict,
    claimed: &mut HashSet<PathBuf>,
) -> Option<PathBuf> {
    let taken = claimed.contains(&arca_core::extraction::claim_key(&path))
        || (!matches!(policy, OnConflict::Overwrite) && path.exists());
    let chosen = if !taken {
        path
    } else {
        match policy {
            OnConflict::Overwrite => path,
            OnConflict::Skip => return None,
            OnConflict::Rename => free_name(&path, claimed),
        }
    };
    claimed.insert(arca_core::extraction::claim_key(&chosen));
    Some(chosen)
}

// A .zip is random access: the central directory says where every entry starts,
// so one thread per core can each open the file and decompress a different one.
// A .tar is a single stream, and a .tar.gz a single gzip stream on top of it, so
// there is nothing to split there and that branch stays sequential.
fn extract(
    archive: &Path,
    dest: &Path,
    policy: OnConflict,
    requested_threads: usize,
    password: Option<&str>,
) -> Result<()> {
    let format_kind = detect_archive(archive)?;
    if format_kind == Format::SevenZ {
        return sevenz::extract(archive, dest, policy, password);
    }
    if !matches!(
        format_kind.container(),
        Container::Rar | Container::Iso | Container::Extra
    ) {
        fs::create_dir_all(dest)?;
    }
    let t0 = Instant::now();
    let mut n = 0u64;
    let mut bytes = 0u64;
    let threads = resolve_threads(requested_threads);

    match format_kind.container() {
        Container::SevenZ => unreachable!(),
        Container::Rar => {
            let a = arca_rar::RarArchive::open(archive, password)?;
            let bytes = a.extract(dest, &[], password, &|_, _, _| true, &|_| match policy {
                OnConflict::Overwrite => arca_rar::Conflict::Overwrite,
                OnConflict::Skip => arca_rar::Conflict::Skip,
                OnConflict::Rename => arca_rar::Conflict::Rename,
            })?;
            println!(
                "{} written in {:.3} s",
                human(bytes),
                t0.elapsed().as_secs_f64()
            );
            return Ok(());
        }
        Container::Extra => {
            let a = open_extra(archive, format_kind)?;
            let done = a.extract(dest, &[], &|_, _, _| true, &|_| match policy {
                OnConflict::Overwrite => arca_extra::Conflict::Overwrite,
                OnConflict::Skip => arca_extra::Conflict::Skip,
                OnConflict::Rename => arca_extra::Conflict::Rename,
            })?;
            for refused in &done.refused {
                eprintln!("arca: {refused}");
            }
            n = done.files as u64;
            bytes = done.bytes;
        }
        Container::Iso => {
            let a = open_iso(archive)?;
            let bytes = a.extract(dest, &[], &|_, _, _| true, &|_| match policy {
                OnConflict::Overwrite => arca_iso::Conflict::Overwrite,
                OnConflict::Skip => arca_iso::Conflict::Skip,
                OnConflict::Rename => arca_iso::Conflict::Rename,
            })?;
            println!(
                "{} written in {:.3} s",
                human(bytes),
                t0.elapsed().as_secs_f64()
            );
            return Ok(());
        }
        Container::Zip => {
            let a = ZipArchive::open(arca_core::volumes::open(archive)?)?;
            // Directories and conflicts are settled here, single threaded: two
            // threads racing on create_dir_all or on picking a free name would
            // give a result that depends on who won.
            let mut claimed: HashSet<PathBuf> = HashSet::new();
            let mut created: HashSet<PathBuf> = HashSet::new();
            let mut jobs: Vec<(arca_core::Entry, PathBuf)> = Vec::new();
            for e in a.entries() {
                let path = dest.join(arca_core::safe_name(&e.name)?);
                if e.is_dir {
                    if created.insert(path.clone()) {
                        fs::create_dir_all(&path)?;
                    }
                    continue;
                }
                if let Some(p) = path.parent() {
                    if !created.contains(p) {
                        fs::create_dir_all(p)?;
                        created.insert(p.to_path_buf());
                    }
                }
                let Some(path) = resolve_conflict(path, policy, &mut claimed) else {
                    continue;
                };
                jobs.push((e.clone(), path));
            }
            // Two entries with one name, overwriting: the later one is what
            // extracting in order would leave, and writing both from two
            // threads at once would leave a mix of the two.
            if matches!(policy, OnConflict::Overwrite) {
                let mut seen: HashSet<PathBuf> = HashSet::new();
                let mut kept: Vec<(arca_core::Entry, PathBuf)> = jobs
                    .into_iter()
                    .rev()
                    .filter(|(_, path)| seen.insert(arca_core::extraction::claim_key(path)))
                    .collect();
                kept.reverse();
                jobs = kept;
            }
            drop(a);

            let pool = rayon::ThreadPoolBuilder::new()
                .num_threads(threads)
                .build()
                .map_err(|e| Error::Format(format!("cannot start the thread pool: {e}")))?;
            let chunk = (jobs.len() / (threads * 8)).max(1);
            let written: Vec<u64> = pool.install(|| {
                jobs.par_chunks(chunk)
                    .map(|batch| {
                        let mut source = ExtractionReader::with_capacity(
                            BUF,
                            arca_core::volumes::open(archive)?,
                        )?;
                        let mut staging = Vec::new();
                        let mut total = 0u64;
                        // Nothing already on disk is touched until the entry has
                        // decoded: a small one is decoded into memory first, a
                        // large one or one replacing a file goes through a
                        // temporary that is renamed over it at the end.
                        for (e, path) in batch {
                            if e.size <= BUF as u64 {
                                staging.clear();
                                let mut bounded = Bounded {
                                    buf: &mut staging,
                                    left: e.size as usize,
                                };
                                total += arca_zip::extract_entry_with(
                                    &mut source,
                                    e,
                                    &mut bounded,
                                    password,
                                )?;
                                let put = |f: &mut File| -> Result<u64> {
                                    f.write_all(&staging)?;
                                    arca_core::extraction::set_mtime(f, e.mtime);
                                    Ok(0)
                                };
                                if path.exists() {
                                    arca_core::extraction::write_staged(path, put)?;
                                } else {
                                    put(&mut File::create(path)?)?;
                                }
                            } else {
                                total += arca_core::extraction::write_staged(path, |f| {
                                    let mut w = BufWriter::with_capacity(BUF, &mut *f);
                                    let n = arca_zip::extract_entry_with(
                                        &mut source,
                                        e,
                                        &mut w,
                                        password,
                                    )?;
                                    w.flush()?;
                                    drop(w);
                                    arca_core::extraction::set_mtime(f, e.mtime);
                                    Ok(n)
                                })?;
                            }
                        }
                        Ok(total)
                    })
                    .collect::<Result<Vec<u64>>>()
            })?;
            n = jobs.len() as u64;
            bytes = written.iter().sum();
        }
        Container::Xz | Container::Bz2 | Container::Zst => {
            let mut source = arca_stream::open(archive, format_kind)?;
            let name = arca_stream::output_name(archive);
            let written =
                Destination::new(dest)
                    .write_entry(&name, false, None, &mut source, &mut |_| conflict(policy))?;
            if let Some(w) = written {
                bytes = w;
                n = 1;
            }
        }
        // Each file is written under a temporary name and renamed only once its
        // data has been read to the end, so a corrupt or truncated archive does
        // not leave a short file under the real name.
        Container::Tar
        | Container::TarGz
        | Container::TarXz
        | Container::TarBz2
        | Container::TarZst => {
            let mut r = TarReader::new(tar_source(archive, format_kind)?);
            let mut destination = Destination::new(dest);
            while let Some(e) = r.next_entry()? {
                if let Some(target) = e.symlink() {
                    destination.write_symlink(&e.entry.name, target)?;
                } else if let Some(target) = e.hard_link() {
                    let made = destination.write_hard_link(
                        &e.entry.name,
                        target,
                        e.entry.mtime,
                        Some(e.mode),
                        &mut |_| conflict(policy),
                    )?;
                    n += u64::from(made);
                }
                // Devices have no data to write, and a file in their place
                // would be an empty one where a device belongs.
                if !e.is_plain() {
                    r.skip_data(&e)?;
                    continue;
                }
                let written = r.with_data(&e, |data| {
                    destination.write_file(
                        &e.entry.name,
                        e.entry.is_dir,
                        e.entry.mtime,
                        Some(e.mode),
                        data,
                        &mut |_| conflict(policy),
                    )
                })?;
                if let (Some(w), false) = (written, e.entry.is_dir) {
                    bytes += w;
                    n += 1;
                }
            }
            arca_stream::drain(&mut r.into_inner())?;
            n += destination.make_links(&mut |_| conflict(policy))? as u64;
            for refused in destination.refused() {
                eprintln!("arca: {refused}");
            }
        }
    }

    println!(
        "{n} files, {} written in {:.3} s",
        human(bytes),
        t0.elapsed().as_secs_f64()
    );
    Ok(())
}

// Rewrites the archive with a different password, or with none. The entries go
// through decrypted but still compressed, so nothing is compressed again and
// the sizes come out identical to the ones that went in.
//
// Replacing in place is the destructive case, and there is only one copy of the
// data. So the new archive is built next to the old one, verified end to end,
// and only then moved over it. Any failure leaves the original untouched.
fn change_password(
    archive: &Path,
    out: Option<&Path>,
    current: Option<&str>,
    new: Option<&str>,
) -> Result<()> {
    if arca_core::volumes::is_part(archive) {
        return Err(split_is_read_only());
    }
    let format_kind = detect(archive)?;
    if !format_kind.can_write() {
        return Err(format_kind.read_only());
    }
    if format_kind != Format::Zip {
        return Err(Error::Unsupported(
            "only ZIP passwords can be changed; 7z password changes are not supported".into(),
        ));
    }
    if let Some(out_format) = out.and_then(Format::detect) {
        if !out_format.can_write() {
            return Err(out_format.read_only());
        }
        if out_format != Format::Zip {
            return Err(Error::Unsupported(
                "a password rewrite writes a ZIP, so --out needs a .zip name".into(),
            ));
        }
    }
    let entries = ZipArchive::open(arca_core::volumes::open(archive)?)?
        .entries()
        .to_vec();
    let was_encrypted = entries.iter().any(|e| e.encrypted);
    if !was_encrypted && new.is_none() {
        return Err(Error::Format(format!(
            "'{}' has no password to remove",
            archive.display()
        )));
    }
    if !was_encrypted && current.is_some() {
        return Err(Error::Format(format!(
            "'{}' is not encrypted, so there is no current password",
            archive.display()
        )));
    }

    let target = out.unwrap_or(archive);
    let temp = target.with_file_name(format!(
        "{}.arca-new",
        target
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default()
    ));
    let t0 = Instant::now();

    let bytes = match arca_zip::rewrite_password(archive, &temp, current, new, &|_, _, _| true) {
        Ok(b) => b,
        Err(e) => {
            let _ = fs::remove_file(&temp);
            return Err(e);
        }
    };

    fs::rename(&temp, target)?;
    println!(
        "{}: {} entries, {} {} in {:.3} s",
        target.display(),
        entries.len(),
        human(bytes),
        if new.is_some() {
            "now encrypted with AES-256"
        } else {
            "no longer encrypted"
        },
        t0.elapsed().as_secs_f64()
    );
    Ok(())
}

// Comments come from whoever made the archive, and a terminal acts on escape
// sequences: everything but line breaks and tabs that could steer it is shown
// as a replacement character instead.
fn printable(text: &str) -> String {
    text.chars()
        .filter(|&c| c != '\r')
        .map(|c| {
            let bidi = matches!(c, '\u{061C}' | '\u{200E}' | '\u{200F}' | '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}');
            if bidi || (c.is_control() && c != '\n' && c != '\t') {
                '\u{FFFD}'
            } else {
                c
            }
        })
        .collect()
}

// What the listing leaves out goes to stderr so `arca l` output stays a
// clean list for scripts.
fn open_iso(archive: &Path) -> Result<arca_iso::IsoArchive> {
    let a = arca_iso::IsoArchive::open(archive)?;
    for notice in a.notices() {
        eprintln!("note: {notice}");
    }
    Ok(a)
}

fn open_extra(archive: &Path, format_kind: Format) -> Result<arca_extra::ExtraArchive> {
    let a = arca_extra::ExtraArchive::open(archive, format_kind)?;
    for notice in a.notices() {
        eprintln!("note: {notice}");
    }
    Ok(a)
}

fn test_archive(archive: &Path, password: Option<&str>) -> Result<()> {
    let format_kind = detect_archive(archive)?;
    let t0 = Instant::now();
    let mut n = 0u64;
    let mut failures = 0u64;

    match format_kind.container() {
        Container::SevenZ => {
            let mut a = arca_7z::SevenZArchive::open(arca_core::volumes::open(archive)?, password)?;
            a.test(&mut |_| true)?;
            n = a.len() as u64;
        }
        Container::Rar => {
            let a = arca_rar::RarArchive::open(archive, password)?;
            a.test(password, &|_, _, _| true)?;
            n = a.entries().iter().filter(|e| !e.is_dir).count() as u64;
        }
        Container::Extra => {
            n = open_extra(archive, format_kind)?.test(&[], &|_, _, _| true)? as u64;
        }
        Container::Iso => {
            let a = open_iso(archive)?;
            a.test(&[], &|_, _, _| true)?;
            n = a.entries().iter().filter(|e| !e.is_dir).count() as u64;
        }
        Container::Zip => {
            let mut a = ZipArchive::open(arca_core::volumes::open(archive)?)?;
            for i in 0..a.len() {
                if a.entries()[i].is_dir {
                    continue;
                }
                let name = a.entries()[i].name.clone();
                match a.extract_to_with(i, io::sink(), password) {
                    Ok(_) => n += 1,
                    Err(e) => {
                        eprintln!("  FALLO  {name}: {e}");
                        failures += 1;
                    }
                }
            }
        }
        Container::Xz | Container::Bz2 | Container::Zst => {
            arca_stream::drain(&mut arca_stream::open(archive, format_kind)?)?;
            n = 1;
        }
        Container::Tar
        | Container::TarGz
        | Container::TarXz
        | Container::TarBz2
        | Container::TarZst => {
            let mut r = TarReader::new(tar_source(archive, format_kind)?);
            while let Some(e) = r.next_entry()? {
                r.with_data(&e, |data| Ok(io::copy(data, &mut io::sink())?))?;
                n += 1;
            }
            arca_stream::drain(&mut r.into_inner())?;
        }
    }

    if failures > 0 {
        return Err(Error::Format(format!(
            "{failures} corrupt entries out of {}",
            n + failures
        )));
    }
    println!(
        "{n} entries verified, no errors ({:.3} s)",
        t0.elapsed().as_secs_f64()
    );
    Ok(())
}

fn bench(archive: &Path) -> Result<()> {
    println!("Performance requirements (design document, section 05)\n");

    let mut best_of = f64::MAX;
    let mut inputs = 0usize;
    for _ in 0..5 {
        let t = Instant::now();
        let a = ZipArchive::open(arca_core::volumes::open(archive)?)?;
        inputs = a.len();
        let d = t.elapsed().as_secs_f64() * 1000.0;
        if d < best_of {
            best_of = d;
        }
    }
    let size = fs::metadata(archive)?.len();
    let r2 = best_of < 200.0;
    println!("  R2  list without extracting");
    println!("      {} entries in a {} archive", inputs, human(size));
    println!(
        "      {:.1} ms   target < 200 ms   {}",
        best_of,
        pass_fail(r2)
    );
    println!();
    println!("  R1  cold start: open and list a one-entry archive, with hyperfine");
    println!("      hyperfine -N --warmup 20 'arca list tiny.zip'");
    Ok(())
}

fn pass_fail(ok: bool) -> &'static str {
    if ok {
        "PASS"
    } else {
        "FAIL"
    }
}

fn human(b: u64) -> String {
    const U: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut v = b as f64;
    let mut i = 0;
    while v >= 1024.0 && i < U.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    if i == 0 {
        format!("{b} B")
    } else {
        format!("{v:.1} {}", U[i])
    }
}
