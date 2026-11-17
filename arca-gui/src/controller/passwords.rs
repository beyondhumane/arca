//! Trying saved passwords before the password box asks.

use super::*;
use crate::vault::{Keyring, Saved, VaultError};
use zeroize::Zeroizing;

#[derive(Default)]
pub(crate) struct PasswordState {
    pub(crate) keyring: Keyring,
    pub(crate) save_typed: bool,
    trying: Option<Attempts>,
    typed: Option<Zeroizing<String>>,
}

impl PasswordState {
    #[cfg(test)]
    pub(super) fn trying(&self) -> bool {
        self.trying.is_some()
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
        if self.state.channel.is_some() || self.state.busy {
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
                    .matching(&archives, true)
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
    // from ever asking.
    fn weak_check(&self, archives: &[PathBuf]) -> bool {
        self.state
            .archive
            .as_ref()
            .is_some_and(|open| archives.contains(open))
            && self.state.entries.iter().any(|entry| entry.zipcrypto)
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
        let result = keyring.add(Saved {
            label: name.clone(),
            mask: name,
            password,
            auto: true,
            session,
        });
        let s = self.s();
        match result {
            Ok(()) if session => self.set_notice(s.saved_for_session, false),
            Ok(()) => self.set_notice(s.saved_password, false),
            Err(error) => self.set_notice(vault_error(s, &error), true),
        }
    }

    pub(super) fn forget_attempts(&mut self) {
        let saved = &mut self.state.saved;
        saved.trying = None;
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
        let mut picks = keyring.matching(&archives, false);
        if self.weak_check(&archives) {
            picks = keyring
                .matching(&archives, true)
                .into_iter()
                .chain(picks)
                .collect();
        }
        picks
            .into_iter()
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

    /// Opens the vault, and tries what it holds on the archive the password
    /// box is waiting on, if it is up.
    pub(crate) fn unlock_vault(&mut self, master: &str) -> Result<(), String> {
        let s = self.s();
        self.state
            .saved
            .keyring
            .unlock(master)
            .map_err(|error| vault_error(s, &error))?;
        self.state.saved.trying = None;
        self.try_saved_passwords();
        Ok(())
    }

    pub(crate) fn create_vault(&mut self, master: &str) -> Result<(), String> {
        let s = self.s();
        self.state
            .saved
            .keyring
            .create(master)
            .map_err(|error| vault_error(s, &error))
    }

    pub(crate) fn vault_do(
        &mut self,
        change: impl FnOnce(&mut Keyring) -> Result<(), VaultError>,
    ) -> Result<(), String> {
        let s = self.s();
        let result = change(&mut self.state.saved.keyring).map_err(|error| vault_error(s, &error));
        self.state.saved.trying = None;
        result
    }

    /// Locks the vault once it has gone unused long enough; otherwise, how
    /// long it has left, so the window can wake up in time to lock it.
    pub(crate) fn vault_tick(&mut self) -> Option<std::time::Duration> {
        let was_open = self.state.saved.keyring.unlocked();
        let left = self.state.saved.keyring.tick(Instant::now());
        if was_open && !self.state.saved.keyring.unlocked() {
            self.state.saved.trying = None;
        }
        left
    }

    pub(crate) fn lock_vault(&mut self) {
        self.state.saved.keyring.lock();
        self.state.saved.trying = None;
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
    }
}
