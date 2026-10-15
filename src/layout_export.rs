//! Portable frame exports. A selected frame is an explicit data boundary: unrelated
//! layers and siblings never enter the exported document.
mod responsive;

use crate::document::{Document, Fill, Shape};
use crate::geom::{Geom, Pt};
use crate::layout::{Constraint, Sizing, StackAlign, StackAxis, StackFlow, StackJustify};
use std::collections::{BTreeSet, HashMap, HashSet};
use std::fmt::Write;

pub fn frame_document(doc: &Document, layer: usize, frame_id: u64) -> Result<Document, String> {
    let frame = doc
        .find_shape(layer, frame_id)
        .filter(|s| s.layout.frame)
        .ok_or("Select a frame to export")?;
    let bounds = frame.world_bbox();
    if !bounds.width().is_finite()
        || !bounds.height().is_finite()
        || bounds.width() <= 0.0
        || bounds.height() <= 0.0
    {
        return Err("Frame dimensions must be finite and positive".into());
    }
    let mut ids: HashSet<_> = crate::layout::descendants(doc, layer, frame_id)
        .into_iter()
        .collect();
    ids.insert(frame_id);
    let mut layers: HashSet<_> = doc.layer_ancestors(layer).into_iter().collect();
    layers.insert(layer);
    let delta = Pt::new(-bounds.min.x, -bounds.min.y);
    let mut output = Document::new(frame.name.clone(), 1.0, 1.0, doc.dpi);
    output.width = bounds.width().max(1.0);
    output.height = bounds.height().max(1.0);
    output.transparent = true;
    output.artboards = vec![crate::document::Artboard::new(
        0,
        Pt::ZERO,
        Pt::new(output.width, output.height),
    )];
    output.cloud = None;
    output.comments.clear();
    output.guides.clear();
    output.layers = doc
        .layers
        .iter()
        .enumerate()
        .filter(|(i, _)| layers.contains(i))
        .map(|(i, source)| {
            let mut l = source.clone();
            if let Some(shapes) = l.kind.shapes_mut() {
                shapes.retain(|shape| i == layer && ids.contains(&shape.id));
                for shape in shapes {
                    shape.geom.translate(delta);
                    if shape.id == frame_id {
                        shape.layout.parent = None;
                    }
                }
            }
            l.mask_origin += delta;
            l
        })
        .collect();
    retain_mask_sources(&mut output, doc, delta)?;
    let referenced: HashSet<_> = output
        .layers
        .iter()
        .filter_map(|l| l.kind.shapes())
        .flatten()
        .flat_map(|s| {
            crate::layout_tokens::TokenProperty::all()
                .into_iter()
                .filter_map(|property| s.layout.tokens.get(property))
        })
        .collect();
    output.layout_tokens = doc
        .layout_tokens
        .iter()
        .filter(|token| referenced.contains(&token.id))
        .cloned()
        .collect();
    Ok(output)
}

fn retain_mask_sources(
    output: &mut Document,
    original: &Document,
    delta: Pt,
) -> Result<(), String> {
    let mut pending: Vec<_> = output.mask_sources().into_iter().collect();
    if pending.is_empty() {
        return Ok(());
    }
    let mut visited = HashSet::new();
    let mut full_layers = HashSet::new();
    let mut required: HashMap<usize, HashSet<u64>> = HashMap::new();
    let mut artwork = HashSet::new();
    while let Some(source) = pending.pop() {
        if !visited.insert(source) {
            continue;
        }
        let (index, id) = source
            .resolve(original)
            .ok_or("The frame's mask source was removed")?;
        let indices: Vec<_> = if id.is_some() {
            vec![index]
        } else {
            (0..original.layers.len())
                .filter(|&i| i == index || original.layer_ancestors(i).contains(&index))
                .collect()
        };
        for index in indices {
            let layer = &original.layers[index];
            if let Some(id) = id {
                let mut shapes = crate::layout::descendants(original, index, id);
                shapes.push(id);
                for id in shapes {
                    required.entry(index).or_default().insert(id);
                    artwork.insert(id);
                    pending.extend(
                        original
                            .find_shape(index, id)
                            .unwrap()
                            .artwork_masks
                            .iter()
                            .map(|m| m.source),
                    );
                    let mut parent = original.find_shape(index, id).and_then(|s| s.layout.parent);
                    for _ in 0..64 {
                        let Some(id) = parent else {
                            break;
                        };
                        required.entry(index).or_default().insert(id);
                        parent = original.find_shape(index, id).and_then(|s| s.layout.parent);
                    }
                }
            } else {
                full_layers.insert(index);
                pending.extend(layer.artwork_masks.iter().map(|m| m.source));
                for shape in layer.kind.shapes().unwrap_or(&[]) {
                    pending.extend(shape.artwork_masks.iter().map(|m| m.source));
                }
            }
        }
    }
    let mut indices: HashSet<_> = full_layers
        .iter()
        .copied()
        .chain(required.keys().copied())
        .collect();
    for index in indices.clone() {
        indices.extend(original.layer_ancestors(index));
    }
    let mut ordered: Vec<_> = indices.into_iter().collect();
    ordered.sort_unstable();
    for index in ordered {
        let source = &original.layers[index];
        let position = output
            .layers
            .iter()
            .position(|l| l.id == source.id)
            .unwrap_or_else(|| {
                let mut layer = crate::document::Layer::group(&source.name);
                layer.id = source.id;
                layer.parent = source.parent;
                layer.is_group = source.is_group;
                if full_layers.contains(&index) {
                    layer = source.clone();
                    layer.kind = crate::document::LayerKind::Vector { shapes: vec![] };
                }
                output.layers.push(layer);
                output.layers.len() - 1
            });
        let target = &mut output.layers[position];
        if let Some(shapes) = source.kind.shapes() {
            let selected = required.get(&index);
            for shape in shapes.iter().filter(|s| {
                full_layers.contains(&index) || selected.is_some_and(|ids| ids.contains(&s.id))
            }) {
                let list = target
                    .kind
                    .shapes_mut()
                    .ok_or("Invalid mask source layer")?;
                if list.iter().any(|s| s.id == shape.id) {
                    continue;
                }
                let mut shape = shape.clone();
                if !full_layers.contains(&index) && !artwork.contains(&shape.id) {
                    shape.style = crate::document::Style {
                        fill: Fill::None,
                        stroke: None,
                    };
                    shape.layout.image = None;
                    shape.layout.clip = false;
                    shape.opacity = 1.;
                    shape.fill_opacity = 1.;
                    shape.filters = Default::default();
                    shape.mask = None;
                    shape.artwork_masks.clear();
                }
                shape.geom.translate(delta);
                list.push(shape);
            }
        } else if full_layers.contains(&index) {
            target.kind = source.kind.clone();
            if let crate::document::LayerKind::Raster { origin, .. } = &mut target.kind {
                *origin += delta;
            }
        }
        if full_layers.contains(&index) {
            target.mask_origin = source.mask_origin + delta;
        }
    }
    let layer_ids = output.layers.iter().map(|l| (l.id, l.id)).collect();
    let shape_ids = output
        .layers
        .iter()
        .flat_map(|l| l.kind.shapes().unwrap_or(&[]))
        .map(|s| (s.id, s.id))
        .collect();
    let placement = tiny_skia::Transform::from_translate(delta.x, delta.y);
    for layer in &mut output.layers {
        for mask in layer.artwork_masks.iter_mut().chain(
            layer
                .kind
                .shapes_mut()
                .into_iter()
                .flatten()
                .flat_map(|s| &mut s.artwork_masks),
        ) {
            mask.remap(&layer_ids, &shape_ids, placement);
        }
    }
    output.validate_artwork_masks()
}

