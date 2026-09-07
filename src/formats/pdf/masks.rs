use super::*;
use crate::geom::Bounds;
use std::rc::Rc;
use std::sync::Arc;

pub(super) struct MaskData {
    pub pixels: Pixels,
    pub origin: Pt,
    pub size: Pt,
}

pub(super) fn transformed_rectangle(rect: [f32; 4], matrix: Matrix) -> Geom {
    Geom::Poly {
        contours: vec![
            [
                Pt::new(rect[0], rect[1]),
                Pt::new(rect[2], rect[1]),
                Pt::new(rect[2], rect[3]),
                Pt::new(rect[0], rect[3]),
            ]
            .map(|p| matrix.map(p))
            .to_vec(),
        ],
        winding: true,
    }
}

pub(super) fn intersect_clip(state: &mut State, clip: Geom) {
    state.clip = Some(Arc::new(if let Some(previous) = &state.clip {
        crate::boolean::apply(crate::boolean::BoolOp::Intersect, previous, &clip).unwrap_or(
            Geom::Poly {
                contours: vec![],
                winding: false,
            },
        )
    } else {
        clip
    }));
}

fn dimensions(bounds: Bounds) -> Result<(Pt, u32, u32), String> {
    let min = Pt::new(bounds.min.x.floor(), bounds.min.y.floor());
    let max = Pt::new(bounds.max.x.ceil(), bounds.max.y.ceil());
    if !min.x.is_finite() || !min.y.is_finite() || !max.x.is_finite() || !max.y.is_finite() {
        return Err("PDF mask has invalid bounds".into());
    }
    let w = (max.x - min.x).max(1.) as u32;
    let h = (max.y - min.y).max(1.) as u32;
    if u64::from(w) * u64::from(h) > PIXEL_LIMIT as u64 {
        return Err("PDF mask exceeds the 64-megapixel limit".into());
    }
    Ok((min, w, h))
}

