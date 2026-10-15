use super::*;
use crate::artwork_mask::MaskSource;

/// Build the hierarchy once per draw; documents without groups retain the flat fast path.
pub(super) fn draw(
    pm: &mut Pixmap,
    doc: &Document,
    transform: Transform,
    draft: &Draft<'_>,
    motion: Option<f32>,
    overrides: Option<&HashMap<u64, Pose>>,
) {
    let sources = doc.mask_sources();
    if doc
        .layers
        .iter()
        .all(|layer| !layer.is_group && layer.parent.is_none() && layer.svg_filter.is_none())
    {
        for (index, layer) in doc.layers.iter().enumerate() {
            if sources.contains(&MaskSource {
                layer: layer.id,
                shape: None,
            }) || !layer.visible
                || (layer.kind.pixels().is_none() && layer.opacity <= 0.0)
                || is_paper_raster(layer)
            {
                continue;
            }
            let brush = draft
                .brush
                .and_then(|(i, p, f)| (i == index).then_some((p, f)));
            draw_layer(
                pm,
                layer,
                transform,
                brush,
                draft.preview,
                motion,
                doc,
                overrides,
                &sources,
            );
        }
        return;
    }
    let mut children: HashMap<Option<u64>, Vec<usize>> = HashMap::new();
    for (index, layer) in doc.layers.iter().enumerate() {
        children.entry(layer.parent).or_default().push(index);
    }
    let context = Context {
        doc,
        transform,
        draft,
        motion,
        overrides,
        children,
        sources,
    };
    context.draw_children(pm, None, 0);
}

struct Context<'a, 'b> {
    doc: &'a Document,
    transform: Transform,
    draft: &'a Draft<'b>,
    motion: Option<f32>,
    overrides: Option<&'a HashMap<u64, Pose>>,
    children: HashMap<Option<u64>, Vec<usize>>,
    sources: std::collections::HashSet<MaskSource>,
}

impl Context<'_, '_> {
    fn draw_children(&self, pm: &mut Pixmap, parent: Option<u64>, depth: usize) {
        if depth > 64 {
            return;
        }
        let Some(children) = self.children.get(&parent) else {
            return;
        };
        for &index in children {
            if self.sources.contains(&MaskSource {
                layer: self.doc.layers[index].id,
                shape: None,
            }) {
                continue;
            }
            self.draw_layer(pm, index, depth);
        }
    }

    fn draw_layer(&self, pm: &mut Pixmap, index: usize, depth: usize) {
        if depth > 64 {
            return;
        }
        let layer = &self.doc.layers[index];
        let mut filters = layer.filters.clone();
        crate::filter::scale(&mut filters, self.transform.get_scale().0);
        if !layer.visible
            || (layer.kind.pixels().is_none()
                && (layer.opacity <= 0.0
                    || masked_outside_view(layer, self.transform, pm.width(), pm.height())))
        {
            return;
        }
        if !layer.is_group && layer.svg_filter.is_none() {
            if is_paper_raster(layer) {
                return;
            }
            let brush = self
                .draft
                .brush
                .and_then(|(i, p, f)| (i == index).then_some((p, f)));
            draw_layer(
                pm,
                layer,
                self.transform,
                brush,
                self.draft.preview,
                self.motion,
                self.doc,
                self.overrides,
                &self.sources,
            );
            return;
        }
        if layer.pass_through
            && layer.opacity >= 1.0
            && layer.fill_opacity >= 1.0
            && layer.mask.is_none()
            && layer.artwork_masks.is_empty()
            && !layer.filters.active()
            && layer.svg_filter.is_none()
        {
            self.draw_children(pm, Some(layer.id), depth + 1);
            return;
        }
        let passthrough = layer.pass_through
            && layer.artwork_masks.is_empty()
            && !layer.filters.active()
            && layer.svg_filter.is_none();
        let Some(mut temp) = (if passthrough {
            Some(pm.clone())
        } else {
            Pixmap::new(pm.width(), pm.height())
        }) else {
            return;
        };
        if layer.svg_filter.is_some() {
            match svg_effects::render(
                self.doc,
                index,
                self.transform,
                pm.width(),
                pm.height(),
                self.motion,
                self.overrides,
            ) {
                Ok(surface) => temp = surface,
                Err(error) => {
                    eprintln!("SVG effect graph: {error}");
                    self.draw_children(&mut temp, Some(layer.id), depth + 1);
                }
            }
        } else {
            self.draw_children(&mut temp, Some(layer.id), depth + 1);
        }
        let mask = layer.mask.as_ref().and_then(|pixels| {
            let mut placed = Pixmap::new(pm.width(), pm.height())?;
            pixels.with_pm(|mask| {
                placed.draw_pixmap(
                    0,
                    0,
                    mask.as_ref(),
                    &PixmapPaint::default(),
                    self.transform.pre_concat(layer_pixel_transform(layer)),
                    None,
                )
            })?;
            Some(tiny_skia::Mask::from_pixmap(
                placed.as_ref(),
                tiny_skia::MaskType::Luminance,
            ))
        });
        let live = artwork_masks::render(
            &layer.artwork_masks,
            self.doc,
            self.transform,
            pm.width(),
            pm.height(),
            self.motion,
            self.overrides,
        );
        if passthrough {
            // Interpolate with the original backdrop so child blend modes still see it.
            let opacity = (layer.opacity * layer.fill_opacity).clamp(0.0, 1.0);
            for (i, (dst, src)) in pm
                .data_mut()
                .as_chunks_mut::<4>()
                .0
                .iter_mut()
                .zip(temp.data().as_chunks::<4>().0)
                .enumerate()
            {
                let amount = opacity
                    * mask
                        .as_ref()
                        .map(|m| m.data()[i] as f32 / 255.0)
                        .unwrap_or(1.0);
                for c in 0..4 {
                    dst[c] =
                        (dst[c] as f32 * (1.0 - amount) + src[c] as f32 * amount).round() as u8;
                }
            }
        } else {
            if let Some(mask) = mask {
                temp.apply_mask(&mask);
            }
            if filters.independent() || layer.fill_opacity < 1. || layer.blend_interior {
                crate::filter::composite(
                    pm,
                    temp,
                    &filters,
                    Transform::identity(),
                    layer.blend.to_skia(),
                    layer.opacity,
                    layer.fill_opacity,
                    layer.blend_interior,
                    live.as_ref(),
                );
                return;
            }
            let cached;
            let (pixels, coverage) = if filters.active() {
                cached = surface_cache::render(layer.id, temp, &filters);
                (cached.as_ref(), surface_cache::bounds(layer.id, &cached))
            } else {
                (&temp, bounded_blit::bounds(temp.as_ref()))
            };
            if let Some(mask) = &live {
                pm.draw_pixmap(
                    0,
                    0,
                    pixels.as_ref(),
                    &PixmapPaint {
                        opacity: layer.opacity.clamp(0., 1.),
                        blend_mode: layer.blend.to_skia(),
                        ..Default::default()
                    },
                    Transform::identity(),
                    Some(mask),
                );
            } else {
                bounded_blit::draw(
                    pm,
                    pixels.as_ref(),
                    coverage,
                    layer.opacity,
                    layer.blend,
                    Transform::identity(),
                );
            }
        }
    }
}

