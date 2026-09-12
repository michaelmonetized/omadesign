//! Nested frames, auto-stack, and resize constraints for the Layout persona.

use crate::document::{Document, Fill, Shape, Stroke, Style};
use crate::geom::{Bounds, Geom, Pt};
use serde::{Deserialize, Serialize};

/// Horizontal or vertical packing inside a frame.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum StackAxis {
    #[default]
    Vertical,
    Horizontal,
}

impl StackAxis {
    pub fn name(self) -> &'static str {
        match self {
            StackAxis::Vertical => "Vertical",
            StackAxis::Horizontal => "Horizontal",
        }
    }
}

/// Cross-axis alignment for packed children.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum StackAlign {
    #[default]
    Start,
    Center,
    End,
    Stretch,
}

impl StackAlign {
    pub fn name(self) -> &'static str {
        match self {
            StackAlign::Start => "Start",
            StackAlign::Center => "Center",
            StackAlign::End => "End",
            StackAlign::Stretch => "Stretch",
        }
    }
}

/// How a child tracks its parent frame when the frame is resized.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Constraint {
    #[default]
    Start,
    End,
    Stretch,
    Center,
    Scale,
}

impl Constraint {
    pub fn name(self) -> &'static str {
        match self {
            Constraint::Start => "Min",
            Constraint::End => "Max",
            Constraint::Stretch => "Stretch",
            Constraint::Center => "Center",
            Constraint::Scale => "Scale",
        }
    }

    pub fn all() -> [Constraint; 5] {
        [
            Constraint::Start,
            Constraint::End,
            Constraint::Stretch,
            Constraint::Center,
            Constraint::Scale,
        ]
    }
}

/// Auto-stack settings on a frame.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AutoStack {
    pub direction: StackAxis,
    pub gap: f32,
    pub padding: [f32; 4],
    pub align: StackAlign,
}

impl Default for AutoStack {
    fn default() -> Self {
        Self {
            direction: StackAxis::Vertical,
            gap: 12.0,
            padding: [16.0; 4],
            align: StackAlign::Start,
        }
    }
}

/// Layout data stored on a shape. Empty on ordinary Design artwork.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct FrameLayout {
    #[serde(default)]
    pub frame: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stack: Option<AutoStack>,
    #[serde(default)]
    pub constraint_x: Constraint,
    #[serde(default)]
    pub constraint_y: Constraint,
    #[serde(default)]
    pub placeholder: bool,
}

impl FrameLayout {
    pub fn is_empty(&self) -> bool {
        !self.frame
            && self.parent.is_none()
            && self.stack.is_none()
            && self.constraint_x == Constraint::Start
            && self.constraint_y == Constraint::Start
            && !self.placeholder
    }

    pub fn frame() -> Self {
        Self {
            frame: true,
            ..Self::default()
        }
    }
}

pub fn frame_style() -> Style {
    Style {
        fill: Fill::Solid(crate::color::Rgba::rgb(0xF7, 0xF5, 0xF2)),
        stroke: Some(Stroke {
            color: crate::color::Rgba::rgb(0x8A, 0x90, 0x9C),
            width: 1.0,
            dash: Some((6.0, 4.0)),
            ..Stroke::default()
        }),
    }
}

pub fn placeholder_style() -> Style {
    Style {
        fill: Fill::Solid(crate::color::Rgba {
            r: 0xD8,
            g: 0xDE,
            b: 0xE8,
            a: 255,
        }),
        stroke: Some(Stroke {
            color: crate::color::Rgba::rgb(0x6B, 0x73, 0x82),
            width: 1.0,
            dash: Some((8.0, 6.0)),
            ..Stroke::default()
        }),
    }
}

pub fn set_bounds(geom: &mut Geom, bounds: Bounds) {
    let src = geom.bbox();
    if matches!(geom, Geom::Rect { .. }) {
        if let Geom::Rect { origin, size, .. } = geom {
            *origin = bounds.min;
            *size = Pt::new(bounds.width().max(1.0), bounds.height().max(1.0));
        }
        return;
    }
    geom.map_into(src, bounds);
}

