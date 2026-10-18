# Changelog

## [0.10.0](https://github.com/beyondhumane/arca/compare/v0.9.0...v0.10.0) (2026-10-18)


### Features

* **site:** list every release file and show sizes in MiB ([#61](https://github.com/beyondhumane/arca/issues/61)) ([ec8403a](https://github.com/beyondhumane/arca/commit/ec8403a6b562351d151038aa7d1f9e73ff602719))


### Bug fixes

* **arca-gui:** keep the real error when an AppImage update fails ([#77](https://github.com/beyondhumane/arca/issues/77)) ([33e5229](https://github.com/beyondhumane/arca/commit/33e5229eda61ac738ba280e8112a4c9294ddb294))
* **arca-gui:** persist the defaults for new archives ([#68](https://github.com/beyondhumane/arca/issues/68)) ([0d32b70](https://github.com/beyondhumane/arca/commit/0d32b70850a441b08ee99cdb7ba62952a461d9cb))
* **arca-gui:** report a cancelled conflict prompt as a stop ([#65](https://github.com/beyondhumane/arca/issues/65)) ([0ff16cc](https://github.com/beyondhumane/arca/commit/0ff16cc999075ed5fd5d462e6a83359db099a9b8))
* **arca-gui:** say why a disk copy or move failed ([#67](https://github.com/beyondhumane/arca/issues/67)) ([2445b1a](https://github.com/beyondhumane/arca/commit/2445b1a40d193584cfe51726baf43871d259a749))
* **arca-setup:** report best-effort step failures and uninstall leftovers ([#81](https://github.com/beyondhumane/arca/issues/81)) ([1068edf](https://github.com/beyondhumane/arca/commit/1068edf157a66c857173b9c1deda22b520b9a059))

## [0.9.0](https://github.com/beyondhumane/arca/compare/v0.8.2...v0.9.0) (2026-10-17)


### Features

* **arca-gui:** Arca is now a file explorer and an archiver in one window. It starts in the folder you left it in, browses the disk like any explorer and opens ZIP, 7z, TAR, RAR and ISO archives in place when you double-click them ([#52](https://github.com/beyondhumane/arca/issues/52)) ([de20c20](https://github.com/beyondhumane/arca/commit/de20c205e3a4c516c6c4cd9bb5bcfc9b21fd3d50))
* **arca-gui:** the window moves from GPUI to egui/eframe over OpenGL: a smaller binary, less memory, the same startup, and GPUI leaves the workspace ([#52](https://github.com/beyondhumane/arca/issues/52))
* **arca-gui:** Panes view with one folder per pane, each with Reload, Up, Sort, Filter and Close; settings to open folders in the next pane, to peek into folders on hover, and for the focus to follow the pointer; Details and Flat views stay ([#52](https://github.com/beyondhumane/arca/issues/52))
* **arca-gui:** right-click menu on files and folders: new folder with selection, open, open with, quick preview, cut, copy, paste, duplicate, rename, move to, copy to, compress, copy path, copy name, properties, move to Trash and permanently delete ([#52](https://github.com/beyondhumane/arca/issues/52))
* **arca-gui:** Devices lists the real volumes (removable, under /media, /run/media or /mnt, or not mounted yet) and offers Mount, Unmount, Format and Set label through UDisks ([#52](https://github.com/beyondhumane/arca/issues/52))
* **arca-gui:** preview panel with the file's icon, size, date and MIME type, text with line numbers and syntax highlighting, hex and images, line wrapping and "open with the system application" ([#52](https://github.com/beyondhumane/arca/issues/52))
* **arca-gui:** redesigned compress box: editable destination, automatic extension, format and method dropdowns, 0-9 level slider, password with hidden names for 7z, and a content list ([#52](https://github.com/beyondhumane/arca/issues/52))
* **arca-gui:** update dialog with the release notes, installed and available versions and a link to the release; the update downloads and installs from inside the app on Windows, macOS and Linux (installer, tar.gz or AppImage) ([#52](https://github.com/beyondhumane/arca/issues/52))
* **arca-gui:** search palette (Ctrl+K), themes (fifteen built in, plus custom theme files), fonts, text size, motion switch and an own title bar ([#52](https://github.com/beyondhumane/arca/issues/52))
* **arca-gui:** the sidebar shows or hides with one button and shrinks to icons on its own when the window is narrow ([#52](https://github.com/beyondhumane/arca/issues/52))
* **arca-cli:** `--level` accepts 0-9 as well as store, fast, normal and best ([#52](https://github.com/beyondhumane/arca/issues/52))


### Bug fixes

* **arca-gui:** skip dot folders in search on every platform ([#60](https://github.com/beyondhumane/arca/issues/60)) ([a03a7b6](https://github.com/beyondhumane/arca/commit/a03a7b6c4a1cbdb9a651e080ea42dc7b05430af3))
* **packaging:** name the launcher icon dev.beyondhumane.Arca ([#56](https://github.com/beyondhumane/arca/issues/56)) ([e8a0597](https://github.com/beyondhumane/arca/commit/e8a059775fc29cb7d501d09865bb2160f155ce72))
* **site:** give inline SVG icons and the logo image an intrinsic size ([#49](https://github.com/beyondhumane/arca/issues/49)) ([3627457](https://github.com/beyondhumane/arca/commit/36274574b71128ad02b3aea87f7daa6809608329))
* **site:** keep download cards aligned on mobile ([#51](https://github.com/beyondhumane/arca/issues/51)) ([a0e2282](https://github.com/beyondhumane/arca/commit/a0e22820d30b8f150f73aaaadc1a0c318ce79f21))


### Maintenance

* release 0.9.0 ([#59](https://github.com/beyondhumane/arca/issues/59)) ([ec94058](https://github.com/beyondhumane/arca/commit/ec94058ef264611014e819e4731ed7293a749494))

## [0.8.2](https://github.com/beyondhumane/arca/compare/v0.8.1...v0.8.2) (2026-10-05)


### Maintenance

* release 0.8.2 ([#47](https://github.com/beyondhumane/arca/issues/47)) ([f568097](https://github.com/beyondhumane/arca/commit/f568097f26ee4b40ebd682730158201206500e81))

## [0.8.1](https://github.com/beyondhumane/arca/compare/v0.8.0...v0.8.1) (2026-10-04)


### Bug fixes

* detect nested and output folders through symlinks and on Windows ([#44](https://github.com/beyondhumane/arca/issues/44)) ([63cddd7](https://github.com/beyondhumane/arca/commit/63cddd7501c5305a772c31780a32d503703675e9))

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
