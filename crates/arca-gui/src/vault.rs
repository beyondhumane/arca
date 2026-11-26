//! Saved archive passwords, sealed under a master password.
//!
//! File layout, version 1. Every integer is little-endian.
//!
//! ```text
//! magic    8  "ARCAVLT\0"
//! version  1  1
//! kdf      1  1 = Argon2id v0x13
//! memory   4  KiB
//! passes   4
//! lanes    4
//! salt    16
//! nonce   12
//! sealed   *  AES-256-GCM of the payload, tag last; the 50 bytes above are the AAD
//! ```
//!
//! The payload is a `u32` count and then, per entry, a flags byte (bit 0: try
//! without asking) and three `u32`-length UTF-8 strings: label, mask, password.

use crate::controller::matches_mask;
use aes_gcm::aead::{Aead, KeyInit, Payload};
use aes_gcm::Aes256Gcm;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use zeroize::Zeroizing;

pub(crate) const LOCK_AFTER: Duration = Duration::from_secs(10 * 60);
const LOOK_AGAIN: Duration = Duration::from_secs(2);

const MAGIC: &[u8; 8] = b"ARCAVLT\0";
const VERSION: u8 = 1;
const KDF_ARGON2ID: u8 = 1;
const SALT: usize = 16;
const NONCE: usize = 12;
const HEADER: usize = MAGIC.len() + 2 + 12 + SALT + NONCE;
const TAG: usize = 16;
const MOST_BYTES: u64 = 16 << 20;
const MOST_ENTRIES: usize = 100_000;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) struct Cost {
    pub(crate) memory_kib: u32,
    pub(crate) passes: u32,
    pub(crate) lanes: u32,
}

impl Cost {
    pub(crate) const STANDARD: Cost = Cost {
        memory_kib: 64 * 1024,
        passes: 3,
        lanes: 1,
    };
    #[cfg(test)]
    pub(crate) const CHEAP: Cost = Cost {
        memory_kib: 8,
        passes: 1,
        lanes: 1,
    };

    // A file states its own cost, so a planted one must not be able to ask
    // for more memory or time than unlocking can reasonably spend.
    fn allowed(self) -> bool {
        (1..=16).contains(&self.lanes)
            && (1..=64).contains(&self.passes)
            && self.memory_kib >= 8 * self.lanes
            && self.memory_kib <= 1 << 20
    }
}

#[derive(Clone)]
pub(crate) struct Saved {
    pub(crate) label: String,
    pub(crate) mask: String,
    pub(crate) password: Zeroizing<String>,
    pub(crate) auto: bool,
    pub(crate) session: bool,
}

#[derive(Debug)]
pub(crate) enum VaultError {
    Io(std::io::Error),
    WrongOrDamaged,
    Damaged,
    Newer,
    Changed,
    Exists,
    Nowhere,
    Locked,
    Gone,
}

impl From<std::io::Error> for VaultError {
    fn from(error: std::io::Error) -> Self {
        VaultError::Io(error)
    }
}

#[derive(Clone)]
struct Sealing {
    key: Zeroizing<[u8; 32]>,
    salt: [u8; SALT],
    cost: Cost,
}

fn derive(master: &str, salt: &[u8; SALT], cost: Cost) -> Result<Zeroizing<[u8; 32]>, VaultError> {
    if !cost.allowed() {
        return Err(VaultError::Damaged);
    }
    let params = argon2::Params::new(cost.memory_kib, cost.passes, cost.lanes, Some(32))
        .map_err(|_| VaultError::Damaged)?;
    let kdf = argon2::Argon2::new(argon2::Algorithm::Argon2id, argon2::Version::V0x13, params);
    let mut key = Zeroizing::new([0u8; 32]);
    kdf.hash_password_into(master.as_bytes(), salt, &mut key[..])
        .map_err(|_| VaultError::Damaged)?;
    Ok(key)
}

