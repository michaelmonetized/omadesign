//! Tool identity, personas, and the shortcut table designers already have in their fingers.

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Persona {
    Design,
    Pixel,
    Photo,
    Motion,
}

impl Persona {
    pub fn name(self) -> &'static str {
        match self {
            Persona::Design => "Design",
            Persona::Pixel => "Pixel",
            Persona::Photo => "Photo",
            Persona::Motion => "Motion",
        }
    }

    pub fn hint(self) -> &'static str {
        match self {
            Persona::Design => "vector drawing, logos, layout",
            Persona::Pixel => "paint, retouch, selections",
            Persona::Photo => "develop, grade, crop",
            Persona::Motion => "timeline, keyframes, Lottie",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Tool {
    Select,
    Node,
    Pen,
    Pencil,
    Rect,
    Ellipse,
    Polygon,
    Star,
    Line,
    Text,
    Gradient,
    Eyedropper,
    Trace,
    Brush,
    Eraser,
    Fill,
    Clone,
    Heal,
    Smudge,
    Crop,
    Marquee,
    EllipseMarquee,
    Lasso,
    Wand,
    Hand,
    Zoom,
    Artboard,
}

impl Tool {
    pub fn label(self) -> &'static str {
        match self {
            Tool::Select => "Move",
            Tool::Node => "Node",
            Tool::Pen => "Pen",
            Tool::Pencil => "Pencil",
            Tool::Rect => "Rectangle",
            Tool::Ellipse => "Ellipse",
            Tool::Polygon => "Polygon",
            Tool::Star => "Star",
            Tool::Line => "Line",
            Tool::Text => "Text",
            Tool::Gradient => "Gradient",
            Tool::Eyedropper => "Eyedropper",
            Tool::Trace => "Trace",
            Tool::Brush => "Brush",
            Tool::Eraser => "Eraser",
            Tool::Fill => "Fill",
            Tool::Clone => "Clone",
            Tool::Heal => "Healing brush",
            Tool::Smudge => "Smudge",
            Tool::Crop => "Crop",
            Tool::Marquee => "Marquee",
            Tool::EllipseMarquee => "Elliptical marquee",
            Tool::Lasso => "Lasso",
            Tool::Wand => "Wand",
            Tool::Hand => "Hand",
            Tool::Zoom => "Zoom",
            Tool::Artboard => "Artboard",
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            Tool::Select => "V",
            Tool::Node => "A",
            Tool::Pen => "P",
            Tool::Pencil => "N",
            Tool::Rect => "R",
            Tool::Ellipse => "O",
            Tool::Polygon => "Y",
            Tool::Star => "S",
            Tool::Line => "L",
            Tool::Text => "T",
            Tool::Gradient => "G",
            Tool::Eyedropper => "I",
            Tool::Trace => "U",
            Tool::Brush => "B",
            Tool::Eraser => "E",
            Tool::Fill => "K",
            Tool::Clone => "J",
            Tool::Heal => "Shift+J",
            Tool::Smudge => "M",
            Tool::Crop => "C",
            Tool::Marquee => "Shift+M",
            Tool::EllipseMarquee => "Shift+O",
            Tool::Lasso => "Q",
            Tool::Wand => "W",
            Tool::Hand => "H",
            Tool::Zoom => "Z",
            Tool::Artboard => "Shift+O",
        }
    }

    pub fn hint(self) -> &'static str {
        match self {
            Tool::Select => {
                "Click to select · drag to move · handles scale · the top handle rotates · Shift-click adds"
            }
            Tool::Node => {
                "Drag a point or handle · click a segment to add · Alt-click corner/smooth · Delete removes the point · Object → Break path"
            }
            Tool::Pen => {
                "Click a corner · click-drag a smooth · Enter finishes open · click the first point to close · click an open end to continue or join"
            }
            Tool::Pencil => "Drag a freehand curve. Release to commit.",
            Tool::Rect => {
                "Drag a rectangle. Shift keeps it square. Corner radius lives in Transform."
            }
            Tool::Ellipse => "Drag an ellipse. Shift keeps it a circle.",
            Tool::Polygon => "Drag a polygon. Sides live in Transform.",
            Tool::Star => "Drag a star. Points and inner radius live in Transform.",
            Tool::Line => "Drag a straight line. Shift snaps to 45°.",
            Tool::Text => {
                "Click to place type and type into it. Character studio: font, size, OpenType. Esc finishes."
            }
            Tool::Gradient => "Drag across a selected shape to set a linear fill.",
            Tool::Eyedropper => "Click anywhere on the canvas to sample fill colour.",
            Tool::Trace => {
                "Click to trace the active pixel layer into vectors. Colours and smoothness live in Trace."
            }
            Tool::Brush => "Paint on the active pixel layer. [ ] size · Shift+[ ] hardness.",
            Tool::Eraser => "Erase on the active pixel layer.",
            Tool::Fill => "Click to flood-fill. Tolerance lives in Brush.",
            Tool::Clone => "Alt-click sets the source, then paint to clone.",
            Tool::Heal => {
                "Alt-click clean texture, then paint to blend it into the surrounding colour."
            }
            Tool::Smudge => "Drag to smear pixels along the stroke.",
            Tool::Crop => "Drag a crop on the photo, Enter commits, Esc cancels.",
            Tool::Marquee => "Drag a rectangular selection on the pixel layer.",
            Tool::EllipseMarquee => "Drag an elliptical selection.",
            Tool::Lasso => "Draw a freehand selection.",
            Tool::Wand => "Click to select similar colour.",
            Tool::Hand => "Drag to pan. Space does this from any tool.",
            Tool::Zoom => {
                "Drag a box · click in · Alt-click out · Ctrl-click artboard · Ctrl+Shift-click selection or all · pinch or scroll to zoom"
            }
            Tool::Artboard => {
                "Click a board to select · drag a handle to resize · drag inside to move · drag empty to draw · Alt clones · wrap from Object"
            }
        }
    }

    pub fn in_persona(self, p: Persona) -> bool {
        match p {
            Persona::Design => matches!(
                self,
                Tool::Select
                    | Tool::Node
                    | Tool::Pen
                    | Tool::Pencil
                    | Tool::Rect
                    | Tool::Ellipse
                    | Tool::Polygon
                    | Tool::Star
                    | Tool::Line
                    | Tool::Text
                    | Tool::Gradient
                    | Tool::Eyedropper
                    | Tool::Trace
                    | Tool::Brush
                    | Tool::Artboard
                    | Tool::Hand
                    | Tool::Zoom
            ),
            Persona::Pixel => matches!(
                self,
                Tool::Select
                    | Tool::Brush
                    | Tool::Eraser
                    | Tool::Fill
                    | Tool::Clone
                    | Tool::Heal
                    | Tool::Smudge
                    | Tool::Marquee
                    | Tool::EllipseMarquee
                    | Tool::Lasso
                    | Tool::Wand
                    | Tool::Eyedropper
                    | Tool::Hand
                    | Tool::Zoom
            ),
            Persona::Photo => matches!(
                self,
                Tool::Hand | Tool::Zoom | Tool::Crop | Tool::Eyedropper
            ),
            Persona::Motion => matches!(self, Tool::Select | Tool::Hand | Tool::Zoom),
        }
    }

    pub fn motion_well() -> &'static [Tool] {
        &[Tool::Select, Tool::Hand, Tool::Zoom]
    }

    pub fn design_well() -> &'static [Tool] {
        &[
            Tool::Select,
            Tool::Node,
            Tool::Pen,
            Tool::Pencil,
            Tool::Rect,
            Tool::Ellipse,
            Tool::Polygon,
            Tool::Star,
            Tool::Line,
            Tool::Text,
            Tool::Gradient,
            Tool::Eyedropper,
            Tool::Trace,
            Tool::Brush,
            Tool::Artboard,
            Tool::Hand,
            Tool::Zoom,
        ]
    }

    pub fn pixel_well() -> &'static [Tool] {
        &[
            Tool::Select,
            Tool::Brush,
            Tool::Eraser,
            Tool::Fill,
            Tool::Clone,
            Tool::Heal,
            Tool::Smudge,
            Tool::Marquee,
            Tool::EllipseMarquee,
            Tool::Lasso,
            Tool::Wand,
            Tool::Eyedropper,
            Tool::Hand,
            Tool::Zoom,
        ]
    }

    pub fn photo_well() -> &'static [Tool] {
        &[Tool::Hand, Tool::Zoom, Tool::Crop, Tool::Eyedropper]
    }
}

