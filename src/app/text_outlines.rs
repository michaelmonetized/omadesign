//! Editable text paint becomes paths inside the existing appearance frame.
use super::*;
use crate::{
    gradient::{Gradient, GradientKind},
    layout::{Constraint, FrameLayout},
};

struct Outline {
    hit: (usize, u64),
    shapes: Vec<Shape>,
}

fn gradient_in_bounds(
    source: &Gradient,
    before: Bounds,
    after: Bounds,
) -> Result<Gradient, String> {
    if source.kind == GradientKind::Shape {
        return Err(
            "Shape-distance text paint needs a shared outline paint space; it was not converted"
                .into(),
        );
    }
    let point = |value: [f32; 2]| {
        [
            (before.min.x + value[0] * before.width().max(0.001) - after.min.x)
                / after.width().max(0.001),
            (before.min.y + value[1] * before.height().max(0.001) - after.min.y)
                / after.height().max(0.001),
        ]
    };
    let mut result = source.clone();
    result.from = point(source.from);
    result.to = point(source.to);
    if source.kind == GradientKind::Radial {
        let transform = source.radial_transform(before);
        result.radial_axis = Some([
            transform.kx / after.width().max(0.001),
            transform.sy / after.height().max(0.001),
        ]);
    }
    Ok(result)
}

fn fill_in_bounds(source: &Fill, before: Bounds, after: Bounds) -> Result<Fill, String> {
    Ok(match source {
        Fill::None | Fill::Solid(_) => source.clone(),
        Fill::Linear { from, to, c0, c1 } => {
            let mut gradient = Gradient::new(GradientKind::Linear, *c0, *c1);
            gradient.from = *from;
            gradient.to = *to;
            let placed = gradient_in_bounds(&gradient, before, after)?;
            Fill::Linear {
                from: placed.from,
                to: placed.to,
                c0: *c0,
                c1: *c1,
            }
        }
        Fill::Radial { c0, c1 } => {
            let mut gradient = Gradient::new(GradientKind::Radial, *c0, *c1);
            let radius = before.width().max(before.height()) * 0.5;
            gradient.to = [0.5 + radius / before.width().max(0.001), 0.5];
            Fill::Gradient(gradient_in_bounds(&gradient, before, after)?)
        }
        Fill::Gradient(gradient) => Fill::Gradient(gradient_in_bounds(gradient, before, after)?),
        Fill::Pattern(pattern) => {
            let old = tiny_skia::Transform::from_translate(before.min.x, before.min.y)
                .pre_scale(before.width().max(0.001), before.height().max(0.001));
            let new = tiny_skia::Transform::from_translate(after.min.x, after.min.y)
                .pre_scale(after.width().max(0.001), after.height().max(0.001));
            let placement = new
                .invert()
                .ok_or("Outline paint bounds are singular")?
                .pre_concat(old)
                .pre_concat(crate::svg_source::transform(pattern.transform));
            let mut result = pattern.clone();
            result.transform = [
                placement.sx,
                placement.ky,
                placement.kx,
                placement.sy,
                placement.tx,
                placement.ty,
            ];
            Fill::Pattern(result)
        }
    })
}

fn path_child(root: &Shape, geom: Geom, style: Style, label: &str) -> Shape {
    let mut child = Shape::new(geom, style);
    child.name = format!("{} {label}", root.name);
    child.layout = FrameLayout {
        parent: Some(root.id),
        constraint_x: Constraint::Scale,
        constraint_y: Constraint::Scale,
        absolute: true,
        ..Default::default()
    };
    child
}

