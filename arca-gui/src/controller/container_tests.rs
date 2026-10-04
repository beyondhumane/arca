use super::*;
use crate::i18n::Lang;
use std::time::Duration;

fn settle(controller: &mut AppController) {
    let limit = Instant::now() + Duration::from_secs(10);
    while controller.state.busy {
        controller.receive();
        assert!(Instant::now() < limit, "listing worker did not complete");
        std::thread::sleep(Duration::from_millis(1));
    }
}

fn container(dir: &Path, name: &str) -> PathBuf {
    fs::create_dir_all(dir).unwrap();
    let input = dir.join("mimetype");
    fs::write(&input, b"application/epub+zip").unwrap();
    let archive = dir.join(name);
    let files = [arca_zip::Source {
        path: input,
        name: "mimetype".into(),
        size: 20,
        mtime: 0,
        codec: Codec::Store,
        level: Level::Store,
    }];
    arca_zip::create_zip(&archive, &files, 1, None, &|_, _, _| true).unwrap();
    archive
}

#[test]
fn zip_containers_open_with_their_own_label() {
    let dir = std::env::temp_dir().join(format!("arca-container-open-{}", std::process::id()));
    for (name, format, label) in [
        ("app.APK", Format::Apk, "APK"),
        ("book.epub", Format::Epub, "EPUB"),
        ("pkg-1.0-py3-none-any.whl", Format::Whl, "WHL"),
    ] {
        let archive = container(&dir, name);
        let mut controller = AppController::new(Settings::default());
        controller.load_listing(archive.clone(), None);
        settle(&mut controller);
        assert!(!controller.state.error, "{}", controller.state.notice);
        assert_eq!(controller.state.format, format);
        assert_eq!(controller.state.entries.len(), 1);
        assert!(
            controller
                .state
                .window_title
                .contains(&format!("({label}: read-only)")),
            "{}",
            controller.state.window_title
        );
        assert!(controller.state.waiting_on_password.is_none());
    }
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn zip_container_mutation_jobs_are_rejected_before_touching_the_archive() {
    let dir = std::env::temp_dir().join(format!("arca-container-write-{}", std::process::id()));
    let archive = container(&dir, "app.jar");
    let before = fs::read(&archive).unwrap();
    let jobs = vec![
        Job::Password {
            archive: archive.clone(),
            current: None,
            new: Some("no".into()),
        },
        Job::Delete {
            archive: archive.clone(),
            names: vec!["mimetype".into()],
            password: None,
        },
        Job::Rename {
            archive: archive.clone(),
            from: "mimetype".into(),
            to: "new".into(),
            folder: false,
            password: None,
        },
        Job::Move {
            archive: archive.clone(),
            moves: vec![("mimetype".into(), "folder/mimetype".into())],
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
    ];
    for job in jobs {
        let result = run_job_blocking(job, strings(Lang::En), &|_, _, _| true, &|_| Answer::Cancel);
        let error = result.unwrap_err();
        assert!(
            error.contains("JAR") && error.contains("read-only"),
            "{error}"
        );
        assert_eq!(fs::read(&archive).unwrap(), before);
    }
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn zip_container_edit_requests_name_the_real_format() {
    let dir = std::env::temp_dir().join(format!("arca-container-notice-{}", std::process::id()));
    let archive = container(&dir, "comic.cbz");
    let mut controller = AppController::new(Settings::default());
    controller.load_listing(archive.clone(), None);
    settle(&mut controller);
    controller.request_delete();
    assert!(controller.state.error);
    assert!(
        controller.state.notice.contains("CBZ") && controller.state.notice.contains("read-only"),
        "{}",
        controller.state.notice
    );
    controller.state.error = false;
    controller.begin_password_change();
    assert!(
        controller.state.notice.contains("CBZ"),
        "{}",
        controller.state.notice
    );
    let _ = fs::remove_dir_all(dir);
}
