#!/bin/bash
# F01 acceptance criterion: interoperability verified by hash.
# Resolved from the script location, not from an absolute path: the one that
# used to be here belonged to the container this was written in, and exists
# neither in CI nor in a plain clone. Override it with ARCA=... bash interop.sh
ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
ARCA=${ARCA:-$ROOT/target/release/arca}
if [ ! -x "$ARCA" ]; then
  echo "binary not found at $ARCA (build it with: cargo build --release)" >&2
  exit 1
fi
for tool in zip unzip tar 7z python3; do
  command -v "$tool" >/dev/null 2>&1 || { echo "required tool not found: $tool" >&2; exit 1; }
done
W=$(mktemp -d "${TMPDIR:-/tmp}/arca-interop.XXXXXX") || exit 1
trap 'rm -rf "$W"' EXIT
mkdir -p "$W/src" "$W/out"; cd "$W" || exit 1
OK=0; KO=0
ok(){ printf "  \033[32mOK\033[0m   %s\n" "$1"; OK=$((OK+1)); }
ko(){ printf "  \033[31mFALLO\033[0m %s\n" "$1"; KO=$((KO+1)); }

# Corpus: text, binary, one large file and nested paths
mkdir -p src/a/b/c
head -c 3000000 /usr/share/doc/*/changelog* 2>/dev/null > src/text.txt || head -c 3000000 /dev/urandom > src/text.txt
head -c 5000000 /usr/bin/python3.11 > src/binary.bin 2>/dev/null || head -c 5000000 /dev/urandom > src/binary.bin
head -c 200 /dev/urandom > src/a/b/c/deep.dat
printf 'line\n%.0s' {1..50000} > src/repetitive.txt
REF=$(cd src && find . -type f | sort | xargs sha256sum | sha256sum | cut -d' ' -f1)
echo "  corpus: $(du -sh src|cut -f1), reference hash ${REF:0:16}"
echo

echo "A) Arca writes -> system tools read"
for niv in store fast normal best; do
  $ARCA create out/a-$niv.zip src -l $niv >/dev/null 2>&1 || { ko "arca create -l $niv"; continue; }
  unzip -tqq out/a-$niv.zip >/dev/null 2>&1 && ok "unzip -t accepts the zip (-l $niv)" || ko "unzip -t rejects the zip (-l $niv)"
  rm -rf x; mkdir x; unzip -qq out/a-$niv.zip -d x 2>/dev/null
  H=$(cd x/src && find . -type f | sort | xargs sha256sum | sha256sum | cut -d' ' -f1)
  [ "$H" = "$REF" ] && ok "unzip returns identical bytes (-l $niv)" || ko "unzip returns different data (-l $niv)"
done
$ARCA create out/a.tar src >/dev/null 2>&1
rm -rf x; mkdir x; tar xf out/a.tar -C x 2>/dev/null
H=$(cd x/src && find . -type f | sort | xargs sha256sum | sha256sum | cut -d' ' -f1)
[ "$H" = "$REF" ] && ok "system tar reads Arca's .tar" || ko "system tar fails on Arca's .tar"
$ARCA create out/a.tar.gz src >/dev/null 2>&1
rm -rf x; mkdir x; tar xzf out/a.tar.gz -C x 2>/dev/null
H=$(cd x/src && find . -type f | sort | xargs sha256sum | sha256sum | cut -d' ' -f1)
[ "$H" = "$REF" ] && ok "system tar reads Arca's .tar.gz" || ko "system tar fails on Arca's .tar.gz"
7z t out/a-normal.zip >/dev/null 2>&1 && ok "7-Zip verifies Arca's zip" || ko "7-Zip rejects Arca's zip"

echo
echo "B) System tools write -> Arca reads"
zip -qr out/z-def.zip src
zip -qr0 out/z-store.zip src
zip -q9r out/z-9.zip src
tar cf out/t.tar src
tar czf out/t.tar.gz src
for f in z-def z-store z-9; do
  rm -rf y; mkdir y
  $ARCA extract out/$f.zip -o y >/dev/null 2>&1 || { ko "arca extract $f.zip"; continue; }
  H=$(cd y/src && find . -type f | sort | xargs sha256sum | sha256sum | cut -d' ' -f1)
  [ "$H" = "$REF" ] && ok "Arca extracts zip($f) without losing a byte" || ko "Arca mis-extracts $f.zip"
done
for f in t.tar t.tar.gz; do
  rm -rf y; mkdir y
  $ARCA extract out/$f -o y >/dev/null 2>&1 || { ko "arca extract $f"; continue; }
  H=$(cd y/src && find . -type f | sort | xargs sha256sum | sha256sum | cut -d' ' -f1)
  [ "$H" = "$REF" ] && ok "Arca extracts tar's $f without losing a byte" || ko "Arca mis-extracts $f"
done
7z a -tzip -mx5 out/s7.zip src >/dev/null 2>&1
rm -rf y; mkdir y; $ARCA extract out/s7.zip -o y >/dev/null 2>&1
H=$(cd y/src && find . -type f|sort|xargs sha256sum|sha256sum|cut -d' ' -f1)
[ "$H" = "$REF" ] && ok "Arca reads the zip created by 7-Zip" || ko "Arca fails on 7-Zip's zip"

echo
echo "C) AES-256, both directions"
# ASCII on purpose: Git Bash mangles a non-ASCII argument on its way to a
# native .exe, and 7-Zip cannot even create the archive. That is the shell, not
# the format, so the non-ASCII case is covered by a Rust test instead.
PW='una clave con espacios'
$ARCA create out/enc.zip src -p "$PW" >/dev/null 2>&1
7z t -p"$PW" out/enc.zip >/dev/null 2>&1 && ok "7-Zip verifies Arca's encrypted zip" || ko "7-Zip rejects Arca's encrypted zip"
7z t -p"wrong" out/enc.zip >/dev/null 2>&1 && ko "7-Zip accepts a wrong password" || ok "7-Zip rejects the wrong password"
rm -rf y; mkdir y
7z x -y -p"$PW" -o"y" out/enc.zip >/dev/null 2>&1
H=$(cd y/src && find . -type f|sort|xargs sha256sum|sha256sum|cut -d' ' -f1)
[ "$H" = "$REF" ] && ok "7-Zip decrypts Arca's zip without losing a byte" || ko "7-Zip mis-decrypts Arca's zip"

7z a -tzip -mem=AES256 -p"$PW" out/enc7.zip src >/dev/null 2>&1
rm -rf y; mkdir y
$ARCA extract out/enc7.zip -o y -p "$PW" >/dev/null 2>&1
H=$(cd y/src && find . -type f|sort|xargs sha256sum|sha256sum|cut -d' ' -f1)
[ "$H" = "$REF" ] && ok "Arca decrypts 7-Zip's AES-256 zip" || ko "Arca fails on 7-Zip's AES-256 zip"
$ARCA test out/enc.zip -p "wrong" >/dev/null 2>&1 && ko "Arca accepts a wrong password" || ok "Arca rejects the wrong password"
$ARCA test out/enc.zip >/dev/null 2>&1 && ko "Arca extracts without the password" || ok "Arca asks for the password"
# A flipped byte in the ciphertext must fail the HMAC, not come out as content.
cp out/enc.zip out/enc-bad.zip
printf '\x5A' | dd of=out/enc-bad.zip bs=1 seek=90 conv=notrunc 2>/dev/null
$ARCA test out/enc-bad.zip -p "$PW" >/dev/null 2>&1 && ko "altered ciphertext went unnoticed" || ok "altered ciphertext fails its authentication code"

echo
echo "D) Adding and removing the password of an existing archive"
cp out/enc.zip out/off.zip
$ARCA password out/off.zip -p "$PW" >/dev/null 2>&1
7z t out/off.zip >/dev/null 2>&1 && ok "7-Zip opens it with no password at all" || ko "7-Zip still asks for a password"
rm -rf y; mkdir y
7z x -y -o"y" out/off.zip >/dev/null 2>&1
H=$(cd y/src && find . -type f|sort|xargs sha256sum|sha256sum|cut -d' ' -f1)
[ "$H" = "$REF" ] && ok "taking the password off keeps every byte" || ko "content changed when the password came off"

# The same, starting from an archive 7-Zip encrypted.
cp out/enc7.zip out/off7.zip
$ARCA password out/off7.zip -p "$PW" >/dev/null 2>&1
rm -rf y; mkdir y
$ARCA extract out/off7.zip -o y >/dev/null 2>&1
H=$(cd y/src && find . -type f|sort|xargs sha256sum|sha256sum|cut -d' ' -f1)
[ "$H" = "$REF" ] && ok "the password comes off a 7-Zip archive too" || ko "7-Zip archive broke when the password came off"

# And back on, on an archive that never had one.
cp out/a-normal.zip out/on.zip
$ARCA password out/on.zip --new "$PW" >/dev/null 2>&1
7z t out/on.zip >/dev/null 2>&1 && ko "it opened without the password that was just set" || ok "without the password it no longer opens"
rm -rf y; mkdir y
7z x -y -p"$PW" -o"y" out/on.zip >/dev/null 2>&1
H=$(cd y/src && find . -type f|sort|xargs sha256sum|sha256sum|cut -d' ' -f1)
[ "$H" = "$REF" ] && ok "7-Zip opens what Arca has just encrypted" || ko "7-Zip cannot read the archive Arca encrypted"

# The archive is the only copy of the data. A wrong password must not touch it.
cp out/enc.zip out/keep.zip
BEFORE=$(sha256sum out/keep.zip | cut -d' ' -f1)
$ARCA password out/keep.zip -p "not the password" >/dev/null 2>&1
AFTER=$(sha256sum out/keep.zip | cut -d' ' -f1)
[ "$BEFORE" = "$AFTER" ] && ok "a wrong password leaves the archive untouched" || ko "THE ARCHIVE WAS DAMAGED BY A WRONG PASSWORD"
ls out/*.arca-new >/dev/null 2>&1 && ko "a half written temporary file was left behind" || ok "no temporary file left behind"

echo
echo "E) Corruption detection"
cp out/a-normal.zip out/corrupt.zip
printf '\xDE\xAD' | dd of=out/corrupt.zip bs=1 seek=200 conv=notrunc 2>/dev/null
$ARCA test out/corrupt.zip >/dev/null 2>&1 && ko "corrupt zip not detected" || ok "corruption detected, exits with an error"
$ARCA test out/a-normal.zip >/dev/null 2>&1 && ok "accepts the intact archive" || ko "rejects a valid archive"

echo
echo "F) Security: Zip Slip"
# Built and checked relative to the working directory, both here and in the
# entry name. Anchored at /tmp this tested nothing on Windows: Git Bash puts
# /tmp under AppData while a native python3 reads it as C:\tmp, so the fixture
# was never written, the extraction failed for want of a file, no escaped file
# appeared, and a security control reported a pass having run nothing. An
# escape one level up lands somewhere both platforms can name.
python3 - <<'PY'
import zipfile
z = zipfile.ZipFile('out/slip.zip', 'w')
z.writestr('../PWNED', 'malicious')
z.close()
PY
if [ ! -s out/slip.zip ]; then
  ko "the zip slip archive was not built, this control tested nothing"
else
  rm -rf y; mkdir y
  $ARCA extract out/slip.zip -o y >/dev/null 2>&1 && ko "extracted an archive that escapes the destination" || ok "rejects the path escaping the destination"
  # Beside the destination and not inside it, which is where the entry aimed.
  if [ -f PWNED ]; then ko "WROTE OUTSIDE THE DESTINATION"; rm -f PWNED; else ok "nothing was written outside the destination"; fi
fi

echo
echo "G) 7z: Arca writes, 7-Zip verifies and extracts"
python3 - <<'PY'
from pathlib import Path
root = Path('src7')
(root / 'nested' / 'empty-dir').mkdir(parents=True)
(root / 'empty').touch()
(root / 'first.txt').write_bytes(b'first solid contents\n' * 2000)
(root / 'second.txt').write_bytes(b'second solid contents\n' * 2000)
(root / 'nested' / '\u00f1-\U0001f680.txt').write_bytes(bytes(range(256)) * 16)
PY
compare_trees() {
  python3 - "$1" "$2" <<'PY'
from pathlib import Path
import sys
def tree(name):
    root = Path(name)
    if not root.is_dir():
        raise RuntimeError(f'missing extracted directory: {root}')
    return {str(p.relative_to(root)): None if p.is_dir() else p.read_bytes()
            for p in root.rglob('*')}
sys.exit(0 if tree(sys.argv[1]) == tree(sys.argv[2]) else 1)
PY
}
unchanged_wrong_password() {
  # Check contents, directory entries and mtimes, not just whether extraction failed.
  python3 - "$ARCA" "$1" <<'PY'
from pathlib import Path
import subprocess
import sys
import tempfile

def snapshot(root):
    return {str(p.relative_to(root)): (p.stat().st_mtime_ns, None if p.is_dir() else p.read_bytes())
            for p in [root, *root.rglob('*')]}

with tempfile.TemporaryDirectory(dir='.') as room:
    root = Path(room)
    dest = root / 'existing'
    (dest / 'src7').mkdir(parents=True)
    (dest / 'src7' / 'first.txt').write_bytes(b'keep this file')
    (dest / 'src7' / 'empty').write_bytes(b'do not truncate')
    for policy in ['overwrite', 'skip', 'rename']:
        for output in [dest, root / 'absent' / 'nested']:
            before = snapshot(root)
            result = subprocess.run([sys.argv[1], 'extract', sys.argv[2], '-p', 'wrong',
                                     '-o', str(output), '--on-conflict', policy], capture_output=True)
            assert result.returncode == 1, result
            assert b'password' in result.stderr.lower(), result.stderr
            assert snapshot(root) == before, 'destination changed after wrong password'
PY
}

for mode in plain visible hidden; do
  arca_pw=(); seven_pw=(); hide=()
  if [ "$mode" != plain ]; then arca_pw=(-p "$PW"); seven_pw=("-p$PW"); fi
  if [ "$mode" = hidden ]; then hide=(--hide-names); fi
  for level in store fast normal best; do
    archive="out/arca-$mode-$level.7z"
    if ! "$ARCA" create "$archive" src7 -l "$level" "${arca_pw[@]}" "${hide[@]}" >/dev/null 2>&1; then
      ko "Arca creates 7z ($mode, $level)"; continue
    fi
    7z t "$archive" "${seven_pw[@]}" </dev/null >/dev/null 2>&1 && ok "7-Zip verifies Arca 7z ($mode, $level)" || ko "7-Zip rejects Arca 7z ($mode, $level)"
    rm -rf x7
    if 7z x -y -ox7 "$archive" "${seven_pw[@]}" </dev/null >/dev/null 2>&1 && compare_trees src7 x7/src7; then
      ok "7-Zip extracts identical bytes and empty directories ($mode, $level)"
    else
      ko "7-Zip extraction differs ($mode, $level)"
    fi
    if [ "$mode" != plain ]; then
      unchanged_wrong_password "$archive" && ok "wrong password leaves destinations untouched ($mode, $level)" || ko "wrong password touched destinations ($mode, $level)"
    fi
  done
done

echo
echo "H) 7z: external solid and mixed encrypted/plain archives"
for mode in plain visible hidden; do
  arca_pw=(); seven_pw=(); hide=()
  if [ "$mode" != plain ]; then arca_pw=(-p "$PW"); seven_pw=("-p$PW"); fi
  if [ "$mode" = hidden ]; then hide=(-mhe=on); fi
  archive="out/seven-$mode.7z"
  if ! 7z a -t7z -m0=LZMA2 -ms=on "$archive" src7 "${seven_pw[@]}" "${hide[@]}" </dev/null >/dev/null 2>&1; then
    ko "7-Zip creates solid $mode archive"; continue
  fi
  7z l -slt "$archive" "${seven_pw[@]}" </dev/null | grep -q 'Solid = +' && ok "external $mode input is actually solid" || ko "external $mode input is not solid"
  7z t "$archive" "${seven_pw[@]}" </dev/null >/dev/null 2>&1 && ok "7-Zip verifies external $mode input" || ko "external $mode fixture is invalid"
  "$ARCA" list "$archive" "${arca_pw[@]}" >/dev/null 2>&1 && ok "Arca lists external $mode input" || ko "Arca cannot list external $mode input"
  "$ARCA" test "$archive" "${arca_pw[@]}" >/dev/null 2>&1 && ok "Arca tests external $mode input" || ko "Arca cannot test external $mode input"
  rm -rf y7
  if "$ARCA" extract "$archive" -o y7 "${arca_pw[@]}" >/dev/null 2>&1 && compare_trees src7 y7/src7; then
    ok "Arca extracts identical solid bytes and Unicode paths ($mode)"
  else
    ko "Arca solid extraction differs ($mode)"
  fi
  if [ "$mode" = hidden ]; then
    "$ARCA" list "$archive" >/dev/null 2>&1 && ko "hidden names visible without password" || ok "hidden names require password"
  else
    "$ARCA" list "$archive" >/dev/null 2>&1 && ok "visible names list without password ($mode)" || ko "cannot list visible names ($mode)"
  fi
  if [ "$mode" != plain ]; then
    unchanged_wrong_password "$archive" && ok "wrong password leaves solid destinations untouched ($mode)" || ko "wrong password touched solid destinations ($mode)"
  fi
done
if 7z a -t7z out/mixed.7z src7/first.txt src7/empty >/dev/null 2>&1 &&
   7z a -t7z -p"$PW" -mhe=off out/mixed.7z src7/second.txt >/dev/null 2>&1; then
  7z l -slt out/mixed.7z > out/mixed-list.txt
  if grep -q 'Encrypted = +' out/mixed-list.txt && grep -q 'Encrypted = -' out/mixed-list.txt; then
    unchanged_wrong_password out/mixed.7z && ok "mixed plain/encrypted archive leaves destinations untouched" || ko "mixed archive touched destinations on wrong password"
  else
    ko "mixed fixture does not contain both encryption modes"
  fi
else
  ko "could not create mixed plain/encrypted fixture"
fi

echo
if [ "${ARCA_TEST_RAR:-1}" = 1 ]; then
  echo "RAR) External fixtures -> Arca (read-only)"
  FIXTURES="$ROOT/arca-rar/tests/fixtures"
  for f in plain stored solid encrypted headers; do
    ARGS=()
    case "$f" in encrypted|headers) ARGS=(-p arca-test-only);; esac
    rm -rf rar-out
    "$ARCA" test "$FIXTURES/$f.rar" "${ARGS[@]}" >/dev/null 2>&1 &&
      "$ARCA" extract "$FIXTURES/$f.rar" -o rar-out "${ARGS[@]}" >/dev/null 2>&1 &&
      python3 -c 'from pathlib import Path; p=Path("rar-out"); assert (p/"first.txt").read_bytes()==b"Arca RAR fixture alpha\n"*64; assert (p/"folder/second.txt").read_bytes()==b"Arca RAR fixture beta\n"*64' &&
      ok "Arca reads official RAR fixture $f" || ko "RAR fixture $f"
    if [ -n "${UNRAR:-}" ]; then
      rm -rf unrar-out; mkdir unrar-out
      "$UNRAR" x -idq -o+ -parca-test-only "$FIXTURES/$f.rar" unrar-out/ &&
        diff -r rar-out unrar-out &&
        ok "Arca and UnRAR produce identical $f files" || ko "UnRAR comparison $f"
    fi
  done
  rm -rf rar-bad-password
  "$ARCA" extract "$FIXTURES/encrypted.rar" -o rar-bad-password -p wrong >/dev/null 2>&1 &&
    ko "RAR wrong password accepted" || ok "RAR wrong password rejected"
  [ ! -e rar-bad-password ] && ok "RAR failure publishes nothing" || ko "RAR failure touched destination"
  for part in 1 4; do
    rm -rf rar-out
    "$ARCA" test "$FIXTURES/volume.part$part.rar" >/dev/null 2>&1 &&
      "$ARCA" extract "$FIXTURES/volume.part$part.rar" -o rar-out >/dev/null 2>&1 &&
      python3 -c 'from pathlib import Path; p=Path("rar-out"); assert (p/"first.txt").read_bytes()==b"Arca RAR fixture alpha\n"*64; assert (p/"folder/second.txt").read_bytes()==b"Arca RAR fixture beta\n"*64' &&
      ok "RAR complete set resolved from volume $part" || ko "RAR volume $part resolution"
  done
  ARCA="$ARCA" python3 "$ROOT/arca-rar/tests/compare-matrix.py" &&
    ok "RAR independent generation/volume/dictionary matrix" || ko "RAR independent matrix"
  echo
fi
echo "-------------------------------------------"
echo "  $OK passed, $KO failed"
[ $KO -eq 0 ] || exit 1
