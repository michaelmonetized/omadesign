use super::*;
use crate::artwork_mask::{ArtworkMask, MaskMode};
use std::cell::Cell;

thread_local! { static DEPTH: Cell<usize> = const { Cell::new(0) }; }

struct Depth;
impl Drop for Depth {
    fn drop(&mut self) {
        DEPTH.with(|depth| depth.set(depth.get() - 1));
    }
}

pub(super) fn render(
    masks: &[ArtworkMask],
    doc: &Document,
    view: Transform,
    w: u32,
    h: u32,
    motion: Option<f32>,
    overrides: Option<&HashMap<u64, Pose>>,
) -> Option<tiny_skia::Mask> {
    if masks.is_empty() {
        return None;
    }
    let mut result = tiny_skia::Mask::new(w, h)?;
    if DEPTH.with(|depth| depth.get() >= 64) {
        return Some(result);
    }
    DEPTH.with(|depth| depth.set(depth.get() + 1));
    let _depth = Depth;
    result.data_mut().fill(255);
    for mask in masks {
        let mut image = Pixmap::new(w, h)?;
        let placement = view.pre_concat(mask.placement());
        if let Some((index, _)) = mask.source.resolve(doc) {
            if let Some(id) = mask.source.shape {
                frames::draw_item(&mut image, doc, index, id, placement, motion, overrides);
            } else {
                groups::draw_item(&mut image, doc, index, placement, motion, overrides);
            }
        }
        let mut values = tiny_skia::Mask::from_pixmap(
            image.as_ref(),
            match mask.mode {
                MaskMode::Alpha => tiny_skia::MaskType::Alpha,
                MaskMode::Luminance => tiny_skia::MaskType::Luminance,
            },
        );
        if let Some(bounds) = mask.region {
            let mut region = tiny_skia::Mask::new(w, h)?;
            if let Some(rect) = tiny_skia::Rect::from_xywh(
                bounds.min.x,
                bounds.min.y,
                bounds.width(),
                bounds.height(),
            ) {
                region.fill_path(
                    &PathBuilder::from_rect(rect),
                    FillRule::Winding,
                    true,
                    placement.pre_concat(mask.region_placement()),
                );
            }
            multiply(&mut values, &region);
        }
        multiply(&mut result, &values);
    }
    Some(result)
}

pub(super) fn multiply(target: &mut tiny_skia::Mask, other: &tiny_skia::Mask) {
    for (a, b) in target.data_mut().iter_mut().zip(other.data()) {
        *a = ((u16::from(*a) * u16::from(*b) + 127) / 255) as u8;
    }
}

/// Render live masks for formats that cannot store artwork references.
/// `doc` stays editable. Returns a copy with only affected layers or groups rasterized.
pub(crate) fn raster_export_document(doc: &Document) -> Result<Document, String> {
    super::admit_precise_export(doc)?;
    doc.validate_hierarchy()?;
    let width = doc.width.round().max(1.) as u32;
    let height = doc.height.round().max(1.) as u32;
    let affected: std::collections::HashSet<_> =
        doc.layers
            .iter()
            .enumerate()
            .filter(|(index, layer)| {
                !doc.is_mask_source(*index, None)
                    && (!layer.artwork_masks.is_empty()
                        || layer.kind.shapes().is_some_and(|shapes| {
                            shapes.iter().any(|s| !s.artwork_masks.is_empty())
                        }))
            })
            .map(|(index, _)| index)
            .collect();
    let roots: Vec<_> = affected
        .iter()
        .copied()
        .filter(|&index| {
            !doc.layer_ancestors(index)
                .iter()
                .any(|i| affected.contains(i))
        })
        .collect();
    if u64::from(width) * u64::from(height) > 64 * 1024 * 1024
        || u64::from(width) * u64::from(height) * 4 * roots.len() as u64 > 512 * 1024 * 1024
    {
        return Err("Rendered artwork masks exceed the export pixel budget".into());
    }
    let mut scene = doc.clone();
    let mut output = doc.clone();
    let mut removed = std::collections::HashSet::new();
    for index in roots {
        let original = &doc.layers[index];
        scene.layers[index].opacity = 1.;
        scene.layers[index].blend = crate::color::Blend::Normal;
        scene.layers[index].visible = true;
        let mut pixels =
            Pixmap::new(width, height).ok_or("Could not allocate artwork mask export")?;
        groups::draw_item(
            &mut pixels,
            &scene,
            index,
            Transform::identity(),
            None,
            None,
        );
        scene.layers[index] = original.clone();
        let replacement = &mut output.layers[index];
        replacement.kind = LayerKind::Raster {
            pixels: crate::document::Pixels::from_pixmap(&pixels)?,
            origin: Pt::ZERO,
            size: Pt::new(width as f32, height as f32),
            rotation: 0.,
            shear: 0.,
        };
        replacement.is_group = false;
        replacement.pass_through = false;
        replacement.mask = None;
        replacement.fill_opacity = 1.;
        replacement.blend_interior = false;
        replacement.filters = Default::default();
        for child in 0..doc.layers.len() {
            if doc.layer_ancestors(child).contains(&index) {
                removed.insert(doc.layers[child].id);
            }
        }
    }
    let sources = doc.mask_sources();
    for layer in &mut output.layers {
        layer.artwork_masks.clear();
        if sources.contains(&crate::artwork_mask::MaskSource {
            layer: layer.id,
            shape: None,
        }) {
            layer.visible = false;
        }
        if let Some(shapes) = layer.kind.shapes_mut() {
            for shape in shapes {
                shape.artwork_masks.clear();
                if sources.contains(&crate::artwork_mask::MaskSource {
                    layer: layer.id,
                    shape: Some(shape.id),
                }) {
                    shape.visible = false;
                }
            }
        }
    }
    output.layers.retain(|layer| !removed.contains(&layer.id));
    Ok(output)
}
