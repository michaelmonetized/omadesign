#!/usr/bin/env bash
set -euo pipefail
REPO="$(cd -- "$(dirname -- "$0")/.." && pwd)"
mkdir -p "$REPO/.artifacts"
SUITE="$(mktemp -d "$REPO/.artifacts/installer-tests.XXXXXX")"
trap 'rm -rf "$SUITE"' EXIT
BASE_PATH="$PATH"
COUNT=0

# Write a fixture app.
# Args: path, version. Returns success after creating an executable.
binary() {
  mkdir -p "$(dirname -- "$1")"
  printf '#!/bin/sh\nversion=\x27%s\x27\n' "$2" > "$1"
  cat >> "$1" <<'SH'
if [ "${1:-}" = --version ]; then echo "omadesign $version"; exit 0; fi
printf '%s\0' "$version" "$PWD" "$@" > "$TEST_LAUNCH_RECORD"
printf '%s' "${XDG_DATA_HOME:-}" > "$TEST_DATA_RECORD"
exit "${TEST_APP_EXIT:-0}"
SH
  chmod +x "$1"
}

# Publish a fixture release.
# Args: optional tag. Returns success after writing its archive and checksum.
publish() {
  local tag="${1:-v0.6.3}" arch name
  arch="$(uname -m)"
  case "$arch" in arm64) arch=aarch64 ;; amd64) arch=x86_64 ;; esac
  name="omadesign-${tag#v}-${arch}-unknown-linux-gnu"
  ARCHIVE="$DOWNLOADS/$name.tar.gz"
  tar -czf "$ARCHIVE" -C "$CASE_DIR" --transform="s|^package|$name|" package
  (cd "$DOWNLOADS" && sha256sum "$name.tar.gz" > "$name.tar.gz.sha256")
  printf '{"tag_name":"%s"}\n' "$tag" > "$DOWNLOADS/release.json"
}

# Prepare an isolated installation.
# Args: none. Returns success; all files stay below the project directory.
fixture() {
  CASE_DIR="$SUITE/$COUNT"
  PACKAGE="$CASE_DIR/package"
  PREFIX="$CASE_DIR/installed app's files"
  CALLER="$CASE_DIR/caller"
  TOOLS="$CASE_DIR/tools"
  DOWNLOADS="$CASE_DIR/downloads"
  TEMPORARY="$CASE_DIR/tmp"
  RECORD="$CASE_DIR/launch"
  CURL_LOG="$CASE_DIR/curl.log"
  PROMPT_LOG="$CASE_DIR/prompt.log"
  mkdir -p "$PACKAGE" "$CALLER" "$TOOLS" "$DOWNLOADS" "$TEMPORARY"
  export PATH="$TOOLS:$BASE_PATH" TMPDIR="$TEMPORARY"
  export OMADESIGN_INSTALL_PREFIX="$PREFIX" TEST_LAUNCH_RECORD="$RECORD"
  export TEST_DATA_RECORD="$CASE_DIR/data-root"
  export TEST_DOWNLOADS="$DOWNLOADS" TEST_CURL_LOG="$CURL_LOG" TEST_PROMPT_LOG="$PROMPT_LOG"
  unset OMADESIGN_TAG TEST_CURL_FAIL TEST_APP_EXIT TEST_NOTIFICATION_FAIL
  export TEST_PROMPT_CHOICE=later
  local file
  for file in omadesign.svg omadesign-mime.xml LICENSE-Phosphor \
    licenses/libraw/LibRaw-0.22.2.tar.gz licenses/libraw/LICENSE.CDDL \
    licenses/native-notices/NOTICE licenses/lua/NOTICE \
    licenses/ml/ONNXRuntime-ThirdPartyNotices.txt licenses/rust/licenses.json \
    lib/libonnxruntime.so.1 skills/omadesign-create/SKILL.md \
    plugins/studio-starter/main.lua plugins/studio-starter/orbit.svg \
    plugins/studio-starter/README.md plugins/studio-starter/LICENSE \
    docs/llms.txt docs/MANUAL.md docs/layout.md docs/format-support.md \
    docs/cloud.md docs/plugins.md docs/CONTRIBUTING.md; do
    mkdir -p "$(dirname -- "$PACKAGE/$file")"
    printf 'fixture\n' > "$PACKAGE/$file"
  done
  printf '[Desktop Entry]\nExec=omadesign %%F\n' > "$PACKAGE/omadesign.desktop"
  cp "$REPO/scripts/install.sh" "$PACKAGE/install.sh"
  cp "$REPO/scripts/install-remote.sh" "$TOOLS/omadesign-install"
  for file in update-mime-database update-desktop-database; do
    printf '#!/bin/sh\nexit 0\n' > "$TOOLS/$file"
  done
  cat > "$TOOLS/curl" <<'SH'
#!/bin/sh
set -eu
url=''
output=''
while [ "$#" -gt 0 ]; do
  case "$1" in
    https://*) url="$1" ;;
    -o) output="$2"; shift ;;
  esac
  shift
