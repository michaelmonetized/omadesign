//! Pixel edits that operate on the destination: masks, erasing and healing.
use crate::app::{Op, Studio};
use crate::document::{Cmd, Fill, Pixels};
use crate::geom::Pt;
use crate::paint;
use crate::tools::Tool;

pub(super) fn pixels(studio: &Studio, layer: usize, mask: bool) -> Option<&Pixels> {
    let layer = studio.doc.layers.get(layer)?;
    if mask {
        layer.mask.as_ref()
    } else {
        layer.kind.pixels()
    }
}

fn publish(
    studio: &mut Studio,
    layer: usize,
    mask: bool,
    buffer: &tiny_skia::Pixmap,
    before: &[u8],
) {
    // Keep working buffers unfeathered so moving the pointer cannot repeatedly
    // attenuate earlier dabs. Only the displayed/committed result is blended.
    let feathered = if !mask {
        studio.cached_pixel_sel_mask(layer).and_then(|selection| {
            if !selection.iter().any(|v| *v > 0 && *v < 255) {
                return None;
            }
            let original =
                Pixels::from_rgba(buffer.width(), buffer.height(), before.to_vec())?.to_pixmap()?;
            let mut output = buffer.clone();
            paint::feather_edit(&mut output, &original, &selection);
            Some(output)
        })
    } else {
        None
    };
    let buffer = feathered.as_ref().unwrap_or(buffer);
    let Some(layer) = studio.doc.layers.get_mut(layer) else {
        return;
    };
    let target = if mask {
        layer.mask.as_mut()
    } else {
        layer.kind.pixels_mut()
    };
    if let Some(target) = target {
        *target = Pixels::from_pixmap(buffer);
        studio.mark();
    }
}

/// Publish just the pixels changed by the latest segment, retaining the
/// premultiplied render cache instead of reallocating/converting a whole layer.
fn publish_region(
    studio: &mut Studio,
    layer: usize,
    mask: bool,
    buffer: &tiny_skia::Pixmap,
    before: &[u8],
    region: paint::PixelRegion,
) {
    let selection = (!mask)
        .then(|| studio.cached_pixel_sel_mask(layer))
        .flatten();
    let mut patch = Vec::with_capacity((region.x1 - region.x0) * (region.y1 - region.y0));
    for row in region.rows(buffer.width()) {
        for i in row {
            let pixel = buffer.pixels()[i];
            let coverage = selection.as_ref().map_or(255, |selection| selection[i]);
            let pixel = if coverage == 255 {
                pixel
            } else {
                let rgba = &before[i * 4..i * 4 + 4];
                let old =
                    tiny_skia::ColorU8::from_rgba(rgba[0], rgba[1], rgba[2], rgba[3]).premultiply();
                let mix = |new: u8, old: u8| {
                    ((new as u32 * coverage as u32 + old as u32 * (255 - coverage as u32) + 127)
                        / 255) as u8
                };
                tiny_skia::PremultipliedColorU8::from_rgba(
                    mix(pixel.red(), old.red()),
                    mix(pixel.green(), old.green()),
                    mix(pixel.blue(), old.blue()),
                    mix(pixel.alpha(), old.alpha()),
                )
                .unwrap()
            };
            patch.push(pixel);
        }
    }
    drop(selection);
    let Some(layer) = studio.doc.layers.get_mut(layer) else {
        return;
    };
    let target = if mask {
        layer.mask.as_mut()
    } else {
        layer.kind.pixels_mut()
    };
    let Some(target) = target else {
        return;
    };
    // A stroke's initial full publish normalizes straight RGBA once; subsequent
    // region updates retain identical bytes and cached pixels outside the edit.
    target.with_pm(|_| {});
    target.version = target.version.wrapping_add(1);
    target.cached_uniform.get_mut().take();
    let cache = target.cached_pm.get_mut().as_mut();
    let mut cache = cache.map(|(version, pm)| {
        *version = target.version;
        pm
    });
    let mut patch = patch.into_iter();
    for row in region.rows(buffer.width()) {
        for i in row {
            let pixel = patch.next().unwrap();
            let straight = pixel.demultiply();
            target.data[i * 4..i * 4 + 4].copy_from_slice(&[
                straight.red(),
                straight.green(),
                straight.blue(),
                straight.alpha(),
            ]);
            if let Some(pm) = &mut cache {
                pm.pixels_mut()[i] = pixel;
            }
        }
    }
    studio.mark_interaction();
}

pub(super) fn start(studio: &mut Studio, world: Pt) {
    let mask = studio.paint_mask;
    let heal = studio.tool == Tool::Heal;
    let erase = studio.tool == Tool::Eraser;
    if heal && mask {
        studio.status = "Choose Pixels to use the healing brush".into();
        return;
    }
    if heal && studio.clone_source.is_none() {
        studio.status = "Alt-click clean texture to set the healing source".into();
        return;
    }
    let target = if mask {
        studio.mask_target()
    } else {
        studio.raster_target()
    };
    let Some(layer) = target else {
        studio.status = if mask {
            "Choose an unlocked layer with a mask"
        } else {
            "Choose an unlocked pixel layer to paint"
        }
        .into();
        return;
    };
    let point = studio.mask_point(layer, world);
    let Some(pixels) = pixels(studio, layer, mask) else {
        return;
    };
    let before = pixels.data.clone();
    let Some(mut buf) = pixels.to_pixmap() else {
        return;
    };
    let source = heal.then(|| buf.clone());
    let offset = studio
        .clone_source
        .map_or(Pt::ZERO, |source| studio.mask_point(layer, source) - point);
    let brush = if mask {
        studio.mask_brush(erase)
    } else {
        studio.brush
    };
    if let Some(source) = &source {
        paint::heal_stamp(&mut buf, source, point, point + offset, &brush);
    } else {
        paint::stamp(&mut buf, point, &brush, erase && !mask);
    }
    let region = paint::PixelRegion::stroke(buf.width(), buf.height(), point, point, &brush);
    clip_working(studio, layer, mask, &mut buf, &before, region);
    publish(studio, layer, mask, &buf, &before);
    studio.op = Some(Op::Retouch {
        layer,
        mask,
        erase,
        heal,
        source,
        buf,
        offset,
        last: world,
        before,
        selection_generation: studio.pixel_sel_gen,
    });
}

