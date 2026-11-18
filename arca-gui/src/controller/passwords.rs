//! Trying saved passwords before the password box asks.

use super::*;
use crate::vault::{Keyring, Saved, VaultError};
use std::collections::VecDeque;
use std::sync::mpsc::{channel, Receiver, TryRecvError};
use zeroize::Zeroizing;

#[derive(Default)]
pub(crate) struct PasswordState {
    pub(crate) keyring: Keyring,
    pub(crate) save_typed: bool,
    trying: Option<Attempts>,
    typed: Option<Zeroizing<String>>,
    weak: Option<(Vec<PathBuf>, bool)>,
    vault_job: Option<VaultJob>,
    vault_queue: VecDeque<(VaultTask, VaultChange)>,
    vault_said: Option<(VaultTask, Result<(), String>)>,
    lock_when_back: bool,
}

/// What a change to the vault was for, so its outcome reaches the place
/// that asked for it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum VaultTask {
    Unlock { from_box: bool },
    Create,
    Change,
    KeepTyped { session: bool },
}

type VaultChange = Box<dyn FnOnce(&mut Keyring) -> Result<(), VaultError> + Send>;

// Stretching the master password costs Argon2id's 64 MiB and three passes,
// and every change rewrites and syncs the file, so changes run on a thread
// of their own, one at a time and in order, on a copy of the vault that
// takes the place of the one here when it comes back.
struct VaultJob {
    task: VaultTask,
    back: Receiver<(Keyring, Result<(), VaultError>)>,
}

impl PasswordState {
    #[cfg(test)]
    pub(super) fn trying(&self) -> bool {
        self.trying.is_some()
    }

    /// Whether the password about to be sent is one typed to be saved, which
    /// has to be proved beyond ZipCrypto's one check byte before it is kept.
    pub(super) fn proving(&self) -> bool {
        self.typed.is_some()
    }

    pub(super) fn note_weak(&mut self, archives: Vec<PathBuf>, weak: bool) {
        self.weak = Some((archives, weak));
    }
}

struct Attempts {
    archives: Vec<PathBuf>,
    left: Vec<Zeroizing<String>>,
    in_flight: Option<Zeroizing<String>>,
}

impl AppController {
    fn asked_for(&self, pending: &Pending) -> Vec<PathBuf> {
        match pending {
            Pending::Extract(job) => match &**job {
                Job::Extract { archives, .. } => archives.clone(),
                Job::Test { archive, .. } => vec![archive.clone()],
                _ => Vec::new(),
            },
            Pending::OpenArchive | Pending::Read(_) => self.state.archive.iter().cloned().collect(),
            Pending::CurrentPassword(_) | Pending::NewPassword(_) => Vec::new(),
        }
    }

    /// The archives the open password box is asking about, when it is asking
    /// for a password they already have.
    pub(crate) fn password_subjects(&self) -> Vec<PathBuf> {
        self.state
            .waiting_on_password
            .as_ref()
            .map(|pending| self.asked_for(pending))
            .unwrap_or_default()
    }

    /// Takes the next saved password that fits and submits it in place of an
    /// answer, while the password box is up and nothing else is running.
    pub(super) fn try_saved_passwords(&mut self) {
        self.vault_tick();
        if self.state.channel.is_some() || self.state.busy || self.vault_busy() {
            return;
        }
        let Some(pending) = &self.state.waiting_on_password else {
            return;
        };
        let archives = self.asked_for(pending);
        if archives.is_empty() {
            return;
        }
        let weak = self.weak_check(&archives);
        let saved = &mut self.state.saved;
        if saved
            .trying
            .as_ref()
            .is_none_or(|attempts| attempts.archives != archives)
        {
            let keyring = &saved.keyring;
            let mut left: Vec<Zeroizing<String>> = if weak {
                Vec::new()
            } else {
                keyring
                    .matching_all(&archives, true)
                    .into_iter()
                    .map(|i| keyring.entries[i].password.clone())
                    .collect()
            };
            left.reverse();
            saved.trying = Some(Attempts {
                archives,
                left,
                in_flight: None,
            });
        }
        let Some(attempts) = saved.trying.as_mut() else {
            return;
        };
        let Some(next) = attempts.left.pop() else {
            return;
        };
        let password = next.to_string();
        attempts.in_flight = Some(next);
        saved.keyring.touch();
        self.submit_password(password);
    }