done
printf '%s\n' "$url" >> "$TEST_CURL_LOG"
[ "${TEST_CURL_FAIL:-0}" != 1 ] || exit 22
name="${url##*/}"
case "$url" in */releases/latest) name=release.json ;; esac
if [ -n "$output" ]; then cp "$TEST_DOWNLOADS/$name" "$output"; else cat "$TEST_DOWNLOADS/$name"; fi
SH
  for file in notify-send zenity kdialog; do
    cat > "$TOOLS/$file" <<'SH'
#!/bin/sh
printf '%s\n' "${0##*/}" "$@" >> "$TEST_PROMPT_LOG"
case "${0##*/}" in
  notify-send)
    [ "${TEST_NOTIFICATION_FAIL:-0}" != 1 ] || exit 1
    printf '%s\n' "$TEST_PROMPT_CHOICE"
    ;;
  *) [ "$TEST_PROMPT_CHOICE" = update ] ;;
esac
SH
  done
  chmod +x "$TOOLS/"*
  binary "$PACKAGE/omadesign" 0.6.3
  publish
}

# Run an installer without discarding its exit status.
# Args: command and arguments. Returns success; STATUS holds the command status.
invoke() {
  set +e
  (cd "$CALLER" && "$@" < /dev/null) > "$CASE_DIR/stdout" 2> "$CASE_DIR/stderr"
  STATUS=$?
  set -e
}

