#!/bin/sh
# Install omadesign into ~/.local for the current user.
set -eu
SCRIPT_DIR="$(CDPATH='' cd -- "$(dirname -- "$0")" && pwd)"
if [ -f "$SCRIPT_DIR/../Cargo.toml" ]; then
  ROOT_DIR="$(CDPATH='' cd -- "$SCRIPT_DIR/.." && pwd)"
  SOURCE_BIN="$ROOT_DIR/target/release/omadesign"
  DESKTOP_FILE="$ROOT_DIR/omadesign.desktop"
else
  SOURCE_BIN="$SCRIPT_DIR/omadesign"
  DESKTOP_FILE="$SCRIPT_DIR/omadesign.desktop"
fi
if [ ! -x "$SOURCE_BIN" ]; then
  echo "omadesign: build first with cargo build --release --bin omadesign" >&2
  exit 1
fi
BIN="${HOME}/.local/bin"
APP="${XDG_DATA_HOME:-${HOME}/.local/share}/applications"
mkdir -p "$BIN" "$APP"
# Rename into place so an existing session can keep running until QA relaunches.
STAGED_BIN="$(mktemp "$BIN/.omadesign.XXXXXX")"
STAGED_APP="$(mktemp "$APP/.omadesign.XXXXXX")"
trap 'rm -f "$STAGED_BIN" "$STAGED_APP"' EXIT HUP INT TERM
install -m755 "$SOURCE_BIN" "$STAGED_BIN"
# Quote paths for the desktop-entry format, then escape the sed replacement.
EXEC_BIN="$(printf '%s' "$BIN/omadesign" | sed 's/[\\"`$]/\\&/g; s/[%]/%%/g; s/\\/\\\\/g')"
EXEC_REPLACEMENT="$(printf '%s' "$EXEC_BIN" | sed 's/[\\&|]/\\&/g')"
sed "s|^Exec=.*|Exec=\"${EXEC_REPLACEMENT}\" %F|" "$DESKTOP_FILE" > "$STAGED_APP"
chmod 644 "$STAGED_APP"
mv -f "$STAGED_BIN" "$BIN/omadesign"
mv -f "$STAGED_APP" "$APP/omadesign.desktop"
if command -v update-desktop-database >/dev/null 2>&1; then
  update-desktop-database "$APP"
fi
echo "installed ${BIN}/omadesign"
echo "relaunch it from your app launcher, or run: omadesign"
