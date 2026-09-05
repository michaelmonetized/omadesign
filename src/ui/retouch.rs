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

fn publish(studio: &mut Studio, layer: usize, mask: bool, buffer: &tiny_skia::Pixmap) {
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
    publish(studio, layer, mask, &buf);
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
    });
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
        publish(studio, layer, mask, &buf);
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
    paint::flood_fill(&mut buffer, point, color, studio.fill_tolerance);
    publish(studio, layer, mask, &buffer);
    finish(studio, layer, mask, before);
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
}