# Run bundled setup.
# Args: installer arguments. Returns success; STATUS retains its exit status.
local_install() { invoke sh "$PACKAGE/install.sh" --prefix "$PREFIX" "$@"; }
# Run published setup.
# Args: installer arguments. Returns success; STATUS retains its exit status.
remote_install() { invoke sh "$REPO/scripts/install-remote.sh" "$@"; }
# Run the store entry.
# Args: app arguments. Returns success; STATUS retains its exit status.
store_launch() { invoke "$TOOLS/omadesign-install" "$@"; }
# Require a successful command.
# Args: none; reads STATUS. Returns failure with output when it failed.
success() { if [ "$STATUS" != 0 ]; then cat "$CASE_DIR/stdout" "$CASE_DIR/stderr"; return 1; fi; }
# Read app arguments without losing their boundaries.
# Args: none; reads RECORD. Returns success and fills ARGS.
launched() {
  mapfile -d '' -t ARGS < "$RECORD"
  local prefix="$OMADESIGN_INSTALL_PREFIX"
  case "$prefix" in /*) ;; *) prefix="$CALLER/$prefix" ;; esac
  [ "$(cat "$TEST_DATA_RECORD")" = "$prefix/share" ]
}
# Require a download count.
# Args: expected count. Returns failure when the request count differs.
downloads() { mapfile -t URLS < "$CURL_LOG"; [ "${#URLS[@]}" = "$1" ]; }
# Require download cleanup.
# Args: none; reads TEMPORARY. Returns failure when files remain.
temporary_empty() { [ -z "$(ls -A "$TEMPORARY")" ]; }

test_local_arguments() {
  export TEST_APP_EXIT=17
  local_install --launch -- 'a file.oma' --export -odd.oma
  [ "$STATUS" = 17 ]
  launched
  [ "${#ARGS[@]}" = 5 ]
  [ "${ARGS[2]}" = 'a file.oma' ]
  [ "${ARGS[3]}" = --export ]
  [ "${ARGS[4]}" = -odd.oma ]
  [ "${ARGS[*]}" = "0.6.3 $CALLER a file.oma --export -odd.oma" ]
  [ -f "$PREFIX/share/mime/packages/omadesign.xml" ]
  temporary_empty
}

test_version_ordering() {
  local_install; success
  local installed available expected action
  while read -r installed available expected; do
    binary "$PACKAGE/omadesign" "$available"
    publish "v$available"
    for action in local_install remote_install; do
      binary "$PREFIX/bin/omadesign" "$installed"
      "$action" --launch; success; launched
      [ "${ARGS[0]}" = "$expected" ]
    done
  done <<'CASES'
0.6.3 0.6.3 0.6.3
0.6.4 0.6.3 0.6.4
0.10.0 0.9.0 0.10.0
0.6.3 0.6.3-rc.1 0.6.3
0.6.3-rc.2 0.6.3 0.6.3
0.6.3-rc.2 0.6.3-rc.10 0.6.3-rc.10
0.6.3-rc.10 0.6.3-rc.2 0.6.3-rc.10
0.6.3-alpha 0.6.3-beta 0.6.3-beta
0.6.3-alpha.1 0.6.3-alpha.beta 0.6.3-alpha.beta
0.6.3-alpha.beta 0.6.3-alpha.1 0.6.3-alpha.beta
0.6.3-alpha 0.6.3-alpha.1 0.6.3-alpha.1
0.6.3+build.1 0.6.3+build.2 0.6.3+build.1
CASES
}

test_current_local() {
  local_install; success
  local before
  before="$(stat -c %y "$PREFIX/bin/omadesign")"
  local_install --launch; success
  [ "$(stat -c %y "$PREFIX/bin/omadesign")" = "$before" ]
}

test_metadata_repair() {
  local_install; success
  printf 'user edit\n' > "$PREFIX/share/omadesign/plugins/org.omadesign.studio-starter/main.lua"
  rm "$PREFIX/share/mime/packages/omadesign.xml"
  local_install --launch; success
  [ -f "$PREFIX/share/mime/packages/omadesign.xml" ]
  grep -qx 'user edit' "$PREFIX/share/omadesign/plugins/org.omadesign.studio-starter/main.lua"
}

test_plugin_main_repair() {
  local_install; success
  local plugin="$PREFIX/share/omadesign/plugins/org.omadesign.studio-starter" action
  printf 'user edit\n' > "$plugin/user.lua"
  for action in local_install remote_install; do
    rm "$plugin/main.lua"
    "$action" --launch; success
    grep -qx fixture "$plugin/main.lua"
    grep -qx 'user edit' "$plugin/user.lua"
  done
}

test_plugin_assets_repair() {
  local_install; success
  local plugin="$PREFIX/share/omadesign/plugins/org.omadesign.studio-starter" file action
  printf 'user edit\n' > "$plugin/main.lua"
  for file in "$plugin/orbit.svg" "$plugin/README.md" "$plugin/LICENSE" "$PREFIX/share/icons/hicolor/scalable/apps/omadesign.svg"; do
    for action in local_install remote_install; do
      rm "$file"
      "$action" --launch; success
      grep -qx fixture "$file"
      grep -qx 'user edit' "$plugin/main.lua"
    done
  done
}

test_incomplete_legacy_install() {
  local_install; success
  rm "$PREFIX/share/omadesign/plugins/org.omadesign.studio-starter/main.lua"
  printf '#!/bin/sh\nexit 0\n' > "$PACKAGE/install.sh"
  publish
  remote_install --launch
  [ "$STATUS" != 0 ]
  grep -q 'installation is incomplete' "$CASE_DIR/stderr"
  [ ! -f "$RECORD" ]
  temporary_empty
}

test_store_owned_guard() {
  mkdir -p "$PREFIX/bin"
  printf '#!/bin/sh\n# omastore-launcher michaelmonetized/omadesign\nexit 99\n' > "$PREFIX/bin/omadesign"
  chmod +x "$PREFIX/bin/omadesign"
  local action
  local_install; [ "$STATUS" != 0 ]
  for action in local_install remote_install; do
    "$action" --launch
    [ "$STATUS" != 0 ]
    grep -q 'belongs to OmaStore' "$CASE_DIR/stderr"
    grep -q '^# omastore-launcher ' "$PREFIX/bin/omadesign"
  done
  [ ! -f "$CURL_LOG" ]
}

test_remote_fresh() {
  remote_install --launch -- 'a file.oma'; success; launched
  [ "${#ARGS[@]}" = 3 ]
  [ "${ARGS[2]}" = 'a file.oma' ]
  [ "${ARGS[*]}" = "0.6.3 $CALLER a file.oma" ]
  downloads 3
  temporary_empty
}

test_remote_current() {
  local_install; success
  local before
  before="$(stat -c %y "$PREFIX/bin/omadesign")"
  remote_install --launch; success
  downloads 1
  [ "$(stat -c %y "$PREFIX/bin/omadesign")" = "$before" ]
}

test_remote_upgrade() {
  local_install; success
  binary "$PREFIX/bin/omadesign" 0.6.2
  remote_install --launch; success; launched
  [ "${ARGS[0]}" = 0.6.3 ]
  downloads 3
}

test_remote_newer() {
  local_install; success
  binary "$PREFIX/bin/omadesign" 0.6.4
  remote_install --launch; success; launched
  [ "${ARGS[0]}" = 0.6.4 ]
  downloads 1
}

test_repair_no_downgrade() {
  local_install; success
  binary "$PREFIX/bin/omadesign" 0.6.4
  cp "$PREFIX/bin/omadesign" "$CASE_DIR/old"
  rm "$PREFIX/share/mime/packages/omadesign.xml"
  local action
  for action in local_install remote_install; do
    "$action" --launch
    [ "$STATUS" != 0 ]
    grep -q 'matching or newer package' "$CASE_DIR/stderr"
    cmp "$CASE_DIR/old" "$PREFIX/bin/omadesign"
    [ ! -f "$RECORD" ]
  done
}

test_offline_current() {
  local_install; success
  export TEST_CURL_FAIL=1
  remote_install --launch; success; launched
  [ "${ARGS[0]}" = 0.6.3 ]
}

test_offline_missing() {
  export TEST_CURL_FAIL=1
  remote_install --launch
  [ "$STATUS" != 0 ]
  [ ! -f "$PREFIX/bin/omadesign" ]
  [ ! -f "$RECORD" ]
}

test_missing_curl() {
  local_install; success
  local tool
  mkdir "$TOOLS/no-curl"
  for tool in uname dirname awk head grep; do
    ln -s "$(command -v "$tool")" "$TOOLS/no-curl/$tool"
  done
  invoke env PATH="$TOOLS/no-curl" "$TOOLS/omadesign-install" 'a file.oma'
  success; launched
  [ "${ARGS[2]}" = 'a file.oma' ]
  [ ! -f "$CURL_LOG" ]
  rm "$PREFIX/bin/omadesign" "$RECORD"
  invoke env PATH="$TOOLS/no-curl" "$TOOLS/omadesign-install"
  [ "$STATUS" != 0 ]
  grep -q 'missing curl' "$CASE_DIR/stderr"
  [ ! -f "$RECORD" ]
}

test_bad_checksum() {
  local_install; success
  binary "$PREFIX/bin/omadesign" 0.6.2
  cp "$PREFIX/bin/omadesign" "$CASE_DIR/old"
  printf '%064d  %s\n' 0 "${ARCHIVE##*/}" > "$ARCHIVE.sha256"
  remote_install --launch
  [ "$STATUS" != 0 ]
  cmp "$CASE_DIR/old" "$PREFIX/bin/omadesign"
  [ ! -f "$RECORD" ]
  temporary_empty
}

