#!/usr/bin/env bash
# Build and package WhatSpace for FreeBSD:
#
#   $DIST/whatspace-<version>-freebsd-<arch>.tar.gz   bin/ (whatspace, whatspace-cli) and share/
#                                                      (desktop entry, metainfo, icons, docs)
#
# Usage: packaging/freebsd/package.sh [--skip-build]
#
# Needs: bash, cargo, and the libraries the window loads at run time (libxkbcommon, wayland or
# libX11, mesa-libs or vulkan-loader). Run by .github/workflows/package.yml inside a FreeBSD VM.
set -euo pipefail
# shellcheck source=../env.sh
. "$(dirname "${BASH_SOURCE[0]}")/../env.sh"
LINUX="$ROOT/packaging/linux"
APP_ID=io.github.billsecond.whatspace

SKIP_BUILD=0
while [ $# -gt 0 ]; do
  case "$1" in
    --skip-build) SKIP_BUILD=1; shift ;;
    -h | --help) sed -n '2,10p' "$0"; exit 0 ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
done

ARCH="$(uname -m)"
case "$ARCH" in
  amd64 | x86_64) ARCH=x86_64 ;;
  arm64 | aarch64) ARCH=aarch64 ;;
  *) echo "unsupported architecture $ARCH" >&2; exit 2 ;;
esac
BASENAME="whatspace-$VERSION-freebsd-$ARCH"

echo "==> WhatSpace $VERSION for FreeBSD $ARCH"

if [ "$SKIP_BUILD" = 0 ]; then
  (cd "$ROOT" && cargo build --release --locked -p whatspace -p whatspace-cli)
fi
BIN="$CARGO_TARGET_DIR/release"
WORK="$CARGO_TARGET_DIR/freebsd-package"
STAGE="$WORK/$BASENAME"
rm -rf "$WORK"

# BSD install has no -D: make the directories first.
mkdir -p "$STAGE/bin" "$STAGE/share/applications" "$STAGE/share/metainfo" \
  "$STAGE/share/icons" "$STAGE/share/doc/whatspace"
install -m 755 "$BIN/whatspace" "$STAGE/bin/whatspace"
install -m 755 "$BIN/whatspace-cli" "$STAGE/bin/whatspace-cli"
strip "$STAGE/bin/whatspace" "$STAGE/bin/whatspace-cli" 2>/dev/null || true
install -m 644 "$LINUX/$APP_ID.desktop" "$STAGE/share/applications/$APP_ID.desktop"
sed -e "s/@VERSION@/$VERSION/g" -e "s/@DATE@/$WHATSPACE_BUILD_DATE/g" \
  "$LINUX/$APP_ID.metainfo.xml.in" >"$STAGE/share/metainfo/$APP_ID.metainfo.xml"
cp -R "$ROOT/assets/app-icon/hicolor" "$STAGE/share/icons/"
copy_docs "$STAGE/share/doc/whatspace"

tar -C "$WORK" -czf "$DIST/$BASENAME.tar.gz" "$BASENAME"
echo "wrote $DIST/$BASENAME.tar.gz"
"$STAGE/bin/whatspace-cli" --version
echo "==> done"
