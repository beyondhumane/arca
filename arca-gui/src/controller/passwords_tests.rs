use super::*;
use crate::test_support::Room;
use crate::vault::{Keyring, Saved};
use zeroize::Zeroizing;

fn settle(controller: &mut AppController) {
    let end = Instant::now() + std::time::Duration::from_secs(60);
    while controller.state.busy
        || controller.working()
        || controller.state.preview.status == PreviewStatus::Loading
    {
        assert!(!controller.receive());
        controller.poll_preview();
        assert!(Instant::now() < end, "worker did not complete");
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
}

fn saved(mask: &str, password: &str, auto: bool) -> Saved {
    Saved {
        label: mask.into(),
        mask: mask.into(),
        password: Zeroizing::new(password.into()),
        auto,
        session: true,
    }
}

fn with(entries: Vec<Saved>) -> AppController {
    let mut app = AppController::new(Settings::default());
    for entry in entries {
        app.state.saved.keyring.add(entry).unwrap();
    }
    app
}

fn zip(room: &Room, password: &str) -> PathBuf {
    let path = room.path("locked.zip");
    let mut zip = arca_zip::ZipWriter::new(fs::File::create(&path).unwrap());
    zip.add_with_password(
        "inside.txt",
        &b"contents"[..],
        Codec::Store,
        Level::STORE,
        None,
        Some(password),
    )
    .unwrap();
    zip.finish().unwrap();
    path
}

fn opens_with_saved(archive: PathBuf) {
    let name = archive.file_name().unwrap().to_string_lossy().to_string();
    let mut app = with(vec![
        saved("", "unmasked-wrong", true),
        saved(&name, "exact-wrong", true),
        saved("*.zip;*.7z", "secret", true),
    ]);
    app.open(archive);
    settle(&mut app);
    assert!(app.state.waiting_on_password.is_none());
    assert!(!app.state.entries.is_empty());
    assert_eq!(app.state.archive_password.as_deref(), Some("secret"));
    assert!(!app.state.password_wrong);
}

#[test]
fn saved_passwords_open_a_7z_with_hidden_names_after_wrong_ones() {
    let room = Room::new();
    opens_with_saved(room.archive(Some("secret"), true));
}

#[test]
fn saved_passwords_open_a_7z_with_clear_names() {
    let room = Room::new();
    opens_with_saved(room.archive(Some("secret"), false));
}

#[test]
fn saved_passwords_open_an_encrypted_zip() {
    let room = Room::new();
    opens_with_saved(zip(&room, "secret"));
}

#[test]
fn when_no_saved_password_fits_the_box_asks_and_can_save_the_answer() {
    let room = Room::new();
    let archive = room.archive(Some("secret"), true);
    let mut app = with(vec![saved("*.7z", "nope", true)]);
    app.open(archive.clone());
    settle(&mut app);
    assert!(matches!(
        app.state.waiting_on_password,
        Some(Pending::OpenArchive)
    ));
    assert!(!app.state.password_wrong, "nobody typed anything");
    assert_eq!(app.state.notice, app.s().saved_none_fit);
    app.dispatch(AppAction::SubmitPassword("typo".into()));
    settle(&mut app);
    assert!(app.state.password_wrong);
    assert!(app.state.waiting_on_password.is_some());
    app.state.saved.save_typed = true;
    app.dispatch(AppAction::SubmitPassword("secret".into()));
    settle(&mut app);
    assert!(app.state.waiting_on_password.is_none());
    assert_eq!(app.state.notice, app.s().saved_for_session);
    let name = archive.file_name().unwrap().to_string_lossy().to_string();
    let kept = app.state.saved.keyring.entries.last().unwrap();
    assert_eq!(kept.mask, name);
    assert_eq!(*kept.password, "secret");
    assert!(kept.auto && kept.session);

    app.close_archive();
    app.open(archive);
    settle(&mut app);
    assert_eq!(app.state.archive_password.as_deref(), Some("secret"));
}

#[test]
fn a_wrong_typed_password_is_never_saved() {
    let room = Room::new();
    let archive = zip(&room, "secret");
    let mut app = with(Vec::new());
    app.open(archive);
    settle(&mut app);
    app.state.saved.save_typed = true;
    app.dispatch(AppAction::SubmitPassword("typo".into()));
    settle(&mut app);
    assert!(app.state.saved.keyring.entries.is_empty());
    app.dispatch(AppAction::SubmitPassword("secret".into()));
    settle(&mut app);
    let entries = &app.state.saved.keyring.entries;
    assert_eq!(entries.len(), 1);
    assert_eq!(*entries[0].password, "secret");
    assert!(!app.state.saved.save_typed);
}

#[test]
fn passwords_that_ask_first_are_offered_and_not_tried() {
    let room = Room::new();
    let archive = zip(&room, "secret");
    let mut app = with(vec![saved("*.zip", "secret", false)]);
    app.open(archive);
    settle(&mut app);
    assert!(app.state.waiting_on_password.is_some());
    let offered = app.offered_passwords();
    assert_eq!(offered.len(), 1);
    app.use_offered(offered[0].0);
    settle(&mut app);
    assert!(app.state.waiting_on_password.is_none());
    assert_eq!(app.state.archive_password.as_deref(), Some("secret"));
}

#[test]
fn a_job_that_asks_before_it_runs_takes_a_saved_password() {
    let room = Room::new();
    let archive = room.archive(Some("secret"), false);
    let mut app = with(Vec::new());
    app.open(archive);
    settle(&mut app);
    app.cancel_password();
    assert!(app.state.archive_password.is_none());
    app.state
        .saved
        .keyring
        .add(saved("*.7z", "secret", true))
        .unwrap();
    app.dispatch(AppAction::Checksums);
    settle(&mut app);
    assert!(app.state.waiting_on_password.is_none());
    assert_eq!(app.state.archive_password.as_deref(), Some("secret"));
}

#[test]
fn testing_an_archive_not_open_in_the_window_takes_a_saved_password() {
    let room = Room::new();
    let archive = room.archive(Some("secret"), true);
    let mut app = with(vec![saved("*.7z", "secret", true)]);
    app.run_job(Job::Test {
        archive,
        password: None,
        only: None,
    });
    settle(&mut app);
    assert!(app.state.waiting_on_password.is_none());
    assert!(app
        .state
        .tasks
        .iter()
        .all(|task| task.status == TaskStatus::Done));
}

#[test]
fn unlocking_from_the_box_tries_the_vault_and_secrets_stay_out_of_the_settings() {
    let room = Room::new();
    let file = room.path("passwords.vault");
    let mut keyring = Keyring::cheap(file.clone());
    keyring.create("master-of-all").unwrap();
    let mut kept = saved("*.zip", "secret", true);
    kept.session = false;
    keyring.add(kept).unwrap();
    drop(keyring);

    let archive = zip(&room, "secret");
    let mut app = AppController::new(Settings::default());
    app.state.saved.keyring = Keyring::cheap(file.clone());
    app.open(archive);
    settle(&mut app);
    assert!(app.state.waiting_on_password.is_some());
    assert!(app.unlock_vault("wrong").is_err());
    assert!(app.state.waiting_on_password.is_some());
    app.unlock_vault("master-of-all").unwrap();
    settle(&mut app);
    assert!(app.state.waiting_on_password.is_none());
    assert_eq!(app.state.archive_password.as_deref(), Some("secret"));

    let settings = app.state.settings.text();
    let vault = fs::read(&file).unwrap();
    for secret in ["secret", "master-of-all"] {
        assert!(!settings.contains(secret), "{secret} in the settings");
        assert!(
            !vault.windows(secret.len()).any(|w| w == secret.as_bytes()),
            "{secret} in the vault in the clear"
        );
    }
}

#[test]
fn reopening_an_archive_tries_its_saved_passwords_afresh() {
    let room = Room::new();
    let archive = room.archive(Some("secret"), true);
    let mut app = with(vec![saved("*.7z", "nope", true)]);
    app.open(archive.clone());
    settle(&mut app);
    assert!(app.state.waiting_on_password.is_some());
    app.state
        .saved
        .keyring
        .add(saved("*.7z", "secret", true))
        .unwrap();
    app.open(archive);
    settle(&mut app);
    assert!(app.state.waiting_on_password.is_none());
    assert_eq!(app.state.archive_password.as_deref(), Some("secret"));
}

#[test]
fn zipcrypto_archives_get_saved_passwords_offered_and_not_tried() {
    let room = Room::new();
    let archive = zip(&room, "secret");
    let mut app = with(vec![saved("*.zip", "secret", true)]);
    app.state.archive = Some(archive.clone());
    app.state.entries = list_entries(&archive, None, &|_, _, _| true).unwrap().0;
    for entry in &mut app.state.entries {
        entry.zipcrypto = true;
    }
    app.state.waiting_on_password = Some(Pending::OpenArchive);
    app.try_saved_passwords();
    assert!(app.state.waiting_on_password.is_some());
    assert!(!app.state.busy);
    assert_eq!(app.offered_passwords().len(), 1);
}

#[test]
fn locking_forgets_the_passwords_waiting_to_be_tried() {
    let room = Room::new();
    let file = room.path("passwords.vault");
    let mut app = AppController::new(Settings::default());
    app.state.saved.keyring = Keyring::cheap(file);
    app.create_vault("master").unwrap();
    let mut kept = saved("*.zip", "wrong", true);
    kept.session = false;
    app.state.saved.keyring.add(kept).unwrap();
    app.open(zip(&room, "secret"));
    settle(&mut app);
    assert!(app.state.saved.trying());
    app.lock_vault();
    assert!(!app.state.saved.trying());
    assert!(app.state.saved.keyring.entries.is_empty());
}

#[test]
fn a_job_on_several_archives_tries_only_passwords_that_fit_them_all() {
    let (one, two) = (Room::new(), Room::new());
    let first = one.archive(Some("secret"), true);
    let second = two.archive(Some("secret"), true);
    let name = first.file_name().unwrap().to_string_lossy().to_string();
    let mut app = with(vec![saved(&name, "secret", true)]);
    let job = || Job::Extract {
        archives: vec![first.clone(), second.clone()],
        dest: Destination::Subfolder,
        password: None,
    };
    app.run_job(job());
    settle(&mut app);
    assert!(app.state.waiting_on_password.is_some());
    assert!(app.state.tasks.is_empty());
    let offered = app.offered_passwords();
    assert_eq!(offered.len(), 1);
    assert_eq!(offered[0].1, name);

    app.cancel_password();
    app.state
        .saved
        .keyring
        .add(saved("*.7z", "secret", true))
        .unwrap();
    app.run_job(job());
    settle(&mut app);
    assert!(app.state.waiting_on_password.is_none());
    assert!(!app.state.tasks.is_empty());
    assert!(app
        .state
        .tasks
        .iter()
        .all(|task| task.status == TaskStatus::Done));
}
