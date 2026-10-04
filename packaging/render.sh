#!/bin/bash
# Writes the Homebrew formula, the Scoop manifest and the AUR PKGBUILD for one
# release, from the checksums the Release workflow publishes.
#
# Usage:  packaging/render.sh VERSION SHA256SUMS.txt OUTDIR

set -euo pipefail

VERSION=$1
SUMS=$2
OUT=$3
REPO=${GITHUB_REPOSITORY:-beyondhumane/arca}
BASE="https://github.com/$REPO/releases/download/v$VERSION"
DESC="Fast, safe archive manager"
HOME_URL="https://arca.beyondhumane.com"

sum() {
  local file="arca-v$VERSION-$1"
  local hash
  hash=$(awk -v f="$file" '$2 == f || $2 == "*" f { print $1 }' "$SUMS")
  [ -n "$hash" ] || { echo "no checksum for $file in $SUMS" >&2; exit 1; }
  echo "$hash"
}

mkdir -p "$OUT/homebrew" "$OUT/scoop" "$OUT/aur"

cat > "$OUT/homebrew/arca.rb" <<END
class Arca < Formula
  desc "$DESC"
  homepage "$HOME_URL"
  version "$VERSION"
  license "Apache-2.0"

  on_macos do
    on_arm do
      url "$BASE/arca-v$VERSION-macos-arm64.tar.gz"
      sha256 "$(sum macos-arm64.tar.gz)"
    end
    on_intel do
      url "$BASE/arca-v$VERSION-macos-x86_64.tar.gz"
      sha256 "$(sum macos-x86_64.tar.gz)"
    end
  end

  on_linux do
    on_arm do
      url "$BASE/arca-v$VERSION-linux-arm64.tar.gz"
      sha256 "$(sum linux-arm64.tar.gz)"
    end
    on_intel do
      url "$BASE/arca-v$VERSION-linux-x86_64.tar.gz"
      sha256 "$(sum linux-x86_64.tar.gz)"
    end
  end

  def install
    bin.install "arca", "arca-gui"
    pkgshare.install "RAR-NOTICES.txt"
  end

  test do
    assert_match version.to_s, shell_output("#{bin}/arca --version")
  end
end
END

cat > "$OUT/scoop/arca.json" <<END
{
  "version": "$VERSION",
  "description": "$DESC",
  "homepage": "$HOME_URL",
  "license": "Apache-2.0",
  "architecture": {
    "64bit": {
      "url": "$BASE/arca-v$VERSION-windows-x86_64.zip",
      "hash": "$(sum windows-x86_64.zip)",
      "extract_dir": "arca-v$VERSION-windows-x86_64"
    }
  },
  "bin": "arca.exe",
  "shortcuts": [["arca-gui.exe", "Arca"]],
  "checkver": "github",
  "autoupdate": {
    "architecture": {
      "64bit": {
        "url": "https://github.com/$REPO/releases/download/v\$version/arca-v\$version-windows-x86_64.zip",
        "extract_dir": "arca-v\$version-windows-x86_64"
      }
    },
    "hash": {
      "url": "https://github.com/$REPO/releases/download/v\$version/SHA256SUMS.txt"
    }
  }
}
END

# arca-gui links libxcb and libxkbcommon(-x11); Wayland is loaded at runtime.
X86=$(sum linux-x86_64.tar.gz)
ARM=$(sum linux-arm64.tar.gz)
cat > "$OUT/aur/PKGBUILD" <<END
pkgname=arca-bin
pkgver=$VERSION
pkgrel=1
pkgdesc="$DESC"
arch=('x86_64' 'aarch64')
url="$HOME_URL"
license=('Apache-2.0')
depends=('gcc-libs' 'glibc' 'libxcb' 'libxkbcommon' 'libxkbcommon-x11')
optdepends=('wayland: native Wayland window')
provides=('arca')
conflicts=('arca')
source_x86_64=("$BASE/arca-v$VERSION-linux-x86_64.tar.gz")
source_aarch64=("$BASE/arca-v$VERSION-linux-arm64.tar.gz")
sha256sums_x86_64=('$X86')
sha256sums_aarch64=('$ARM')

package() {
  cd "arca-v\$pkgver-linux-\$( [ "\$CARCH" = aarch64 ] && echo arm64 || echo x86_64 )"
  install -Dm755 arca arca-gui -t "\$pkgdir/usr/bin/"
  install -Dm644 LICENSE "\$pkgdir/usr/share/licenses/\$pkgname/LICENSE"
  install -Dm644 RAR-NOTICES.txt "\$pkgdir/usr/share/licenses/\$pkgname/RAR-NOTICES.txt"
}
END

cat > "$OUT/aur/.SRCINFO" <<END
pkgbase = arca-bin
	pkgdesc = $DESC
	pkgver = $VERSION
	pkgrel = 1
	url = $HOME_URL
	arch = x86_64
	arch = aarch64
	license = Apache-2.0
	depends = gcc-libs
	depends = glibc
	depends = libxcb
	depends = libxkbcommon
	depends = libxkbcommon-x11
	optdepends = wayland: native Wayland window
	provides = arca
	conflicts = arca
	source_x86_64 = $BASE/arca-v$VERSION-linux-x86_64.tar.gz
	sha256sums_x86_64 = $X86
	source_aarch64 = $BASE/arca-v$VERSION-linux-arm64.tar.gz
	sha256sums_aarch64 = $ARM

pkgname = arca-bin
END

find "$OUT" -type f
