#!/usr/bin/env bash
# Builds the AppImage, .deb and .rpm for one architecture from the release
# binaries.
#
# Usage: packaging/linux/build.sh <version> <x86_64|aarch64> <bin-dir> <out-dir>
#
# The tools are fetched at fixed versions and checked against the digests
# below, so a moved tag or a swapped asset upstream stops the build here.
# Nothing is bundled into the AppImage beyond the two binaries: glibc, Vulkan,
# Wayland, X11 and the GPU driver come from the system, as with the tarball.
set -euo pipefail

VERSION=$1 ARCH=$2 BIN=$(realpath "$3") OUT=$4
HERE=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
ROOT=$(cd "$HERE/../.." && pwd)
mkdir -p "$OUT"
OUT=$(realpath "$OUT")
WORK=$(mktemp -d)
trap 'rm -rf "$WORK"' EXIT

APPIMAGETOOL=1.9.1
RUNTIME=20251108
NFPM=2.47.0
case "$ARCH" in
  x86_64)
    NAME=x86_64 GOARCH=amd64
    APPIMAGETOOL_SHA=ed4ce84f0d9caff66f50bcca6ff6f35aae54ce8135408b3fa33abfc3cb384eb0
    RUNTIME_SHA=2fca8b443c92510f1483a883f60061ad09b46b978b2631c807cd873a47ec260d
    NFPM_FILE=nfpm_${NFPM}_Linux_x86_64.tar.gz
    NFPM_SHA=0660ca602b2d2d2ae4781a06c692b3eeb9d437ffea05b831d76e41f4a3188783
    ;;
  aarch64)
    NAME=arm64 GOARCH=arm64
    APPIMAGETOOL_SHA=f0837e7448a0c1e4e650a93bb3e85802546e60654ef287576f46c71c126a9158
    RUNTIME_SHA=00cbdfcf917cc6c0ff6d3347d59e0ca1f7f45a6df1a428a0d6d8a78664d87444
    NFPM_FILE=nfpm_${NFPM}_Linux_arm64.tar.gz
    NFPM_SHA=1c0f5f2999b9a974bfb04fdb0cc3306096de530ac5dbb25d739cc5f5219c919c
    ;;
  *) echo "unknown architecture: $ARCH" >&2; exit 1 ;;
esac

fetch() { # url sha256 file
  curl -fsSL --retry 3 -o "$WORK/$3" "$1"
  echo "$2  $WORK/$3" | sha256sum -c --quiet -
}
fetch "https://github.com/AppImage/appimagetool/releases/download/$APPIMAGETOOL/appimagetool-$ARCH.AppImage" "$APPIMAGETOOL_SHA" appimagetool
fetch "https://github.com/AppImage/type2-runtime/releases/download/$RUNTIME/runtime-$ARCH" "$RUNTIME_SHA" runtime
fetch "https://github.com/goreleaser/nfpm/releases/download/v$NFPM/$NFPM_FILE" "$NFPM_SHA" nfpm.tar.gz
chmod +x "$WORK/appimagetool"
tar -xzf "$WORK/nfpm.tar.gz" -C "$WORK" nfpm

# The files both formats share, laid out as they end up on disk.
STAGE="$WORK/stage"
install -Dm755 "$BIN/arca" "$STAGE/usr/bin/arca"
install -Dm755 "$BIN/arca-gui" "$STAGE/usr/bin/arca-gui"
install -Dm644 "$HERE/arca.desktop" "$STAGE/usr/share/applications/arca.desktop"
install -Dm644 "$ROOT/brand/arca-monolito-256.png" "$STAGE/usr/share/icons/hicolor/256x256/apps/arca.png"
install -Dm644 "$ROOT/LICENSE" "$STAGE/usr/share/licenses/arca/LICENSE"
install -Dm644 "$ROOT/README.md" "$STAGE/usr/share/doc/arca/README.md"

# AppImage. Runners have no FUSE, so appimagetool unpacks itself to run.
APPDIR="$WORK/Arca.AppDir"
cp -a "$STAGE" "$APPDIR"
install -m755 "$HERE/AppRun" "$APPDIR/AppRun"
cp "$HERE/arca.desktop" "$APPDIR/arca.desktop"
cp "$ROOT/brand/arca-monolito-256.png" "$APPDIR/arca.png"
ln -s arca.png "$APPDIR/.DirIcon"
APPIMAGE_EXTRACT_AND_RUN=1 ARCH=$ARCH VERSION=$VERSION "$WORK/appimagetool" \
  --no-appstream --runtime-file "$WORK/runtime" \
  "$APPDIR" "$OUT/arca-v$VERSION-linux-$NAME.AppImage"

# .deb and .rpm, named the way apt and dnf users expect.
for packager in deb rpm; do
  (cd "$STAGE" && VERSION=$VERSION GOARCH=$GOARCH \
    "$WORK/nfpm" package --config "$HERE/nfpm.yaml" --packager "$packager" --target "$OUT/")
done
ls -la "$OUT"