pub fn children(doc: &Document, layer: usize, frame_id: u64) -> Vec<u64> {
    let Some(shapes) = doc.layers.get(layer).and_then(|l| l.kind.shapes()) else {
        return vec![];
    };
    shapes
        .iter()
        .filter(|s| s.layout.parent == Some(frame_id))
        .map(|s| s.id)
        .collect()
}

pub fn descendants(doc: &Document, layer: usize, frame_id: u64) -> Vec<u64> {
    let mut out = Vec::new();
    let mut stack = children(doc, layer, frame_id);
    while let Some(id) = stack.pop() {
        out.push(id);
        stack.extend(children(doc, layer, id));
    }
    out
}

pub fn containing_frame(doc: &Document, layer: usize, point: Pt) -> Option<u64> {
    let shapes = doc.layers.get(layer)?.kind.shapes()?;
    shapes
        .iter()
        .rev()
        .find(|s| s.layout.frame && s.visible && !s.locked && s.world_bbox().contains(point))
        .map(|s| s.id)
}

/// Pack auto-stack children inside `frame_id`. Nested frames are packed after
/// their own children have been sized.
pub fn reflow(doc: &mut Document, layer: usize, frame_id: u64) {
    reflow_inner(doc, layer, frame_id, 0);
}

fn reflow_inner(doc: &mut Document, layer: usize, frame_id: u64, depth: u32) {
    if depth > 32 {
        return;
    }
    let kids = children(doc, layer, frame_id);
    for id in &kids {
        if doc
            .find_shape(layer, *id)
            .is_some_and(|s| s.layout.frame && s.layout.stack.is_some())
        {
            reflow_inner(doc, layer, *id, depth + 1);
        }
    }
    let Some(frame) = doc.find_shape(layer, frame_id) else {
        return;
    };
    let Some(stack) = frame.layout.stack.clone() else {
        return;
    };
    let bounds = frame.world_bbox();
    let pad = stack.padding;
    let inner = Bounds {
        min: Pt::new(bounds.min.x + pad[3], bounds.min.y + pad[0]),
        max: Pt::new(
            (bounds.max.x - pad[1]).max(bounds.min.x + pad[3] + 1.0),
            (bounds.max.y - pad[2]).max(bounds.min.y + pad[0] + 1.0),
        ),
    };
    let mut cursor = match stack.direction {
        StackAxis::Vertical => inner.min.y,
        StackAxis::Horizontal => inner.min.x,
    };
    let ids = kids;
    for id in ids {
        let Some(shape) = doc.find_shape(layer, id) else {
            continue;
        };
        if !shape.visible {
            continue;
        }
        let size = shape.geom.bbox().size();
        let (w, h) = match (stack.direction, stack.align) {
            (StackAxis::Vertical, StackAlign::Stretch) => (inner.width().max(1.0), size.y.max(1.0)),
            (StackAxis::Horizontal, StackAlign::Stretch) => {
                (size.x.max(1.0), inner.height().max(1.0))
            }
            _ => (size.x.max(1.0), size.y.max(1.0)),
        };
        let (x, y) = match stack.direction {
            StackAxis::Vertical => {
                let x = match stack.align {
                    StackAlign::Start | StackAlign::Stretch => inner.min.x,
                    StackAlign::Center => inner.min.x + (inner.width() - w) * 0.5,
                    StackAlign::End => inner.max.x - w,
                };
                let y = cursor;
                cursor += h + stack.gap;
                (x, y)
            }
            StackAxis::Horizontal => {
                let y = match stack.align {
                    StackAlign::Start | StackAlign::Stretch => inner.min.y,
                    StackAlign::Center => inner.min.y + (inner.height() - h) * 0.5,
                    StackAlign::End => inner.max.y - h,
                };
                let x = cursor;
                cursor += w + stack.gap;
                (x, y)
            }
        };
        let target = Bounds::from_min_size(Pt::new(x, y), Pt::new(w, h));
        if let Some(shape) = doc.find_shape_mut(layer, id) {
            set_bounds(&mut shape.geom, target);
        }
    }
}

