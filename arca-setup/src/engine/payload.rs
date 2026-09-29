use arca_zip::ZipArchive;
use std::{io::Cursor, path::PathBuf};

static PAYLOAD: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/payload.zip"));

pub struct File {
    pub path: PathBuf,
    pub bytes: Vec<u8>,
}

pub fn files() -> Result<Vec<File>, String> {
    files_of(PAYLOAD)
}

fn files_of(zip: &[u8]) -> Result<Vec<File>, String> {
    if zip.is_empty() {
        return Err(
            "this installer carries no files; build it with windows/build-setup.ps1".to_string(),
        );
    }
    let mut archive =
        ZipArchive::open(Cursor::new(zip)).map_err(|e| format!("the installer's files: {e}"))?;
    let entries: Vec<(usize, String)> = archive
        .entries()
        .iter()
        .enumerate()
        .filter(|(_, entry)| !entry.is_dir)
        .map(|(index, entry)| (index, entry.name.clone()))
        .collect();
    let mut out = Vec::with_capacity(entries.len());
    for (index, name) in entries {
        let path = arca_core::safe_name(&name).map_err(|e| format!("{name}: {e}"))?;
        let mut bytes = Vec::new();
        archive
            .extract_to(index, &mut bytes)
            .map_err(|e| format!("{name}: {e}"))?;
        out.push(File { path, bytes });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use arca_core::{Codec, Level};

    fn zip_of(files: &[(&str, &[u8])]) -> Vec<u8> {
        let mut buffer = Cursor::new(Vec::new());
        let mut writer = arca_zip::ZipWriter::new(&mut buffer);
        for (name, data) in files {
            writer
                .add(name, *data, Codec::Deflate, Level::Fast, None)
                .unwrap();
        }
        writer.finish().unwrap();
        buffer.into_inner()
    }

    #[test]
    fn an_empty_payload_says_so() {
        let error = files_of(b"").err().expect("an error");
        assert!(error.contains("no files"));
    }

    #[test]
    fn a_truncated_payload_is_an_error_not_a_panic() {
        assert!(files_of(b"PK\x03\x04 and then nothing useful").is_err());
        assert!(files_of(&[0u8; 64]).is_err());
        let whole = zip_of(&[("arca.exe", b"MZ....")]);
        for cut in [1, whole.len() / 2, whole.len() - 30] {
            assert!(files_of(&whole[..cut]).is_err(), "cut at {cut}");
        }
    }

    #[test]
    fn files_come_out_with_their_folders_and_bytes() {
        let zip = zip_of(&[("arca.exe", b"one"), ("Assets/logo.png", b"two")]);
        let files = files_of(&zip).unwrap();
        assert_eq!(files.len(), 2);
        assert_eq!(files[0].path, PathBuf::from("arca.exe"));
        assert_eq!(files[0].bytes, b"one");
        assert_eq!(files[1].path, PathBuf::from("Assets").join("logo.png"));
        assert_eq!(files[1].bytes, b"two");
    }

    #[test]
    fn a_zip_with_an_escaping_name_is_refused() {
        let zip = zip_of(&[("../evil.txt", b"x")]);
        assert!(files_of(&zip).is_err());
    }
}
