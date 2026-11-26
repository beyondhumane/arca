"""Verify fixture hashes, CLI metadata/bytes, and optionally official UnRAR."""

import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parent / "fixtures/matrix"


def run(*args):
    result = subprocess.run(args, capture_output=True, text=True, timeout=60,
                            env={**os.environ, "LC_ALL": "C.UTF-8"})
    if result.returncode:
        raise AssertionError(f"{args}: {result.stdout}\n{result.stderr}")
    return result.stdout


def files(root):
    return {p.relative_to(root).as_posix(): {"size": p.stat().st_size,
            "sha256": hashlib.sha256(p.read_bytes()).hexdigest()}
            for p in root.rglob("*") if p.is_file()}


def main():
    independent = json.loads((ROOT / "manifest.json").read_text())
    generations = ROOT.parent / "generations"
    legacy = json.loads((generations / "manifest.json").read_text())
    arca = str(Path(os.environ["ARCA"]).resolve())
    unrar = os.environ.get("UNRAR")
    if unrar:
        unrar = str(Path(unrar).resolve())
        banner = subprocess.run([unrar], capture_output=True, text=True).stdout
        assert re.search(r"UNRAR \d", banner), "use official UnRAR, not a wrapper"
        print(banner.splitlines()[1])
    else:
        print("SKIP official UnRAR comparison: set UNRAR to a local official decoder")
    for base, case in [(ROOT, c) for c in independent["cases"]] + [(generations, c) for c in legacy["cases"]]:
        source = base / case["name"]
        for name, checksum in case["files"].items():
            assert hashlib.sha256((source / name).read_bytes()).hexdigest() == checksum
        archive = str(source / case["first"])
        args = ["-p", case["password"]] if case["password"] else []
        listing = run(arca, "list", archive, *args)
        metadata = {}
        for line in listing.splitlines():
            match = re.fullmatch(r"\s*(\d+)\s+\S+\s+-?[\d.]+%\s+(.+)", line)
            assert match, line
            metadata[match[2]] = int(match[1])
        expected = {name: meta["size"] for name, meta in case["payloads"].items()}
        assert metadata == expected, (case["name"], metadata, expected)
        run(arca, "test", archive, *args)
        with tempfile.TemporaryDirectory(prefix="arca-unrar-compare-") as tmp:
            out = Path(tmp) / "arca"
            run(arca, "extract", archive, "-o", str(out), *args)
            assert files(out) == case["payloads"], case["name"]
            if unrar:
                pw = "-p" + case["password"] if case["password"] else "-p-"
                reference = Path(tmp) / "unrar"
                reference.mkdir()
                # -v includes all volumes. Split entries repeat; logical sizes must agree.
                official = run(unrar, "lt", "-v", pw, archive)
                names = {}
                name = None
                for line in official.splitlines():
                    if line.strip().startswith("Name:"):
                        name = line.split(":", 1)[1].strip()
                    elif line.strip().startswith("Size:"):
                        size = int(line.split(":", 1)[1])
                        assert name not in names or names[name] == size
                        names[name] = size
                assert names == metadata, (case["name"], names, metadata)
                run(unrar, "t", "-idq", pw, archive)
                run(unrar, "x", "-idq", "-o+", pw, archive, str(reference) + os.sep)
                assert files(reference) == files(out), case["name"]
        print(f"PASS {case['name']}: hashes, names/sizes, verify, extracted bytes"
              + (", official UnRAR" if unrar else ""))
    print(f"{len(independent['cases'])} independent matrix cases and {len(legacy['cases'])} upstream generation cases passed")


if __name__ == "__main__":
    main()