fn escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

fn axis_css(shape: &Shape, parent: Option<&Shape>, horizontal: bool, css: &mut String) {
    let bounds = shape.geom.bbox();
    let (name, span, sizing, min, max, constraint) = if horizontal {
        (
            "width",
            bounds.width(),
            shape.layout.width,
            shape.layout.min_width,
            shape.layout.max_width,
            shape.layout.constraint_x,
        )
    } else {
        (
            "height",
            bounds.height(),
            shape.layout.height,
            shape.layout.min_height,
            shape.layout.max_height,
            shape.layout.constraint_y,
        )
    };
    if let Some(parent) = parent {
        let pb = parent.geom.bbox();
        let stack = parent
            .layout
            .stack
            .as_ref()
            .filter(|_| !shape.layout.absolute);
        if let Some(stack) = stack {
            let main = horizontal == (stack.direction == StackAxis::Horizontal);
            match sizing {
                Sizing::Fixed => {
                    let _ = write!(css, "{name}:{:.3}px;", span.max(1.));
                }
                Sizing::Hug => {
                    let _ = write!(css, "{name}:max-content;");
                }
                Sizing::Fill if main && stack.flow != StackFlow::Grid => {
                    // Wrapping uses the authored basis when choosing line breaks,
                    // then distributes each line's remaining space like native flow.
                    if stack.flow == StackFlow::Wrap {
                        let _ = write!(css, "flex:1 1 {:.3}px;{name}:0;", span.max(1.));
                    } else {
                        let _ = write!(css, "flex:1 1 0;{name}:0;");
                    }
                }
                Sizing::Fill => {
                    let _ = write!(css, "{name}:100%;align-self:stretch;");
                }
            }
            if sizing != Sizing::Fill && main {
                css.push_str("flex-shrink:0;");
            }
        } else {
            let (start, end, min_pos, max_pos, pmin, pmax) = if horizontal {
                (
                    "left",
                    "right",
                    bounds.min.x,
                    bounds.max.x,
                    pb.min.x,
                    pb.max.x,
                )
            } else {
                (
                    "top",
                    "bottom",
                    bounds.min.y,
                    bounds.max.y,
                    pb.min.y,
                    pb.max.y,
                )
            };
            let parent_span = (pmax - pmin).max(1.0);
            if sizing == Sizing::Fill || constraint == Constraint::Stretch {
                let _ = write!(
                    css,
                    "{start}:{:.3}px;{end}:{:.3}px;{name}:auto;",
                    min_pos - pmin,
                    pmax - max_pos
                );
            } else {
                match constraint {
                    Constraint::End => {
                        let _ = write!(css, "{end}:{:.3}px;", pmax - max_pos);
                    }
                    Constraint::Center => {
                        let _ = write!(
                            css,
                            "{start}:calc(50% + {:.3}px);",
                            min_pos - (pmin + pmax) * 0.5
                        );
                    }
                    Constraint::Scale => {
                        let _ = write!(
                            css,
                            "{start}:{:.5}%;{name}:{:.5}%;",
                            (min_pos - pmin) / parent_span * 100.,
                            span / parent_span * 100.
                        );
                    }
                    _ => {
                        let _ = write!(css, "{start}:{:.3}px;", min_pos - pmin);
                    }
                }
                if constraint != Constraint::Scale {
                    if sizing == Sizing::Hug {
                        let _ = write!(css, "{name}:max-content;");
                    } else {
                        let _ = write!(css, "{name}:{:.3}px;", span.max(1.));
                    }
                }
            }
        }
    } else if horizontal {
        let _ = write!(css, "width:100%;--authored-width:{:.3}px;", span.max(1.));
    } else if sizing == Sizing::Hug {
        css.push_str("height:max-content;");
    } else {
        let _ = write!(css, "height:{:.3}px;", span.max(1.));
    }
    if let Some(min) = min {
        let _ = write!(css, "min-{name}:{:.3}px;", min.max(1.));
    } else {
        let _ = write!(css, "min-{name}:1px;");
    }
    if let Some(max) = max {
        let _ = write!(css, "max-{name}:{:.3}px;", max.max(1.));
    }
}

