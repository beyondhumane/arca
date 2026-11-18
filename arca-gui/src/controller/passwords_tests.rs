use super::*;
use crate::test_support::Room;
use crate::vault::{Keyring, Saved};
use zeroize::Zeroizing;

fn settle(controller: &mut AppController) {
    let end = Instant::now() + std::time::Duration::from_secs(60);
    while controller.state.busy
        || controller.working()
        || controller.vault_busy()
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
    app.unlock_vault(Zeroizing::new("wrong".into()), true);
    assert!(matches!(app.vault_settle(), Some((_, Err(_)))));
    assert!(app.state.waiting_on_password.is_some());
    app.unlock_vault(Zeroizing::new("master-of-all".into()), true);
    assert!(app.vault_busy());
    assert!(!app.state.saved.keyring.unlocked());
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
    app.create_vault(Zeroizing::new("master".into()));
    assert!(matches!(app.vault_settle(), Some((_, Ok(())))));
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

fn crc_byte(crc: u32, byte: u8) -> u32 {
    let mut crc = crc ^ byte as u32;
    for _ in 0..8 {
        crc = if crc & 1 != 0 {
            0xEDB8_8320 ^ (crc >> 1)
        } else {
            crc >> 1
        };
    }
    crc
}

struct Keys([u32; 3]);

impl Keys {
    fn new(password: &[u8]) -> Self {
        let mut keys = Keys([0x1234_5678, 0x2345_6789, 0x3456_7890]);
        for byte in password {
            keys.update(*byte);
        }
        keys
    }

    fn update(&mut self, byte: u8) {
        self.0[0] = crc_byte(self.0[0], byte);
        self.0[1] = self.0[1]
            .wrapping_add(self.0[0] & 0xFF)
            .wrapping_mul(134_775_813)
            .wrapping_add(1);
        self.0[2] = crc_byte(self.0[2], (self.0[1] >> 24) as u8);
    }

    fn encrypt(&mut self, buf: &mut [u8]) {
        for byte in buf.iter_mut() {
            let t = ((self.0[2] | 2) & 0xFFFF) as u16;
            let stream = (t.wrapping_mul(t ^ 1) >> 8) as u8;
            self.update(*byte);
            *byte ^= stream;
        }
    }
}

fn zipcrypto_zip(path: &Path, password: &str) {
    let name = b"inside.txt";
    let plain = b"contents of the file".to_vec();
    let crc = !plain.iter().fold(!0u32, |c, b| crc_byte(c, *b));
    let mut body = vec![0u8; 12];
    body[11] = (crc >> 24) as u8;
    body.extend_from_slice(&plain);
    Keys::new(password.as_bytes()).encrypt(&mut body);
    let mut out = Vec::new();
    let u16le = |o: &mut Vec<u8>, v: u16| o.extend_from_slice(&v.to_le_bytes());
    let u32le = |o: &mut Vec<u8>, v: u32| o.extend_from_slice(&v.to_le_bytes());
    u32le(&mut out, 0x0403_4b50);
    for v in [20, 1, 0, 0, 0x21] {
        u16le(&mut out, v);
    }
    u32le(&mut out, crc);
    u32le(&mut out, body.len() as u32);
    u32le(&mut out, plain.len() as u32);
    u16le(&mut out, name.len() as u16);
    u16le(&mut out, 0);
    out.extend_from_slice(name);
    out.extend_from_slice(&body);
    let directory = out.len() as u32;
    u32le(&mut out, 0x0201_4b50);
    for v in [20, 20, 1, 0, 0, 0x21] {
        u16le(&mut out, v);
    }
    u32le(&mut out, crc);
    u32le(&mut out, body.len() as u32);
    u32le(&mut out, plain.len() as u32);
    for v in [name.len() as u16, 0, 0, 0, 0] {
        u16le(&mut out, v);
    }
    u32le(&mut out, 0);
    u32le(&mut out, 0);
    out.extend_from_slice(name);
    let size = out.len() as u32 - directory;
    u32le(&mut out, 0x0605_4b50);
    for v in [0, 0, 1, 1] {
        u16le(&mut out, v);
    }
    u32le(&mut out, size);
    u32le(&mut out, directory);
    u16le(&mut out, 0);
    fs::write(path, out).unwrap();
}

fn one_byte_impostor(archive: &Path) -> String {
    let entries = list_entries(archive, None, &|_, _, _| true).unwrap().0;
    assert!(entries[0].zipcrypto);
    (0..100_000)
        .map(|i| format!("wrong{i}"))
        .find(|candidate| {
            let mut source = fs::File::open(archive).unwrap();
            arca_zip::check_password(&mut source, &entries, candidate).is_ok()
        })
        .unwrap()
}

#[test]
fn a_zipcrypto_archive_not_open_gets_saved_passwords_offered_and_not_tried() {
    let room = Room::new();
    let archive = room.path("old.zip");
    zipcrypto_zip(&archive, "secret");
    let wrong = one_byte_impostor(&archive);
    let mut app = with(vec![
        saved("old.zip", &wrong, true),
        saved("*.zip", "secret", true),
    ]);
    app.run_job(Job::Test {
        archive: archive.clone(),
        password: None,
        only: None,
    });
    settle(&mut app);
    assert!(app.state.waiting_on_password.is_some());
    assert!(app.state.tasks.is_empty());
    assert_eq!(app.offered_passwords().len(), 2);
}

#[test]
fn a_typed_password_that_only_passes_the_zipcrypto_byte_is_never_saved() {
    let room = Room::new();
    let archive = room.path("old.zip");
    zipcrypto_zip(&archive, "secret");
    let wrong = one_byte_impostor(&archive);
    let mut app = with(Vec::new());
    app.run_job(Job::Test {
        archive: archive.clone(),
        password: None,
        only: None,
    });
    settle(&mut app);
    assert!(app.state.waiting_on_password.is_some());
    app.state.saved.save_typed = true;
    app.dispatch(AppAction::SubmitPassword(wrong));
    settle(&mut app);
    assert!(app.state.waiting_on_password.is_some());
    assert!(app.state.password_wrong);
    assert!(app.state.saved.keyring.entries.is_empty());
    app.state.saved.save_typed = true;
    app.dispatch(AppAction::SubmitPassword("secret".into()));
    settle(&mut app);
    assert!(app.state.waiting_on_password.is_none());
    let entries = &app.state.saved.keyring.entries;
    assert_eq!(entries.len(), 1);
    assert_eq!(*entries[0].password, "secret");
    assert!(app
        .state
        .tasks
        .iter()
        .all(|task| task.status == TaskStatus::Done));
}

#[test]
fn proving_a_password_on_a_broken_zipcrypto_archive_fails_without_panicking() {
    let room = Room::new();
    let archive = room.path("old.zip");
    zipcrypto_zip(&archive, "secret");
    assert!(weak_password_check(&archive));
    assert!(prove_password(&archive, "secret").is_ok());
    let whole = fs::read(&archive).unwrap();
    let broken = room.path("broken.zip");
    for cut in 0..whole.len() {
        fs::write(&broken, &whole[..cut]).unwrap();
        let _ = prove_password(&broken, "secret");
        let _ = weak_password_check(&broken);
    }
    let mut flipped = whole.clone();
    flipped[30 + 10 + 14] ^= 0x55;
    fs::write(&broken, &flipped).unwrap();
    assert!(matches!(
        prove_password(&broken, "secret"),
        Err(arca_core::Error::PasswordOrCorrupt)
    ));
    assert!(!weak_password_check(&zip(&room, "secret")));
}

#[test]
fn vault_changes_run_off_the_window_thread_one_after_another() {
    let room = Room::new();
    let mut app = AppController::new(Settings::default());
    app.state.saved.keyring = Keyring::cheap(room.path("passwords.vault"));
    app.create_vault(Zeroizing::new("master".into()));
    assert!(app.vault_busy());
    assert!(!app.state.saved.keyring.unlocked());
    for mask in ["*.zip", "*.7z"] {
        let mut kept = saved(mask, "secret", true);
        kept.session = false;
        app.vault_do(move |keyring| keyring.add(kept));
    }
    settle(&mut app);
    assert!(app.state.saved.keyring.unlocked());
    let masks: Vec<&str> = app
        .state
        .saved
        .keyring
        .entries
        .iter()
        .map(|e| e.mask.as_str())
        .collect();
    assert_eq!(masks, ["*.zip", "*.7z"]);
    app.lock_vault();
    app.unlock_vault(Zeroizing::new("master".into()), false);
    assert!(matches!(app.vault_settle(), Some((_, Ok(())))));
    assert_eq!(app.state.saved.keyring.entries.len(), 2);
}

#[test]
fn changes_still_on_their_way_reach_the_file_before_the_window_closes() {
    let room = Room::new();
    let file = room.path("passwords.vault");
    let mut app = AppController::new(Settings::default());
    app.state.saved.keyring = Keyring::cheap(file.clone());
    app.create_vault(Zeroizing::new("master".into()));
    for mask in ["*.zip", "*.7z"] {
        let mut kept = saved(mask, "secret", true);
        kept.session = false;
        app.vault_do(move |keyring| keyring.add(kept));
    }
    app.finish_vault();
    assert!(!app.vault_busy());
    let mut again = Keyring::cheap(file);
    again.unlock("master").unwrap();
    assert_eq!(again.entries.len(), 2);
}
