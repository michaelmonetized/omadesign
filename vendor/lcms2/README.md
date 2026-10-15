Little CMS 2.19, unchanged C sources and public headers from the upstream release:
https://github.com/mm2/Little-CMS/releases/download/lcms2.19/lcms2-2.19.tar.gz
SHA-256: 49e7e134e4299733dd0eda434fa468997a28ab3d33fa397c642b03644f552216

MIT license, Copyright (c) 2023 Marti Maria Saguer. See LICENSE.

Omadesign builds this source into its executable through cc in build.rs using the
same target compiler as the rest of its native code. Builds do not download it.
No external liblcms2 is needed by source builds, native packages, portable archives
or prefix installs. The normal native-notices packaging includes its license.
`omadesign --color-runtime-check` reports and checks the linked CMM version.
