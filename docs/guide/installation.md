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

Every asset for every version — with its SHA-256 digest — is on the [releases page](https://github.com/beyondhumane/arca/releases).

### Windows

Run the installer. It’s produced with Inno Setup from `windows/arca.iss` and sets Arca up together with the Explorer context menu — see [Windows integration](windows-integration.md).

### macOS and Linux

Download the tarball for your platform, unpack it, and put the `arca` binary somewhere on your `PATH`:

```sh
tar -xzf arca-v*-linux-x86_64.tar.gz
# then move the arca binary onto your PATH, for example:
sudo mv arca /usr/local/bin/
```

> [!TIP]
> **macOS Gatekeeper**
>
> If macOS refuses to run a binary downloaded through the browser, clear the quarantine flag with `xattr -d com.apple.quarantine ./arca`.

## Build from source

You need Rust **1.75 or newer**. The default build also compiles libzstd, so a C compiler (cc, clang or MSVC) must be available.

```sh
git clone https://github.com/beyondhumane/arca
cd arca
cargo build --release      # binary at target/release/arca
```

### Two build profiles

| Profile | Command | What you get |
| --- | --- | --- |
| codecs-native (default) | `cargo build --release` | Includes libzstd (C) for native Zstandard performance. |
| pure Rust | `cargo build --release --no-default-features` | No C dependency — builds for any target Rust supports. |

### Verify your build

```sh
cargo test --workspace
bash interop.sh            # the phase acceptance criterion
```

`interop.sh` round-trips archives through the system’s `zip`, `unzip`, `tar` and 7-Zip, so have them installed first. See [Interoperability](interoperability.md) for what it checks.

### About the release profile

Release builds use fat LTO, a single codegen unit, `panic = "abort"` and stripped symbols — tuned for requirement R1 (startup) and R3/R4 (throughput).

```toml
[profile.release]
opt-level = 3
lto = "fat"
codegen-units = 1
panic = "abort"
strip = true
```
