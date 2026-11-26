use arca_zip::{extract_entry_with, ExtractionReader, ZipArchive};
use std::cell::Cell;
use std::fs::File;
use std::io::{self, BufReader, Read, Seek, SeekFrom};
use std::path::Path;
use std::rc::Rc;

#[derive(Default)]
struct Counts {
    opens: Cell<u64>,
    reads: Cell<u64>,
    seeks: Cell<u64>,
    bytes: Cell<u64>,
}

struct Counted {
    file: File,
    counts: Rc<Counts>,
}

impl Counted {
    fn open(path: &Path, counts: &Rc<Counts>) -> io::Result<Self> {
        let file = File::open(path)?;
        counts.opens.set(counts.opens.get() + 1);
        Ok(Self {
            file,
            counts: counts.clone(),
        })
    }
}

impl Read for Counted {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.counts.reads.set(self.counts.reads.get() + 1);
        let n = self.file.read(buf)?;
        self.counts.bytes.set(self.counts.bytes.get() + n as u64);
        Ok(n)
    }
}

impl Seek for Counted {
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        self.counts.seeks.set(self.counts.seeks.get() + 1);
        self.file.seek(pos)
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 3 {
        return Err("usage: read_amplification ARCHIVE baseline|reuse|buffered|both".into());
    }
    let path = Path::new(&args[1]);
    let mode = args[2].as_str();
    let counts = Rc::new(Counts::default());
    let archive = ZipArchive::open(Counted::open(path, &counts)?)?;
    let mut reused: Option<Box<dyn Source>> = None;
    for entry in archive.entries().iter().filter(|entry| !entry.is_dir) {
        if reused.is_none() || matches!(mode, "baseline" | "buffered") {
            let file = Counted::open(path, &counts)?;
            reused = Some(match mode {
                "baseline" | "reuse" => Box::new(BufReader::with_capacity(256 * 1024, file)),
                "buffered" | "both" => Box::new(ExtractionReader::with_capacity(256 * 1024, file)?),
                _ => return Err("unknown mode".into()),
            });
        }
        extract_entry_with(reused.as_mut().unwrap(), entry, &mut io::sink(), None)?;
    }
    println!(
        "{{\"mode\":\"{mode}\",\"opens\":{},\"reads\":{},\"seeks\":{},\"bytes\":{}}}",
        counts.opens.get(),
        counts.reads.get(),
        counts.seeks.get(),
        counts.bytes.get()
    );
    Ok(())
}

trait Source: Read + Seek {}
impl<T: Read + Seek> Source for T {}