fn header(cost: Cost, salt: &[u8; SALT], nonce: &[u8; NONCE]) -> Vec<u8> {
    let mut out = Vec::with_capacity(HEADER);
    out.extend_from_slice(MAGIC);
    out.push(VERSION);
    out.push(KDF_ARGON2ID);
    out.extend_from_slice(&cost.memory_kib.to_le_bytes());
    out.extend_from_slice(&cost.passes.to_le_bytes());
    out.extend_from_slice(&cost.lanes.to_le_bytes());
    out.extend_from_slice(salt);
    out.extend_from_slice(nonce);
    out
}

fn put(out: &mut Vec<u8>, text: &str) {
    out.extend_from_slice(&(text.len() as u32).to_le_bytes());
    out.extend_from_slice(text.as_bytes());
}

fn encode<'a>(entries: impl Iterator<Item = &'a Saved>) -> Zeroizing<Vec<u8>> {
    let kept: Vec<&Saved> = entries.collect();
    let room = kept
        .iter()
        .map(|e| 13 + e.label.len() + e.mask.len() + e.password.len())
        .sum::<usize>();
    let mut out = Zeroizing::new(Vec::with_capacity(4 + room));
    out.extend_from_slice(&(kept.len() as u32).to_le_bytes());
    for entry in kept {
        out.push(u8::from(entry.auto));
        put(&mut out, &entry.label);
        put(&mut out, &entry.mask);
        put(&mut out, &entry.password);
    }
    out
}

struct Reader<'a>(&'a [u8]);

impl Reader<'_> {
    fn take(&mut self, n: usize) -> Result<&[u8], VaultError> {
        if self.0.len() < n {
            return Err(VaultError::Damaged);
        }
        let (head, rest) = self.0.split_at(n);
        self.0 = rest;
        Ok(head)
    }

    fn word(&mut self) -> Result<u32, VaultError> {
        let bytes = self.take(4)?;
        Ok(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }

    fn text(&mut self) -> Result<Zeroizing<String>, VaultError> {
        let n = self.word()? as usize;
        let bytes = self.take(n)?;
        std::str::from_utf8(bytes)
            .map(|text| Zeroizing::new(text.to_string()))
            .map_err(|_| VaultError::Damaged)
    }
}

fn decode(payload: &[u8]) -> Result<Vec<Saved>, VaultError> {
    let mut reader = Reader(payload);
    let count = reader.word()? as usize;
    if count > MOST_ENTRIES || count > payload.len() / 13 {
        return Err(VaultError::Damaged);
    }
    let mut entries = Vec::with_capacity(count);
    for _ in 0..count {
        let flags = reader.take(1)?[0];
        if flags > 1 {
            return Err(VaultError::Damaged);
        }
        entries.push(Saved {
            auto: flags == 1,
            label: reader.text()?.to_string(),
            mask: reader.text()?.to_string(),
            password: reader.text()?,
            session: false,
        });
    }
    if !reader.0.is_empty() {
        return Err(VaultError::Damaged);
    }
    Ok(entries)
}

fn seal<'a>(
    sealing: &Sealing,
    entries: impl Iterator<Item = &'a Saved>,
) -> Result<Vec<u8>, VaultError> {
    let mut nonce = [0u8; NONCE];
    getrandom::fill(&mut nonce).map_err(|e| VaultError::Io(std::io::Error::other(e)))?;
    let mut out = header(sealing.cost, &sealing.salt, &nonce);
    let cipher = Aes256Gcm::new_from_slice(&sealing.key[..]).map_err(|_| VaultError::Damaged)?;
    let payload = encode(entries);
    let sealed = cipher
        .encrypt(
            &nonce.into(),
            Payload {
                msg: &payload,
                aad: &out,
            },
        )
        .map_err(|_| VaultError::Damaged)?;
    out.extend_from_slice(&sealed);
    Ok(out)
}

