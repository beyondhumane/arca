//! Repairing a damaged archive into a copy beside it.

use super::*;

#[derive(Default)]
pub(crate) struct RepairState {
    /// A repair that wrote a copy, waiting to be offered for opening.
    pub(crate) offer: Option<RepairOffer>,
}

pub(crate) struct RepairOffer {
    pub(crate) path: PathBuf,
    pub(crate) password: Option<String>,
    pub(crate) text: String,
}

impl AppController {
    /// Whether `path` is a file whose format a repair takes.
    pub(crate) fn can_repair(path: &Path) -> bool {
        path.is_file() && detect(path).is_some_and(repairable)
    }

    /// Repairs `archive` into a new file beside it, named after it.
    pub(crate) fn start_repair(&mut self, archive: PathBuf) {
        let out = repaired_name(&archive, self.s().repaired_suffix);
        let password = if self.state.archive.as_ref() == Some(&archive) {
            self.state.archive_password.clone()
        } else {
            None
        };
        self.run_job(Job::Repair {
            archive,
            out,
            password,
        });
    }

    pub(crate) fn repair_offer(&self) -> Option<&RepairOffer> {
        self.state.repair.offer.as_ref()
    }

    pub(crate) fn answer_repair_offer(&mut self, open: bool) {
        let Some(offer) = self.state.repair.offer.take() else {
            return;
        };
        if open {
            self.open_with_password(offer.path, offer.password);
        }
    }
}
