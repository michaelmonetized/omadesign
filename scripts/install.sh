#!/bin/sh
# Install omadesign into ~/.local for the current user.
set -eu
cd "$(dirname "$0")"
BIN="${HOME}/.local/bin"
APP="${HOME}/.local/share/applications"
mkdir -p "$BIN" "$APP"
install -Dm755 omadesign "$BIN/omadesign"
# Point the desktop entry at the installed binary so the launcher always finds it.
sed "s|^Exec=.*|Exec=${BIN}/omadesign|" omadesign.desktop > "$APP/omadesign.desktop"
chmod 644 "$APP/omadesign.desktop"
echo "installed ${BIN}/omadesign"
echo "open it from your app launcher, or run: omadesign"
