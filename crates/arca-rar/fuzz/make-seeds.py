"""Pack unchanged Apache-2.0 Arca synthetic fixtures as bounded fuzz seeds."""
import hashlib
import json
from pathlib import Path
import struct

root = Path(__file__).resolve().parent
fixtures = root.parent / "tests/fixtures"
seeds = {}
for name, mode, password, sources in [
    ("stored", 0, 0, ["stored.rar"]),
    ("solid", 0, 0, ["solid.rar"]),
    ("content", 0, 1, ["encrypted.rar"]),
    ("headers", 0, 1, ["headers.rar"]),
    ("split", 0, 0, [f"volume.part{i}.rar" for i in range(1, 5)]),
    ("legacy", 2, 0, ["matrix/rar4-solid/set.rar"]),
    ("legacy-headers", 2, 1, ["matrix/rar4-headers/set.rar"]),
    ("legacy-split", 6, 1, [f"matrix/rar4-legacy-content/set.{ext}" for ext in ["rar", "r00", "r01"]]),
    ("solid-split", 4, 0, [f"matrix/rar5-solid-split/set.part{i}.rar" for i in range(1, 4)]),
    ("header-split", 4, 1, [f"matrix/rar5-headers-split/set.part{i}.rar" for i in range(1, 4)]),
]:
    data = bytes([mode, password])
    for source in sources:
        archive = (fixtures / source).read_bytes()
        data += struct.pack("<I", len(archive)) + archive
    (root / "corpus" / (name + ".case")).write_bytes(data)
    seeds[name + ".case"] = {"sources": sources, "sha256": hashlib.sha256(data).hexdigest()}
(root / "corpus/manifest.json").write_text(json.dumps({"license": "Apache-2.0", "seeds": seeds}, indent=2) + "\n")