fn text_box(shape: &Shape) -> Pt {
    let bounds = shape.geom.bbox();
    let mut size = bounds.size();
    if let Geom::Text(run) = &shape.geom {
        let (width, height) = crate::text::measure(run);
        if shape.layout.width == Sizing::Hug
            || (shape.layout.text_size.is_some() && run.wrap_width.is_none())
        {
            size.x = width;
        }
        if shape.layout.height == Sizing::Hug || shape.layout.text_size.is_some() {
            size.y = height;
        }
    }
    size.x = size.x.clamp(1., 1_000_000.);
    size.y = size.y.clamp(1., 1_000_000.);
    size
}

fn fill_css(fill: &Fill) -> String {
    match fill {
        Fill::Gradient(g) => {
            use crate::gradient::GradientKind;
            let stops = g
                .stops
                .iter()
                .map(|s| format!("{} {:.3}%", s.color.css(), s.offset * 100.))
                .collect::<Vec<_>>()
                .join(",");
            let angle = (g.to[1] - g.from[1])
                .atan2(g.to[0] - g.from[0])
                .to_degrees();
            match g.kind {
                GradientKind::Linear => format!("linear-gradient({:.3}deg,{stops})", angle + 90.),
                GradientKind::Conic => format!(
                    "conic-gradient(from {:.3}deg at {:.3}% {:.3}%,{stops})",
                    angle + 90.,
                    g.from[0] * 100.,
                    g.from[1] * 100.
                ),
                _ => format!(
                    "radial-gradient(circle at {:.3}% {:.3}%,{stops})",
                    g.from[0] * 100.,
                    g.from[1] * 100.
                ),
            }
        }
        Fill::None => "transparent".into(),
        Fill::Solid(color) => color.css(),
        Fill::Linear { from, to, c0, c1 } => {
            let angle = (to[0] - from[0]).atan2(-(to[1] - from[1])).to_degrees();
            format!("linear-gradient({angle:.3}deg,{}, {})", c0.css(), c1.css())
        }
        Fill::Radial { c0, c1 } => format!("radial-gradient(ellipse,{}, {})", c0.css(), c1.css()),
    }
}

struct Html<'a> {
    scope: String,
    shapes: HashMap<u64, &'a Shape>,
    children: HashMap<u64, Vec<u64>>,
    fonts: HashMap<String, (String, f32, f32)>,
    styles: String,
    visited: HashSet<u64>,
}

impl<'a> Html<'a> {
    fn font(&mut self, run: &crate::geom::TypeRun) -> (String, f32, f32) {
        if let Some(font) = self.fonts.get(&run.font) {
            return font.clone();
        }
        let family = format!("oma-font-{}", self.fonts.len());
        let bytes = crate::text::font_bytes_for(run).filter(|b| b.len() <= 20_000_000);
        let mut metrics = (0.8, 0.2);
        let name = if let Some(bytes) =
            bytes.filter(|bytes| rustybuzz::Face::from_slice(bytes, 0).is_some())
        {
            if let Some(face) = rustybuzz::Face::from_slice(&bytes, 0) {
                metrics = (
                    face.ascender() as f32 / face.units_per_em() as f32,
                    -(face.descender() as f32) / face.units_per_em() as f32,
                );
            }
            let data = base64::Engine::encode(
                &base64::engine::general_purpose::STANDARD,
                bytes.as_slice(),
            );
            let _ = write!(
                self.styles,
                "@font-face{{font-family:'{family}';src:url(data:font/ttf;base64,{data});font-display:block;}}"
            );
            format!("'{family}',sans-serif")
        } else {
            "sans-serif".into()
        };
        let font = (name, metrics.0, metrics.1);
        self.fonts.insert(run.font.clone(), font.clone());
        font
    }

