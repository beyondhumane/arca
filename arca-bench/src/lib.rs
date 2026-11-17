#![forbid(unsafe_code)]
//! A benchmark of the codecs Arca writes, in the spirit of 7-Zip's: data
//! generated in memory from a fixed seed, so nothing is read from or written
//! to the disk, compressed and decompressed by every codec on one thread and
//! on several, with a rating to compare machines by.
//!
//! The corpus is cut into chunks of [`corpus::CHUNK`] bytes and every chunk
//! is compressed on its own, the way a ZIP compresses its entries. A run on
//! `n` threads takes the next `n` chunks and works on them side by side, so
//! one thread and many measure the same work and only the parallelism
//! differs.

mod codecs;
pub mod corpus;
mod report;
#[cfg(test)]
mod tests;

use arca_core::{Error, Level, Result};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;

pub use report::{json, speed_text, table};

/// Every codec runs at Arca's default level, the one `arca create` uses.
pub const LEVEL: Level = Level::NORMAL;

/// What the rating is worked out from: codecs every build has.
pub const RATED: [Codec; 3] = [Codec::Deflate, Codec::Lzma2, Codec::Bzip2];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Codec {
    Store,
    Deflate,
    Zstd,
    Lzma2,
    Bzip2,
    Rar,
}

impl Codec {
    pub const ALL: [Codec; 6] = [
        Codec::Store,
        Codec::Deflate,
        Codec::Zstd,
        Codec::Lzma2,
        Codec::Bzip2,
        Codec::Rar,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Codec::Store => "Store",
            Codec::Deflate => "Deflate",
            Codec::Zstd => "Zstandard",
            Codec::Lzma2 => "LZMA2",
            Codec::Bzip2 => "bzip2",
            Codec::Rar => "RAR5",
        }
    }

    pub fn id(self) -> &'static str {
        match self {
            Codec::Store => "store",
            Codec::Deflate => "deflate",
            Codec::Zstd => "zstd",
            Codec::Lzma2 => "lzma2",
            Codec::Bzip2 => "bzip2",
            Codec::Rar => "rar5",
        }
    }

    /// Threads this codec gets when `threads` are asked for. The LZMA2
    /// encoder is the only one big enough to need a cap: it takes as many as
    /// fit in the memory budget `arca create` gives a `.tar.xz`.
    pub fn threads(self, threads: usize) -> usize {
        match self {
            Codec::Lzma2 => arca_xz::workers(LEVEL, threads),
            _ => threads.max(1),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Options {
    /// Threads for the second measurement; 1 measures only one thread.
    pub threads: usize,
    /// How long each measurement keeps repeating its runs. At least one run
    /// is always made.
    pub seconds: f64,
    /// Corpus size in MiB, raised to one chunk per thread when smaller.
    pub size_mib: usize,
    pub codecs: Vec<Codec>,
    pub kind: corpus::Kind,
}

impl Options {
    pub const SECONDS: f64 = 1.0;
    pub const SIZE_MIB: usize = 16;

    pub fn new(threads: usize) -> Self {
        Options {
            threads: threads.max(1),
            seconds: Self::SECONDS,
            size_mib: Self::SIZE_MIB,
            codecs: Codec::ALL.to_vec(),
            kind: corpus::Kind::Mixed,
        }
    }

    /// Chunks in the corpus: the size asked for, or one per thread if that
    /// is more.
    pub fn chunks(&self) -> usize {
        self.size_mib.max(self.threads).max(1)
    }

    pub fn corpus_mib(&self) -> usize {
        (self.chunks() * corpus::CHUNK) >> 20
    }

    fn modes(&self) -> Vec<usize> {
        if self.threads > 1 {
            vec![1, self.threads]
        } else {
            vec![1]
        }
    }

    /// The `arca` command that measures the same thing.
    pub fn command(&self) -> String {
        format!(
            "arca bench --codecs --threads {} --seconds {} --size {}",
            self.threads, self.seconds, self.size_mib
        )
    }

    /// An upper estimate of the data the benchmark holds at once, in MiB:
    /// the corpus, one compressed copy of it, and a chunk in and a chunk out
    /// per thread. Codec state comes on top; see [`Options::codec_state_mib`].
    pub fn data_mib(&self) -> u64 {
        let chunk_mib = (corpus::CHUNK >> 20) as u64;
        let chunks = self.chunks() as u64;
        2 * chunks * chunk_mib + 2 * self.threads as u64 * chunk_mib + 2 * chunk_mib
    }

    /// The largest codec state held at once, in MiB: the LZMA2 encoders, at
    /// the memory xz(1) documents for preset 6, on as many threads as their
    /// budget allows.
    pub fn codec_state_mib(&self) -> u64 {
        const XZ_PRESET_6_ENCODER_MIB: u64 = 94;
        XZ_PRESET_6_ENCODER_MIB * Codec::Lzma2.threads(self.threads) as u64
    }
}

/// Speeds in MB/s (10^6 bytes a second) of uncompressed data, the median of
/// the runs.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Speeds {
    pub compress: f64,
    pub decompress: f64,
    pub compress_runs: usize,
    pub decompress_runs: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Outcome {
    Measured {
        /// Compressed bytes over uncompressed bytes.
        ratio: f64,
        single: Option<Speeds>,
        /// On [`Row::threads`] threads.
        multi: Option<Speeds>,
    },
    /// This build does not have the codec, and why.
    Unavailable(String),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    pub codec: Codec,
    pub threads: usize,
    pub outcome: Outcome,
}

impl Row {
    pub fn speeds(&self, multi: bool) -> Option<Speeds> {
        match &self.outcome {
            Outcome::Measured {
                single, multi: m, ..
            } => {
                if multi {
                    *m
                } else {
                    *single
                }
            }
            Outcome::Unavailable(_) => None,
        }
    }

    pub fn ratio(&self) -> Option<f64> {
        match self.outcome {
            Outcome::Measured { ratio, .. } => Some(ratio),
            Outcome::Unavailable(_) => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Report {
    pub options: Options,
    pub rows: Vec<Row>,
    /// Stopped before every measurement was made.
    pub stopped: bool,
    /// Whether this build has the native codecs (libdeflate and libzstd).
    /// The rating only compares between builds that agree on this.
    pub native: bool,
}

impl Report {
    /// The geometric mean of the compression and decompression speeds of
    /// the [`RATED`] codecs, on one thread or on many. None until all of
    /// them have been measured that way.
    pub fn rating(&self, multi: bool) -> Option<f64> {
        if multi && self.options.threads < 2 {
            return None;
        }
        let mut logs = Vec::new();
        for codec in RATED {
            let speeds = self.rows.iter().find(|r| r.codec == codec)?.speeds(multi)?;
            logs.push(speeds.compress.ln());
            logs.push(speeds.decompress.ln());
        }
        let mean = (logs.iter().sum::<f64>() / logs.len() as f64).exp();
        mean.is_finite().then_some(mean)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Generating,
    Compressing,
    Decompressing,
}

/// Where the benchmark is: `step` of `steps` measurements done, and what
/// the next one is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Progress {
    pub step: usize,
    pub steps: usize,
    pub codec: Option<Codec>,
    pub threads: usize,
    pub phase: Phase,
}

pub enum Event<'a> {
    Progress(Progress),
    /// A row as it stands, sent again each time it gains a measurement.
    Row(&'a Row),
}

const MB: f64 = 1_000_000.0;

fn median(mut values: Vec<f64>) -> f64 {
    values.sort_by(f64::total_cmp);
    let mid = values.len() / 2;
    if values.len().is_multiple_of(2) {
        (values[mid - 1] + values[mid]) / 2.0
    } else {
        values[mid]
    }
}

fn speed(bytes: usize, started: Instant) -> f64 {
    bytes as f64 / MB / started.elapsed().as_secs_f64().max(1e-9)
}

struct Batch {
    chunks: Vec<usize>,
    packed: codecs::Packed,
}

struct Measurement {
    speeds: Speeds,
    raw: u64,
    packed: u64,
}

struct Bench<'a> {
    options: &'a Options,
    corpus: &'a [Arc<[u8]>],
    stop: &'a AtomicBool,
    events: &'a dyn Fn(Event<'_>),
    step: usize,
    steps: usize,
}

impl Bench<'_> {
    fn stopped(&self) -> bool {
        self.stop.load(Ordering::Relaxed)
    }

    fn announce(&self, codec: Codec, threads: usize, phase: Phase) {
        (self.events)(Event::Progress(Progress {
            step: self.step,
            steps: self.steps,
            codec: Some(codec),
            threads,
            phase,
        }));
    }

    fn picked(&self, run: usize, threads: usize) -> Vec<usize> {
        (0..threads)
            .map(|j| (run * threads + j) % self.corpus.len())
            .collect()
    }

    /// Compression runs until the time is up, then decompression runs over
    /// what they produced. None if stopped first.
    fn measure(&mut self, codec: Codec, threads: usize) -> Result<Option<Measurement>> {
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(threads)
            .build()
            .map_err(|e| Error::Format(format!("could not create the thread pool: {e}")))?;
        let n = self.corpus.len();
        self.announce(codec, threads, Phase::Compressing);
        let mut kept: Vec<Batch> = Vec::new();
        let mut speeds = Vec::new();
        let started = Instant::now();
        let mut run = 0;
        while run == 0 || started.elapsed().as_secs_f64() < self.options.seconds {
            if self.stopped() {
                return Ok(None);
            }
            let chunks = self.picked(run, threads);
            let data: Vec<Arc<[u8]>> = chunks
                .iter()
                .map(|&i| Arc::clone(&self.corpus[i]))
                .collect();
            let bytes = data.iter().map(|c| c.len()).sum();
            let at = Instant::now();
            let packed = pool.install(|| codecs::pack(codec, &data))?;
            speeds.push(speed(bytes, at));
            if run * threads < n {
                kept.push(Batch { chunks, packed });
            }
            run += 1;
        }
        let compress = median(speeds);
        let compress_runs = run;
        self.step += 1;
        self.announce(codec, threads, Phase::Decompressing);
        let mut speeds = Vec::new();
        let started = Instant::now();
        let mut run = 0;
        while run == 0 || started.elapsed().as_secs_f64() < self.options.seconds {
            if self.stopped() {
                return Ok(None);
            }
            let batch = &kept[run % kept.len()];
            let sizes: Vec<usize> = batch.chunks.iter().map(|&i| self.corpus[i].len()).collect();
            let at = Instant::now();
            let out = pool.install(|| codecs::unpack(codec, &batch.packed, &sizes))?;
            speeds.push(speed(sizes.iter().sum(), at));
            let same = out.len() == batch.chunks.len()
                && out
                    .iter()
                    .zip(&batch.chunks)
                    .all(|(o, &i)| o[..] == self.corpus[i][..]);
            if !same {
                return Err(Error::Format(format!(
                    "{} did not decompress to what it was given",
                    codec.label()
                )));
            }
            run += 1;
        }
        self.step += 1;
        let raw = kept
            .iter()
            .flat_map(|b| &b.chunks)
            .map(|&i| self.corpus[i].len() as u64)
            .sum();
        let packed = kept.iter().map(|b| b.packed.len()).sum();
        Ok(Some(Measurement {
            speeds: Speeds {
                compress,
                decompress: median(speeds),
                compress_runs,
                decompress_runs: run,
            },
            raw,
            packed,
        }))
    }
}

/// Runs the benchmark. `stop` is read between runs: once it is set the
/// report comes back with what was measured so far and `stopped` set.
/// `events` hears about each measurement before it starts and about each
/// row whenever it changes.
pub fn run(options: &Options, stop: &AtomicBool, events: &dyn Fn(Event<'_>)) -> Result<Report> {
    let modes = options.modes();
    let steps = options.codecs.len() * modes.len() * 2;
    events(Event::Progress(Progress {
        step: 0,
        steps,
        codec: None,
        threads: options.threads,
        phase: Phase::Generating,
    }));
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(options.threads)
        .build()
        .map_err(|e| Error::Format(format!("could not create the thread pool: {e}")))?;
    let corpus = pool.install(|| corpus::generate(options.kind, corpus::SEED, options.chunks()));
    drop(pool);
    let mut report = Report {
        options: options.clone(),
        rows: Vec::new(),
        stopped: false,
        native: codecs::missing(Codec::Zstd).is_none(),
    };
    let mut bench = Bench {
        options,
        corpus: &corpus,
        stop,
        events,
        step: 0,
        steps,
    };
    for &codec in &options.codecs {
        let threads = codec.threads(options.threads);
        if let Some(why) = codecs::missing(codec) {
            let row = Row {
                codec,
                threads,
                outcome: Outcome::Unavailable(why),
            };
            events(Event::Row(&row));
            report.rows.push(row);
            bench.step += modes.len() * 2;
            continue;
        }
        let mut row = Row {
            codec,
            threads,
            outcome: Outcome::Measured {
                ratio: 0.0,
                single: None,
                multi: None,
            },
        };
        let mut covered = 0;
        for &mode in &modes {
            let mode_threads = if mode == 1 { 1 } else { threads };
            let Some(m) = bench.measure(codec, mode_threads)? else {
                report.stopped = true;
                break;
            };
            if let Outcome::Measured {
                ratio,
                single,
                multi,
            } = &mut row.outcome
            {
                if m.raw > covered {
                    covered = m.raw;
                    *ratio = m.packed as f64 / m.raw as f64;
                }
                if mode == 1 {
                    *single = Some(m.speeds);
                } else {
                    *multi = Some(m.speeds);
                }
            }
            events(Event::Row(&row));
        }
        if covered > 0 {
            report.rows.push(row);
        }
        if report.stopped {
            break;
        }
    }
    Ok(report)
}
