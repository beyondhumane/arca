use super::*;
use arca_core::{Entry, Error};
use std::io::{Read, Write};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, TryRecvError};
use std::sync::Arc;

const IMAGE_BYTES_LIMIT: u64 = 64 * 1024 * 1024;
const IMAGE_DIMENSION_LIMIT: u32 = 8192;

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum PreviewStatus {
    Hidden,
    Empty,
    Loading,
    Ready,
    PasswordRequired { wrong: bool },
    Unsupported(String),
    Oversized { limit: u64 },
    Error(String),
}

pub(crate) struct PreviewState {
    pub(crate) status: PreviewStatus,
    pub(crate) entry: Option<Entry>,
    pub(crate) index: Option<usize>,
    pub(crate) generation: u64,
    pub(crate) image: Option<Arc<image::RgbaImage>>,
    clock: Arc<AtomicU64>,
    running: Option<Receiver<PreviewResult>>,
    pending: Option<PreviewRequest>,
    context: Option<PreviewContext>,
}

impl Default for PreviewState {
    fn default() -> Self {
        Self {
            status: PreviewStatus::Hidden,
            entry: None,
            index: None,
            generation: 0,
            image: None,
            clock: Arc::new(AtomicU64::new(0)),
            running: None,
            pending: None,
            context: None,
        }
    }
}

impl Drop for PreviewState {
    fn drop(&mut self) {
        self.clock.fetch_add(1, Ordering::Relaxed);
    }
}

#[derive(PartialEq, Eq)]
struct PreviewContext {
    archive: Option<PathBuf>,
    directory: String,
    cursor: Option<usize>,
    checked: Vec<bool>,
    filter: String,
}

struct PreviewRequest {
    archive: PathBuf,
    index: usize,
    entry: Entry,
    password: Option<String>,
    generation: u64,
    clock: Arc<AtomicU64>,
}

struct PreviewResult {
    generation: u64,
    outcome: Result<PreviewContent, PreviewStatus>,
}

struct PreviewContent {
    viewed: Viewed,
    image: Option<Arc<image::RgbaImage>>,
}

impl AppController {
    pub(crate) fn request_preview(&mut self, index: usize) {
        self.cancel_preview();
        self.state.preview.status = PreviewStatus::Empty;
        let (Some(archive), Some(entry)) = (
            self.state.archive.clone(),
            self.state.entries.get(index).cloned(),
        ) else {
            return;
        };
        if entry.is_dir {
            return;
        }
        self.state.preview.entry = Some(entry.clone());
        self.state.preview.index = Some(index);
        self.state.preview.context = Some(self.preview_context());
        if entry.size > VIEW_LIMIT {
            self.state.preview.status = PreviewStatus::Oversized { limit: VIEW_LIMIT };
            return;
        }
        if entry.encrypted && self.state.archive_password.is_none() {
            self.state.preview.status = PreviewStatus::PasswordRequired { wrong: false };
            return;
        }
        let request = PreviewRequest {
            archive,
            index,
            entry,
            password: self.state.archive_password.clone(),
            generation: self.state.preview.generation,
            clock: self.state.preview.clock.clone(),
        };
        self.state.preview.status = PreviewStatus::Loading;
        if self.state.preview.running.is_some() {
            self.state.preview.pending = Some(request);
        } else {
            self.start_preview(request);
        }
    }

    pub(crate) fn cancel_preview(&mut self) {
        let preview = &mut self.state.preview;
        preview.generation = preview.generation.wrapping_add(1);
        preview.clock.store(preview.generation, Ordering::Relaxed);
        preview.pending = None;
        preview.context = None;
        preview.entry = None;
        preview.index = None;
        preview.image = None;
        preview.status = PreviewStatus::Hidden;
        self.state.viewing = None;
    }

