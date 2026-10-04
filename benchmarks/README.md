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
