---
description: ZIP and 7z AES-256, hidden names, password checks and integrity limits.
group: Using Arca
order: 7
keywords: password aes aes-256 encrypt decrypt zipcrypto hmac pbkdf2 security
---

# Encryption

Arca encrypts `.zip` archives with AES-256 using the WinZip AE-2 scheme, the same one 7-Zip, WinRAR and NanaZip write. `interop.sh` checks it in both directions against 7-Zip.

## The ZIP scheme

| Step | Algorithm |
| --- | --- |
| Key derivation | PBKDF2-HMAC-SHA1, 1000 rounds |
| Cipher | AES-256 in CTR mode |
| Authentication | HMAC-SHA1 over the ciphertext |
| Salt | Random 16 bytes, fresh for every entry |

## Design decisions

- **A salt per entry.** Reusing a salt would reuse the keystream, and two identical files would look identical inside the archive.
- **Compress, then encrypt.** That’s the order the specification calls for, because encrypted bytes leave the compressor nothing to compress.
- **The CRC is stored as zero.** AE-2 says so: it’s a checksum of the plaintext and has no business being there once the HMAC speaks for the data.
- **Tampering fails loudly.** An altered byte doesn’t come out as content; it fails the authentication code.

> [!WARNING]
> **Streaming decryption**
>
> Decryption streams, so the HMAC verdict arrives once the bytes have already been written. If extraction fails, throw away whatever it wrote.

> [!NOTE]
> **File names are visible**
>
> The ZIP format doesn’t allow file names to be encrypted, so anyone can list an encrypted archive without the password.

## Usage

```sh
arca create secret.zip folder/ -p "a password"
arca extract secret.zip -o where/ -p "a password"
7z t -p"a password" secret.zip        # 7-Zip reads it
```

## 7z encryption and hidden names

```sh
arca create secret.7z folder/ -p "a password" --hide-names
arca list secret.7z -p "a password"
arca test secret.7z -p "a password"
arca extract secret.7z -o where/ -p "a password"
7z t -p"a password" secret.7z
```

7z uses AES-256-CBC and a SHA-256 password KDF (power 19 when Arca creates it),
with fresh random salt/IV material per data block and header. It is not WinZip
AE-2 and has no HMAC: CRC-32 detects accidental corruption but is **not
cryptographic authentication**. An encrypted decode/CRC error means an incorrect
password **or corrupted archive**, not proof of either one.

Without `--hide-names`, file names stay visible and `list` needs no password.
With it, headers are encrypted and even listing needs the password. All encrypted
7z data is validated before extraction creates directories, opens destination
files or replaces existing ones. Extraction then decodes the selected data again;
the source must remain stable between the two passes. This protects destinations
on failed password/CRC checks, not against malicious CRC forgery.

The password must be nonempty. Empty-only input requires hidden names so a
password can be verified. CLI passwords can appear in shell history/process
arguments; use the desktop password field when that matters. Password fields are
not a claim of secret-memory zeroization.

## Legacy ZipCrypto

ZipCrypto, the old password scheme, is read but never written: an archive from another tool opens with its password, and `arca password` moves it to AES-256. The scheme is broken by design, so nothing new is created with it. Its password check is a single byte, so one wrong password in 256 slips past it and fails on the checksum instead.

## Changing the password of an existing archive

```sh
arca password secret.zip -p "a password"                 # takes it off
arca password plain.zip --new "a password"               # puts one on
arca password secret.zip -p "old" --new "new"            # changes it
arca password secret.zip -p "a password" -o clean.zip    # leaves the original alone
```

**Nothing is compressed again.** AES encrypts the already-compressed bytes, so removing the encryption gives back exactly the Deflate stream that was there, and the compressed size doesn’t change. What it does cost is the CRC: an AE-2 entry stores zero there, so the entry is decompressed once to compute it before it can be written unencrypted.

> [!TIP]
> **Safe in place**
>
> Replacing in place would destroy the only copy of the data, so the new archive is built alongside the old one, read back in full to check it’s sound, and only then moved over it. If anything fails, the original is left as it was and no temporary file is left behind.

### In the window

The create dialog offers passwords for ZIP and 7z, plus **Hide file names** for
7z. Hidden headers prompt before listing; other encrypted 7z contents are
validated in a background worker before extraction, testing, preview or opening
a member. Retry resumes the original action; cancel does not write output.

Changing an existing password, adding/deleting entries and renaming entries are
**ZIP-only**. They are unavailable for 7z because solid blocks may need rewriting.