    pub(crate) fn poll_preview(&mut self) -> bool {
        let mut changed = false;
        if self.state.preview.context.as_ref().is_some_and(|c| {
            c.archive != self.state.archive
                || c.directory != self.state.current_dir
                || c.cursor != self.state.cursor
                || c.checked != self.state.checked
                || c.filter != self.state.filter
        }) {
            self.cancel_preview();
            changed = true;
        }
        let Some(rx) = self.state.preview.running.as_ref() else {
            return changed;
        };
        let result = match rx.try_recv() {
            Ok(result) => Some(result),
            Err(TryRecvError::Empty) => return changed,
            Err(TryRecvError::Disconnected) => None,
        };
        self.state.preview.running = None;
        if let Some(result) = result {
            if result.generation == self.state.preview.generation {
                match result.outcome {
                    Ok(content) => {
                        self.state.preview.status = if content.viewed.bytes.is_empty() {
                            PreviewStatus::Empty
                        } else {
                            PreviewStatus::Ready
                        };
                        self.state.viewing = Some(content.viewed);
                        self.state.preview.image = content.image;
                    }
                    Err(status) => self.state.preview.status = status,
                }
                changed = true;
            }
        } else if self.state.preview.pending.is_none()
            && self.state.preview.status == PreviewStatus::Loading
        {
            self.state.preview.status = PreviewStatus::Error("preview worker disconnected".into());
            changed = true;
        }
        if let Some(request) = self.state.preview.pending.take() {
            self.start_preview(request);
        }
        changed
    }

    fn preview_context(&self) -> PreviewContext {
        PreviewContext {
            archive: self.state.archive.clone(),
            directory: self.state.current_dir.clone(),
            cursor: self.state.cursor,
            checked: self.state.checked.clone(),
            filter: self.state.filter.clone(),
        }
    }

    fn start_preview(&mut self, request: PreviewRequest) {
        let (tx, rx) = channel();
        self.state.preview.running = Some(rx);
        std::thread::spawn(move || {
            let outcome = read_preview(&request);
            let _ = tx.send(PreviewResult {
                generation: request.generation,
                outcome,
            });
        });
    }
}

fn preview_error(error: Error) -> PreviewStatus {
    match error {
        Error::PasswordRequired => PreviewStatus::PasswordRequired { wrong: false },
        Error::BadPassword => PreviewStatus::PasswordRequired { wrong: true },
        // The ZIP engine predates Error::BadPassword and still returns Format.
        Error::Format(text) if text.ends_with("': wrong password") => {
            PreviewStatus::PasswordRequired { wrong: true }
        }
        Error::Unsupported(text) => PreviewStatus::Unsupported(text),
        Error::Limit(_) => PreviewStatus::Oversized { limit: VIEW_LIMIT },
        other => PreviewStatus::Error(other.to_string()),
    }
}

fn read_preview(request: &PreviewRequest) -> Result<PreviewContent, PreviewStatus> {
    let bytes = read_bounded(request).map_err(preview_error)?;
    if request.clock.load(Ordering::Relaxed) != request.generation {
        return Err(PreviewStatus::Hidden);
    }
    let name = request
        .entry
        .name
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(&request.entry.name);
    let image = if looks_like_picture(name) {
        decode_picture(&bytes)?
    } else {
        None
    };
    let picture = image.is_some();
    let look = if picture {
        Look::Picture
    } else if looks_like_text(&bytes) {
        Look::Text
    } else {
        Look::Hex
    };
    let lines = String::from_utf8_lossy(&bytes)
        .lines()
        .map(str::to_string)
        .collect();
    Ok(PreviewContent {
        viewed: Viewed {
            name: name.to_string(),
            bytes: bytes.into(),
            look,
            lines,
            picture,
        },
        image,
    })
}

fn decode_picture(bytes: &[u8]) -> Result<Option<Arc<image::RgbaImage>>, PreviewStatus> {
    let format = image::guess_format(bytes).map_err(|e| PreviewStatus::Error(e.to_string()))?;
    if !matches!(
        format,
        image::ImageFormat::Png
            | image::ImageFormat::Jpeg
            | image::ImageFormat::Gif
            | image::ImageFormat::Bmp
            | image::ImageFormat::WebP
    ) {
        return Err(PreviewStatus::Unsupported(format!("{format:?}")));
    }
    let reader = image::ImageReader::with_format(std::io::Cursor::new(bytes), format);
    let (width, height) = reader
        .into_dimensions()
        .map_err(|e| PreviewStatus::Error(e.to_string()))?;
    if width > IMAGE_DIMENSION_LIMIT
        || height > IMAGE_DIMENSION_LIMIT
        || u64::from(width) * u64::from(height) * 4 > IMAGE_BYTES_LIMIT
    {
        return Err(PreviewStatus::Oversized {
            limit: IMAGE_BYTES_LIMIT,
        });
    }
    let mut reader = image::ImageReader::with_format(std::io::Cursor::new(bytes), format);
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(IMAGE_DIMENSION_LIMIT);
    limits.max_image_height = Some(IMAGE_DIMENSION_LIMIT);
    limits.max_alloc = Some(IMAGE_BYTES_LIMIT);
    reader.limits(limits);
    let image = reader.decode().map_err(|e| match e {
        image::ImageError::Limits(_) => PreviewStatus::Oversized {
            limit: IMAGE_BYTES_LIMIT,
        },
        other => PreviewStatus::Error(other.to_string()),
    })?;
    Ok(Some(Arc::new(image.into_rgba8())))
}

