"""Assert default RAR support, the opt-out and the encryption+write+parallel feature set.

The writer feature is intentional (new RAR5 archives), and parallel lets it use
every core; recovery stays off, and no build may fall back to the crate
defaults. The crate is the vendored copy in arca-rar/vendor/rars."""

import pathlib
import subprocess

root = pathlib.Path(__file__).resolve().parents[2]
cases = [
    ("workspace defaults", ["--workspace"], True),
    ("CLI defaults", ["-p", "arca-cli"], True),
    ("GUI defaults", ["-p", "arca-gui"], True),
    ("no defaults", ["--workspace", "--no-default-features"], False),
    ("native codecs without RAR", ["--workspace", "--no-default-features",
                                  "--features", "codecs-native"], False),
    ("RAR without native codecs", ["--workspace", "--no-default-features",
                                   "--features", "rar"], True),
]
for name, flags, enabled in cases:
    args = ["cargo", "tree", *flags, "--locked", "-e", "normal,build",
            "--prefix", "none", "-f", "{p}|{f}"]
    tree = subprocess.check_output(args, cwd=root, text=True)
    rar = [line for line in tree.splitlines() if line.startswith("rars v")]
    if enabled:
        assert rar, "RAR feature did not activate the reader"
        assert all(line.removesuffix(" (*)").split("|")[1] == "encryption,parallel,write"
                   and line.startswith("rars v0.10.2 (") and "vendor" in line for line in rar), rar
    else:
        assert not rar, f"{name} unexpectedly includes rars"
    print(f"{name}: {'vendored RAR with encryption,parallel,write' if enabled else 'no rars dependency'}")
