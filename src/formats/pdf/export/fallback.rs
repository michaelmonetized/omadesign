//! Preserve visual effects that cannot be represented by the editable PDF
//! writer. Rasterization is scoped to the affected layer or compositing group.

use super::*;
use crate::document::Pixels;
use std::borrow::Cow;
use std::collections::HashSet;

pub(super) fn prepare(
    document: &Document,
) -> Result<(Cow<'_, Document>, BTreeSet<String>), String> {
    if document.layers.iter().any(|layer| {
        layer
            .svg_filter
            .as_ref()
            .is_some_and(|graph| !graph.filters.is_empty())
    }) {
        return Err("PDF export of retained SVG filter graphs requires the integrated graph adapter; save the editable .oma document instead.".into());
    }
    let affected = document
        .layers
        .iter()
        .enumerate()
        .filter(|(_, layer)| needs_pixels(layer))
        .map(|(index, _)| index)
        .collect::<HashSet<_>>();
    if affected.is_empty() {
        return Ok((Cow::Borrowed(document), BTreeSet::new()));
    }
    let sources = document.mask_sources();
    let interacts_with_masks = document.layers.iter().enumerate().any(|(index, layer)| {
        let selected = affected.contains(&index)
            || document
                .layer_ancestors(index)
                .iter()
                .any(|i| affected.contains(i));
        selected
            && (!layer.artwork_masks.is_empty()
                || layer
                    .kind
                    .shapes()
                    .is_some_and(|shapes| shapes.iter().any(|s| !s.artwork_masks.is_empty()))
                || sources
                    .iter()
                    .any(|source| source.resolve(document).is_some_and(|(i, _)| i == index)))
    });
    if interacts_with_masks {
        let rendered = crate::compositor::raster_export_document(document)?;
        let (prepared, mut warnings) = prepare(&rendered)?;
        warnings.insert("PDF export renders live masked compositions when unsupported effects or frame/stroke appearances affect their targets or source artwork; ordinary vector masks remain editable without those appearances.".into());
        return Ok((Cow::Owned(prepared.into_owned()), warnings));
    }
    let roots = affected
        .iter()
        .copied()
        .filter(|index| {
            !document
                .layer_ancestors(*index)
                .iter()
                .any(|ancestor| affected.contains(ancestor))
        })
        .collect::<Vec<_>>();
    let needs_backdrop = roots.iter().any(|index| {
        std::iter::once(*index)
            .chain(document.layer_ancestors(*index))
            .any(|ancestor| {
                let group = &document.layers[ancestor];
                group.is_group
                    && group.pass_through
                    && document.layers.iter().enumerate().any(|(child, layer)| {
                        layer.blend != Blend::Normal
                            && document.layer_ancestors(child).contains(&ancestor)
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
    let mut byte_budget = 0usize;
    for index in roots {
        let original = &document.layers[index];
        let selected = document
            .layers
            .iter()
            .enumerate()
            .filter(|(child, _)| {
                *child == index || document.layer_ancestors(*child).contains(&index)
            })
            .map(|(_, layer)| layer.clone())
            .collect::<Vec<_>>();
        let mut bounds: Option<Bounds> = None;
        let mut group_pad = 0.;
        for layer in &selected {
            if layer.is_group {
                group_pad += crate::filter::svg_pad(&layer.filters);
                continue;
            }
            if let Some(next) = layer_bounds(layer) {
                bounds = Some(bounds.map_or(next, |bounds| bounds.union(next)));
            }
        }
        let bounds = round_bounds(
            bounds
                .unwrap_or(Bounds::from_min_size(Pt::ZERO, Pt::splat(1.)))
                .inflate(group_pad),
        );
        check_budget(bounds, &mut byte_budget)?;
        let mut scene = scene(document, bounds, selected.clone(), true);
        let root = scene
            .layers
            .iter_mut()
            .find(|layer| layer.id == original.id)
            .ok_or("Missing PDF fallback layer")?;
        root.parent = None;
        root.visible = true;
        root.blend = Blend::Normal;
        let pm = crate::compositor::render_export(&scene, 1)?;
        let mut replacement = Layer::placed_raster(
            &original.name,
            Pixels::from_pixmap(&pm),
            bounds.min,
            Pt::new(bounds.width(), bounds.height()),
        );
        replacement.id = original.id;
        replacement.parent = original.parent;
        replacement.visible = original.visible;
        replacement.locked = original.locked;
        replacement.blend = if original.is_group && original.pass_through {
            Blend::Normal
        } else {
            original.blend
        };
        for layer in &selected {
            if layer.id != original.id {
                removed.insert(layer.id);
            }
        }
        replacements.insert(original.id, replacement);
        warnings.insert(if original.is_group {
            "PDF export preserves complex group appearance as a raster layer; its internal objects remain editable in the .oma project.".into()
        }else{
            "PDF export preserves unsupported filters, shape/conic gradients, frame composition, or stroke alignment as raster layers; supported vector paint and masks stay editable.".into()
        });
    }
    prepared.layers.retain(|layer| !removed.contains(&layer.id));
    for layer in &mut prepared.layers {
        if let Some(replacement) = replacements.remove(&layer.id) {
            *layer = replacement;
        }
    }
    Ok((Cow::Owned(prepared), warnings))
}

fn needs_pixels(layer: &Layer) -> bool {
    layer.filters.active()
        || layer.blend_interior
        || layer.kind.shapes().is_some_and(|shapes| {
            shapes.iter().any(|shape| {
                shape.filters.active()
                    || shape.blend_interior
                    || shape.layout.parent.is_some()
                    || shape.style.stroke.as_ref().is_some_and(|s| {
                        s.alignment != crate::document::StrokeAlignment::Center
                            || s.gradient.as_ref().is_some_and(unsupported_gradient)
                    })
                    || shape.style.fill.gradient().as_ref().map_or(
                        !matches!(shape.style.fill, Fill::None | Fill::Solid(_)),
                        unsupported_gradient,
                    )
            })
        })
}

/// Identify gradient kinds without a PDF shading equivalent.
/// Args: gradient. Returns true when native appearance fallback is required.
fn unsupported_gradient(g: &Gradient) -> bool {
    !matches!(g.kind, GradientKind::Linear | GradientKind::Radial)
        || g.radial_axis.is_some()
        || g.focal != [0., 0.]
        || g.inner_radius != 0.
        || g.spread != crate::gradient::Spread::Pad
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
    let mut scene = Document::new("PDF appearance", 1., 1., document.dpi);
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
            Pixels::from_pixmap(&pm),
            bounds.min,
            Pt::new(bounds.width(), bounds.height()),
        ));
    }
    Ok((Cow::Owned(prepared),BTreeSet::from(["PDF export rasterizes pages containing backdrop-dependent pass-through effects to preserve their appearance; use .oma to retain the editable layer hierarchy.".into()])))
}
