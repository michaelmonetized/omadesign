# Color workflows

Open Color Guide, Color Themes or Recolor Artwork from Edit → Edit Colors or Window.

Color Guide keeps a fixed base while you try its swatches. Choose one of 21 hue relationships. Its options menu contains tint/shade, warm/cool and vivid/muted variations, the step count, reach and swatch-library limit. Click applies a swatch to Fill or Stroke; Shift/Ctrl/Cmd-click collects swatches without applying them. Save writes the collected colors, or the current harmony, through the existing palette collection writer.

Color Themes edits five colors. Link keeps their relationship while moving a point on the hue/saturation wheel; unlinking makes the edited palette Custom. The wheel supports keyboard arrows. Named themes save to `color-themes.json` beside the existing personal palette file. Its bounded versioned file is replaced atomically. An unreadable or changed library is preserved and reports an error. Reload retires the old edit target. Saved themes can also be added to the current swatch collection.

Recolor Artwork captures the current editable vector selection, including selected frame descendants or a selected vector layer. It maps existing fills, gradient stops, strokes and text-span fills. Apply preserves straight alpha, opacity, gradient geometry, Motion tracks and unassigned colors. The entire accepted mapping is one Undo step. Changing document, selection, revision or edit permissions retires that captured target. Patterns are explicitly left unchanged.

The shared workspace queues pane-opening requests made from inside a pane, then applies them to the actual current layout. This prevents a pane button's request from being lost when drawing finishes.

Focused checks cover real native field events, short-dock swatch visibility, actual pane-to-pane navigation, file corruption/concurrent-change protection, color relationships, locked/stale targets, one-step Undo/Redo and native document reopen. Receipts remain tied to their exact source. Native process and cold-restart observations are recorded in the implementation handoff. This layer does not establish full Craft color or product parity.
