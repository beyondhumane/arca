---
description: Download a prebuilt binary for Windows, macOS or Linux, or build Arca from source with Cargo.
group: Getting started
order: 2
keywords: install download build cargo rust release binary windows macos linux setup exe
---

# Installation

Every release ships prebuilt binaries for Windows, macOS and Linux. You can also build Arca from source with a stock Rust toolchain.

## Download a release

Get the file for your platform from the [latest release](https://github.com/beyondhumane/arca/releases/latest):

| Platform | File | Notes |
| --- | --- | --- |
| Windows | `arca-setup-<version>-x86_64.exe` | Installer, x86\_64 |
| Windows | `arca-v<version>-windows-x86_64.zip` | Portable, no installer |
| macOS | `arca-v<version>-macos-arm64.tar.gz` | Apple silicon |
| macOS Intel | `arca-v<version>-macos-x86_64.tar.gz` | Intel |
| Linux | `arca-v<version>-linux-x86_64.tar.gz` | x86\_64 |
| Linux ARM | `arca-v<version>-linux-arm64.tar.gz` | arm64 |
| Linux | `arca-v<version>-linux-x86_64.AppImage` | Desktop window, any distribution; also `linux-arm64` |
| Debian, Ubuntu | `arca_<version>-1_amd64.deb` | Package; also `arm64` |
| Fedora, openSUSE | `arca-<version>-1.x86_64.rpm` | Package; also `aarch64` |

Every asset for every version, with its SHA-256 digest, is on the [releases page](https://github.com/beyondhumane/arca/releases). Each one also carries a build provenance attestation, which `gh attestation verify <file> -R beyondhumane/arca` checks.

On Linux the desktop window needs glibc 2.35 or newer (Ubuntu 22.04, Debian 12, current Fedora, Arch and openSUSE) and the command line 2.34, so it also runs on RHEL 9. Older systems, such as Debian 11, can [build from source](#build-from-source).

### Windows

Run the installer. It’s produced with Inno Setup from `windows/arca.iss` and sets Arca up together with the Explorer context menu; see [Windows integration](windows-integration.md).

### Linux packages

The `.deb` and `.rpm` install both binaries into `/usr/bin` and set Arca up on the desktop:

```sh
sudo apt install ./arca_*_amd64.deb      # Debian, Ubuntu
sudo dnf install ./arca-*.x86_64.rpm     # Fedora
sudo zypper install ./arca-*.x86_64.rpm  # openSUSE
```

| Path | What it does |
| --- | --- |
| `/usr/share/applications/arca.desktop` | Arca in the applications menu, and in **Open with** for ZIP, 7z, RAR, TAR, TAR.GZ, XZ, BZ2 and ZST (alone or around a TAR) and ISO images |
| `/usr/share/icons/hicolor/*/apps/dev.beyondhumane.Arca.*` | The icon from 16 to 256 px, plus the SVG |
| `/usr/share/kio/servicemenus/arca-*.desktop` | An **Arca** submenu in Dolphin: **Extract here** and **Extract to a new folder** on archives, **Add to archive…** and **Compress to ZIP** on anything |

Installing never changes which program opens a type by default: choose Arca in **Open with**, or make it the default in your desktop’s settings. ZIP-based packages and documents such as `.jar`, `.apk` or `.epub` are not claimed; Arca still opens them from the window. Every type above already has a standard name in shared-mime-info, so Arca ships no MIME definitions of its own. After installing and after removing, the package refreshes the desktop-entry and icon caches if `update-desktop-database` and `gtk-update-icon-cache` are present.

Other file managers keep their context-menu entries in your own configuration, so a package can’t add them:

- **Nautilus (GNOME Files)**: save a script like this one in `~/.local/share/nautilus/scripts/` and make it executable; it appears under **Scripts** in the right-click menu. `--extract-to-folder`, `--add` and `--add-quick` give the other actions.

  ```sh
  #!/bin/sh
  exec arca-gui --extract-here "$@"
  ```

- **Thunar**: in **Edit > Configure custom actions…**, add an action with the command `arca-gui --extract-here %F` (or one of the other flags) and pick the file types under **Appearance Conditions**.

The AppImage needs no installation and doesn’t register anything with the desktop. Make it executable and run it; `--cli` runs the command line instead of the window:

```sh
chmod +x arca-v*-linux-x86_64.AppImage
./arca-v*-linux-x86_64.AppImage
./arca-v*-linux-x86_64.AppImage --cli --help
```

### macOS and Linux

Download the tarball for your platform, unpack it, and put the `arca` binary somewhere on your `PATH`:

```sh
tar -xzf arca-v*-linux-x86_64.tar.gz
# then move the arca binary onto your PATH, for example:
sudo mv arca /usr/local/bin/
```

On macOS the release is the two binaries, not an app bundle, so Finder can’t offer **Open with Arca** or give archives Arca’s icon. That takes an `Arca.app` whose `Info.plist` declares the archive types (`CFBundleDocumentTypes`, plus `UTImportedTypeDeclarations` for the formats macOS has no type for), and a signed, notarised build so that Gatekeeper lets it open. The release workflow doesn’t build any of that yet.

> [!TIP]
> **macOS Gatekeeper**
>
> If macOS refuses to run a binary downloaded through the browser, clear the quarantine flag with `xattr -d com.apple.quarantine ./arca`.

### Nix

On NixOS or any Linux with Nix and flakes enabled, the repository is a flake. It builds both programs from source and installs the desktop entry:

```sh
nix run github:beyondhumane/arca              # opens the window
nix run github:beyondhumane/arca#arca -- --help
nix profile install github:beyondhumane/arca  # installs arca and arca-gui
```

`nix develop` gives a shell with the Rust toolchain and the tools `interop.sh` and `bench.sh` compare against.

## Build from source

You need Rust 1.95 or newer. The default build also compiles libzstd, so a C compiler (cc, clang or MSVC) must be available.

```sh
git clone https://github.com/beyondhumane/arca
cd arca
cargo build --release      # binary at target/release/arca
```

### Two build profiles

| Profile | Command | What you get |
| --- | --- | --- |
| Default | `cargo build --release` | Native Zstandard codecs and read-only RAR. |
| Pure-Rust codecs with RAR | `cargo build --release --no-default-features --features rar` | No native ZIP/7z codecs; retains the RAR reader. |
| No default features | `cargo build --release --no-default-features` | No native ZIP/7z codecs or RAR reader. |

Use `--no-default-features --features codecs-native` to exclude only RAR.

### Verify your build

```sh
cargo test --workspace
bash interop.sh            # the phase acceptance criterion
```

`interop.sh` round-trips archives through the system’s `zip`, `unzip`, `tar` and 7-Zip, so have them installed first. See [Interoperability](interoperability.md) for what it checks.

### About the release profile

Release builds use fat LTO, a single codegen unit, `panic = "abort"` and stripped symbols, tuned for requirement R1 (startup) and R3/R4 (throughput).

```toml
[profile.release]
opt-level = 3
lto = "fat"
codegen-units = 1
panic = "abort"
strip = true
```
