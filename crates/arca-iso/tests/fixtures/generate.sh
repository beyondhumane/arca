#!/usr/bin/env bash
# Regenerates the ISO fixtures. Needs genisoimage and xorriso.
set -euo pipefail
cd "$(dirname "$0")"
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

src="$work/src"
mkdir -p "$src/docs/deep/er" "$src/empty"
printf 'hello from arca\n' > "$src/readme.txt"
printf 'a file with a much longer name than plain ISO 9660 allows\n' > "$src/docs/A long name with spaces.txt"
printf 'nested\n' > "$src/docs/deep/er/leaf.bin"
printf 'ñandú\n' > "$src/docs/año.txt"
: > "$src/zero"
ln -s readme.txt "$src/link-to-readme"

stamp=2024010203040500
genisoimage -quiet -no-pad -V PLAIN -o plain.iso "$src" 2>/dev/null
genisoimage -quiet -no-pad -V JOLIET -J -joliet-long -o joliet.iso "$src" 2>/dev/null
xorriso -as mkisofs -quiet -no-pad -V ROCKRIDGE -R -J --modification-date="$stamp" -o rockridge.iso "$src" 2>/dev/null
