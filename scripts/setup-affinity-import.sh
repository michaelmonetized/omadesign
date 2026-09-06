#!/usr/bin/env bash
# Installs the separately licensed Affinity -> SVG converter for this user.
# Network access happens only in this explicit setup command, never on import.
set -euo pipefail

revision=cd5cf29d5df22e07b1e9209219079ca44015b7fe
checksum=fcc89d79f93d16a52631baf32edfcb5c33dd3b181c7b5c43075b39709d2bb861
archive_url="https://gitlab.com/inkscape/extras/extension-afdesign/-/archive/$revision/extension-afdesign-$revision.tar.gz"
data_dir="${XDG_DATA_HOME:-$HOME/.local/share}/omadesign/affinity-import"
script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
python_bin="${OMADESIGN_AFFINITY_PYTHON:-$(command -v python3)}"
inkex_dir="${OMADESIGN_INKEX_PATH:-}"

if [[ -z "$inkex_dir" ]]; then
    for candidate in /usr/share/inkscape/extensions /usr/local/share/inkscape/extensions; do
        if [[ -f "$candidate/inkex/__init__.py" ]]; then
            inkex_dir="$candidate"
            break
        fi
    done
fi

if [[ -z "$inkex_dir" || ! -f "$inkex_dir/inkex/__init__.py" ]]; then
    printf '%s\n' 'Inkscape Python extensions were not found. Install Inkscape, or set OMADESIGN_INKEX_PATH to the directory containing inkex/.' >&2
    exit 1
fi

# Reuse the distribution-maintained Python libraries that ship with Inkscape.
# No pip packages are installed into the system Python.
"$python_bin" - "$inkex_dir" <<'PY'
import importlib
import sys
sys.path.insert(0, sys.argv[1])
if sys.version_info < (3, 10):
    raise SystemExit("Affinity import requires Python 3.10 or later.")
missing = []
for name in ("inkex", "zstandard", "PIL", "numpy"):
    try:
        importlib.import_module(name)
    except ImportError as error:
        missing.append(f"{name}: {error}")
if missing:
    raise SystemExit("Missing Affinity import dependencies:\n" + "\n".join(missing) +
        "\nInstall Inkscape and Python zstandard, Pillow and NumPy using your distribution's package manager.\n"
        "Arch: pacman -S inkscape python-zstandard python-pillow python-numpy\n"
        "Debian/Ubuntu: apt install inkscape python3-zstandard python3-pil python3-numpy")
PY

mkdir -p "$data_dir/releases"
stage="$(mktemp -d "$data_dir/.install.XXXXXX")"
trap 'rm -rf -- "$stage"' EXIT
curl --fail --location --silent --show-error --connect-timeout 15 --max-time 120 \
    --retry 2 "$archive_url" --output "$stage/source.tar.gz"
"$python_bin" - "$stage/source.tar.gz" "$checksum" <<'PY'
import hashlib
import sys
with open(sys.argv[1], "rb") as source:
    digest = hashlib.file_digest(source, "sha256").hexdigest() if hasattr(hashlib, "file_digest") else hashlib.sha256(source.read()).hexdigest()
if digest != sys.argv[2]:
    raise SystemExit("Affinity converter archive checksum mismatch; installation stopped.")
PY
tar -xzf "$stage/source.tar.gz" --no-same-owner --no-same-permissions -C "$stage"
mv "$stage/extension-afdesign-$revision" "$stage/source"
cp "$script_dir/affinity-bridge/compat.py" "$stage/source/inkaf/omadesign_compat.py"
cp "$script_dir/affinity-bridge/test_compat.py" "$stage/source/omadesign_test_compat.py"
cp "$script_dir/affinity-bridge/README.md" "$stage/source/OMADESIGN_BRIDGE.md"

release_dir="$data_dir/releases/$revision-$(date +%s)-$$"
"$python_bin" - "$stage" "$release_dir" "$inkex_dir" "$python_bin" "$revision" <<'PY'
import json
from pathlib import Path
import shlex
import sys

stage, release, inkex, python, revision = sys.argv[1:]
runner = '''import os
import sys

# Only installed, trusted source directories are added to Python's import path.
sys.path.insert(0, INKEX)
sys.path.insert(0, SOURCE)
os.environ["PYTHONNOUSERSITE"] = "1"
try:
    import resource
    # The parent also enforces elapsed time and checks both output streams.
    resource.setrlimit(resource.RLIMIT_CPU, (60, 60))
    resource.setrlimit(resource.RLIMIT_FSIZE, (256 * 1024 * 1024, 256 * 1024 * 1024))
    resource.setrlimit(resource.RLIMIT_AS, (3 * 1024**3, 3 * 1024**3))
except ImportError:
    pass
from inkaf.omadesign_compat import install
install()
from inkaf.afinput import main
main()
'''
runner = 'INKEX = ' + repr(str(Path(inkex).resolve())) + '\nSOURCE = ' + repr(str(Path(release) / "source")) + '\n' + runner
(Path(stage) / "run.py").write_text(runner)
launcher = '#!/bin/sh\nexec ' + shlex.quote(str(Path(python).resolve())) + ' -s ' + shlex.quote(str(Path(release) / 'run.py')) + ' "$@"\n'
(Path(stage) / "convert").write_text(launcher)
(Path(stage) / "convert").chmod(0o755)
(Path(stage) / "installation.json").write_text(json.dumps({
    "upstream": "https://gitlab.com/inkscape/extras/extension-afdesign",
    "revision": revision,
    "license": "GPL-2.0-or-later",
    "python": str(Path(python).resolve()),
    "inkex": str(Path(inkex).resolve()),
}, indent=2) + '\n')
PY

mv "$stage" "$release_dir"
stage="$(mktemp -d "$data_dir/.link.XXXXXX")"
ln -s "$release_dir/convert" "$stage/convert"
mv -Tf "$stage/convert" "$data_dir/convert"
printf 'Affinity converter installed: %s\nPinned source and GPL license: %s/source\n' "$data_dir/convert" "$release_dir"