struct BoundedBytes<'a> {
    bytes: Vec<u8>,
    request: &'a PreviewRequest,
    exceeded: bool,
}

impl Write for BoundedBytes<'_> {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if self.request.clock.load(Ordering::Relaxed) != self.request.generation {
            return Err(std::io::Error::other("preview cancelled"));
        }
        if bytes.len() > VIEW_LIMIT as usize - self.bytes.len() {
            self.exceeded = true;
            return Err(std::io::Error::other("preview byte limit exceeded"));
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

struct CancellableRead<'a, R> {
    inner: R,
    request: &'a PreviewRequest,
}

impl<R: Read> Read for CancellableRead<'_, R> {
    fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
        if self.request.clock.load(Ordering::Relaxed) != self.request.generation {
            return Err(std::io::Error::other("preview cancelled"));
        }
        self.inner.read(out)
    }
}

fn verify_entry(actual: &Entry, expected: &Entry) -> arca_core::Result<()> {
    let same_name = if !actual.raw_name.is_empty() && !expected.raw_name.is_empty() {
        actual.raw_name == expected.raw_name
    } else {
        actual.name == expected.name
    };
    if !same_name
        || actual.size != expected.size
        || actual.crc32 != expected.crc32
        || actual.is_dir != expected.is_dir
        || actual.encrypted != expected.encrypted
        || actual.method != expected.method
    {
        return Err(Error::Format("entry changed since listing".into()));
    }
    if actual.size > VIEW_LIMIT {
        return Err(Error::Limit("preview byte limit exceeded".into()));
    }
    Ok(())
}

