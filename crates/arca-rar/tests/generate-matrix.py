"""Generate synthetic Apache-2.0 fixtures, never redistribute the local encoders."""

import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile

PASSWORD = "arca-test-only"


def payloads():
    state = 0x41524341
    noise = bytearray()
    for _ in range(8192):
        state ^= (state << 13) & 0xFFFFFFFF
        state ^= state >> 17
        state ^= (state << 5) & 0xFFFFFFFF
        noise.append(state & 255)
    return {
        "first.txt": b"Arca independent matrix\n" * 64,
        "noise.bin": bytes(noise),
        "nested/caf\u00e9-\u65e5\u672c.txt": b"Arca Unicode filename\n" * 16,
        "empty.txt": b"",
    }


def generate(rar5, rar4, destination):
    cases = [
        ("rar5-small-dict", rar5, ["-ma5", "-md256k"]),
        ("rar5-solid", rar5, ["-ma5", "-s"]),
        ("rar5-content", rar5, ["-ma5", "-p" + PASSWORD]),
        ("rar5-headers", rar5, ["-ma5", "-s", "-hp" + PASSWORD]),
        ("rar5-split", rar5, ["-ma5", "-m0", "-v4k"]),
        ("rar5-padded", rar5, ["-ma5", "-m0", "-v1k"]),
        ("rar5-solid-split", rar5, ["-ma5", "-s", "-v4k"]),
        ("rar5-content-split", rar5, ["-ma5", "-s", "-p" + PASSWORD, "-v4k"]),
        ("rar5-headers-split", rar5, ["-ma5", "-s", "-hp" + PASSWORD, "-v4k"]),
        ("rar5-dict64", rar5, ["-ma5", "-md64m"]),
        ("rar4-plain", rar4, ["-ma4", "-md128k"]),
        ("rar4-solid", rar4, ["-ma4", "-s"]),
        ("rar4-content", rar4, ["-ma4", "-p" + PASSWORD]),
        ("rar4-headers", rar4, ["-ma4", "-s", "-hp" + PASSWORD]),
        ("rar4-legacy", rar4, ["-ma4", "-vn", "-m0", "-v4k"]),
        ("rar4-legacy-solid", rar4, ["-ma4", "-vn", "-s", "-v4k"]),
        ("rar4-legacy-content", rar4, ["-ma4", "-vn", "-s", "-p" + PASSWORD, "-v4k"]),
        ("rar4-legacy-headers", rar4, ["-ma4", "-vn", "-s", "-hp" + PASSWORD, "-v4k"]),
    ]
    manifest = {"license": "Apache-2.0", "payloads": {}, "cases": []}
    with tempfile.TemporaryDirectory(prefix="arca-matrix-") as temporary:
        root = Path(temporary)
        for name, data in payloads().items():
            path = root / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(data)
            os.chmod(path, 0o644)
            os.utime(path, (946684800, 946684800))
            manifest["payloads"][name] = {
                "size": len(data), "sha256": hashlib.sha256(data).hexdigest(),
            }
        for name, encoder, flags in cases:
            target = destination / name
            target.mkdir(parents=True, exist_ok=False)
            contents = payloads()
            if name == "rar5-dict64":
                contents = {"dictionary.bin": b"0123456789ABCDEF" * (33 * 1024 * 1024 // 16)}
                (root / "dictionary.bin").write_bytes(contents["dictionary.bin"])
                os.chmod(root / "dictionary.bin", 0o644)
            # Disable external configuration, timestamps, threading and extension sorting.
            args = ["a", "-cfg-", "-idq", "-mt1", "-ts-", "-ds", *flags,
                    str(target / "set.rar"), *contents]
            subprocess.run([str(encoder), *args], cwd=root, check=True)
            files = {p.name: hashlib.sha256(p.read_bytes()).hexdigest()
                     for p in sorted(target.iterdir())}
            first = "set.rar" if "set.rar" in files else next(iter(files))
            version = subprocess.run([str(encoder)], capture_output=True, text=True).stdout.splitlines()[1]
            manifest["cases"].append({
                "name": name, "producer": version.strip(),
                "encoder_sha256": hashlib.sha256(encoder.read_bytes()).hexdigest(),
                "command": ["rar", *args[:args.index(str(target / "set.rar"))], "set.rar", *contents],
                "payloads": {n: {"size": len(d), "sha256": hashlib.sha256(d).hexdigest()}
                             for n, d in contents.items()},
                "password": PASSWORD if any(f.startswith(("-p", "-hp")) for f in flags) else None,
                "first": first, "files": files,
            })
    (destination / "manifest.json").write_text(json.dumps(manifest, indent=2, ensure_ascii=True) + "\n")


if __name__ == "__main__":
    generate(Path(sys.argv[1]).resolve(), Path(sys.argv[2]).resolve(),
             Path(sys.argv[3]).resolve() if len(sys.argv) > 3 else
             Path(__file__).resolve().parent / "fixtures/matrix")
