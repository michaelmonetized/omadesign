//! Deterministic green-screen subject for native filter UI capture.
use super::*;
use crate::document::{Document, Layer};
pub(super) fn pixel_effects(s: &mut Studio) {
    let (w, h) = (720_u32, 480_u32);
    s.doc = Document::new("Green screen studio", w as f32, h as f32, 72.0);
    s.doc.layers = vec![Layer::raster("Product · green screen", w, h)];
    let pixels = s.doc.layers[0].kind.pixels_mut().unwrap();
    for y in 0..h {
        for x in 0..w {
            let dx = (x as f32 - 360.0) / 125.0;
            let dy = (y as f32 - 230.0) / 160.0;
            let distance = (dx * dx + dy * dy).sqrt();
            let coverage = ((1.0 - distance) * 90.0 + 0.5).clamp(0.0, 1.0);
            let lighting = (0.85 - dx * 0.18 - dy * 0.1).clamp(0.45, 1.0);
            let stripe = dy.abs() < 0.19;
            let object = if stripe {
                [240.0 * lighting, 235.0 * lighting, 220.0 * lighting]
            } else {
                [248.0 * lighting, 80.0 * lighting, 55.0 * lighting]
            };
            let green = [12.0, 220.0 + (y as f32 / h as f32) * 30.0, 18.0];
            let i = ((y * w + x) * 4) as usize;
            for ch in 0..3 {
                pixels.data[i + ch] = (object[ch] * coverage + green[ch] * (1.0 - coverage)) as u8;
            }
            pixels.data[i + 3] = 255;
        }
    }
    pixels.touch();
    s.show_welcome = false;
    s.persona = Persona::Pixel;
    s.tool = Tool::Wand;
    s.active_layer = Some(0);
    s.need_fit = true;
    s.status = "Pixel · Filters / Effects · live preview · one-step Undo".into();
}
