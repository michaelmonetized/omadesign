#!/bin/sh
# One-liner: curl -fsSL https://raw.githubusercontent.com/michaelmonetized/omadesign/master/scripts/install-remote.sh | sh
set -eu
REPO="michaelmonetized/omadesign"
if [ "$(uname -s)" != Linux ]; then
  echo "omadesign: this installer requires Linux" >&2
  exit 1
fi
for tool in curl sha256sum tar; do
  command -v "$tool" >/dev/null || { echo "omadesign: missing $tool" >&2; exit 1; }
done
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
case "$TAG" in
  *[!A-Za-z0-9._+-]*) echo "omadesign: invalid release tag" >&2; exit 1 ;;
esac
if [ -z "$TAG" ]; then
  echo "omadesign: could not resolve latest release" >&2
  exit 1
fi
VER="${TAG#v}"
NAME="omadesign-${VER}-${TRIPLE}"
URL="https://github.com/${REPO}/releases/download/${TAG}/${NAME}.tar.gz"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' 0
trap 'exit 1' HUP INT TERM
echo "downloading ${URL}"
curl -fL "$URL" -o "$TMP/${NAME}.tar.gz"
curl -fL "${URL}.sha256" -o "$TMP/${NAME}.tar.gz.sha256"
# Accept exactly the single archive named by this release, then verify its bytes
# before tar can extract or execute anything from the download.
if ! expected="$(awk -v archive="${NAME}.tar.gz" '
  NF == 2 && $2 == archive && length($1) == 64 && $1 !~ /[^0-9a-fA-F]/ { hash = $1; valid++ }
  END { if (NR != 1 || valid != 1) exit 1; print hash }
' "$TMP/${NAME}.tar.gz.sha256")"; then
  echo "omadesign: invalid release checksum file" >&2
  exit 1
fi
(cd "$TMP" && printf '%s  %s\n' "$expected" "${NAME}.tar.gz" | sha256sum -c -)
tar -xzf "$TMP/${NAME}.tar.gz" -C "$TMP" --no-same-owner
DIR="$TMP/$NAME"
if [ ! -f "$DIR/install.sh" ] || [ -L "$DIR/install.sh" ]; then
  echo "omadesign: release archive is missing its installer" >&2
  exit 1
fi
cd "$DIR"
./install.sh
echo
echo "omadesign ${VER} is on PATH as ~/.local/bin/omadesign"
echo "run: omadesign"
