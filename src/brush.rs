use eframe::egui::{Color32, Pos2};
use tiny_skia::{FillRule, Paint, PathBuilder, Pixmap, PixmapPaint, Transform};

#[derive(Clone)]
pub struct BrushSettings {
    pub size: f32,
    pub color: Color32,
    pub flow: f32,
}

impl Default for BrushSettings {
    fn default() -> Self {
        Self {
            size: 16.0,
            color: Color32::from_rgb(0xE5, 0x48, 0x4D),
            flow: 0.9,
        }
    }
}

pub struct StrokeBuffer {
    pub pixmap: Pixmap,
    pub version: u64,
}

impl StrokeBuffer {
    pub fn new(w: u32, h: u32) -> Option<Self> {
        Some(Self {
            pixmap: Pixmap::new(w, h)?,
            version: 0,
        })
    }

    pub fn touch(&mut self) {
        self.version += 1;
    }
}

fn circle_path(cx: f32, cy: f32, r: f32) -> Option<tiny_skia::Path> {
    let mut pb = PathBuilder::new();
    let n = 24;
    for i in 0..=n {
        let a = std::f32::consts::TAU * i as f32 / n as f32;
        let x = cx + r * a.cos();
        let y = cy + r * a.sin();
        if i == 0 {
            pb.move_to(x, y);
        } else {
            pb.line_to(x, y);
        }
    }
    pb.close();
    pb.finish()
}

pub fn stamp(buffer: &mut Pixmap, pos: Pos2, settings: &BrushSettings) {
    let r = (settings.size * 0.5).max(0.5);
    if let Some(path) = circle_path(pos.x, pos.y, r) {
        let mut paint = Paint {
            anti_alias: true,
            ..Paint::default()
        };
        paint.set_color(tiny_skia::Color::from_rgba8(
            settings.color.r(),
            settings.color.g(),
            settings.color.b(),
            settings.color.a(),
        ));
        buffer.fill_path(
            &path,
            &paint,
            FillRule::Winding,
            Transform::identity(),
            None,
        );
    }
}

fn lerp(a: Pos2, b: Pos2, t: f32) -> Pos2 {
    eframe::egui::pos2(a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t)
}

pub fn stroke_segment(buffer: &mut Pixmap, from: Pos2, to: Pos2, settings: &BrushSettings) {
    let dist = (to - from).length();
    let spacing = (settings.size * 0.25).clamp(1.0, 32.0);
    let steps = (dist / spacing).ceil() as usize;
    for i in 0..=steps {
        let t = if steps == 0 {
            1.0
        } else {
            i as f32 / steps as f32
        };
        stamp(buffer, lerp(from, to, t), settings);
    }
}

pub fn blend_onto(dst: &mut Pixmap, src: &Pixmap, opacity: f32) {
    let paint = PixmapPaint {
        opacity: opacity.clamp(0.0, 1.0),
        ..Default::default()
    };
    dst.draw_pixmap(0, 0, src.as_ref(), &paint, Transform::identity(), None);
}

#[cfg(test)]
mod tests {
    use super::*;
    use eframe::egui::pos2;

    #[test]
    fn stamp_paints_inside_radius_only() {
        let mut pm = Pixmap::new(32, 32).unwrap();
        stamp(
            &mut pm,
            pos2(16.0, 16.0),
            &BrushSettings {
                size: 10.0,
                color: Color32::from_rgba_unmultiplied(255, 0, 0, 255),
                flow: 1.0,
            },
        );
        let center = &pm.data()[(16 * 32 + 16) * 4..(16 * 32 + 16) * 4 + 4];
        assert_eq!(center[3], 255);
        let corner = &pm.data()[0..4];
        assert_eq!(corner[3], 0);
    }

    #[test]
    fn blend_onto_applies_opacity() {
        let mut base = Pixmap::new(1, 1).unwrap();
        let mut overlay = Pixmap::new(1, 1).unwrap();
        let white = tiny_skia::Color::from_rgba8(255, 255, 255, 255);
        let mut paint = Paint::default();
        paint.set_color(white);
        overlay.fill_path(
            &PathBuilder::from_rect(tiny_skia::Rect::from_xywh(0.0, 0.0, 1.0, 1.0).unwrap()),
            &paint,
            FillRule::Winding,
            Transform::identity(),
            None,
        );
        blend_onto(&mut base, &overlay, 0.5);
        let alpha = base.data()[3];
        assert!(alpha.abs_diff(128) <= 2, "alpha {alpha}");
    }
}