fn clip_working(
    studio: &Studio,
    layer: usize,
    mask_edit: bool,
    buf: &mut tiny_skia::Pixmap,
    before: &[u8],
    region: paint::PixelRegion,
) {
    if mask_edit {
        return;
    }
    let Some(sel) = studio.cached_pixel_sel_mask(layer) else {
        return;
    };
    let width = buf.width();
    for row in region.rows(width) {
        for i in row {
            if sel[i] == 0 {
                let rgba = &before[i * 4..i * 4 + 4];
                buf.pixels_mut()[i] =
                    tiny_skia::ColorU8::from_rgba(rgba[0], rgba[1], rgba[2], rgba[3]).premultiply();
            }
        }
    }
}

fn clip_overlay(
    studio: &Studio,
    layer: usize,
    buf: &mut tiny_skia::Pixmap,
    region: paint::PixelRegion,
) {
    if let Some(sel) = studio.cached_pixel_sel_mask(layer) {
        paint::clip_overlay_region(buf, &sel, region);
    }
}

fn layer_pixels(studio: &Studio, layer: usize) -> Option<(u32, u32, Vec<u8>)> {
    let pixels = studio.doc.layers.get(layer)?.kind.pixels()?;
    Some((pixels.w, pixels.h, pixels.data.clone()))
}

pub(super) fn start_brush(studio: &mut Studio, world: Pt) {
    let Some(layer) = studio.raster_target() else {
        studio.status = "add a pixel layer to paint".into();
        return;
    };
    let Some((w, h, before)) = layer_pixels(studio, layer) else {
        return;
    };
    let Some(mut buf) = tiny_skia::Pixmap::new(w, h) else {
        return;
    };
    let point = studio.mask_point(layer, world);
    paint::stamp(&mut buf, point, &studio.brush, false);
    let region = paint::PixelRegion::stroke(w, h, point, point, &studio.brush);
    clip_overlay(studio, layer, &mut buf, region);
    let preview = studio.cached_pixel_sel_mask(layer).and_then(|mask| {
        if !mask.iter().any(|&coverage| coverage > 0 && coverage < 255) {
            return None;
        }
        let mut preview = tiny_skia::Pixmap::new(w, h)?;
        paint::feather_overlay_region(&mut preview, &buf, &mask, region);
        Some(preview)
    });
    studio.op = Some(Op::Brush {
        layer,
        erase: false,
        buf,
        preview,
        last: Some(world),
        before,
        selection_generation: studio.pixel_sel_gen,
    });
}

pub(super) fn start_smudge(studio: &mut Studio, world: Pt) {
    let Some(layer) = studio.raster_target() else {
        studio.status = "Choose an unlocked pixel layer to smudge".into();
        return;
    };
    let Some((_w, _h, before)) = layer_pixels(studio, layer) else {
        return;
    };
    let Some(buf) = pixels(studio, layer, false).and_then(Pixels::to_pixmap) else {
        return;
    };
    studio.op = Some(Op::Smudge {
        layer,
        last: Some(world),
        buf,
        published: false,
        before,
        selection_generation: studio.pixel_sel_gen,
    });
}

pub(super) fn start_clone(studio: &mut Studio, world: Pt) {
    if studio.clone_source.is_none() {
        studio.status = "Alt-click to set clone source".into();
        return;
    }
    let Some(layer) = studio.raster_target() else {
        studio.status = "Choose an unlocked pixel layer to clone".into();
        return;
    };
    let Some((_w, _h, before)) = layer_pixels(studio, layer) else {
        return;
    };
    let Some(mut buf) = pixels(studio, layer, false).and_then(Pixels::to_pixmap) else {
        return;
    };
    let dest = studio.mask_point(layer, world);
    let source = studio
        .clone_source
        .map_or(dest, |source| studio.mask_point(layer, source));
    let offset = source - dest;
    paint::clone_stamp(&mut buf, dest, dest + offset, &studio.brush);
    let region = paint::PixelRegion::stroke(buf.width(), buf.height(), dest, dest, &studio.brush);
    clip_working(studio, layer, false, &mut buf, &before, region);
    publish(studio, layer, false, &buf, &before);
    studio.op = Some(Op::Clone {
        layer,
        last: Some(world),
        offset,
        buf,
        before,
        selection_generation: studio.pixel_sel_gen,
    });
}

pub(super) fn pixel_op(shift: bool, alt: bool) -> paint::PixelCombine {
    if alt {
        paint::PixelCombine::Subtract
    } else if shift {
        paint::PixelCombine::Add
    } else {
        paint::PixelCombine::Replace
    }
}

pub(super) fn apply_wand(studio: &mut Studio, world: Pt, op: paint::PixelCombine) {
    let Some(layer) = studio.raster_target() else {
        studio.status = "Choose an unlocked pixel layer to select".into();
        return;
    };
    let Some(pm) = pixels(studio, layer, false).and_then(Pixels::to_pixmap) else {
        return;
    };
    let seed = studio.mask_point(layer, world);
    let clip = (op == paint::PixelCombine::Add)
        .then(|| {
            studio
                .cached_pixel_sel_mask(layer)
                .map(|mask| mask.to_vec())
        })
        .flatten();
    let mask = paint::wand_mask_clipped(&pm, seed, studio.fill_tolerance, clip.as_deref());
    if op == paint::PixelCombine::Replace {
        studio.set_pixel_sel(Some(mask));
    } else {
        studio.merge_pixel_sel(mask, op);
    }
}

