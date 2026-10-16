#!/bin/sh
# Build portable Linux tarballs on this machine. Never calls GitHub Actions.
# Links with Zig against glibc 2.35 so the binary runs on Asahi / Ubuntu 22.04+
# instead of demanding this Arch box's glibc 2.44.
set -eu
cd "$(dirname "$0")/.."
ROOT="$PWD"
VERSION="$(node scripts/version.mjs get)"
BUILD=1
ARCHITECTURES='aarch64 x86_64'
TARGET_ROOT="${CARGO_TARGET_DIR:-$ROOT/target}"
while [ "$#" -gt 0 ]; do
  case "$1" in
    --package-only) BUILD=0; shift ;;
    --arch)
      case "${2:-}" in
        aarch64|x86_64) ARCHITECTURES="$2" ;;
        all) ARCHITECTURES='aarch64 x86_64' ;;
        *) echo 'use --arch aarch64|x86_64|all' >&2; exit 1 ;;
      esac
      shift 2
      ;;
    --target-dir)
      case "${2:-}" in /*) TARGET_ROOT="$2" ;; *) echo '--target-dir requires an absolute path' >&2; exit 1 ;; esac
      shift 2
      ;;
    *) echo 'usage: scripts/release.sh [--package-only] [--arch aarch64|x86_64|all] [--target-dir DIRECTORY]' >&2; exit 1 ;;
  esac
done
export CARGO_TARGET_DIR="$TARGET_ROOT"
case "$VERSION" in
  ''|*[!A-Za-z0-9.+-]*) echo "invalid package version: $VERSION" >&2; exit 1 ;;
esac
DIST="dist"
mkdir -p "$DIST"
for tool in node readelf tar sha256sum; do
  command -v "$tool" >/dev/null || { echo "missing release tool: $tool" >&2; exit 1; }
done
if [ "$BUILD" = 1 ]; then
  command -v cargo >/dev/null || { echo 'missing release tool: cargo' >&2; exit 1; }
fi

# Bundle a pinned runtime per architecture; do not depend on a system ORT.
for arch in $ARCHITECTURES; do
  sh scripts/prepare-ml-runtime.sh "$arch"
done
printf '%s  %s\n' 309c8469258dda742793dce0ebea8e6dd393174f89934733ecc8b14c76f4ddd8 assets/models/u2netp.onnx | sha256sum -c
printf '%s  %s\n' 74b0bf4bdad2868e256ef722980586b7ff1a8b2c9ba0b8e526498cd28acf5b80 assets/models/realesr-general-x4v3.onnx | sha256sum -c

# LLVM LTO + zig cc's lld plugin is a fight we don't need.
export CARGO_PROFILE_RELEASE_LTO=false
export OMA_RAW_CXX_STDLIB=c++

if [ "$BUILD" = 1 ]; then
  chmod +x scripts/zig-cc scripts/zig-cc-aarch64 scripts/zig-cc-x86_64 \
    scripts/zig-cxx-aarch64 scripts/zig-cxx-x86_64
  for arch in $ARCHITECTURES; do
    echo "building $arch-unknown-linux-gnu (glibc 2.35)..."
    case "$arch" in
      aarch64)
        CC_aarch64_unknown_linux_gnu="$ROOT/scripts/zig-cc-aarch64" \
        CXX_aarch64_unknown_linux_gnu="$ROOT/scripts/zig-cxx-aarch64" \
        CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER="$ROOT/scripts/zig-cc-aarch64" \
          cargo build --locked --release --bin omadesign --target aarch64-unknown-linux-gnu
        ;;
      x86_64)
        CC_x86_64_unknown_linux_gnu="$ROOT/scripts/zig-cc-x86_64" \
        CXX_x86_64_unknown_linux_gnu="$ROOT/scripts/zig-cxx-x86_64" \
        CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER="$ROOT/scripts/zig-cc-x86_64" \
          cargo build --locked --release --bin omadesign --target x86_64-unknown-linux-gnu
        ;;
    esac
  done
fi

package() {
  target_dir="$1"
  triple="$2"
  bin="$TARGET_ROOT/${target_dir}release/omadesign"
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
  if printf '%s\n' "$native_libraries" | grep -Eq 'Shared library: \[lib(raw|lcms2|jpeg|mozjpeg|z\.|stdc\+\+|c\+\+)'; then
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
    *) strip_bin="x86_64-linux-gnu-strip" ;;
  esac
  if ! "$strip_bin" "$stage/omadesign" 2>/dev/null \
    && ! llvm-strip "$stage/omadesign" 2>/dev/null \
    && ! strip "$stage/omadesign" 2>/dev/null; then
    echo "could not strip $triple; install LLVM or matching binutils" >&2
    exit 1
  fi
  install -Dm644 omadesign.desktop "$stage/omadesign.desktop"
  install -Dm644 assets/omadesign.svg "$stage/omadesign.svg"
  install -Dm644 omadesign-mime.xml "$stage/omadesign-mime.xml"
  # The archive has no source checkout; make its documentation links usable.
  sed "s|srcset=\"docs/|srcset=\"https://raw.githubusercontent.com/michaelmonetized/omadesign/v${VERSION}/docs/|g; s|src=\"docs/|src=\"https://raw.githubusercontent.com/michaelmonetized/omadesign/v${VERSION}/docs/|g; s|](docs/media/|](https://raw.githubusercontent.com/michaelmonetized/omadesign/v${VERSION}/docs/media/|g; s|](docs/|](https://github.com/michaelmonetized/omadesign/blob/v${VERSION}/docs/|g; s|](examples/|](https://github.com/michaelmonetized/omadesign/tree/v${VERSION}/examples/|g" \
    README.md > "$stage/README.md"
  chmod 644 "$stage/README.md"
  install -Dm644 LICENSE "$stage/LICENSE"
  case "$triple" in
    aarch64-*) ml_arch=aarch64 ;;
    *) ml_arch=x64 ;;
  esac
  runtime="$TARGET_ROOT/ml-downloads/onnxruntime-linux-$ml_arch-1.28.0"
  mkdir -p "$stage/lib" "$stage/licenses/ml" "$stage/licenses/rust"
  cp -P "$runtime/lib/"*.so* "$stage/lib/"
  cp vendor/ml-notices/* "$stage/licenses/ml/"
  cp vendor/rust-notices/* "$stage/licenses/rust/"
  # Inspect every shared object, not just the Rust executable.
  for library in "$stage/lib/"*.so*; do
    required="$(readelf --version-info "$library" | sed -n 's/.*Name: \(GLIBC_[0-9.]*\).*/\1/p')"
    if [ -z "$required" ] || ! printf '%s\n' "$required" | awk -F '[_.]' '$2 > 2 || ($2 == 2 && $3 > 35) { exit 1 }'; then
      echo "refusing to ship $library: unsupported glibc requirements" >&2
      exit 1
    fi
  done
  install -Dm644 skills/omadesign-create/SKILL.md "$stage/skills/omadesign-create/SKILL.md"
  mkdir -p "$stage/docs"
  for document in MANUAL.md layout.md format-support.md cloud.md plugins.md brushes.md CONTRIBUTING.md; do
    install -Dm644 "docs/$document" "$stage/docs/$document"
  done
  install -Dm644 site/public/llms.txt "$stage/docs/llms.txt"
  install -Dm644 assets/phosphor/LICENSE-MIT "$stage/LICENSE-Phosphor"
  mkdir -p "$stage/licenses/libraw" "$stage/licenses/native-notices"
  cp vendor/libraw/* "$stage/licenses/libraw/"
  cp vendor/native-notices/* "$stage/licenses/native-notices/"
  cp docs/licenses/effectcraft-model-MIT.txt "$stage/licenses/native-notices/EffectCraft-MIT.txt"
  mkdir -p "$stage/licenses/lua" "$stage/plugins"
  cp vendor/lua-notices/* "$stage/licenses/lua/"
  cp -R plugins/studio-starter "$stage/plugins/"
  install -Dm755 scripts/install.sh "$stage/install.sh"
  tar -C "$DIST" -czf "${DIST}/${name}.tar.gz" "$name"
  (cd "$DIST" && sha256sum "${name}.tar.gz" > "${name}.tar.gz.sha256")
  echo "wrote ${DIST}/${name}.tar.gz"
}

for arch in $ARCHITECTURES; do
  package "$arch-unknown-linux-gnu/" "$arch-unknown-linux-gnu"
done
sh scripts/package-installer.sh "$VERSION" "$ARCHITECTURES"

echo
ls -lh "$DIST"/*.tar.gz
cat "$DIST"/*.sha256
