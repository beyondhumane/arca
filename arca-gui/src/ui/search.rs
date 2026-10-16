//! The search palette: Ctrl+K, a field in the middle of the window, and the
//! files and folders under the current place that match what is typed.
//!
//! An archive is already in memory, so it is searched as it is. A disk folder
//! is walked on another thread, in batches, with a ceiling on how much is
//! read, so a search from the top of a large disk still answers quickly and
//! never grows without bound.

use super::theme::{alpha, hex, palette_of};
use super::widgets::*;
use super::*;
use crate::controller::disk::{disk_path, hidden_on_disk};
use eframe::egui::{self, RichText};
use egui_phosphor::regular as icon;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver};
use std::sync::Arc;
use std::time::{Duration, Instant};

const MOST_INDEXED: usize = 200_000;
const DEEPEST: usize = 48;
const TIME_BUDGET: Duration = Duration::from_secs(8);
const BATCH: usize = 512;
const MOST_SHOWN: usize = 200;
const ROW: f32 = 34.0;

/// One thing that can be found: its leaf name, its path in the form the
/// browser uses (folders end in `/`), and which of the two it is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Hit {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
}

impl Hit {
    /// The folder the hit sits in, trailing slash included; empty at the top.
    pub(crate) fn parent(&self) -> String {
        let inner = self.path.trim_end_matches('/');
        match inner.rfind('/') {
            Some(cut) => inner[..=cut].to_string(),
            None => String::new(),
        }
    }
}

pub(crate) enum Pick {
    Open(Hit),
    Reveal(Hit),
}

pub(crate) struct Search {
    pub(crate) query: String,
    pub(crate) pick: usize,
    pub(crate) focus: bool,
    pub(crate) scope: String,
    index: Vec<Hit>,
    rx: Option<Receiver<Vec<Hit>>>,
    stop: Arc<AtomicBool>,
    shown: Vec<usize>,
    filtered_for: (String, usize),
    follow: bool,
    scroll: f32,
}

impl Drop for Search {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

impl Search {
    pub(crate) fn over_archive(index: Vec<Hit>, scope: String) -> Self {
        Self::new(index, None, scope)
    }

    pub(crate) fn over_disk(dir: &str, scope: String) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let (tx, rx) = channel();
        let root = disk_path(dir);
        let halt = stop.clone();
        std::thread::Builder::new()
            .name("arca-search".into())
            .spawn(move || walk(&root, &halt, |batch| tx.send(batch).is_ok()))
            .ok();
        let mut search = Self::new(Vec::new(), Some(rx), scope);
        search.stop = stop;
        search
    }

    fn new(index: Vec<Hit>, rx: Option<Receiver<Vec<Hit>>>, scope: String) -> Self {
        Search {
            query: String::new(),
            pick: 0,
            focus: true,
            scope,
            index,
            rx,
            stop: Arc::new(AtomicBool::new(false)),
            shown: Vec::new(),
            filtered_for: (String::from("\u{0}"), 0),
            follow: false,
            scroll: 0.0,
        }
    }

    /// Still reading folders in the background.
    pub(crate) fn busy(&self) -> bool {
        self.rx.is_some()
    }

    /// Takes in whatever the walker has sent since the last frame.
    pub(crate) fn receive(&mut self) {
        let Some(rx) = &self.rx else {
            return;
        };
        loop {
            match rx.try_recv() {
                Ok(batch) => self.index.extend(batch),
                Err(std::sync::mpsc::TryRecvError::Empty) => break,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.rx = None;
                    break;
                }
            }
        }
    }

    /// The hits that match the query, best first, recomputed only when the
    /// query or the index has changed.
    pub(crate) fn hits(&mut self) -> Vec<Hit> {
        let key = (self.query.clone(), self.index.len());
        if key != self.filtered_for {
            self.shown = rank(&self.index, &self.query);
            self.filtered_for = key;
            self.pick = self.pick.min(self.shown.len().saturating_sub(1));
        }
        self.shown.iter().map(|&i| self.index[i].clone()).collect()
    }

    pub(crate) fn step(&mut self, delta: isize) {
        let len = self.shown.len();
        if len == 0 {
            return;
        }
        let next = (self.pick as isize + delta).rem_euclid(len as isize);
        self.pick = next as usize;
        self.follow = true;
    }

    pub(crate) fn picked(&self) -> Option<Hit> {
        self.shown.get(self.pick).map(|&i| self.index[i].clone())
    }
}

