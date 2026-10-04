use crate::{archive_ops::compress, Format};
use arca_core::{Codec, Level};
use std::path::{Path, PathBuf};

pub(crate) struct Room(pub(crate) PathBuf);

impl Room {
    pub(crate) fn new() -> Self {
        static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let serial = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let temp = std::fs::canonicalize(std::env::temp_dir()).unwrap();
        let temp = match temp.to_str().and_then(|t| t.strip_prefix(r"\\?\")) {
            Some(plain) => PathBuf::from(plain),
            None => temp,
        };
        let path = temp.join(format!("arca-gui-7z-{}-{serial}", std::process::id()));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }

    pub(crate) fn archive(&self, password: Option<&str>, hidden: bool) -> PathBuf {
        let source = self.0.join("source");
        std::fs::create_dir_all(source.join("empty")).unwrap();
        std::fs::write(source.join("a.txt"), b"first contents").unwrap();
        std::fs::write(source.join("b.txt"), b"second contents").unwrap();
        let out = self.0.join(format!(
            "{}.7z",
            self.0.file_name().unwrap().to_string_lossy()
        ));
        compress(
            &out,
            &[source],
            Format::SevenZ,
            Codec::Deflate,
            Level::Normal,
            &|_, _, _| true,
            (password, hidden),
        )
        .unwrap();
        out
    }

    pub(crate) fn path(&self, name: &str) -> PathBuf {
        self.0.join(Path::new(name))
    }
}

impl Drop for Room {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
