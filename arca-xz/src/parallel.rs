//! Decoding the blocks of a single-stream file side by side.
//!
//! Each block goes to a worker as a stream of its own: the file's stream
//! header, the block as it is on disk, and an index and footer that record
//! the size the file's own index gives it. The ordinary decoder then makes
//! every check it makes on the whole file, the block's check and its sizes
//! against the index included, and the dictionary limit still holds.

use crate::{describe, DECODER_BUDGET_MIB, MEMORY_LIMIT_MIB};
use lzma_rust2::XzReader;
use std::collections::BTreeMap;
use std::io::{self, Read};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread;

/// More blocks than this and the file is read on one thread: the list alone
/// would take memory for nothing.
pub const MAX_BLOCKS: usize = 1 << 20;
const STEP: usize = 1 << 20;

#[derive(Clone, Copy)]
pub struct Block {
    pub unpadded: u64,
    pub size: u64,
}

impl Block {
    fn padded(self) -> u64 {
        (self.unpadded + 3) & !3
    }

    fn cost(self) -> u64 {
        self.padded()
            .saturating_add(self.size.saturating_mul(2))
            .saturating_add(1 << 20)
    }
}

/// Threads worth decoding `blocks` on, or 1 when the file should be read as
/// one stream: a single block, or blocks too large for more than one to fit
/// in [`DECODER_BUDGET_MIB`] beside the one being read and the one queued.
pub fn threads(blocks: &[Block], available: usize) -> usize {
    let Some(cost) = blocks.iter().map(|b| b.cost()).max() else {
        return 1;
    };
    let slots = (DECODER_BUDGET_MIB << 20) / cost;
    (slots.saturating_sub(2) as usize)
        .min(available)
        .min(blocks.len())
        .max(1)
}

fn varint(mut value: u64, out: &mut Vec<u8>) {
    while value >= 0x80 {
        out.push(value as u8 | 0x80);
        value >>= 7;
    }
    out.push(value as u8);
}

/// The index and footer of a stream that holds `block` alone.
fn trailer(block: Block, flags: [u8; 2]) -> Vec<u8> {
    let mut index = vec![0, 1];
    varint(block.unpadded, &mut index);
    varint(block.size, &mut index);
    while index.len() % 4 != 0 {
        index.push(0);
    }
    let crc = crc32fast::hash(&index);
    index.extend_from_slice(&crc.to_le_bytes());
    let backward = (index.len() as u32 / 4 - 1).to_le_bytes();
    let mut footer = [0u8; 12];
    footer[4..8].copy_from_slice(&backward);
    footer[8..10].copy_from_slice(&flags);
    let crc = crc32fast::hash(&footer[4..10]);
    footer[..4].copy_from_slice(&crc.to_le_bytes());
    footer[10..].copy_from_slice(b"YZ");
    index.extend_from_slice(&footer);
    index
}

struct Job {
    seq: u64,
    block: Block,
    bytes: Vec<u8>,
}

type Outcome = (u64, io::Result<Vec<u8>>);

fn decode(job: &Job, header: &[u8; 12], flags: [u8; 2], stop: &AtomicBool) -> io::Result<Vec<u8>> {
    let size = usize::try_from(job.block.size)
        .map_err(|_| io::Error::new(io::ErrorKind::OutOfMemory, "XZ block too large"))?;
    let trailer = trailer(job.block, flags);
    let stream = (&header[..]).chain(&job.bytes[..]).chain(&trailer[..]);
    let mut reader = XzReader::new_mem_limit(stream, false, MEMORY_LIMIT_MIB * 1024);
    let mut out = Vec::new();
    out.try_reserve_exact(size)
        .map_err(|_| io::Error::new(io::ErrorKind::OutOfMemory, "XZ block too large"))?;
    out.resize(size, 0);
    let mut filled = 0;
    loop {
        if stop.load(Ordering::Relaxed) {
            return Err(io::Error::other("cancelled"));
        }
        if filled == size {
            let mut probe = [0u8; 1];
            return match reader.read(&mut probe).map_err(describe)? {
                0 => Ok(out),
                _ => Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "corrupt XZ data: a block holds more than the index says",
                )),
            };
        }
        let end = size.min(filled + STEP);
        match reader.read(&mut out[filled..end]).map_err(describe)? {
            0 => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "corrupt XZ data: a block holds less than the index says",
                ))
            }
            n => filled += n,
        }
    }
}