    // A ZipCrypto check is one byte, so one wrong password in 256 passes it,
    // and the same one passes every time. Such an archive gets its saved
    // passwords offered, never tried, or a false match would stop the box
    // from ever asking. The open archive says so through its entries; any
    // other, through the worker that found it needs a password.
    fn weak_check(&self, archives: &[PathBuf]) -> bool {
        let open = self
            .state
            .archive
            .as_ref()
            .is_some_and(|open| archives.contains(open))
            && self.state.entries.iter().any(|entry| entry.zipcrypto);
        open || self
            .state
            .saved
            .weak
            .as_ref()
            .is_some_and(|(asked, weak)| *weak && asked == archives)
    }

    /// Whether the password that just failed was the saved one sent for it,
    /// rather than one typed in the box.
    pub(super) fn saved_attempt_failed(&mut self, password: Option<&str>) -> bool {
        let sent = self
            .state
            .saved
            .trying
            .as_mut()
            .and_then(|attempts| attempts.in_flight.take());
        sent.is_some_and(|sent| Some(sent.as_str()) == password)
    }

    pub(super) fn password_accepted(&mut self, pending: &Pending, password: &str) {
        let archive = self.asked_for(pending).into_iter().next();
        let saved = &mut self.state.saved;
        saved.trying = None;
        saved.weak = None;
        saved.save_typed = false;
        saved.keyring.touch();
        let Some(typed) = saved.typed.take() else {
            return;
        };
        if let Some(archive) = archive.filter(|_| *typed == password) {
            self.keep_typed(&archive, typed);
        }
    }

    fn keep_typed(&mut self, archive: &Path, password: Zeroizing<String>) {
        let name = archive
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        let keyring = &mut self.state.saved.keyring;
        if keyring
            .entries
            .iter()
            .any(|e| e.password == password && e.mask.eq_ignore_ascii_case(&name))
        {
            return;
        }
        let session = !keyring.unlocked();
        let entry = Saved {
            label: name.clone(),
            mask: name,
            password,
            auto: true,
            session,
        };
        self.vault_run(VaultTask::KeepTyped { session }, move |keyring| {
            keyring.add(entry)
        });
    }

    pub(super) fn forget_attempts(&mut self) {
        let saved = &mut self.state.saved;
        saved.trying = None;
        saved.weak = None;
        saved.typed = None;
        saved.save_typed = false;
    }

    /// A password typed in the box, kept aside to be saved if it opens what
    /// the box is asking about.
    pub(super) fn submit_typed(&mut self, password: String) {
        let opening = self
            .state
            .waiting_on_password
            .as_ref()
            .is_some_and(|pending| !self.asked_for(pending).is_empty());
        let saved = &mut self.state.saved;
        saved.typed = (opening && saved.save_typed && !password.is_empty())
            .then(|| Zeroizing::new(password.clone()));
        if let Some(attempts) = saved.trying.as_mut() {
            attempts.in_flight = None;
        }
        self.submit_password(password);
    }

    /// The saved passwords that fit the archive being asked about but wait
    /// to be picked, as their index and label.
    pub(crate) fn offered_passwords(&self) -> Vec<(usize, String)> {
        let archives = self.password_subjects();
        if archives.is_empty() {
            return Vec::new();
        }
        let keyring = &self.state.saved.keyring;
        let tried = if self.weak_check(&archives) {
            Vec::new()
        } else {
            keyring.matching_all(&archives, true)
        };
        keyring
            .matching(&archives, true)
            .into_iter()
            .filter(|i| !tried.contains(i))
            .chain(keyring.matching(&archives, false))
            .map(|i| {
                let entry = &keyring.entries[i];
                let label = if entry.label.is_empty() {
                    entry.mask.clone()
                } else {
                    entry.label.clone()
                };
                (i, label)
            })
            .collect()
    }

    pub(crate) fn use_offered(&mut self, index: usize) {
        let Some(password) = self
            .state
            .saved
            .keyring
            .entries
            .get(index)
            .map(|e| e.password.to_string())
        else {
            return;
        };
        self.state.saved.typed = None;
        self.state.saved.keyring.touch();
        if let Some(attempts) = self.state.saved.trying.as_mut() {
            attempts.in_flight = None;
        }
        self.submit_password(password);
    }

    /// Opens the vault, and then tries what it holds on the archive the
    /// password box is waiting on, if it is up.
    pub(crate) fn unlock_vault(&mut self, master: Zeroizing<String>, from_box: bool) {
        self.vault_run(VaultTask::Unlock { from_box }, move |keyring| {
            keyring.unlock(&master)
        });
    }

    pub(crate) fn create_vault(&mut self, master: Zeroizing<String>) {
        self.vault_run(VaultTask::Create, move |keyring| keyring.create(&master));
    }

    pub(crate) fn vault_do(
        &mut self,
        change: impl FnOnce(&mut Keyring) -> Result<(), VaultError> + Send + 'static,
    ) {
        self.vault_run(VaultTask::Change, change);
    }

