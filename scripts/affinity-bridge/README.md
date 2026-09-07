# Affinity converter compatibility code

The Python files in this directory are licensed under GPL-2.0-or-later, as marked in each file. The full license is in [LICENSE](LICENSE). They run inside the separately installed Inkscape Affinity converter and are not linked into the MIT-licensed Rust application.

`compat.py` adds the floating-point raster-tile decoder described in [the Affinity import notes](../../docs/affinity-import.md). `test_compat.py` uses synthetic data; it contains no private document content.

`../setup-affinity-import.sh` pins and verifies the upstream source archive, copies these compatibility files into the installed source tree, and retains the upstream source, GPL license and source archive alongside the launcher. Inkscape's Python modules and other Python dependencies are supplied by the system package manager.

To run the synthetic test from the source checkout, supply paths for Inkscape's extensions and the downloaded converter source:

```sh
PYTHONPATH=/usr/share/inkscape/extensions:/path/to/extension-afdesign python3 scripts/affinity-bridge/test_compat.py
```