pub(super) fn commit_marquee(
    studio: &mut Studio,
    start: Pt,
    cur: Pt,
    ellipse: bool,
    op: paint::PixelCombine,
) {
    let Some(layer) = studio.raster_target() else {
        studio.status = "Choose an unlocked pixel layer to select".into();
        return;
    };
    let Some((w, h, _)) = layer_pixels(studio, layer) else {
        return;
    };
    if (cur - start).length() < 2.0 {
        if op == paint::PixelCombine::Replace {
            studio.set_pixel_sel(None);
        }
        return;
    }
    let a = studio.mask_point(layer, start);
    let b = studio.mask_point(layer, cur);
    let mask = if ellipse {
        paint::fill_ellipse_mask(w, h, a.x, a.y, b.x, b.y)
    } else {
        paint::fill_rect_mask(w, h, a.x, a.y, b.x, b.y)
    };
    studio.merge_pixel_sel(mask, op);
}

pub(super) fn commit_lasso(studio: &mut Studio, pts: &[Pt], op: paint::PixelCombine) {
    let Some(layer) = studio.raster_target() else {
        studio.status = "Choose an unlocked pixel layer to select".into();
        return;
    };
    let Some((w, h, _)) = layer_pixels(studio, layer) else {
        return;
    };
    if pts.len() < 3 {
        if op == paint::PixelCombine::Replace {
            studio.set_pixel_sel(None);
        }
        return;
    }
    let mapped: Vec<Pt> = pts
        .iter()
        .map(|point| studio.mask_point(layer, *point))
        .collect();
    let mask = paint::fill_poly_mask(w, h, &mapped);
    studio.merge_pixel_sel(mask, op);
}

pub(super) fn drag(studio: &mut Studio, world: Pt) {
    let Some(Op::Retouch {
        layer,
        mask,
        erase,
        heal,
        source,
        mut buf,
        offset,
        last,
        before,
        mut selection_generation,
    }) = studio.op.take()
    else {
        return;
    };
    let point = studio.mask_point(layer, world);
    let from = studio.mask_point(layer, last);
    let brush = if mask {
        studio.mask_brush(erase)
    } else {
        studio.brush
    };
    if (point - from).length_sq() > 0.0001 {
        if let Some(source) = &source {
            paint::heal_stroke(&mut buf, source, from, point, offset, &brush);
        } else {
            paint::stroke_to(&mut buf, from, point, &brush, erase && !mask);
        }
        let selection_changed = selection_generation != studio.pixel_sel_gen;
        let region = if selection_changed {
            paint::PixelRegion::full(buf.width(), buf.height())
        } else {
            paint::PixelRegion::stroke(buf.width(), buf.height(), from, point, &brush)
        };
        clip_working(studio, layer, mask, &mut buf, &before, region);
        if selection_changed {
            publish(studio, layer, mask, &buf, &before);
        } else {
            publish_region(studio, layer, mask, &buf, &before, region);
        }
        selection_generation = studio.pixel_sel_gen;
    }
    studio.op = Some(Op::Retouch {
        layer,
        mask,
        erase,
        heal,
        source,
        buf,
        offset,
        last: world,
        before,
        selection_generation,
    });
}

pub(super) fn finish(studio: &mut Studio, layer: usize, mask: bool, before: Vec<u8>) {
    let Some(after) = pixels(studio, layer, mask).map(|pixels| pixels.data.clone()) else {
        return;
    };
    if before != after {
        studio.history.push(Cmd::Pixels {
            layer,
            mask,
            before,
            after,
        });
        studio.dirty = true;
        studio.mark();
    }
}

pub(super) fn fill(studio: &mut Studio, world: Pt) {
    let mask = studio.paint_mask;
    let Some(layer) = (if mask {
        studio.mask_target()
    } else {
        studio.raster_target()
    }) else {
        return;
    };
    let point = studio.mask_point(layer, world);
    let Some(pixels) = pixels(studio, layer, mask) else {
        return;
    };
    let before = pixels.data.clone();
    let Some(mut buffer) = pixels.to_pixmap() else {
        return;
    };
    let color = if mask {
        studio.mask_brush(false).color
    } else {
        match studio.style.fill {
            Fill::Solid(color) => color,
            _ => studio.brush.color,
        }
    };
    let clip = if mask {
        None
    } else {
        studio
            .cached_pixel_sel_mask(layer)
            .map(|mask| mask.to_vec())
    };
    paint::flood_fill_clipped(
        &mut buffer,
        point,
        color,
        studio.fill_tolerance,
        clip.as_deref(),
    );
    publish(studio, layer, mask, &buffer, &before);
    finish(studio, layer, mask, before);
}

pub(super) fn brush_drag(studio: &mut Studio, world: Pt) {
    let Some(Op::Brush {
        layer,
        erase,
        mut buf,
        mut preview,
        last,
        before,
        selection_generation,
    }) = (match &studio.op {
        Some(Op::Brush { .. }) => studio.op.take(),
        _ => None,
    })
    else {
        return;
    };
    if last.is_some_and(|prev| (prev - world).length_sq() < 0.0001) {
        studio.op = Some(Op::Brush {
            layer,
            erase,
            buf,
            preview,
            last,
            before,
            selection_generation,
        });
        return;
    }
    let from = last.map_or_else(
        || studio.mask_point(layer, world),
        |p| studio.mask_point(layer, p),
    );
    let to = studio.mask_point(layer, world);
    if let Some(_) = last {
        paint::stroke_to(&mut buf, from, to, &studio.brush, erase);
    } else {
        paint::stamp(&mut buf, to, &studio.brush, erase);
    }
    let selection_changed = selection_generation != studio.pixel_sel_gen;
    let region = if selection_changed {
        paint::PixelRegion::full(buf.width(), buf.height())
    } else {
        paint::PixelRegion::stroke(buf.width(), buf.height(), from, to, &studio.brush)
    };
    clip_overlay(studio, layer, &mut buf, region);
    if selection_changed {
        let soft = studio
            .cached_pixel_sel_mask(layer)
            .is_some_and(|mask| mask.iter().any(|&v| v > 0 && v < 255));
        if soft {
            if preview.is_none() {
                preview = tiny_skia::Pixmap::new(buf.width(), buf.height());
            }
        } else {
            preview = None;
        }
    }
    if let Some(preview) = &mut preview
        && let Some(mask) = studio.cached_pixel_sel_mask(layer)
    {
        paint::feather_overlay_region(preview, &buf, &mask, region);
    }
    studio.op = Some(Op::Brush {
        layer,
        erase,
        buf,
        preview,
        last: Some(world),
        before,
        selection_generation: studio.pixel_sel_gen,
    });
}

