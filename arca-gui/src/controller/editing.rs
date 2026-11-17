//! What changing an archive in place needs from the window before it starts.

use super::*;

impl AppController {
    /// A 7z is decoded and encoded again to be changed, so changing an
    /// encrypted one takes its password, which a 7z with names anyone can read
    /// opens without. The job comes back when it can run now; otherwise the
    /// password is asked for and the job runs again once it is given.
    pub(super) fn with_password(&mut self, mut job: Job) -> Option<Job> {
        let (Job::Delete {
            archive, password, ..
        }
        | Job::Rename {
            archive, password, ..
        }
        | Job::Move {
            archive, password, ..
        }
        | Job::NewFolder {
            archive, password, ..
        }
        | Job::Add {
            archive, password, ..
        }) = &mut job
        else {
            return Some(job);
        };
        if detect(archive) != Some(Format::SevenZ) || self.state.archive.as_ref() != Some(archive) {
            return Some(job);
        }
        if password.is_none() {
            password.clone_from(&self.state.archive_password);
        }
        if password.is_some() || !self.state.entries.iter().any(|e| e.encrypted) {
            return Some(job);
        }
        self.read_access(AppAction::Run(job));
        None
    }
}
