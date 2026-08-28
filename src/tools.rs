//! Tool identity, personas, and the shortcut table designers already have in their fingers.

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Persona {
    Design,
    Pixel,
    Photo,
}

impl Persona {
    pub fn name(self) -> &'static str {
        match self {
            Persona::Design => "Design",
            Persona::Pixel => "Pixel",
            Persona::Photo => "Photo",
        }
    }

    pub fn hint(self) -> &'static str {
        match self {
            Persona::Design => "vector drawing, logos, layout",
            Persona::Pixel => "paint, retouch, selections",
            Persona::Photo => "develop, grade, crop",
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
    Brush,
    Eraser,
    Fill,
    Clone,
    Smudge,
    Crop,
    Marquee,
    EllipseMarquee,
    Lasso,
    Wand,
    Hand,
    Zoom,
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
            Tool::Brush => "Brush",
            Tool::Eraser => "Eraser",
            Tool::Fill => "Fill",
            Tool::Clone => "Clone",
            Tool::Smudge => "Smudge",
            Tool::Crop => "Crop",
            Tool::Marquee => "Marquee",
            Tool::EllipseMarquee => "Elliptical marquee",
            Tool::Lasso => "Lasso",
            Tool::Wand => "Wand",
            Tool::Hand => "Hand",
            Tool::Zoom => "Zoom",
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
            Tool::Brush => "B",
            Tool::Eraser => "E",
            Tool::Fill => "K",
            Tool::Clone => "J",
            Tool::Smudge => "M",
            Tool::Crop => "C",
            Tool::Marquee => "Shift+M",
            Tool::EllipseMarquee => "Shift+O",
            Tool::Lasso => "Q",
            Tool::Wand => "W",
            Tool::Hand => "H",
            Tool::Zoom => "Z",
        }
    }

    pub fn hint(self) -> &'static str {
        match self {
            Tool::Select => "Click to select · drag to move · handles scale · the top handle rotates · Shift-click adds",
            Tool::Node => "Click a point to select · drag handles · Alt-click converts corner/smooth · click a segment to insert",
            Tool::Pen => "Click a corner · click-drag a smooth point · Enter or double-click finishes · click the first point to close",
            Tool::Pencil => "Drag a freehand curve. Release to commit.",
            Tool::Rect => "Drag a rectangle. Shift keeps it square. Corner radius lives in Transform.",
            Tool::Ellipse => "Drag an ellipse. Shift keeps it a circle.",
            Tool::Polygon => "Drag a polygon. Sides live in Transform.",
            Tool::Star => "Drag a star. Points and inner radius live in Transform.",
            Tool::Line => "Drag a straight line. Shift snaps to 45°.",
            Tool::Text => "Click to place type and type into it. Character studio: font, size, OpenType. Esc finishes.",
            Tool::Gradient => "Drag across a selected shape to set a linear fill.",
            Tool::Eyedropper => "Click anywhere on the canvas to sample fill colour.",
            Tool::Brush => "Paint on the active pixel layer. [ ] size · Shift+[ ] hardness.",
            Tool::Eraser => "Erase on the active pixel layer.",
            Tool::Fill => "Click to flood-fill. Tolerance lives in Brush.",
            Tool::Clone => "Alt-click sets the source, then paint to clone.",
            Tool::Smudge => "Drag to smear pixels along the stroke.",
            Tool::Crop => "Drag a crop on the photo, Enter commits, Esc cancels.",
            Tool::Marquee => "Drag a rectangular selection on the pixel layer.",
            Tool::EllipseMarquee => "Drag an elliptical selection.",
            Tool::Lasso => "Draw a freehand selection.",
            Tool::Wand => "Click to select similar colour.",
            Tool::Hand => "Drag to pan. Space does this from any tool.",
            Tool::Zoom => "Drag a box to zoom to that area. Click zooms in, Alt-click zooms out. Scroll always works.",
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
                    | Tool::Brush
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
        }
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
            Tool::Brush,
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

pub fn shortcuts_markdown() -> &'static str {
    "\
Move V · Node A · Pen P · Pencil N
Rectangle R · Ellipse O · Polygon Y · Star S · Line L
Type T · Gradient G · Eyedropper I · Brush B · Eraser E
Fill K · Clone J · Smudge M · Crop C · Wand W · Hand H · Zoom Z
Undo Ctrl+Z · Redo Ctrl+Shift+Z · Duplicate Ctrl+D
Copy Ctrl+C · Paste Ctrl+V · Cut Ctrl+X · Select all Ctrl+A
Save Ctrl+S · Open Ctrl+O · New Ctrl+N · Export Ctrl+E
Fit Ctrl+0 · 100% Ctrl+1 · Pan Space · Zoom Ctrl+scroll
Nudge arrows · Nudge ×10 Shift+arrows
Swap fill/stroke X · Defaults D · Brush size [ ]
"
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
            Tool::Brush,
            Tool::Eraser,
            Tool::Fill,
            Tool::Clone,
            Tool::Smudge,
            Tool::Crop,
            Tool::Marquee,
            Tool::EllipseMarquee,
            Tool::Lasso,
            Tool::Wand,
            Tool::Hand,
            Tool::Zoom,
        ];
        for t in all {
            let ok = t.in_persona(Persona::Design)
                || t.in_persona(Persona::Pixel)
                || t.in_persona(Persona::Photo);
            assert!(ok, "{:?} belongs nowhere", t);
        }
    }
}