fn open(bytes: &[u8], master: &str) -> Result<(Sealing, Vec<Saved>), VaultError> {
    if bytes.len() < MAGIC.len() + 1 || &bytes[..MAGIC.len()] != MAGIC {
        return Err(VaultError::Damaged);
    }
    let version = bytes[MAGIC.len()];
    if version != VERSION {
        return Err(if version > VERSION {
            VaultError::Newer
        } else {
            VaultError::Damaged
        });
    }
    if bytes.len() < HEADER + TAG || bytes[MAGIC.len() + 1] != KDF_ARGON2ID {
        return Err(VaultError::Damaged);
    }
    let mut reader = Reader(&bytes[MAGIC.len() + 2..HEADER]);
    let cost = Cost {
        memory_kib: reader.word()?,
        passes: reader.word()?,
        lanes: reader.word()?,
    };
    let mut salt = [0u8; SALT];
    salt.copy_from_slice(reader.take(SALT)?);
    let mut nonce = [0u8; NONCE];
    nonce.copy_from_slice(reader.take(NONCE)?);
    let key = derive(master, &salt, cost)?;
    let cipher = Aes256Gcm::new_from_slice(&key[..]).map_err(|_| VaultError::Damaged)?;
    let payload = Zeroizing::new(
        cipher
            .decrypt(
                &nonce.into(),
                Payload {
                    msg: &bytes[HEADER..],
                    aad: &bytes[..HEADER],
                },
            )
            .map_err(|_| VaultError::WrongOrDamaged)?,
    );
    let entries = decode(&payload)?;
    Ok((Sealing { key, salt, cost }, entries))
}

fn fingerprint(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}

fn read_bounded(path: &Path) -> Result<Vec<u8>, VaultError> {
    use std::io::Read;
    let file = std::fs::File::open(path)?;
    let mut bytes = Vec::new();
    file.take(MOST_BYTES + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MOST_BYTES {
        return Err(VaultError::Damaged);
    }
    Ok(bytes)
}

fn write_private(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path)?;
    file.write_all(bytes)?;
    file.sync_all()
}

pub(crate) fn vault_file() -> Option<PathBuf> {
    Some(
        crate::settings::config_file()?
            .parent()?
            .join("passwords.vault"),
    )
}

/// How closely `mask` describes `archive`, or `None` when it does not.
///
/// A mask with a slash in it is matched against the whole path, and a folder
/// takes in everything under it; one without is matched against the name.
/// Several masks can be given at once, separated by `;`. A mask with no
/// wildcard outranks any with one, then the one with more fixed characters
/// wins. An empty mask fits every archive and ranks last.
pub(crate) fn rank(mask: &str, archive: &Path) -> Option<(u8, usize)> {
    let masks: Vec<&str> = mask
        .split(';')
        .map(str::trim)
        .filter(|m| !m.is_empty())
        .collect();
    if masks.is_empty() {
        return Some((0, 0));
    }
    let whole = archive.to_string_lossy().replace('\\', "/");
    let whole = match whole.strip_prefix("//?/") {
        Some(rest) => rest
            .strip_prefix("UNC/")
            .map_or_else(|| rest.to_string(), |share| format!("//{share}")),
        None => whole,
    };
    let name = archive
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    masks
        .iter()
        .filter_map(|mask| {
            let mask = mask.replace('\\', "/");
            let fits = if mask.contains('/') {
                let folder = format!("{}/*", mask.trim_end_matches('/'));
                matches_mask(&mask, &whole) || matches_mask(&folder, &whole)
            } else {
                matches_mask(&mask, &name)
            };
            let wild = mask.contains(['*', '?']);
            let fixed = mask.chars().filter(|c| !matches!(c, '*' | '?')).count();
            fits.then_some((if wild { 1 } else { 2 }, fixed))
        })
        .max()
}

#[derive(Clone, Default)]
pub(crate) struct Keyring {
    file: Option<PathBuf>,
    cost: Option<Cost>,
    sealing: Option<Sealing>,
    seen: Option<[u8; 32]>,
    pub(crate) entries: Vec<Saved>,
    used: Option<Instant>,
    present: std::cell::Cell<Option<(Instant, bool)>>,
}