pub(super) fn smudge_drag(studio: &mut Studio, world: Pt) {
    let Some(Op::Smudge {
        layer,
        last,
        mut buf,
        mut published,
        before,
        mut selection_generation,
    }) = (match &studio.op {
        Some(Op::Smudge { .. }) => studio.op.take(),
        _ => None,
    })
    else {
        return;
    };
    if let Some(prev) = last
        && (prev - world).length_sq() > 0.0001
    {
        let from = studio.mask_point(layer, prev);
        let to = studio.mask_point(layer, world);
        paint::smudge_stroke(&mut buf, from, to, &studio.brush);
        let selection_changed = selection_generation != studio.pixel_sel_gen;
        let region = if selection_changed {
            paint::PixelRegion::full(buf.width(), buf.height())
        } else {
            paint::PixelRegion::stroke(buf.width(), buf.height(), from, to, &studio.brush)
        };
        clip_working(studio, layer, false, &mut buf, &before, region);
        if published && !selection_changed {
            publish_region(studio, layer, false, &buf, &before, region);
        } else {
            publish(studio, layer, false, &buf, &before);
            published = true;
        }
        selection_generation = studio.pixel_sel_gen;
    }
    studio.op = Some(Op::Smudge {
        layer,
        last: Some(world),
        buf,
        published,
        before,
        selection_generation,
    });
}

