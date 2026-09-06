#!/bin/sh
# One-liner: curl -fsSL https://raw.githubusercontent.com/michaelmonetized/omadesign/master/scripts/install-remote.sh | sh
set -eu
REPO="michaelmonetized/omadesign"
ARCH="$(uname -m)"
case "$ARCH" in
  aarch64|arm64) TRIPLE="aarch64-unknown-linux-gnu" ;;
  x86_64|amd64) TRIPLE="x86_64-unknown-linux-gnu" ;;
  *) echo "omadesign: unsupported arch $ARCH (need aarch64 or x86_64)" >&2; exit 1 ;;
esac

TAG="${OMADESIGN_TAG:-}"
if [ -z "$TAG" ]; then
  TAG="$(curl -fsSL "https://api.github.com/repos/${REPO}/releases/latest" | sed -n 's/.*"tag_name": *"\([^"]*\)".*/\1/p' | head -1)"
fi
if [ -z "$TAG" ]; then
  echo "omadesign: could not resolve latest release" >&2
  exit 1
fi
VER="${TAG#v}"
NAME="omadesign-${VER}-${TRIPLE}"
URL="https://github.com/${REPO}/releases/download/${TAG}/${NAME}.tar.gz"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT
echo "downloading ${URL}"
curl -fL "$URL" -o "$TMP/oma.tgz"
tar -xzf "$TMP/oma.tgz" -C "$TMP"
DIR="$(find "$TMP" -maxdepth 1 -type d -name 'omadesign-*' | head -1)"
cd "$DIR"
./install.sh
echo
echo "omadesign ${VER} is on PATH as ~/.local/bin/omadesign"
echo "run: omadesign"
