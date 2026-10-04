use std::{fs, path::Path, process::Command};

use arca_7z::{create_7z, CreateOptions, SevenZArchive, Source};
use arca_core::Level;

fn seven_zip(cwd: &Path, args: &[&str]) {
    let result = Command::new("7z")
        .current_dir(cwd)
        .args(args)
        .output()
        .expect("install 7z to run interoperability tests");
    assert!(
        result.status.success(),
        "7z {args:?}: {} {}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
}

#[test]
#[ignore = "requires the external 7z binary"]
fn seven_zip_reads_arca_and_arca_reads_seven_zip() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    fs::create_dir(root.join("src")).unwrap();
    fs::create_dir(root.join("src/empty-dir")).unwrap();
    fs::write(root.join("src/empty-file"), b"").unwrap();
    fs::write(root.join("src/first"), b"hello from Arca").unwrap();
    fs::write(root.join("src/\u{00f1}.txt"), b"unicode entry").unwrap();
    let sources = ["empty-dir", "empty-file", "first", "\u{00f1}.txt"].map(|name| Source {
        path: root.join("src").join(name),
        name: name.into(),
    });
    for (level, password, hide_names) in [
        (Level::Store, None, false),
        (Level::Fast, Some("secret"), false),
        (Level::Normal, Some("secret"), true),
        (Level::Store, Some("secret"), true),
    ] {
        create_7z(
            &root.join("arca.7z"),
            &sources,
            &CreateOptions {
                level,
                password,
                hide_names,
            },
            &mut |_| true,
        )
        .unwrap();
        seven_zip(root, &["t", "-psecret", "arca.7z"]);
        seven_zip(root, &["x", "-y", "-psecret", "-oout", "arca.7z"]);
        assert_eq!(
            fs::read(root.join("out/first")).unwrap(),
            b"hello from Arca"
        );
        assert_eq!(
            fs::read(root.join("out/\u{00f1}.txt")).unwrap(),
            b"unicode entry"
        );
        assert!(root.join("out/empty-dir").is_dir());
        assert_eq!(fs::metadata(root.join("out/empty-file")).unwrap().len(), 0);
        fs::remove_dir_all(root.join("out")).unwrap();
    }
    for (method, password, hidden, solid) in [
        ("-m0=LZMA2", None, false, false),
        ("-m0=LZMA", None, false, true),
        ("-m0=LZMA2", Some("secret"), false, true),
        ("-m0=LZMA2", Some("secret"), true, true),
    ] {
        let mut args = vec![
            "a",
            "-t7z",
            method,
            "external.7z",
            "src",
            if solid { "-ms=on" } else { "-ms=off" },
        ];
        if password.is_some() {
            args.push("-psecret");
        }
        if hidden {
            args.push("-mhe=on");
        }
        seven_zip(root, &args);
        let path = root.join("external.7z");
        let mut archive = SevenZArchive::open(fs::File::open(&path).unwrap(), password).unwrap();
        assert_eq!(archive.has_encrypted_header(), hidden);
        assert_eq!(archive.is_solid(), solid);
        archive.test(&mut |_| true).unwrap();
        let indices = (0..archive.len()).collect::<Vec<_>>();
        let mut nonempty = 0;
        archive
            .extract(
                &indices,
                &mut |_, entry, reader| {
                    if entry.is_dir {
                        return Ok(());
                    }
                    let mut bytes = Vec::new();
                    reader.read_to_end(&mut bytes)?;
                    assert_eq!(bytes, fs::read(root.join(&entry.name)).unwrap());
                    nonempty += usize::from(!bytes.is_empty());
                    Ok(())
                },
                &mut |_| true,
            )
            .unwrap();
        assert_eq!(nonempty, 2);
        fs::remove_file(path).unwrap();
    }
}
