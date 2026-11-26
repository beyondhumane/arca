// Arca: rewritten to keep memory bounded. The published reader gathered each
// run of chunks between dictionary resets into memory however long it was,
// decoded it into a buffer that grew with the output, could keep reading
// ahead while an early run was still decoding, and reported a worker's error
// before the runs ahead of it were returned. This one gathers at most
// `run_max` bytes of a run, compressed or decoded, keeps at most one run
// queued beyond its threads, returns runs and errors strictly in order, and
// decodes the rest of the stream on the calling thread, streaming, from the
// first run that is longer than `run_max`.
//
// Arca: a stream that breaks off or goes bad while a run is being gathered
// (a source that ends, an invalid control byte) is also decoded on the
// calling thread from the start of that run, once every run before it has
// been returned, so it gives out what a single-threaded reader would before
// the same error. A run already decoded is never dropped because reading
// ahead failed; the failure is returned after it.

use std::{
    collections::BTreeMap,
    io::{self, Chain, Cursor, Read},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread,
};

use crate::Lzma2Reader;

/// How much one worker decodes between looks at the stop flag.
const STEP: usize = 1 << 20;

struct Job {
    seq: u64,
    bytes: Vec<u8>,
    size: usize,
}

type Outcome = (u64, io::Result<Vec<u8>>);

enum Gather {
    /// A whole run, ended with an end marker of its own.
    Run(Vec<u8>, usize),
    /// The stream ended.
    End,
    /// Arca: the run went past `run_max`, or the source ended or went bad
    /// inside it; what was read of it stays in `run`, to be decoded inline.
    Inline,
}

enum Source<R: Read> {
    Scanning(R),
    Inline(Box<Lzma2Reader<Chain<Cursor<Vec<u8>>, R>>>),
    Gone,
}

/// A multi-threaded LZMA2 decompressor.
///
/// A stream that resets its dictionary now and then, as a multi-threaded
/// encoder's does, falls into runs that decode independently; each one goes
/// to a worker thread. Memory stays within about `(threads + 2) * 3 *
/// run_max` plus one dictionary per thread.
pub struct Lzma2ReaderMt<R: Read> {
    source: Source<R>,
    dict_size: u32,
    preset_dict: Option<Arc<Vec<u8>>>,
    run_max: usize,
    pending_control: Option<u8>,
    run: Vec<u8>,
    run_size: usize,
    ended: bool,
    inline: bool,
    halt: Option<io::Error>,
    jobs: Option<mpsc::Sender<Job>>,
    queue: Arc<Mutex<mpsc::Receiver<Job>>>,
    done: mpsc::Sender<Outcome>,
    results: mpsc::Receiver<Outcome>,
    ready: BTreeMap<u64, io::Result<Vec<u8>>>,
    dispatched: u64,
    returned: u64,
    max_workers: usize,
    current: Vec<u8>,
    at: usize,
    failed: bool,
    stop: Arc<AtomicBool>,
    workers: Vec<thread::JoinHandle<()>>,
}

impl<R: Read> Lzma2ReaderMt<R> {
    /// Creates a new multi-threaded LZMA2 reader.
    ///
    /// - `inner`: The reader to read compressed data from.
    /// - `dict_size`: The dictionary size in bytes, as specified in the stream properties.
    /// - `preset_dict`: An optional preset dictionary.
    /// - `num_workers`: The maximum number of worker threads for decompression. Currently capped at 256 Threads.
    ///
    /// Runs longer than four dictionaries (at least 1 MiB) are decoded on the
    /// calling thread; see [`Self::new_bounded`].
    pub fn new(inner: R, dict_size: u32, preset_dict: Option<&[u8]>, num_workers: u32) -> Self {
        let run_max = (dict_size as usize).saturating_mul(4).max(1 << 20);
        Self::new_bounded(inner, dict_size, preset_dict, num_workers, run_max)
    }

