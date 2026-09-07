The tiny PSD and PSB fixtures were generated independently with psd-tools 1.19.0
and Pillow (not Omadesign's writer). They contain a 4×3 RGBA base and an isolated
Multiply group named `Design 🎨`, opacity 192/255. The group's hidden 2×2 child
is positioned at (1, -1), has opacity 128/255, and has pixel-mask samples
0, 85, 170, 255. Raster channels use RLE, including the different PSB row sizes.
These synthetic test assets contain no third-party artwork.

`independent-real-mask.psd` was also generated with psd-tools. It adds a
separate real user mask (-3 channel) before variable mask density and feather
parameters. This catches a discrepancy between Adobe's published field order
and the order used in real Photoshop documents.