    pub(crate) fn vault_busy(&self) -> bool {
        self.state.saved.vault_job.is_some()
    }

    /// The outcome of the last change the settings page or the password box
    /// asked for, once it is known.
    pub(crate) fn take_vault_said(&mut self) -> Option<(VaultTask, Result<(), String>)> {
        self.state.saved.vault_said.take()
    }

    fn vault_run(
        &mut self,
        task: VaultTask,
        change: impl FnOnce(&mut Keyring) -> Result<(), VaultError> + Send + 'static,
    ) {
        self.state
            .saved
            .vault_queue
            .push_back((task, Box::new(change)));
        self.vault_next();
    }

    fn vault_next(&mut self) {
        let saved = &mut self.state.saved;
        if saved.vault_job.is_some() {
            return;
        }
        let Some((task, change)) = saved.vault_queue.pop_front() else {
            return;
        };
        let mut keyring = saved.keyring.clone();
        let (tx, back) = channel();
        std::thread::spawn(move || {
            let result = change(&mut keyring);
            let _ = tx.send((keyring, result));
        });
        saved.vault_job = Some(VaultJob { task, back });
    }

    fn vault_poll(&mut self) {
        let Some(job) = &self.state.saved.vault_job else {
            return;
        };
        let task = job.task;
        let result = match job.back.try_recv() {
            Ok((keyring, result)) => {
                self.state.saved.keyring = keyring;
                result
            }
            Err(TryRecvError::Empty) => return,
            Err(TryRecvError::Disconnected) => Err(VaultError::Io(std::io::Error::other(
                "the vault worker stopped",
            ))),
        };
        let saved = &mut self.state.saved;
        saved.vault_job = None;
        saved.trying = None;
        if std::mem::take(&mut saved.lock_when_back) {
            saved.keyring.lock();
        }
        let s = self.s();
        match (task, result) {
            (VaultTask::KeepTyped { session: true }, Ok(())) => {
                self.set_notice(s.saved_for_session, false)
            }
            (VaultTask::KeepTyped { .. }, Ok(())) => self.set_notice(s.saved_password, false),
            (VaultTask::KeepTyped { .. }, Err(error)) => {
                self.set_notice(vault_error(s, &error), true)
            }
            (task, result) => {
                self.state.saved.vault_said =
                    Some((task, result.map_err(|error| vault_error(s, &error))))
            }
        }
        self.vault_next();
    }

    /// Takes back what the vault's thread finished, and locks the vault
    /// once it has gone unused long enough. Gives how long until the window
    /// should look again: soon while a change is under way, otherwise in
    /// time to lock it.
    pub(crate) fn vault_tick(&mut self) -> Option<std::time::Duration> {
        self.vault_poll();
        if self.vault_busy() {
            return Some(std::time::Duration::from_millis(30));
        }
        let was_open = self.state.saved.keyring.unlocked();
        let left = self.state.saved.keyring.tick(Instant::now());
        if was_open && !self.state.saved.keyring.unlocked() {
            self.state.saved.trying = None;
        }
        left
    }

    pub(crate) fn lock_vault(&mut self) {
        let saved = &mut self.state.saved;
        saved.keyring.lock();
        saved.trying = None;
        saved.lock_when_back = saved.vault_job.is_some();
    }

    /// Waits for the vault's thread to get through every change asked of
    /// it, so closing the window loses none that is still on its way to the
    /// file.
    pub(crate) fn finish_vault(&mut self) {
        let end = Instant::now() + std::time::Duration::from_secs(10);
        while self.vault_busy() && Instant::now() < end {
            std::thread::sleep(std::time::Duration::from_millis(2));
            self.vault_poll();
        }
    }

    #[cfg(test)]
    pub(crate) fn vault_settle(&mut self) -> Option<(VaultTask, Result<(), String>)> {
        self.finish_vault();
        assert!(!self.vault_busy(), "the vault did not finish");
        self.take_vault_said()
    }
}

pub(crate) fn vault_error(s: &Strings, error: &VaultError) -> String {
    match error {
        VaultError::Io(error) => fill(s.vault_io_failed, &[("error", &error.to_string())]),
        VaultError::WrongOrDamaged => s.vault_wrong_master.to_string(),
        VaultError::Damaged => s.vault_damaged.to_string(),
        VaultError::Newer => s.vault_newer.to_string(),
        VaultError::Changed => s.vault_changed.to_string(),
        VaultError::Exists => s.vault_exists.to_string(),
        VaultError::Nowhere => s.vault_nowhere.to_string(),
        VaultError::Locked => s.vault_locked.to_string(),
        VaultError::Gone => s.vault_entry_gone.to_string(),
    }
}
