#![cfg_attr(windows, windows_subsystem = "windows")]
//! The program at the front of a self-extracting archive. `arca create
//! --sfx` copies it, appends a ZIP and a trailer, and when the copy runs it
//! finds that ZIP in its own file, asks where to put what it holds and
//! extracts it there with the same path checks as the rest of Arca.

use arca_core::extraction::{Conflict, Destination};
use arca_core::sfx::{Trailer, Window};
use arca_core::{Entry, Error, Result};
use arca_zip::ZipArchive;
use std::fs::File;
use std::io::{self, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::thread::ScopedJoinHandle;

mod words;
use words::{fill, Words};

struct Args {
    dest: Option<PathBuf>,
    console: bool,
}

fn parse(mut args: impl Iterator<Item = std::ffi::OsString>) -> Option<Args> {
    let mut parsed = Args {
        dest: None,
        console: false,
    };
    while let Some(arg) = args.next() {
        match arg.to_str() {
            Some("-o" | "--output") => parsed.dest = Some(PathBuf::from(args.next()?)),
            Some("--console") => parsed.console = true,
            _ => return None,
        }
    }
    Some(parsed)
}

fn main() {
    let words = words::pick();
    let Some(args) = parse(std::env::args_os().skip(1)) else {
        println!("{}", words.usage);
        std::process::exit(2);
    };
    let mut console = args.console || !cfg!(windows);
    let outcome = run(&args, words, &mut console);
    match outcome {
        Ok(Some(done)) => tell(console, words, &done, false),
        Ok(None) => {}
        Err(error) => {
            tell(
                console,
                words,
                &fill(words.failed, &[("error", &error.to_string())]),
                true,
            );
            std::process::exit(1);
        }
    }
}

fn tell(console: bool, words: &Words, text: &str, error: bool) {
    if console {
        if error {
            eprintln!("{text}");
        } else {
            println!("{text}");
        }
        return;
    }
    window::tell(words, text, error);
}

/// Extracts the archive this program carries and answers with what to say
/// about it, or nothing when the folder question was cancelled.
fn run(args: &Args, words: &Words, console: &mut bool) -> Result<Option<String>> {
    let exe = std::env::current_exe()?;
    let mut file = File::open(&exe)?;
    let trailer = Trailer::read(&mut file)?;
    *console |= trailer.console();
    let archive = ZipArchive::open(Window::new(file, trailer.offset, trailer.len))?;
    if archive.has_encrypted() {
        return Err(Error::Unsupported(words.encrypted.into()));
    }
    let entries = archive.entries().to_vec();
    drop(archive);
    let suggested = suggested(&exe);
    let dest = match &args.dest {
        Some(dest) => dest.clone(),
        None if *console => ask_console(words, &suggested)?,
        None => match window::ask(words, &exe, &suggested) {
            Some(dest) => dest,
            None => return Ok(None),
        },
    };
    let source = Window::new(File::open(&exe)?, trailer.offset, trailer.len);
    let files = extract(source, &entries, &dest)?;
    Ok(Some(fill(
        words.done,
        &[
            ("n", &files.to_string()),
            ("dir", &dest.display().to_string()),
        ],
    )))
}

/// A folder beside the program, named after it.
fn suggested(exe: &Path) -> PathBuf {
    let stem = exe
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "extracted".into());
    let beside = exe.with_file_name(&stem);
    if beside == exe {
        exe.with_file_name(format!("{stem}.files"))
    } else {
        beside
    }
}

fn ask_console(words: &Words, suggested: &Path) -> Result<PathBuf> {
    print!(
        "{}",
        fill(words.prompt, &[("dir", &suggested.display().to_string())])
    );
    io::stdout().flush()?;
    let mut line = String::new();
    io::stdin().read_line(&mut line)?;
    let typed = line.trim().trim_matches('"');
    Ok(if typed.is_empty() {
        suggested.to_path_buf()
    } else {
        PathBuf::from(typed)
    })
}

