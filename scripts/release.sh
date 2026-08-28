#!/bin/sh
# Build release tarballs on this machine. Never calls GitHub Actions.
set -eu
cd "$(dirname "$0")/.."
VERSION="$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)"
DIST="dist"
OUT="${DIST}/omadesign-${VERSION}"
mkdir -p "$OUT"

package() {
  target="$1"
  triple="$2"
  bin="target/${target}release/omadesign"
  # target is "" for host, "aarch64-unknown-linux-gnu/" for cross
  if [ ! -x "$bin" ]; then
    echo "missing $bin" >&2
    exit 1
  fi
  name="omadesign-${VERSION}-${triple}"
  stage="${DIST}/${name}"
  rm -rf "$stage"
  mkdir -p "$stage"
  install -Dm755 "$bin" "$stage/omadesign"
  case "$triple" in
    aarch64-*) strip_bin="aarch64-linux-gnu-strip" ;;
    *) strip_bin="strip" ;;
  esac
  "$strip_bin" "$stage/omadesign" 2>/dev/null || strip "$stage/omadesign" 2>/dev/null || true
  install -Dm644 omadesign.desktop "$stage/omadesign.desktop"
  install -Dm644 README.md "$stage/README.md"
  install -Dm644 LICENSE "$stage/LICENSE"
  install -Dm755 scripts/install.sh "$stage/install.sh"
  tar -C "$DIST" -czf "${DIST}/${name}.tar.gz" "$name"
  (cd "$DIST" && sha256sum "${name}.tar.gz" > "${name}.tar.gz.sha256")
  echo "wrote ${DIST}/${name}.tar.gz"
}

# host (this machine)
host="$(rustc -vV | sed -n 's/^host: //p')"
package "" "$host"

# Asahi / aarch64 Linux, when the cross binary is present
if [ -x target/aarch64-unknown-linux-gnu/release/omadesign ]; then
  package "aarch64-unknown-linux-gnu/" "aarch64-unknown-linux-gnu"
fi

echo
ls -lh "$DIST"/*.tar.gz
