# Affinity import

Omadesign opens supported Affinity artwork as editable SVG objects through an optional, separate converter. Set it up once from the source checkout:

```sh
./scripts/setup-affinity-import.sh
```

The installer downloads the Inkscape `extension-afdesign` converter at commit `cd5cf29d5df22e07b1e9209219079ca44015b7fe` (March 9, 2026), verifies its archive checksum, and installs it below `${XDG_DATA_HOME:-~/.local/share}/omadesign/affinity-import`. It uses the Python libraries maintained by your Linux distribution. Install Inkscape plus Python zstandard, Pillow and NumPy first. No system packages or Python environments are changed by the installer.

For a nonstandard installation, set `OMADESIGN_INKEX_PATH` to the directory containing `inkex/`, and optionally `OMADESIGN_AFFINITY_PYTHON` to the Python executable when running setup. At runtime, `OMADESIGN_AFFINITY_CONVERTER` can point to an alternative executable that accepts the source document path and writes SVG to stdout and diagnostics to stderr. Opening a file never downloads software.

## Coverage and limits

The current converter adds the new unified `.af` format and reads the legacy Affinity container. Omadesign checks the container magic, version and document class, so Affinity add-ons sharing the same magic are not mistaken for documents. `.afdesign`, `.afphoto`, `.afpub`, `.aftemplate` and `.afpackage` documents use this conversion path. These extensions identify document families; they do not guarantee that every feature inside them can be converted.

The converter supports shapes, paths, layers, groups, text, embedded raster images, transforms, names, visibility, opacity, SVG blend modes, Gaussian blur, vector masks, artboards and guides. Symbols are expanded into SVG objects. Omadesign's SVG importer determines which converted objects remain editable and how unsupported SVG features are preserved.

This is partial interoperability. Most Affinity adjustments, most live effects, proprietary blends, rich publishing features and edit history are not supported. The converter currently accepts one root spread; multi-spread Publisher documents can fail. In particular, `.afphoto` and `.afpub` are not claims of complete Photo or Publisher compatibility. Failed objects can appear as placeholder layers named `-Failed-…-`; converter diagnostics are shown with the imported document. Keep the original file. Native Affinity export is not provided.

`.afassets`, `.afbrushes`, `.afstyles`, `.afpalette`, `.afmacro` and similar files contain resources or settings rather than complete layered documents. `.afbook` organizes separate chapter documents. These are not imported as artwork. An `.afpackage` is the document within a package folder, accompanied by `Fonts/` and `Images/`; it is not necessarily a ZIP archive. Linked resources still need to be available.

The importer limits input to 512 MiB, SVG output to 256 MiB, diagnostics to 512 KiB and elapsed conversion time to 60 seconds. Each conversion has a private temporary directory that is removed on completion. The supplied launcher also limits CPU time and address space. These are resource limits, not a security sandbox.

## Source, license and verification

The Rust integration is part of MIT-licensed Omadesign. The converter and `inkex` remain separate GPL programs. The compatibility modules in [`scripts/affinity-bridge/`](../scripts/affinity-bridge/README.md) are also GPL-2.0-or-later and execute only in that separate converter; their license is included in that directory. The installer preserves the complete downloaded converter source archive, source files, compatibility additions and license alongside the executable; it does not copy GPL converter code into the Rust application. Distributors bundling the converter must include its corresponding source and license obligations rather than describe the entire bundle as MIT-only.

The compatibility module handles format-9 floating-point raster tiles found in a real Photo document: byte-width tile layout and constant-one alpha tiles. Channels are quantized to 8-bit RGBA, out-of-range values are clipped, and profile/gamma transforms are not guessed. This conversion produces an explicit import note. Synthetic tests cover tile boundaries, constant alpha and quantization without including private documents in the repository.

On the development machine, the pinned converter produced parseable SVG without diagnostics for 23 of its 24 `.afdesign` converter fixtures. The adjustments fixture produced explicit unsupported-adjustment diagnostics. A real private `.afphoto` fixture now yields seven image objects, including five floating-point raster layers, without conversion exceptions after the compatibility fix. The original converter produced only four image objects. The fixture remains outside the repository. This is not proof of complete Affinity fidelity. The upstream repository has no `.af`, `.afphoto` or `.afpub` regression fixtures, and a real unified `.af` document has not yet been verified in this change.

Primary references:

- [Pinned Affinity V3 implementation](https://gitlab.com/inkscape/extras/extension-afdesign/-/commit/cd5cf29d5df22e07b1e9209219079ca44015b7fe)
- [Converter source and fixtures](https://gitlab.com/inkscape/extras/extension-afdesign)
- [Official Affinity format and compatibility information](https://www.affinity.studio/get-affinity)
- [Official template documentation](https://affinity.help/publisher2/en-US.lproj/pages/GetStarted/templates.html)
- [Official package structure](https://affinity.help/publisher2/en-US.lproj/pages/Publishing/aboutPackaging.html)
- [Official add-on documentation](https://affinity.help/designer2/en-US.lproj/pages/Addons/aboutAddons.html)
