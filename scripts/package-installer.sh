#!/bin/sh
set -eu
cd "$(dirname "$0")/.."
VERSION="${1:-$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)}"
case "$VERSION" in
  ''|*[!A-Za-z0-9.+-]*) echo 'invalid installer version' >&2; exit 1 ;;
esac
mkdir -p dist
for ARCH in x86_64 aarch64; do
  NAME="omadesign-installer-${VERSION}-${ARCH}-linux"
  STAGE="dist/$NAME"
  rm -rf "$STAGE"
  mkdir -p "$STAGE"
  install -Dm755 scripts/install-remote.sh "$STAGE/omadesign-install"
  install -Dm644 assets/omadesign.svg "$STAGE/omadesign.svg"
  install -Dm644 LICENSE "$STAGE/LICENSE"
  tar -C dist -czf "dist/$NAME.tar.gz" "$NAME"
  (cd dist && sha256sum "$NAME.tar.gz" > "$NAME.tar.gz.sha256")
  echo "wrote dist/$NAME.tar.gz"
done
