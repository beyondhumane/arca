# Changelog

## [0.8.0](https://github.com/beyondhumane/arca/compare/v0.7.2...v0.8.0) (2026-10-04)


### ⚠ BREAKING CHANGES

* **7z:** arca-core's Method gains LZMA/LZMA2/Bzip2 variants and Format moves to arca_core::Format; downstream code that matched exhaustively or defined its own Format must migrate.
* **rar:** Entry.crc32 is now Option<u32>; use Some for known ZIP CRCs and None for unavailable checksums. Method::code() now returns Result<u16> so callers must propagate or handle errors for non-ZIP methods.

### Features

* **7z:** Add encrypted 7z support to CLI and desktop ([#26](https://github.com/beyondhumane/arca/issues/26)) ([7e6917f](https://github.com/beyondhumane/arca/commit/7e6917f098c3e3474a9482b12fbc84caeb9806f6))
* **arca-gui:** Align desktop themes with the Arca brand ([#11](https://github.com/beyondhumane/arca/issues/11)) ([ca00024](https://github.com/beyondhumane/arca/commit/ca0002402fb984929cefdef216b3cb11f70daae3))
* **rar:** Add opt-in read-only RAR support ([#12](https://github.com/beyondhumane/arca/issues/12)) ([c8918c8](https://github.com/beyondhumane/arca/commit/c8918c8f7d9804a98cda45aaf84f8e7483082150))
* **site:** add landing page, Markdown guide and Pages deployment ([#2](https://github.com/beyondhumane/arca/issues/2)) ([a57e17d](https://github.com/beyondhumane/arca/commit/a57e17d82019e453dfc8cc0877da036b7e4fa984))
* **site:** prerender every page with real URLs and SEO metadata ([#7](https://github.com/beyondhumane/arca/issues/7)) ([01b26e8](https://github.com/beyondhumane/arca/commit/01b26e8bdd566d4140562d295130d1c55fffd012))


### Bug fixes

* **arca-zip:** honor thread limits in zstd streaming ([#10](https://github.com/beyondhumane/arca/issues/10)) ([20c86c7](https://github.com/beyondhumane/arca/commit/20c86c7959fae1c14d014b07a6c7357991fd66f0))
* **site:** let taps reach the page beneath the fixed header on mobile ([#6](https://github.com/beyondhumane/arca/issues/6)) ([6abb2f5](https://github.com/beyondhumane/arca/commit/6abb2f53d35e644afb08b5289ef0aeacdbeaa0cd))
* **site:** use a middle dot instead of an em dash in the home preview title ([#8](https://github.com/beyondhumane/arca/issues/8)) ([fbac63e](https://github.com/beyondhumane/arca/commit/fbac63e9dda126ef95ef74942791ddc8beae05f4))


### Documentation

* move design rationale to docs/plans and remove Claude outputs ([#7](https://github.com/beyondhumane/arca/issues/7)) ([ca1acc2](https://github.com/beyondhumane/arca/commit/ca1acc2897839cef73869ed32e3e8285eb8dd846))