    fn node(
        &mut self,
        id: u64,
        parent: Option<&Shape>,
        out: &mut String,
        depth: usize,
    ) -> Result<(), String> {
        let original = *self.shapes.get(&id).ok_or("Missing frame child")?;
        let canonical = if matches!(original.geom, Geom::Text(_))
            && original.layout.text_size.is_some()
        {
            let mut canonical = original.clone();
            let Geom::Text(run) = &mut canonical.geom else {
                unreachable!()
            };
            let px = original.layout.text_size.unwrap();
            let ratio = px / run.px.max(1.0);
            if run.leading > 0.0 {
                run.leading *= ratio;
            }
            run.tracking *= ratio;
            run.scale_character_metrics(ratio);
            run.px = px;
            if original.layout.width == Sizing::Hug {
                run.wrap_width = None;
            }
            let before = original.geom.bbox();
            if run.wrap_width.is_none() {
                run.contours = crate::text::shape(run);
                let ink = canonical.geom.bbox();
                canonical.geom.translate(before.min - ink.min);
            } else {
                let (_, height) = crate::text::measure(run);
                crate::layout::set_bounds(
                    &mut canonical.geom,
                    crate::geom::Bounds::from_min_size(before.min, Pt::new(before.width(), height)),
                );
            }
            Some(canonical)
        } else {
            None
        };
        let shape = canonical.as_ref().unwrap_or(original);
        if !shape.visible || shape.guide {
            return Ok(());
        }
        if depth >= 64 || !self.visited.insert(id) {
            return Err("Invalid frame hierarchy in HTML export".into());
        }
        self.styles
            .push_str(&responsive::breakpoints(shape, &self.scope));
        let mut css = String::new();
        let in_stack = parent.is_some_and(|p| p.layout.stack.is_some()) && !shape.layout.absolute;
        css.push_str(if parent.is_none() || in_stack {
            "position:relative;"
        } else {
            "position:absolute;"
        });
        axis_css(shape, parent, true, &mut css);
        axis_css(shape, parent, false, &mut css);
        if let Geom::Text(run) = &shape.geom {
            if run.wrap_width.is_none() {
                let size = text_box(shape);
                if shape.layout.width == Sizing::Hug
                    || (shape.layout.text_size.is_some() && shape.layout.width != Sizing::Fill)
                {
                    let _ = write!(css, "width:{:.6}px;", size.x);
                }
                if shape.layout.height == Sizing::Hug
                    || (shape.layout.text_size.is_some() && shape.layout.height != Sizing::Fill)
                {
                    let _ = write!(css, "height:{:.6}px;", size.y);
                }
                let b = shape.geom.bbox();
                let _ = write!(
                    css,
                    "--ink-width:{:.6};--ink-height:{:.6};--text-scale:1;",
                    b.width(),
                    b.height()
                );
            }
        }
        if let Some(parent) = parent.filter(|_| !shape.layout.absolute) {
            if let Some(stack) = parent
                .layout
                .stack
                .as_ref()
                .filter(|s| s.flow == StackFlow::Grid)
            {
                let children: Vec<_> = self
                    .children
                    .get(&parent.id)
                    .into_iter()
                    .flatten()
                    .filter_map(|id| self.shapes.get(id).copied())
                    .filter(|s| s.visible && !s.guide && !s.layout.absolute)
                    .collect();
                let cells = crate::layout::grid_cells(
                    stack,
                    &children.iter().map(|s| s.layout.grid).collect::<Vec<_>>(),
                );
                if let Some(index) = children.iter().position(|s| s.id == shape.id) {
                    let (col, row, cols, rows) = cells[index];
                    let _ = write!(
                        css,
                        "grid-column:{} / span {cols};grid-row:{} / span {rows};",
                        col + 1,
                        row + 1
                    );
                }
            }
        }
        let _ = write!(css, "opacity:{:.4};", shape.opacity.clamp(0., 1.));
        let _ = write!(css, "mix-blend-mode:{};", shape.blend.css());
        if shape.rotation.abs() > 1e-5 {
            let _ = write!(
                css,
                "transform:rotate({:.5}deg);",
                shape.rotation.to_degrees()
            );
        }
        let rect = matches!(shape.geom, Geom::Rect { .. });
        if rect {
            let corners = shape.effective_corners();
            let _ = write!(
                css,
                "background:{};border-radius:{:.3}px {:.3}px {:.3}px {:.3}px;",
                fill_css(&shape.style.fill),
                corners[0],
                corners[1],
                corners[2],
                corners[3]
            );
            if let Some(stroke) = shape.style.stroke.as_ref().filter(|s| s.width > 0.) {
                let _ = write!(
                    css,
                    "outline:{:.3}px {} {};outline-offset:{:.3}px;",
                    stroke.width,
                    if stroke.dash.is_some() {
                        "dashed"
                    } else {
                        "solid"
                    },
                    stroke.color.css(),
                    -stroke.width * 0.5
                );
            }
        }
        if shape.layout.frame {
            css.push_str("isolation:isolate;");
            css.push_str(if shape.layout.clip {
                "overflow:hidden;"
            } else {
                "overflow:visible;"
            });
            if let Some(stack) = &shape.layout.stack {
                css.push_str(&responsive::stack_css(stack));
            }
        }
        if let Some(ratio) = shape.layout.aspect_ratio {
            let _ = write!(css, "height:auto;aspect-ratio:{ratio:.6};");
        }

        if shape.filters.active() {
            let mut filters = String::new();
            for fx in &shape.filters.items {
                use crate::filter::Fx;
                match fx {
                    Fx::Shadow {
                        dx,
                        dy,
                        blur,
                        color,
                        opacity,
                        ..
                    } => {
                        let _ = write!(
                            filters,
                            "drop-shadow({dx}px {dy}px {blur}px {}) ",
                            crate::color::Rgba::new(
                                color.r,
                                color.g,
                                color.b,
                                (color.a as f32 * opacity.clamp(0., 1.)).round() as u8
                            )
                            .css()
                        );
                    }
                    Fx::Blur { std } => {
                        let _ = write!(filters, "blur({std}px) ");
                    }
                    Fx::Saturate { amount } => {
                        let _ = write!(filters, "saturate({amount}) ");
                    }
                    Fx::Brightness { amount } => {
                        let _ = write!(filters, "brightness({amount}) ");
                    }
                    Fx::Contrast { amount } => {
                        let _ = write!(filters, "contrast({amount}) ");
                    }
                    Fx::HueRotate { degrees } => {
                        let _ = write!(filters, "hue-rotate({degrees}deg) ");
                    }
                    Fx::Invert { amount } => {
                        let _ = write!(filters, "invert({amount}) ");
                    }
                    Fx::InnerShadow {
                        dx,
                        dy,
                        blur,
                        color,
                        opacity,
                        ..
                    } => {
                        let _ = write!(
                            css,
                            "box-shadow:inset {dx}px {dy}px {blur}px {};",
                            crate::color::Rgba::new(
                                color.r,
                                color.g,
                                color.b,
                                (color.a as f32 * opacity.clamp(0., 1.)).round() as u8
                            )
                            .css()
                        );
                    }
                    _ => {}
                }
            }
            if !filters.is_empty() {
                let _ = write!(css, "filter:{filters};");
            }
        }
        let bounds = shape.geom.bbox();
        let basis = text_box(shape);
        let mut metadata = format!(
            " data-basis=\"{:.6},{:.6}\" data-width-sizing=\"{:?}\" data-height-sizing=\"{:?}\"",
            basis.x, basis.y, shape.layout.width, shape.layout.height
        );
        if let Some(stack) = &shape.layout.stack {
            let _ = write!(
                metadata,
                " data-grid=\"{}\"",
                escape(&serde_json::to_string(stack).map_err(|e| e.to_string())?)
            );
            let children: Vec<_> = self
                .children
                .get(&shape.id)
                .into_iter()
                .flatten()
                .filter_map(|id| self.shapes.get(id).copied())
                .filter(|s| s.visible && !s.guide && !s.layout.absolute)
                .collect();
            let advanced = !stack.column_tracks.is_empty()
                || !stack.row_tracks.is_empty()
                || children.iter().any(|s| !s.layout.grid.is_auto());
            let _ = write!(metadata, " data-grid-advanced=\"{advanced}\"");
            let mut widths: Vec<_> = shape
                .layout
                .breakpoints
                .iter()
                .map(|b| b.max_width)
                .filter(|v| v.is_finite() && *v > 0.)
                .collect();
            widths.sort_by(|a, b| b.total_cmp(a));
            widths.dedup();
            for width in widths {
                if let Some(stack) = crate::layout::resolve_for_width(&shape.layout, width)
                    .stack
                    .filter(|s| s.flow == StackFlow::Grid)
                {
                    let cells = crate::layout::grid_cells(
                        &stack,
                        &children.iter().map(|s| s.layout.grid).collect::<Vec<_>>(),
                    );
                    for (child, (col, row, cols, rows)) in children.iter().zip(cells) {
                        let _ = write!(
                            self.styles,
                            "@container(max-width:{width:.3}px){{[data-state=\"{}\"][data-node=\"{}\"]{{grid-column:{} / span {cols}!important;grid-row:{} / span {rows}!important;}}}}",
                            self.scope,
                            child.id,
                            col + 1,
                            row + 1
                        );
                    }
                }
            }
        }
        if let Geom::Text(run) = &shape.geom {
            if run.wrap_width.is_none() {
                metadata.push_str(" data-text-ink");
            }
            if let Some((ascent, descent, line_gap)) = crate::text::font_metrics_for(run) {
                let baseline = crate::text::caret_pt(run, 0).y - bounds.min.y;
                let _ = write!(
                    metadata,
                    " data-baseline=\"{baseline:.6},{ascent:.6},{:.6},{}\"",
                    descent + line_gap.max(0.),
                    run.px
                );
            }
        }
        let interactive = !shape.layout.interactions.is_empty();
        let mut actions =
            serde_json::to_value(&shape.layout.interactions).map_err(|e| e.to_string())?;
        for interaction in actions.as_array_mut().unwrap() {
            for kind in ["Navigate", "OpenOverlay", "SetVariant"] {
                let key = if kind == "SetVariant" {
                    "component"
                } else {
                    "target"
                };
                if let Some(value) = interaction
                    .get_mut("action")
                    .and_then(|a| a.get_mut(kind))
                    .and_then(|a| a.get_mut(key))
                {
                    if let Some(id) = value.as_u64() {
                        *value = serde_json::Value::String(id.to_string());
                    }
                }
            }
        }
        let action_data = escape(&actions.to_string());
        let component = if shape.layout.component.is_some() {
            format!(" data-component=\"{id}\"")
        } else {
            String::new()
        };
        let _ = write!(
            out,
            "<div id=\"oma-{id}\" data-state=\"{}\" data-variant-context=\"{}\" data-node=\"{id}\" data-actions=\"{action_data}\" class=\"node{}\" aria-label=\"{}\"{}{component}{metadata} style=\"{}\">",
            self.scope,
            self.scope,
            if interactive { " interactive" } else { "" },
            escape(&shape.name),
            if interactive {
                " role=\"button\" tabindex=\"0\""
            } else {
                " tabindex=\"-1\""
            },
            escape(&css)
        );
        if let Some(image) = shape.layout.image.as_ref().filter(|_| rect) {
            let fit = match image.fit {
                crate::layout_images::ImageFit::Cover => "cover",
                crate::layout_images::ImageFit::Contain => "contain",
                crate::layout_images::ImageFit::Stretch => "fill",
            };
            let _ = write!(
                out,
                "<img alt=\"{}\" src=\"data:image/png;base64,{}\" style=\"position:absolute;inset:0;width:100%;height:100%;object-fit:{fit};object-position:{:.3}% {:.3}%;border-radius:inherit;pointer-events:none\">",
                escape(&shape.name),
                escape(&image.data),
                image.focal.x * 100.,
                image.focal.y * 100.
            );
        }
        if let Geom::Text(run) = &shape.geom {
            if run.on_path.is_some()
                || run.frame.is_some()
                || !run.manual_kern.is_empty()
                || run.spans.iter().any(|s| {
                    s.kerning == Some(crate::geom::KernMode::Optical)
                        || s.hscale.is_some()
                        || s.vscale.is_some()
                })
            {
                let mut art = shape.clone();
                art.rotation = 0.;
                art.opacity = 1.;
                art.blend = crate::color::Blend::Normal;
                out.push_str(&crate::svg::shape_fragment(&art));
                out.push_str("</div>");
                return Ok(());
            }
            let (font, asc, desc) = self.font(run);
            let b = shape.geom.bbox();
            if run.wrap_width.is_none() && run.spans.is_empty() && run.features.is_empty() {
                let _ = write!(
                    out,
                    "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"{} {} {} {}\" width=\"100%\" height=\"100%\" style=\"overflow:visible\"><text font-family=\"{}\" font-size=\"{}\" letter-spacing=\"{}\" fill=\"{}\" style=\"font-kerning:{};font-feature-settings:'liga' {},'tnum' {},'smcp' {}\" xml:space=\"preserve\">",
                    b.min.x,
                    b.min.y,
                    b.width().max(1.),
                    b.height().max(1.),
                    escape(&font),
                    run.px,
                    run.tracking,
                    match &shape.style.fill {
                        Fill::Solid(c) => c.css(),
                        _ => "#111111".into(),
                    },
                    if run.kern { "normal" } else { "none" },
                    u8::from(run.liga),
                    u8::from(run.tnum),
                    u8::from(run.smcp)
                );
                for line in crate::text::compose(run).iter() {
                    let _ = write!(
                        out,
                        "<tspan x=\"{}\" y=\"{}\">{}</tspan>",
                        run.origin.x + line.offset,
                        run.origin.y + line.baseline,
                        escape(&line.text)
                    );
                }
                out.push_str("</text></svg></div>");
                return Ok(());
            }
            let px = shape.layout.text_size.unwrap_or(run.px);
            let leading = run.line_height() / run.px.max(1.) * px;
            let baseline = (leading - (asc + desc) * px) * 0.5 + asc * px;
            let mut text_style = format!(
                "position:relative;display:block;left:{:.3}px;top:{:.3}px;font-family:{font};font-size:{:.3}px;line-height:{:.3}px;letter-spacing:{:.3}px;font-kerning:{};font-feature-settings:'liga' {},'tnum' {},'smcp' {};white-space:pre;color:{};",
                run.origin.x - b.min.x,
                run.origin.y - b.min.y - baseline,
                px,
                leading,
                run.tracking,
                if run.kern { "normal" } else { "none" },
                u8::from(run.liga),
                u8::from(run.tnum),
                u8::from(run.smcp),
                match &shape.style.fill {
                    Fill::Solid(c) => c.css(),
                    _ => "#111111".into(),
                }
            );
            if let Some(width) = run.wrap_width {
                let _ = write!(
                    text_style,
                    "width:{};max-width:100%;white-space:pre-wrap;overflow-wrap:anywhere;text-align:{};",
                    if shape.layout.width == Sizing::Fill {
                        "100%".into()
                    } else {
                        format!("{width:.3}px")
                    },
                    match run.align {
                        crate::geom::TextAlign::Start => "left",
                        crate::geom::TextAlign::Center => "center",
                        crate::geom::TextAlign::End => "right",
                        crate::geom::TextAlign::Justify { .. } => "justify",
                    }
                );
            }
            if let crate::geom::TextAlign::Justify { last } = run.align {
                let _ = write!(
                    text_style,
                    "text-align-last:{};",
                    match last {
                        crate::geom::LastLine::Start => "left",
                        crate::geom::LastLine::Center => "center",
                        crate::geom::LastLine::End => "right",
                        crate::geom::LastLine::Justify => "justify",
                    }
                );
            }
            let _ = write!(
                out,
                "<span style=\"{}\">{}</span>",
                escape(&text_style),
                crate::text::paragraph_html(run)
            );
        } else if !rect {
            let mut art = shape.clone();
            art.rotation = 0.;
            art.opacity = 1.;
            art.blend = crate::color::Blend::Normal;
            out.push_str(&crate::svg::shape_fragment(&art));
        }
        for child in self.children.get(&id).cloned().unwrap_or_default() {
            self.node(child, Some(shape), out, depth + 1)?;
        }
        out.push_str("</div>");
        Ok(())
    }
}

