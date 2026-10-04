use std::{
    fs,
    io::{self, Read, Write},
    path::{Path, PathBuf},
};

use arca_core::{Error, Level, Result};
use sevenz_rust2::{
    encoder_options::{AesEncoderOptions, Lzma2Options},
    ArchiveEntry, ArchiveWriter, EncoderConfiguration, EncoderMethod, Password,
};

use crate::{entry_path, upstream, Phase, Progress};

pub struct Source {
    pub path: PathBuf,
    pub name: String,
}

pub struct CreateOptions<'a> {
    pub level: Level,
    pub password: Option<&'a str>,
    pub hide_names: bool,
}

impl Default for CreateOptions<'_> {
    fn default() -> Self {
        Self {
            level: Level::Normal,
            password: None,
            hide_names: false,
        }
    }
}

pub fn create_7z(
    output: &Path,
    sources: &[Source],
    options: &CreateOptions<'_>,
    notify: &mut dyn FnMut(Progress<'_>) -> bool,
) -> Result<()> {
    if sources.len() > 100_000 {
        return Err(Error::Limit("7z entry count".into()));
    }
    if options.hide_names && options.password.is_none_or(str::is_empty) {
        return Err(Error::Format(
            "hiding 7z names requires a nonempty password".into(),
        ));
    }
    if options.password.is_some_and(str::is_empty) {
        return Err(Error::Format(
            "7z encryption requires a nonempty password".into(),
        ));
    }
    let mut names = std::collections::HashSet::new();
    let mut metadata = Vec::with_capacity(sources.len());
    let mut header_size = 0usize;
    for source in sources {
        if !names.insert(entry_path(&source.name)?) {
            return Err(Error::Format("duplicate 7z entry name".into()));
        }
        header_size = header_size
            .checked_add(source.name.encode_utf16().count() * 2 + 128)
            .ok_or_else(|| Error::Limit("7z header size".into()))?;
        if header_size > 16 * 1024 * 1024 {
            return Err(Error::Limit("7z header size".into()));
        }
        let meta = fs::symlink_metadata(&source.path)?;
        if !meta.is_file() && !meta.is_dir() {
            return Err(Error::Unsupported(
                "7z source is not a regular file or directory".into(),
            ));
        }
        metadata.push(meta);
    }
    if options.password.is_some()
        && !options.hide_names
        && !metadata.iter().any(|m| m.is_file() && m.len() > 0)
    {
        return Err(Error::Unsupported(
            "encrypting only empty entries requires hide_names to validate the password".into(),
        ));
    }
    if !notify(Progress {
        phase: Phase::Create,
        entries_done: 0,
        entries_total: sources.len(),
        bytes_done: 0,
        name: "",
    }) {
        return Err(Error::Cancelled);
    }
    let parent = output
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
    let mut writer = ArchiveWriter::new(io::BufWriter::new(temporary.as_file_mut()))
        .map_err(|e| upstream(e, false))?;
    writer.set_encrypt_header(options.hide_names);
    let mut bytes = 0;
    for (index, (source, meta)) in sources.iter().zip(&metadata).enumerate() {
        writer.set_content_methods(methods(options)?);
        let mut entry = if meta.is_dir() {
            ArchiveEntry::new_directory(&source.name)
        } else {
            ArchiveEntry::new_file(&source.name)
        };
        if let Ok(time) = meta.modified().and_then(|t| {
            sevenz_rust2::NtTime::try_from(t)
                .map_err(|_| io::Error::other("timestamp outside 7z range"))
        }) {
            entry.has_last_modified_date = true;
            entry.last_modified_date = time;
        }
        let progress = Progress {
            phase: Phase::Create,
            entries_done: index,
            entries_total: sources.len(),
            bytes_done: bytes,
            name: &source.name,
        };
        if !notify(progress) {
            return Err(Error::Cancelled);
        }
        if meta.is_dir() || meta.len() == 0 {
            entry.has_stream = false;
            writer
                .push_archive_entry::<io::Empty>(entry, None)
                .map_err(|e| upstream(e, false))?;
        } else {
            let file = fs::File::open(&source.path)?;
            let mut input = Input {
                file,
                notify,
                progress,
                cancelled: false,
                bytes: 0,
            };
            let result = writer.push_archive_entry(entry, Some(&mut input));
            if input.cancelled {
                return Err(Error::Cancelled);
            }
            result.map_err(|e| upstream(e, false))?;
            if input.bytes != meta.len() {
                return Err(Error::Io(io::Error::other(
                    "7z source changed size during creation",
                )));
            }
            bytes += input.bytes;
        }
    }
    // A header must never reuse the last file's AES salt and IV.
    writer.set_content_methods(methods(options)?);
    writer.finish()?.flush()?;
    if !notify(Progress {
        phase: Phase::Create,
        entries_done: sources.len(),
        entries_total: sources.len(),
        bytes_done: bytes,
        name: "",
    }) {
        return Err(Error::Cancelled);
    }
    temporary.as_file().sync_all()?;
    temporary.persist(output).map_err(|e| Error::Io(e.error))?;
    Ok(())
}

fn methods(options: &CreateOptions<'_>) -> Result<Vec<EncoderConfiguration>> {
    let mut methods = Vec::new();
    if let Some(password) = options.password {
        let mut iv = [0; 16];
        let mut salt = [0; 16];
        getrandom::fill(&mut iv)
            .and_then(|()| getrandom::fill(&mut salt))
            .map_err(|e| Error::Io(io::Error::other(e.to_string())))?;
        methods.push(
            AesEncoderOptions {
                password: Password::from(password),
                iv,
                salt,
                num_cycles_power: 19,
            }
            .into(),
        );
    }
    methods.push(match options.level {
        Level::Store => EncoderMethod::COPY.into(),
        Level::Fast => Lzma2Options::from_level(1).into(),
        Level::Normal => Lzma2Options::from_level(6).into(),
        Level::Best => Lzma2Options::from_level(9).into(),
    });
    Ok(methods)
}

struct Input<'a> {
    file: fs::File,
    notify: &'a mut dyn FnMut(Progress<'_>) -> bool,
    progress: Progress<'a>,
    cancelled: bool,
    bytes: u64,
}

impl Read for Input<'_> {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        let mut progress = self.progress;
        progress.bytes_done += self.bytes;
        if !(self.notify)(progress) {
            self.cancelled = true;
            return Err(io::Error::other("cancelled"));
        }
        let size = self.file.read(buffer)?;
        self.bytes += size as u64;
        Ok(size)
    }
}
