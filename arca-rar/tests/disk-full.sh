#!/bin/bash
# Linux-only: all mounts live in a new user/mount namespace and disappear on exit.
set -euo pipefail
if [ "${1:-}" = --inside ]; then
  test_binary=$2
  scratch=$(mktemp -d)
  trap 'umount "$scratch/full"; rm -rf "$scratch"' EXIT
  mkdir "$scratch/full" "$scratch/normal"
  mount -t tmpfs -o size=64k,mode=700 tmpfs "$scratch/full"
  export ARCA_RAR_ENOSPC="$scratch/full" ARCA_RAR_NORMAL="$scratch/normal" LC_ALL=C
  TMPDIR="$scratch/full" "$test_binary" --ignored --exact disk_full_staging_publishes_nothing
  TMPDIR="$scratch/normal" "$test_binary" --ignored --exact disk_full_publication_preserves_existing_target_and_cleans_temporary_file
else
  cd "$(dirname "$0")/../.."
  binary=$(cargo test -p arca-rar --features rar --test io_failures --no-run --message-format=json |
    python3 -c 'import json,sys; print(next(m["executable"] for line in sys.stdin if (m:=json.loads(line)).get("executable")))')
  exec unshare --user --map-root-user --mount bash arca-rar/tests/disk-full.sh --inside "$binary"
fi
