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

/// Sizing on either axis. Hug measures content; Fill shares available parent space.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Sizing {
    #[default]
    Fixed,
    Hug,
    Fill,
}

impl Sizing {
    pub fn name(self) -> &'static str {
        match self {
            Self::Fixed => "Fixed",
            Self::Hug => "Hug",
            Self::Fill => "Fill",
        }
    }
    pub fn all() -> [Self; 3] {
        [Self::Fixed, Self::Hug, Self::Fill]
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum StackFlow {
    #[default]
    Stack,
    Wrap,
    Grid,
}

impl StackFlow {
    pub fn name(self) -> &'static str {
        match self {
            Self::Stack => "Stack",
            Self::Wrap => "Wrap",
            Self::Grid => "Grid",
        }
    }
    pub fn all() -> [Self; 3] {
        [Self::Stack, Self::Wrap, Self::Grid]
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum StackJustify {
    #[default]
    Start,
    Center,
    End,
    SpaceBetween,
    SpaceAround,
    SpaceEvenly,
}

impl StackJustify {
    pub fn name(self) -> &'static str {
        match self {
            Self::Start => "Start",
            Self::Center => "Center",
            Self::End => "End",
            Self::SpaceBetween => "Space between",
            Self::SpaceAround => "Space around",
            Self::SpaceEvenly => "Space evenly",
        }
    }
    pub fn all() -> [Self; 6] {
        [
            Self::Start,
            Self::Center,
            Self::End,
            Self::SpaceBetween,
            Self::SpaceAround,
            Self::SpaceEvenly,
        ]
    }
}

fn default_cross_gap() -> f32 {
    12.0
}
fn default_columns() -> u32 {
    3
}

/// Auto-stack settings on a frame.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AutoStack {
    pub direction: StackAxis,
    pub gap: f32,
    pub padding: [f32; 4],
    pub align: StackAlign,
    #[serde(default)]
    pub flow: StackFlow,
    #[serde(default)]
    pub justify: StackJustify,
    #[serde(default = "default_cross_gap")]
    pub cross_gap: f32,
    #[serde(default = "default_columns")]
    pub columns: u32,
}

impl Default for AutoStack {
    fn default() -> Self {
        Self {
            direction: StackAxis::Vertical,
            gap: 12.0,
            padding: [16.0; 4],
            align: StackAlign::Start,
            flow: StackFlow::Stack,
            justify: StackJustify::Start,
            cross_gap: default_cross_gap(),
            columns: default_columns(),
        }
    }
}

/// Overrides activated by the top-level frame viewport. Larger thresholds
/// apply first, so a phone rule can inherit tablet spacing and change its flow.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LayoutBreakpoint {
    pub max_width: f32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text_size: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub direction: Option<StackAxis>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub flow: Option<StackFlow>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub columns: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gap: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cross_gap: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub padding: Option<[f32; 4]>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub align: Option<StackAlign>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub justify: Option<StackJustify>,
}
impl Default for LayoutBreakpoint {
    fn default() -> Self {
        Self {
            max_width: 768.0,
            text_size: None,
            direction: None,
            flow: None,
            columns: None,
            gap: None,
            cross_gap: None,
            padding: None,
            align: None,
            justify: None,
        }
    }
}

/// Resolve responsive settings without changing the authored desktop values.
pub fn resolve_for_width(base: &FrameLayout, viewport_width: f32) -> FrameLayout {
    let mut effective = base.clone();
    apply_breakpoints(&mut effective, viewport_width);
    effective
}