/// The blocks of one stream, read in order from just past its header and
/// handed out in order, with at most one more block queued than there are
/// threads.
pub struct Parallel<R> {
    source: R,
    blocks: Vec<Block>,
    jobs: Option<mpsc::Sender<Job>>,
    results: mpsc::Receiver<Outcome>,
    ready: BTreeMap<u64, io::Result<Vec<u8>>>,
    dispatched: u64,
    returned: u64,
    window: u64,
    current: Vec<u8>,
    at: usize,
    failed: bool,
    stop: Arc<AtomicBool>,
    workers: Vec<thread::JoinHandle<()>>,
}

impl<R: Read> Parallel<R> {
    /// `source` must be positioned at the first block, just past the stream
    /// header `header`.
    pub fn new(
        source: R,
        header: [u8; 12],
        blocks: Vec<Block>,
        threads: usize,
    ) -> io::Result<Self> {
        let flags = [header[6], header[7]];
        let (jobs, queue) = mpsc::channel::<Job>();
        let queue = Arc::new(Mutex::new(queue));
        let (done, results) = mpsc::channel::<Outcome>();
        let stop = Arc::new(AtomicBool::new(false));
        let mut workers = Vec::with_capacity(threads);
        for _ in 0..threads {
            let queue = Arc::clone(&queue);
            let done = done.clone();
            let stop = Arc::clone(&stop);
            workers.push(
                thread::Builder::new()
                    .name("arca-xz".into())
                    .spawn(move || loop {
                        let next = match queue.lock() {
                            Ok(queue) => queue.recv(),
                            Err(_) => return,
                        };
                        let Ok(job) = next else { return };
                        let outcome = decode(&job, &header, flags, &stop);
                        if done.send((job.seq, outcome)).is_err() {
                            return;
                        }
                    })?,
            );
        }
        Ok(Self {
            source,
            blocks,
            jobs: Some(jobs),
            results,
            ready: BTreeMap::new(),
            dispatched: 0,
            returned: 0,
            window: threads as u64 + 1,
            current: Vec::new(),
            at: 0,
            failed: false,
            stop,
            workers,
        })
    }

    fn dispatch(&mut self) -> io::Result<()> {
        while self.dispatched < self.blocks.len() as u64
            && self.dispatched - self.returned < self.window
        {
            let block = self.blocks[self.dispatched as usize];
            let len = usize::try_from(block.padded())
                .map_err(|_| io::Error::new(io::ErrorKind::OutOfMemory, "XZ block too large"))?;
            let mut bytes = Vec::new();
            bytes
                .try_reserve_exact(len)
                .map_err(|_| io::Error::new(io::ErrorKind::OutOfMemory, "XZ block too large"))?;
            bytes.resize(len, 0);
            self.source.read_exact(&mut bytes).map_err(describe)?;
            let job = Job {
                seq: self.dispatched,
                block,
                bytes,
            };
            let sent = self
                .jobs
                .as_ref()
                .is_some_and(|jobs| jobs.send(job).is_ok());
            if !sent {
                return Err(io::Error::other("the XZ decoding threads stopped"));
            }
            self.dispatched += 1;
        }
        Ok(())
    }

    fn next_block(&mut self) -> io::Result<Vec<u8>> {
        loop {
            if let Some(outcome) = self.ready.remove(&self.returned) {
                self.returned += 1;
                return outcome;
            }
            let (seq, outcome) = self
                .results
                .recv()
                .map_err(|_| io::Error::other("the XZ decoding threads stopped"))?;
            self.ready.insert(seq, outcome);
        }
    }
}

impl<R: Read> Read for Parallel<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        loop {
            if self.at < self.current.len() {
                let n = buf.len().min(self.current.len() - self.at);
                buf[..n].copy_from_slice(&self.current[self.at..self.at + n]);
                self.at += n;
                return Ok(n);
            }
            if self.failed {
                return Err(io::Error::other("corrupt XZ data: an earlier block failed"));
            }
            if buf.is_empty() || self.returned == self.blocks.len() as u64 {
                return Ok(0);
            }
            self.current = Vec::new();
            self.at = 0;
            let next = self.dispatch().and_then(|()| self.next_block());
            self.failed = next.is_err();
            self.current = next?;
            self.failed = self.dispatch().is_err();
            if self.failed {
                self.current = Vec::new();
                return Err(io::Error::other(
                    "corrupt XZ data: the blocks could not be read",
                ));
            }
        }
    }
}

impl<R> Drop for Parallel<R> {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        self.jobs = None;
        for worker in self.workers.drain(..) {
            let _ = worker.join();
        }
    }
}