fn read_bounded(request: &PreviewRequest) -> arca_core::Result<Vec<u8>> {
    let format =
        detect(&request.archive).ok_or_else(|| Error::Unsupported("unknown format".into()))?;
    let mut out = BoundedBytes {
        bytes: Vec::new(),
        request,
        exceeded: false,
    };
    let result = match format {
        Format::Zip => {
            let mut archive = arca_zip::ZipArchive::open(fs::File::open(&request.archive)?)?;
            let entry = archive
                .entries()
                .get(request.index)
                .ok_or_else(|| Error::Format("entry missing".into()))?;
            verify_entry(entry, &request.entry)?;
            archive
                .extract_to_with(request.index, &mut out, request.password.as_deref())
                .map(|_| ())
        }
        Format::Tar | Format::TarGz => {
            let mut archive = arca_tar::TarReader::new(CancellableRead {
                inner: open_source(&request.archive, format)?,
                request,
            });
            let mut index = 0;
            loop {
                let entry = archive
                    .next_entry()?
                    .ok_or_else(|| Error::Format("entry missing".into()))?;
                if index == request.index {
                    verify_entry(&entry.entry, &request.entry)?;
                    break archive.copy_data(&entry, &mut out).map(|_| ());
                }
                archive.skip_data(&entry)?;
                index += 1;
            }
        }
        Format::Rar => {
            let archive = arca_rar::RarArchive::open_with_progress(
                &request.archive,
                request.password.as_deref(),
                &|_, _, _| request.clock.load(Ordering::Relaxed) == request.generation,
            )?;
            let entry = archive
                .entries()
                .get(request.index)
                .ok_or_else(|| Error::Format("entry missing".into()))?;
            verify_entry(entry, &request.entry)?;
            let bytes = archive.read_entry(request.index, request.password.as_deref())?;
            out.write_all(&bytes).map_err(Error::from)
        }
    };
    if out.exceeded {
        return Err(Error::Limit("preview byte limit exceeded".into()));
    }
    result?;
    Ok(out.bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;
    use std::time::{Duration, Instant};

    static NEXT_FIXTURE: AtomicU64 = AtomicU64::new(0);

    struct Fixture(PathBuf);

    impl Fixture {
        fn zip(password: Option<&str>) -> Self {
            let path = std::env::temp_dir().join(format!(
                "arca-preview-{}-{}.zip",
                std::process::id(),
                NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed)
            ));
            let mut zip = arca_zip::ZipWriter::new(fs::File::create(&path).unwrap());
            for (name, bytes) in [
                ("first.txt", b"first\nline".as_slice()),
                ("second.txt", b"second"),
                ("empty.txt", b""),
                ("nested/deep/child.txt", b"child"),
            ] {
                zip.add_with_password(
                    name,
                    bytes,
                    arca_core::Codec::Store,
                    arca_core::Level::Store,
                    None,
                    password,
                )
                .unwrap();
            }
            zip.finish().unwrap();
            Self(path)
        }

        fn controller(&self) -> AppController {
            let mut c = AppController::new(Settings::default());
            c.state.archive = Some(self.0.clone());
            c.state.entries = list_entries(&self.0, None, &|_, _, _| true).unwrap();
            c.state.checked = vec![false; c.state.entries.len()];
            c
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_file(&self.0);
        }
    }

    fn settle(c: &mut AppController) {
        let deadline = Instant::now() + Duration::from_secs(10);
        while c.state.preview.running.is_some() {
            c.poll_preview();
            assert!(Instant::now() < deadline, "preview worker did not finish");
            std::thread::sleep(Duration::from_millis(1));
        }
    }

    fn fake_running(c: &mut AppController) -> std::sync::mpsc::Sender<PreviewResult> {
        c.cancel_preview();
        c.state.preview.context = Some(c.preview_context());
        c.state.preview.status = PreviewStatus::Loading;
        let (tx, rx) = channel();
        c.state.preview.running = Some(rx);
        tx
    }

    fn fake_content(generation: u64) -> PreviewResult {
        PreviewResult {
            generation,
            outcome: Ok(PreviewContent {
                viewed: Viewed {
                    name: "stale.txt".into(),
                    bytes: b"old".to_vec().into(),
                    look: Look::Text,
                    lines: vec!["old".into()],
                    picture: false,
                },
                image: None,
            }),
        }
    }

    #[test]
    fn real_zip_preview_is_independent_of_mutation_channel_and_handles_empty_files() {
        let fixture = Fixture::zip(None);
        let mut c = fixture.controller();
        let (_job_tx, job_rx) = channel();
        c.state.channel = Some(job_rx);
        c.request_preview(0);
        assert_eq!(c.state.preview.status, PreviewStatus::Loading);
        assert!(!c.state.busy);
        settle(&mut c);
        assert_eq!(c.state.preview.status, PreviewStatus::Ready);
        assert_eq!(
            c.state.viewing.as_ref().unwrap().bytes.as_ref(),
            b"first\nline"
        );
        assert!(c.state.channel.is_some());
        c.request_preview(2);
        settle(&mut c);
        assert_eq!(c.state.preview.status, PreviewStatus::Empty);
        assert!(c.state.viewing.as_ref().unwrap().bytes.is_empty());
    }

    #[test]
    fn rapid_requests_keep_one_worker_and_only_the_latest_pending_request() {
        let fixture = Fixture::zip(None);
        let mut c = fixture.controller();
        let tx = fake_running(&mut c);
        let stale_generation = c.state.preview.generation;
        for _ in 0..100 {
            c.request_preview(0);
        }
        c.request_preview(1);
        assert_eq!(c.state.preview.pending.as_ref().unwrap().index, 1);
        assert!(c.state.viewing.is_none());
        tx.send(fake_content(stale_generation)).unwrap();
        c.poll_preview();
        assert!(c.state.preview.pending.is_none());
        assert!(c.state.preview.running.is_some());
        assert!(c.state.viewing.is_none());
        settle(&mut c);
        assert_eq!(c.state.viewing.as_ref().unwrap().name, "second.txt");
    }

    #[test]
    fn navigation_selection_close_refresh_and_replacement_reject_stale_results() {
        let fixture = Fixture::zip(None);
        for mutation in 0..7 {
            let mut c = fixture.controller();
            let tx = fake_running(&mut c);
            let generation = c.state.preview.generation;
            match mutation {
                0 => c.go_to(String::new()),
                1 => c.state.checked[0] = true,
                2 => c.state.cursor = Some(1),
                3 => c.state.filter = "second".into(),
                4 => c.cancel_preview(),
                5 => c.reset_browser_panes(),
                _ => c.state.archive = Some(PathBuf::from("other.zip")),
            }
            tx.send(fake_content(generation)).unwrap();
            c.poll_preview();
            assert!(c.state.viewing.is_none());
            assert_eq!(c.state.preview.status, PreviewStatus::Hidden);
        }
    }

    #[test]
    fn encrypted_preview_reports_required_wrong_and_successful_passwords() {
        let fixture = Fixture::zip(Some("preview-test-only"));
        let mut c = fixture.controller();
        c.request_preview(0);
        assert_eq!(
            c.state.preview.status,
            PreviewStatus::PasswordRequired { wrong: false }
        );
        assert!(c.state.preview.running.is_none());
        c.state.archive_password = Some("wrong".into());
        c.request_preview(0);
        settle(&mut c);
        assert_eq!(
            c.state.preview.status,
            PreviewStatus::PasswordRequired { wrong: true }
        );
        c.state.archive_password = Some("preview-test-only".into());
        c.request_preview(0);
        settle(&mut c);
        assert_eq!(c.state.preview.status, PreviewStatus::Ready);
    }

    #[test]
    fn size_checks_identity_validation_and_output_bounds_prevent_unbounded_reads() {
        let fixture = Fixture::zip(None);
        let mut c = fixture.controller();
        c.state.entries[0].size = VIEW_LIMIT + 1;
        c.request_preview(0);
        assert_eq!(
            c.state.preview.status,
            PreviewStatus::Oversized { limit: VIEW_LIMIT }
        );
        assert!(c.state.preview.running.is_none());
        c.state.entries[0].size = 1;
        c.request_preview(0);
        settle(&mut c);
        assert!(matches!(c.state.preview.status, PreviewStatus::Error(_)));
        let request = PreviewRequest {
            archive: fixture.0.clone(),
            index: 0,
            entry: c.state.entries[0].clone(),
            password: None,
            generation: 0,
            clock: Arc::new(AtomicU64::new(0)),
        };
        let mut sink = BoundedBytes {
            bytes: vec![0; VIEW_LIMIT as usize - 1],
            request: &request,
            exceeded: false,
        };
        assert!(sink.write_all(&[1, 2]).is_err());
        assert!(sink.exceeded);
        assert_eq!(sink.bytes.len(), VIEW_LIMIT as usize - 1);
        request.clock.store(1, Ordering::Relaxed);
        assert!(sink.write_all(&[1]).is_err());
        let mut reader = CancellableRead {
            inner: Cursor::new(b"bytes"),
            request: &request,
        };
        assert!(reader.read(&mut [0]).is_err());
    }

    #[test]
    fn image_decoding_enforces_dimensions_and_handles_malformed_bytes() {
        let mut bytes = Cursor::new(Vec::new());
        image::DynamicImage::new_rgba8(2, 3)
            .write_to(&mut bytes, image::ImageFormat::Png)
            .unwrap();
        assert_eq!(
            decode_picture(bytes.get_ref())
                .unwrap()
                .unwrap()
                .dimensions(),
            (2, 3)
        );
        let mut bytes = Cursor::new(Vec::new());
        image::DynamicImage::new_rgba8(IMAGE_DIMENSION_LIMIT + 1, 1)
            .write_to(&mut bytes, image::ImageFormat::Png)
            .unwrap();
        assert!(matches!(
            decode_picture(bytes.get_ref()),
            Err(PreviewStatus::Oversized { .. })
        ));
        assert!(matches!(
            decode_picture(b"not a picture"),
            Err(PreviewStatus::Error(_))
        ));
        assert!(matches!(
            decode_picture(b"II\x2a\0\0\0\0\0"),
            Err(PreviewStatus::Unsupported(_))
        ));
    }

    #[test]
    fn disconnected_worker_is_an_explicit_error_not_permanent_loading() {
        let fixture = Fixture::zip(None);
        let mut c = fixture.controller();
        drop(fake_running(&mut c));
        assert!(c.poll_preview());
        assert!(matches!(c.state.preview.status, PreviewStatus::Error(_)));
        assert!(!c.poll_preview());
    }

    #[test]
    fn dropping_preview_cancels_the_worker_clock_and_pending_request() {
        let fixture = Fixture::zip(None);
        let mut c = fixture.controller();
        let tx = fake_running(&mut c);
        c.request_preview(0);
        let clock = c.state.preview.clock.clone();
        let generation = c.state.preview.generation;
        drop(c);
        assert_ne!(clock.load(Ordering::Relaxed), generation);
        assert!(tx.send(fake_content(generation)).is_err());
    }

    #[test]
    fn refresh_keeps_the_nested_directory_but_rebuilds_selection_and_preview() {
        let fixture = Fixture::zip(None);
        let mut c = fixture.controller();
        c.go_to("nested/deep/".into());
        c.select_all_visible();
        c.request_preview(3);
        settle(&mut c);
        c.refresh();
        let deadline = Instant::now() + Duration::from_secs(10);
        while c.state.busy {
            c.receive();
            assert!(Instant::now() < deadline, "refresh did not finish");
            std::thread::sleep(Duration::from_millis(1));
        }
        assert_eq!(c.state.current_dir, "nested/deep/");
        assert_eq!(c.state.browser.panes.len(), 3);
        assert!(c.selected_names().is_empty());
        assert!(c.state.viewing.is_none());
        assert_eq!(c.state.preview.status, PreviewStatus::Hidden);
    }

    #[test]
    fn readonly_mutations_are_rejected_before_starting_a_job_or_cancelling_preview() {
        for name in ["readonly.rar", "readonly.tar", "readonly.tar.gz"] {
            let mut c = AppController::new(Settings::default());
            c.state.preview.status = PreviewStatus::Empty;
            let archive = PathBuf::from(name);
            for job in [
                Job::Password {
                    archive: archive.clone(),
                    current: None,
                    new: None,
                },
                Job::Delete {
                    archive: archive.clone(),
                    names: vec![],
                    password: None,
                },
                Job::Rename {
                    archive: archive.clone(),
                    from: "a".into(),
                    to: "b".into(),
                    folder: false,
                    password: None,
                },
                Job::Move {
                    archive: archive.clone(),
                    moves: vec![],
                    password: None,
                },
                Job::NewFolder {
                    archive: archive.clone(),
                    name: "new/".into(),
                    password: None,
                },
                Job::Add {
                    archive: archive.clone(),
                    inputs: vec![],
                    dir: String::new(),
                    codec: Codec::Store,
                    level: Level::Store,
                    password: None,
                },
            ] {
                c.run_job(job);
                assert!(c.state.error);
                assert!(!c.state.busy);
                assert!(c.state.channel.is_none());
                assert!(c.state.undo.is_none());
                assert_eq!(c.state.preview.status, PreviewStatus::Empty);
            }
        }
    }

    #[test]
    fn real_tar_preview_checks_index_and_entry_identity() {
        let path = std::env::temp_dir().join(format!(
            "arca-preview-{}-{}.tar",
            std::process::id(),
            NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed)
        ));
        let fixture = Fixture(path);
        let mut tar = arca_tar::TarWriter::new(fs::File::create(&fixture.0).unwrap());
        tar.add("first.txt", 5, 0, 0o644, b"first".as_slice())
            .unwrap();
        tar.add("second.txt", 6, 0, 0o644, b"second".as_slice())
            .unwrap();
        tar.finish().unwrap();
        let mut c = fixture.controller();
        c.request_preview(1);
        settle(&mut c);
        assert_eq!(c.state.viewing.as_ref().unwrap().bytes.as_ref(), b"second");
    }

    #[test]
    fn archive_actions_cannot_replace_a_running_mutation_channel() {
        let fixture = Fixture::zip(None);
        let mut c = fixture.controller();
        let (tx, rx) = channel();
        c.state.channel = Some(rx);
        c.state.busy = true;
        c.state.archive_password = Some("keep".into());
        c.state.undo = Some((fixture.0.clone(), "undo"));
        c.open(PathBuf::from("another.zip"));
        c.open_file(0);
        c.copy_to_clipboard(false);
        c.start_extract_to(false, std::env::temp_dir());
        c.run_job(Job::Test {
            archive: fixture.0.clone(),
            only: None,
            password: None,
        });
        c.undo_last();
        assert!(c.state.undo.is_some());
        assert_eq!(c.state.archive.as_ref(), Some(&fixture.0));
        assert_eq!(c.state.archive_password.as_deref(), Some("keep"));
        tx.send(Message::Progress(1, 2, "mutation".into())).unwrap();
        c.go_to(String::new());
        c.request_preview(0);
        settle(&mut c);
        assert!(c.state.busy);
        c.receive();
        assert_eq!(c.state.current_file, "mutation");
    }
}