fn apply_breakpoints(layout: &mut FrameLayout, viewport_width: f32) {
    let mut matches: Vec<_> = layout
        .breakpoints
        .iter()
        .filter(|b| b.max_width.is_finite() && b.max_width > 0.0 && viewport_width <= b.max_width)
        .collect();
    matches.sort_by(|a, b| b.max_width.total_cmp(&a.max_width));
    for point in matches {
        if let Some(value) = point.text_size {
            layout.text_size = Some(value);
        }
        let Some(stack) = &mut layout.stack else {
            continue;
        };
        if let Some(value) = point.direction {
            stack.direction = value;
        }
        if let Some(value) = point.flow {
            stack.flow = value;
        }
        if let Some(value) = point.columns {
            stack.columns = value.clamp(1, 1024);
        }
        if let Some(value) = point.gap {
            stack.gap = value;
        }
        if let Some(value) = point.cross_gap {
            stack.cross_gap = value;
        }
        if let Some(value) = point.padding {
            stack.padding = value;
        }
        if let Some(value) = point.align {
            stack.align = value;
        }
        if let Some(value) = point.justify {
            stack.justify = value;
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
    #[serde(default)]
    pub width: Sizing,
    #[serde(default)]
    pub height: Sizing,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_width: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_width: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_height: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_height: Option<f32>,
    /// Exclude this child from automatic flow while retaining resize constraints.
    #[serde(default)]
    pub absolute: bool,
    #[serde(default)]
    pub clip: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text_size: Option<f32>,
    /// Lock the height to resolved width / ratio while the width responds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub aspect_ratio: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image: Option<crate::layout_images::ImageFill>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub breakpoints: Vec<LayoutBreakpoint>,
    #[serde(
        default,
        skip_serializing_if = "crate::layout_tokens::TokenBindings::is_empty"
    )]
    pub tokens: crate::layout_tokens::TokenBindings,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub component: Option<crate::layout_components::ComponentBinding>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub interactions: Vec<crate::layout_prototype::Interaction>,
}

impl FrameLayout {
    pub fn is_empty(&self) -> bool {
        !self.frame
            && self.parent.is_none()
            && self.stack.is_none()
            && self.constraint_x == Constraint::Start
            && self.constraint_y == Constraint::Start
            && !self.placeholder
            && self.width == Sizing::Fixed
            && self.height == Sizing::Fixed
            && self.min_width.is_none()
            && self.max_width.is_none()
            && self.min_height.is_none()
            && self.max_height.is_none()
            && !self.absolute
            && !self.clip
            && self.text_size.is_none()
            && self.aspect_ratio.is_none()
            && self.image.is_none()
            && self.breakpoints.is_empty()
            && self.tokens.is_empty()
            && self.component.is_none()
            && self.interactions.is_empty()
    }

