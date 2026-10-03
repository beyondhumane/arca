"""Assert both the opt-in boundary and Cargo's unified RAR feature set."""

import pathlib
import subprocess

root = pathlib.Path(__file__).resolve().parents[2]
for enabled in (False, True):
    args = ["cargo", "tree", "--workspace", "--locked", "-e", "normal,build",
            "--prefix", "none", "-f", "{p}|{f}"]
    if enabled:
        args += ["--features", "rar"]
    tree = subprocess.check_output(args, cwd=root, text=True)
    rar = [line for line in tree.splitlines() if line.startswith("rars v")]
    if enabled:
        assert rar, "RAR feature did not activate the reader"
        assert all(line.removesuffix(" (*)") == "rars v0.10.0|encryption" for line in rar), rar
    else:
        assert not rar, "default workspace unexpectedly includes rars"
    print(f"RAR {'enabled: encryption only' if enabled else 'disabled: no rars dependency'}")
