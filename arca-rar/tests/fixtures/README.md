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
cargo build -p arca-cli
ARCA="$PWD/target/debug/arca" ARCA_TEST_RAR=1 UNRAR=/absolute/path/to/unrar bash interop.sh
```

## Independent matrix

`matrix/manifest.json` records the exact command, encoder version and SHA-256,
every volume SHA-256, and the expected names, sizes and payload SHA-256 values.
The 18 cases were produced locally using official Linux RAR 7.23 (RAR5) and
6.24 (`-ma4`, RAR 1.5-4.x container / Unpack29). These contain only synthetic
Arca strings and deterministic xorshift bytes, licensed under this repository's
Apache-2.0 license. No encoder, decoder, SFX module or proprietary document is
included. Test passwords are public fixture data, not credentials.

```sh
# Destination must be new/empty; never overwrite the checked-in matrix in place.
python3 arca-rar/tests/generate-matrix.py /path/to/rar-7.23 /path/to/rar-6.24 /path/to/new-matrix
```

Generation disables external configuration, timestamps and extension sorting;
it uses one compression thread. Encryption salts are random, so regeneration
preserves payloads and coverage, not encrypted archive hashes. See the manifest
for every flag and the script for exact input bytes. `rar5-padded` has 12 genuine
encoder-produced `.part01.rar` ... `.part12.rar` volumes; the other modern sets
are unpadded. Legacy sets were generated with `-vn`, not renamed from RAR5.
The 33 MiB repetitive dictionary payload produces a small archive that actually
declares a 64 MiB dictionary. RAR reduces the `-md256k` small-dictionary
request to an actual 128 KiB declaration for these small files (verified with
official UnRAR `lt`, rather than inferring it from the generation flag).

Local tool downloads (not checked in):

| Package | Source | Package SHA-256 |
| --- | --- | --- |
| RAR 7.23 Linux x64 | `https://www.rarlab.com/rar/rarlinux-x64-723.tar.gz` | `759b4b6aa0d9f77131882162951193f3a0e54bf60e1d8dc4255aa308accab588` |
| RAR 6.24 Linux x64 | `https://www.rarlab.com/rar/rarlinux-x64-624.tar.gz` | `88e22a8e84125c947637bbf28c746e338a0a63279d80f9f9d7373603875db1eb` |

RAR's proprietary evaluation license applies to the local tools; obtain and
use them under appropriate terms. Official UnRAR 7.23 was the comparison tool,
SHA-256 `926d3a00775ed96afccfdef69c3781334b71ccdb733931edf30f4866e4f08410`.
This provenance is not legal clearance for distributing the RAR backend.

## Historical generations

`generations/manifest.json` records four **unchanged upstream**, not locally
produced, fixtures from the Apache-2.0-declared `rars 0.10.0` source tree at the
same revision linked above. Source paths and archive/payload hashes are recorded.
Upstream attributes `MULTIFIL.RAR` to RAR 1.402, `readme_154_normal.rar` to
RAR 1.54, and `AUTOREJ.RAR` / `SOLID.RAR` to RAR 2.50. The former contains two
tiny test strings; the 1.54 payload is the upstream clean-room specification's
README; the 2.50 payloads are synthetic text. No proprietary reference manual
or binary is included. Exact vintage producer commands were not supplied by
those fixture READMEs and were **not reconstructed or reproduced** here.

Official UnRAR checks those archive bytes and supplies independent extracted
payload hashes in the manifest. The Rust tests verify, preview and extract all
four. This adds RAR13-container, Unpack15 and Unpack20 coverage, not broad
historical producer compatibility. The previously present RAR3 fixtures and
new RAR4 matrix cover Unpack29, and the independent RAR5 matrix covers Unpack50.
