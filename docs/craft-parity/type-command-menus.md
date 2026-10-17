# Type command menus

The top Type menu contains the existing area/point conversion, text-inside-shape, threading and path-text commands. Object no longer duplicates that submenu. Character, Paragraph, OpenType, Glyphs and the Typography library remain available as explicitly named pane/library entries.

Converting a linked frame to point text and attaching area text to a path retain the existing typed commands and reflow order, collected into one ordinary Undo entry. The current text draft is committed normally before the structural operation. Busy or scalar-channel contexts and locked, hidden or missing story members reject structural changes before committing that draft or disturbing redo. Native fields and the document model are unchanged.

Source `25386a9e` passed all five affected checks, including exact native and rendered-image reopen. Seven existing readiness/keyboard checks passed separately at `e029c9fa`. That earlier cohort also exposed stale area contours after point conversion; the current source rebuilds those contours before committing the geometry. The failed receipt and images remain retained.

The genuine current desktop binary passed actual Type-menu point conversion, thread break/rejoin, native save, Undo/Redo and separate-process cold reopen in a private Sway session using CPU Vulkan. Independent JSON/byte inspection confirmed all 276 source characters survived conversion and breaking, stable object/style fields, the existing paragraph-newline append policy when rethreading, and byte-identical Undo/Redo/cold saves. This is software qualification, not human acceptance. Evidence: `implementation/craft-type-command-menus.json`.

Full Type parity remains open. The native run exposed an area-text Character inspector layout problem: selecting an area frame showed only its width control. Responsive pane repair is next. Paint-preserving rich text outlines, direct font/style menus, further multi-object formatting and the remaining reference commands still require work. Existing outline conversion is not promoted as lossless rich-type support by this menu change. No dependency, schema, writer or app-version change.