/// What the destination reads while another thread decompresses into the
/// other end of a pipe. Reaching the end waits for that thread, so a CRC that
/// does not match fails the read, and the file is never put in place.
struct Piped<'s> {
    reader: io::PipeReader,
    job: Option<ScopedJoinHandle<'s, Result<u64>>>,
}

impl Read for Piped<'_> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let n = self.reader.read(buf)?;
        if n == 0 && !buf.is_empty() {
            if let Some(job) = self.job.take() {
                match job.join() {
                    Ok(Ok(_)) => {}
                    Ok(Err(e)) => {
                        return Err(io::Error::new(io::ErrorKind::InvalidData, e.to_string()))
                    }
                    Err(_) => return Err(io::Error::other("the extraction stopped")),
                }
            }
        }
        Ok(n)
    }
}

/// Every entry into `dest`, replacing what is there. Answers with the number
/// of files written.
fn extract<R: Read + io::Seek + Send>(source: R, entries: &[Entry], dest: &Path) -> Result<usize> {
    std::fs::create_dir_all(dest)?;
    let mut source = BufReader::with_capacity(256 * 1024, source);
    let mut destination = Destination::new(dest);
    let mut files = 0;
    for entry in entries {
        if entry.is_dir {
            destination.write_entry(
                &entry.name,
                true,
                entry.mtime,
                &mut io::empty(),
                &mut |_| Conflict::Overwrite,
            )?;
            continue;
        }
        let (reader, writer) = io::pipe()?;
        let from = &mut source;
        let written = std::thread::scope(|scope| {
            let job = scope.spawn(move || arca_zip::extract_entry_with(from, entry, writer, None));
            let mut piped = Piped {
                reader,
                job: Some(job),
            };
            destination.write_entry(&entry.name, false, entry.mtime, &mut piped, &mut |_| {
                Conflict::Overwrite
            })
        })?;
        if written.is_some() {
            files += 1;
        }
    }
    Ok(files)
}

#[cfg(windows)]
mod window {
    use super::Words;
    use std::path::{Path, PathBuf};

    pub(super) fn ask(words: &Words, exe: &Path, suggested: &Path) -> Option<PathBuf> {
        let name = exe
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let mut dialog =
            rfd::FileDialog::new().set_title(super::fill(words.pick, &[("name", &name)]));
        if let Some(dir) = suggested.parent() {
            dialog = dialog.set_directory(dir);
        }
        dialog.pick_folder()
    }

    pub(super) fn tell(words: &Words, text: &str, error: bool) {
        rfd::MessageDialog::new()
            .set_level(if error {
                rfd::MessageLevel::Error
            } else {
                rfd::MessageLevel::Info
            })
            .set_title(words.title)
            .set_description(text)
            .set_buttons(rfd::MessageButtons::Ok)
            .show();
    }
}

#[cfg(not(windows))]
mod window {
    use super::Words;
    use std::path::{Path, PathBuf};

    pub(super) fn ask(_: &Words, _: &Path, _: &Path) -> Option<PathBuf> {
        None
    }

    pub(super) fn tell(_: &Words, text: &str, error: bool) {
        if error {
            eprintln!("{text}");
        } else {
            println!("{text}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Option<Args> {
        parse(list.iter().map(std::ffi::OsString::from))
    }

    #[test]
    fn arguments_are_a_folder_and_the_console_or_nothing() {
        assert!(args(&[]).is_some_and(|a| a.dest.is_none() && !a.console));
        let a = args(&["-o", "out dir", "--console"]).unwrap();
        assert_eq!(a.dest, Some(PathBuf::from("out dir")));
        assert!(a.console);
        assert!(args(&["--output", "x"]).is_some());
        assert!(args(&["-o"]).is_none());
        assert!(args(&["--what"]).is_none());
    }

    #[test]
    fn the_suggested_folder_never_is_the_program_itself() {
        assert_eq!(suggested(Path::new("d/setup.exe")), Path::new("d/setup"));
        assert_eq!(suggested(Path::new("d/setup")), Path::new("d/setup.files"));
    }
}