fn prepare(shape: &Shape) -> Result<Vec<Shape>, String> {
    let Geom::Text(run) = &shape.geom else {
        return Err("Select editable text to outline".into());
    };
    if run.svg_text.is_some() {
        return Err(
            "Imported SVG text needs its full styled outline tree; it was not converted".into(),
        );
    }
    if run.contours.is_empty() {
        return Err("This text has no visible outlines to convert".into());
    }
    if !shape.text_wrap.is_none() || shape.layout.image.is_some() || shape.layout.frame {
        return Err("Text with contour wrapping, image fill or existing frame children needs an outline appearance adapter; it was not converted".into());
    }
    if shape
        .style
        .fill
        .gradient()
        .is_some_and(|g| g.kind == GradientKind::Shape)
        || shape
            .style
            .stroke
            .as_ref()
            .and_then(|s| s.gradient.as_ref())
            .is_some_and(|g| g.kind == GradientKind::Shape)
    {
        return Err(
            "Shape-distance text paint needs a shared outline paint space; it was not converted"
                .into(),
        );
    }
    let geometry = crate::text::outline_path(run)
        .and_then(|path| crate::geom::path_geom(&path, false))
        .unwrap_or_else(|| Geom::Poly {
            contours: run.contours.clone(),
            winding: false,
        });
    let painted = crate::text::has_character_paint(run);
    if !painted && run.frame.is_none() && run.on_path.is_none() {
        let mut result = shape.clone();
        result.geom = geometry;
        return Ok(vec![result]);
    }
    let bounds = shape.geom.bbox();
    let mut root = shape.clone();
    root.geom = Geom::Rect {
        origin: bounds.min,
        size: bounds.size(),
        radius: 0.,
    };
    root.style = Style {
        fill: Fill::None,
        stroke: None,
    };
    root.layout.frame = true;
    root.layout.clip = false;
    let mut result = vec![root];
    let fills = if painted {
        crate::text::paint_paths(run, &shape.style.fill)
            .into_iter()
            .filter_map(|(fill, path)| crate::geom::path_geom(&path, true).map(|geom| (fill, geom)))
            .collect::<Vec<_>>()
    } else {
        vec![(shape.style.fill.clone(), geometry.clone())]
    };
    for (index, (fill, geom)) in fills.into_iter().enumerate() {
        let fill = fill_in_bounds(&fill, bounds, geom.bbox())?;
        result.push(path_child(
            shape,
            geom,
            Style { fill, stroke: None },
            &format!("paint {}", index + 1),
        ));
    }
    if let Some(mut stroke) = shape
        .style
        .stroke
        .clone()
        .filter(|stroke| stroke.width > 0.)
    {
        if let Some(gradient) = &stroke.gradient {
            stroke.gradient = Some(gradient_in_bounds(gradient, bounds, geometry.bbox())?);
        }
        result.push(path_child(
            shape,
            geometry,
            Style {
                fill: Fill::None,
                stroke: Some(stroke),
            },
            "stroke",
        ));
    }
    Ok(result)
}

impl Studio {
    fn outline_text_hits(&self) -> Vec<(usize, u64)> {
        let mut hits = Vec::new();
        for &(layer, id) in &self.selection {
            for id in std::iter::once(id).chain(crate::layout::descendants(&self.doc, layer, id)) {
                if !hits.contains(&(layer, id))
                    && self
                        .doc
                        .find_shape(layer, id)
                        .is_some_and(|s| matches!(s.geom, Geom::Text(_)))
                {
                    hits.push((layer, id));
                }
            }
        }
        hits
    }

    fn outline_text_editable(&self, hit: (usize, u64)) -> bool {
        if !self.text_structure_editable(hit) {
            return false;
        }
        let mut current = self
            .doc
            .find_shape(hit.0, hit.1)
            .and_then(|s| s.layout.parent);
        let mut seen = HashSet::new();
        while let Some(id) = current {
            if !seen.insert(id) || seen.len() > 64 {
                return false;
            }
            let Some(parent) = self.doc.find_shape(hit.0, id) else {
                return false;
            };
            if !parent.visible || parent.locked {
                return false;
            }
            current = parent.layout.parent;
        }
        true
    }

    /// Check whether the selected roots contain writable editable text.
    /// Args: none. Returns false for missing, hidden, locked or busy targets and stories.
    pub(crate) fn has_outline_text_targets(&self) -> bool {
        let hits = self.outline_text_hits();
        !hits.is_empty() && hits.iter().all(|&hit| self.outline_text_editable(hit))
    }

