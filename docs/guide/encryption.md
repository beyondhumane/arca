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

### AES-128 and AES-192

WinZip AES also comes in 128- and 192-bit strengths, with an 8- or 12-byte salt and a 16- or 24-byte key. Arca reads both, with the same password check and the same HMAC verdict as AES-256, so a wrong password or a tampered byte fails the same way. It never writes them: `arca password` with a new password moves such an archive to AES-256, while adding, removing or renaming entries copies them across with the strength they had.

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

Changing an existing password is **ZIP-only**. Adding, deleting and renaming
entries also work in 7z: the archive is decoded and encoded again with the same
password, and with its names hidden again if they were.

## Saved passwords

The window can keep archive passwords in **Settings > Passwords** and try them before it asks. The command line can’t use them yet; `arca` still takes `-p`.

### What gets tried, and when

Each saved password has a label, a mask and a **Try without asking** switch. A mask is a name pattern with `*` and `?` (`*.zip`, `backup-*.7z`), a folder (`D:\Backups\` takes in everything under it), several of those separated by `;`, or nothing, which fits every archive. Case is ignored.

When opening, extracting, testing, viewing a file with F3, computing checksums, comparing or editing an encrypted 7z needs a password, the window first tries the saved passwords whose mask fits and that have **Try without asking** on: an exact name first, then the masks with the most fixed characters, then the ones without a mask. Each try is the same local check the password box runs; no password leaves the computer. Saved passwords with the switch off aren’t tried: the password box offers them as **Use …** buttons. If none fit, the box asks as usual, without calling the attempt a wrong password.

**Save this password** in the password box keeps the password you type, but only once it has opened the archive, so a typo is never kept. It’s saved with the archive’s name as its mask and **Try without asking** on, and from then on that archive opens without asking. Widen the mask in Settings to cover a whole folder or family of names.

### The master password

Nothing is written to disk without a master password. Without one, saved passwords last until Arca closes. With one, they go to `passwords.vault` beside `gui.conf` (`%APPDATA%\Arca` on Windows, `~/Library/Application Support/Arca` on macOS, `$XDG_CONFIG_HOME/Arca` or `~/.config/Arca` elsewhere). They never go into `gui.conf`, a log or the clipboard.

| Step | Algorithm |
| --- | --- |
| Key derivation | Argon2id (v1.3), 64 MiB, 3 passes, 1 lane, 32-byte key |
| Salt | Random 16 bytes, new for every master password |
| Cipher | AES-256-GCM, a random 12-byte nonce for every write |
| Authentication | The GCM tag covers the whole list and, as associated data, the header with the cost parameters |

The file starts with `ARCAVLT\0`, a format version and the Argon2id cost, so a later version can raise the cost or change the format and still read this one. A file from a newer Arca is refused rather than misread, and a file that asks for an absurd cost (over 1 GiB, 64 passes or 16 lanes) is refused before any work is done.

- **Wrong master password or damaged file.** GCM can’t tell them apart, and Arca doesn’t pretend to. A truncated or altered file is refused the same way, never read halfway.
- **Never overwritten blind.** Before every write Arca checks that the file is still the one it read. A file that changed (another window, a restore, damage) is left alone and the change is refused. A file that can’t be opened can only be **set aside**: it’s renamed with a timestamp, not deleted, so it still opens with the old master password.
- **Locking.** The vault locks after 10 minutes without use and when the window closes. Locking drops the key and the passwords read from the file; passwords kept only for the session stay until Arca closes.
- **Memory.** The key, the master password, the decrypted list and the saved passwords are held in buffers that are zeroed when freed. A password handed to an open, extract or test job is still a plain string, like a typed one.

> [!NOTE]
> **ZipCrypto and automatic tries**
>
> The ZipCrypto password check is a single byte, so about one wrong password in 256 passes it, and the same one passes every time for that archive. A ZipCrypto archive open in the window therefore never has its saved passwords tried: they are all offered as **Use …** buttons instead. An extraction or test started from the file manager on a ZipCrypto archive still tries them; if one slips past the check, the job fails on the checksum each time. Turn **Try without asking** off for that entry, or narrow its mask. AES archives check a two-byte verifier and the HMAC, so this is far less likely there.
