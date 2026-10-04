"""Regenerate the tiny external fixtures with 7z (tested with p7zip 16.02)."""
from pathlib import Path
import shutil
import subprocess
import tempfile

DEST = Path(__file__).resolve().parent

with tempfile.TemporaryDirectory() as temp:
    root = Path(temp)
    (root / "first.txt").write_bytes(b"first contents\n" * 8)
    (root / "second.txt").write_bytes(b"second contents\n" * 8)
    (root / "empty").touch()
    (root / "empty-dir").mkdir()

    def seven(*args):
        subprocess.run(["7z", *args], cwd=root, check=True, stdout=subprocess.DEVNULL)

    for name, extra in [
        ("solid", []),
        ("encrypted", ["-psecret"]),
        ("hidden", ["-psecret", "-mhe=on"]),
    ]:
        seven("a", "-t7z", "-m0=LZMA2", "-ms=on", name + ".7z",
              "first.txt", "second.txt", "empty", "empty-dir", *extra)
        shutil.copyfile(root / (name + ".7z"), DEST / (name + ".7z"))

    seven("a", "-t7z", "mixed.7z", "first.txt", "empty", "empty-dir")
    seven("a", "-t7z", "-psecret", "-mhe=off", "mixed.7z", "second.txt")
    shutil.copyfile(root / "mixed.7z", DEST / "mixed.7z")

    seven("a", "-t7z", "unsafe.7z", "first.txt")
    seven("rn", "unsafe.7z", "first.txt", "../escape")
    shutil.copyfile(root / "unsafe.7z", DEST / "unsafe.7z")
