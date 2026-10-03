# arca-7z

Bounded, safe-Rust 7z integration. This crate does not implement filesystem
extraction or UI behavior. CLI/GUI manifests forward codec features, but callers
must integrate their own destination, symlink, conflict, and cleanup policies.

## API

```rust,ignore
SevenZArchive::<R: Read + Seek>::open(source: R, password: Option<&str>) -> Result<Self>
archive.entries() -> &[arca_core::Entry]
archive.len() -> usize
archive.is_empty() -> bool
archive.is_solid() -> bool
archive.has_encrypted() -> bool
archive.has_encrypted_header() -> bool
archive.validate_encrypted(notify: &mut dyn FnMut(Progress<'_>) -> bool) -> Result<()>
archive.test(notify: &mut dyn FnMut(Progress<'_>) -> bool) -> Result<()>
archive.extract(
    indices: &[usize],
    each: &mut dyn FnMut(usize, &Entry, &mut dyn Read) -> Result<()>,
    notify: &mut dyn FnMut(Progress<'_>) -> bool,
) -> Result<()>
archive.read_entry(index: usize, max_bytes: u64, notify: &mut dyn FnMut(Progress<'_>) -> bool)
    -> Result<Vec<u8>>
create_7z(output: &Path, sources: &[Source], options: &CreateOptions<'_>,
    notify: &mut dyn FnMut(Progress<'_>) -> bool) -> Result<()>
password_required(error: &Error) -> bool

Source { path: PathBuf, name: String }
CreateOptions { level: Level, password: Option<&str>, hide_names: bool }
Progress { phase: Phase, entries_done: usize, entries_total: usize, bytes_done: u64, name: &str }
Phase::{Validate, Extract, Test, Create}
```

## Reading and validation contract

- `open` validates the signature, start/next headers, counts, stream mapping,
  archive bounds and every entry name. It reads/decrypts encoded headers, but
  does not decode file contents. Clear-header encrypted archives can therefore
  be listed without a password; encrypted headers require one during `open`.
- `PasswordRequired` means no password was supplied for encrypted bytes.
  `PasswordOrCorrupt` means an encrypted decode or CRC check failed: a wrong
  password **or corruption**, never proof of tampering/authentication. The helper
  `password_required` matches both so a UI may offer password entry/retry.
- `extract` validates the entire archive's data before **any** extraction
  callback if any data/header is encrypted. It therefore does not invoke the
  callback even for a selected directory if an unselected encrypted file fails.
  Callers must not create directories, open/truncate output files, or otherwise
  prepare the destination before that callback. `validate_encrypted` is exposed
  for workflows that need this barrier separately; `extract` repeats it rather
  than trusting cached validation. The source must remain unchanged between
  passes. CRC-32 is not a cryptographic authentication mechanism.
- Selection uses entry indices, deduplicated. Callbacks run in block order,
  then standalone empty entries, not the order of `indices`. A selected solid
  block is decoded once per pass, including unselected members. Unread callback
  bytes are drained and checked; no whole-archive uncompressed buffer is used.
  `test` similarly streams all entries. Plain extraction is not transactional:
  its callback can have written before a later CRC/decode failure.
- `read_entry` rejects declared sizes above the caller's byte cap before any
  allocation and uses the same validated extraction path. The one selected
  entry is buffered. Large solid previews still decode the containing block.
- Names pass `arca_core::safe_name` plus checks for absolute, ADS/drive,
  reserved-device, trailing-dot/space, duplicate normalized names, anti-items,
  links and reparse entries. Callbacks must still use `safe_name` when joining
  paths and prevent destination symlinks from escaping the root. CLI/GUI use
  `arca_core::extraction::Destination` for handle-relative, staged filesystem writes.
- `Entry.offset` is the entry index, not a byte offset. `raw_name` is UTF-8
  decoded from 7z UTF-16, not ZIP bytes. A solid block's compressed size belongs
  only to its first entry; later entries report zero. `Entry.encrypted` describes
  its data block; `has_encrypted_header` separately describes hidden names.
- `Method::Lzma`, `Lzma2`, `Bzip2`, and `Other7z` are display/metadata values.
  `Method::code() -> Result<u16>` is ZIP-specific and rejects those values.
- Callback cancellation returns `Error::Cancelled`. Byte progress counts actual
  decoded bytes (including skipped solid members), resets at each phase, and
  is reported at most 64 KiB apart on extraction. Entry totals count selections.
  Opening headers and constructing a decoder/KDF are bounded but not cancellable
  internally. Sink errors remain `Error::Io`, not wrong-password errors.
- Malformed headers/streams return `Format` or source `Io`; decoder memory and
  preview limits return `Limit`; unsupported codecs/layouts return `Unsupported`.
  Header/count policy limits from the upstream parser are reported as `Format`.

## Creation

`Level::Store` uses Copy, and Fast/Normal/Best use LZMA2 levels 1/6/9. Each regular
file is a separate block; creation is non-solid. Directories and empty files are
explicit `Source` entries, not recursively enumerated. Symlinks/special sources
are rejected. Inputs must not change during creation.

A nonempty password enables AES-256-CBC with upstream's SHA-256 KDF at power 19.
Each data block and encrypted header has an independently generated OS-random
salt/IV. `hide_names` encrypts headers and requires a nonempty password. An
archive containing only empty files/directories requires `hide_names` when
encrypted, since empty plaintext data cannot validate a password with CRC.
Encryption, compression and KDF implementations are entirely upstream libraries.

Output is written to a temporary file beside the destination, flushed, finished,
synced and persisted only on success. Failure/cancellation removes the temporary
file and preserves existing output. The parent directory must already exist.
Successful persistence replaces an existing output; prompt for overwrite before
calling. This is not a crash-durable directory-fsync guarantee.

## Dependency and resource policy

See [vendor/PATCHES.md](vendor/PATCHES.md) for exact versions and local changes.
Encoded and decoded headers are limited to 16 MiB; file/block/stream counts to
100,000; coder chains to four single-input/output stages. LZMA/LZMA2 dictionary
memory is limited to 256 MiB per stage; Zstd's window is capped at 256 MiB.
Work is sequential and cancellation is cooperative, not a CPU-time quota.
There is no fixed total extracted-byte cap; callers can cancel using progress.

Supported baseline decoders are Copy, LZMA, LZMA2, DEFLATE, AES and upstream's
single-stream BCJ/Delta filters. Default `codecs-native` adds Bzip2 and Zstd.
`--no-default-features` disables both optional codecs and ZIP native codecs when
used through the CLI/GUI. PPMd, Brotli, LZ4, BCJ2/multi-input blocks and archives
above the policy bounds are unsupported. Large resources are rejected rather
than silently allocated. No password change, append, rename, or deletion API.

## Checks

```sh
cargo test -p arca-7z
cargo test -p arca-7z --no-default-features
cargo clippy -p arca-7z --all-targets -- -D warnings
cargo clippy -p arca-7z --all-targets --no-default-features -- -D warnings
cargo test -p arca-7z --test interop -- --ignored # requires 7z
```

Small adversarial encrypted fixtures use power 1 solely to bound repeated tests;
production creation is also round-tripped at power 19, unchanged by test builds.