impl Keyring {
    pub(crate) fn at(file: Option<PathBuf>) -> Self {
        Keyring {
            file,
            ..Default::default()
        }
    }

    #[cfg(test)]
    pub(crate) fn cheap(file: PathBuf) -> Self {
        Keyring {
            file: Some(file),
            cost: Some(Cost::CHEAP),
            ..Default::default()
        }
    }

    pub(crate) fn can_keep(&self) -> bool {
        self.file.is_some()
    }

    /// Whether the file is there. The settings page asks on every frame, so
    /// the disk is looked at again only now and then, or after this vault
    /// wrote or moved the file itself.
    pub(crate) fn has_file(&self) -> bool {
        let Some(file) = self.file.as_deref() else {
            return false;
        };
        let now = Instant::now();
        if let Some((at, present)) = self.present.get() {
            if now.saturating_duration_since(at) < LOOK_AGAIN {
                return present;
            }
        }
        let present = file.exists();
        self.present.set(Some((now, present)));
        present
    }

    fn look_again(&self) {
        self.present.set(None);
    }

    pub(crate) fn unlocked(&self) -> bool {
        self.sealing.is_some()
    }

    pub(crate) fn touch(&mut self) {
        if self.unlocked() {
            self.used = Some(Instant::now());
        }
    }

    /// Locks when the vault has gone unused for `LOCK_AFTER`, and otherwise
    /// says how long it has left.
    pub(crate) fn tick(&mut self, now: Instant) -> Option<Duration> {
        let used = self.used.filter(|_| self.unlocked())?;
        let idle = now.saturating_duration_since(used);
        if idle >= LOCK_AFTER {
            self.lock();
            return None;
        }
        Some(LOCK_AFTER - idle)
    }

    pub(crate) fn lock(&mut self) {
        self.sealing = None;
        self.seen = None;
        self.used = None;
        self.entries.retain(|entry| entry.session);
    }

    pub(crate) fn close(&mut self) {
        self.lock();
        self.entries.clear();
    }

    fn file(&self) -> Result<&Path, VaultError> {
        self.file.as_deref().ok_or(VaultError::Nowhere)
    }

    pub(crate) fn create(&mut self, master: &str) -> Result<(), VaultError> {
        let file = self.file()?.to_path_buf();
        self.look_again();
        if file.exists() {
            return Err(VaultError::Exists);
        }
        if let Some(dir) = file.parent() {
            std::fs::create_dir_all(dir)?;
        }
        self.rekey(master, None)?;
        for entry in &mut self.entries {
            entry.session = false;
        }
        if let Err(error) = self.persist() {
            self.sealing = None;
            for entry in &mut self.entries {
                entry.session = true;
            }
            return Err(error);
        }
        self.touch();
        Ok(())
    }

    fn rekey(&mut self, master: &str, keep: Option<Cost>) -> Result<(), VaultError> {
        let mut salt = [0u8; SALT];
        getrandom::fill(&mut salt).map_err(|e| VaultError::Io(std::io::Error::other(e)))?;
        let cost = keep.or(self.cost).unwrap_or(Cost::STANDARD);
        let key = derive(master, &salt, cost)?;
        self.sealing = Some(Sealing { key, salt, cost });
        Ok(())
    }

    pub(crate) fn unlock(&mut self, master: &str) -> Result<(), VaultError> {
        self.look_again();
        let bytes = read_bounded(self.file()?)?;
        let (sealing, kept) = open(&bytes, master)?;
        self.seen = Some(fingerprint(&bytes));
        self.sealing = Some(sealing);
        self.entries.retain(|entry| entry.session);
        let mut session = std::mem::take(&mut self.entries);
        self.entries = kept;
        self.entries.append(&mut session);
        self.touch();
        Ok(())
    }