    pub fn frame() -> Self {
        Self {
            frame: true,
            clip: true,
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
    // Layout changes the text box, never the font size or glyph proportions.
    if let Geom::Text(run) = geom {
        run.wrap_width = Some(bounds.width().max(1.0));
        run.origin = Pt::new(bounds.min.x, bounds.min.y + run.px * 0.85);
        run.contours = crate::text::shape(run);
        return;
    }
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
    let Some(shapes) = doc.layers.get(layer).and_then(|l| l.kind.shapes()) else {
        return vec![];
    };
    let mut edges: std::collections::HashMap<u64, Vec<u64>> = std::collections::HashMap::new();
    for shape in shapes {
        if let Some(parent) = shape.layout.parent {
            edges.entry(parent).or_default().push(shape.id);
        }
    }
    let mut seen = std::collections::HashSet::from([frame_id]);
    let mut out = Vec::new();
    let mut stack = edges.get(&frame_id).cloned().unwrap_or_default();
    while let Some(id) = stack.pop() {
        if !seen.insert(id) {
            continue;
        }
        out.push(id);
        if let Some(kids) = edges.get(&id) {
            stack.extend(kids.iter().copied());
        }
    }
    out
}

/// Outermost ancestor, with malformed cycles terminating safely.
pub fn top_frame(doc: &Document, layer: usize, id: u64) -> Option<u64> {
    let shapes = doc.layers.get(layer)?.kind.shapes()?;
    let by_id: std::collections::HashMap<_, _> = shapes.iter().map(|s| (s.id, s)).collect();
    let mut current = *by_id.get(&id)?;
    let mut frame = current.layout.frame.then_some(id);
    let mut seen = std::collections::HashSet::from([id]);
    while let Some(parent) = current.layout.parent.and_then(|p| by_id.get(&p).copied()) {
        if !parent.layout.frame || !seen.insert(parent.id) {
            break;
        }
        frame = Some(parent.id);
        current = parent;
    }
    frame
}

/// Pick the deepest visible frame under the pointer. Hierarchy order matters
/// when wrapping appends an outer frame after its children in the flat store.
pub fn containing_frame(doc: &Document, layer: usize, point: Pt) -> Option<u64> {
    if !doc.layer_editable(layer) {
        return None;
    }
    let shapes = doc.layers.get(layer)?.kind.shapes()?;
    let mut children: std::collections::HashMap<Option<u64>, Vec<usize>> =
        std::collections::HashMap::new();
    for (index, shape) in shapes.iter().enumerate() {
        children.entry(shape.layout.parent).or_default().push(index);
    }
    let mut pending: Vec<_> = children
        .get(&None)
        .into_iter()
        .flatten()
        .rev()
        .map(|&index| (index, point, 0usize))
        .collect();
    let mut seen = std::collections::HashSet::new();
    let mut best = None;
    let mut best_depth = 0;
    while let Some((index, local_point, depth)) = pending.pop() {
        if !seen.insert(index) {
            continue;
        }
        let shape = &shapes[index];
        if !shape.visible || shape.locked {
            continue;
        }
        let contains = shape.layout.frame && shape.contains_world(local_point);
        if contains && (best.is_none() || depth >= best_depth) {
            best = Some(shape.id);
            best_depth = depth;
        }
        if !shape.layout.frame || (shape.layout.clip && !contains) {
            continue;
        }
        let next_point = shape.local_point(local_point);
        if let Some(kids) = children.get(&Some(shape.id)) {
            pending.extend(
                kids.iter()
                    .rev()
                    .map(|&index| (index, next_point, depth + 1)),
            );
        }
    }
    best
}

mod solver;
pub use solver::{apply_resize, reflow, reflow_roots, repair_hierarchy, validate_hierarchy};

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
            (new_min + start, (new_max - end).max(new_min + start + 1.0))
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

/// A responsive standalone HTML export of the frame subtree.
pub fn export_html(doc: &Document, layer: usize, frame_id: u64) -> Result<String, String> {
    crate::layout_export::export_html(doc, layer, frame_id)
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
            ..AutoStack::default()
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
    fn parent_picker_prefers_deepest_frame_even_when_outer_is_appended_later() {
        let (mut doc, layer) = rect_doc();
        let inner = add_frame(&mut doc, layer, 20.0, 20.0, 100.0, 100.0);
        let outer = add_frame(&mut doc, layer, 0.0, 0.0, 200.0, 200.0);
        doc.find_shape_mut(layer, inner).unwrap().layout.parent = Some(outer);
        assert_eq!(
            containing_frame(&doc, layer, Pt::new(50.0, 50.0)),
            Some(inner)
        );
        doc.find_shape_mut(layer, outer).unwrap().locked = true;
        assert_eq!(containing_frame(&doc, layer, Pt::new(50.0, 50.0)), None);
        doc.find_shape_mut(layer, outer).unwrap().locked = false;
        doc.find_shape_mut(layer, outer).unwrap().visible = false;
        assert_eq!(containing_frame(&doc, layer, Pt::new(50.0, 50.0)), None);
    }

    #[test]
    fn parent_picker_respects_ancestor_clip_and_painter_order() {
        let (mut doc, layer) = rect_doc();
        let outer = add_frame(&mut doc, layer, 0.0, 0.0, 100.0, 100.0);
        let a = add_frame(&mut doc, layer, 80.0, 20.0, 100.0, 100.0);
        let b = add_frame(&mut doc, layer, 80.0, 20.0, 100.0, 100.0);
        for id in [a, b] {
            doc.find_shape_mut(layer, id).unwrap().layout.parent = Some(outer);
        }
        assert_eq!(containing_frame(&doc, layer, Pt::new(90.0, 50.0)), Some(b));
        assert_eq!(containing_frame(&doc, layer, Pt::new(120.0, 50.0)), None);
        doc.find_shape_mut(layer, outer).unwrap().layout.clip = false;
        assert_eq!(containing_frame(&doc, layer, Pt::new(120.0, 50.0)), Some(b));
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
