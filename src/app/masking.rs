use super::*;
use crate::document::Pixels;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SelectionSpace {
    pub w: u32,
    pub h: u32,
    pub transform: tiny_skia::Transform,
}

#[derive(Clone)]
pub struct ItemMask {
    pub values: Vec<u8>,
    pub space: SelectionSpace,
}

fn resample(values: &[u8], source: SelectionSpace, target: SelectionSpace) -> Option<Vec<u8>> {
    if values.len() != source.w as usize * source.h as usize {
        return None;
    }
    if source == target {
        return Some(values.to_vec());
    }
    let data = values.iter().flat_map(|v| [255, 255, 255, *v]).collect();
    let src = Pixels::from_rgba(source.w, source.h, data)?.to_pixmap()?;
    let mut dest = tiny_skia::Pixmap::new(target.w, target.h)?;
    let transform = target.transform.invert()?.pre_concat(source.transform);
    dest.draw_pixmap(
        0,
        0,
        src.as_ref(),
        &tiny_skia::PixmapPaint {
            quality: tiny_skia::FilterQuality::Bilinear,
            ..Default::default()
        },
        transform,
        None,
    );
    Some(dest.pixels().iter().map(|p| p.alpha()).collect())
}

impl Studio {
    /// Finish or restore an in-flight pixel edit before changing document context.
    /// Returning true lets Undo cancel the current gesture before touching history.
    pub(crate) fn end_pixel_stroke(&mut self, cancel: bool) -> bool {
        let (layer, mask, before, buffer) = match self.op.take() {
            Some(Op::Retouch {
                layer,
                mask,
                before,
                ..
            }) => (layer, mask, before, None),
            Some(Op::Smudge { layer, before, .. } | Op::Clone { layer, before, .. }) => {
                (layer, false, before, None)
            }
            Some(Op::Brush {
                layer, before, buf, ..
            }) => (layer, false, before, Some(buf)),
            other => {
                self.op = other;
                return false;
            }
        };
        let Some(target) = self.doc.layers.get_mut(layer) else {
            return true;
        };
        let pixels = if mask {
            target.mask.as_mut()
        } else {
            target.kind.pixels_mut()
        };
        if let Some(pixels) = pixels {
            if cancel {
                pixels.data = before;
                pixels.touch();
            } else {
                if let Some(buffer) = buffer
                    && let Some(mut destination) = pixels.to_pixmap()
                {
                    paint::blend_onto(&mut destination, &buffer, self.brush.opacity);
                    *pixels = Pixels::from_pixmap(&destination);
                }
                if pixels.data != before {
                    self.history.push(Cmd::Pixels {
                        layer,
                        mask,
                        before,
                        after: pixels.data.clone(),
                    });
                    self.dirty = true;
                }
            }
        }
        self.reset_snap_gesture();
        self.mark();
        true
    }

    fn layer_selection_space(&self, layer: usize) -> Option<SelectionSpace> {
        let item = self.doc.layers.get(layer)?;
        let (w, h) = self.mask_dimensions(layer)?;
        Some(SelectionSpace {
            w,
            h,
            transform: compositor::layer_pixel_transform(item),
        })
    }