test_wrong_archive_fresh() {
  binary "$PACKAGE/omadesign" 0.6.2
  publish
  remote_install --launch
  [ "$STATUS" != 0 ]
  grep -q 'expected 0.6.3' "$CASE_DIR/stderr"
  [ ! -f "$PREFIX/bin/omadesign" ]
  [ ! -f "$RECORD" ]
  temporary_empty
}

test_wrong_archive_preserves_install() {
  local_install; success
  binary "$PREFIX/bin/omadesign" 0.6.2
  cp "$PREFIX/bin/omadesign" "$CASE_DIR/old"
  binary "$PACKAGE/omadesign" 0.6.1
  publish
  remote_install --launch
  [ "$STATUS" != 0 ]
  cmp "$CASE_DIR/old" "$PREFIX/bin/omadesign"
  [ ! -f "$RECORD" ]
  temporary_empty
}

test_wrong_archive_install_only() {
  local_install; success
  cp "$PREFIX/bin/omadesign" "$CASE_DIR/old"
  binary "$PACKAGE/omadesign" 0.6.2
  publish
  remote_install
  [ "$STATUS" != 0 ]
  grep -q 'expected 0.6.3' "$CASE_DIR/stderr"
  cmp "$CASE_DIR/old" "$PREFIX/bin/omadesign"
  [ ! -f "$RECORD" ]
  temporary_empty
}

