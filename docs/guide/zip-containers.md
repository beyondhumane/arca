---
description: Open, list, test and extract APK, JAR, EPUB, CBZ, wheels and other ZIP-based files without renaming them, and why Arca keeps them read-only.
group: Reference
order: 21
keywords: zip container apk aar jar war ear epub cbz xpi whl wheel nupkg ipa read-only signature
---

# ZIP containers

Many file types are ordinary ZIP archives with their own extension and a few
extra rules. Arca opens them with its ZIP reader, so they need no renaming:

| Extension | What it is |
| --- | --- |
| `.apk`, `.aar` | Android app and library |
| `.jar`, `.war`, `.ear` | Java archive, web app and enterprise app |
| `.epub` | E-book |
| `.cbz` | Comic book |
| `.xpi` | Firefox add-on |
| `.whl` | Python wheel |
| `.nupkg` | NuGet package |
| `.ipa` | iOS app |

```sh
arca list app.apk
arca test book.epub                  # verify every CRC, write nothing
arca extract comic.cbz -o pages/
```

The desktop window opens them too, and shows the real format in the title,
for example `app.apk - Arca (APK: read-only)`. Extensions are matched without
regard to case, so `BOOK.EPUB` works the same.

## Read-only

Arca never creates or modifies these files. `arca create app.jar ...`,
`arca password`, and adding, deleting or renaming entries in the window are all
refused, with an error that names the format.

Rewriting them would break what makes them more than a ZIP:

- APK, AAR, JAR, XPI and IPA files usually carry a signature over their
  entries. Any change invalidates it, and the app or add-on no longer installs.
- An EPUB must start with an uncompressed `mimetype` entry. A rewrite that
  reorders or compresses it gives a book that readers reject.
- Wheels and NuGet packages list their files and hashes in a manifest that
  has to stay in step with the contents.

To change one, extract it, edit the files, and rebuild it with the tool made
for that format (`apksigner`, `jar`, `pip wheel`, `nuget pack` and so on).

Arca does not check the meaning of these formats: it doesn't validate an EPUB,
verify a signature or show comic pages. It reads them as ZIP, with the same
limits and the same protection against entries that try to escape the
destination folder.
