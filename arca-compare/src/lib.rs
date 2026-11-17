//! Checksums of the entries of any archive Arca reads, worked out by
//! streaming them through the hasher instead of extracting them, and the
//! comparison of two archives, or of an archive and a folder, built on them.

mod digest;
mod side;
#[cfg(test)]
mod tests;

pub use digest::{digest_entries, digest_file, tar_source, Digest};
pub use side::{Kind, Side};

use arca_core::hash::Algorithm;
use arca_core::Result;
use rayon::prelude::*;
use side::{Item, Origin, Source};
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};

pub type Notify<'a> = &'a (dyn Fn(usize, usize, &str) -> bool + Sync);

// A ZIP keeps its times in DOS form, to the even second, and some tools round
// up where others round down: two seconds apart is still the same time.
const TIME_SLACK: i64 = 2;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Change {
    Added,
    Removed,
    Changed,
    Type,
    Time,
}

impl Change {
    pub fn marker(self) -> char {
        match self {
            Change::Added => '+',
            Change::Removed => '-',
            Change::Changed => 'M',
            Change::Type => 'T',
            Change::Time => 'm',
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Difference {
    pub change: Change,
    pub path: String,
}

impl Difference {
    pub fn line(&self) -> String {
        format!("{} {}", self.change.marker(), self.path)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Options {
    pub quick: bool,
    pub times: bool,
}

enum Verdict {
    Same,
    Differs(Change),
    Read,
}

fn shown(name: &str, item: &Item) -> String {
    if item.kind == Kind::Folder {
        format!("{name}/")
    } else {
        name.to_string()
    }
}

fn times_differ(a: &Item, b: &Item) -> bool {
    matches!((a.mtime, b.mtime), (Some(x), Some(y)) if (x - y).abs() > TIME_SLACK)
}

fn judge(a: &Item, b: &Item, options: Options) -> Verdict {
    if !a.kind.same_type(&b.kind) {
        return Verdict::Differs(Change::Type);
    }
    match (&a.kind, &b.kind) {
        (Kind::Folder, _) => Verdict::Same,
        (Kind::Link(x), Kind::Link(y)) if x != y => Verdict::Differs(Change::Changed),
        (Kind::Link(_), _) => Verdict::Same,
        _ => {
            let sizes = matches!((a.size, b.size), (Some(x), Some(y)) if x != y);
            let crcs = matches!((a.crc32, b.crc32), (Some(x), Some(y)) if x != y);
            if sizes || crcs {
                Verdict::Differs(Change::Changed)
            } else if options.quick {
                Verdict::Same
            } else {
                Verdict::Read
            }
        }
    }
}

// A side that cannot be read, or an entry that has to be read and cannot be,
// is an error and never a difference: a damaged archive must not pass for a
// changed one.
pub fn compare(
    a: &Side,
    b: &Side,
    options: Options,
    notify: Notify<'_>,
) -> Result<Vec<Difference>> {
    let mut names: Vec<&String> = a.items.keys().chain(b.items.keys()).collect();
    names.sort();
    names.dedup();
    let mut rows: Vec<(&str, Option<&Item>, Option<&Item>, Verdict)> = Vec::new();
    let (mut read_a, mut read_b) = (Vec::new(), Vec::new());
    for name in names {
        let (x, y) = (a.items.get(name), b.items.get(name));
        let verdict = match (x, y) {
            (Some(_), None) => Verdict::Differs(Change::Removed),
            (None, Some(_)) => Verdict::Differs(Change::Added),
            (Some(x), Some(y)) => judge(x, y, options),
            (None, None) => Verdict::Same,
        };
        if let (Verdict::Read, Some(x), Some(y)) = (&verdict, x, y) {
            read_a.push((name.as_str(), x));
            read_b.push((name.as_str(), y));
        }
        rows.push((name, x, y, verdict));
    }
    let total = read_a.len() + read_b.len();
    let sums_a = digests(a, &read_a, notify, 0, total)?;
    let sums_b = digests(b, &read_b, notify, read_a.len(), total)?;
    let _ = notify(total, total, "");
    let mut out = Vec::new();
    for (name, x, y, verdict) in rows {
        let change = match verdict {
            Verdict::Differs(change) => Some(change),
            Verdict::Read => match (sums_a.get(name), sums_b.get(name)) {
                (Some(x), Some(y)) => (x != y).then_some(Change::Changed),
                _ => {
                    return Err(arca_core::Error::Format(format!(
                        "'{name}' is listed but could not be read"
                    )))
                }
            },
            Verdict::Same => None,
        };
        let change = match (change, x, y) {
            (None, Some(x), Some(y)) if options.times && x.kind != Kind::Folder => {
                times_differ(x, y).then_some(Change::Time)
            }
            (change, ..) => change,
        };
        if let Some(change) = change {
            let item = match change {
                Change::Added => y,
                _ => x,
            };
            out.push(Difference {
                change,
                path: match (change, item) {
                    (Change::Type, _) | (_, None) => name.to_string(),
                    (_, Some(item)) => shown(name, item),
                },
            });
        }
    }
    Ok(out)
}

fn digests(
    side: &Side,
    wanted: &[(&str, &Item)],
    notify: Notify<'_>,
    offset: usize,
    total: usize,
) -> Result<HashMap<String, String>> {
    let mut found = HashMap::new();
    if wanted.is_empty() {
        return Ok(found);
    }
    let count = wanted.len();
    let shifted = |done: usize, _: usize, name: &str| notify(offset + done.min(count), total, name);
    match &side.source {
        Source::Archive {
            path,
            format,
            entries,
            password,
        } => {
            let mut pick = vec![false; entries.len()];
            let mut named: HashMap<usize, Vec<&str>> = HashMap::new();
            for (name, item) in wanted {
                if let Origin::Entry(i) = item.origin {
                    if let Some(slot) = pick.get_mut(i) {
                        *slot = true;
                        named.entry(i).or_default().push(name);
                    }
                }
            }
            let read = digest_entries(
                path,
                *format,
                entries,
                &pick,
                password.as_deref(),
                &[Algorithm::Sha256],
                &shifted,
            )?;
            for digest in read {
                let mut sums = digest.sums?;
                let sum = sums.pop().unwrap_or_default();
                for name in named.get(&digest.index).into_iter().flatten() {
                    found.insert(name.to_string(), sum.clone());
                }
            }
        }
        Source::Folder => {
            let done = AtomicUsize::new(0);
            let read = wanted
                .par_iter()
                .filter_map(|(name, item)| match &item.origin {
                    Origin::Disk(path) => Some((name, path)),
                    _ => None,
                })
                .map(|(name, path)| {
                    let at = (done.load(Ordering::Relaxed), count, *name);
                    if !shifted(at.0, count, name) {
                        return Err(arca_core::Error::Cancelled);
                    }
                    let mut sums = digest_file(path, &[Algorithm::Sha256], &shifted, at)?;
                    done.fetch_add(1, Ordering::Relaxed);
                    Ok((name.to_string(), sums.pop().unwrap_or_default()))
                })
                .collect::<Result<Vec<_>>>()?;
            found.extend(read);
        }
    }
    Ok(found)
}