    /// Like [`Self::new`], with the longest run, compressed or decoded, that
    /// goes to a worker thread. From the first run longer than this on, the
    /// stream is decoded on the calling thread as it is read.
    pub fn new_bounded(
        inner: R,
        dict_size: u32,
        preset_dict: Option<&[u8]>,
        num_workers: u32,
        run_max: usize,
    ) -> Self {
        let (jobs, queue) = mpsc::channel::<Job>();
        let (done, results) = mpsc::channel::<Outcome>();
        Self {
            source: Source::Scanning(inner),
            dict_size,
            preset_dict: preset_dict.map(|s| Arc::new(s.to_vec())),
            run_max: run_max.max(1),
            pending_control: None,
            run: Vec::new(),
            run_size: 0,
            ended: false,
            inline: false,
            halt: None,
            jobs: Some(jobs),
            queue: Arc::new(Mutex::new(queue)),
            done,
            results,
            ready: BTreeMap::new(),
            dispatched: 0,
            returned: 0,
            max_workers: num_workers.clamp(1, 256) as usize,
            current: Vec::new(),
            at: 0,
            failed: false,
            stop: Arc::new(AtomicBool::new(false)),
            workers: Vec::new(),
        }
    }

    /// The count of independent runs returned so far.
    pub fn chunk_count(&self) -> u64 {
        self.returned
    }

    fn take_run(&mut self) -> Gather {
        let size = core::mem::take(&mut self.run_size);
        Gather::Run(core::mem::take(&mut self.run), size)
    }

    fn gather(&mut self) -> Gather {
        loop {
            let Source::Scanning(inner) = &mut self.source else {
                return Gather::End;
            };
            let control = match self.pending_control.take() {
                Some(control) => control,
                None => {
                    let mut byte = [0u8; 1];
                    if read_up_to(inner, &mut byte) == 0 {
                        return Gather::Inline;
                    }
                    byte[0]
                }
            };
            if control == 0x00 {
                self.ended = true;
                if self.run.is_empty() {
                    return Gather::End;
                }
                self.run.push(0x00);
                return self.take_run();
            }
            let independent = control >= 0xE0 || control == 0x01;
            if independent && !self.run.is_empty() {
                self.pending_control = Some(control);
                self.run.push(0x00);
                return self.take_run();
            }
            let uncompressed = control == 0x01 || control == 0x02;
            self.run.push(control);
            let header_len = match control {
                0xC0..=0xFF => 5,
                0x80..=0xBF => 4,
                0x01 | 0x02 => 2,
                _ => return Gather::Inline,
            };
            let at = self.run.len();
            if !read_onto(inner, &mut self.run, header_len) {
                return Gather::Inline;
            }
            let header = &self.run[at..];
            let first = u16::from_be_bytes([header[0], header[1]]) as usize + 1;
            let (packed, size) = if uncompressed {
                (first, first)
            } else {
                (
                    u16::from_be_bytes([header[2], header[3]]) as usize + 1,
                    (((control & 0x1F) as usize) << 16) + first,
                )
            };
            if !read_onto(inner, &mut self.run, packed) {
                return Gather::Inline;
            }
            self.run_size += size;
            if self.run.len() > self.run_max || self.run_size > self.run_max {
                return Gather::Inline;
            }
        }
    }

    fn spawn_worker(&mut self) -> io::Result<()> {
        let queue = Arc::clone(&self.queue);
        let done = self.done.clone();
        let stop = Arc::clone(&self.stop);
        let dict_size = self.dict_size;
        let preset_dict = self.preset_dict.clone();
        let handle = thread::Builder::new()
            .name("lzma2-decoder".into())
            .spawn(move || {
                loop {
                    let next = match queue.lock() {
                        Ok(queue) => queue.recv(),
                        Err(_) => return,
                    };
                    let Ok(job) = next else { return };
                    let outcome = decode_run(&job, dict_size, preset_dict.as_deref(), &stop);
                    if done.send((job.seq, outcome)).is_err() {
                        return;
                    }
                }
            })?;
        self.workers.push(handle);
        Ok(())
    }

