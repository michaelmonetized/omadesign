//! Preserve visual effects that cannot be represented by the editable PDF
//! writer. Rasterization is scoped to the affected layer or compositing group.

use super::*;
use crate::document::Pixels;
use std::borrow::Cow;
use std::collections::HashSet;

/// Reduce only appearances that PDF cannot represent, using the current native subtree.
/// Args: editable source document. Returns an immutable source view or a bounded appearance copy with named notes.
pub(super) fn prepare(
    document: &Document,
) -> Result<(Cow<'_, Document>, BTreeSet<String>), String> {
    for layer in &document.layers {
        let Some(graph) = layer.svg_filter.as_ref().filter(|g| !g.filters.is_empty()) else {
            continue;
        };
        if graph.transform.iter().any(|v| !v.is_finite())
            || crate::svg_source::transform(graph.transform)
                .invert()
                .is_none_or(|t| !t.is_finite())
        {
            return Err(format!(
                "PDF layer '{}' ({}) has an invalid retained filter transform",
                layer.name, layer.id
            ));
        }
        let source = format!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" xmlns:xlink=\"http://www.w3.org/1999/xlink\">{}</svg>",
            graph.definitions
        );
        let xml = roxmltree::Document::parse(&source).map_err(|e| {
            format!(
                "PDF layer '{}' has invalid retained filter source: {e}",
                layer.name
            )
        })?;
        for id in &graph.filters {
            if !xml
                .descendants()
                .any(|n| n.has_tag_name("filter") && n.attribute("id") == Some(id.as_str()))
            {
                return Err(format!(
                    "PDF layer '{}' ({}) is missing retained filter '{id}'",
                    layer.name, layer.id
                ));
            }
        }
    }
    let mut affected = document
        .layers
        .iter()
        .enumerate()
        .filter(|(_, l)| needs_pixels(l))
        .map(|(i, _)| i)
        .collect::<HashSet<_>>();
    if affected.is_empty() {
        return Ok((Cow::Borrowed(document), BTreeSet::new()));
    }
    let sources = document.mask_sources();
    let bake_masks = document.layers.iter().enumerate().any(|(i, l)| {
        let selected = affected.contains(&i)
            || document
                .layer_ancestors(i)
                .iter()
                .any(|a| affected.contains(a));
        selected
            && (!l.artwork_masks.is_empty()
                || l.kind
                    .shapes()
                    .is_some_and(|s| s.iter().any(|s| !s.artwork_masks.is_empty())))
    }) || sources
        .iter()
        .filter_map(|s| s.resolve(document))
        .any(|(i, _)| {
            affected.iter().any(|a| {
                *a == i
                    || document.layer_ancestors(i).contains(a)
                    || document.layer_ancestors(*a).contains(&i)
            })
        });
    if bake_masks {
        affected.extend(
            document
                .layers
                .iter()
                .enumerate()
                .filter(|(_, l)| {
                    !l.artwork_masks.is_empty()
                        || l.kind
                            .shapes()
                            .is_some_and(|s| s.iter().any(|s| !s.artwork_masks.is_empty()))
                })
                .map(|(i, _)| i),
        );
    }
    let mut roots = affected
        .iter()
        .copied()
        .filter(|i| {
            !document
                .layer_ancestors(*i)
                .iter()
                .any(|a| affected.contains(a))
        })
        .collect::<Vec<_>>();
    roots.sort_unstable();
    let needs_backdrop = roots.iter().any(|i| {
        std::iter::once(*i)
            .chain(document.layer_ancestors(*i))
            .any(|a| {
                let group = &document.layers[a];
                group.is_group
                    && group.pass_through
                    && group.svg_filter.is_none()
                    && !group.filters.active()
                    && group.artwork_masks.is_empty()
                    && document.layers.iter().enumerate().any(|(child, l)| {
                        l.blend != Blend::Normal && document.layer_ancestors(child).contains(&a)
                    })
            })
    });
    let effect_backdrop = document.layers.iter().any(|l| {
        l.filters.independent() && (l.filters.blends_backdrop() || l.blend != Blend::Normal)
    });
    if needs_backdrop || effect_backdrop {
        return bake_pages(document);
    }
    let mut prepared = document.clone();
    let mut removed = HashSet::new();
    let mut replacements = HashMap::new();
    let mut warnings = BTreeSet::new();
    let mut budget = 0;
    for index in roots {
        let original = &document.layers[index];
        let ids = document
            .layers
            .iter()
            .enumerate()
            .filter(|(i, _)| *i == index || document.layer_ancestors(*i).contains(&index))
            .map(|(_, l)| l.id)
            .collect::<HashSet<_>>();
        let mut isolated = document.clone();
        isolated.transparent = true;
        for (layer_index, layer) in isolated.layers.iter_mut().enumerate() {
            let source_layer = document.mask_source_member(layer_index, None, &sources);
            let source_shapes = sources
                .iter()
                .any(|s| s.layer == layer.id && s.shape.is_some());
            if layer.id == original.id {
                layer.parent = None;
                layer.visible = true;
                layer.blend = Blend::Normal;
            } else if !ids.contains(&layer.id)
                && layer.parent.is_none()
                && !source_layer
                && !source_shapes
            {
                layer.visible = false;
            }
            if !ids.contains(&layer.id) && !source_layer {
                layer.artwork_masks.clear();
                if let Some(shapes) = layer.kind.shapes_mut() {
                    for shape in shapes {
                        if !sources.contains(&MaskSource {
                            layer: layer.id,
                            shape: Some(shape.id),
                        }) {
                            shape.artwork_masks.clear();
                            if source_shapes {
                                shape.visible = false;
                            }
                        }
                    }
                }
            }
        }
        let active_sources = isolated.mask_sources();
        for layer in &mut isolated.layers {
            if let Some(shapes) = layer.kind.shapes_mut() {
                for shape in shapes {
                    let source = MaskSource {
                        layer: layer.id,
                        shape: Some(shape.id),
                    };
                    if sources.contains(&source) && !active_sources.contains(&source) {
                        shape.visible = false;
                    }
                }
            }
        }
        let selected = document
            .layers
            .iter()
            .filter(|l| ids.contains(&l.id))
            .cloned()
            .collect::<Vec<_>>();
        let mut bounds = selected
            .iter()
            .filter_map(layer_bounds)
            .reduce(|a, b| a.union(b))
            .unwrap_or(Bounds::from_min_size(Pt::ZERO, Pt::splat(1.)));
        let has_graph = selected
            .iter()
            .any(|l| l.svg_filter.as_ref().is_some_and(|g| !g.filters.is_empty()));
        if has_graph {
            let source = crate::svg::export_posed(&isolated, &HashMap::new())?;
            let tree = crate::svg_source::scene(&source)?;
            let b = tree.root().abs_layer_bounding_box();
            bounds = bounds.union(Bounds::from_min_size(
                Pt::new(b.x(), b.y()),
                Pt::new(b.width(), b.height()),
            ));
            for layer in &selected {
                if let Some(graph) = &layer.svg_filter {
                    for note in
                        graph
                            .reductions
                            .iter()
                            .cloned()
                            .chain(crate::svg_source::reductions(&format!(
                                "<svg xmlns=\"http://www.w3.org/2000/svg\" xmlns:xlink=\"http://www.w3.org/1999/xlink\">{}</svg>",
                                graph.definitions
                            )))
                    {
                        warnings.insert(format!(
                            "PDF layer '{}' ({}): {note}",
                            original.name, original.id
                        ));
                    }
                }
            }
        } else {
            bounds = bounds.inflate(
                selected
                    .iter()
                    .filter(|l| l.is_group)
                    .map(|l| crate::filter::svg_pad(&l.filters))
                    .sum(),
            );
        }
        if !document.artboards.is_empty() {
            let page = document
                .artboards
                .iter()
                .map(|b| Bounds::from_min_size(b.origin, b.size))
                .reduce(|a, b| a.union(b))
                .unwrap();
            let min = Pt::new(bounds.min.x.max(page.min.x), bounds.min.y.max(page.min.y));
            let max = Pt::new(bounds.max.x.min(page.max.x), bounds.max.y.min(page.max.y));
            bounds = Bounds::from_min_size(
                min,
                Pt::new((max.x - min.x).max(1.), (max.y - min.y).max(1.)),
            );
        }
        let bounds = round_bounds(bounds);
        check_budget(bounds, &mut budget)?;
        let scene = scene(&isolated, bounds, isolated.layers.clone(), true);
        let pm = crate::compositor::render_export(&scene, 1)?;
        let mut replacement = Layer::placed_raster(
            &original.name,
            Pixels::from_pixmap(&pm)?,
            bounds.min,
            Pt::new(bounds.width(), bounds.height()),
        );
        replacement.id = original.id;
        replacement.parent = original.parent;
        replacement.visible = original.visible;
        replacement.locked = original.locked;
        replacement.blend =
            if original.is_group && original.pass_through && original.svg_filter.is_none() {
                Blend::Normal
            } else {
                original.blend
            };
        for id in &ids {
            if *id != original.id {
                removed.insert(*id);
            }
        }
        replacements.insert(original.id, replacement);
        warnings.insert(format!("PDF layer '{}' ({}) preserves {} as a bounded RGBA8 raster layer; affected objects {:?} and editable source remain in .oma.",original.name,original.id,if has_graph{"the current retained SVG filter graph and subtree"}else{"unsupported filters, shape/conic gradients, frame composition, or stroke alignment"},selected.iter().flat_map(|l|l.kind.shapes().unwrap_or(&[])).map(|s|s.id).collect::<Vec<_>>()));
    }
    prepared.layers.retain(|l| !removed.contains(&l.id));
    for layer in &mut prepared.layers {
        if let Some(replacement) = replacements.remove(&layer.id) {
            *layer = replacement;
        }
        if bake_masks {
            layer.artwork_masks.clear();
            if sources.contains(&MaskSource {
                layer: layer.id,
                shape: None,
            }) {
                layer.visible = false;
            }
            if let Some(shapes) = layer.kind.shapes_mut() {
                for s in shapes {
                    s.artwork_masks.clear();
                    if sources.contains(&MaskSource {
                        layer: layer.id,
                        shape: Some(s.id),
                    }) {
                        s.visible = false;
                    }
                }
            }
        }
    }
    if bake_masks {
        warnings.insert("PDF export renders live masked compositions when unsupported effects affect their targets or source artwork; ordinary vector masks remain editable without those appearances.".into());
    }
    Ok((Cow::Owned(prepared), warnings))
}