    pub fn pixel_sel_mask(&self, layer: usize) -> Option<std::borrow::Cow<'_, [u8]>> {
        let target = self.layer_selection_space(layer)?;
        let mask = self.pixel_sel.as_ref()?;
        let source = self.pixel_sel_space.unwrap_or(target);
        if source == target && mask.len() == target.w as usize * target.h as usize {
            Some(std::borrow::Cow::Borrowed(mask))
        } else {
            resample(mask, source, target).map(std::borrow::Cow::Owned)
        }
    }

    pub fn set_pixel_sel(&mut self, mask: Option<Vec<u8>>) {
        self.pixel_sel_space = if mask.is_some() {
            self.raster_target()
                .and_then(|i| self.layer_selection_space(i))
                .or(Some(SelectionSpace {
                    w: self.doc.width.ceil().max(1.) as u32,
                    h: self.doc.height.ceil().max(1.) as u32,
                    transform: tiny_skia::Transform::identity(),
                }))
        } else {
            None
        };
        self.pixel_sel = mask;
        self.pixel_sel_gen = self.pixel_sel_gen.wrapping_add(1);
        self.status = self.pixel_sel.as_ref().map_or_else(
            || "Pixel selection cleared".into(),
            |mask| format!("{} pixels selected", paint::selected_count(mask)),
        );
    }

    pub fn nudge_pixel_sel(&mut self, dx: i32, dy: i32) {
        self.retouch_pixel_sel(|mask, w, h| paint::shift_mask(mask, w, h, dx, dy));
        self.status = "Selection moved".into();
    }

    pub fn scale_pixel_sel(&mut self, sx: f32, sy: f32) {
        self.retouch_pixel_sel(|mask, w, h| paint::scale_mask(mask, w, h, sx, sy));
        self.status = "Selection resized".into();
    }

    pub fn feather_pixel_sel(&mut self, radius: u32) {
        self.retouch_pixel_sel(|mask, w, h| paint::feather_mask(mask, w, h, radius));
        self.status = format!("Selection feathered {radius} px");
    }

    pub fn distort_pixel_sel(&mut self, kx: f32) {
        self.retouch_pixel_sel(|mask, w, h| paint::shear_mask(mask, w, h, kx));
        self.status = "Selection distorted".into();
    }

    fn retouch_pixel_sel(&mut self, edit: impl FnOnce(&[u8], u32, u32) -> Vec<u8>) {
        let Some(space) = self.pixel_sel_space else {
            return;
        };
        let Some(mask) = self.pixel_sel.clone() else {
            return;
        };
        if mask.len() != space.w as usize * space.h as usize {
            return;
        }
        self.pixel_sel = Some(edit(&mask, space.w, space.h));
        self.pixel_sel_gen = self.pixel_sel_gen.wrapping_add(1);
    }

    pub fn merge_pixel_sel(&mut self, next: Vec<u8>, op: paint::PixelCombine) {
        let existing = self.raster_target().and_then(|li| self.pixel_sel_mask(li));
        let combined = paint::combine_masks(existing.as_deref(), &next, op);
        self.set_pixel_sel(Some(combined));
    }

    fn item_mask(&self, layer: usize, id: Option<u64>) -> Option<ItemMask> {
        use tiny_skia::Transform;
        let mut bounds = Bounds::from_min_size(Pt::ZERO, Pt::new(self.doc.width, self.doc.height));
        // Include off-artboard artwork, retaining a bounded sampling allocation.
        for i in self.layer_tree_indices(layer) {
            if let Some(shapes) = self.doc.layers[i].kind.shapes() {
                for s in shapes {
                    let b = s
                        .world_bbox()
                        .inflate(s.style.stroke.as_ref().map_or(2., |s| s.width * 5. + 2.));
                    let t = compositor::shape_parent_transform(&self.doc, i, s.id);
                    for p in [
                        b.min,
                        Pt::new(b.max.x, b.min.y),
                        b.max,
                        Pt::new(b.min.x, b.max.y),
                    ] {
                        let mut p = tiny_skia::Point::from_xy(p.x, p.y);
                        t.map_point(&mut p);
                        bounds = bounds.union(Bounds::from_min_size(Pt::new(p.x, p.y), Pt::ZERO));
                    }
                }
            } else if let Some(b) = self.doc.layers[i].kind.raster_bounds() {
                bounds = bounds.union(b);
            }
        }
        let scale = (16_777_216. / (bounds.width().max(1.) * bounds.height().max(1.)))
            .sqrt()
            .min(1.);
        let w = (bounds.width() * scale).ceil().max(1.) as u32;
        let h = (bounds.height() * scale).ceil().max(1.) as u32;
        let transform =
            Transform::from_translate(bounds.min.x, bounds.min.y).pre_scale(1. / scale, 1. / scale);
        let values = compositor::item_alpha(&self.doc, layer, id, transform.invert()?, w, h)?;
        Some(ItemMask {
            values,
            space: SelectionSpace { w, h, transform },
        })
    }

    pub fn select_item_outline(&mut self, layer: usize, id: Option<u64>) {
        self.apply_item_outline(layer, id, paint::PixelCombine::Replace);
    }

    pub fn apply_item_outline(&mut self, layer: usize, id: Option<u64>, op: paint::PixelCombine) {
        self.end_pixel_stroke(false);
        let Some(mask) = self.item_mask(layer, id) else {
            self.status = "That object has no pixel edge to select".into();
            return;
        };
        if op == paint::PixelCombine::Replace
            || (self.pixel_sel.is_none() && op != paint::PixelCombine::Subtract)
        {
            self.pixel_sel = Some(mask.values);
            self.pixel_sel_space = Some(mask.space);
            self.pixel_sel_gen = self.pixel_sel_gen.wrapping_add(1);
            self.status = "Item outline selected · choose any layer or object to mask".into();
            return;
        }
        if self.pixel_sel.is_none() {
            self.status = "Nothing to subtract from".into();
            return;
        }
        let target = self.pixel_sel_space.unwrap_or(mask.space);
        let next = if target == mask.space {
            mask.values
        } else {
            match resample(&mask.values, mask.space, target) {
                Some(values) => values,
                None => {
                    self.status = "Could not line that outline up with the selection".into();
                    return;
                }
            }
        };
        let existing = self
            .pixel_sel
            .as_deref()
            .filter(|_| self.pixel_sel_space == Some(target));
        let combined = paint::combine_masks(existing, &next, op);
        let count = paint::selected_count(&combined);
        self.pixel_sel = Some(combined);
        self.pixel_sel_space = Some(target);
        self.pixel_sel_gen = self.pixel_sel_gen.wrapping_add(1);
        self.status = format!("{count} pixels selected");
    }

    pub fn begin_item_mask(&mut self, layer: usize, id: Option<u64>) {
        self.end_pixel_stroke(false);
        self.pending_item_mask = self.item_mask(layer, id);
        self.status = "Mask from item · click the target layer or object · Esc cancels".into();
    }

    pub fn apply_pending_item_mask(&mut self, layer: usize, id: Option<u64>) -> bool {
        let Some(mask) = self.pending_item_mask.clone() else {
            return false;
        };
        if self.apply_item_mask(layer, id, &mask) {
            self.pending_item_mask = None;
        }
        true
    }

    pub fn mask_object_from_selection(&mut self, layer: usize, id: u64) {
        let Some(values) = self.pixel_sel.clone() else {
            return;
        };
        let Some(space) = self.pixel_sel_space else {
            return;
        };
        self.apply_item_mask(layer, Some(id), &ItemMask { values, space });
    }

    fn apply_item_mask(&mut self, layer: usize, id: Option<u64>, mask: &ItemMask) -> bool {
        if !self.layer_unlocked(layer) {
            self.status = "Unlock the target to apply its mask".into();
            return false;
        }
        let target = if let Some(id) = id {
            let Some(shape) = self.doc.find_shape(layer, id).filter(|s| !s.locked) else {
                return false;
            };
            let b = shape.geom.bbox();
            let mut transform = compositor::shape_mask_transform(shape);
            let mut parent = shape.layout.parent;
            for _ in 0..64 {
                let Some(frame) = parent.and_then(|id| self.doc.find_shape(layer, id)) else {
                    break;
                };
                let c = frame.geom.bbox().center();
                transform =
                    tiny_skia::Transform::from_rotate_at(frame.rotation.to_degrees(), c.x, c.y)
                        .pre_concat(transform);
                parent = frame.layout.parent;
            }
            SelectionSpace {
                w: shape
                    .mask
                    .as_ref()
                    .map_or(b.width().ceil().max(1.) as u32, |m| m.w),
                h: shape
                    .mask
                    .as_ref()
                    .map_or(b.height().ceil().max(1.) as u32, |m| m.h),
                transform,
            }
        } else {
            let Some(target) = self.layer_selection_space(layer) else {
                return false;
            };
            target
        };
        if u64::from(target.w) * u64::from(target.h) > 67_108_864 {
            self.status = "Mask target exceeds the 64 megapixel limit".into();
            return false;
        }
        let Some(values) = resample(&mask.values, mask.space, target) else {
            return false;
        };
        let pixels = Pixels::from_rgba(
            target.w,
            target.h,
            values.iter().flat_map(|v| [*v, *v, *v, 255]).collect(),
        );
        if let Some(id) = id {
            let before = self.doc.find_shape(layer, id).unwrap().mask.clone();
            self.commit(Cmd::SetShapeMask {
                layer,
                id,
                before,
                after: pixels,
            });
        } else {
            self.replace_layer_mask(layer, pixels);
        }
        crate::telemetry::count("feature.mask");
        self.status = "Mask applied · original artwork remains editable".into();
        true
    }

    pub fn remove_object_mask(&mut self, layer: usize, id: u64) {
        if !self.layer_unlocked(layer) {
            return;
        }
        if let Some(shape) = self
            .doc
            .find_shape(layer, id)
            .filter(|s| !s.locked && s.mask.is_some())
        {
            self.commit(Cmd::SetShapeMask {
                layer,
                id,
                before: shape.mask.clone(),
                after: None,
            });
        }
    }

    pub fn clear_selected_pixels(&mut self) -> bool {
        let Some(layer) = self.raster_target() else {
            return false;
        };
        let Some(mask) = self.pixel_sel_mask(layer).map(|mask| mask.to_vec()) else {
            return false;
        };
        if !mask.iter().any(|value| *value > 0) {
            return false;
        }
        let Some(pixels) = self
            .doc
            .layers
            .get(layer)
            .and_then(|layer| layer.kind.pixels())
        else {
            return false;
        };
        let before = pixels.data.clone();
        let mut after = before.clone();
        for (index, &selected) in mask.iter().enumerate() {
            if selected != 0 {
                let offset = index * 4;
                after[offset..offset + 4].fill(0);
            }
        }
        if after == before {
            return true;
        }
        self.commit(Cmd::Pixels {
            layer,
            mask: false,
            before,
            after,
        });
        self.status = "Selected pixels cleared".into();
        true
    }

    pub fn mask_target(&self) -> Option<usize> {
        let index = self.active_layer?;
        self.doc
            .layers
            .get(index)
            .filter(|layer| {
                self.paint_mask
                    && self.layer_unlocked(index)
                    && self.doc.layer_visible(index)
                    && layer.mask.is_some()
            })
            .map(|_| index)
    }

    pub fn mask_point(&self, index: usize, world: Pt) -> Pt {
        let Some(layer) = self.doc.layers.get(index) else {
            return world;
        };
        let Some(inverse) = compositor::layer_pixel_transform(layer).invert() else {
            return world;
        };
        let mut point = tiny_skia::Point::from_xy(world.x, world.y);
        inverse.map_point(&mut point);
        Pt::new(point.x, point.y)
    }

    pub fn mask_brush(&self, erase: bool) -> Brush {
        let mut brush = self.brush;
        let value = if erase {
            0
        } else {
            ((54 * u32::from(brush.color.r)
                + 183 * u32::from(brush.color.g)
                + 19 * u32::from(brush.color.b)
                + 128)
                / 256) as u8
        };
        brush.color = Rgba::rgb(value, value, value);
        brush
    }

    pub fn set_mask_edit(&mut self, index: usize, enabled: bool) {
        self.end_pixel_stroke(false);
        self.end_deform(false);
        if !enabled {
            self.paint_mask = false;
            self.status = "Painting layer pixels".into();
        } else if self
            .doc
            .layers
            .get(index)
            .is_some_and(|layer| !layer.locked && layer.visible && layer.mask.is_some())
        {
            self.commit_type_edit();
            self.active_layer = Some(index);
            self.paint_mask = true;
            self.persona = Persona::Pixel;
            self.tool = Tool::Brush;
            self.brush.color = Rgba::rgb(0, 0, 0);
            self.status = "Painting mask · black hides · white reveals".into();
        }
    }

    pub fn add_layer_mask(&mut self, index: usize, reveal: bool) {
        let Some((w, h)) = self.mask_dimensions(index) else {
            return;
        };
        let value = if reveal { 255 } else { 0 };
        let mut pixels = Pixels::new(w, h);
        for pixel in pixels.data.as_chunks_mut::<4>().0 {
            pixel.copy_from_slice(&[value, value, value, 255]);
        }
        pixels.touch();
        self.replace_layer_mask(index, Some(pixels));
        self.set_mask_edit(index, true);
    }

    pub fn mask_from_selection(&mut self, index: usize) {
        let Some(values) = self.pixel_sel.clone() else {
            self.status = "Make a pixel selection first".into();
            return;
        };
        let Some(space) = self
            .pixel_sel_space
            .or_else(|| self.layer_selection_space(index))
        else {
            return;
        };
        if self.apply_item_mask(index, None, &ItemMask { values, space }) {
            self.set_mask_edit(index, true);
        }
    }

    pub fn invert_layer_mask(&mut self, index: usize) {
        let Some(layer) = self
            .doc
            .layers
            .get(index)
            .filter(|_| self.layer_unlocked(index))
        else {
            return;
        };
        let Some(mut mask) = layer.mask.clone() else {
            return;
        };
        for pixel in mask.data.as_chunks_mut::<4>().0 {
            // Include alpha, so an erased/transparent mask pixel inverts to white.
            let amount = mask_amount(pixel);
            let value = 255 - amount;
            pixel.copy_from_slice(&[value, value, value, 255]);
        }
        mask.touch();
        self.replace_layer_mask(index, Some(mask));
        self.status = "Mask inverted".into();
    }

    pub fn remove_layer_mask(&mut self, index: usize) {
        if self
            .doc
            .layers
            .get(index)
            .is_some_and(|layer| !layer.locked && layer.mask.is_some())
        {
            self.replace_layer_mask(index, None);
            self.paint_mask = false;
            self.status = "Mask removed · pixels preserved".into();
        }
    }

    pub fn apply_layer_mask(&mut self, index: usize) {
        let Some(layer) = self
            .doc
            .layers
            .get(index)
            .filter(|_| self.layer_unlocked(index))
        else {
            return;
        };
        let Some(mask) = layer.mask.as_ref() else {
            return;
        };
        let Some(pixels) = layer.kind.pixels() else {
            self.status = "Apply is available for pixel layers; vector masks stay editable".into();
            return;
        };
        if mask.w != pixels.w || mask.h != pixels.h || mask.data.len() != pixels.data.len() {
            self.status = "Mask size does not match its pixel layer".into();
            return;
        }
        let before = pixels.data.clone();
        let mut after = before.clone();
        for (pixel, mask_pixel) in after
            .as_chunks_mut::<4>()
            .0
            .iter_mut()
            .zip(mask.data.as_chunks::<4>().0.iter())
        {
            pixel[3] =
                ((u32::from(pixel[3]) * u32::from(mask_amount(mask_pixel)) + 127) / 255) as u8;
        }
        let mask = mask.clone();
        self.commit(Cmd::Batch(vec![
            Cmd::Pixels {
                layer: index,
                mask: false,
                before,
                after,
            },
            Cmd::SetLayerMask {
                index,
                before: Some(mask),
                after: None,
            },
        ]));
        self.paint_mask = false;
        self.status = "Mask applied · undo restores the original pixels and mask".into();
    }

    fn mask_dimensions(&self, index: usize) -> Option<(u32, u32)> {
        let layer = self
            .doc
            .layers
            .get(index)
            .filter(|_| self.layer_unlocked(index))?;
        if layer.mask_size.x > 0.0 && layer.mask_size.y > 0.0 {
            return Some(layer.mask.as_ref().map(|mask| (mask.w, mask.h)).unwrap_or((
                layer.mask_size.x.ceil().max(1.0) as u32,
                layer.mask_size.y.ceil().max(1.0) as u32,
            )));
        }
        Some(layer.kind.pixels().map_or(
            (
                self.doc.width.ceil().max(1.0) as u32,
                self.doc.height.ceil().max(1.0) as u32,
            ),
            |pixels| (pixels.w, pixels.h),
        ))
    }

    fn replace_layer_mask(&mut self, index: usize, after: Option<Pixels>) {
        let Some(layer) = self
            .doc
            .layers
            .get(index)
            .filter(|_| self.layer_unlocked(index))
        else {
            return;
        };
        self.commit(Cmd::SetLayerMask {
            index,
            before: layer.mask.clone(),
            after,
        });
    }
}

