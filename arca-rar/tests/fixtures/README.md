# RAR fixtures

The RAR5 fixtures were produced independently of `rars`, with the official
RAR 7.23 Linux command-line encoder. They contain only Arca test strings, no
third-party documents. The test password is `arca-test-only`, not a credential.
The encoder and decoder executables are not redistributed or needed by tests.

Regenerate with an installed, appropriately licensed/evaluation RAR binary:

```sh
python3 arca-rar/tests/generate-fixtures.py /absolute/path/to/rar
```

`plain.rar` is non-solid, `stored.rar` has uncompressed members, `solid.rar`
uses solid compression, `encrypted.rar` encrypts file contents, and
`headers.rar` is solid with encrypted headers. `volume.part*.rar` is a complete
four-volume set used to exercise discovery, split-chain checks and extraction
from any of its volumes, including renamed padded and legacy naming sets.
Timestamps, salts and ciphertext can change on regeneration; the expected
payloads in the tests do not.

`legacy-solid.rar` is an unchanged copy of
[`solid_simple_rar300.rar`](https://github.com/bitplane/rars/blob/8aa1429cc51ba248d81e32ad5b5eac6e4ec63ba2/crates/rars/tests/fixtures/rar15_40/rar300/solid_simple_rar300.rar)
from the Apache-2.0-declared `bitplane/rars` repository. Its fixture README
identifies the RAR 3.00 / Unpack29 solid case. It contains two short test strings
(`shared prefix shared prefix shared prefix alpha\n` and `... beta\n`).
SHA-256: `45a8f0825901b19df6e3bbe9d18f69d3f044b38d856176698f34ee0dbb52a980`.

`adversarial.rs` derives malicious paths, name collisions and oversized output
claims from the stored fixture, recomputing header CRCs so the adapter's safety
checks, rather than an earlier checksum rejection, are exercised.

Additional split-stream fixtures are unchanged copies from the pinned
[`rars 0.10.0` source tree](https://github.com/bitplane/rars/tree/8aa1429cc51ba248d81e32ad5b5eac6e4ec63ba2/crates/rars/tests/fixtures):

- `rar50/solid_multivol.part01.rar` through `part06.rar`: solid history and padded naming.
- `rar50/encrypted_multivol.part1.rar` through `part3.rar`: compressed encrypted split stream.
- `rar50/header_encrypted_stored_multivol.part1.rar` through `part3.rar`: stored, header-encrypted split stream; upstream documents WinRAR 7.12 generation.
- `rar15_40/rar300/encrypted_multivol_rar300.rar` and `.r00`: legacy AES-encrypted split stream.

The legacy pair sets the first-volume flag in both parts; the adapter rejects
that ordering. The regression also normalizes that bit and its main-header CRC
in a temporary copy to exercise encrypted legacy decoding, without altering
the committed upstream fixture or claiming it is independent encoder output.

Their test password is `password`, not a credential. Expected payloads/checksums
are asserted in `volumes.rs`, matching the upstream fixture tests. These expand
coverage, not the independent-encoder claim for Arca's existing four-volume set.

An independent decoder can be checked against Arca's output with:

```sh
cargo build -p arca-cli --features rar
ARCA="$PWD/target/debug/arca" ARCA_TEST_RAR=1 UNRAR=/absolute/path/to/unrar bash interop.sh
```
