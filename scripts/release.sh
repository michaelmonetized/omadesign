#!/bin/sh
# Build portable Linux tarballs on this machine. Never calls GitHub Actions.
# Links with Zig against glibc 2.35 so the binary runs on Asahi / Ubuntu 22.04+
# instead of demanding this Arch box's glibc 2.44.
set -eu
cd "$(dirname "$0")/.."
ROOT="$PWD"
VERSION="$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)"
case "$VERSION" in
  ''|*[!A-Za-z0-9.+-]*) echo "invalid package version: $VERSION" >&2; exit 1 ;;
esac
DIST="dist"
mkdir -p "$DIST"
for tool in cargo readelf tar sha256sum; do
  command -v "$tool" >/dev/null || { echo "missing release tool: $tool" >&2; exit 1; }
done

chmod +x scripts/zig-cc scripts/zig-cc-aarch64 scripts/zig-cc-x86_64 \
  scripts/zig-cxx-aarch64 scripts/zig-cxx-x86_64

# LLVM LTO + zig cc's lld plugin is a fight we don't need.
export CARGO_PROFILE_RELEASE_LTO=false
export OMA_RAW_CXX_STDLIB=c++

echo "building aarch64-unknown-linux-gnu (glibc 2.35)..."
CC_aarch64_unknown_linux_gnu="$ROOT/scripts/zig-cc-aarch64" \
CXX_aarch64_unknown_linux_gnu="$ROOT/scripts/zig-cxx-aarch64" \
CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER="$ROOT/scripts/zig-cc-aarch64" \
  cargo build --locked --release --bin omadesign --target aarch64-unknown-linux-gnu

echo "building x86_64-unknown-linux-gnu (glibc 2.35)..."
CC_x86_64_unknown_linux_gnu="$ROOT/scripts/zig-cc-x86_64" \
CXX_x86_64_unknown_linux_gnu="$ROOT/scripts/zig-cxx-x86_64" \
CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER="$ROOT/scripts/zig-cc-x86_64" \
  cargo build --locked --release --bin omadesign --target x86_64-unknown-linux-gnu

package() {
  target_dir="$1"
  triple="$2"
  bin="target/${target_dir}release/omadesign"
  if [ ! -x "$bin" ]; then
    echo "missing $bin" >&2
    exit 1
  fi
  # readelf understands both ELF architectures on either build host. Inspect
  # required version names, and fail closed if inspection produces no evidence.
  if ! versions="$(readelf --version-info "$bin")"; then
    echo "could not inspect $bin" >&2
    exit 1
  fi
  if ! native_libraries="$(readelf --dynamic "$bin")"; then
    echo "could not inspect native dependencies of $bin" >&2
    exit 1
  fi
  if printf '%s\n' "$native_libraries" | grep -Eq 'Shared library: \[lib(raw|jpeg|mozjpeg|z\.|stdc\+\+|c\+\+)'; then
    echo "refusing to ship $triple: RAW support has an external library dependency" >&2
    exit 1
  fi
  glibc_versions="$(printf '%s\n' "$versions" | sed -n 's/.*Name: \(GLIBC_[^ ]*\).*/\1/p')"
  if [ -z "$glibc_versions" ]; then
    echo "refusing to ship $triple: no required glibc versions found" >&2
    exit 1
  fi
  if ! printf '%s\n' "$glibc_versions" | awk -F '[_.]' '
    NF < 3 || $1 != "GLIBC" { exit 1 }
    { for (i = 2; i <= NF; i++) if ($i !~ /^[0-9]+$/) exit 1 }
    $2 > 2 || ($2 == 2 && $3 > 35) { exit 1 }
    $2 == 2 && $3 == 35 { for (i = 4; i <= NF; i++) if ($i > 0) exit 1 }
  '; then
    echo "refusing to ship $triple: requires glibc newer than 2.35 or an unsupported ABI" >&2
    printf '%s\n' "$glibc_versions" >&2
    exit 1
  fi
  max_glibc="$(printf '%s\n' "$glibc_versions" | sort -V | tail -1)"
  echo "$triple glibc ceiling: $max_glibc"
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
  install -Dm644 omadesign-mime.xml "$stage/omadesign-mime.xml"
  # The archive has no source checkout; make its documentation links usable.
  sed "s|](docs/|](https://github.com/michaelmonetized/omadesign/blob/v${VERSION}/docs/|g; s|](examples/|](https://github.com/michaelmonetized/omadesign/tree/v${VERSION}/examples/|g" \
    README.md > "$stage/README.md"
  chmod 644 "$stage/README.md"
  install -Dm644 LICENSE "$stage/LICENSE"
  install -Dm644 assets/phosphor/LICENSE-MIT "$stage/LICENSE-Phosphor"
  mkdir -p "$stage/licenses/libraw" "$stage/licenses/native-notices"
  cp vendor/libraw/* "$stage/licenses/libraw/"
  cp vendor/native-notices/* "$stage/licenses/native-notices/"
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