    /// Convert selected native text to real editable paths in one Undo operation.
    /// Args: none. Non-text roots stay unchanged; nested text uses its existing parent.
    pub(crate) fn create_text_outlines(&mut self) {
        let hits = self.outline_text_hits();
        self.outline_text_targets(&hits);
    }

    /// Apply preflighted outline plans without changing persistent object or layer identities.
    /// Args: concrete text targets. Returns after one ordinary command transaction or a named refusal.
    pub(super) fn outline_text_targets(&mut self, hits: &[(usize, u64)]) {
        if hits.is_empty() {
            self.status = "Select editable text to outline".into();
            return;
        }
        let mut plans = Vec::new();
        for &hit in hits {
            if !self.outline_text_editable(hit) {
                self.status =
                    "Unlock the text, its parents and its whole story before outlining".into();
                return;
            }
            let paint = |prop| {
                !matches!(
                    prop,
                    crate::motion::Prop::X
                        | crate::motion::Prop::Y
                        | crate::motion::Prop::Rotation
                        | crate::motion::Prop::Scale
                        | crate::motion::Prop::Width
                        | crate::motion::Prop::Height
                        | crate::motion::Prop::Opacity
                )
            };
            if self
                .doc
                .motion
                .tracks
                .iter()
                .any(|track| track.shape == hit.1 && !track.keys.is_empty() && paint(track.prop))
                || self.doc.motion.expressions.iter().any(|expression| {
                    expression.shape == hit.1 && expression.enabled && paint(expression.prop)
                })
            {
                self.status =
                    "Animated text paint needs an outline animation adapter; it was not converted"
                        .into();
                return;
            }
            let shape = self.doc.find_shape(hit.0, hit.1).unwrap();
            match prepare(shape) {
                Ok(shapes) => plans.push(Outline { hit, shapes }),
                Err(error) => {
                    self.status = error;
                    return;
                }
            }
        }
        self.commit_type_edit();
        self.text_structure_transaction(|studio| {
            for plan in plans {
                let selected = studio.selection.iter().position(|hit| *hit == plan.hit);
                if selected.is_none() && studio.actions.depth == 0 {
                    studio
                        .actions
                        .pause("Nested text outlining is outside the recorded selection");
                }
                studio.with_action_step(crate::actions::Step::Outline { selected }, |studio| {
                    let previous = studio.doc.find_shape(plan.hit.0, plan.hit.1).and_then(
                        |shape| match &shape.geom {
                            Geom::Text(run) => run.thread.as_ref().and_then(|thread| thread.prev),
                            _ => None,
                        },
                    );
                    if let Some(previous) = previous {
                        if let Some(layer) = studio.doc.layers.iter().position(|layer| {
                            layer
                                .kind
                                .shapes()
                                .unwrap_or(&[])
                                .iter()
                                .any(|shape| shape.id == previous)
                        }) {
                            studio.break_text_thread((layer, previous));
                        }
                    }
                    studio.break_text_thread(plan.hit);
                    let before = studio.doc.layers[plan.hit.0]
                        .kind
                        .shapes()
                        .unwrap()
                        .to_vec();
                    let mut after = Vec::with_capacity(before.len() + plan.shapes.len() - 1);
                    for shape in &before {
                        if shape.id == plan.hit.1 {
                            after.extend(plan.shapes.iter().cloned());
                        } else {
                            after.push(shape.clone());
                        }
                    }
                    studio.commit(Cmd::SetVectorShapes {
                        layer: plan.hit.0,
                        before,
                        after,
                    });
                });
            }
        });
        self.node_sel.clear();
        self.reset_snap_gesture();
        self.status = format!(
            "{} text object(s) outlined · Undo restores editable text",
            hits.len()
        );
    }
}

#[cfg(test)]
#[path = "text_outlines_tests.rs"]
mod tests;
