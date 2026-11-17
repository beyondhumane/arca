# ZIP extraction benchmark

`extract.py` uses only Python's standard library. It times **external process
wall time**, including startup, and never reads the extractor's self-reported
time. It verifies every output path, size, SHA-256, and explicit empty directory
after every run. Hashing, verification, output removal, archive generation and
environment collection are outside the timer. Each extraction gets a fresh,
nonexistent output path. This measures warm-cache extraction, not durable writes
(`fsync`), cold-cache performance, compression, or physical disk traffic.

## Reproduce

Build the baseline before modifying extraction code and copy it outside `target`:

```sh
cargo build --release --locked -p arca-cli
```

Use the same compiler, features and release profile for each Arca revision. Copy
each binary to a distinct filename. Install official 7-Zip separately and record
its provenance. On Windows use an NTFS directory for both corpus and scratch.

```sh
python benchmarks/extract.py generate --corpus /absolute/path/corpus
python benchmarks/extract.py run --corpus /absolute/path/corpus \
  --variants /absolute/path/variants.json --results /absolute/path/results \
  --scratch /absolute/path/scratch --runs 6 --warmups 1 \
  --cases small-5000-flat small-5000-nested small-20000-flat \
  small-20000-nested small-50000-flat small-50000-nested large-8
```

On Windows paths like `C:/bench/corpus` work. Result directories must not already
exist. A variants file is a JSON array; commands are argument arrays, not shell
strings. `{archive}` and `{dest}` are replaced with absolute paths:

```json
[
  {"name":"baseline-1", "command":["C:/bench/arca-baseline.exe","extract","{archive}","-o","{dest}","-j","1"]},
  {"name":"candidate-1", "command":["C:/bench/arca-candidate.exe","extract","{archive}","-o","{dest}","-j","1"]},
  {"name":"7zip-1", "kind":"7zip", "command":["C:/Program Files/7-Zip/7z.exe","x","{archive}","-o{dest}","-y","-bd","-mmt1"]}
]
```

Repeat with explicit physical-core count, default (omit thread flags), and a
tuned count selected in an earlier screening run. Do not infer 7-Zip actually
uses the requested thread count for every ZIP method. Do not cap Arca globally
based on one machine's result. No other builds, extractors or installers should
run while measuring.

The default corpus has 5k, 20k and 50k files in both flat and nested layouts,
512..16384 bytes each, plus eight 16 MiB files. Payloads mix repeated source-like
text and SHAKE-256 bytes. Fixed timestamps, names, order, Deflate level and seed
make generation deterministic within the recorded Python/zlib version. Compare
archive hashes before treating different runs as the same corpus. These are
synthetic files, not a representative real-world source checkout.

A pinned source tree can be converted without unpacking its tarball (regular
files only, sorted paths, normalized timestamps/permissions):

```sh
curl -L --fail https://codeload.github.com/python/cpython/tar.gz/refs/tags/v3.13.0 -o cpython-v3.13.0.tar.gz
python benchmarks/extract.py source --tar cpython-v3.13.0.tar.gz \
  --sha256 d2c5dbd73111a80ee654489eb0f52b663b38ef9ce9e9323312515d941d0dfaa2 \
  --prefix cpython-3.13.0/Lib --name cpython-3.13.0-Lib --corpus /absolute/path/corpus
```

Then add `cpython-3.13.0-Lib` to `--cases`. This source snapshot is 2,262 regular
files / 42,692,366 bytes; it includes real library source and test fixtures,
not only small files. The supplied tarball hash, not the movable URL, pins it.

## Evidence and interpretation

- `samples.jsonl`: every warmup/measured command, return code, stdout/stderr,
  elapsed time, order and verification result; flushed after each run.
- `samples.csv`: compact raw samples.
- `summary.json`: median, min/max, IQR and median absolute deviation.
- `environment.json`: OS, CPU count, tools and binary hashes; Windows CIM data
  includes filesystem, disk model, RAM and current Defender state. Failures to
  query a field are retained, never silently replaced with guesses.
- Per-case manifests: archive SHA-256, byte/file totals and all expected hashes.

Forward/reverse run pairs balance relative order; pair rotation reduces fixed
first-position bias. Use an even count (six by default). All tools get the same
archive bytes and verification warms filesystem caches between runs. This is
not a randomized or cold-cache experiment. Report VM-specific absolute times
and dispersion, not universal product claims or an inferred NTFS percentage.
Default extraction semantics differ: 7-Zip may restore timestamps/metadata that
Arca currently does not. The benchmark compares normal behaviors, not identical
metadata work. Never disable antivirus or other security controls to improve
results; report their observed state and this limitation.

No generated corpora, binaries or raw machine-specific results belong in git.
Attach those to the experiment report along with exact commands and revisions.

# Comparison with other archivers on Windows

`compare.ps1` measures Arca against WinRAR, 7-Zip, NanaZip and xz in a single
unattended command. It builds the corpora, runs every row, checks every output,
and writes tables ready for `docs/guide/benchmarks.md`. Run it once on a quiet
machine: close builds, browsers, games and sync clients, and leave the machine
alone until the script finishes.

```powershell
cargo build --release -p arca-cli
powershell -NoProfile -ExecutionPolicy Bypass -File benchmarks\compare.ps1
```

## What it needs

- Windows PowerShell 5.1 or later. Admin rights and downloads are not needed.
- This repository. The script builds the corpus generators with cargo, runs
  `git archive` on a pinned commit, and uses Windows' own `tar.exe`.
