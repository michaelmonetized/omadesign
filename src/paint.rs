//! Pixel brushes: paint, erase, smudge, clone, fill, wand.

use crate::color::Rgba;
use crate::geom::Pt;
use tiny_skia::{
    FillRule, GradientStop, Paint, PathBuilder, Pixmap, PixmapPaint, Point, RadialGradient,
    SpreadMode, Transform,
};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Brush {
    pub size: f32,
    pub hardness: f32,
    pub opacity: f32,
    pub flow: f32,
    pub spacing: f32,
    pub color: Rgba,
}

impl Default for Brush {
    fn default() -> Self {
        Self {
            size: 24.0,
            hardness: 0.75,
            opacity: 1.0,
            flow: 0.85,
            spacing: 0.18,
            color: Rgba::rgb(0xE5, 0x48, 0x4D),
        }
    }
}

fn stamp_path(cx: f32, cy: f32, r: f32) -> Option<tiny_skia::Path> {
    let mut pb = PathBuilder::new();
    pb.push_circle(cx, cy, r.max(0.5));
    pb.finish()
}

pub fn stamp(pm: &mut Pixmap, pos: Pt, brush: &Brush, erase: bool) {
    let r = (brush.size * 0.5).max(0.5);
    let Some(path) = stamp_path(pos.x, pos.y, r) else {
        return;
    };
    let inner = (r * brush.hardness.clamp(0.0, 1.0)).max(0.1);
    let alpha = (brush.flow * brush.opacity).clamp(0.0, 1.0);
    let c = if erase {
        tiny_skia::Color::from_rgba(0.0, 0.0, 0.0, alpha).unwrap_or(tiny_skia::Color::TRANSPARENT)
    } else {
        let a = (brush.color.a as f32 / 255.0 * alpha * 255.0) as u8;
        tiny_skia::Color::from_rgba8(brush.color.r, brush.color.g, brush.color.b, a)
    };
    let mut c_edge = c;
    c_edge.set_alpha(c.alpha() * 0.9);
    let mut c0 = c;
    c0.set_alpha(0.0);
    let stops = vec![
        GradientStop::new(0.0, c),
        GradientStop::new((inner / r).clamp(0.05, 0.98), c_edge),
        GradientStop::new(1.0, c0),
    ];
    let shader = RadialGradient::new(
        Point::from_xy(pos.x, pos.y),
        0.0,
        Point::from_xy(pos.x, pos.y),
        r,
        stops,
        SpreadMode::Pad,
        Transform::identity(),
    );
    let mut paint = Paint {
        anti_alias: true,
        blend_mode: if erase {
            tiny_skia::BlendMode::DestinationOut
        } else {
            tiny_skia::BlendMode::SourceOver
        },
        ..Paint::default()
    };
    if let Some(shader) = shader {
        paint.shader = shader;
    } else {
        paint.set_color(c);
    }
    pm.fill_path(&path, &paint, FillRule::Winding, Transform::identity(), None);
}

pub fn stroke_to(pm: &mut Pixmap, from: Pt, to: Pt, brush: &Brush, erase: bool) {
    let dist = (to - from).length();
    let spacing = (brush.size * brush.spacing.max(0.05)).clamp(0.5, 64.0);
    let steps = (dist / spacing).ceil() as usize;
    for i in 0..=steps {
        let t = if steps == 0 {
            1.0
        } else {
            i as f32 / steps as f32
        };
        stamp(pm, from.lerp(to, t), brush, erase);
    }
}

pub fn blend_onto(dst: &mut Pixmap, src: &Pixmap, opacity: f32) {
    dst.draw_pixmap(
        0,
        0,
        src.as_ref(),
        &PixmapPaint {
            opacity: opacity.clamp(0.0, 1.0),
            ..Default::default()
        },
        Transform::identity(),
        None,
    );
}

fn sample(pm: &Pixmap, x: i32, y: i32) -> [u8; 4] {
    if x < 0 || y < 0 || x >= pm.width() as i32 || y >= pm.height() as i32 {
        return [0; 4];
    }
    let i = ((y as u32 * pm.width() + x as u32) * 4) as usize;
    let d = pm.data();
    [d[i], d[i + 1], d[i + 2], d[i + 3]]
}

fn set_px(pm: &mut Pixmap, x: i32, y: i32, px: [u8; 4]) {
    if x < 0 || y < 0 || x >= pm.width() as i32 || y >= pm.height() as i32 {
        return;
    }
    let i = ((y as u32 * pm.width() + x as u32) * 4) as usize;
    pm.data_mut()[i..i + 4].copy_from_slice(&px);
}