/// Restore each child from `orig`, then apply constraints or auto-stack against
/// the live parent frame. `orig` is the pre-gesture geometry.
pub fn apply_resize(doc: &mut Document, orig: &[(usize, u64, Geom)], changed: &[(usize, u64)]) {
    let mut frames = Vec::new();
    for (layer, id) in changed {
        if doc.find_shape(*layer, *id).is_some_and(|s| s.layout.frame) {
            frames.push((*layer, *id));
        }
    }
    for (layer, frame_id) in frames {
        let Some((_, _, before)) = orig
            .iter()
            .find(|(l, id, _)| *l == layer && *id == frame_id)
        else {
            continue;
        };
        let old_bounds = before.bbox();
        let Some(after) = doc.find_shape(layer, frame_id) else {
            continue;
        };
        let new_bounds = after.geom.bbox();
        if after.layout.stack.is_some() {
            reflow(doc, layer, frame_id);
            continue;
        }
        let kids = children(doc, layer, frame_id);
        for child_id in kids {
            let Some((_, _, child_orig)) = orig
                .iter()
                .find(|(l, id, _)| *l == layer && *id == child_id)
            else {
                continue;
            };
            let Some(child) = doc.find_shape(layer, child_id) else {
                continue;
            };
            let cx = child.layout.constraint_x;
            let cy = child.layout.constraint_y;
            let src = child_orig.bbox();
            let dst = constrain_bounds(src, old_bounds, new_bounds, cx, cy);
            if let Some(shape) = doc.find_shape_mut(layer, child_id) {
                set_bounds(&mut shape.geom, dst);
            }
        }
    }
}

fn constrain_axis(
    child_min: f32,
    child_max: f32,
    old_min: f32,
    old_max: f32,
    new_min: f32,
    new_max: f32,
    how: Constraint,
) -> (f32, f32) {
    let old_span = (old_max - old_min).max(1.0);
    let new_span = (new_max - new_min).max(1.0);
    let size = (child_max - child_min).max(1.0);
    match how {
        Constraint::Start => {
            let inset = child_min - old_min;
            (new_min + inset, new_min + inset + size)
        }
        Constraint::End => {
            let inset = old_max - child_max;
            (new_max - inset - size, new_max - inset)
        }
        Constraint::Stretch => {
            let start = child_min - old_min;
            let end = old_max - child_max;
            (new_min + start, new_max - end)
        }
        Constraint::Center => {
            let center = (child_min + child_max) * 0.5;
            let offset = center - (old_min + old_max) * 0.5;
            let c = (new_min + new_max) * 0.5 + offset;
            (c - size * 0.5, c + size * 0.5)
        }
        Constraint::Scale => {
            let t0 = (child_min - old_min) / old_span;
            let t1 = (child_max - old_min) / old_span;
            (new_min + t0 * new_span, new_min + t1 * new_span)
        }
    }
}

fn constrain_bounds(
    child: Bounds,
    old: Bounds,
    new: Bounds,
    x: Constraint,
    y: Constraint,
) -> Bounds {
    let (x0, x1) = constrain_axis(
        child.min.x,
        child.max.x,
        old.min.x,
        old.max.x,
        new.min.x,
        new.max.x,
        x,
    );
    let (y0, y1) = constrain_axis(
        child.min.y,
        child.max.y,
        old.min.y,
        old.max.y,
        new.min.y,
        new.max.y,
        y,
    );
    Bounds {
        min: Pt::new(x0.min(x1), y0.min(y1)),
        max: Pt::new(x0.max(x1), y0.max(y1)),
    }
}

/// HTML snapshot of a frame and its descendants. Stretch goal for 0.5.0.
pub fn export_html(doc: &Document, layer: usize, frame_id: u64) -> Result<String, String> {
    let frame = doc
        .find_shape(layer, frame_id)
        .filter(|s| s.layout.frame)
        .ok_or_else(|| "Select a frame to export".to_string())?;
    let origin = frame.world_bbox();
    let mut body = String::new();
    write_html_shape(doc, layer, frame_id, origin, &mut body, true)?;
    Ok(format!(
        "<!doctype html><html><head><meta charset=\"utf-8\"><title>{}</title><style>body{{margin:0;background:#111}} .stage{{position:relative;width:{:.0}px;height:{:.0}px;margin:24px auto;background:#fff;overflow:hidden}} .node{{position:absolute;box-sizing:border-box}}</style></head><body><div class=\"stage\">{}</div></body></html>",
        xml_escape(&frame.name),
        origin.width().max(1.0),
        origin.height().max(1.0),
        body
    ))
}