/// Matching is by words: every word typed has to appear in the name, in any
/// order and any case. Names that start with the first word come first, then
/// shorter names, then the alphabet, so `read` finds `README` before
/// `spreadsheet.ods`.
pub(crate) fn rank(index: &[Hit], query: &str) -> Vec<usize> {
    let words: Vec<String> = query.split_whitespace().map(|w| w.to_lowercase()).collect();
    if words.is_empty() {
        return Vec::new();
    }
    let mut found: Vec<(u8, usize, String, usize)> = index
        .iter()
        .enumerate()
        .filter_map(|(i, hit)| {
            let lower = hit.name.to_lowercase();
            if !words.iter().all(|w| lower.contains(w.as_str())) {
                return None;
            }
            let lead = if lower.starts_with(words[0].as_str()) {
                0
            } else {
                1
            };
            Some((lead, lower.chars().count(), lower, i))
        })
        .collect();
    found.sort_unstable();
    found.truncate(MOST_SHOWN);
    found.into_iter().map(|(.., i)| i).collect()
}

/// The browser's name for a path on the disk: forward slashes, no leading
/// one, and a trailing one on folders. The inverse of `disk_path`.
fn disk_name(path: &Path, is_dir: bool) -> String {
    let text = path.to_string_lossy().replace('\\', "/");
    let text = text.trim_start_matches('/');
    if is_dir {
        format!("{text}/")
    } else {
        text.to_string()
    }
}

/// Reads everything under `root`, breadth first so the nearest things arrive
/// first, and hands it over in batches. Hidden folders are skipped, as are
/// symlinks, so a loop in the tree cannot keep it going. Stops at the
/// ceiling, the time budget, or the moment `halt` is raised.
fn walk(root: &Path, halt: &AtomicBool, mut send: impl FnMut(Vec<Hit>) -> bool) {
    let started = Instant::now();
    let mut queue = std::collections::VecDeque::from([(root.to_path_buf(), 0usize)]);
    let mut batch = Vec::with_capacity(BATCH);
    let mut total = 0usize;
    while let Some((dir, depth)) = queue.pop_front() {
        if halt.load(Ordering::Relaxed) || started.elapsed() > TIME_BUDGET {
            break;
        }
        let Ok(items) = std::fs::read_dir(&dir) else {
            continue;
        };
        for item in items.flatten() {
            if total >= MOST_INDEXED {
                break;
            }
            let leaf = item.file_name().to_string_lossy().to_string();
            let Ok(kind) = item.file_type() else {
                continue;
            };
            let meta = std::fs::symlink_metadata(item.path()).ok();
            if hidden_on_disk(&leaf, meta.as_ref()) {
                continue;
            }
            let is_dir = kind.is_dir();
            if is_dir && depth < DEEPEST {
                queue.push_back((item.path(), depth + 1));
            }
            batch.push(Hit {
                name: leaf,
                path: disk_name(&item.path(), is_dir),
                is_dir,
            });
            total += 1;
            if batch.len() >= BATCH && !send(std::mem::take(&mut batch)) {
                return;
            }
        }
        if total >= MOST_INDEXED {
            break;
        }
    }
    if !batch.is_empty() {
        send(batch);
    }
}

/// Every folder in the tree, as `a/b/`, that is not already a hit.
fn implied_folders(
    folder: &crate::tree::Folder,
    prefix: &str,
    known: &std::collections::HashSet<String>,
    out: &mut Vec<Hit>,
) {
    for (name, kid) in &folder.kids {
        let path = format!("{prefix}{name}/");
        if !known.contains(&path) {
            out.push(Hit {
                name: name.clone(),
                path: path.clone(),
                is_dir: true,
            });
        }
        implied_folders(kid, &path, known, out);
    }
}