    /// Arca: a failure here is kept in `halt`, to be returned once every run
    /// dispatched before it has been.
    fn dispatch(&mut self) {
        while self.halt.is_none()
            && !self.ended
            && !self.inline
            && self.dispatched - self.returned <= self.max_workers as u64
        {
            match self.gather() {
                Gather::Run(bytes, size) => {
                    if self.workers.len() < self.max_workers
                        && self.workers.len() as u64 <= self.dispatched - self.returned
                    {
                        if let Err(error) = self.spawn_worker() {
                            if self.workers.is_empty() {
                                self.halt = Some(error);
                                return;
                            }
                        }
                    }
                    let job = Job {
                        seq: self.dispatched,
                        bytes,
                        size,
                    };
                    let sent = self
                        .jobs
                        .as_ref()
                        .is_some_and(|jobs| jobs.send(job).is_ok());
                    if !sent {
                        self.halt = Some(io::Error::other("the LZMA2 decoding threads stopped"));
                        return;
                    }
                    self.dispatched += 1;
                }
                Gather::End => {}
                Gather::Inline => self.inline = true,
            }
        }
    }

    fn next_run(&mut self) -> io::Result<Vec<u8>> {
        loop {
            if let Some(outcome) = self.ready.remove(&self.returned) {
                self.returned += 1;
                return outcome;
            }
            let (seq, outcome) = self
                .results
                .recv()
                .map_err(|_| io::Error::other("the LZMA2 decoding threads stopped"))?;
            self.ready.insert(seq, outcome);
        }
    }

    fn go_inline(&mut self) {
        if let Source::Scanning(inner) = core::mem::replace(&mut self.source, Source::Gone) {
            let run = Cursor::new(core::mem::take(&mut self.run));
            self.source = Source::Inline(Box::new(Lzma2Reader::new(
                run.chain(inner),
                self.dict_size,
                self.preset_dict.as_deref().map(|v| v.as_slice()),
            )));
        }
    }

    fn fill(&mut self) -> io::Result<bool> {
        self.dispatch();
        if self.returned < self.dispatched {
            self.current = self.next_run()?;
            self.at = 0;
            self.dispatch();
            return Ok(true);
        }
        if let Some(error) = self.halt.take() {
            return Err(error);
        }
        if self.inline {
            self.go_inline();
            return Ok(true);
        }
        Ok(false)
    }
}

/// Arca: reads into `buf` until it is full or `inner` ends or fails, and
/// says how much it read.
fn read_up_to<R: Read>(inner: &mut R, buf: &mut [u8]) -> usize {
    let mut got = 0;
    while got < buf.len() {
        match inner.read(&mut buf[got..]) {
            Ok(0) => break,
            Ok(n) => got += n,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            Err(_) => break,
        }
    }
    got
}

/// Arca: appends `len` bytes of `inner` to `run`, or as many as there are
/// before it ends or fails, and says whether it got them all.
fn read_onto<R: Read>(inner: &mut R, run: &mut Vec<u8>, len: usize) -> bool {
    let start = run.len();
    run.resize(start + len, 0);
    let got = read_up_to(inner, &mut run[start..]);
    run.truncate(start + got);
    got == len
}

fn decode_run(
    job: &Job,
    dict_size: u32,
    preset_dict: Option<&Vec<u8>>,
    stop: &AtomicBool,
) -> io::Result<Vec<u8>> {
    let mut reader = Lzma2Reader::new(
        job.bytes.as_slice(),
        dict_size,
        preset_dict.map(|v| v.as_slice()),
    );
    let mut out = Vec::new();
    out.try_reserve_exact(job.size)
        .map_err(|_| io::Error::new(io::ErrorKind::OutOfMemory, "LZMA2 run too large"))?;
    out.resize(job.size, 0);
    let mut filled = 0;
    while filled < job.size {
        if stop.load(Ordering::Relaxed) {
            return Err(io::Error::other("cancelled"));
        }
        let end = job.size.min(filled + STEP);
        match reader.read(&mut out[filled..end])? {
            0 => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "LZMA2 run ended before its chunks said it would",
                ));
            }
            n => filled += n,
        }
    }
    let mut probe = [0u8; 1];
    match reader.read(&mut probe)? {
        0 => Ok(out),
        _ => Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "LZMA2 run is longer than its chunks say",
        )),
    }
}