/// Assign stable export-only IDs to nodes introduced by a variant swap.
/// Args: materialized document, original identities and next portable identity.
/// Returns success after remapping parents, interactions and component baselines.
fn portable_variant_ids(
    doc: &mut Document,
    known: &HashSet<u64>,
    next: &mut u64,
) -> Result<(), String> {
    let mut ids = HashMap::new();
    for shape in doc.layers.iter().filter_map(|l| l.kind.shapes()).flatten() {
        if !known.contains(&shape.id) {
            ids.insert(shape.id, *next);
            *next = next
                .checked_add(1)
                .ok_or("Export object identity overflow")?;
        }
    }
    for shape in doc
        .layers
        .iter_mut()
        .filter_map(|l| l.kind.shapes_mut())
        .flatten()
    {
        shape.id = ids.get(&shape.id).copied().unwrap_or(shape.id);
        shape.layout.parent = shape
            .layout
            .parent
            .map(|id| ids.get(&id).copied().unwrap_or(id));
        let family = match &shape.layout.component {
            Some(crate::layout_components::ComponentBinding::Main { family, .. }) => Some(*family),
            _ => None,
        };
        crate::layout_components::remap_duplicate(shape, &ids);
        if let (
            Some(before),
            Some(crate::layout_components::ComponentBinding::Main { family, .. }),
        ) = (family, &mut shape.layout.component)
        {
            *family = ids.get(&before).copied().unwrap_or(before);
        }
    }
    Ok(())
}

