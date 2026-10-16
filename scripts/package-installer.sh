#!/bin/sh
set -eu
cd "$(dirname "$0")/.."
VERSION="${1:-$(node scripts/version.mjs get)}"
ARCHITECTURES="${2:-x86_64 aarch64}"
case "$VERSION" in
  ''|*[!A-Za-z0-9.+-]*) echo 'invalid installer version' >&2; exit 1 ;;
esac
mkdir -p dist
for ARCH in $ARCHITECTURES; do
  case "$ARCH" in x86_64|aarch64) ;; *) echo 'unsupported installer architecture' >&2; exit 1 ;; esac
  NAME="omadesign-installer-${VERSION}-${ARCH}-linux"
  STAGE="dist/$NAME"
  rm -rf "$STAGE"
  mkdir -p "$STAGE"
  install -Dm755 scripts/install-remote.sh "$STAGE/omadesign-install"
  case "$VERSION" in
    *-nightly.*)
      {
        printf '#!/bin/sh\nset -eu\n'
        printf 'export OMADESIGN_TAG="\044{OMADESIGN_TAG:-v%s}"\n' "$VERSION"
        cat <<'SH'
if [ -z "${OMADESIGN_INSTALL_PREFIX:-}" ]; then
  export OMADESIGN_NIGHTLY_ENTRY="${OMADESIGN_TAG#v}"
  export OMADESIGN_INSTALL_PREFIX="$HOME/.local/share/omadesign-nightly/releases/$OMADESIGN_NIGHTLY_ENTRY"
fi
SH
        tail -n +2 scripts/install-remote.sh
      } > "$STAGE/omadesign-install"
      ;;
  esac
  install -Dm755 scripts/install.sh "$STAGE/install.sh"
  install -Dm644 assets/omadesign.svg "$STAGE/omadesign.svg"
  install -Dm644 LICENSE "$STAGE/LICENSE"
  tar -C dist -czf "dist/$NAME.tar.gz" "$NAME"
  (cd dist && sha256sum "$NAME.tar.gz" > "$NAME.tar.gz.sha256")
  echo "wrote dist/$NAME.tar.gz"
done