pub fn smudge(pm: &mut Pixmap, from: Pt, to: Pt, brush: &Brush) {
    let r = (brush.size * 0.5).max(1.0) as i32;
    let dir = to - from;
    if dir.length_sq() < 0.01 {
        return;
    }
    let strength = brush.flow.clamp(0.05, 1.0);
    let cx = to.x as i32;
    let cy = to.y as i32;
    let sx = from.x as i32;
    let sy = from.y as i32;
    for dy in -r..=r {
        for dx in -r..=r {
            if dx * dx + dy * dy > r * r {
                continue;
            }
            let src = sample(pm, sx + dx, sy + dy);
            let dst = sample(pm, cx + dx, cy + dy);
            let mix = |a: u8, b: u8| {
                (a as f32 * (1.0 - strength) + b as f32 * strength).round() as u8
            };
            set_px(
                pm,
                cx + dx,
                cy + dy,
                [
                    mix(dst[0], src[0]),
                    mix(dst[1], src[1]),
                    mix(dst[2], src[2]),
                    mix(dst[3], src[3]).max(dst[3]),
                ],
            );
        }
    }
}

pub fn clone_stamp(pm: &mut Pixmap, pos: Pt, source: Pt, brush: &Brush) {
    let r = (brush.size * 0.5).max(1.0) as i32;
    let cx = pos.x as i32;
    let cy = pos.y as i32;
    let ox = (source.x - pos.x) as i32;
    let oy = (source.y - pos.y) as i32;
    let strength = (brush.flow * brush.opacity).clamp(0.05, 1.0);
    for dy in -r..=r {
        for dx in -r..=r {
            let d2 = dx * dx + dy * dy;
            if d2 > r * r {
                continue;
            }
            let fall = 1.0 - (d2 as f32).sqrt() / (r as f32).max(1.0);
            let k = strength * (brush.hardness * 0.6 + fall * (1.0 - brush.hardness * 0.6));
            let src = sample(pm, cx + dx + ox, cy + dy + oy);
            let dst = sample(pm, cx + dx, cy + dy);
            let mix = |a: u8, b: u8| (a as f32 * (1.0 - k) + b as f32 * k).round() as u8;
            set_px(
                pm,
                cx + dx,
                cy + dy,
                [mix(dst[0], src[0]), mix(dst[1], src[1]), mix(dst[2], src[2]), mix(dst[3], src[3]).max(src[3])],
            );
        }
    }
}

fn color_dist(a: [u8; 4], b: [u8; 4]) -> f32 {
    let dr = a[0] as f32 - b[0] as f32;
    let dg = a[1] as f32 - b[1] as f32;
    let db = a[2] as f32 - b[2] as f32;
    (dr * dr + dg * dg + db * db).sqrt()
}

pub fn flood_fill(pm: &mut Pixmap, seed: Pt, color: Rgba, tolerance: f32) {
    let x = seed.x.round() as i32;
    let y = seed.y.round() as i32;
    if x < 0 || y < 0 || x >= pm.width() as i32 || y >= pm.height() as i32 {
        return;
    }
    let target = sample(pm, x, y);
    let fill = [color.r, color.g, color.b, color.a];
    if color_dist(target, fill) < 1.0 {
        return;
    }
    let mut stack = vec![(x, y)];
    let mut seen = vec![0u8; (pm.width() * pm.height()) as usize];
    let w = pm.width() as i32;
    let h = pm.height() as i32;
    while let Some((cx, cy)) = stack.pop() {
        if cx < 0 || cy < 0 || cx >= w || cy >= h {
            continue;
        }
        let idx = (cy * w + cx) as usize;
        if seen[idx] != 0 {
            continue;
        }
        seen[idx] = 1;
        let px = sample(pm, cx, cy);
        if color_dist(px, target) > tolerance {
            continue;
        }
        set_px(pm, cx, cy, fill);
        stack.push((cx + 1, cy));
        stack.push((cx - 1, cy));
        stack.push((cx, cy + 1));
        stack.push((cx, cy - 1));
    }
}