pub(super) fn clone_drag(studio: &mut Studio, world: Pt) {
    let Some(Op::Clone {
        layer,
        last,
        offset,
        mut buf,
        before,
        mut selection_generation,
    }) = (match &studio.op {
        Some(Op::Clone { .. }) => studio.op.take(),
        _ => None,
    })
    else {
        return;
    };
    if let Some(prev) = last
        && (prev - world).length_sq() > 0.0001
    {
        let from = studio.mask_point(layer, prev);
        let to = studio.mask_point(layer, world);
        if studio.clone_aligned {
            paint::clone_stroke(&mut buf, from, to, from + offset, &studio.brush);
        } else if let Some(source) = studio.clone_source {
            paint::clone_stamp(
                &mut buf,
                to,
                studio.mask_point(layer, source),
                &studio.brush,
            );
        } else {
            paint::clone_stroke(&mut buf, from, to, from + offset, &studio.brush);
        }
        let selection_changed = selection_generation != studio.pixel_sel_gen;
        let region = if selection_changed {
            paint::PixelRegion::full(buf.width(), buf.height())
        } else {
            paint::PixelRegion::stroke(buf.width(), buf.height(), from, to, &studio.brush)
        };
        clip_working(studio, layer, false, &mut buf, &before, region);
        if selection_changed {
            publish(studio, layer, false, &buf, &before);
        } else {
            publish_region(studio, layer, false, &buf, &before, region);
        }
        selection_generation = studio.pixel_sel_gen;
    }
    studio.op = Some(Op::Clone {
        layer,
        last: Some(world),
        offset,
        buf,
        before,
        selection_generation,
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color::Rgba;
    use crate::document::{Document, Layer, LayerKind};
    use eframe::egui::{
        Context, Event, Modifiers, PointerButton, Pos2, RawInput, Rect, pos2, vec2,
    };

    fn studio(w: u32, h: u32) -> Studio {
        let mut studio = Studio::new();
        studio.doc = Document::new("Retouch", w as f32, h as f32, 96.0);
        studio.doc.transparent = true;
        studio.doc.layers = vec![Layer::raster("Pixels", w, h)];
        let pixels = studio.doc.layers[0].kind.pixels_mut().unwrap();
        pixels.data = [160, 170, 180, 255].repeat((w * h) as usize);
        pixels.touch();
        studio.active_layer = Some(0);
        studio.selection.clear();
        studio.show_welcome = false;
        studio.show_rulers = false;
        studio.need_fit = false;
        studio.snap.enabled = false;
        studio.brush = paint::Brush {
            size: 6.0,
            hardness: 0.9,
            flow: 1.0,
            opacity: 1.0,
            ..Default::default()
        };
        studio
    }

    fn rgba(studio: &Studio, x: u32, y: u32, mask: bool) -> [u8; 4] {
        let pixels = pixels(studio, 0, mask).unwrap();
        let offset = ((y * pixels.w + x) * 4) as usize;
        pixels.data[offset..offset + 4].try_into().unwrap()
    }

    #[test]
    fn dirty_region_publication_matches_full_buffer_for_every_retouch_tool() {
        for tool in [
            Tool::Eraser,
            Tool::Heal,
            Tool::Smudge,
            Tool::Clone,
            Tool::Brush,
        ] {
            for selected in [false, true] {
                let mut studio = studio(96, 80);
                studio.tool = tool;
                studio.brush.size = 13.5;
                studio.brush.flow = 0.63;
                studio.brush.opacity = 0.71;
                studio.clone_source = Some(Pt::new(45.0, 25.0));
                let px = studio.doc.layers[0].kind.pixels_mut().unwrap();
                for (i, pixel) in px.data.chunks_exact_mut(4).enumerate() {
                    pixel.copy_from_slice(&[
                        (i * 29) as u8,
                        (i * 43) as u8,
                        (i * 61) as u8,
                        (i * 17) as u8,
                    ]);
                }
                px.touch();
                if selected {
                    studio.set_pixel_sel(Some(
                        (0..96 * 80)
                            .map(|i| [0, 19, 128, 233, 255][i % 5])
                            .collect(),
                    ));
                }
                if tool == Tool::Brush {
                    studio.add_layer_mask(0, true);
                }
                let mask = studio.paint_mask;
                let before = pixels(&studio, 0, mask).unwrap().data.clone();
                let start_point = Pt::new(8.25, 7.75);
                match tool {
                    Tool::Smudge => start_smudge(&mut studio, start_point),
                    Tool::Clone => start_clone(&mut studio, start_point),
                    _ => start(&mut studio, start_point),
                }
                for (index, point) in [
                    Pt::new(18.5, 25.0),
                    Pt::new(45.0, 43.25),
                    Pt::new(93.5, 78.75),
                    Pt::new(103.0, 85.0),
                ]
                .into_iter()
                .enumerate()
                {
                    if selected && index == 1 {
                        studio.nudge_pixel_sel(2, 1);
                    }
                    if selected && index == 2 {
                        studio.set_pixel_sel(None);
                    }
                    match tool {
                        Tool::Smudge => smudge_drag(&mut studio, point),
                        Tool::Clone => clone_drag(&mut studio, point),
                        _ => drag(&mut studio, point),
                    }
                    let buffer = match studio.op.as_ref().unwrap() {
                        Op::Smudge { buf, .. }
                        | Op::Clone { buf, .. }
                        | Op::Retouch { buf, .. } => buf,
                        _ => unreachable!(),
                    };
                    let mut reference = buffer.clone();
                    if !mask && let Some(selection) = studio.cached_pixel_sel_mask(0) {
                        let original = Pixels::from_rgba(96, 80, before.clone())
                            .unwrap()
                            .to_pixmap()
                            .unwrap();
                        paint::restrict_pixmap(&mut reference, &original, &selection);
                        paint::feather_edit(&mut reference, &original, &selection);
                    }
                    let actual = pixels(&studio, 0, mask).unwrap();
                    assert_eq!(
                        actual.data,
                        Pixels::from_pixmap(&reference).data,
                        "{tool:?}, selection {selected}, {point:?}"
                    );
                    actual.with_pm(|cached| {
                        assert_eq!(cached.data(), reference.data(), "render cache {tool:?}")
                    });
                }
                studio.end_pixel_stroke(false);
                let after = pixels(&studio, 0, mask).unwrap().data.clone();
                studio.undo();
                assert_eq!(pixels(&studio, 0, mask).unwrap().data, before);
                studio.redo();
                assert_eq!(pixels(&studio, 0, mask).unwrap().data, after);
            }
        }
    }

    #[test]
    fn incremental_brush_preview_matches_full_feathering_and_keeps_all_dabs() {
        let mut studio = studio(96, 80);
        studio.tool = Tool::Brush;
        studio.brush.size = 15.5;
        studio.brush.flow = 0.45;
        studio.set_pixel_sel(Some(
            (0..96 * 80)
                .map(|i| [0, 31, 128, 235, 255][i % 5])
                .collect(),
        ));
        start_brush(&mut studio, Pt::new(-3.0, 5.5));
        for point in [
            Pt::new(19.5, 18.25),
            Pt::new(21.0, 55.0),
            Pt::new(101.0, 79.0),
        ] {
            brush_drag(&mut studio, point);
            let Some(Op::Brush {
                buf,
                preview: Some(preview),
                ..
            }) = &studio.op
            else {
                panic!("soft preview");
            };
            let mut reference = buf.clone();
            paint::feather_overlay(&mut reference, &studio.cached_pixel_sel_mask(0).unwrap());
            assert_eq!(preview, &reference);
        }
        studio.nudge_pixel_sel(-2, 3);
        brush_drag(&mut studio, Pt::new(40.0, 15.0));
        let Some(Op::Brush {
            buf,
            preview: Some(preview),
            ..
        }) = &studio.op
        else {
            panic!("updated soft preview");
        };
        let mut reference = buf.clone();
        paint::feather_overlay(&mut reference, &studio.cached_pixel_sel_mask(0).unwrap());
        assert_eq!(preview, &reference);
        studio.set_pixel_sel(None);
        brush_drag(&mut studio, Pt::new(50.0, 15.0));
        assert!(matches!(studio.op, Some(Op::Brush { preview: None, .. })));
        studio.end_pixel_stroke(true);
        studio.set_pixel_sel(Some(vec![255; 96 * 80]));
        start_brush(&mut studio, Pt::new(20.0, 20.0));
        assert!(matches!(studio.op, Some(Op::Brush { preview: None, .. })));
    }

    #[test]
    fn queued_brush_samples_keep_corners_release_and_one_undo_step() {
        let mut batched = studio(96, 80);
        batched.tool = Tool::Brush;
        batched.view = crate::compositor::View {
            scale: 2.0,
            offset: Pt::new(20.0, 20.0),
        };
        let before = pixels(&batched, 0, false).unwrap().data.clone();
        let mut separate = studio(96, 80);
        separate.tool = batched.tool;
        separate.view = batched.view;
        let batch_ctx = Context::default();
        let separate_ctx = Context::default();
        canvas_frame(&batch_ctx, &mut batched, vec![]);
        canvas_frame(&separate_ctx, &mut separate, vec![]);
        let rect = batched.canvas_rect.unwrap();
        let at = |x: f32, y: f32| rect.min + vec2(20.0 + x * 2.0, 20.0 + y * 2.0);
        let events = vec![
            Event::PointerMoved(at(10.0, 10.0)),
            Event::PointerButton {
                pos: at(10.0, 10.0),
                button: PointerButton::Primary,
                pressed: true,
                modifiers: Modifiers::NONE,
            },
            Event::PointerMoved(at(10.0, 35.0)),
            Event::PointerMoved(at(40.0, 35.0)),
            Event::PointerButton {
                pos: at(40.0, 60.0),
                button: PointerButton::Primary,
                pressed: false,
                modifiers: Modifiers::NONE,
            },
            Event::PointerMoved(at(80.0, 60.0)),
            Event::PointerGone,
        ];
        for event in events.clone() {
            canvas_frame(&separate_ctx, &mut separate, vec![event]);
        }
        canvas_frame(&batch_ctx, &mut batched, events);
        assert_eq!(
            pixels(&batched, 0, false).unwrap().data,
            pixels(&separate, 0, false).unwrap().data
        );
        assert_ne!(rgba(&batched, 10, 35, false), [160, 170, 180, 255]);
        assert_ne!(rgba(&batched, 40, 60, false), [160, 170, 180, 255]);
        assert_eq!(rgba(&batched, 80, 60, false), [160, 170, 180, 255]);
        assert_eq!(batched.history.len(), 1);
        batched.undo();
        assert_eq!(pixels(&batched, 0, false).unwrap().data, before);
    }

    #[test]
    fn single_frame_brush_click_commits_and_idle_hold_does_not_dirty_canvas() {
        let mut studio = studio(96, 80);
        studio.tool = Tool::Brush;
        studio.view = crate::compositor::View {
            scale: 2.0,
            offset: Pt::new(20.0, 20.0),
        };
        let ctx = Context::default();
        canvas_frame(&ctx, &mut studio, vec![]);
        let at = studio.canvas_rect.unwrap().min + vec2(60.0, 60.0);
        canvas_frame(
            &ctx,
            &mut studio,
            vec![
                Event::PointerMoved(at),
                Event::PointerButton {
                    pos: at,
                    button: PointerButton::Primary,
                    pressed: true,
                    modifiers: Modifiers::NONE,
                },
                Event::PointerButton {
                    pos: at,
                    button: PointerButton::Primary,
                    pressed: false,
                    modifiers: Modifiers::NONE,
                },
            ],
        );
        assert!(studio.op.is_none());
        assert_eq!(studio.history.len(), 1);
        assert_ne!(rgba(&studio, 20, 20, false), [160, 170, 180, 255]);
        canvas_frame(
            &ctx,
            &mut studio,
            vec![Event::PointerButton {
                pos: at,
                button: PointerButton::Primary,
                pressed: true,
                modifiers: Modifiers::NONE,
            }],
        );
        assert!(matches!(studio.op, Some(Op::Brush { .. })));
        let generation = studio.canvas_gen;
        canvas_frame(&ctx, &mut studio, vec![]);
        assert_eq!(studio.canvas_gen, generation);
        studio.end_pixel_stroke(true);
    }

    #[test]
    #[ignore = "manual CPU benchmark; run optimized with --nocapture"]
    fn benchmark_retouch_region_publication() {
        use std::time::Instant;
        let mut studio = studio(2048, 2048);
        studio.tool = Tool::Eraser;
        studio.brush.size = 32.0;
        studio.set_pixel_sel(Some(vec![128; 2048 * 2048]));
        let before = pixels(&studio, 0, false).unwrap().data.clone();
        let mut buffer = pixels(&studio, 0, false).unwrap().to_pixmap().unwrap();
        paint::stroke_to(
            &mut buffer,
            Pt::new(600.0, 800.0),
            Pt::new(610.0, 810.0),
            &studio.brush,
            true,
        );
        let region = paint::PixelRegion::stroke(
            2048,
            2048,
            Pt::new(600.0, 800.0),
            Pt::new(610.0, 810.0),
            &studio.brush,
        );
        let mut full = Vec::new();
        let mut partial = Vec::new();
        for _ in 0..32 {
            let start = Instant::now();
            publish(&mut studio, 0, false, &buffer, &before);
            full.push(start.elapsed().as_secs_f64() * 1000.0);
            let expected = pixels(&studio, 0, false).unwrap().data.clone();
            let start = Instant::now();
            publish_region(&mut studio, 0, false, &buffer, &before, region);
            partial.push(start.elapsed().as_secs_f64() * 1000.0);
            assert_eq!(pixels(&studio, 0, false).unwrap().data, expected);
        }
        full.sort_by(f64::total_cmp);
        partial.sort_by(f64::total_cmp);
        println!(
            "retouch publish 2048x2048 soft selection, 32px brush, 10px segment: full median {:.3} ms p95 {:.3} ms; region median {:.3} ms p95 {:.3} ms",
            (full[15] + full[16]) * 0.5,
            full[30],
            (partial[15] + partial[16]) * 0.5,
            partial[30]
        );
    }

    #[test]
    fn painting_a_placed_mask_uses_native_coordinates_and_undo_preserves_pixels() {
        let mut studio = studio(32, 32);
        if let LayerKind::Raster {
            origin,
            size,
            rotation,
            ..
        } = &mut studio.doc.layers[0].kind
        {
            *origin = Pt::new(50.0, 40.0);
            *size = Pt::new(64.0, 64.0);
            *rotation = std::f32::consts::FRAC_PI_2;
        }
        studio.add_layer_mask(0, true);
        let original = pixels(&studio, 0, false).unwrap().data.clone();
        let mask_before = pixels(&studio, 0, true).unwrap().data.clone();
        studio.history.clear();
        let transform = crate::compositor::layer_pixel_transform(&studio.doc.layers[0]);
        let world = |local: Pt| {
            let mut point = tiny_skia::Point::from_xy(local.x, local.y);
            transform.map_point(&mut point);
            Pt::new(point.x, point.y)
        };
        start(&mut studio, world(Pt::new(12.0, 14.0)));
        drag(&mut studio, world(Pt::new(20.0, 14.0)));
        assert!(rgba(&studio, 12, 14, true)[0] < 10);
        assert!(rgba(&studio, 18, 14, true)[0] < 10);
        assert_eq!(rgba(&studio, 2, 2, true), [255; 4]);
        assert_eq!(pixels(&studio, 0, false).unwrap().data, original);
        assert!(studio.end_pixel_stroke(false));
        assert_eq!(studio.history.len(), 1);
        studio.undo();
        assert_eq!(pixels(&studio, 0, true).unwrap().data, mask_before);
        assert_eq!(pixels(&studio, 0, false).unwrap().data, original);
        studio.redo();
        assert!(rgba(&studio, 18, 14, true)[0] < 10);
    }

    fn canvas_frame(ctx: &Context, studio: &mut Studio, events: Vec<Event>) {
        let mut output = ctx.run_ui(
            RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(400.0, 300.0))),
                events,
                ..Default::default()
            },
            |ui| {
                studio.handle_shortcuts(ui.ctx());
                super::super::canvas::show(ui, studio);
            },
        );
        output.textures_delta.clear();
    }

    #[test]
    fn alt_source_click_heals_with_local_colour_and_one_undo() {
        let mut studio = studio(32, 16);
        let image = studio.doc.layers[0].kind.pixels_mut().unwrap();
        for y in 0..16usize {
            for x in 0..16usize {
                image.data[(y * 32 + x) * 4..(y * 32 + x) * 4 + 4]
                    .copy_from_slice(&[60, 80, 100, 255]);
            }
        }
        image.data[(8 * 32 + 7) * 4..(8 * 32 + 7) * 4 + 4].copy_from_slice(&[80, 100, 120, 255]);
        image.data[(8 * 32 + 24) * 4..(8 * 32 + 24) * 4 + 4].copy_from_slice(&[20, 20, 20, 255]);
        image.touch();
        let original = image.data.clone();
        studio.tool = Tool::Heal;
        studio.brush.size = 10.0;
        start(&mut studio, Pt::new(24.0, 8.0));
        assert!(studio.op.is_none(), "healing requires a chosen source");
        let ctx = Context::default();
        let source = pos2(7.0, 8.0);
        canvas_frame(&ctx, &mut studio, vec![Event::PointerMoved(source)]);
        canvas_frame(
            &ctx,
            &mut studio,
            vec![
                Event::ModifiersChanged(Modifiers::ALT),
                Event::PointerButton {
                    pos: source,
                    button: PointerButton::Primary,
                    pressed: true,
                    modifiers: Modifiers::ALT,
                },
            ],
        );
        canvas_frame(
            &ctx,
            &mut studio,
            vec![Event::PointerButton {
                pos: source,
                button: PointerButton::Primary,
                pressed: false,
                modifiers: Modifiers::ALT,
            }],
        );
        assert_eq!(studio.clone_source, Some(Pt::new(7.0, 8.0)));
        assert_eq!(pixels(&studio, 0, false).unwrap().data, original);
        start(&mut studio, Pt::new(24.0, 8.0));
        let repaired = rgba(&studio, 24, 8, false);
        for (actual, expected) in repaired[..3].iter().zip([180, 190, 200]) {
            assert!(actual.abs_diff(expected) <= 1);
        }
        assert_eq!(repaired[3], 255);
        drag(&mut studio, Pt::new(25.0, 8.0));
        assert!(studio.end_pixel_stroke(false));
        assert_eq!(studio.history.len(), 1);
        studio.undo();
        assert_eq!(pixels(&studio, 0, false).unwrap().data, original);
    }

    #[test]
    fn eraser_changes_destination_alpha_and_cancel_restores_the_stroke() {
        let mut studio = studio(24, 24);
        studio.tool = Tool::Eraser;
        let original = pixels(&studio, 0, false).unwrap().data.clone();
        start(&mut studio, Pt::new(8.0, 8.0));
        drag(&mut studio, Pt::new(16.0, 8.0));
        assert!(rgba(&studio, 10, 8, false)[3] < 10);
        assert_eq!(rgba(&studio, 2, 2, false)[3], 255);
        studio.undo();
        assert_eq!(pixels(&studio, 0, false).unwrap().data, original);
        assert_eq!(studio.history.len(), 0);
        start(&mut studio, Pt::new(8.0, 8.0));
        assert!(studio.end_pixel_stroke(false));
        assert!(rgba(&studio, 8, 8, false)[3] < 10);
        studio.undo();
        assert_eq!(pixels(&studio, 0, false).unwrap().data, original);

        studio.add_layer_mask(0, true);
        studio.tool = Tool::Brush;
        studio.brush.color = Rgba::BLACK;
        let mask_before = pixels(&studio, 0, true).unwrap().data.clone();
        start(&mut studio, Pt::new(8.0, 8.0));
        studio.set_tool(Tool::Hand);
        assert_eq!(pixels(&studio, 0, true).unwrap().data, mask_before);
        assert!(studio.op.is_none());
    }

    #[test]
    fn marquee_wand_and_lasso_create_pixel_selections() {
        let mut studio = studio(16, 16);
        studio.tool = Tool::Marquee;
        commit_marquee(
            &mut studio,
            Pt::new(2.0, 2.0),
            Pt::new(8.0, 8.0),
            false,
            paint::PixelCombine::Replace,
        );
        let mask = studio.pixel_sel.as_ref().expect("rect selection");
        assert_eq!(mask.len(), 16 * 16);
        assert!(paint::selected_count(mask) >= 25);
        assert_eq!(mask[(2 * 16 + 2) as usize], 255);
        assert_eq!(mask[0], 0);

        studio.tool = Tool::EllipseMarquee;
        commit_marquee(
            &mut studio,
            Pt::new(0.0, 0.0),
            Pt::new(16.0, 16.0),
            true,
            paint::PixelCombine::Replace,
        );
        let ellipse = studio.pixel_sel.as_ref().unwrap();
        assert_eq!(ellipse[(8 * 16 + 8) as usize], 255);
        assert_eq!(ellipse[0], 0);

        studio.tool = Tool::Lasso;
        commit_lasso(
            &mut studio,
            &[
                Pt::new(1.0, 1.0),
                Pt::new(10.0, 1.0),
                Pt::new(10.0, 10.0),
                Pt::new(1.0, 10.0),
            ],
            paint::PixelCombine::Replace,
        );
        assert!(paint::selected_count(studio.pixel_sel.as_ref().unwrap()) > 20);

        apply_wand(&mut studio, Pt::new(4.0, 4.0), paint::PixelCombine::Replace);
        assert!(paint::selected_count(studio.pixel_sel.as_ref().unwrap()) > 1);
    }

    #[test]
    fn alt_subtracts_and_shift_adds_a_pixel_selection() {
        let mut studio = studio(16, 16);
        commit_marquee(
            &mut studio,
            Pt::new(0.0, 0.0),
            Pt::new(12.0, 12.0),
            false,
            paint::PixelCombine::Replace,
        );
        let before = paint::selected_count(studio.pixel_sel.as_ref().unwrap());
        commit_marquee(
            &mut studio,
            Pt::new(2.0, 2.0),
            Pt::new(8.0, 8.0),
            false,
            paint::PixelCombine::Subtract,
        );
        let mask = studio.pixel_sel.as_ref().unwrap();
        assert!(paint::selected_count(mask) < before);
        assert_eq!(mask[(1 * 16 + 1) as usize], 255);
        assert_eq!(mask[(4 * 16 + 4) as usize], 0);
        commit_marquee(
            &mut studio,
            Pt::new(12.0, 0.0),
            Pt::new(16.0, 8.0),
            false,
            paint::PixelCombine::Add,
        );
        let mask = studio.pixel_sel.as_ref().unwrap();
        assert_eq!(mask[(1 * 16 + 1) as usize], 255);
        assert_eq!(mask[(4 * 16 + 4) as usize], 0);
        assert_eq!(mask[(2 * 16 + 14) as usize], 255);
        commit_lasso(
            &mut studio,
            &[
                Pt::new(0.0, 0.0),
                Pt::new(6.0, 0.0),
                Pt::new(6.0, 6.0),
                Pt::new(0.0, 6.0),
            ],
            paint::PixelCombine::Subtract,
        );
        assert_eq!(studio.pixel_sel.as_ref().unwrap()[(1 * 16 + 1) as usize], 0);
    }

    #[test]
    fn brush_stays_inside_a_pixel_selection() {
        let mut studio = studio(16, 16);
        studio.tool = Tool::Brush;
        studio.brush.color = Rgba::rgb(255, 0, 0);
        studio.brush.size = 8.0;
        studio.brush.flow = 1.0;
        studio.brush.opacity = 1.0;
        commit_marquee(
            &mut studio,
            Pt::new(0.0, 0.0),
            Pt::new(6.0, 16.0),
            false,
            paint::PixelCombine::Replace,
        );
        start_brush(&mut studio, Pt::new(3.0, 8.0));
        brush_drag(&mut studio, Pt::new(12.0, 8.0));
        assert!(studio.end_pixel_stroke(false));
        let painted = rgba(&studio, 3, 8, false);
        assert!(painted[0] > 100, "selected side should paint {painted:?}");
        let outside = rgba(&studio, 12, 8, false);
        assert_eq!(outside, [160, 170, 180, 255], "unselected pixels stay put");
    }

    #[test]
    fn feathered_selection_limits_entire_brush_stroke_without_fading_earlier_dabs() {
        let mut studio = studio(32, 16);
        studio.tool = Tool::Brush;
        studio.brush.color = Rgba::rgb(255, 0, 0);
        studio.set_pixel_sel(Some(vec![128; 32 * 16]));
        start_brush(&mut studio, Pt::new(4.0, 8.0));
        brush_drag(&mut studio, Pt::new(14.0, 8.0));
        brush_drag(&mut studio, Pt::new(26.0, 8.0));
        studio.end_pixel_stroke(false);
        for x in [4, 10, 20] {
            let actual = rgba(&studio, x, 8, false);
            assert!(
                (200..=210).contains(&actual[0]),
                "half-selected red at {x}: {actual:?}"
            );
            assert!(
                (80..=90).contains(&actual[1]),
                "half-selected green at {x}: {actual:?}"
            );
        }
        studio.undo();
        assert_eq!(rgba(&studio, 4, 8, false), [160, 170, 180, 255]);
    }

    #[test]
    fn feathered_fill_and_eraser_blend_with_original_pixels() {
        let mut studio = studio(32, 16);
        studio.set_pixel_sel(Some(vec![128; 32 * 16]));
        studio.style.fill = Fill::Solid(Rgba::rgb(255, 0, 0));
        fill(&mut studio, Pt::new(8.0, 8.0));
        assert_eq!(rgba(&studio, 8, 8, false), [208, 85, 90, 255]);
        studio.undo();
        studio.tool = Tool::Eraser;
        start(&mut studio, Pt::new(4.0, 8.0));
        drag(&mut studio, Pt::new(14.0, 8.0));
        drag(&mut studio, Pt::new(26.0, 8.0));
        studio.end_pixel_stroke(false);
        for x in [4, 10, 20] {
            assert!((125..=130).contains(&rgba(&studio, x, 8, false)[3]));
        }
    }

    #[test]
    fn eyedropper_samples_placed_raster_and_sets_brush_color() {
        let mut studio = studio(8, 8);
        if let LayerKind::Raster { origin, size, .. } = &mut studio.doc.layers[0].kind {
            *origin = Pt::new(100.0, 40.0);
            *size = Pt::new(16.0, 16.0);
        }
        let pixels = studio.doc.layers[0].kind.pixels_mut().unwrap();
        pixels.data[(3 * 8 + 2) * 4..(3 * 8 + 2) * 4 + 4].copy_from_slice(&[12, 34, 56, 255]);
        pixels.touch();
        studio.persona = crate::tools::Persona::Pixel;
        studio.tool = Tool::Eyedropper;
        let transform = crate::compositor::layer_pixel_transform(&studio.doc.layers[0]);
        let mut point = tiny_skia::Point::from_xy(2.5, 3.5);
        transform.map_point(&mut point);
        studio.eyedrop(Pt::new(point.x, point.y));
        assert_eq!(studio.brush.color, Rgba::new(12, 34, 56, 255));
        assert!(studio.status.contains("#"));
    }

    #[test]
    fn smudge_live_stroke_has_one_undo() {
        let mut studio = studio(24, 8);
        let image = studio.doc.layers[0].kind.pixels_mut().unwrap();
        for x in 0..6usize {
            image.data[(4 * 24 + x) * 4..(4 * 24 + x) * 4 + 4].copy_from_slice(&[220, 30, 30, 255]);
        }
        image.touch();
        let original = image.data.clone();
        studio.tool = Tool::Smudge;
        studio.brush.size = 6.0;
        studio.brush.flow = 0.9;
        start_smudge(&mut studio, Pt::new(3.0, 4.0));
        smudge_drag(&mut studio, Pt::new(18.0, 4.0));
        assert!(studio.end_pixel_stroke(false));
        assert_eq!(studio.history.len(), 1);
        let smeared = rgba(&studio, 14, 4, false);
        assert!(smeared[0] > 80, "smudge should move paint {smeared:?}");
        studio.undo();
        assert_eq!(pixels(&studio, 0, false).unwrap().data, original);
    }
}
