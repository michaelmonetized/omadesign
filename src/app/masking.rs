use super::*;
use crate::document::Pixels;

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

    pub fn mask_target(&self) -> Option<usize> {
        let index = self.active_layer?;
        self.doc
            .layers
            .get(index)
            .filter(|layer| {
                self.paint_mask && !layer.locked && layer.visible && layer.mask.is_some()
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
        let Some((w, h)) = self.mask_dimensions(index) else {
            return;
        };
        let Some(selection) = self
            .pixel_sel
            .as_ref()
            .filter(|mask| mask.len() == w as usize * h as usize)
        else {
            self.status = "Make a pixel selection on this layer first".into();
            return;
        };
        let data = selection
            .iter()
            .flat_map(|value| [*value, *value, *value, 255])
            .collect();
        self.replace_layer_mask(index, Pixels::from_rgba(w, h, data));
        self.set_mask_edit(index, true);
    }

    pub fn invert_layer_mask(&mut self, index: usize) {
        let Some(layer) = self.doc.layers.get(index).filter(|layer| !layer.locked) else {
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
        let Some(layer) = self.doc.layers.get(index).filter(|layer| !layer.locked) else {
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
        let layer = self.doc.layers.get(index).filter(|layer| !layer.locked)?;
        Some(layer.kind.pixels().map_or(
            (
                self.doc.width.ceil().max(1.0) as u32,
                self.doc.height.ceil().max(1.0) as u32,
            ),
            |pixels| (pixels.w, pixels.h),
        ))
    }

    fn replace_layer_mask(&mut self, index: usize, after: Option<Pixels>) {
        let Some(layer) = self.doc.layers.get(index).filter(|layer| !layer.locked) else {
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