/// Returns a mask (255 = selected) from a wand click.
pub fn wand_mask(pm: &Pixmap, seed: Pt, tolerance: f32) -> Vec<u8> {
    let w = pm.width() as i32;
    let h = pm.height() as i32;
    let mut mask = vec![0u8; (w * h) as usize];
    let x = seed.x.round() as i32;
    let y = seed.y.round() as i32;
    if x < 0 || y < 0 || x >= w || y >= h {
        return mask;
    }
    let target = sample(pm, x, y);
    let mut stack = vec![(x, y)];
    while let Some((cx, cy)) = stack.pop() {
        if cx < 0 || cy < 0 || cx >= w || cy >= h {
            continue;
        }
        let idx = (cy * w + cx) as usize;
        if mask[idx] != 0 {
            continue;
        }
        let px = sample(pm, cx, cy);
        if color_dist(px, target) > tolerance {
            continue;
        }
        mask[idx] = 255;
        stack.push((cx + 1, cy));
        stack.push((cx - 1, cy));
        stack.push((cx, cy + 1));
        stack.push((cx, cy - 1));
    }
    mask
}

pub fn apply_selection_clip(dst: &mut Pixmap, src: &Pixmap, mask: &[u8]) {
    let n = (dst.width() * dst.height()) as usize;
    if mask.len() != n || src.data().len() != dst.data().len() {
        dst.data_mut().copy_from_slice(src.data());
        return;
    }
    for i in 0..n {
        if mask[i] == 0 {
            continue;
        }
        let o = i * 4;
        dst.data_mut()[o..o + 4].copy_from_slice(&src.data()[o..o + 4]);
    }
}

pub fn fill_rect_mask(w: u32, h: u32, x0: f32, y0: f32, x1: f32, y1: f32) -> Vec<u8> {
    let mut m = vec![0u8; w as usize * h as usize];
    let xa = x0.min(x1).max(0.0) as u32;
    let xb = x0.max(x1).min(w as f32) as u32;
    let ya = y0.min(y1).max(0.0) as u32;
    let yb = y0.max(y1).min(h as f32) as u32;
    for y in ya..yb {
        for x in xa..xb {
            m[(y * w + x) as usize] = 255;
        }
    }
    m
}

pub fn fill_ellipse_mask(w: u32, h: u32, x0: f32, y0: f32, x1: f32, y1: f32) -> Vec<u8> {
    let mut m = vec![0u8; w as usize * h as usize];
    let cx = (x0 + x1) * 0.5;
    let cy = (y0 + y1) * 0.5;
    let rx = ((x1 - x0).abs() * 0.5).max(0.5);
    let ry = ((y1 - y0).abs() * 0.5).max(0.5);
    for y in 0..h {
        for x in 0..w {
            let nx = (x as f32 + 0.5 - cx) / rx;
            let ny = (y as f32 + 0.5 - cy) / ry;
            if nx * nx + ny * ny <= 1.0 {
                m[(y * w + x) as usize] = 255;
            }
        }
    }
    m
}

pub fn fill_poly_mask(w: u32, h: u32, pts: &[Pt]) -> Vec<u8> {
    let mut m = vec![0u8; w as usize * h as usize];
    if pts.len() < 3 {
        return m;
    }
    for y in 0..h {
        for x in 0..w {
            if crate::geom::point_in_poly(Pt::new(x as f32 + 0.5, y as f32 + 0.5), pts) {
                m[(y * w + x) as usize] = 255;
            }
        }
    }
    m
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stamp_paints_center() {
        let mut pm = Pixmap::new(64, 64).unwrap();
        stamp(
            &mut pm,
            Pt::new(32.0, 32.0),
            &Brush {
                size: 16.0,
                hardness: 1.0,
                opacity: 1.0,
                flow: 1.0,
                spacing: 0.2,
                color: Rgba::rgb(255, 0, 0),
            },
            false,
        );
        let i = ((32 * 64 + 32) * 4) as usize;
        assert!(pm.data()[i + 3] > 200, "alpha {}", pm.data()[i + 3]);
        assert_eq!(pm.data()[3], 0);
    }

    #[test]
    fn flood_fills_connected() {
        let mut pm = Pixmap::new(8, 8).unwrap();
        flood_fill(&mut pm, Pt::new(0.0, 0.0), Rgba::rgb(0, 255, 0), 8.0);
        assert_eq!(pm.data()[1], 255);
        assert_eq!(pm.data()[(63 * 4) + 1], 255);
    }

    #[test]
    fn wand_selects_whole_empty() {
        let pm = Pixmap::new(4, 4).unwrap();
        let m = wand_mask(&pm, Pt::new(1.0, 1.0), 4.0);
        assert_eq!(m.iter().filter(|&&v| v == 255).count(), 16);
    }
}