fn mask_amount(pixel: &[u8]) -> u8 {
    let premultiplied =
        tiny_skia::ColorU8::from_rgba(pixel[0], pixel[1], pixel[2], pixel[3]).premultiply();
    (f32::from(premultiplied.red()) * 0.2126
        + f32::from(premultiplied.green()) * 0.7152
        + f32::from(premultiplied.blue()) * 0.0722)
        .ceil()
        .clamp(0.0, 255.0) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pixels_studio() -> Studio {
        let mut studio = Studio::new();
        studio.doc = Document::new("Mask", 4.0, 1.0, 96.0);
        studio.doc.transparent = true;
        let mut layer = Layer::raster("Pixels", 4, 1);
        layer.kind.pixels_mut().unwrap().data = [220, 50, 20, 255].repeat(4);
        layer.kind.pixels_mut().unwrap().touch();
        studio.doc.layers = vec![layer];
        studio.active_layer = Some(0);
        studio.history.clear();
        studio
    }

    fn alphas(doc: &Document) -> Vec<u8> {
        let png = compositor::export_png(doc, 1).unwrap();
        image::load_from_memory(&png)
            .unwrap()
            .to_rgba8()
            .pixels()
            .map(|p| p[3])
            .collect()
    }

    #[test]
    fn selection_mask_survives_serialization_and_apply_is_one_undo() {
        let mut studio = pixels_studio();
        studio.pixel_sel = Some(vec![0, 64, 128, 255]);
        studio.mask_from_selection(0);
        assert!(studio.paint_mask);
        let masked = alphas(&studio.doc);
        for (&actual, expected) in masked.iter().zip([0u8, 64, 128, 255]) {
            assert!(actual.abs_diff(expected) <= 1);
        }
        let json = serde_json::to_vec(&studio.doc).unwrap();
        let reopened: Document = serde_json::from_slice(&json).unwrap();
        assert_eq!(alphas(&reopened), masked);

        studio.invert_layer_mask(0);
        assert_eq!(alphas(&studio.doc)[0], 255);
        assert_eq!(alphas(&studio.doc)[3], 0);
        studio.undo();
        assert_eq!(alphas(&studio.doc), masked);

        let original = studio.doc.layers[0].kind.pixels().unwrap().data.clone();
        studio.apply_layer_mask(0);
        assert!(studio.doc.layers[0].mask.is_none());
        let applied = alphas(&studio.doc);
        for (&a, b) in applied.iter().zip(&masked) {
            assert!(a.abs_diff(*b) <= 1);
        }
        studio.undo();
        assert_eq!(studio.doc.layers[0].kind.pixels().unwrap().data, original);
        assert!(studio.doc.layers[0].mask.is_some());
        assert_eq!(alphas(&studio.doc), masked);
        studio.redo();
        assert!(studio.doc.layers[0].mask.is_none());
        assert_eq!(alphas(&studio.doc), applied);
    }

    #[test]
    fn masks_follow_a_scaled_rotated_placed_image() {
        let mut studio = pixels_studio();
        studio.doc.width = 24.0;
        studio.doc.height = 20.0;
        let pixels = Pixels::from_rgba(4, 2, [220, 50, 20, 255].repeat(8)).unwrap();
        studio.doc.layers[0] =
            Layer::placed_raster("Placed", pixels, Pt::new(6.0, 4.0), Pt::new(8.0, 4.0));
        if let LayerKind::Raster { rotation, .. } = &mut studio.doc.layers[0].kind {
            *rotation = std::f32::consts::FRAC_PI_2;
        }
        studio.pixel_sel = Some(vec![255, 255, 0, 0, 255, 255, 0, 0]);
        studio.mask_from_selection(0);
        let transform = compositor::layer_pixel_transform(&studio.doc.layers[0]);
        let png = compositor::export_png(&studio.doc, 1).unwrap();
        let result = image::load_from_memory(&png).unwrap().to_rgba8();
        for (local, visible) in [(Pt::new(0.75, 0.75), true), (Pt::new(3.0, 0.75), false)] {
            let mut point = tiny_skia::Point::from_xy(local.x, local.y);
            transform.map_point(&mut point);
            assert!((studio.mask_point(0, Pt::new(point.x, point.y)) - local).length() < 0.001);
            let alpha = result.get_pixel(point.x.floor() as u32, point.y.floor() as u32)[3];
            if visible {
                assert!(alpha > 220);
            } else {
                assert!(alpha < 10);
            }
        }
    }

    #[test]
    fn hide_reveal_remove_and_locked_mask_targets_are_safe() {
        let mut studio = pixels_studio();
        studio.add_layer_mask(0, false);
        assert_eq!(alphas(&studio.doc), vec![0; 4]);
        studio.add_layer_mask(0, true);
        assert_eq!(alphas(&studio.doc), vec![255; 4]);
        studio.remove_layer_mask(0);
        assert!(!studio.paint_mask);
        assert_eq!(alphas(&studio.doc), vec![255; 4]);
        studio.undo();
        studio.set_mask_edit(0, true);
        assert_eq!(studio.mask_target(), Some(0));
        studio.doc.layers[0].locked = true;
        assert_eq!(studio.mask_target(), None);
        let history = studio.history.len();
        studio.add_layer_mask(0, false);
        studio.remove_layer_mask(0);
        assert_eq!(studio.history.len(), history);
        studio.active_layer = Some(30);
        assert_eq!(studio.mask_target(), None);
    }
}
