# Design rationale

Arca has two goals, in this order: **be the fastest archiver end to end** (from the click to the finished archive) and make sure **a malicious archive cannot corrupt the program's memory**.

## The rule behind the decisions

This was measured before the project started. A codec rewritten from scratch in Rust loses between 3× and 12× against its C equivalent; zlib-rs, which is mature, **beats** C zlib. The conclusion is not "Rust is slow" but that **the mature implementation wins, whatever its language**.

That sets the project's boundary:

- **Code that touches bytes of unknown origin is safe Rust.** Container parsers process an archive that arrived by email. That is where 7-Zip has accumulated CVEs for 25 years, and that is where Arca's value lies.
- **Code that only does heavy maths may be C.** A codec processes data the parser has already validated. Insisting on pure Rust there costs an order of magnitude in performance in exchange for safety in the part that almost never fails.

Codec choice per algorithm:

| Algorithm | Implementation | Why |
|---|---|---|
| DEFLATE | `zlib-rs` via `flate2`; `libdeflater` (C) for whole in-memory blocks | `zlib-rs` is Rust and **faster than C zlib**; `libdeflater` is faster still and produces smaller output |
| Zstandard | `libzstd` (C) | No mature Rust encoder exists |
| LZMA2 | `liblzma` (C), pending | Same reason |
| LZ4 | `lz4_flex`, pending | Rust is competitive |

The C codecs sit behind the `codecs-native` feature, so `cargo build --release --no-default-features` produces a pure-Rust build.

All header parsing goes through `arca_core::Cursor`, which checks bounds. Slices are never indexed directly: a truncated header returns `Error::Format`, never a panic or an out-of-bounds read.

## Performance requirements

These are acceptance criteria, not aspirations. Measured results, the machine behind them and the commands that reproduce them live in [`docs/guide/benchmarks.md`](../guide/benchmarks.md); `bench.sh` reproduces R1–R3 and the compression comparison.

| | Requirement | Target |
|---|---|---|
| R1 | Cold start | < 15 ms |
| R2 | List without decompressing | < 200 ms |
| R3 | Multithreaded scaling | ≥ 0.8 × N |
| R4 | Saturate the disk in fast mode | ±20 % of the medium |
| R5 | No UI frame > 16 ms | — |
| R6 | Bounded peak memory | < 2 × dictionary/thread |

R2 holds because the reader **does not load the whole archive**: it reads the tail to locate the EOCD and then only the central directory. R6 relies on `IN_FLIGHT_PER_THREAD` in `arca-zip`, which caps data in flight at 32 MB per thread.

## Zstandard in ZIP

Zstandard is ZIP method 93. It is registered in the specification, but classic `unzip` cannot read it yet. That is why `-c auto` uses deflate for `.zip`: a zip exists so that anyone can open it. Zstandard has to be requested explicitly, and it will become the default once Arca has its own format.