fn write_html_shape(
    doc: &Document,
    layer: usize,
    id: u64,
    origin: Bounds,
    out: &mut String,
    is_root: bool,
) -> Result<(), String> {
    let shape = doc
        .find_shape(layer, id)
        .ok_or_else(|| "Missing frame child".to_string())?;
    let b = shape.world_bbox();
    let left = if is_root { 0.0 } else { b.min.x - origin.min.x };
    let top = if is_root { 0.0 } else { b.min.y - origin.min.y };
    let bg = match shape.style.fill {
        Fill::Solid(c) if c.a > 0 => format!(
            "background:rgba({},{},{},{:.3});",
            c.r,
            c.g,
            c.b,
            c.a as f32 / 255.0
        ),
        _ => String::new(),
    };
    let radius = match shape.geom {
        Geom::Rect { radius, .. } => radius,
        _ => 0.0,
    };
    let text = if let Geom::Text(run) = &shape.geom {
        xml_escape(&run.content).replace('\n', "<br>")
    } else if shape.layout.placeholder {
        "<span style=\"opacity:.55\">Image</span>".into()
    } else {
        String::new()
    };
    out.push_str(&format!(
        "<div class=\"node\" style=\"left:{:.1}px;top:{:.1}px;width:{:.1}px;height:{:.1}px;border-radius:{:.1}px;{bg}overflow:hidden\">{}</div>",
        left,
        top,
        b.width().max(1.0),
        b.height().max(1.0),
        radius,
        text
    ));
    for child in children(doc, layer, id) {
        write_html_shape(doc, layer, child, origin, out, false)?;
    }
    Ok(())
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

pub fn make_frame(origin: Pt, size: Pt) -> Shape {
    let mut shape = Shape::new(
        Geom::Rect {
            origin,
            size: Pt::new(size.x.max(1.0), size.y.max(1.0)),
            radius: 0.0,
        },
        frame_style(),
    );
    shape.name = "Frame".into();
    shape.layout = FrameLayout::frame();
    shape
}

pub fn make_placeholder(origin: Pt, size: Pt, parent: Option<u64>) -> Shape {
    let mut shape = Shape::new(
        Geom::Rect {
            origin,
            size: Pt::new(size.x.max(1.0), size.y.max(1.0)),
            radius: 8.0,
        },
        placeholder_style(),
    );
    shape.name = "Image".into();
    shape.layout.placeholder = true;
    shape.layout.parent = parent;
    shape.corners = [8.0; 4];
    shape
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::{Cmd, Document, Style, apply};
    use crate::geom::Geom;

    fn rect_doc() -> (Document, usize) {
        let doc = Document::new("layout", 800.0, 600.0, 72.0);
        (doc, 1)
    }

    fn add_frame(doc: &mut Document, layer: usize, x: f32, y: f32, w: f32, h: f32) -> u64 {
        let shape = make_frame(Pt::new(x, y), Pt::new(w, h));
        let id = shape.id;
        apply(doc, &Cmd::AddShape { layer, shape });
        id
    }

    fn add_child(
        doc: &mut Document,
        layer: usize,
        parent: u64,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
    ) -> u64 {
        let mut shape = Shape::new(
            Geom::Rect {
                origin: Pt::new(x, y),
                size: Pt::new(w, h),
                radius: 0.0,
            },
            Style::default(),
        );
        shape.layout.parent = Some(parent);
        let id = shape.id;
        apply(doc, &Cmd::AddShape { layer, shape });
        id
    }

    #[test]
    fn vertical_stack_packs_children() {
        let (mut doc, layer) = rect_doc();
        let frame = add_frame(&mut doc, layer, 0.0, 0.0, 200.0, 400.0);
        let a = add_child(&mut doc, layer, frame, 0.0, 0.0, 40.0, 20.0);
        let b = add_child(&mut doc, layer, frame, 0.0, 0.0, 40.0, 30.0);
        doc.find_shape_mut(layer, frame).unwrap().layout.stack = Some(AutoStack {
            direction: StackAxis::Vertical,
            gap: 10.0,
            padding: [8.0, 8.0, 8.0, 8.0],
            align: StackAlign::Start,
        });
        reflow(&mut doc, layer, frame);
        let ga = doc.find_shape(layer, a).unwrap().geom.bbox();
        let gb = doc.find_shape(layer, b).unwrap().geom.bbox();
        assert!((ga.min - Pt::new(8.0, 8.0)).length() < 0.01);
        assert!((gb.min.y - (ga.max.y + 10.0)).abs() < 0.01);
        assert!((gb.min.x - 8.0).abs() < 0.01);
    }

    #[test]
    fn stretch_constraint_keeps_insets() {
        let (mut doc, layer) = rect_doc();
        let frame = add_frame(&mut doc, layer, 0.0, 0.0, 100.0, 80.0);
        let child = add_child(&mut doc, layer, frame, 10.0, 10.0, 80.0, 20.0);
        doc.find_shape_mut(layer, child)
            .unwrap()
            .layout
            .constraint_x = Constraint::Stretch;
        let orig = vec![
            (
                layer,
                frame,
                doc.find_shape(layer, frame).unwrap().geom.clone(),
            ),
            (
                layer,
                child,
                doc.find_shape(layer, child).unwrap().geom.clone(),
            ),
        ];
        set_bounds(
            &mut doc.find_shape_mut(layer, frame).unwrap().geom,
            Bounds::from_min_size(Pt::ZERO, Pt::new(200.0, 80.0)),
        );
        apply_resize(&mut doc, &orig, &[(layer, frame)]);
        let b = doc.find_shape(layer, child).unwrap().geom.bbox();
        assert!((b.min.x - 10.0).abs() < 0.1);
        assert!((b.max.x - 190.0).abs() < 0.1);
    }

    #[test]
    fn nested_frames_keep_parent_links() {
        let (mut doc, layer) = rect_doc();
        let outer = add_frame(&mut doc, layer, 0.0, 0.0, 320.0, 200.0);
        let inner = add_frame(&mut doc, layer, 20.0, 20.0, 120.0, 80.0);
        doc.find_shape_mut(layer, inner).unwrap().layout.parent = Some(outer);
        let leaf = add_child(&mut doc, layer, inner, 30.0, 30.0, 40.0, 20.0);
        assert_eq!(children(&doc, layer, outer), vec![inner]);
        assert_eq!(children(&doc, layer, inner), vec![leaf]);
        let mut desc = descendants(&doc, layer, outer);
        desc.sort();
        let mut expect = vec![inner, leaf];
        expect.sort();
        assert_eq!(desc, expect);
    }

    #[test]
    fn scale_constraint_grows_with_the_frame() {
        let (mut doc, layer) = rect_doc();
        let frame = add_frame(&mut doc, layer, 0.0, 0.0, 100.0, 100.0);
        let child = add_child(&mut doc, layer, frame, 25.0, 25.0, 50.0, 50.0);
        doc.find_shape_mut(layer, child)
            .unwrap()
            .layout
            .constraint_x = Constraint::Scale;
        doc.find_shape_mut(layer, child)
            .unwrap()
            .layout
            .constraint_y = Constraint::Scale;
        let orig = vec![
            (
                layer,
                frame,
                doc.find_shape(layer, frame).unwrap().geom.clone(),
            ),
            (
                layer,
                child,
                doc.find_shape(layer, child).unwrap().geom.clone(),
            ),
        ];
        set_bounds(
            &mut doc.find_shape_mut(layer, frame).unwrap().geom,
            Bounds::from_min_size(Pt::ZERO, Pt::new(200.0, 200.0)),
        );
        apply_resize(&mut doc, &orig, &[(layer, frame)]);
        let b = doc.find_shape(layer, child).unwrap().geom.bbox();
        assert!((b.min.x - 50.0).abs() < 0.2);
        assert!((b.max.x - 150.0).abs() < 0.2);
    }
}