- Arca at `target\release\arca.exe`, or another build given with `-Arca`.
- Optional tools. Each one is found where it is usually installed, or can be
  given with `-WinRar`, `-Rar`, `-SevenZip`, `-NanaZip` or `-Xz`:
  - WinRAR: `C:\Program Files\WinRAR\WinRAR.exe` for ZIP and `Rar.exe` for RAR.
  - 7-Zip: `7z.exe` in Program Files or on PATH.
  - NanaZip: the `NanaZipC.exe` app alias.
  - xz from Git for Windows: `mingw64\bin\xz.exe`.

  A tool that is not installed is skipped with a note in the log and in the
  report. The run still succeeds.
- About 2 GB free in `%TEMP%`, or in the folder given with `-Work`.

## How long it takes

Budget about an hour. That is an estimate: no full run has been timed end to
end. Building the corpora comes first and takes a few minutes; they are kept
if the run stops. Most of the measuring time goes to LZMA at `best`, which
Arca's 7z runs on one thread, and which a .tar.xz of this size also gets on
one thread, from Arca and from xz alike. 7-Zip and NanaZip add their own rows
to ZIP, 7z and .tar.xz, and `-SingleThread` adds a second pass at one thread.
The log prints a rough estimate of the time left after every group.

## What it measures

The script builds five corpora once, with fixed seeds, and records the file
count, the bytes and a SHA-256 of each corpus's file list:

| Corpus | Contents |
| --- | --- |
| `small` | 50,000 files of 0 to 16 KiB, from `small_files` |
| `source` | This repository at a pinned commit, from `git archive` |
| `text` | 4 prose-like text files of up to 8 MiB each |
| `mixed` | The `exe` and `media` kinds of `rar-corpus` |
| `compressed` | Random bytes from `rar-corpus`, standing in for data that is already compressed |

Each format is measured at three levels:

| Format | fast | normal | best |
| --- | --- | --- | --- |
| ZIP | Arca `-l 1`, 7-Zip and NanaZip `-mx1`, WinRAR `-afzip -m1` | `-l 6`, `-mx5`, `-m3` | `-l 9`, `-mx9`, `-m5` |
| 7z (LZMA2) | Arca `-l 1`, 7-Zip and NanaZip `-mx1` | `-l 6`, `-mx5` | `-l 9`, `-mx9` |
| .tar.xz | Arca `-l 1`, xz `-1`, 7-Zip and NanaZip `-txz -mx1` | `-l 6`, `-6`, `-mx5` | `-l 9`, `-9`, `-mx9` |
| RAR | Arca `-l 1`, `Rar.exe -m1` | `-l 6`, `-m3` | `-l 9`, `-m5` |

Every tool gets the same thread count: all logical processors, through `-j`,
`-mmt`, `-mt` or `-T`. Arca's 7z and RAR writers ignore `-j`, and the report
marks those rows.

Not every format runs on every corpus, to keep the run near an hour:

- ZIP runs on all five corpora.
- 7z runs on `source`, `text` and `mixed`.
- .tar.xz runs on `source` and `text`.
- RAR runs on every corpus except `small`.

Pass `-Corpora` to choose the corpora yourself.

Each tool's normal-level archive is extracted and tested by every tool that
reads the format. Arca's archive is always included. So is one third-party
archive per format, from 7-Zip, NanaZip or WinRAR for ZIP, from 7-Zip for 7z,
from xz for .tar.xz, and from `Rar.exe` for RAR.

Arca writes the TAR itself when it creates a .tar.xz. xz and 7-Zip instead
compress a TAR made beforehand, untimed. When extracting a .tar.xz, only Arca
writes the files; the other tools write the `.tar`.

## How the runs are done

- **Order and repeats.** The tools in a group run interleaved, A B C then
  C B A, `-Runs` times (3 by default).
- **What each run records.** The wall time, including process start. The CPU
  time, user plus kernel, of the whole process tree, measured through a job
  object. The peak working set. The exact output size.
- **Before and between runs.** Every corpus is read in full before the first
  run, so the cache is warm. Outputs are deleted between runs, outside the
  timed part.
- **Checks.** Every archive is tested by its own tool and by a second one:
  7-Zip, NanaZip, WinRAR or xz for Arca's archives, and Arca for the others'.
  `arca diff` also compares every archive with the corpus. Every extraction is
  compared with the corpus.
- **Failures.** A failed run or check marks its row as failed, and the script
  goes on.
- **A quiet machine.** At the start, the processor load is sampled for 10 s.
  If the average is over 15%, the script names the busiest processes and
  stops. The check runs again after the corpora are built; that time the
  script waits up to five minutes for the load to settle, because the
  antivirus scans the new files, and then stops if it is still over 15%.
  `-Force` skips both checks.
- **Interruptions.** Run the same command again to resume. Rows already in
  `results.csv` are skipped, unless their command, tool binary or corpus has
  changed.
- **Cleanup.** The script writes nothing outside its work folder, apart from
  the corpus generators in `target`. When it finishes, it deletes its
  archives, its extractions and, unless you pass `-KeepCorpora`, the corpora.

## Results

The script writes these files to `<work>\results`:

- `samples.csv`: every run.
- `results.csv`: one line per row, with the median and range of each measure,
  the verification and the exact command.
- `results.json`: the rows, plus the machine, the tools and the corpora.
- `results.md`: one section per format, with tables for creating, extracting
  and testing.
- `compare.log`: the log of the run.

The report records the CPU, the cores, the RAM, the OS and the disk model
(from `Get-PhysicalDisk`). It also records whether Defender's real-time
protection was on, the power plan, the Arca commit, and every tool's version
and SHA-256.

In `results.md`, the commands use `<work>` in place of the absolute path, so
the user name does not end up in the docs.

## Dry run

`-Quick` runs on tiny corpora, once per tool, in its own folder
(`%TEMP%\arca-compare-quick`). It takes a few minutes and proves that every
row works on the machine. It does not stop on a busy machine. Never publish
its numbers.

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File benchmarks\compare.ps1 -Quick
```
