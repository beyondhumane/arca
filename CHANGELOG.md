# Changelog

## [0.8.0](https://github.com/beyondhumane/arca/compare/v0.7.2...v0.8.0) (2026-10-04)


### ⚠ BREAKING CHANGES

* **7z:** arca-core's Method gains LZMA/LZMA2/Bzip2 variants and Format moves to arca_core::Format; downstream code that matched exhaustively or defined its own Format must migrate.
* **rar:** Entry.crc32 is now Option<u32>; use Some for known ZIP CRCs and None for unavailable checksums. Method::code() now returns Result<u16> so callers must propagate or handle errors for non-ZIP methods.

### Features

* **7z:** Add encrypted 7z support to CLI and desktop ([#26](https://github.com/beyondhumane/arca/issues/26)) ([7e6917f](https://github.com/beyondhumane/arca/commit/7e6917f098c3e3474a9482b12fbc84caeb9806f6))
* **arca-gui:** add a column workspace with integrated previews ([#27](https://github.com/beyondhumane/arca/issues/27)) ([3a99198](https://github.com/beyondhumane/arca/commit/3a991986c35eb743ca9d606c9ebd63b07b4ba7ad)), closes [#13](https://github.com/beyondhumane/arca/issues/13)
* **arca-gui:** Align desktop themes with the Arca brand ([#11](https://github.com/beyondhumane/arca/issues/11)) ([ca00024](https://github.com/beyondhumane/arca/commit/ca0002402fb984929cefdef216b3cb11f70daae3))
* **arca-gui:** open on the local disk like a file explorer ([#39](https://github.com/beyondhumane/arca/issues/39)) ([7fed577](https://github.com/beyondhumane/arca/commit/7fed577a302a56017dc0dcb37fef01863f17e3c3))
* **arca-gui:** restyle the explorer shell and Settings after Strata ([#41](https://github.com/beyondhumane/arca/issues/41)) ([b0ba10a](https://github.com/beyondhumane/arca/commit/b0ba10a1e0868f9e80e31fe1b4a00a1c3f41a8ed))
* **arca-gui:** run operations from a queue with a floating progress panel ([#40](https://github.com/beyondhumane/arca/issues/40)) ([c6a7458](https://github.com/beyondhumane/arca/commit/c6a74587dee159b20c3ca955deabeef46647190f))
* **iso:** read ISO 9660 images, read-only ([#36](https://github.com/beyondhumane/arca/issues/36)) ([b994ef6](https://github.com/beyondhumane/arca/commit/b994ef6707082152abf630b64d9357211545b4e2))
* **nix:** add a flake that builds arca and arca-gui from source ([#32](https://github.com/beyondhumane/arca/issues/32)) ([4e8ee69](https://github.com/beyondhumane/arca/commit/4e8ee699f1188403e0fff84dff23ff378eca74af))
* open ZIP-based containers and CBR as read-only formats ([#34](https://github.com/beyondhumane/arca/issues/34)) ([ae10fdb](https://github.com/beyondhumane/arca/commit/ae10fdb5f4962e01f10cbed2f2799b4ae1db380d))
* **rar:** Add opt-in read-only RAR support ([#12](https://github.com/beyondhumane/arca/issues/12)) ([c8918c8](https://github.com/beyondhumane/arca/commit/c8918c8f7d9804a98cda45aaf84f8e7483082150))
* **rar:** Enable multivolume reading and safe RAR5 creation ([#28](https://github.com/beyondhumane/arca/issues/28)) ([415c662](https://github.com/beyondhumane/arca/commit/415c662ca893d424d1c626acd38915a64ab7e4ae))
* read and create XZ streams and TAR.XZ archives ([#38](https://github.com/beyondhumane/arca/issues/38)) ([db7a49a](https://github.com/beyondhumane/arca/commit/db7a49ab2b811f02942ca295e768ae4b22518bbc)), closes [#18](https://github.com/beyondhumane/arca/issues/18)
* **release:** publish an AppImage, .deb and .rpm for Linux x86_64 and arm64 ([#31](https://github.com/beyondhumane/arca/issues/31)) ([c3103f2](https://github.com/beyondhumane/arca/commit/c3103f2ccf5d792b7e8f28cb9a8d8e51307407a4))
* **site:** add landing page, Markdown guide and Pages deployment ([#2](https://github.com/beyondhumane/arca/issues/2)) ([a57e17d](https://github.com/beyondhumane/arca/commit/a57e17d82019e453dfc8cc0877da036b7e4fa984))
* **site:** prerender every page with real URLs and SEO metadata ([#7](https://github.com/beyondhumane/arca/issues/7)) ([01b26e8](https://github.com/beyondhumane/arca/commit/01b26e8bdd566d4140562d295130d1c55fffd012))


### Bug fixes

* **arca-gui:** ask again after a wrong password ([#37](https://github.com/beyondhumane/arca/issues/37)) ([152f4be](https://github.com/beyondhumane/arca/commit/152f4beb75d54faff4dd9e6301b89bbe31bd48c3))
* **arca-zip:** honor thread limits in zstd streaming ([#10](https://github.com/beyondhumane/arca/issues/10)) ([20c86c7](https://github.com/beyondhumane/arca/commit/20c86c7959fae1c14d014b07a6c7357991fd66f0))
* **site:** let taps reach the page beneath the fixed header on mobile ([#6](https://github.com/beyondhumane/arca/issues/6)) ([6abb2f5](https://github.com/beyondhumane/arca/commit/6abb2f53d35e644afb08b5289ef0aeacdbeaa0cd))
* **site:** use a middle dot instead of an em dash in the home preview title ([#8](https://github.com/beyondhumane/arca/issues/8)) ([fbac63e](https://github.com/beyondhumane/arca/commit/fbac63e9dda126ef95ef74942791ddc8beae05f4))


### Documentation

* move design rationale to docs/plans and remove Claude outputs ([#7](https://github.com/beyondhumane/arca/issues/7)) ([ca1acc2](https://github.com/beyondhumane/arca/commit/ca1acc2897839cef73869ed32e3e8285eb8dd846))
