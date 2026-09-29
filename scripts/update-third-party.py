#!/usr/bin/env python3
"""Regenerate offline Rust credits from Cargo.lock using cargo-about 0.9.2."""
import hashlib
import json
from pathlib import Path
import subprocess
import sys

root = Path(__file__).resolve().parent.parent
raw_path = root / "target/third-party-raw.json"
if len(sys.argv) == 2:
    raw_path = Path(sys.argv[1])
else:
    raw_path.parent.mkdir(parents=True, exist_ok=True)
    subprocess.run(["cargo", "about", "generate", "--locked", "--fail", "--format", "json", "-o", str(raw_path)], cwd=root, check=True)
raw = json.loads(raw_path.read_text())
texts = []
def intern(text):
    text = text.replace("\r\n", "\n").strip()
    if text not in texts:
        texts.append(text)
    return texts.index(text)

components = []
for item in raw["crates"]:
    package = item["package"]
    if package["name"] == "omadesign":
        continue
    notices = []
    for license in raw["licenses"]:
        if any(u["crate"]["id"] == package["id"] for u in license["used_by"]):
            notices.append(intern(license["text"]))
    # Apache notices and separate copyright statements must survive license
    # deduplication. Include packaged native/font notices as well.
    source = Path(package["manifest_path"]).parent
    for path in sorted(source.rglob("*")):
        if not path.is_file() or path.is_symlink() or path.stat().st_size > 2_000_000:
            continue
        if path.name.upper().startswith(("NOTICE", "COPYRIGHT")):
            try:
                notices.append(intern(path.read_text()))
            except UnicodeError:
                raise SystemExit(f"Non-text notice needs review: {path}")
    copyright_lines = []
    for index in notices:
        for line in texts[index].splitlines():
            line = line.strip().strip("/* ")
            if ("copyright" in line.lower() and ("(c)" in line.lower() or "©" in line or "Copyright 20" in line or "Copyright 19" in line)) and "[yyyy]" not in line:
                if line not in copyright_lines:
                    copyright_lines.append(line)
    if not notices:
        raise SystemExit(f"No license text for {package['name']}")
    components.append({"name": package["name"], "version": package["version"], "license": package.get("license") or item["license"], "authors": package["authors"], "copyright": copyright_lines, "repository": package.get("repository"), "notices": sorted(set(notices))})

result = {"format": 1, "generator": "cargo-about 0.9.2 / scripts/update-third-party.py", "cargo_lock_sha256": hashlib.sha256((root / "Cargo.lock").read_bytes()).hexdigest(), "components": components, "texts": texts}
destination = root / "vendor/rust-notices/licenses.json"
destination.parent.mkdir(parents=True, exist_ok=True)
destination.write_text(json.dumps(result, ensure_ascii=False, indent=2) + "\n")
print(f"Wrote {len(components)} components and {len(texts)} full license/notice texts")