fn needs_pixels(layer: &Layer) -> bool {
    layer.svg_filter.as_ref().is_some_and(|g|!g.filters.is_empty())
        || layer.filters.active()
        || layer.blend_interior
        || layer.kind.shapes().is_some_and(|shapes| {
            shapes.iter().any(|shape| {
                shape.filters.active()
                    || shape.blend_interior
                    || shape.layout.image.is_some()
                    || shape.style.stroke.as_ref().is_some_and(|s| {
                        s.alignment != crate::document::StrokeAlignment::Center
                            || s.gradient.as_ref().is_some_and(unsupported_gradient)
                    })
                    || unsupported_fill(&shape.style.fill)
                    || matches!(&shape.geom,Geom::Text(run) if run.spans.iter().filter_map(|s|s.fill.as_ref()).any(unsupported_fill))
            })
        })
}

/// Identify gradient kinds without a PDF shading equivalent.
/// Args: gradient. Returns true when native appearance fallback is required.
fn unsupported_gradient(g: &Gradient) -> bool {
    !matches!(g.kind, GradientKind::Linear | GradientKind::Radial)
}
fn unsupported_fill(fill: &Fill) -> bool {
    fill.gradient().as_ref().map_or(
        !matches!(fill, Fill::None | Fill::Solid(_) | Fill::Pattern(_)),
        unsupported_gradient,
    )
}