impl Shell {
    /// Ctrl+K and the toolbar button: opens the palette over what is showing,
    /// or closes it if it is already up.
    pub(super) fn toggle_search(&mut self) {
        if self.search.is_some() {
            self.search = None;
            return;
        }
        if self.modal_kind().is_some() || self.dialog.is_some() {
            return;
        }
        let s = self.controller.s();
        let state = &self.controller.state;
        if self.controller.on_disk() {
            let dir = state.current_dir.clone();
            let scope = dir
                .trim_end_matches('/')
                .rsplit('/')
                .next()
                .filter(|leaf| !leaf.is_empty())
                .map_or_else(|| s.computer_word.to_string(), str::to_string);
            self.search = Some(Search::over_disk(&dir, scope));
        } else if let Some(archive) = &state.archive {
            let mut index: Vec<Hit> = state
                .entries
                .iter()
                .map(|entry| {
                    let path = entry.name.replace('\\', "/");
                    let name = path
                        .trim_end_matches('/')
                        .rsplit('/')
                        .next()
                        .unwrap_or(&path)
                        .to_string();
                    Hit {
                        name,
                        path,
                        is_dir: entry.is_dir,
                    }
                })
                .filter(|hit| !hit.name.is_empty())
                .collect();
            // Many zips never store their folders as entries; the tree
            // already knows them from the file names, so they can be found.
            let known: std::collections::HashSet<String> = index
                .iter()
                .filter(|hit| hit.is_dir)
                .map(|hit| hit.path.clone())
                .collect();
            implied_folders(&state.folders, "", &known, &mut index);
            let scope = archive
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            self.search = Some(Search::over_archive(index, scope));
        }
    }

    pub(super) fn search_key(&mut self, key: egui::Key, modifiers: egui::Modifiers) -> bool {
        let Some(search) = self.search.as_mut() else {
            return false;
        };
        match key {
            egui::Key::ArrowDown => search.step(1),
            egui::Key::ArrowUp => search.step(-1),
            egui::Key::Escape => self.search = None,
            egui::Key::Enter => {
                if let Some(hit) = search.picked() {
                    let pick = if modifiers.alt {
                        Pick::Reveal(hit)
                    } else {
                        Pick::Open(hit)
                    };
                    self.take_pick(pick);
                }
            }
            _ => return false,
        }
        true
    }

    fn take_pick(&mut self, pick: Pick) {
        self.search = None;
        let (target, select) = match pick {
            Pick::Open(hit) if hit.is_dir => (hit.path.clone(), None),
            Pick::Open(hit) => (hit.parent(), Some(hit.path)),
            Pick::Reveal(hit) => (hit.parent(), Some(hit.path)),
        };
        self.controller.go_to(target);
        let Some(path) = select else {
            return;
        };
        let rows = self.controller.visible_rows();
        let Some((index, row)) = rows
            .iter()
            .enumerate()
            .find(|(_, row)| row.path == path)
            .map(|(i, row)| (i, row.clone()))
        else {
            return;
        };
        self.controller.state.checked.fill(false);
        self.controller
            .dispatch(AppAction::SetChecked { row, value: true });
        let active = self.controller.state.browser.active;
        self.controller.set_pane_cursor(active, Some(index));
        self.scroll_to_cursor = true;
    }

