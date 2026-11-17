#!/usr/bin/env python3
"""Lays out Arca's icon in the freedesktop hicolor theme.

Usage: packaging/linux/icons.py <root>

Writes <root>/usr/share/icons/hicolor/<n>x<n>/apps/dev.beyondhumane.Arca.png
for the standard sizes, taken from brand/arca-monolito.ico (every frame there
is already a PNG, so nothing is redrawn or resampled), and the scalable SVG
from brand/arca-monolito-app-claro.svg, the light tile the .ico was made from.
"""

import shutil
import struct
import sys
from pathlib import Path

NAME = "dev.beyondhumane.Arca"
SIZES = (16, 24, 32, 48, 64, 128, 256)
BRAND = Path(__file__).resolve().parents[2] / "brand"
PNG = b"\x89PNG\r\n\x1a\n"


def frames(ico: bytes) -> dict[int, bytes]:
    reserved, kind, count = struct.unpack_from("<HHH", ico, 0)
    if reserved != 0 or kind != 1:
        raise SystemExit("not an .ico file")
    found = {}
    for index in range(count):
        width, height, _, _, _, _, size, offset = struct.unpack_from(
            "<BBBBHHII", ico, 6 + 16 * index
        )
        width, height = width or 256, height or 256
        data = ico[offset : offset + size]
        if width == height and len(data) == size and data.startswith(PNG):
            found[width] = data
    return found


def main() -> None:
    if len(sys.argv) != 2:
        raise SystemExit(__doc__)
    hicolor = Path(sys.argv[1]) / "usr/share/icons/hicolor"
    found = frames((BRAND / "arca-monolito.ico").read_bytes())
    missing = [size for size in SIZES if size not in found]
    if missing:
        raise SystemExit(f"arca-monolito.ico has no PNG frame for {missing}")
    for size in SIZES:
        target = hicolor / f"{size}x{size}/apps/{NAME}.png"
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes(found[size])
    scalable = hicolor / f"scalable/apps/{NAME}.svg"
    scalable.parent.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(BRAND / "arca-monolito-app-claro.svg", scalable)


if __name__ == "__main__":
    main()
