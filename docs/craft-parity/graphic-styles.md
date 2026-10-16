# Graphic style presets

The Styles pane uses `graphic-styles.json` beside the personal palette. It saves immediately after accepted capture, rename, duplicate, redefine and delete actions. Reload reads the actual file. Names and row IDs stay stable across restart. Corrupt, future, oversized or concurrently changed files are preserved; rejected writes leave the displayed library unchanged.

Each preset retains the existing object's fill/stroke, gradients including native affine radial fields, vector pattern source, effect stack, object/fill opacity, blend mode/interior setting and embedded image fill. Thumbnails use the shared document compositor. Apply changes those fields on every selected editable vector object in one ordinary command batch. Geometry, text/character formatting, masks, object IDs, hierarchy and Motion remain intact. Locked, hidden, missing or busy targets refuse the complete mutation.

This is a portable appearance-preset library. Reference document-level linked graphic styles, linked redefinition, additive appearances, character-color override, layer SVG graph effects and dedicated library import/export controls remain open. A pattern retains its exact source; this does not create a new font loader or change the existing pattern font-resolution contract.
