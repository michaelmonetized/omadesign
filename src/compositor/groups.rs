use super::*;

/// Build the hierarchy once per draw; documents without groups retain the flat fast path.
pub(super) fn draw(
    pm: &mut Pixmap,
    doc: &Document,
    transform: Transform,
    draft: &Draft<'_>,
    motion: Option<f32>,
    overrides: Option<&HashMap<u64, Pose>>,
) {
    if doc
        .layers
        .iter()
        .all(|layer| !layer.is_group && layer.parent.is_none())
    {
        for (index, layer) in doc.layers.iter().enumerate() {
            if !layer.visible || layer.opacity <= 0.0 || is_paper_raster(layer) {
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
            let layer = &self.doc.layers[index];
            if !layer.visible
                || layer.opacity <= 0.0
                || masked_outside_view(layer, self.transform, pm.width(), pm.height())
            {
                continue;
            }
            if !layer.is_group {
                if is_paper_raster(layer) {
                    continue;
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
                );
                continue;
            }
            if layer.pass_through
                && layer.opacity >= 1.0
                && layer.mask.is_none()
                && !layer.filters.active()
            {
                self.draw_children(pm, Some(layer.id), depth + 1);
                continue;
            }
            let passthrough = layer.pass_through && !layer.filters.active();
            let Some(mut temp) = (if passthrough {
                Some(pm.clone())
            } else {
                Pixmap::new(pm.width(), pm.height())
            }) else {
                continue;
            };
            self.draw_children(&mut temp, Some(layer.id), depth + 1);
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
            if passthrough {
                // Interpolate with the original backdrop so child blend modes still see it.
                let opacity = layer.opacity.clamp(0.0, 1.0);
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
                if layer.filters.active() {
                    crate::filter::apply(&mut temp, &layer.filters);
                }
                pm.draw_pixmap(
                    0,
                    0,
                    temp.as_ref(),
                    &PixmapPaint {
                        opacity: layer.opacity.clamp(0.0, 1.0),
                        blend_mode: layer.blend.to_skia(),
                        ..Default::default()
                    },
                    Transform::identity(),
                    None,
                );
            }
        }
    }
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
        let decoded = crate::project::decode(&crate::project::encode(&doc).unwrap()).unwrap();
        assert_eq!(decoded.layers[0].parent, Some(decoded.layers[2].id));
    }
}
