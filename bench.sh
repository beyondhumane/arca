#!/bin/bash
# Reproduces the Linux figures in README.md and docs/guide/benchmarks.md.
# Needs hyperfine, zip, unzip and 7zz on PATH, for example:
#   cargo build --release
#   nix-shell -p hyperfine zip unzip _7zz --run 'bash bench.sh'
# The corpus is the Silesia corpus, pinned by hash so every run measures the
# same bytes. Results go to /tmp/arca-bench/*.md as hyperfine Markdown tables.
set -euo pipefail
ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
ARCA=${ARCA:-$ROOT/target/release/arca}
if [ ! -x "$ARCA" ]; then
  echo "binary not found at $ARCA (build it with: cargo build --release)" >&2
  exit 1
fi
for t in hyperfine zip unzip 7zz; do
  command -v $t >/dev/null || { echo "missing tool: $t" >&2; exit 1; }
done

W=/tmp/arca-bench; mkdir -p $W; cd $W
SILESIA_SHA=0626e25f45c0ffb5dc801f13b7c82a3b75743ba07e3a71835a41e3d9f63c77af
if ! echo "$SILESIA_SHA  silesia.zip" | sha256sum -c --status 2>/dev/null; then
  curl -fsSL -o silesia.zip https://sun.aei.polsl.pl/~sdeor/corpus/silesia.zip
  echo "$SILESIA_SHA  silesia.zip" | sha256sum -c --quiet
fi
# The same bytes cut into 120 equal files: Silesia's own twelve are dominated by
# one 49 MB file, and a single file only ever gets one thread.
rm -rf silesia corpus small out; mkdir silesia corpus small
unzip -qq silesia.zip -d silesia
cat silesia/* | split -n 120 -d -a 3 - corpus/part
for i in $(seq 1 6000); do printf 'entry %d\n' $i > small/f$i.txt; done
"$ARCA" create tiny.zip small/f1.txt >/dev/null
"$ARCA" create list-6000.zip small >/dev/null

echo "== machine"
lscpu | grep 'Model name' | sed 's/  */ /g'
echo "$(nproc) threads, $("$ARCA" --version), $(7zz | sed -n 2p | cut -c1-24)"

echo "== R1 cold start: open and list a one-entry archive"
hyperfine -N --warmup 20 --export-markdown r1.md \
  "$ARCA list tiny.zip" "unzip -l tiny.zip" "7zz l tiny.zip"

echo "== R2 list 6000 entries"
hyperfine -N --warmup 5 --export-markdown r2.md \
  "$ARCA list list-6000.zip" "unzip -l list-6000.zip" "7zz l list-6000.zip"

# zstd spreads one entry over every core it can see whatever -j says, so the
# two-thread runs are pinned to two physical cores to make "2 threads" true.
TWO=$(lscpu -p=CPU,CORE | grep -v '^#' | sort -t, -k2,2n -u | head -2 | cut -d, -f1 | paste -sd,)
P="taskset -c $TWO"

echo "== R3 scaling: Silesia in 120 files, $(du -sh corpus | cut -f1), deflate, cores $TWO"
hyperfine -N --warmup 1 --runs 5 --prepare 'rm -f out.zip' --export-markdown r3.md \
  "$P $ARCA create out.zip corpus -c deflate -j 1" \
  "$P $ARCA create out.zip corpus -c deflate -j 2"

echo "== compression: same corpus, same two cores"
hyperfine -N --warmup 1 --runs 5 --prepare 'rm -f out.zip' --export-markdown compress.md \
  "$P $ARCA create out.zip corpus -c zstd" \
  "$P $ARCA create out.zip corpus -c deflate" \
  "$P zip -qr -6 out.zip corpus" \
  "$P 7zz a -tzip -mx5 out.zip corpus"

echo "== sizes (bytes)"
size() { rm -f out.zip; $P "$@" >/dev/null; stat -c %s out.zip; }
echo "arca zstd    $(size "$ARCA" create out.zip corpus -c zstd)"
echo "arca deflate $(size "$ARCA" create out.zip corpus -c deflate)"
echo "zip -6       $(size zip -qr -6 out.zip corpus)"
echo "7zz -mx5     $(size 7zz a -tzip -mx5 out.zip corpus)"
