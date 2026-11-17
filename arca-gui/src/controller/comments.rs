//! Comments on the open archive.

use super::*;

#[derive(Default)]
pub(crate) struct CommentState {
    /// The comments of the archive listed last, and which archive that was.
    pub(crate) listed: Option<(PathBuf, Comments)>,
}

impl AppController {
    fn listed_comments(&self) -> Option<&Comments> {
        let (path, comments) = self.state.comments.listed.as_ref()?;
        (self.state.archive.as_ref() == Some(path)).then_some(comments)
    }

    pub(crate) fn archive_comment(&self) -> String {
        self.listed_comments()
            .map(|c| c.archive.clone())
            .unwrap_or_default()
    }

    pub(crate) fn entry_comment(&self, entry: usize) -> Option<&str> {
        let offset = self.state.entries.get(entry)?.offset;
        self.listed_comments()?
            .entries
            .get(&offset)
            .map(String::as_str)
    }

    /// Whether the comments of what is open can be changed: a whole ZIP.
    pub(crate) fn can_comment(&self) -> bool {
        self.writable() && self.state.format == Format::Zip
    }

    pub(crate) fn save_comment(&mut self, entry: Option<String>, text: String) {
        let Some(archive) = self.state.archive.clone() else {
            return;
        };
        let password = self.state.archive_password.clone();
        self.run_job(Job::Comment {
            archive,
            entry,
            text,
            password,
        });
    }
}