test_portable_plugin_copy() {
  local_install; success
  local plugin="$PREFIX/share/omadesign/plugins/org.omadesign.studio-starter"
  printf 'user edit\n' > "$plugin/main.lua"
  rm "$plugin/orbit.svg"
  export TEST_REAL_CP
  TEST_REAL_CP="$(command -v cp)"
  cat > "$TOOLS/cp" <<'SH'
#!/bin/sh
for argument do
  case "$argument" in -n|-Rn|--no-clobber) exit 1 ;; esac
done
exec "$TEST_REAL_CP" "$@"
SH
  chmod +x "$TOOLS/cp"
  local_install --launch; success; launched
  grep -qx fixture "$plugin/orbit.svg"
  grep -qx 'user edit' "$plugin/main.lua"
}

test_relative_prefix() {
  export OMADESIGN_INSTALL_PREFIX='relative prefix'
  remote_install --launch; success; launched
  [ -f "$CALLER/relative prefix/bin/omadesign" ]
  [ "${ARGS[1]}" = "$CALLER" ]
}

test_install_only() {
  local_install; success
  binary "$PREFIX/bin/omadesign" 0.6.4
  remote_install; success
  grep -q "version='0.6.3'" "$PREFIX/bin/omadesign"
  [ ! -f "$RECORD" ]
}

test_invalid_release() {
  printf '{"tag_name":"release"}\n' > "$DOWNLOADS/release.json"
  remote_install --launch
  [ "$STATUS" != 0 ]
  grep -q 'invalid release version' "$CASE_DIR/stderr"
  [ ! -f "$RECORD" ]
}

test_public_parity() { cmp "$REPO/scripts/install-remote.sh" "$REPO/site/public/install"; }

test_payload_setup() {
  invoke sh "$REPO/scripts/install.sh" --package "$PACKAGE" --prefix "$PREFIX" --launch -- 'a file.oma'
  success; launched
  [ "${#ARGS[@]}" = 3 ]
  [ "${ARGS[2]}" = 'a file.oma' ]
  [ -f "$PREFIX/share/mime/packages/omadesign.xml" ]
}

test_store_packaged_repair() {
  local_install; success
  local plugin="$PREFIX/share/omadesign/plugins/org.omadesign.studio-starter" arch entry
  printf 'user edit\n' > "$plugin/main.lua"
  rm "$plugin/orbit.svg"
  printf '#!/bin/sh\nexit 0\n' > "$PACKAGE/install.sh"
  publish
  arch="$(uname -m)"
  case "$arch" in arm64) arch=aarch64 ;; amd64) arch=x86_64 ;; esac
  entry="$CASE_DIR/omadesign-installer-0.6.3-$arch-linux"
  mkdir -p "$entry"
  cp "$REPO/scripts/install-remote.sh" "$entry/omadesign-install"
  cp "$REPO/scripts/install.sh" "$entry/install.sh"
  invoke "$entry/omadesign-install" 'a file.oma'; success; launched
  grep -qx fixture "$plugin/orbit.svg"
  grep -qx 'user edit' "$plugin/main.lua"
  [ "${ARGS[2]}" = 'a file.oma' ]
  temporary_empty
}

