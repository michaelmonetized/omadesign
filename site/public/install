#!/bin/sh
# One-liner: curl -fsSL https://omadesign.app/install | sh
set -eu
LAUNCH=0
case "${1:-}" in
  --launch) LAUNCH=1; shift; if [ "${1:-}" = -- ]; then shift; fi ;;
  '') ;;
  *) echo "usage: $0 [--launch [-- APP_ARGUMENTS...]]" >&2; exit 1 ;;
esac
LAUNCH_DIR="$PWD"
INSTALL_PREFIX="${OMADESIGN_INSTALL_PREFIX:-}"
if [ -n "$INSTALL_PREFIX" ]; then
  case "$INSTALL_PREFIX" in /*) ;; *) INSTALL_PREFIX="$PWD/$INSTALL_PREFIX" ;; esac
  BIN="$INSTALL_PREFIX/bin"
  DATA="$INSTALL_PREFIX/share"
else
  BIN="${HOME}/.local/bin"
  DATA="${XDG_DATA_HOME:-${HOME}/.local/share}"
fi
# Read an installed version.
# Args: executable path. Prints its version; returns failure for invalid output.
read_version() {
  output="$("$1" --version 2>/dev/null)" || return 1
  printf '%s\n' "$output" | awk '
    NF == 2 && $1 == "omadesign" && $2 ~ /^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.-]+)?(\+[0-9A-Za-z.-]+)?$/ { version = $2; valid++ }
    END { if (NR != 1 || valid != 1) exit 1; print version }
  '
}

# Check the files needed by an installed app.
# Args: none; uses BIN and DATA. Returns failure when setup needs repair.
installation_complete() {
  [ -x "$BIN/omadesign" ] || return 1
  for file in "$DATA/applications/omadesign.desktop" "$DATA/mime/packages/omadesign.xml" \
    "$DATA/icons/hicolor/scalable/apps/omadesign.svg" \
    "$DATA/omadesign/lib/libonnxruntime.so.1" "$DATA/omadesign/docs/MANUAL.md" \
    "$DATA/omadesign/skills/omadesign-create/SKILL.md" \
    "$DATA/omadesign/plugins/org.omadesign.studio-starter/main.lua" \
    "$DATA/omadesign/plugins/org.omadesign.studio-starter/orbit.svg" \
    "$DATA/omadesign/plugins/org.omadesign.studio-starter/README.md" \
    "$DATA/omadesign/plugins/org.omadesign.studio-starter/LICENSE"; do
    [ -f "$file" ] || return 1
  done
}

# Keep the installed version when it is current or newer.
# Args: installed version, available version. Returns success without a downgrade.
version_is_current() {
  awk -v installed="$1" -v available="$2" '
    function number(a, b) {
      if (length(a) != length(b)) return length(a) > length(b) ? 1 : -1
      return "x" a == "x" b ? 0 : ("x" a > "x" b ? 1 : -1)
    }
    function pre(v, pos) {
      sub(/\+.*/, "", v); pos = index(v, "-")
      return pos ? substr(v, pos + 1) : ""
    }
    BEGIN {
      a = installed; b = available; sub(/[-+].*/, "", a); sub(/[-+].*/, "", b)
      split(a, ac, "."); split(b, bc, ".")
      for (i = 1; i <= 3; i++) { c = number(ac[i], bc[i]); if (c) exit c < 0 }
      a = pre(installed); b = pre(available)
      if (a == b || a == "") exit 0
      if (b == "") exit 1
      na = split(a, ap, "."); nb = split(b, bp, ".")
      for (i = 1; i <= na && i <= nb; i++) {
        if ("x" ap[i] == "x" bp[i]) continue
        an = ap[i] ~ /^[0-9]+$/; bn = bp[i] ~ /^[0-9]+$/
        if (an && bn) c = number(ap[i], bp[i])
        else if (an != bn) c = an ? -1 : 1
        else c = "x" ap[i] > "x" bp[i] ? 1 : -1
        exit c < 0
      }
      exit na < nb
    }
  '
}
REPO="michaelmonetized/omadesign"
if [ "$(uname -s)" != Linux ]; then
  echo "omadesign: this installer requires Linux" >&2
  exit 1