pub fn export_html(doc: &Document, layer: usize, frame_id: u64) -> Result<String, String> {
    crate::layout::validate_hierarchy(doc)?;
    let frame = doc
        .find_shape(layer, frame_id)
        .filter(|s| s.layout.frame)
        .ok_or("Select a frame to export")?;
    let mut html = Html {
        scope: String::new(),
        shapes: HashMap::new(),
        children: HashMap::new(),
        fonts: HashMap::new(),
        styles: String::new(),
        visited: HashSet::new(),
    };
    let mut pending = std::collections::BTreeMap::new();
    let mut first = doc.layout_snapshot();
    for layer in 0..first.layers.len() {
        crate::layout::reflow_roots(&mut first, layer);
    }
    pending.insert(format!("screen-{frame_id}"), (first, frame_id, false));
    let mut emitted = BTreeSet::new();
    let mut screens = BTreeSet::new();
    let mut contexts = std::collections::BTreeMap::new();
    let mut targets = BTreeSet::new();
    let mut body = String::new();
    let mut next = doc
        .layers
        .iter()
        .filter_map(|l| l.kind.shapes())
        .flatten()
        .map(|s| s.id)
        .max()
        .unwrap_or(0)
        .checked_add(1)
        .ok_or("Export object identity overflow")?;
    loop {
        while let Some((scope, (state, root, is_variant))) = pending.pop_first() {
            if !emitted.insert(scope.clone()) {
                continue;
            }
            if emitted.len() > 1280 {
                return Err("Prototype contains too many component variant combinations".into());
            }
            if !is_variant && screens.insert(root) && screens.len() > 256 {
                return Err("Prototype has more than 256 linked screens".into());
            }
            let owners: HashMap<_, _> = state
                .layers
                .iter()
                .enumerate()
                .flat_map(|(li, l)| {
                    l.kind
                        .shapes()
                        .unwrap_or(&[])
                        .iter()
                        .map(move |s| (s.id, li))
                })
                .collect();
            let shapes: Vec<_> = state
                .layers
                .iter()
                .filter_map(|l| l.kind.shapes())
                .flatten()
                .collect();
            let mut tree = Html {
                scope: scope.clone(),
                shapes: shapes.iter().map(|s| (s.id, *s)).collect(),
                children: HashMap::new(),
                fonts: html.fonts.clone(),
                styles: String::new(),
                visited: HashSet::new(),
            };
            for shape in shapes {
                if let Some(parent) = shape.layout.parent {
                    tree.children.entry(parent).or_default().push(shape.id);
                }
            }
            let parent = is_variant
                .then(|| {
                    state
                        .find_shape(owners[&root], root)
                        .and_then(|s| s.layout.parent)
                        .and_then(|id| state.find_shape(owners[&root], id))
                })
                .flatten();
            let mut content = String::new();
            tree.node(root, parent, &mut content, 0)?;
            let _ = write!(body, "<template id=\"{scope}\">{content}</template>");
            let mut visited: Vec<_> = tree.visited.iter().copied().collect();
            visited.sort_unstable();
            for node in visited {
                let shape = tree.shapes[&node];
                if matches!(
                    shape.layout.component,
                    Some(crate::layout_components::ComponentBinding::Instance { .. })
                ) && !(is_variant && node == root)
                {
                    contexts
                        .entry((scope.clone(), node))
                        .or_insert_with(|| state.layout_snapshot());
                }
                for interaction in &shape.layout.interactions {
                    crate::layout_prototype::validate_interaction(
                        &state,
                        owners[&node],
                        node,
                        interaction,
                    )
                    .map_err(|e| format!("Invalid prototype link on '{}': {e}", shape.name))?;
                    use crate::layout_prototype::PrototypeAction;
                    match interaction.action {
                        PrototypeAction::Navigate { target }
                        | PrototypeAction::OpenOverlay { target } => {
                            let key = format!("screen-{target}");
                            if !emitted.contains(&key) {
                                pending
                                    .entry(key)
                                    .or_insert_with(|| (state.layout_snapshot(), target, false));
                            }
                        }
                        PrototypeAction::SetVariant { component } => {
                            targets.insert(component);
                        }
                        _ => {}
                    }
                }
            }
            html.fonts = tree.fonts;
            html.styles.push_str(&tree.styles);
        }
        for ((context, instance), state) in &contexts {
            for &target in &targets {
                if crate::layout_components::family_of(state, *instance)
                    != crate::layout_components::family_of(state, target)
                {
                    continue;
                }
                let scope = format!("variant-{context}-{instance}-{target}");
                if emitted.contains(&scope) || pending.contains_key(&scope) {
                    continue;
                }
                let mut variant = state.layout_snapshot();
                let layer = variant
                    .layers
                    .iter()
                    .position(|l| {
                        l.kind
                            .shapes()
                            .is_some_and(|s| s.iter().any(|s| s.id == *instance))
                    })
                    .ok_or("Missing variant instance")?;
                let known = variant
                    .layers
                    .iter()
                    .filter_map(|l| l.kind.shapes())
                    .flatten()
                    .map(|s| s.id)
                    .collect();
                crate::layout_components::swap_variant(&mut variant, layer, *instance, target)?;
                portable_variant_ids(&mut variant, &known, &mut next)?;
                crate::layout::reflow(&mut variant, layer, *instance);
                pending.insert(scope, (variant, *instance, true));
            }
        }
        if pending.is_empty() {
            break;
        }
    }
    let script = PROTOTYPE_RUNTIME.replace("__ROOT__", &format!("\"{frame_id}\""));
    Ok(format!(
        "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\"><title>{}</title><style>{BASE_CSS}{}</style></head><body><main id=\"stage\" aria-label=\"Design prototype\"></main><dialog id=\"overlay\" aria-label=\"Prototype overlays\"></dialog><p id=\"prototype-error\" role=\"alert\"></p>{body}<script>{script}</script></body></html>",
        escape(&frame.name),
        html.styles
    ))
}

