# LibRaw 0.22.2

The unchanged source release is vendored here and statically compiled by
`build.rs`. Omadesign chooses LibRaw's **CDDL 1.0** license option. The Rust app
and its original C++ adapter remain MIT licensed. LibRaw's source, copyright,
CDDL and alternative LGPL license are distributed with release packages and
installed in `share/omadesign/licenses/libraw`.

Source: https://www.libraw.org/data/LibRaw-0.22.2.tar.gz

SHA-256: `de86b035655accff8d4010f1a221fdf50d353cb7b1422ba26f14a0db92612cfa`

No upstream source files are changed. The build enables the reentrant decoder,
JPEG, zlib and X3F support; it does not enable OpenMP, RawSpeed, Adobe's DNG SDK, LCMS
or separately licensed demosaic packs. MozJPEG and zlib are statically built
through their pinned Cargo dependencies. Their notices are in the sibling
`native-notices` directory. Updating LibRaw requires replacing this archive,
its notices, version references and checksum together, then validating real
camera files and portable builds.