    pub(super) fn search_dialog(&mut self, ui: &mut egui::Ui) {
        let s = self.controller.s();
        let p = palette_of(ui.ctx());
        let Some(search) = self.search.as_mut() else {
            return;
        };
        search.receive();
        let busy = search.busy();
        if busy {
            ui.ctx().request_repaint_after(Duration::from_millis(80));
        }
        egui::Frame::new()
            .inner_margin(egui::Margin::symmetric(14, 10))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(icon::MAGNIFYING_GLASS)
                            .size(18.0)
                            .color(hex(p.muted)),
                    );
                    let room = ui.available_width() - if busy { 30.0 } else { 0.0 };
                    let response = ui.add(
                        egui::TextEdit::singleline(&mut search.query)
                            .id(egui::Id::new("arca-search-field"))
                            .lock_focus(true)
                            .frame(egui::Frame::NONE)
                            .hint_text(s.search_files)
                            .font(egui::TextStyle::Heading)
                            .desired_width(room.max(80.0)),
                    );
                    if search.focus || !response.has_focus() {
                        response.request_focus();
                        search.focus = false;
                    }
                    if busy {
                        ui.add(egui::Spinner::new().size(16.0).color(hex(p.focus)));
                    }
                });
            });
        let line = ui.available_rect_before_wrap();
        ui.painter().hline(
            line.x_range(),
            line.top(),
            egui::Stroke::new(1.0, alpha(p.border, 0.8)),
        );
        let hits = search.hits();
        let pick = search.pick;
        let follow = std::mem::take(&mut search.follow);
        let scroll = search.scroll;
        let scope = search.scope.clone();
        let query_empty = search.query.trim().is_empty();
        let mut chosen = None;
        let height = (ui.ctx().content_rect().height() - 220.0).clamp(160.0, 380.0);
        if hits.is_empty() {
            let text = if query_empty {
                fill(s.search_scope, &[("place", &scope)])
            } else if busy {
                s.search_reading.to_string()
            } else {
                s.search_none.to_string()
            };
            ui.allocate_ui_with_layout(
                egui::vec2(ui.available_width(), height),
                egui::Layout::centered_and_justified(egui::Direction::TopDown),
                |ui| {
                    ui.label(RichText::new(text).color(hex(p.muted)));
                },
            );
        } else {
            // Only the rows in view are laid out, so a pick that moved off
            // screen cannot ask to be scrolled to: the offset is set here
            // from where the row would be instead.
            let mut area = egui::ScrollArea::vertical()
                .max_height(height)
                .min_scrolled_height(height)
                .auto_shrink([false, false]);
            if follow {
                let top = pick as f32 * ROW;
                let bottom = top + ROW + 8.0;
                let offset = if top < scroll {
                    top
                } else if bottom > scroll + height {
                    bottom - height
                } else {
                    scroll
                };
                area = area.vertical_scroll_offset(offset.max(0.0));
            }
            let shown = area.show_rows(ui, ROW, hits.len(), |ui, range| {
                ui.add_space(4.0);
                for i in range {
                    let hit = &hits[i];
                    let size = egui::vec2(ui.available_width() - 12.0, ROW - 2.0);
                    ui.horizontal(|ui| {
                        ui.add_space(6.0);
                        let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());
                        let on = i == pick;
                        if on {
                            ui.painter().rect_filled(rect, 6, alpha(p.selected, 1.0));
                        } else if response.hovered() {
                            ui.painter().rect_filled(rect, 6, alpha(p.focus, 0.08));
                        }
                        let glyph = if hit.is_dir { icon::FOLDER } else { icon::FILE };
                        let painter = ui.painter();
                        let left = rect.left() + 10.0;
                        painter.text(
                            egui::pos2(left, rect.center().y),
                            egui::Align2::LEFT_CENTER,
                            glyph,
                            egui::FontId::proportional(16.0),
                            hex(if hit.is_dir { p.focus } else { p.muted }),
                        );
                        let name = painter.layout_no_wrap(
                            hit.name.clone(),
                            egui::TextStyle::Body.resolve(ui.style()),
                            hex(p.text),
                        );
                        let name_at =
                            egui::pos2(left + 26.0, rect.center().y - name.size().y / 2.0);
                        painter.galley(name_at, name.clone(), hex(p.text));
                        let room = rect.right() - 10.0 - (name_at.x + name.size().x + 12.0);
                        if room > 40.0 {
                            let parent = hit.parent();
                            let shown = if parent.is_empty() {
                                scope.clone()
                            } else {
                                parent
                            };
                            let where_at = painter.layout(
                                shown,
                                egui::TextStyle::Small.resolve(ui.style()),
                                hex(p.muted),
                                room,
                            );
                            painter.galley(
                                egui::pos2(
                                    rect.right() - 10.0 - where_at.size().x.min(room),
                                    rect.center().y - where_at.size().y / 2.0,
                                ),
                                where_at,
                                hex(p.muted),
                            );
                        }
                        if response.clicked() {
                            chosen = Some(if ui.input(|i| i.modifiers.alt) {
                                Pick::Reveal(hit.clone())
                            } else {
                                Pick::Open(hit.clone())
                            });
                        }
                    });
                }
                ui.add_space(4.0);
            });
            search.scroll = shown.state.offset.y;
        }
        let line = ui.available_rect_before_wrap();
        ui.painter().hline(
            line.x_range(),
            line.top(),
            egui::Stroke::new(1.0, alpha(p.border, 0.8)),
        );
        egui::Frame::new()
            .inner_margin(egui::Margin::symmetric(14, 8))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 6.0;
                    for (keys, what) in [
                        ("↑↓", s.hint_navigate),
                        ("↵", s.hint_open),
                        ("Alt+↵", s.hint_containing),
                    ] {
                        kbd(ui, keys);
                        ui.label(RichText::new(what).small().color(hex(p.muted)));
                        ui.add_space(8.0);
                    }
                });
            });
        if let Some(pick) = chosen {
            self.take_pick(pick);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hit(path: &str) -> Hit {
        let is_dir = path.ends_with('/');
        let name = path
            .trim_end_matches('/')
            .rsplit('/')
            .next()
            .unwrap_or(path)
            .to_string();
        Hit {
            name,
            path: path.to_string(),
            is_dir,
        }
    }

    #[test]
    fn parent_is_the_folder_above_with_its_slash() {
        assert_eq!(hit("a/b/c.txt").parent(), "a/b/");
        assert_eq!(hit("a/b/").parent(), "a/");
        assert_eq!(hit("top.txt").parent(), "");
        assert_eq!(hit("top/").parent(), "");
    }

    #[test]
    fn ranking_prefers_names_that_start_with_the_word_then_shorter_ones() {
        let index = vec![
            hit("docs/spreadsheet.ods"),
            hit("README"),
            hit("src/reader.rs"),
            hit("other.txt"),
        ];
        let got: Vec<&str> = rank(&index, "read")
            .into_iter()
            .map(|i| index[i].name.as_str())
            .collect();
        assert_eq!(got, ["README", "reader.rs", "spreadsheet.ods"]);
        assert!(rank(&index, "").is_empty());
        assert!(rank(&index, "zzz").is_empty());
    }

    #[test]
    fn every_word_has_to_match_in_any_order() {
        let index = vec![hit("Photos 2024/beach day.jpg"), hit("beach.txt")];
        let got = rank(&index, "day beach");
        assert_eq!(got, [0]);
    }

    #[test]
    fn folders_the_zip_never_stored_are_still_found() {
        let mut root = crate::tree::Folder::default();
        let mut docs = crate::tree::Folder::default();
        docs.kids
            .insert("deep".into(), crate::tree::Folder::default());
        root.kids.insert("docs".into(), docs);
        root.kids
            .insert("stored".into(), crate::tree::Folder::default());
        let known = std::collections::HashSet::from(["stored/".to_string()]);
        let mut out = Vec::new();
        implied_folders(&root, "", &known, &mut out);
        let paths: Vec<&str> = out.iter().map(|h| h.path.as_str()).collect();
        assert_eq!(paths, ["docs/", "docs/deep/"]);
        assert!(out.iter().all(|h| h.is_dir));
    }

    #[test]
    fn disk_names_mirror_disk_paths() {
        let dir = disk_name(Path::new("/tmp/dnd-test"), true);
        let file = disk_name(Path::new("/tmp/dnd-test/a.txt"), false);
        if cfg!(windows) {
            assert!(dir.ends_with("tmp/dnd-test/"));
        } else {
            assert_eq!(dir, "tmp/dnd-test/");
            assert_eq!(file, "tmp/dnd-test/a.txt");
            assert_eq!(disk_path(&file), Path::new("/tmp/dnd-test/a.txt"));
        }
    }

    #[test]
    fn walking_a_folder_lists_what_is_under_it_and_can_be_halted() {
        let dir = std::env::temp_dir().join(format!("arca-search-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("inner/deeper")).unwrap();
        std::fs::create_dir_all(dir.join(".hidden")).unwrap();
        std::fs::write(dir.join("inner/deeper/leaf.txt"), b"x").unwrap();
        std::fs::write(dir.join(".hidden/secret.txt"), b"x").unwrap();
        let mut got = Vec::new();
        walk(&dir, &AtomicBool::new(false), |batch| {
            got.extend(batch);
            true
        });
        let names: Vec<&str> = got.iter().map(|h| h.name.as_str()).collect();
        assert!(names.contains(&"inner"));
        assert!(names.contains(&"deeper"));
        assert!(names.contains(&"leaf.txt"));
        assert!(!names
            .iter()
            .any(|n| n.contains("secret") || n.starts_with('.')));
        let mut none = Vec::new();
        walk(&dir, &AtomicBool::new(true), |batch| {
            none.extend(batch);
            true
        });
        assert!(none.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