pub(super) fn layer_bounds(layer: &Layer) -> Option<Bounds> {
    let bounds = match &layer.kind {
        LayerKind::Vector { shapes } => shapes
            .iter()
            .filter(|shape| shape.visible && !shape.guide)
            .map(|shape| {
                shape.world_bbox().inflate(
                    shape
                        .style
                        .stroke
                        .as_ref()
                        .map_or(0., |stroke| stroke.width * 0.5)
                        + crate::filter::svg_pad(&shape.filters),
                )
            })
            .reduce(|a, b| a.union(b)),
        LayerKind::Raster { .. } => layer.kind.raster_bounds(),
    }?;
    Some(bounds.inflate(crate::filter::svg_pad(&layer.filters)))
}

fn round_bounds(bounds: Bounds) -> Bounds {
    Bounds::from_min_size(
        Pt::new(bounds.min.x.floor(), bounds.min.y.floor()),
        Pt::new(
            (bounds.max.x.ceil() - bounds.min.x.floor()).max(1.),
            (bounds.max.y.ceil() - bounds.min.y.floor()).max(1.),
        ),
    )
}

fn check_budget(bounds: Bounds, budget: &mut usize) -> Result<(), String> {
    let w = bounds.width();
    let h = bounds.height();
    if !w.is_finite()
        || !h.is_finite()
        || w <= 0.
        || h <= 0.
        || f64::from(w) * f64::from(h) > 64. * 1024. * 1024.
    {
        return Err("PDF effect raster exceeds the 64-megapixel limit".into());
    }
    let bytes = (w as usize)
        .checked_mul(h as usize)
        .and_then(|n| n.checked_mul(4))
        .ok_or("PDF effect raster size overflow")?;
    *budget = budget
        .checked_add(bytes)
        .filter(|size| *size <= 512 * 1024 * 1024)
        .ok_or("PDF effect rasters exceed the 512 MiB export budget")?;
    Ok(())
}