pub(super) fn draw_item(
    pm: &mut Pixmap,
    doc: &Document,
    index: usize,
    transform: Transform,
    motion: Option<f32>,
    overrides: Option<&HashMap<u64, Pose>>,
) {
    let mut children: HashMap<Option<u64>, Vec<usize>> = HashMap::new();
    for (i, layer) in doc.layers.iter().enumerate() {
        children.entry(layer.parent).or_default().push(i);
    }
    Context {
        doc,
        transform,
        draft: &Draft::none(),
        motion,
        overrides,
        children,
        sources: doc.mask_sources(),
    }
    .draw_layer(pm, index, 0);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        color::Blend,
        document::{Layer, Pixels},
    };
    #[test]
    fn group_opacity_is_applied_once_and_hidden_parent_hides_children() {
        let mut doc = Document::new("groups", 2., 2., 72.);
        doc.transparent = true;
        doc.layers.clear();
        let mut group = Layer::group("Group");
        group.pass_through = false;
        group.opacity = 0.5;
        for color in [[255, 0, 0, 255], [0, 0, 255, 255]] {
            let mut layer = Layer::placed_raster(
                "child",
                Pixels::from_rgba(2, 2, color.repeat(4)).unwrap(),
                Pt::ZERO,
                Pt::new(2., 2.),
            );
            layer.parent = Some(group.id);
            doc.layers.push(layer);
        }
        doc.layers.push(group);
        let pm = render_export(&doc, 1).unwrap();
        assert_eq!(pm.pixels()[0].alpha(), 128);
        assert_eq!(pm.pixels()[0].blue(), 128);
        doc.layers[2].visible = false;
        assert_eq!(render_export(&doc, 1).unwrap().pixels()[0].alpha(), 0);
        assert!(doc.hit_test(Pt::new(1., 1.), 1.).is_none());
        doc.layers[2].visible = true;
        doc.layers[2].pass_through = true;
        doc.layers[1].blend = Blend::Multiply;
        assert_eq!(render_export(&doc, 1).unwrap().pixels()[0].alpha(), 128);
        let opacity_pixels = render_export(&doc, 1).unwrap();
        doc.layers[2].opacity = 1.;
        doc.layers[2].fill_opacity = 0.5;
        assert_eq!(
            render_export(&doc, 1).unwrap().data(),
            opacity_pixels.data(),
            "Fill opacity must preserve a pass-through child's backdrop"
        );
        let decoded = crate::project::decode(&crate::project::encode(&doc).unwrap()).unwrap();
        assert_eq!(decoded.layers[0].parent, Some(decoded.layers[2].id));
    }
}
