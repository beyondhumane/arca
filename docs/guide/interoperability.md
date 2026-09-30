---
description: What interop.sh verifies against unzip, tar and 7-Zip.
group: Reference
order: 13
keywords: interop compatibility 7-zip unzip tar winrar nanazip sha-256
---

# Interoperability

An archiver is only useful if other tools can open what it writes. `interop.sh` checks 35 cases, verifying the SHA-256 of the contents every time.

## What interop.sh checks

- What Arca writes is read by `unzip`, `tar` and 7-Zip, at all four levels.
- What `zip`, `tar` and 7-Zip write is read by Arca without losing a byte.
- An archive Arca encrypted with AES-256 opens in 7-Zip, and the other way round.
- Adding and removing the password of an existing archive, Arca’s own or 7-Zip’s.
- An altered byte is caught by the CRC, or by the HMAC when encrypted.
- An entry with `../../` is rejected instead of writing outside the destination.

```sh
bash interop.sh
```

## Zstandard in ZIP

Zstandard is ZIP method 93: registered in the specification, but not yet read by classic `unzip`. That’s why `-c auto` uses Deflate in a `.zip`, so that anything can open it. Zstandard is asked for by hand, and will be the default once there is a native format.

## Encryption

AES-256 archives use WinZip AE-2, the scheme 7-Zip, WinRAR and NanaZip write. Legacy ZipCrypto archives from other tools open with their password and can be upgraded with `arca password`.

```sh
7z t -p"a password" secret.zip        # 7-Zip reads what Arca encrypted
```
