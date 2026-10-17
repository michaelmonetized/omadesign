use super::*;
use crate::artwork_mask::{ArtworkMask, MaskMode, MaskSource};
use crate::document::Pixels;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SelectionSpace {
    pub w: u32,
    pub h: u32,
    pub transform: tiny_skia::Transform,
}

/// The same-space case borrows the source. A transformed mask shares one
/// cached allocation across all dabs instead of resampling and copying it.
pub(crate) enum SelectionMask<'a> {
    Borrowed(&'a [f32]),
    Mapped(std::sync::Arc<crate::resources::Allocation<f32>>),
}

impl std::ops::Deref for SelectionMask<'_> {
    type Target = [f32];
    fn deref(&self) -> &[f32] {
        match self {
            Self::Borrowed(values) => values,
            Self::Mapped(values) => values,
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
struct SelectionKey {
    generation: u64,
    source: SelectionSpace,
    target: SelectionSpace,
    address: usize,
    length: usize,
}

#[derive(Default)]
pub(super) struct SelectionCache {
    entry: std::cell::RefCell<Option<(SelectionKey, std::sync::Arc<crate::resources::Allocation<f32>>)>>,
}

impl SelectionCache {
    pub(super) fn clear(&self) {
        self.entry.borrow_mut().take();
    }
}

#[derive(Clone)]
pub struct ItemMask {
    pub values: std::sync::Arc<crate::resources::Allocation<f32>>,
    pub space: SelectionSpace,
}

pub(super) fn resample(values:&[f32],source:SelectionSpace,target:SelectionSpace)->Option<Vec<f32>> {resample_admitted(values,source,target).ok().map(|v|v.to_vec())}
pub(super) fn resample_admitted(values:&[f32],source:SelectionSpace,target:SelectionSpace)->Result<std::sync::Arc<crate::resources::Allocation<f32>>,String>{crate::selection_coverage::resample(values,source,target,&crate::deep_color::storage::check)}

impl Studio {
    /// Finish or restore an in-flight pixel edit before changing document context.
    /// Returning true lets Undo cancel the current gesture before touching history.
    pub(crate) fn end_pixel_stroke(&mut self, cancel: bool) -> bool {
        if matches!(self.op, Some(Op::ChannelStroke(_))) {
            return self.end_channel_stroke(cancel);
        }
        if matches!(
            self.op,
            Some(Op::NativeStroke { .. } | Op::NativeRetouch { .. })
        ) {
            let (layer, mask, before) = match self.op.take().unwrap() {
                Op::NativeStroke {
                    layer,
                    mask,
                    before,
                    ..
                } => (layer, mask, before),
                Op::NativeRetouch { layer, before, .. } => (layer, false, before),
                _ => unreachable!(),
            };
            let target = if mask {
                self.doc.layers[layer].mask.as_mut()
            } else {
                self.doc.layers[layer].kind.pixels_mut()
            };
            if let Some(target) = target {
                if cancel {
                    *target = before;
                } else {
                    let after = target.clone();
                    self.history.push(Cmd::NativePixels {
                        layer,
                        mask,
                        before,
                        after,
                    });
                    self.dirty = true;
                }
            }
            self.paint_contact = None;
            self.brush.pressure = 1.;
            self.mark();
            return true;
        }
        if matches!(self.op, Some(Op::Brush { .. })) {
            let Some(Op::Brush {
                layer,
                before,
                mut buf,
                ..
            }) = self.op.take()
            else {
                unreachable!()
            };
            self.paint_contact = None;
            self.brush.pressure = 1.0;
            if !cancel && let Some(selection) = self.cached_pixel_sel_mask(layer) {
                paint::feather_overlay(&mut buf, &selection);
            }
            if let Some(target) = self.doc.layers.get_mut(layer) {
                if cancel
                    || self.brush.opacity <= 0.0
                    || buf.pixels().iter().all(|p| p.alpha() == 0)
                {
                    before.restore(target);
                } else if let Some(pixels) = target.kind.pixels_mut() {
                    if pixels.native.is_some() {
                        if let Err(error) = Pixels::from_pixmap(&buf)
                            .and_then(|overlay| pixels.paint_overlay(&overlay, self.brush.opacity))
                        {
                            self.status = error;
                            before.restore(target);
                            return true;
                        }
                    } else if let Some(mut destination) = pixels.to_pixmap() {
                        paint::blend_onto(&mut destination, &buf, self.brush.opacity);
                        match Pixels::from_pixmap(&destination) {
                            Ok(after) => *pixels = after,
                            Err(error) => {
                                self.status = error;
                                before.restore(target);
                                return true;
                            }
                        }
                    }
                    if pixels.native != before.kind.pixels().unwrap().native
                        || pixels.data != before.kind.pixels().unwrap().data
                        || target.kind.raster_xform() != before.kind.raster_xform()
                    {
                        let after = crate::document::RasterState::capture(target).unwrap();
                        self.history.push(Cmd::PaintRaster {
                            layer,
                            before,
                            after,
                        });
                        self.dirty = true;
                    }
                }
            }
            self.reset_snap_gesture();
            self.mark();
            return true;
        }
        let (layer, mask, before) = match self.op.take() {
            Some(Op::Retouch {
                layer,
                mask,
                before,
                ..
            }) => (layer, mask, before),
            Some(Op::Smudge { layer, before, .. } | Op::Clone { layer, before, .. }) => {
                (layer, false, before)
            }
            other => {
                self.op = other;
                return false;
            }
        };
        self.paint_contact = None;
        self.brush.pressure = 1.0;
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
                if pixels.data.as_slice() != before.as_slice() {
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
        let item = self
            .doc
            .layers
            .get(layer)
            .filter(|_| self.layer_unlocked(layer))?;
        // Raster coordinates come from the raster itself, even when a loaded
        // layer retains a differently sized mask or vector-mask metadata.
        // layer_pixel_transform uses this same native raster grid.
        let (w, h) = item
            .kind
            .pixels()
            .map(|pixels| (pixels.w, pixels.h))
            .or_else(|| self.mask_dimensions(layer))?;
        Some(SelectionSpace {
            w,
            h,
            transform: compositor::layer_pixel_transform(item),
        })
    }

    pub fn pixel_sel_mask(&self, layer: usize) -> Option<std::borrow::Cow<'_, [f32]>> {
        self.cached_pixel_sel_mask(layer).map(|mask| match mask {
            SelectionMask::Borrowed(values) => std::borrow::Cow::Borrowed(values),
            SelectionMask::Mapped(values) => std::borrow::Cow::Owned(values.to_vec()),
        })
    }

    pub(crate) fn cached_pixel_sel_mask(&self, layer: usize) -> Option<SelectionMask<'_>> {
        let target = self.layer_selection_space(layer)?;
        let mask = self.pixel_sel.as_ref()?;
        let source = self.pixel_sel_space.unwrap_or(target);
        if source == target && mask.len() == target.w as usize * target.h as usize {
            return Some(SelectionMask::Borrowed(mask));
        }
        let key = SelectionKey {
            generation: self.pixel_sel_gen,
            source,
            target,
            address: mask.as_ptr() as usize,
            length: mask.len(),
        };
        if let Some((cached_key, values)) = self.pixel_sel_cache.entry.borrow().as_ref()
            && *cached_key == key
        {
            return Some(SelectionMask::Mapped(values.clone()));
        }
        self.pixel_sel_cache.clear();
        let values = resample_admitted(mask, source, target).ok()?;
        // Keep at most one derived mask, never one per open tab or layer. The
        // image size limit is 64M pixels; oversized inputs remain uncached.
        if values.len() <= 64 * 1024 * 1024 {
            *self.pixel_sel_cache.entry.borrow_mut() = Some((key, values.clone()));
        }
        Some(SelectionMask::Mapped(values))
    }

    /// Share a mapped selection with a worker instead of copying its full grid.
    /// Args: target layer. Returns the existing admitted source or transformed owner.
    pub(crate) fn owned_pixel_sel_mask(&self,layer:usize)->Option<std::sync::Arc<crate::resources::Allocation<f32>>>{
        match self.cached_pixel_sel_mask(layer)?{SelectionMask::Borrowed(_)=>self.pixel_sel.clone(),SelectionMask::Mapped(values)=>Some(values)}
    }

    pub fn set_pixel_sel(&mut self, mask: Option<Vec<u8>>) {
        let mask = match mask.map(|values| crate::selection_coverage::from_bytes(&values,&crate::deep_color::storage::check)).transpose() {
            Ok(mask) => mask,
            Err(error) => { self.status=error; return; }
        };
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
        self.pixel_sel_cache.clear();
        self.status = self.pixel_sel.as_ref().map_or_else(
            || "Pixel selection cleared".into(),
            |mask| format!("{} pixels selected", paint::selected_count(mask)),
        );
    }

    pub(crate) fn selection_target_space(&self) -> Option<SelectionSpace> {
        self.raster_target()
            .and_then(|i| self.layer_selection_space(i))
            .or_else(|| {
                let w = self.doc.width.ceil().max(1.) as u32;
                let h = self.doc.height.ceil().max(1.) as u32;
                (u64::from(w) * u64::from(h) <= 64_000_000).then_some(SelectionSpace {
                    w,
                    h,
                    transform: tiny_skia::Transform::identity(),
                })
            })
    }

    pub fn nudge_pixel_sel(&mut self, dx: i32, dy: i32) {
        if self.retouch_pixel_sel(|mask, w, h| crate::selection_coverage::warp(mask,w,h,|x,y|(x-dx as f32,y-dy as f32))) {
            self.status = "Selection moved".into();
        }
    }

    pub fn scale_pixel_sel(&mut self, sx: f32, sy: f32) {
        if self.retouch_pixel_sel(|mask, w, h| crate::selection_coverage::warp(mask,w,h,|x,y|{let cx=w as f32/2.;let cy=h as f32/2.;(cx+(x-cx)/sx,cy+(y-cy)/sy)})) {
            self.status = "Selection resized".into();
        }
    }

    pub fn feather_pixel_sel(&mut self, radius: u32) {
        if self.retouch_pixel_sel(|mask, w, h| super::pixel_selection::edit_admitted(mask,SelectionSpace{w,h,transform:tiny_skia::Transform::identity()},super::pixel_selection::Edit::Feather{radius}).and_then(|result|result.map(|(v,_)|v).ok_or("Selection is empty".to_string()))) {
            self.status = format!("Selection feathered {radius} px");
        }
    }

    pub fn distort_pixel_sel(&mut self, kx: f32) {
        if self.retouch_pixel_sel(|mask, w, h| crate::selection_coverage::warp(mask,w,h,|x,y|(x-kx*(y-h as f32/2.),y))) {
            self.status = "Selection distorted".into();
        }
    }

    fn retouch_pixel_sel(&mut self, edit: impl FnOnce(&[f32], u32, u32) -> Result<std::sync::Arc<crate::resources::Allocation<f32>>,String>) -> bool {
        let Some(space) = self.pixel_sel_space else {
            return false;
        };
        let Some(mask) = self.pixel_sel.clone() else {
            return false;
        };
        if mask.len() != space.w as usize * space.h as usize {
            return false;
        }
        match edit(&mask,space.w,space.h){Ok(values)=>self.replace_pixel_selection_admitted(values,space),Err(error)=>{self.status=error;false}}
    }

    pub fn merge_pixel_sel(&mut self, next: Vec<u8>, op: paint::PixelCombine) {
        let existing = self.raster_target().and_then(|li| self.pixel_sel_mask(li));
        let Some(space)=self.selection_target_space() else{return;};
        match crate::selection_coverage::combine_bytes(existing.as_deref(),&next,op){Ok(values)=>{self.replace_pixel_selection_admitted(values,space);},Err(error)=>self.status=error}
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
            values: crate::selection_coverage::from_bytes(&values,&crate::deep_color::storage::check).ok()?,
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
            self.pixel_sel_cache.clear();
            self.status = "Item outline selected · choose any layer or object to mask".into();
            return;
        }
        if self.pixel_sel.is_none() {
            self.status = "Nothing to subtract from".into();
            return;
        }
        let target = self.pixel_sel_space.unwrap_or(mask.space);
        let next = if target == mask.space {
            mask.values.to_vec()
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
            .pixel_sel.as_ref().map(|values| values.as_slice())
            .filter(|_| self.pixel_sel_space == Some(target));
        let combined = match crate::selection_coverage::combine(existing, &next, op){Ok(v)=>v,Err(error)=>{self.status=error;return;}};
        let count = paint::selected_count(&combined);
        if !self.replace_pixel_selection_admitted(combined, target) {return;}
        self.status = format!("{count} pixels selected");
    }

    pub fn begin_item_mask(&mut self, layer: usize, id: Option<u64>) {
        self.end_pixel_stroke(false);
        self.pending_item_mask = self
            .doc
            .layers
            .get(layer)
            .filter(|l| {
                id.is_none()
                    || l.kind
                        .shapes()
                        .is_some_and(|ss| ss.iter().any(|s| Some(s.id) == id))
            })
            .map(|l| MaskSource {
                layer: l.id,
                shape: id,
            });
        self.status = "Use as mask · click the target layer or object · Esc cancels".into();
    }

    pub fn apply_pending_item_mask(&mut self, layer: usize, id: Option<u64>) -> bool {
        let Some(mask) = self.pending_item_mask else {
            return false;
        };
        if self.link_artwork_mask(layer, id, mask) {
            self.pending_item_mask = None;
        }
        true
    }

    fn link_artwork_mask(&mut self, layer: usize, id: Option<u64>, source: MaskSource) -> bool {
        if !self.layer_unlocked(layer)
            || id.is_some_and(|id| self.doc.find_shape(layer, id).is_none_or(|s| s.locked))
        {
            self.status = "Unlock the target to apply its mask".into();
            return false;
        }
        if !self
            .doc
            .layers
            .iter()
            .any(|l| l.id == source.layer && source.shape.is_none_or(|id| l.find(id).is_some()))
        {
            self.status = "The mask source was removed".into();
            return false;
        }
        let mut candidate = self.doc.clone();
        let masks = if let Some(id) = id {
            &mut candidate.find_shape_mut(layer, id).unwrap().artwork_masks
        } else {
            &mut candidate.layers[layer].artwork_masks
        };
        let before = masks.clone();
        let after = vec![ArtworkMask::new(source)];
        *masks = after.clone();
        if let Err(error) = candidate.validate_artwork_masks() {
            self.status = error;
            return false;
        }
        self.commit(Cmd::SetArtworkMasks {
            layer,
            id,
            before,
            after,
        });
        self.status = "Live mask applied · edit its source in Layers".into();
        true
    }

    pub fn remove_artwork_masks(&mut self, layer: usize, id: Option<u64>) {
        if !self.layer_unlocked(layer) {
            return;
        }
        let masks = if let Some(id) = id {
            self.doc
                .find_shape(layer, id)
                .filter(|s| !s.locked)
                .map(|s| &s.artwork_masks)
        } else {
            self.doc.layers.get(layer).map(|l| &l.artwork_masks)
        };
        if let Some(before) = masks.filter(|m| !m.is_empty()).cloned() {
            self.commit(Cmd::SetArtworkMasks {
                layer,
                id,
                before,
                after: Vec::new(),
            });
            self.status = "Live mask released · source artwork restored".into();
        }
    }

    pub fn set_artwork_mask_mode(&mut self, layer: usize, id: Option<u64>, mode: MaskMode) {
        if !self.layer_unlocked(layer) {
            return;
        }
        let masks = if let Some(id) = id {
            self.doc
                .find_shape(layer, id)
                .filter(|s| !s.locked)
                .map(|s| &s.artwork_masks)
        } else {
            self.doc.layers.get(layer).map(|l| &l.artwork_masks)
        };
        if let Some(before) = masks.filter(|m| !m.is_empty()).cloned() {
            let mut after = before.clone();
            for mask in &mut after {
                mask.mode = mode;
            }
            self.commit(Cmd::SetArtworkMasks {
                layer,
                id,
                before,
                after,
            });
        }
    }

    pub fn select_mask_source(&mut self, source: MaskSource) {
        let Some((index, _)) = source.resolve(&self.doc) else {
            return;
        };
        if let Some(id) = source.shape {
            self.selection = vec![(index, id)];
            self.active_layer = Some(index);
            self.selected_layer = None;
        } else {
            self.activate_layer_tree(index);
        }
        self.paint_mask = false;
        self.node_sel.clear();
        self.layer_expanded.insert(self.doc.layers[index].id);
        for ancestor in self.doc.layer_ancestors(index) {
            self.layer_expanded.insert(self.doc.layers[ancestor].id);
        }
        self.tool = Tool::Select;
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
            // Replacing a layer mask preserves that mask's grid; this is a
            // different target from editing the raster's native pixel grid.
            let Some((w, h)) = self.mask_dimensions(layer) else {
                return false;
            };
            SelectionSpace {
                w,
                h,
                transform: compositor::layer_pixel_transform(&self.doc.layers[layer]),
            }
        };
        if u64::from(target.w) * u64::from(target.h) > 67_108_864 {
            self.status = "Mask target exceeds the 64 megapixel limit".into();
            return false;
        }
        let Some(values) = resample(&mask.values, mask.space, target) else {
            return false;
        };
        let pixels = match crate::selection_coverage::mask_pixels(&values,target.w,target.h){Ok(pixels)=>Some(pixels),Err(error)=>{self.status=error;return false;}};
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
        if self.channel_target.is_some() || self.quick_mask_painting() {
            self.clear_channel_selection();
            return true;
        }
        let Some(layer) = self.raster_target() else {
            return false;
        };
        let Some(mask) = self.owned_pixel_sel_mask(layer) else {
            return false;
        };
        if !mask.iter().any(|value| *value > 0.) {
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
        if pixels.native.is_some() {
            let before = pixels.clone();
            let samples = crate::deep_color::storage::collect(
                (0..pixels.w as usize * pixels.h as usize).flat_map(|i| {
                    let p=pixels.sample(i);
                    if mask[i]==1.{[0.;4]}else if mask[i]==0.{p}else{[p[0],p[1],p[2],p[3]*(1.-mask[i])]}
                }),
            );
            let mut after = before.clone();
            match samples.and_then(|samples| after.set_sample_allocation(samples)) {
                Ok(()) => self.commit(Cmd::NativePixels {
                    layer,
                    mask: false,
                    before,
                    after,
                }),
                Err(error) => self.status = error,
            };
            return true;
        }
        let before = pixels.data.clone();
        let mut after = match before.try_clone_now(&crate::deep_color::storage::check) {
            Ok(value) => value,
            Err(error) => {
                self.status = error;
                return false;
            }
        };
        for (index, &selected) in mask.iter().enumerate() {
            if selected != 0. {
                let offset = index * 4;
                if selected==1.{after.as_mut_slice()[offset..offset+4].fill(0);}else{after.as_mut_slice()[offset+3]=(before[offset+3] as f32*(1.-selected)).round() as u8;}
            }
        }
        if after.as_slice() == before.as_slice() {
            return true;
        }
        self.commit(Cmd::Pixels {
            layer,
            mask: false,
            before,
            after: std::sync::Arc::new(after),
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
        self.channel_target = None;
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
        let mut pixels = match Pixels::new(w, h) {
            Ok(pixels) => pixels,
            Err(error) => {
                self.status = error;
                return;
            }
        };
        for pixel in pixels.bytes_mut().unwrap().as_chunks_mut::<4>().0 {
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
        if mask.native.is_some() {
            let samples = crate::deep_color::storage::collect(
                (0..mask.w as usize * mask.h as usize).flat_map(|i| {
                    let p = mask.sample(i);
                    let v = 1. - (p[0] * 0.2126 + p[1] * 0.7152 + p[2] * 0.0722) * p[3];
                    [v, v, v, 1.]
                }),
            );
            if let Err(error) = samples.and_then(|samples| mask.set_sample_allocation(samples)) {
                self.status = error;
                return;
            }
            self.replace_layer_mask(index, Some(mask));
            self.status = "Mask inverted".into();
            return;
        }
        let bytes = match mask.bytes_mut() {
            Ok(bytes) => bytes,
            Err(error) => {
                self.status = error;
                return;
            }
        };
        for pixel in bytes.as_chunks_mut::<4>().0 {
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
        if pixels.native.is_some() || mask.native.is_some() {
            let after = match pixels.masked(mask) {
                Ok(after) => after,
                Err(error) => {
                    self.status = error;
                    return;
                }
            };
            let cmd = Cmd::NativePixels {
                layer: index,
                mask: false,
                before: pixels.clone(),
                after,
            };
            let old_mask = mask.clone();
            self.commit(Cmd::Batch(vec![
                cmd,
                Cmd::SetLayerMask {
                    index,
                    before: Some(old_mask),
                    after: None,
                },
            ]));
            self.paint_mask = false;
            self.status = "Native-precision mask applied".into();
            return;
        }
        let before = pixels.data.clone();
        let mut after = match before.try_clone_now(&crate::deep_color::storage::check) {
            Ok(value) => value,
            Err(error) => {
                self.status = error;
                return;
            }
        };
        for (pixel, mask_pixel) in after
            .as_mut_slice()
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
                after: std::sync::Arc::new(after),
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

    pub(crate) fn replace_layer_mask(&mut self, index: usize, after: Option<Pixels>) {
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
        let mut layer = Layer::raster("Pixels", 4, 1).unwrap();
        layer
            .kind
            .pixels_mut()
            .unwrap()
            .replace_bytes([220, 50, 20, 255].repeat(4))
            .unwrap();
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

    fn transformed_selection_studio() -> Studio {
        let mut studio = pixels_studio();
        studio.doc.width = 64.0;
        studio.doc.height = 48.0;
        studio.doc.layers[0] = Layer::placed_raster(
            "Placed",
            Pixels::new(16, 12).unwrap(),
            Pt::new(5.0, 7.0),
            Pt::new(40.0, 30.0),
        );
        if let LayerKind::Raster { rotation, .. } = &mut studio.doc.layers[0].kind {
            *rotation = 0.27;
        }
        studio.replace_pixel_selection(
            (0..64 * 48)
                .map(|i| ((i % 64) * 3 + (i / 64) * 2) as u8)
                .collect(),
            SelectionSpace {
                w: 64,
                h: 48,
                transform: tiny_skia::Transform::identity(),
            },
        );
        studio
    }

    fn mapped(studio: &Studio) -> std::sync::Arc<crate::resources::Allocation<f32>> {
        let SelectionMask::Mapped(values) = studio.cached_pixel_sel_mask(0).unwrap() else {
            panic!("transformed mask");
        };
        let expected = resample(
            studio.pixel_sel.as_ref().unwrap(),
            studio.pixel_sel_space.unwrap(),
            studio.layer_selection_space(0).unwrap(),
        )
        .unwrap();
        assert_eq!(values.as_slice(), expected.as_slice());
        values
    }

    #[test]
    fn mapped_selection_reuses_pixels_and_invalidates_on_transform_resize_and_nudge() {
        let mut studio = transformed_selection_studio();
        let first = mapped(&studio);
        assert!(std::sync::Arc::ptr_eq(&first, &mapped(&studio)));
        // Preserve the public Cow API for callers that need an owned snapshot.
        assert_eq!(&*studio.pixel_sel_mask(0).unwrap(), first.as_slice());
        if let LayerKind::Raster { origin, .. } = &mut studio.doc.layers[0].kind {
            origin.x += 6.0;
        }
        let moved = mapped(&studio);
        assert!(!std::sync::Arc::ptr_eq(&first, &moved));
        assert_ne!(first, moved);
        if let LayerKind::Raster { pixels, .. } = &mut studio.doc.layers[0].kind {
            *pixels = Pixels::new(24, 8).unwrap();
        }
        let resized = mapped(&studio);
        assert_eq!(
            resized.len(),
            moved.len(),
            "equal byte counts with different dimensions must invalidate"
        );
        assert!(!std::sync::Arc::ptr_eq(&moved, &resized));
        studio.nudge_pixel_sel(3, -2);
        assert!(studio.pixel_sel_cache.entry.borrow().is_none());
        let nudged = mapped(&studio);
        assert!(!std::sync::Arc::ptr_eq(&resized, &nudged));
        assert!(std::sync::Arc::ptr_eq(&nudged, &mapped(&studio)));
        studio.set_pixel_sel(None);
        assert!(studio.pixel_sel_cache.entry.borrow().is_none());
        assert!(studio.cached_pixel_sel_mask(0).is_none());
        studio.set_pixel_sel(Some(vec![128; 24 * 8]));
        let Some(SelectionMask::Borrowed(mask)) = studio.cached_pixel_sel_mask(0) else {
            panic!("native selection should borrow");
        };
        assert_eq!(mask.as_ptr(), studio.pixel_sel.as_ref().unwrap().as_ptr());
    }

    #[test]
    fn cancelling_a_brush_restores_pixels_without_preparing_its_transformed_selection() {
        let mut studio = transformed_selection_studio();
        let before = studio.doc.layers[0].kind.pixels().unwrap().data.to_vec();
        let mut buf = crate::deep_color::surface::Surface::new(16, 12).unwrap();
        buf.fill(tiny_skia::Color::from_rgba8(200, 50, 20, 255));
        studio.op = Some(Op::Brush {
            layer: 0,
            erase: false,
            buf,
            preview: None,
            last: Some(Pt::new(4.0, 4.0)),
            before: crate::document::RasterState::capture(&studio.doc.layers[0]).unwrap(),
            selection_generation: studio.pixel_sel_gen,
        });
        assert!(studio.end_pixel_stroke(true));
        assert!(
            studio.pixel_sel_cache.entry.borrow().is_none(),
            "discarded paint must not resample a selection"
        );
        assert_eq!(
            studio.doc.layers[0].kind.pixels().unwrap().data.as_slice(),
            before
        );
        assert!(studio.op.is_none());
        assert_eq!(studio.history.len(), 0);
    }

    #[test]
    fn mapped_selection_cache_is_released_on_tab_switch_and_restored_from_that_tabs_mask() {
        let mut studio = transformed_selection_studio();
        let first = mapped(&studio);
        studio.show_welcome = false;
        studio.ensure_tabs();
        studio.new_tab();
        assert_eq!(studio.tab_count(), 2);
        assert_eq!(studio.active_tab, 1);
        assert!(studio.pixel_sel_cache.entry.borrow().is_none());
        assert!(studio.cached_pixel_sel_mask(0).is_none());
        studio.switch_tab(0);
        assert!(studio.pixel_sel_cache.entry.borrow().is_none());
        let restored = mapped(&studio);
        assert_eq!(first, restored);
        assert!(!std::sync::Arc::ptr_eq(&first, &restored));
        assert!(std::sync::Arc::ptr_eq(&restored, &mapped(&studio)));
    }

    #[test]
    fn selection_mask_survives_serialization_and_apply_is_one_undo() {
        let mut studio = pixels_studio();
        studio.pixel_sel = Some(crate::deep_color::storage::adopt(vec![0., 64./255., 128./255., 1.]).unwrap());
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

        let original = studio.doc.layers[0].kind.pixels().unwrap().data.to_vec();
        studio.apply_layer_mask(0);
        assert!(studio.doc.layers[0].mask.is_none());
        let applied = alphas(&studio.doc);
        for (&a, b) in applied.iter().zip(&masked) {
            assert!(a.abs_diff(*b) <= 1);
        }
        studio.undo();
        assert_eq!(
            studio.doc.layers[0].kind.pixels().unwrap().data.as_slice(),
            original
        );
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
        studio.pixel_sel = Some(crate::deep_color::storage::adopt(vec![1., 1., 0., 0., 1., 1., 0., 0.]).unwrap());
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

#[cfg(test)]
mod precision_tests {
    use super::*;
    #[test]
    fn craft_selection_precision_delete_uses_original_float_weight_and_one_undo() {
        let mut s=Studio::new();s.doc=crate::document::Document::new("Selection clear",1.,1.,96.);
        let original=Pixels::from_samples(1,1,crate::deep_color::Depth::F32,vec![0.25,0.5,0.75,0.5]).unwrap();
        s.doc.layers=vec![crate::document::Layer::placed_raster("Native",original.clone(),Pt::ZERO,Pt::new(1.,1.))];s.active_layer=Some(0);
        let weight=f32::from_bits(0x3eaa8001);s.replace_pixel_selection_float(vec![weight],SelectionSpace{w:1,h:1,transform:tiny_skia::Transform::identity()});
        let before=crate::project::encode(&s.doc).unwrap();let history=s.history.len();assert!(s.clear_selected_pixels());
        let actual=s.doc.layers[0].kind.pixels().unwrap().sample(0);assert_eq!(&actual[..3],&[0.25,0.5,0.75]);assert_eq!(actual[3].to_bits(),(0.5*(1.-weight)).to_bits());
        assert_eq!(s.history.len(),history+1);s.undo();assert_eq!(crate::project::encode(&s.doc).unwrap(),before);s.redo();assert_eq!(s.doc.layers[0].kind.pixels().unwrap().sample(0),actual);
    }
}
