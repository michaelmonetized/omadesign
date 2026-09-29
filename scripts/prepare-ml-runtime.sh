#!/bin/sh
# Pinned CPU runtime, fetched only by the builder. The installed app never fetches it.
set -eu
cd "$(dirname "$0")/.."
arch="${1:-$(uname -m)}"
case "$arch" in
  aarch64|aarch64-unknown-linux-gnu) arch=aarch64; sha=e15ff8b5d85afe6c144d97c6fd432254bf76a219daaf17658087d6ecb3e8f0bb ;;
  x86_64|x86_64-unknown-linux-gnu) arch=x64; sha=a3e1b79d7bb1bf09696ce675f49e4064e6c81f6202b8225624fff0e93f8d6407 ;;
  *) echo "Unsupported ONNX Runtime architecture: $arch" >&2; exit 1 ;;
esac
name="onnxruntime-linux-$arch-1.28.0"
cache="target/ml-downloads"
mkdir -p "$cache"
archive="$cache/$name.tgz"
if ! printf '%s  %s\n' "$sha" "$archive" | sha256sum -c --status 2>/dev/null; then
  part="$(mktemp "$cache/.runtime.XXXXXX")"
  trap 'rm -f "$part"' EXIT HUP INT TERM
  curl --fail --location --retry 3 --output "$part" "https://github.com/microsoft/onnxruntime/releases/download/v1.28.0/$name.tgz"
  printf '%s  %s\n' "$sha" "$part" | sha256sum -c
  mv "$part" "$archive"
fi
# Always extract the verified archive, including upstream license and notices.
tar -xzf "$archive" -C "$cache"
printf '%s\n' "$cache/$name"