fi
command -v curl >/dev/null || { echo "omadesign: missing curl" >&2; exit 1; }
ARCH="$(uname -m)"
case "$ARCH" in
  aarch64|arm64) TRIPLE="aarch64-unknown-linux-gnu" ;;
  x86_64|amd64) TRIPLE="x86_64-unknown-linux-gnu" ;;
  *) echo "omadesign: unsupported arch $ARCH (need aarch64 or x86_64)" >&2; exit 1 ;;
esac

if [ -f "$BIN/omadesign" ] && head -c 256 "$BIN/omadesign" | grep -q '^# omastore-launcher '; then
  echo "omadesign: this launcher belongs to OmaStore; update it through OmaStore or choose a separate OMADESIGN_INSTALL_PREFIX" >&2
  exit 1
fi
INSTALLED=''
if [ "$LAUNCH" = 1 ] && [ -x "$BIN/omadesign" ]; then
  INSTALLED="$(read_version "$BIN/omadesign")" || INSTALLED=''
fi
TAG="${OMADESIGN_TAG:-}"
if [ -z "$TAG" ]; then
  if ! release="$(curl -fsSL --connect-timeout 10 --max-time 20 "https://api.github.com/repos/${REPO}/releases/latest")"; then
    if [ -n "$INSTALLED" ] && installation_complete; then
      echo "omadesign: could not check for updates; launching installed $INSTALLED" >&2
      exec "$BIN/omadesign" "$@"
    fi
    echo "omadesign: could not resolve latest release" >&2
    exit 1
  fi
  TAG="$(printf '%s\n' "$release" | sed -n 's/.*"tag_name": *"\([^"]*\)".*/\1/p' | head -1)"
fi
case "$TAG" in
  *[!A-Za-z0-9._+-]*) echo "omadesign: invalid release tag" >&2; exit 1 ;;
esac
if [ -z "$TAG" ]; then
  echo "omadesign: could not resolve latest release" >&2
  exit 1
fi
VER="${TAG#v}"
if ! printf '%s\n' "$VER" | awk '/^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.-]+)?(\+[0-9A-Za-z.-]+)?$/ { valid = 1 } END { exit !valid }'; then
  echo "omadesign: invalid release version" >&2
  exit 1
fi
if [ -n "$INSTALLED" ] && version_is_current "$INSTALLED" "$VER"; then
  if installation_complete; then
    exec "$BIN/omadesign" "$@"
  fi
  if ! version_is_current "$VER" "$INSTALLED"; then
    echo "omadesign: repair installed $INSTALLED with a matching or newer package; latest stable is $VER" >&2
    exit 1
  fi
fi
for tool in sha256sum tar; do
  command -v "$tool" >/dev/null || { echo "omadesign: missing $tool" >&2; exit 1; }
done
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
if [ "$LAUNCH" = 1 ]; then
  archive_version="$(read_version "$DIR/omadesign")" || {
    echo "omadesign: could not verify the release archive version" >&2
    exit 1
  }
  if [ "$archive_version" != "$VER" ]; then
    echo "omadesign: expected $VER in release archive, got $archive_version" >&2
    exit 1
  fi
fi
if [ -n "$INSTALL_PREFIX" ]; then
  ./install.sh --prefix "$INSTALL_PREFIX"
else
  ./install.sh
fi
echo
echo "omadesign ${VER} is installed"
if [ "$LAUNCH" = 1 ]; then
  installed="$(read_version "$BIN/omadesign")" || {
    echo "omadesign: could not verify the installed version" >&2
    exit 1
  }
  if [ "$installed" != "$VER" ]; then
    echo "omadesign: expected $VER after installation, got $installed" >&2
    exit 1
  fi
  if ! installation_complete; then
    echo "omadesign: installation is incomplete; repair it with an updated package before launching" >&2
    exit 1
  fi
  cd "$LAUNCH_DIR"
  rm -rf "$TMP"
  trap - 0 HUP INT TERM
  exec "$BIN/omadesign" "$@"
fi
