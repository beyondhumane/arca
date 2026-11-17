// Arca: rewritten to keep memory bounded. The published reader gathered each
// run of chunks between dictionary resets into memory however long it was,
// decoded it into a buffer that grew with the output, could keep reading
// ahead while an early run was still decoding, and reported a worker's error
// before the runs ahead of it were returned. This one gathers at most
// `run_max` bytes of a run, compressed or decoded, keeps at most one run
// queued beyond its threads, returns runs and errors strictly in order, and
// decodes the rest of the stream on the calling thread, streaming, from the
// first run that is longer than `run_max`.

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
    /// The stream ended, or the source did.
    End,
    /// The run went past `run_max`; what was read of it stays in `run`.
    TooLong,
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
    too_long: bool,
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
            too_long: false,
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

    fn gather(&mut self) -> io::Result<Gather> {
        loop {
            let Source::Scanning(inner) = &mut self.source else {
                return Ok(Gather::End);
            };
            let control = match self.pending_control.take() {
                Some(control) => control,
                None => {
                    let mut byte = [0u8; 1];
                    match inner.read_exact(&mut byte) {
                        Ok(()) => byte[0],
                        Err(error) if error.kind() == io::ErrorKind::UnexpectedEof => {
                            self.ended = true;
                            if self.run.is_empty() {
                                return Err(error);
                            }
                            return Ok(self.take_run());
                        }
                        Err(error) => return Err(error),
                    }
                }
            };
            if control == 0x00 {
                self.ended = true;
                if self.run.is_empty() {
                    return Ok(Gather::End);
                }
                self.run.push(0x00);
                return Ok(self.take_run());
            }
            let independent = control >= 0xE0 || control == 0x01;
            if independent && !self.run.is_empty() {
                self.pending_control = Some(control);
                self.run.push(0x00);
                return Ok(self.take_run());
            }
            let uncompressed = control == 0x01 || control == 0x02;
            let header_len = match control {
                0xC0..=0xFF => 5,
                0x80..=0xBF => 4,
                0x01 | 0x02 => 2,
                _ => {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        format!("invalid LZMA2 control byte: {control:X}"),
                    ));
                }
            };
            let mut header = [0u8; 5];
            inner.read_exact(&mut header[..header_len])?;
            let first = u16::from_be_bytes([header[0], header[1]]) as usize + 1;
            let (packed, size) = if uncompressed {
                (first, first)
            } else {
                (
                    u16::from_be_bytes([header[2], header[3]]) as usize + 1,
                    (((control & 0x1F) as usize) << 16) + first,
                )
            };
            self.run.push(control);
            self.run.extend_from_slice(&header[..header_len]);
            let start = self.run.len();
            self.run.resize(start + packed, 0);
            inner.read_exact(&mut self.run[start..])?;
            self.run_size += size;
            if self.run.len() > self.run_max || self.run_size > self.run_max {
                return Ok(Gather::TooLong);
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

    fn dispatch(&mut self) -> io::Result<()> {
        while !self.ended
            && !self.too_long
            && self.dispatched - self.returned <= self.max_workers as u64
        {
            match self.gather()? {
                Gather::Run(bytes, size) => {
                    if self.workers.len() < self.max_workers
                        && self.workers.len() as u64 <= self.dispatched - self.returned
                    {
                        self.spawn_worker()?;
                    }
                    let job = Job {
                        seq: self.dispatched,
                        bytes,
                        size,
                    };
                    let sent = self.jobs.as_ref().is_some_and(|jobs| jobs.send(job).is_ok());
                    if !sent {
                        return Err(io::Error::other("the LZMA2 decoding threads stopped"));
                    }
                    self.dispatched += 1;
                }
                Gather::End => {}
                Gather::TooLong => self.too_long = true,
            }
        }
        Ok(())
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
        self.dispatch()?;
        if self.returned < self.dispatched {
            self.current = self.next_run()?;
            self.at = 0;
            self.dispatch()?;
            return Ok(true);
        }
        if self.too_long {
            self.go_inline();
            return Ok(true);
        }
        Ok(false)
    }
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
}
