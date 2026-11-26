# Arca guide

**English** | [Español](es/README.md)

User documentation. The website renders these files under `/docs/<file name>/` (and `/es/docs/<file name>/`), and publishes the raw Markdown next to them as `/docs/<file name>.md`; the front matter sets the sidebar group, order, description and search keywords. Spanish translations live in [`es/`](es/README.md) under the same file names; a page missing there falls back to English, and changes to a page should update both. The `English | Español` line under each title comes from `site/scripts/doc-langs.mjs`: to add a language, list it there, run `node scripts/doc-langs.mjs` from `site/` and add a root `README.<code>.md`.

1. [Introduction](introduction.md)
2. [Installation](installation.md)
3. [Quick start](quick-start.md)
4. [Creating archives](creating-archives.md)
5. [Listing & extracting](listing-extracting.md)
6. [Testing & benchmarking](testing-benchmarking.md)
7. [Encryption](encryption.md)
8. [The desktop window](desktop-app.md)
9. [Keyboard shortcuts](keyboard-shortcuts.md)
10. [Windows integration](windows-integration.md)
11. [CLI reference](cli-reference.md)
12. [Architecture](architecture.md)
13. [Interoperability](interoperability.md)
14. [Benchmarks](benchmarks.md)
15. [Roadmap](roadmap.md)
16. [Contributing & license](contributing.md)
17. [RAR](rar.md)
18. [ZIP containers](zip-containers.md)
19. [ISO images](iso.md)
20. [CPIO, AR, DEB, RPM, CAB and LZH](more-formats.md)
21. [Disk images](disk-images.md)
