#!/bin/sh
# Install into ~/.local, or stage an isolated installation with --prefix DIR.
set -eu
INSTALL_PREFIX=''
case "$#" in
  0) ;;
  2)
    if [ "$1" != --prefix ] || [ -z "$2" ]; then
      echo "usage: $0 [--prefix DIRECTORY]" >&2
      exit 1
    fi
    INSTALL_PREFIX="$2"
    ;;
  *) echo "usage: $0 [--prefix DIRECTORY]" >&2; exit 1 ;;
esac
SCRIPT_DIR="$(CDPATH='' cd -- "$(dirname -- "$0")" && pwd)"
if [ -f "$SCRIPT_DIR/../Cargo.toml" ]; then
  ROOT_DIR="$(CDPATH='' cd -- "$SCRIPT_DIR/.." && pwd)"
  SOURCE_BIN="$ROOT_DIR/target/release/omadesign"
  DESKTOP_FILE="$ROOT_DIR/omadesign.desktop"
  MIME_FILE="$ROOT_DIR/omadesign-mime.xml"
  ICON_FILE="$ROOT_DIR/assets/omadesign.svg"
  LICENSE_DIR="$ROOT_DIR/vendor"
else
  SOURCE_BIN="$SCRIPT_DIR/omadesign"
  DESKTOP_FILE="$SCRIPT_DIR/omadesign.desktop"
  MIME_FILE="$SCRIPT_DIR/omadesign-mime.xml"
  ICON_FILE="$SCRIPT_DIR/omadesign.svg"
  LICENSE_DIR="$SCRIPT_DIR/licenses"
fi
if [ ! -x "$SOURCE_BIN" ]; then
  echo "omadesign: build first with cargo build --release --bin omadesign" >&2
  exit 1
fi
if [ ! -f "$DESKTOP_FILE" ] || [ ! -f "$MIME_FILE" ] || [ ! -f "$ICON_FILE" ]; then
  echo "omadesign: installation is missing desktop, icon, or MIME metadata" >&2
  exit 1
fi
if [ ! -f "$LICENSE_DIR/libraw/LibRaw-0.22.2.tar.gz" ] || \
   [ ! -f "$LICENSE_DIR/libraw/LICENSE.CDDL" ] || \
   [ ! -d "$LICENSE_DIR/native-notices" ]; then
  echo "omadesign: installation is missing the bundled RAW source and licenses" >&2
  exit 1
fi
if [ -n "$INSTALL_PREFIX" ]; then
  mkdir -p "$INSTALL_PREFIX"
  INSTALL_PREFIX="$(CDPATH='' cd -- "$INSTALL_PREFIX" && pwd)"
  BIN="$INSTALL_PREFIX/bin"
  DATA="$INSTALL_PREFIX/share"
else
  BIN="${HOME}/.local/bin"
  DATA="${XDG_DATA_HOME:-${HOME}/.local/share}"
fi
APP="$DATA/applications"
MIME="$DATA/mime"
mkdir -p "$BIN" "$APP" "$MIME/packages"
install -Dm644 "$ICON_FILE" "$DATA/icons/hicolor/scalable/apps/omadesign.svg"
mkdir -p "$DATA/omadesign/licenses/libraw" "$DATA/omadesign/licenses/native-notices"
cp "$LICENSE_DIR/libraw/"* "$DATA/omadesign/licenses/libraw/"
cp "$LICENSE_DIR/native-notices/"* "$DATA/omadesign/licenses/native-notices/"
# Rename into place so an existing session can keep running until QA relaunches.
STAGED_BIN="$(mktemp "$BIN/.omadesign.XXXXXX")"
STAGED_APP="$(mktemp "$APP/.omadesign.XXXXXX")"
STAGED_MIME="$(mktemp "$MIME/packages/.omadesign.XXXXXX")"
trap 'rm -f "$STAGED_BIN" "$STAGED_APP" "$STAGED_MIME"' EXIT HUP INT TERM
install -m755 "$SOURCE_BIN" "$STAGED_BIN"
install -m644 "$MIME_FILE" "$STAGED_MIME"
# Quote paths for the desktop-entry format, then escape the sed replacement.
EXEC_BIN="$(printf '%s' "$BIN/omadesign" | sed 's/[\\"`$]/\\&/g; s/[%]/%%/g; s/\\/\\\\/g')"
EXEC_REPLACEMENT="$(printf '%s' "$EXEC_BIN" | sed 's/[\\&|]/\\&/g')"
sed "s|^Exec=.*|Exec=\"${EXEC_REPLACEMENT}\" %F|" "$DESKTOP_FILE" > "$STAGED_APP"
chmod 644 "$STAGED_APP"
mv -f "$STAGED_BIN" "$BIN/omadesign"
mv -f "$STAGED_APP" "$APP/omadesign.desktop"
mv -f "$STAGED_MIME" "$MIME/packages/omadesign.xml"
# Advertise Open With support without changing any mimeapps.list defaults.
if command -v update-mime-database >/dev/null 2>&1; then
  update-mime-database "$MIME" || echo "omadesign: could not refresh the MIME database" >&2
else
  echo "omadesign: MIME refresh skipped (update-mime-database is unavailable)" >&2
fi
if command -v update-desktop-database >/dev/null 2>&1; then
  update-desktop-database "$APP" || echo "omadesign: could not refresh the desktop database" >&2
fi
echo "installed ${BIN}/omadesign"
if [ -n "$INSTALL_PREFIX" ]; then
  echo "files stay under ${INSTALL_PREFIX}; nothing is written to /usr"
else
  echo "files stay under your home directory; nothing is written to /usr"
fi
case ":${PATH}:" in
  *:"$BIN":*)
    echo "relaunch it from your app launcher, or run: omadesign"
    ;;
  *)
    echo "relaunch it from your app launcher, or run: ${BIN}/omadesign"
    echo "add ${BIN} to PATH if you want the short command"
    ;;
esac