impl<R: Read> Read for Lzma2ReaderMt<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        loop {
            if self.at < self.current.len() {
                let n = buf.len().min(self.current.len() - self.at);
                buf[..n].copy_from_slice(&self.current[self.at..self.at + n]);
                self.at += n;
                return Ok(n);
            }
            if let Source::Inline(reader) = &mut self.source {
                return reader.read(buf);
            }
            if self.failed {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "an earlier LZMA2 run failed",
                ));
            }
            if buf.is_empty() {
                return Ok(0);
            }
            self.current = Vec::new();
            self.at = 0;
            match self.fill() {
                Ok(true) => {}
                Ok(false) => return Ok(0),
                Err(error) => {
                    self.failed = true;
                    self.current = Vec::new();
                    return Err(error);
                }
            }
        }
    }
}

impl<R: Read> Drop for Lzma2ReaderMt<R> {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        self.jobs = None;
        for worker in self.workers.drain(..) {
            let _ = worker.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Lzma2Options, Lzma2Writer, Lzma2WriterMt};
    use std::io::Write;

    fn sample(len: usize) -> Vec<u8> {
        let mut state = 0x2545_F491u32;
        (0..len)
            .map(|i| {
                state = state.wrapping_mul(1_103_515_245).wrapping_add(12_345);
                if i % 4 == 0 {
                    (state >> 24) as u8
                } else {
                    b"lzma2 runs "[i % 11]
                }
            })
            .collect()
    }

    fn runs(data: &[u8], chunk: u64) -> Vec<u8> {
        let mut options = Lzma2Options::with_preset(1);
        options.lzma_options.dict_size = 64 << 10;
        options.set_chunk_size(std::num::NonZeroU64::new(chunk));
        let mut w = Lzma2WriterMt::new(Vec::new(), options, 3).unwrap();
        w.write_all(data).unwrap();
        w.finish().unwrap()
    }

    fn decode(packed: &[u8], threads: u32, run_max: usize) -> io::Result<Vec<u8>> {
        let mut out = Vec::new();
        Lzma2ReaderMt::new_bounded(packed, 64 << 10, None, threads, run_max)
            .read_to_end(&mut out)?;
        Ok(out)
    }

    #[test]
    fn runs_decode_in_order_on_threads_and_inline() {
        let data = sample(512 << 10);
        let packed = runs(&data, 64 << 10);
        let mut reader = Lzma2ReaderMt::new_bounded(&packed[..], 64 << 10, None, 4, 1 << 20);
        let mut out = Vec::new();
        reader.read_to_end(&mut out).unwrap();
        assert!(out == data);
        assert!(reader.chunk_count() >= 8);
        for run_max in [1, 1000, 100 << 10] {
            assert!(decode(&packed, 4, run_max).unwrap() == data, "{run_max}");
        }
        let mut single = Vec::new();
        let mut options = Lzma2Options::with_preset(1);
        options.lzma_options.dict_size = 64 << 10;
        let mut w = Lzma2Writer::new(&mut single, options);
        w.write_all(&data).unwrap();
        w.finish().unwrap();
        assert!(decode(&single, 4, 64 << 10).unwrap() == data);
        assert!(decode(&[0x00], 4, 1 << 20).unwrap().is_empty());
    }

    #[test]
    fn truncated_and_damaged_streams_fail_without_panicking() {
        let data = sample(48 << 10);
        let packed = runs(&data, 8 << 10);
        for cut in (0..packed.len()).step_by(53) {
            for run_max in [1 << 20, 10 << 10] {
                assert!(decode(&packed[..cut], 4, run_max).is_err(), "cut at {cut}");
            }
        }
        for at in (0..packed.len()).step_by(37) {
            let mut bad = packed.clone();
            bad[at] ^= 0x24;
            for run_max in [1 << 20, 10 << 10] {
                if let Ok(out) = decode(&bad, 4, run_max) {
                    assert!(out.len() == data.len(), "byte {at} changed the length");
                }
            }
        }
        assert!(decode(&[0x03, 0, 0], 2, 1 << 20).is_err());
        assert!(decode(&[0xE0, 0xFF], 2, 1 << 20).is_err());
    }

