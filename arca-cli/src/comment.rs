use super::{detect, detect_archive, printable, split_is_read_only, staged_path, tar_source};
use arca_core::{Container, Error, Format, Result};
use arca_tar::TarReader;
use arca_zip::ZipArchive;
use std::fs;
use std::io::{self, Read};
use std::path::PathBuf;

pub(crate) struct Request {
    pub archive: PathBuf,
    pub set: Option<String>,
    pub file: Option<PathBuf>,
    pub clear: bool,
    pub entry: Option<String>,
    pub out: Option<PathBuf>,
    pub password: Option<String>,
}

pub(crate) fn run(r: Request) -> Result<()> {
    let text = match (&r.set, &r.file, r.clear) {
        (Some(text), _, _) => Some(text.clone().into_bytes()),
        (_, Some(path), _) if path.as_os_str() == "-" => {
            let mut bytes = Vec::new();
            io::stdin().lock().read_to_end(&mut bytes)?;
            Some(bytes)
        }
        (_, Some(path), _) => Some(fs::read(path)?),
        (_, _, true) => Some(Vec::new()),
        _ => None,
    };
    match text {
        Some(text) => write(&r, &text),
        None => show(&r),
    }
}

fn same(a: &str, b: &str) -> bool {
    let tidy = |s: &str| s.replace('\\', "/").trim_end_matches('/').to_string();
    tidy(a) == tidy(b)
}

fn no_entry(name: &str) -> Error {
    Error::Format(format!("no entry named '{}'", printable(name)))
}

fn show(r: &Request) -> Result<()> {
    let archive = &r.archive;
    let entry = r.entry.as_deref();
    let comment = match detect_archive(archive)?.container() {
        Container::Zip => {
            let a = ZipArchive::open(arca_core::volumes::open(archive)?)?;
            match entry {
                None => Some(a.comment()),
                Some(name) => {
                    let i = a
                        .entries()
                        .iter()
                        .position(|e| same(&e.name, name))
                        .ok_or_else(|| no_entry(name))?;
                    a.entry_comment(i)
                }
            }
        }
        Container::Rar if entry.is_none() => {
            arca_rar::RarArchive::open(archive, r.password.as_deref())?
                .comment(r.password.as_deref())?
        }
        Container::Rar => {
            return Err(Error::Unsupported(
                "Arca reads the comment of a RAR archive, not of its entries".into(),
            ))
        }
        Container::Tar
        | Container::TarGz
        | Container::TarXz
        | Container::TarBz2
        | Container::TarZst => {
            let format = detect_archive(archive)?;
            let mut reader = TarReader::new(tar_source(archive, format)?);
            let mut found = None;
            let mut lines = Vec::new();
            while let Some(e) = reader.next_entry()? {
                if let Some(name) = entry {
                    if same(&e.entry.name, name) {
                        found = Some(e.comment.clone());
                    }
                } else if let Some(c) = &e.comment {
                    lines.push(format!(
                        "{}: {}",
                        printable(&e.entry.name).replace('\n', " "),
                        printable(c).replace('\n', " ")
                    ));
                }
                reader.skip_data(&e)?;
            }
            match entry {
                Some(name) => found.ok_or_else(|| no_entry(name))?,
                None => Some(lines.join("\n")),
            }
        }
        _ => {
            return Err(Error::Unsupported(
                "only ZIP, RAR and TAR archives carry comments".into(),
            ))
        }
    };
    if let Some(text) = comment.filter(|c| !c.is_empty()) {
        println!("{}", printable(&text).trim_end_matches('\n'));
    }
    Ok(())
}

fn write(r: &Request, text: &[u8]) -> Result<()> {
    let archive = &r.archive;
    if arca_core::volumes::is_part(archive) {
        return Err(split_is_read_only());
    }
    if detect(archive)? != Format::Zip {
        return Err(Error::Unsupported(
            "only ZIP comments can be changed".into(),
        ));
    }
    if let Some(out) = &r.out {
        if detect(out)? != Format::Zip {
            return Err(Error::Unsupported(
                "a comment rewrite writes a ZIP, so --out needs a .zip name".into(),
            ));
        }
    }
    let comments = arca_zip::Comments {
        archive: r.entry.is_none().then_some(text),
        entry: r.entry.as_deref().map(|name| (name, text)),
    };
    let target = r.out.as_deref().unwrap_or(archive);
    let temp = staged_path(target);
    if let Err(e) = arca_zip::set_comments(archive, &temp, &comments, &|_, _, _| true) {
        let _ = fs::remove_file(&temp);
        return Err(e);
    }
    fs::rename(&temp, target)?;
    let what = match &r.entry {
        Some(name) => format!("comment on {}", printable(name)),
        None => "archive comment".to_string(),
    };
    println!(
        "{}: {what} {}",
        target.display(),
        if text.is_empty() { "cleared" } else { "set" }
    );
    Ok(())
}

pub(crate) fn notes_for_rar(
    a: &arca_rar::RarArchive,
    password: Option<&str>,
    notes: &mut Vec<String>,
) {
    if let Ok(Some(comment)) = a.comment(password) {
        if !comment.is_empty() {
            notes.push("archive comment:".into());
            notes.extend(printable(&comment).lines().map(|l| format!("  {l}")));
        }
    }
}

pub(crate) fn note_for_tar(e: &arca_tar::TarEntry, notes: &mut Vec<String>) {
    if let Some(c) = &e.comment {
        notes.push(format!(
            "comment on {}: {}",
            printable(&e.entry.name).replace('\n', " "),
            printable(c).replace('\n', " ")
        ));
    }
}