impl Reader<'_> {
    pub(super) fn soft_mask(
        &mut self,
        object: &Object,
        state: &State,
    ) -> Result<Rc<MaskData>, String> {
        if self.mask_depth >= 4 {
            return Err("PDF soft-mask nesting exceeds 4".into());
        }
        let mask = dictionary(self.pdf, object).ok_or("Invalid PDF soft mask dictionary")?;
        if mask
            .get(b"TR")
            .is_ok_and(|value| value.as_name().ok() != Some(b"Identity"))
        {
            self.warn("PDF soft-mask transfer functions are not applied.");
        }
        let alpha = mask.get(b"S").and_then(Object::as_name).ok() == Some(b"Alpha");
        let form = resolve(
            self.pdf,
            mask.get(b"G").map_err(|_| "PDF soft mask has no group")?,
        )
        .and_then(|object| object.as_stream().ok())
        .ok_or("PDF mask group is not a Form")?;
        let matrix = state.matrix.concat(
            form.dict
                .get(b"Matrix")
                .ok()
                .and_then(|object| matrix_value(self.pdf, object))
                .unwrap_or(Matrix::IDENTITY),
        );
        let rect = form
            .dict
            .get(b"BBox")
            .ok()
            .and_then(|object| rectangle(self.pdf, object))
            .ok_or("PDF mask group has no bounding box")?;
        let clip = transformed_rectangle(rect, matrix);
        let (origin, w, h) = dimensions(clip.bbox())?;
        let mut document = Document::new("PDF mask", 1., 1., 72.);
        document.width = w as f32;
        document.height = h as f32;
        document.transparent = true;
        document.layers.clear();
        document.artboards = vec![Artboard::new(0, Pt::ZERO, Pt::new(w as f32, h as f32))];
        let group = Layer::group("Mask");
        // Luminosity masks composite their group against /BC (default black).
        if !alpha {
            let background = mask
                .get(b"BC")
                .ok()
                .and_then(|object| resolve(self.pdf, object))
                .and_then(|object| object.as_array().ok());
            let values = background
                .map(|values| values.iter().filter_map(number_value).collect::<Vec<_>>())
                .unwrap_or_default();
            let color = match values.as_slice() {
                [r, g, b, ..] => rgb(*r, *g, *b),
                [g] => gray(*g),
                _ => Rgba::BLACK,
            };
            let mut layer = Layer::vector("Mask backdrop");
            layer.parent = Some(group.id);
            layer.kind.shapes_mut().unwrap().push(Shape::new(
                Geom::Rect {
                    origin: Pt::ZERO,
                    size: Pt::new(w as f32, h as f32),
                    radius: 0.,
                },
                Style {
                    fill: Fill::Solid(color),
                    stroke: None,
                },
            ));
            document.layers.push(layer);
        }
        let mut reader = Reader {
            pdf: self.pdf,
            document,
            warnings: BTreeSet::new(),
            off: self.off.clone(),
            base_off: self.base_off,
            on: self.on.clone(),
            font_cache: HashMap::new(),
            cff_cache: HashMap::new(),
            objects: 0,
            page_parent: group.id,
            page_name: "Mask".into(),
            mask_depth: self.mask_depth + 1,
            last_mask_key: (0, 0),
            last_mask_state: None,
            pixel_bytes: 0,
        };
        let local = Matrix::IDENTITY
            .translate(-origin.x, -origin.y)
            .concat(matrix);
        let mut initial = State::new(local);
        initial.clip = Some(Arc::new(transformed_rectangle(rect, local)));
        let resources = form
            .dict
            .get(b"Resources")
            .ok()
            .and_then(|object| dictionary(self.pdf, object))
            .cloned()
            .unwrap_or_default();
        reader.operations(&decoded(form, STREAM_LIMIT)?, &resources, initial, &[], 0)?;
        reader.document.layers.push(group);
        let pm = crate::compositor::render_export(&reader.document, 1)?;
        for warning in reader.warnings {
            self.warn(&warning);
        }
        let mask = if alpha {
            tiny_skia::Mask::from_pixmap(pm.as_ref(), tiny_skia::MaskType::Alpha)
        } else {
            tiny_skia::Mask::from_pixmap(pm.as_ref(), tiny_skia::MaskType::Luminance)
        };
        let mut rgba = Vec::with_capacity(w as usize * h as usize * 4);
        for value in mask.data() {
            rgba.extend_from_slice(&[*value, *value, *value, 255]);
        }
        let pixels = Pixels::from_rgba(w, h, rgba).ok_or("Invalid mask raster")?;
        self.reserve_pixels(w, h)?;
        Ok(Rc::new(MaskData {
            pixels,
            origin,
            size: Pt::new(w as f32, h as f32),
        }))
    }

    pub(super) fn paint_mask(&mut self, state: &State) -> Result<Option<MaskData>, String> {
        if state.clip.is_none() && state.soft_mask.is_none() {
            return Ok(None);
        }
        let mut bounds = if let Some(clip) = &state.clip {
            clip.bbox()
        } else {
            let mask = state.soft_mask.as_ref().unwrap();
            Bounds::from_min_size(mask.origin, mask.size)
        };
        if let Some(soft) = &state.soft_mask {
            let other = Bounds::from_min_size(soft.origin, soft.size);
            bounds.min = bounds.min.max(other.min);
            bounds.max = bounds.max.min(other.max);
        }
        let (origin, w, h) = dimensions(bounds)?;
        self.reserve_pixels(w, h)?;
        let mut mask = tiny_skia::Mask::new(w, h).ok_or("Could not allocate PDF clip mask")?;
        if let Some(clip) = &state.clip {
            let shape = Shape::new((**clip).clone(), Style::default());
            if let Some(path) = shape.get_cached_path(96) {
                let rule = if matches!(**clip, Geom::Poly { winding: false, .. }) {
                    tiny_skia::FillRule::EvenOdd
                } else {
                    tiny_skia::FillRule::Winding
                };
                mask.fill_path(
                    &path,
                    rule,
                    true,
                    tiny_skia::Transform::from_translate(-origin.x, -origin.y),
                );
            }
        } else {
            mask.data_mut().fill(255);
        }
        let mut rgba = Vec::with_capacity(w as usize * h as usize * 4);
        for y in 0..h as usize {
            for x in 0..w as usize {
                let mut value = mask.data()[y * w as usize + x];
                if let Some(soft) = &state.soft_mask {
                    value = ((u16::from(value)
                        * u16::from(sample(
                            soft,
                            origin + Pt::new(x as f32 + 0.5, y as f32 + 0.5),
                        ))
                        + 127)
                        / 255) as u8;
                }
                rgba.extend_from_slice(&[value, value, value, 255]);
            }
        }
        Ok(Some(MaskData {
            pixels: Pixels::from_rgba(w, h, rgba).ok_or("Invalid PDF clip mask")?,
            origin,
            size: Pt::new(w as f32, h as f32),
        }))
    }

    pub(super) fn mask_image(&mut self, layer: &mut Layer, state: &State) -> Result<(), String> {
        let Some(mask) = self.paint_mask(state)? else {
            return Ok(());
        };
        let transform = crate::compositor::layer_pixel_transform(layer);
        let Some(pixels) = layer.kind.pixels_mut() else {
            return Ok(());
        };
        for y in 0..pixels.h as usize {
            for x in 0..pixels.w as usize {
                let mut point = tiny_skia::Point::from_xy(x as f32 + 0.5, y as f32 + 0.5);
                transform.map_point(&mut point);
                let mask = sample(&mask, Pt::new(point.x, point.y));
                let alpha = &mut pixels.data[(y * pixels.w as usize + x) * 4 + 3];
                *alpha = ((u16::from(*alpha) * u16::from(mask) + 127) / 255) as u8;
            }
        }
        pixels.touch();
        Ok(())
    }
}

pub(super) fn rectangle_contains(clip: &Geom, bounds: Bounds) -> bool {
    let rect = clip.bbox();
    if bounds.min.x < rect.min.x - 0.001
        || bounds.min.y < rect.min.y - 0.001
        || bounds.max.x > rect.max.x + 0.001
        || bounds.max.y > rect.max.y + 0.001
    {
        return false;
    }
    match clip {
        Geom::Rect { radius, .. } => *radius <= 0.,
        Geom::Poly { contours, .. }
            if contours.len() == 1 && (4..=5).contains(&contours[0].len()) =>
        {
            contours[0].iter().all(|point| {
                ((point.x - rect.min.x).abs() < 0.001 || (point.x - rect.max.x).abs() < 0.001)
                    && ((point.y - rect.min.y).abs() < 0.001
                        || (point.y - rect.max.y).abs() < 0.001)
            })
        }
        _ => false,
    }
}

fn sample(mask: &MaskData, point: Pt) -> u8 {
    let x = ((point.x - mask.origin.x) / mask.size.x * mask.pixels.w as f32).floor() as i64;
    let y = ((point.y - mask.origin.y) / mask.size.y * mask.pixels.h as f32).floor() as i64;
    if x < 0 || y < 0 || x >= i64::from(mask.pixels.w) || y >= i64::from(mask.pixels.h) {
        return 0;
    }
    mask.pixels.data[(y as usize * mask.pixels.w as usize + x as usize) * 4]
}