fn scene(
    document: &Document,
    bounds: Bounds,
    mut layers: Vec<Layer>,
    transparent: bool,
) -> Document {
    let mut scene = document.clone();
    scene.name = "PDF appearance".into();
    scene.width = bounds.width();
    scene.height = bounds.height();
    scene.transparent = transparent;
    scene.artboards = vec![Artboard::new(
        0,
        Pt::ZERO,
        Pt::new(bounds.width(), bounds.height()),
    )];
    let layer_ids = layers
        .iter()
        .map(|l| (l.id, l.id))
        .collect::<HashMap<_, _>>();
    let shape_ids = layers
        .iter()
        .flat_map(|l| l.kind.shapes().unwrap_or(&[]))
        .map(|s| (s.id, s.id))
        .collect::<HashMap<_, _>>();
    let placement = tiny_skia::Transform::from_translate(-bounds.min.x, -bounds.min.y);
    for layer in &mut layers {
        if let Some(graph) = &mut layer.svg_filter {
            let t = placement.pre_concat(crate::svg_source::transform(graph.transform));
            graph.transform = [t.sx, t.ky, t.kx, t.sy, t.tx, t.ty];
        }
        for mask in &mut layer.artwork_masks {
            mask.remap(&layer_ids, &shape_ids, placement);
        }
        if let Some(shapes) = layer.kind.shapes_mut() {
            for shape in shapes {
                for mask in &mut shape.artwork_masks {
                    mask.remap(&layer_ids, &shape_ids, placement);
                }
            }
        }
        match &mut layer.kind {
            LayerKind::Vector { shapes } => {
                for shape in shapes {
                    shape.geom.translate(-bounds.min);
                }
                if let Some(mask) = &layer.mask {
                    if layer.mask_size.x <= 0. || layer.mask_size.y <= 0. {
                        layer.mask_size = Pt::new(mask.w as f32, mask.h as f32);
                    }
                    layer.mask_origin += -bounds.min;
                }
            }
            LayerKind::Raster { origin, .. } => *origin += -bounds.min,
        }
    }
    scene.layers = layers;
    scene
}

fn bake_pages(document: &Document) -> Result<(Cow<'_, Document>, BTreeSet<String>), String> {
    let mut prepared = document.clone();
    prepared.layers.clear();
    let fallback = vec![Artboard::new(
        0,
        Pt::ZERO,
        Pt::new(document.width, document.height),
    )];
    let boards = if document.artboards.is_empty() {
        &fallback
    } else {
        &document.artboards
    };
    let mut budget = 0;
    for board in boards {
        let bounds = round_bounds(Bounds::from_min_size(board.origin, board.size));
        check_budget(bounds, &mut budget)?;
        let scene = scene(
            document,
            bounds,
            document.layers.clone(),
            document.transparent,
        );
        let pm = crate::compositor::render_export(&scene, 1)?;
        prepared.layers.push(Layer::placed_raster(
            &board.name,
            Pixels::from_pixmap(&pm)?,
            bounds.min,
            Pt::new(bounds.width(), bounds.height()),
        ));
    }
    Ok((Cow::Owned(prepared),BTreeSet::from(["PDF export rasterizes pages containing backdrop-dependent pass-through effects to preserve their appearance; use .oma to retain the editable layer hierarchy.".into()])))
}