    pub(crate) fn change_master(&mut self, master: &str) -> Result<(), VaultError> {
        let Some(old) = self.sealing.take() else {
            return Err(VaultError::Locked);
        };
        if let Err(error) = self
            .rekey(master, Some(old.cost))
            .and_then(|()| self.persist())
        {
            self.sealing = Some(old);
            return Err(error);
        }
        self.touch();
        Ok(())
    }

    /// Writes the entries that are not for this session only. Refuses when
    /// the file on disk is no longer the one this vault read or wrote last,
    /// so a damaged file, or one another window rewrote, is never replaced.
    pub(crate) fn persist(&mut self) -> Result<(), VaultError> {
        let Some(sealing) = &self.sealing else {
            return Err(VaultError::Locked);
        };
        let file = self.file()?.to_path_buf();
        self.look_again();
        match (read_bounded(&file), self.seen) {
            (Ok(bytes), Some(seen)) if fingerprint(&bytes) == seen => {}
            (Err(VaultError::Io(error)), None) if error.kind() == std::io::ErrorKind::NotFound => {}
            _ => return Err(VaultError::Changed),
        }
        let bytes = seal(sealing, self.entries.iter().filter(|e| !e.session))?;
        let temporary = file.with_extension("vault.tmp");
        write_private(&temporary, &bytes)?;
        if let Err(error) = std::fs::rename(&temporary, &file) {
            let _ = std::fs::remove_file(&temporary);
            return Err(error.into());
        }
        self.seen = Some(fingerprint(&bytes));
        Ok(())
    }

    /// Renames the file out of the way, for a vault that cannot be opened.
    /// Nothing is deleted: with the old master password it still opens.
    pub(crate) fn set_aside(&mut self) -> Result<PathBuf, VaultError> {
        let file = self.file()?.to_path_buf();
        self.look_again();
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or_default();
        let aside = file.with_extension(format!("vault.{stamp}.old"));
        std::fs::rename(&file, &aside)?;
        self.lock();
        Ok(aside)
    }

    fn changed(&mut self, kept: bool) -> Result<(), VaultError> {
        self.touch();
        if kept {
            self.persist()
        } else {
            Ok(())
        }
    }

    pub(crate) fn add(&mut self, entry: Saved) -> Result<(), VaultError> {
        if !entry.session && !self.unlocked() {
            return Err(VaultError::Locked);
        }
        let kept = !entry.session;
        self.entries.push(entry);
        let result = self.changed(kept);
        if result.is_err() {
            self.entries.pop();
        }
        result
    }

    pub(crate) fn replace(&mut self, index: usize, entry: Saved) -> Result<(), VaultError> {
        let Some(old) = self.entries.get(index) else {
            return Err(VaultError::Gone);
        };
        let kept = !old.session || !entry.session;
        if kept && !self.unlocked() {
            return Err(VaultError::Locked);
        }
        let old = std::mem::replace(&mut self.entries[index], entry);
        let result = self.changed(kept);
        if result.is_err() {
            self.entries[index] = old;
        }
        result
    }

    pub(crate) fn remove(&mut self, index: usize) -> Result<(), VaultError> {
        let Some(old) = self.entries.get(index) else {
            return Ok(());
        };
        let kept = !old.session;
        if kept && !self.unlocked() {
            return Err(VaultError::Locked);
        }
        let old = self.entries.remove(index);
        let result = self.changed(kept);
        if result.is_err() {
            self.entries.insert(index, old);
        }
        result
    }

    /// The entries that fit any of `archives`, best fit first. Entries that
    /// hold a password already listed are left out.
    pub(crate) fn matching(&self, archives: &[PathBuf], auto: bool) -> Vec<usize> {
        self.ranked(auto, |mask| {
            archives
                .iter()
                .filter_map(|archive| rank(mask, archive))
                .max()
        })
    }