test_store_fresh() {
  store_launch 'a file.oma'; success; launched
  [ "${#ARGS[@]}" = 3 ]
  [ "${ARGS[2]}" = 'a file.oma' ]
  [ "${ARGS[*]}" = "0.6.3 $CALLER a file.oma" ]
  [ ! -f "$PROMPT_LOG" ]
  downloads 3
}

test_store_current() {
  local_install; success
  store_launch; success; launched
  [ "${ARGS[0]}" = 0.6.3 ]
  [ ! -f "$PROMPT_LOG" ]
  downloads 1
}

test_store_later() {
  local_install; success
  binary "$PREFIX/bin/omadesign" 0.6.2
  cp "$PREFIX/bin/omadesign" "$CASE_DIR/old"
  store_launch 'a file.oma'; success; launched
  [ "${ARGS[0]}" = 0.6.2 ]
  [ "${ARGS[2]}" = 'a file.oma' ]
  cmp "$CASE_DIR/old" "$PREFIX/bin/omadesign"
  grep -q '0.6.3 is available. You have 0.6.2' "$PROMPT_LOG"
  downloads 1
}

test_store_update() {
  local_install; success
  binary "$PREFIX/bin/omadesign" 0.6.2
  export TEST_PROMPT_CHOICE=update
  store_launch 'a file.oma'; success; launched
  [ "${ARGS[0]}" = 0.6.3 ]
  [ "${ARGS[2]}" = 'a file.oma' ]
  downloads 3
}

test_store_prompt_fallback() {
  local_install; success
  binary "$PREFIX/bin/omadesign" 0.6.2
  export TEST_PROMPT_CHOICE=update TEST_NOTIFICATION_FAIL=1
  store_launch; success; launched
  [ "${ARGS[0]}" = 0.6.3 ]
  grep -qx zenity "$PROMPT_LOG"
}

test_store_explicit_update() {
  local_install; success
  binary "$PREFIX/bin/omadesign" 0.6.2
  store_launch --launch; success; launched
  [ "${ARGS[0]}" = 0.6.3 ]
  [ ! -f "$PROMPT_LOG" ]
}

test_store_launcher_preserved() {
  printf '#!/bin/sh\n# omastore-launcher michaelmonetized/omadesign\nexec "%s" "$@"\n' "$TOOLS/omadesign-install" > "$TOOLS/omastore-entry"
  chmod +x "$TOOLS/omastore-entry"
  cp "$TOOLS/omastore-entry" "$CASE_DIR/old"
  invoke "$TOOLS/omastore-entry" 'a file.oma'; success; launched
  [ "${ARGS[0]}" = 0.6.3 ]
  cmp "$CASE_DIR/old" "$TOOLS/omastore-entry"
  [ -x "$PREFIX/bin/omadesign" ]
}

TESTS=(test_local_arguments test_version_ordering test_current_local test_metadata_repair \
  test_plugin_main_repair test_plugin_assets_repair test_incomplete_legacy_install test_store_owned_guard \
  test_remote_fresh test_remote_current test_remote_upgrade test_remote_newer test_repair_no_downgrade \
  test_offline_current test_offline_missing test_missing_curl test_bad_checksum test_wrong_archive_fresh \
  test_wrong_archive_preserves_install test_wrong_archive_install_only test_portable_plugin_copy \
  test_relative_prefix test_install_only test_invalid_release \
  test_public_parity test_payload_setup test_store_packaged_repair test_store_fresh test_store_current test_store_later test_store_update \
  test_store_prompt_fallback test_store_explicit_update test_store_launcher_preserved)
for test in "${TESTS[@]}"; do
  COUNT=$((COUNT + 1))
  set +e
  (set -e; fixture; "$test") > "$SUITE/$COUNT.log" 2>&1
  result=$?
  set -e
  if [ "$result" != 0 ]; then
    printf 'FAIL %s\n' "$test" >&2
    cat "$SUITE/$COUNT.log" >&2
    exit "$result"
  fi
  printf 'PASS %s\n' "$test"
done
printf '%s installer tests passed\n' "$COUNT"
