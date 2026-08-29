#!/bin/sh
# Build portable Linux tarballs on this machine. Never calls GitHub Actions.
# Links with Zig against glibc 2.35 so the binary runs on Asahi / Ubuntu 22.04+
# instead of demanding this Arch box's glibc 2.44.
set -eu
cd "$(dirname "$0")/.."
ROOT="$PWD"
VERSION="$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)"
DIST="dist"
mkdir -p "$DIST"

chmod +x scripts/zig-cc scripts/zig-cc-aarch64 scripts/zig-cc-x86_64

# LLVM LTO + zig cc's lld plugin is a fight we don't need.
export CARGO_PROFILE_RELEASE_LTO=false

echo "building aarch64-unknown-linux-gnu (glibc 2.35)..."
CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER="$ROOT/scripts/zig-cc-aarch64" \
  cargo build --release --target aarch64-unknown-linux-gnu

echo "building x86_64-unknown-linux-gnu (glibc 2.35)..."
CC_x86_64_unknown_linux_gnu="$ROOT/scripts/zig-cc-x86_64" \
CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER="$ROOT/scripts/zig-cc-x86_64" \
  cargo build --release --target x86_64-unknown-linux-gnu

package() {
  target_dir="$1"
  triple="$2"
  bin="target/${target_dir}release/omadesign"
  if [ ! -x "$bin" ]; then
    echo "missing $bin" >&2
    exit 1
  fi
  max_glibc="$(objdump -T "$bin" | rg -o 'GLIBC_[0-9.]+' | sort -V | tail -1 || true)"
  echo "$triple glibc ceiling: ${max_glibc:-unknown}"
  case "$max_glibc" in
    GLIBC_2.4*|GLIBC_2.3[89]|GLIBC_2.36|GLIBC_2.37)
      echo "refusing to ship $triple: still needs $max_glibc (want <= 2.35)" >&2
      objdump -T "$bin" | rg "$max_glibc" >&2 || true
      exit 1
      ;;
  esac
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

package "aarch64-unknown-linux-gnu/" "aarch64-unknown-linux-gnu"
package "x86_64-unknown-linux-gnu/" "x86_64-unknown-linux-gnu"

echo
ls -lh "$DIST"/*.tar.gz
cat "$DIST"/*.sha256
