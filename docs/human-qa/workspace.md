# Workspace correction

The rejected nightly added a second mode bar without a usable docking system. This branch replaces that bar with native pane tabs and keeps the existing top-bar persona icons.

Drag a tab onto another group to nest it. Drop near a pane body’s edge to split the group (left/right for top or bottom docks; upper/lower for side docks); drop on a canvas edge to dock there, or in its middle to float. Drag a column's inner edge to change its width, or a separator between stacked panes to change their heights. Window contains pane access, Reset workspace, and the keyboard arranger (Ctrl+F6). A narrow window constrains the visible dock sizes without rewriting their saved placement.

The Layers pane keeps the existing selection, reorder, group, visibility, lock, and typed document commands. It adds rendered thumbnails and a fixed bottom strip for layer/group/mask/effects/duplicate/delete actions. Double-click a layer name to rename; Enter commits and Escape cancels.

Transform has a proportion lock beside W/H. For a mixed vector selection, each object retains its own ratio. The lock changes editor preferences, while dimension edits use the existing undoable commands.

Independent panes expose the existing appearance, stroke, gradients, SVG patterns/effects, type, paragraph, OpenType, layout, hierarchy, components, variables, prototype, timeline, keyframes, compositions, 3D Scene, Photo library/develop/presets, adjustments, masking, and selection controls. Navigator, History, Actions, Styles, Shapes, Paths, Channels, Glyphs, Artboards, Links, and Info have dedicated bodies. Actions currently support ordered duplicate/group/ungroup/flip/outline/front/back commands; this is not arbitrary gesture recording. Channels shows actual composite display channels, not destructive native channel editing. Those limits remain explicit.

Human acceptance of this correction is pending. The earlier nightly's human QA rejection remains a rejection. Full Craft feature parity is not established by the number of pane names or software tests.
