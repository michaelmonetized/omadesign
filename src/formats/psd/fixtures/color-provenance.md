The FOGRA39L Coated and sRGB ICC test profiles are from Arch colord 1.4.8-1.
Both profile definitions declare CC0 and "free of known copyright restrictions":

- https://github.com/hughsie/colord/blob/main/data/profiles/FOGRA39L_coated.iccprofile.xml
- https://github.com/hughsie/colord/blob/main/data/profiles/sRGB.iccprofile.xml

Profiles are test inputs, not bundled working profiles. No Storytold code or assets
are copied into this implementation. Native PSD format handling follows Adobe's
published specification and is checked separately with ag-psd 31.0.3.

`independent-16.psd` and `independent-32.psd` are original tiny raw-channel PSDs
constructed directly from the published header and planar image-data format.
They contain adjacent values below one byte's precision and out-of-range float samples.

`independent-type.psd` and `independent-smart.psd` are original fixtures written by
ag-psd 31.0.3. The type fixture has Unicode text, affine shear and two font-size/color
runs. The smart fixture embeds an original generated PNG, a perspective quad and a
Gaussian smart-filter descriptor. No private/client or third-party artwork is used.