    #[test]
    fn a_failed_run_is_reported_after_the_runs_before_it() {
        let data = sample(128 << 10);
        let packed = runs(&data, 16 << 10);
        let mut bad = packed.clone();
        let at = packed.len() * 3 / 4;
        bad[at] ^= 0xFF;
        bad[at + 1] ^= 0xFF;
        let mut reader = Lzma2ReaderMt::new_bounded(&bad[..], 64 << 10, None, 4, 1 << 20);
        let mut out = Vec::new();
        let mut buf = [0u8; 4096];
        loop {
            match reader.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => out.extend_from_slice(&buf[..n]),
                Err(_) => {
                    assert!(reader.read(&mut buf).is_err());
                    break;
                }
            }
        }
        assert!(out.len() >= 32 << 10);
        assert!(out[..] == data[..out.len()] || out.len() == data.len());
    }

    fn until_error<R: Read>(mut reader: R) -> (Vec<u8>, bool) {
        let mut out = Vec::new();
        let mut buf = [0u8; 4096];
        loop {
            match reader.read(&mut buf) {
                Ok(0) => return (out, false),
                Ok(n) => out.extend_from_slice(&buf[..n]),
                Err(_) => return (out, true),
            }
        }
    }

    fn chunk_starts(packed: &[u8]) -> Vec<usize> {
        let mut starts = Vec::new();
        let mut at = 0;
        while packed[at] != 0x00 {
            starts.push(at);
            let c = packed[at];
            let first = u16::from_be_bytes([packed[at + 1], packed[at + 2]]) as usize + 1;
            at += match c {
                0x01 | 0x02 => 3 + first,
                _ => {
                    let packed_len = u16::from_be_bytes([packed[at + 3], packed[at + 4]]) as usize;
                    (if c >= 0xC0 { 6 } else { 5 }) + packed_len + 1
                }
            };
        }
        starts.push(at);
        starts
    }

    #[test]
    fn a_stream_that_breaks_off_gives_out_what_a_single_thread_does() {
        let data = sample(512 << 10);
        let packed = runs(&data, 16 << 10);
        let starts = chunk_starts(&packed);
        assert!(starts.len() >= 8, "{}", starts.len());
        let mut cuts: Vec<usize> = [50, 70, 90, 99]
            .iter()
            .map(|p| packed.len() * p / 100)
            .collect();
        cuts.extend(starts.iter().flat_map(|&s| [s, s + 1, s + 3]));
        cuts.retain(|&cut| cut < packed.len());
        let bad_controls = starts[1..starts.len() - 1].iter().step_by(5);
        for run_max in [1 << 20, 10 << 10] {
            for &cut in &cuts {
                let single = until_error(Lzma2Reader::new(&packed[..cut], 64 << 10, None));
                let multi = until_error(Lzma2ReaderMt::new_bounded(
                    &packed[..cut],
                    64 << 10,
                    None,
                    4,
                    run_max,
                ));
                assert!(single.1 && multi.1, "cut at {cut} did not fail");
                assert!(data.starts_with(&multi.0), "cut at {cut}");
                assert!(
                    multi.0.len() >= single.0.len(),
                    "cut at {cut}: {} < {}",
                    multi.0.len(),
                    single.0.len()
                );
            }
            for &at in bad_controls.clone() {
                let mut bad = packed.clone();
                bad[at] = 0x03;
                let single = until_error(Lzma2Reader::new(&bad[..], 64 << 10, None));
                let multi = until_error(Lzma2ReaderMt::new_bounded(
                    &bad[..],
                    64 << 10,
                    None,
                    4,
                    run_max,
                ));
                assert!(single.1 && multi.1, "control at {at}");
                assert!(multi.0 == single.0, "control at {at}");
            }
        }
    }
}