pub struct ShortcutRow {
    pub action: &'static str,
    pub keys: &'static str,
}

pub fn shortcut_groups() -> &'static [(&'static str, &'static [ShortcutRow])] {
    &[
        (
            "File",
            &[
                ShortcutRow {
                    action: "New",
                    keys: "Ctrl+N",
                },
                ShortcutRow {
                    action: "Open",
                    keys: "Ctrl+O",
                },
                ShortcutRow {
                    action: "Place",
                    keys: "Ctrl+Shift+P",
                },
                ShortcutRow {
                    action: "Save",
                    keys: "Ctrl+S",
                },
                ShortcutRow {
                    action: "Save as",
                    keys: "Ctrl+Shift+S",
                },
                ShortcutRow {
                    action: "Export PNG",
                    keys: "Ctrl+E",
                },
            ],
        ),
        (
            "Edit",
            &[
                ShortcutRow {
                    action: "Undo",
                    keys: "Ctrl+Z",
                },
                ShortcutRow {
                    action: "Redo",
                    keys: "Ctrl+Shift+Z / Ctrl+Y",
                },
                ShortcutRow {
                    action: "Cut",
                    keys: "Ctrl+X",
                },
                ShortcutRow {
                    action: "Copy",
                    keys: "Ctrl+C",
                },
                ShortcutRow {
                    action: "Paste",
                    keys: "Ctrl+V",
                },
                ShortcutRow {
                    action: "Duplicate",
                    keys: "Ctrl+D",
                },
                ShortcutRow {
                    action: "Delete",
                    keys: "Delete",
                },
                ShortcutRow {
                    action: "Select all",
                    keys: "Ctrl+A",
                },
                ShortcutRow {
                    action: "Copy style",
                    keys: "Ctrl+Alt+C",
                },
                ShortcutRow {
                    action: "Paste style",
                    keys: "Ctrl+Alt+V",
                },
            ],
        ),
        (
            "Arrange",
            &[
                ShortcutRow {
                    action: "Bring to front",
                    keys: "Ctrl+Shift+]",
                },
                ShortcutRow {
                    action: "Bring forward",
                    keys: "Ctrl+]",
                },
                ShortcutRow {
                    action: "Send backward",
                    keys: "Ctrl+[",
                },
                ShortcutRow {
                    action: "Send to back",
                    keys: "Ctrl+Shift+[",
                },
                ShortcutRow {
                    action: "Combine",
                    keys: "Ctrl+G",
                },
                ShortcutRow {
                    action: "Release",
                    keys: "Ctrl+Shift+G",
                },
                ShortcutRow {
                    action: "Nudge",
                    keys: "Arrows",
                },
                ShortcutRow {
                    action: "Nudge ×10",
                    keys: "Shift+Arrows",
                },
                ShortcutRow {
                    action: "Break path",
                    keys: "Object menu",
                },
            ],
        ),
        (
            "Tools",
            &[
                ShortcutRow {
                    action: "Move",
                    keys: "V",
                },
                ShortcutRow {
                    action: "Node",
                    keys: "A",
                },
                ShortcutRow {
                    action: "Pen",
                    keys: "P",
                },
                ShortcutRow {
                    action: "Pencil",
                    keys: "N",
                },
                ShortcutRow {
                    action: "Rectangle",
                    keys: "R",
                },
                ShortcutRow {
                    action: "Ellipse",
                    keys: "O",
                },
                ShortcutRow {
                    action: "Polygon",
                    keys: "Y",
                },
                ShortcutRow {
                    action: "Star",
                    keys: "S",
                },
                ShortcutRow {
                    action: "Line",
                    keys: "L",
                },
                ShortcutRow {
                    action: "Type",
                    keys: "T",
                },
                ShortcutRow {
                    action: "Gradient",
                    keys: "G",
                },
                ShortcutRow {
                    action: "Eyedropper",
                    keys: "I",
                },
                ShortcutRow {
                    action: "Trace",
                    keys: "U",
                },
                ShortcutRow {
                    action: "Brush",
                    keys: "B",
                },
                ShortcutRow {
                    action: "Eraser",
                    keys: "E",
                },
                ShortcutRow {
                    action: "Fill",
                    keys: "K",
                },
                ShortcutRow {
                    action: "Clone",
                    keys: "J",
                },
                ShortcutRow {
                    action: "Healing brush",
                    keys: "Shift+J",
                },
                ShortcutRow {
                    action: "Smudge",
                    keys: "M",
                },
                ShortcutRow {
                    action: "Crop",
                    keys: "C",
                },
                ShortcutRow {
                    action: "Wand",
                    keys: "W",
                },
                ShortcutRow {
                    action: "Lasso",
                    keys: "Q",
                },
                ShortcutRow {
                    action: "Hand",
                    keys: "H",
                },
                ShortcutRow {
                    action: "Zoom",
                    keys: "Z",
                },
                ShortcutRow {
                    action: "Artboard",
                    keys: "Shift+O",
                },
                ShortcutRow {
                    action: "Marquee (Pixel)",
                    keys: "Shift+M",
                },
                ShortcutRow {
                    action: "Ellipse marquee (Pixel)",
                    keys: "Shift+O",
                },
            ],
        ),
        (
            "View",
            &[
                ShortcutRow {
                    action: "Fit artboard",
                    keys: "Ctrl+0",
                },
                ShortcutRow {
                    action: "100%",
                    keys: "Ctrl+1",
                },
                ShortcutRow {
                    action: "Zoom in",
                    keys: "Ctrl++",
                },
                ShortcutRow {
                    action: "Zoom out",
                    keys: "Ctrl+-",
                },
                ShortcutRow {
                    action: "Pan",
                    keys: "Space",
                },
                ShortcutRow {
                    action: "Zoom",
                    keys: "Ctrl+scroll",
                },
                ShortcutRow {
                    action: "Show / hide guides",
                    keys: "Ctrl+;",
                },
                ShortcutRow {
                    action: "Toggle snapping",
                    keys: "Ctrl+Shift+;",
                },
                ShortcutRow {
                    action: "Invert snapping while dragging",
                    keys: "Hold Ctrl",
                },
                ShortcutRow {
                    action: "Constrain direction to 45°",
                    keys: "Hold Shift",
                },
                ShortcutRow {
                    action: "Keys",
                    keys: "F1",
                },
            ],
        ),
        (
            "Motion",
            &[
                ShortcutRow {
                    action: "Play / pause",
                    keys: "Space",
                },
                ShortcutRow {
                    action: "Key at playhead",
                    keys: "K",
                },
                ShortcutRow {
                    action: "To start",
                    keys: "Home",
                },
                ShortcutRow {
                    action: "To end",
                    keys: "End",
                },
            ],
        ),
        (
            "Colour / brush",
            &[
                ShortcutRow {
                    action: "Swap fill/stroke",
                    keys: "X",
                },
                ShortcutRow {
                    action: "Default fill/stroke",
                    keys: "D",
                },
                ShortcutRow {
                    action: "Brush size",
                    keys: "[ ]",
                },
                ShortcutRow {
                    action: "Brush hardness",
                    keys: "Shift+[ ]",
                },
                ShortcutRow {
                    action: "Set clone / heal source",
                    keys: "Alt-click",
                },
            ],
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_tool_has_a_persona() {
        let all = [
            Tool::Select,
            Tool::Node,
            Tool::Pen,
            Tool::Pencil,
            Tool::Rect,
            Tool::Ellipse,
            Tool::Polygon,
            Tool::Star,
            Tool::Line,
            Tool::Text,
            Tool::Gradient,
            Tool::Eyedropper,
            Tool::Trace,
            Tool::Brush,
            Tool::Eraser,
            Tool::Fill,
            Tool::Clone,
            Tool::Heal,
            Tool::Smudge,
            Tool::Crop,
            Tool::Marquee,
            Tool::EllipseMarquee,
            Tool::Lasso,
            Tool::Wand,
            Tool::Hand,
            Tool::Zoom,
            Tool::Artboard,
        ];
        for t in all {
            let ok = t.in_persona(Persona::Design)
                || t.in_persona(Persona::Pixel)
                || t.in_persona(Persona::Photo)
                || t.in_persona(Persona::Motion);
            assert!(ok, "{:?} belongs nowhere", t);
        }
    }
}