const BASE_CSS: &str = "*{box-sizing:border-box}body{margin:0;background:#17191d;font-family:system-ui,sans-serif}#stage{display:flex;justify-content:center;min-height:100vh;width:clamp(240px,100%,2560px);margin:0 auto;container-type:inline-size}.node{flex-shrink:0}#stage>.node{clip-path:inset(0)}.node>svg{display:block;overflow:visible}.node[data-text-ink]>svg{width:calc(var(--ink-width)*var(--text-scale)*1px);height:calc(var(--ink-height)*var(--text-scale)*1px);max-width:none}.interactive{cursor:pointer}.interactive:focus-visible{outline:2px solid #5b9fff!important;outline-offset:3px!important}#overlay{position:fixed;inset:auto;margin:0;container-type:inline-size;padding:0;border:0;background:transparent;max-width:none;max-height:none;overflow:hidden}#overlay::backdrop{background:transparent}.overlay-layer{position:absolute;inset:0;background:rgba(0,0,0,0.392157)}.overlay-layer>.node{position:absolute!important}#prototype-error{color:#fff;background:#a22;margin:0}#prototype-error:empty{display:none}@media(prefers-reduced-motion:reduce){*{animation:none!important;transition:none!important}}";
const PROTOTYPE_RUNTIME: &str = include_str!("layout_export/prototype.js");

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color::Rgba;
    use crate::document::{Layer, Style};
    fn rect(name: &str, origin: Pt, size: Pt, color: Rgba) -> Shape {
        let mut s = Shape::new(
            Geom::Rect {
                origin,
                size,
                radius: 0.,
            },
            Style {
                fill: Fill::Solid(color),
                stroke: None,
            },
        );
        s.name = name.into();
        s
    }
    fn scene() -> (Document, u64, u64) {
        let mut doc = Document::new("Export boundary", 100., 100., 72.);
        doc.transparent = true;
        doc.layers = vec![Layer::vector("Artwork")];
        let mut frame = rect(
            "Selected",
            Pt::new(-20., -10.),
            Pt::new(60., 40.),
            Rgba::rgb(255, 255, 255),
        );
        frame.layout = crate::layout::FrameLayout::frame();
        let mut child = rect(
            "Public child",
            Pt::new(-10., 0.),
            Pt::new(20., 20.),
            Rgba::rgb(0, 255, 0),
        );
        child.layout.parent = Some(frame.id);
        let private = rect(
            "PRIVATE SIBLING",
            Pt::new(-20., -10.),
            Pt::new(60., 40.),
            Rgba::rgb(255, 0, 0),
        );
        let ids = (frame.id, child.id);
        *doc.layers[0].kind.shapes_mut().unwrap() = vec![frame, child, private];
        (doc, ids.0, ids.1)
    }
    #[test]
    fn frame_png_and_svg_exclude_overlapping_siblings_and_support_negative_origins() {
        let (doc, frame, child) = scene();
        let image = crate::compositor::export_frame_png(&doc, 0, frame, 2).unwrap();
        let pixels = tiny_skia::Pixmap::decode_png(&image).unwrap();
        assert_eq!((pixels.width(), pixels.height()), (120, 80));
        assert_eq!(pixels.pixel(30, 30).unwrap().green(), 255);
        assert_eq!(pixels.pixel(30, 30).unwrap().red(), 0);
        assert_eq!(pixels.pixel(5, 5).unwrap().red(), 255);
        assert_eq!(pixels.pixel(5, 5).unwrap().green(), 255);
        let svg = crate::svg::export_frame(&doc, 0, frame).unwrap();
        assert!(!svg.contains("PRIVATE"));
        assert!(svg.contains(&format!("id=\"oma-{child}\"")));
        assert!(svg.contains(&format!("clip-path=\"url(#oma-frame-clip-{frame})\"")));
        assert_eq!(
            roxmltree::Document::parse(&svg)
                .unwrap()
                .root_element()
                .attribute("width"),
            Some("60")
        );
        assert_eq!(
            doc.find_shape(0, frame).unwrap().geom.bbox().min,
            Pt::new(-20., -10.)
        );
    }
    #[test]
    fn html_shadow_and_inner_shadow_multiply_effect_and_color_opacity() {
        let (mut doc, frame, child) = scene();
        let shape = doc.find_shape_mut(0, child).unwrap();
        shape.filters.items = vec![
            crate::filter::Fx::Shadow {
                dx: 1.,
                dy: 2.,
                blur: 3.,
                color: Rgba::new(10, 20, 30, 128),
                blend: crate::color::Blend::Normal,
                opacity: 0.,
                knockout: true,
                spread: 0.,
            },
            crate::filter::Fx::InnerShadow {
                dx: 4.,
                dy: 5.,
                blur: 6.,
                color: Rgba::new(40, 50, 60, 128),
                blend: crate::color::Blend::Normal,
                opacity: 0.5,
                choke: 0.,
            },
        ];
        let html = export_html(&doc, 0, frame).unwrap();
        assert!(
            html.contains("drop-shadow(1px 2px 3px rgba(10,20,30,0.000))"),
            "{html}"
        );
        assert!(
            html.contains("box-shadow:inset 4px 5px 6px rgba(40,50,60,0.251);"),
            "{html}"
        );
    }

    #[test]
    fn html_preserves_containment_layout_typography_and_escapes_user_content() {
        let (mut doc, frame, child) = scene();
        let root = doc.find_shape_mut(0, frame).unwrap();
        root.layout.stack = Some(crate::layout::AutoStack {
            direction: StackAxis::Horizontal,
            flow: StackFlow::Wrap,
            justify: StackJustify::SpaceBetween,
            ..Default::default()
        });
        let nested = doc.find_shape_mut(0, child).unwrap();
        nested.layout.frame = true;
        nested.layout.width = Sizing::Fill;
        let mut run = crate::geom::TypeRun {
            content: "<script>private()</script> & \"text\"".into(),
            origin: Pt::new(-8., 12.),
            px: 14.,
            tracking: 1.5,
            wrap_width: Some(40.),
            align: crate::geom::TextAlign::Center,
            ..Default::default()
        };
        run.contours = crate::text::shape(&run);
        let mut text = Shape::new(
            Geom::Text(run),
            Style {
                fill: Fill::Solid(Rgba::BLACK),
                stroke: None,
            },
        );
        text.layout.parent = Some(child);
        let tid = text.id;
        doc.layers[0].kind.shapes_mut().unwrap().push(text);
        let html = export_html(&doc, 0, frame).unwrap();
        assert!(!html.contains("PRIVATE SIBLING") && !html.contains("<script>private()"));
        assert!(html.contains("&lt;script&gt;private()&lt;/script&gt;"));
        assert!(
            html.contains("flex-wrap:wrap")
                && html.contains("justify-content:space-between")
                && html.contains("flex:1 1 28.000px")
        );
        assert!(
            html.contains("font-size:14.000px")
                && html.contains("letter-spacing:1.500px")
                && html.contains("text-align:center")
        );
        let start = html
            .find(&format!("<template id=\"screen-{frame}\">"))
            .unwrap();
        let content = html[start..]
            .split_once('>')
            .unwrap()
            .1
            .split_once("</template>")
            .unwrap()
            .0;
        let parsed = roxmltree::Document::parse(content).unwrap();
        let node = parsed
            .descendants()
            .find(|n| n.attribute("id") == Some(format!("oma-{tid}").as_str()))
            .unwrap();
        assert_eq!(
            node.parent_element().unwrap().attribute("id"),
            Some(format!("oma-{child}").as_str())
        );
    }
    #[test]
    fn html_includes_only_reachable_prototype_screens() {
        let (mut doc, frame, _) = scene();
        let mut destination = rect(
            "Destination",
            Pt::new(110., 0.),
            Pt::new(60., 40.),
            Rgba::rgb(0, 0, 255),
        );
        destination.layout = crate::layout::FrameLayout::frame();
        let target = destination.id;
        doc.layers[0].kind.shapes_mut().unwrap().push(destination);
        doc.find_shape_mut(0, frame)
            .unwrap()
            .layout
            .interactions
            .push(crate::layout_prototype::Interaction {
                trigger: crate::layout_prototype::Trigger::Click,
                action: crate::layout_prototype::PrototypeAction::Navigate { target },
                transition: crate::layout_prototype::Transition::Dissolve,
                duration_ms: 200,
            });
        let html = export_html(&doc, 0, frame).unwrap();
        assert!(html.contains(&format!("id=\"screen-{target}\"")));
        assert!(html.contains("&quot;Navigate&quot;:{&quot;target&quot;:"));
        assert!(!html.contains("PRIVATE SIBLING"));
        assert_eq!(html.matches("<template ").count(), 2);
    }
}

#[cfg(test)]
mod depth_tests;
