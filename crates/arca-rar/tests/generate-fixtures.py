"""Generate RAR5 test data with an installed, licensed/evaluation RAR executable."""

import pathlib
import subprocess
import sys
import tempfile

rar = pathlib.Path(sys.argv[1]).resolve()
destination = pathlib.Path(__file__).resolve().parent / "fixtures"
with tempfile.TemporaryDirectory(prefix="arca-rar-fixtures-") as directory:
    root = pathlib.Path(directory)
    (root / "folder").mkdir()
    (root / "empty").mkdir()
    (root / "first.txt").write_bytes(b"Arca RAR fixture alpha\n" * 64)
    (root / "folder/second.txt").write_bytes(b"Arca RAR fixture beta\n" * 64)
    for name, flags in [
        ("plain.rar", []),
        ("stored.rar", ["-m0"]),
        ("solid.rar", ["-s"]),
        ("encrypted.rar", ["-parca-test-only"]),
        ("headers.rar", ["-hparca-test-only", "-s"]),
        ("volume.rar", ["-m0", "-v1k"]),
    ]:
        subprocess.run(
            [str(rar), "a", "-idq", "-r", "-ma5", *flags, str(destination / name),
             "first.txt", "folder", "empty"],
            cwd=root, check=True,
        )