    /// The entries that fit every one of `archives`, ranked by the one they
    /// fit worst. One password goes with all the archives of a job, so only
    /// these can be tried on it without being sent to an archive their mask
    /// leaves out.
    pub(crate) fn matching_all(&self, archives: &[PathBuf], auto: bool) -> Vec<usize> {
        self.ranked(auto, |mask| {
            archives
                .iter()
                .map(|archive| rank(mask, archive))
                .collect::<Option<Vec<_>>>()?
                .into_iter()
                .min()
        })
    }

    fn ranked(&self, auto: bool, fit: impl Fn(&str) -> Option<(u8, usize)>) -> Vec<usize> {
        let mut ranked: Vec<((u8, usize), usize)> = self
            .entries
            .iter()
            .enumerate()
            .filter(|(_, entry)| entry.auto == auto)
            .filter_map(|(i, entry)| fit(&entry.mask).map(|r| (r, i)))
            .collect();
        ranked.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
        let mut out: Vec<usize> = Vec::new();
        for (_, i) in ranked {
            if !out
                .iter()
                .any(|&j| *self.entries[j].password == *self.entries[i].password)
            {
                out.push(i);
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(label: &str, mask: &str, password: &str, auto: bool) -> Saved {
        Saved {
            label: label.into(),
            mask: mask.into(),
            password: Zeroizing::new(password.into()),
            auto,
            session: false,
        }
    }

    fn room() -> crate::test_support::Room {
        crate::test_support::Room::new()
    }

    fn sealed(entries: &[Saved], master: &str) -> Vec<u8> {
        let salt = [7u8; SALT];
        let key = derive(master, &salt, Cost::CHEAP).unwrap();
        let sealing = Sealing {
            key,
            salt,
            cost: Cost::CHEAP,
        };
        seal(&sealing, entries.iter()).unwrap()
    }

    #[test]
    fn whether_the_file_is_there_is_not_asked_of_the_disk_every_time() {
        let room = room();
        let file = room.path("passwords.vault");
        let mut keyring = Keyring::cheap(file.clone());
        assert!(!keyring.has_file());
        std::fs::write(&file, b"not a vault").unwrap();
        assert!(!keyring.has_file());
        assert!(keyring.unlock("master").is_err());
        assert!(keyring.has_file());
    }

    #[test]
    fn replacing_an_entry_that_is_not_there_says_so() {
        let mut keyring = Keyring::default();
        let mut kept = entry("a", "*.zip", "pw", false);
        kept.session = true;
        keyring.add(kept).unwrap();
        let mut other = entry("b", "*.zip", "pw", false);
        other.session = true;
        assert!(matches!(keyring.replace(1, other), Err(VaultError::Gone)));
        assert_eq!(keyring.entries.len(), 1);
        assert_eq!(keyring.entries[0].label, "a");
    }

    #[test]
    fn a_vault_reads_back_what_was_sealed() {
        let entries = [
            entry("Backups", "backup-*.7z", "s3cr\u{e9}t", true),
            entry("", "", "", false),
        ];
        let bytes = sealed(&entries, "master");
        let (_, back) = open(&bytes, "master").unwrap();
        assert_eq!(back.len(), 2);
        assert_eq!(back[0].label, "Backups");
        assert_eq!(back[0].mask, "backup-*.7z");
        assert_eq!(*back[0].password, "s3cr\u{e9}t");
        assert!(back[0].auto && !back[1].auto);
        assert!(back.iter().all(|e| !e.session));
    }

    #[test]
    fn the_wrong_master_password_is_told_apart_from_a_broken_header() {
        let bytes = sealed(&[entry("a", "", "pw", true)], "master");
        assert!(matches!(
            open(&bytes, "nope"),
            Err(VaultError::WrongOrDamaged)
        ));
        let mut newer = bytes.clone();
        newer[8] = 9;
        assert!(matches!(open(&newer, "master"), Err(VaultError::Newer)));
        let mut alien = bytes.clone();
        alien[0] = b'X';
        assert!(matches!(open(&alien, "master"), Err(VaultError::Damaged)));
    }

    #[test]
    fn every_tampered_or_truncated_vault_is_refused_without_panicking() {
        let bytes = sealed(&[entry("label", "*.zip", "password", true)], "master");
        for cut in 0..bytes.len() {
            assert!(open(&bytes[..cut], "master").is_err(), "cut at {cut}");
        }
        for at in 0..bytes.len() {
            let mut bad = bytes.clone();
            bad[at] ^= 0x01;
            assert!(open(&bad, "master").is_err(), "flip at {at}");
        }
    }

    #[test]
    fn a_vault_asking_for_absurd_memory_is_refused_before_deriving() {
        let mut bytes = sealed(&[], "master");
        bytes[10..14].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(matches!(open(&bytes, "master"), Err(VaultError::Damaged)));
    }

    #[test]
    fn malformed_payloads_are_errors() {
        assert!(decode(&[]).is_err());
        assert!(decode(&[1, 0, 0, 0]).is_err());
        assert!(decode(&[0xff, 0xff, 0xff, 0xff]).is_err());
        assert!(decode(&[1, 0, 0, 0, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]).is_err());
        let mut good = encode([entry("a", "b", "c", true)].iter()).to_vec();
        assert_eq!(decode(&good).unwrap().len(), 1);
        good.push(0);
        assert!(decode(&good).is_err(), "trailing bytes");
        let mut invalid = encode([entry("a", "b", "c", true)].iter()).to_vec();
        let last = invalid.len() - 1;
        invalid[last] = 0xff;
        assert!(decode(&invalid).is_err(), "not UTF-8");
    }

    #[test]
    fn a_new_vault_uses_the_standard_cost() {
        let room = room();
        let mut keyring = Keyring::at(Some(room.path("passwords.vault")));
        keyring.create("master").unwrap();
        let bytes = std::fs::read(room.path("passwords.vault")).unwrap();
        assert_eq!(&bytes[..10], b"ARCAVLT\0\x01\x01");
        let mut reader = Reader(&bytes[10..22]);
        let cost = Cost {
            memory_kib: reader.word().unwrap(),
            passes: reader.word().unwrap(),
            lanes: reader.word().unwrap(),
        };
        assert_eq!(cost, Cost::STANDARD);
    }

    #[test]
    fn the_keyring_keeps_only_what_is_not_for_the_session() {
        let room = room();
        let file = room.path("passwords.vault");
        let mut keyring = Keyring::cheap(file.clone());
        let mut early = entry("early", "", "before-master", true);
        early.session = true;
        keyring.add(early).unwrap();
        keyring.create("master").unwrap();
        let mut session = entry("session", "", "only-today", true);
        session.session = true;
        keyring.add(session).unwrap();
        keyring
            .add(entry("kept", "*.zip", "on-disk", false))
            .unwrap();
        let bytes = std::fs::read(&file).unwrap();
        for secret in ["before-master", "only-today", "on-disk", "master"] {
            assert!(
                !bytes.windows(secret.len()).any(|w| w == secret.as_bytes()),
                "{secret} in the clear"
            );
        }
        keyring.lock();
        assert_eq!(keyring.entries.len(), 1);
        assert_eq!(keyring.entries[0].label, "session");
        assert!(matches!(
            keyring.unlock("wrong"),
            Err(VaultError::WrongOrDamaged)
        ));
        keyring.unlock("master").unwrap();
        let labels: Vec<&str> = keyring.entries.iter().map(|e| e.label.as_str()).collect();
        assert_eq!(labels, ["early", "kept", "session"]);
        keyring.remove(0).unwrap();
        keyring.change_master("other").unwrap();
        let mut again = Keyring::cheap(file);
        assert!(again.unlock("master").is_err());
        again.unlock("other").unwrap();
        let labels: Vec<&str> = again.entries.iter().map(|e| e.label.as_str()).collect();
        assert_eq!(labels, ["kept"]);
    }

    #[test]
    fn a_damaged_vault_is_never_written_over() {
        let room = room();
        let file = room.path("passwords.vault");
        let mut keyring = Keyring::cheap(file.clone());
        keyring.create("master").unwrap();
        keyring.add(entry("a", "", "first", true)).unwrap();
        let mut bytes = std::fs::read(&file).unwrap();
        let last = bytes.len() - 1;
        bytes[last] ^= 0x55;
        std::fs::write(&file, &bytes).unwrap();
        assert!(matches!(
            keyring.add(entry("b", "", "second", true)),
            Err(VaultError::Changed)
        ));
        assert_eq!(keyring.entries.len(), 1);
        assert_eq!(std::fs::read(&file).unwrap(), bytes);
        let mut fresh = Keyring::cheap(file.clone());
        assert!(fresh.unlock("master").is_err());
        assert!(matches!(fresh.create("master"), Err(VaultError::Exists)));
        assert!(matches!(
            fresh.add(entry("c", "", "third", true)),
            Err(VaultError::Locked)
        ));
        assert_eq!(std::fs::read(&file).unwrap(), bytes);
        let aside = fresh.set_aside().unwrap();
        assert_eq!(std::fs::read(aside).unwrap(), bytes);
        assert!(!file.exists());
        fresh.create("new").unwrap();
    }

    #[test]
    fn the_vault_locks_itself_after_a_while_unused() {
        let room = room();
        let mut keyring = Keyring::cheap(room.path("passwords.vault"));
        keyring.create("master").unwrap();
        keyring.add(entry("a", "", "kept", true)).unwrap();
        let now = Instant::now();
        assert!(keyring.tick(now).is_some());
        assert!(keyring.unlocked());
        assert!(keyring.tick(now + LOCK_AFTER).is_none());
        assert!(!keyring.unlocked());
        assert!(keyring.entries.is_empty());
    }

    #[test]
    fn masks_rank_the_most_specific_first() {
        let archive = Path::new("/data/Backups/backup-2024.7z");
        assert_eq!(rank("", archive), Some((0, 0)));
        assert_eq!(rank("*.zip", archive), None);
        assert!(rank("*.7Z", archive).is_some(), "case is ignored");
        assert!(rank("/data/backups", archive).is_some(), "a folder");
        assert!(rank("/data/backups/", archive).is_some());
        assert!(rank("/data/back", archive).is_none(), "not a prefix match");
        assert!(rank("*.zip; *.7z", archive).is_some());
        let verbatim = Path::new(r"\\?\D:\Backups\x.zip");
        assert!(rank(r"D:\Backups\", verbatim).is_some());
        assert!(rank(r"\\server\share", Path::new(r"\\?\UNC\server\share\x.zip")).is_some());
        assert!(rank("backup-2024.7z", archive) > rank("backup-*.7z", archive));
        assert!(rank("backup-*.7z", archive) > rank("*.7z", archive));
        assert!(rank("*.7z", archive) > rank("", archive));

        let mut keyring = Keyring::default();
        for (label, mask, pw, auto) in [
            ("any", "", "p-any", true),
            ("seven", "*.7z", "p-seven", true),
            ("offered", "*.7z", "p-offered", false),
            ("zip", "*.zip", "p-zip", true),
            ("exact", "backup-2024.7z", "p-exact", true),
            ("same", "backup-*.7z", "p-seven", true),
            ("prefix", "backup-*.7z", "p-prefix", true),
        ] {
            let mut saved = entry(label, mask, pw, auto);
            saved.session = true;
            keyring.add(saved).unwrap();
        }
        let tried: Vec<&str> = keyring
            .matching(&[archive.to_path_buf()], true)
            .into_iter()
            .map(|i| keyring.entries[i].label.as_str())
            .collect();
        assert_eq!(tried, ["exact", "same", "prefix", "any"]);
        let offered = keyring.matching(&[archive.to_path_buf()], false);
        assert_eq!(offered.len(), 1);
        assert_eq!(keyring.entries[offered[0]].label, "offered");
    }
}
