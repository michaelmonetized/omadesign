//! Pixel brushes: paint, erase, smudge, clone, heal, fill, wand.

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
    pm.fill_path(
        &path,
        &paint,
        FillRule::Winding,
        Transform::identity(),
        None,
    );
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

fn sample_at(pm: &Pixmap, x: f32, y: f32) -> [u8; 4] {
    let x0 = x.floor() as i32;
    let y0 = y.floor() as i32;
    let fx = (x - x0 as f32).clamp(0.0, 1.0);
    let fy = (y - y0 as f32).clamp(0.0, 1.0);
    let p00 = sample(pm, x0, y0);
    let p10 = sample(pm, x0 + 1, y0);
    let p01 = sample(pm, x0, y0 + 1);
    let p11 = sample(pm, x0 + 1, y0 + 1);
    let mix = |a: u8, b: u8, t: f32| a as f32 + (b as f32 - a as f32) * t;
    let a = [
        mix(p00[0], p10[0], fx),
        mix(p00[1], p10[1], fx),
        mix(p00[2], p10[2], fx),
        mix(p00[3], p10[3], fx),
    ];
    let b = [
        mix(p01[0], p11[0], fx),
        mix(p01[1], p11[1], fx),
        mix(p01[2], p11[2], fx),
        mix(p01[3], p11[3], fx),
    ];
    [
        (a[0] + (b[0] - a[0]) * fy).round() as u8,
        (a[1] + (b[1] - a[1]) * fy).round() as u8,
        (a[2] + (b[2] - a[2]) * fy).round() as u8,
        (a[3] + (b[3] - a[3]) * fy).round() as u8,
    ]
}

fn set_px(pm: &mut Pixmap, x: i32, y: i32, px: [u8; 4]) {
    if x < 0 || y < 0 || x >= pm.width() as i32 || y >= pm.height() as i32 {
        return;
    }
    let i = ((y as u32 * pm.width() + x as u32) * 4) as usize;
    pm.data_mut()[i..i + 4].copy_from_slice(&px);
}

fn dab_spacing(brush: &Brush) -> f32 {
    (brush.size * brush.spacing.max(0.05)).clamp(0.5, 64.0)
}

/// Pull pixels from behind the stroke so a drag smears instead of stamping a disk.
pub fn smudge(pm: &mut Pixmap, from: Pt, to: Pt, brush: &Brush) {
    let delta = to - from;
    let dist = delta.length();
    if dist < 0.01 {
        return;
    }
    let dir = delta * (1.0 / dist);
    let radius = (brush.size * 0.5).max(1.0);
    let strength = brush.flow.clamp(0.05, 1.0);
    let hardness = brush.hardness.clamp(0.0, 0.98);
    let pull = radius * (0.25 + 0.55 * strength);
    let extent = radius.ceil() as i32;
    let mut writes = Vec::with_capacity(((extent * 2 + 1) as usize).pow(2));
    for dy in -extent..=extent {
        for dx in -extent..=extent {
            let d = ((dx * dx + dy * dy) as f32).sqrt() / radius;
            if d >= 1.0 {
                continue;
            }
            let edge = ((1.0 - d) / (1.0 - hardness)).clamp(0.0, 1.0);
            let amount = strength * edge * edge * (3.0 - 2.0 * edge);
            if amount < 0.002 {
                continue;
            }
            let x = to.x + dx as f32;
            let y = to.y + dy as f32;
            let ix = x.round() as i32;
            let iy = y.round() as i32;
            let src = sample_at(pm, x - dir.x * pull, y - dir.y * pull);
            let dst = sample(pm, ix, iy);
            let mix = |a: u8, b: u8| (a as f32 * (1.0 - amount) + b as f32 * amount).round() as u8;
            writes.push((
                ix,
                iy,
                [
                    mix(dst[0], src[0]),
                    mix(dst[1], src[1]),
                    mix(dst[2], src[2]),
                    mix(dst[3], src[3]).max(dst[3]),
                ],
            ));
        }
    }
    for (x, y, px) in writes {
        set_px(pm, x, y, px);
    }
}

pub fn smudge_stroke(pm: &mut Pixmap, from: Pt, to: Pt, brush: &Brush) {
    let dist = (to - from).length();
    if dist < 0.01 {
        return;
    }
    let spacing = dab_spacing(brush);
    let steps = (dist / spacing).ceil().max(1.0) as usize;
    for step in 1..=steps {
        let a = from.lerp(to, (step - 1) as f32 / steps as f32);
        let b = from.lerp(to, step as f32 / steps as f32);
        smudge(pm, a, b, brush);
    }
}

pub fn clone_stamp(pm: &mut Pixmap, pos: Pt, source: Pt, brush: &Brush) {
    let r = (brush.size * 0.5).max(1.0) as i32;
    let cx = pos.x;
    let cy = pos.y;
    let ox = source.x - pos.x;
    let oy = source.y - pos.y;
    let strength = (brush.flow * brush.opacity).clamp(0.05, 1.0);
    let hardness = brush.hardness.clamp(0.0, 0.98);
    for dy in -r..=r {
        for dx in -r..=r {
            let d = ((dx * dx + dy * dy) as f32).sqrt() / (r as f32).max(1.0);
            if d >= 1.0 {
                continue;
            }
            let edge = ((1.0 - d) / (1.0 - hardness)).clamp(0.0, 1.0);
            let k = strength * edge * edge * (3.0 - 2.0 * edge);
            let src = sample_at(pm, cx + dx as f32 + ox, cy + dy as f32 + oy);
            let dst = sample(
                pm,
                (cx + dx as f32).round() as i32,
                (cy + dy as f32).round() as i32,
            );
            let mix = |a: u8, b: u8| (a as f32 * (1.0 - k) + b as f32 * k).round() as u8;
            set_px(
                pm,
                (cx + dx as f32).round() as i32,
                (cy + dy as f32).round() as i32,
                [
                    mix(dst[0], src[0]),
                    mix(dst[1], src[1]),
                    mix(dst[2], src[2]),
                    mix(dst[3], src[3]).max(src[3]),
                ],
            );
        }
    }
}

pub fn clone_stroke(pm: &mut Pixmap, from: Pt, to: Pt, source_from: Pt, brush: &Brush) {
    let offset = source_from - from;
    let dist = (to - from).length();
    let spacing = dab_spacing(brush);
    let steps = (dist / spacing).ceil().max(1.0) as usize;
    for step in 1..=steps {
        let t = step as f32 / steps as f32;
        let pos = from.lerp(to, t);
        clone_stamp(pm, pos, pos + offset, brush);
    }
}

/// Restore unselected pixels from the stroke-start image (both premul pixmaps).
pub fn restrict_pixmap(dst: &mut Pixmap, original: &Pixmap, mask: &[u8]) {
    if dst.width() != original.width() || dst.height() != original.height() {
        return;
    }
    let n = (dst.width() * dst.height()) as usize;
    if mask.len() != n {
        return;
    }
    let src = original.data();
    let out = dst.data_mut();
    for i in 0..n {
        if mask[i] == 0 {
            let o = i * 4;
            out[o..o + 4].copy_from_slice(&src[o..o + 4]);
        }
    }
}

/// Zero overlay pixels outside the selection.
pub fn clip_overlay(pm: &mut Pixmap, mask: &[u8]) {
    let n = (pm.width() * pm.height()) as usize;
    if mask.len() != n {
        return;
    }
    for (i, pixel) in pm.pixels_mut().iter_mut().enumerate() {
        if mask[i] == 0 {
            *pixel = tiny_skia::ColorU8::from_rgba(0, 0, 0, 0).premultiply();
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PixelCombine {
    Replace,
    Add,
    Subtract,
}

pub fn combine_masks(base: Option<&[u8]>, next: &[u8], op: PixelCombine) -> Vec<u8> {
    let Some(base) = base.filter(|base| base.len() == next.len() && op != PixelCombine::Replace)
    else {
        return match op {
            PixelCombine::Subtract => vec![0; next.len()],
            _ => next.to_vec(),
        };
    };
    match op {
        PixelCombine::Add => base.iter().zip(next).map(|(a, b)| (*a).max(*b)).collect(),
        PixelCombine::Subtract => base
            .iter()
            .zip(next)
            .map(|(a, b)| if *b > 0 { 0 } else { *a })
            .collect(),
        PixelCombine::Replace => next.to_vec(),
    }
}

pub fn selected_count(mask: &[u8]) -> usize {
    mask.iter().filter(|value| **value > 0).count()
}

pub fn selection_bounds(mask: &[u8], w: u32, h: u32) -> Option<(u32, u32, u32, u32)> {
    if mask.len() != w as usize * h as usize {
        return None;
    }
    let mut min_x = w;
    let mut min_y = h;
    let mut max_x = 0u32;
    let mut max_y = 0u32;
    let mut any = false;
    for y in 0..h {
        for x in 0..w {
            if mask[(y * w + x) as usize] != 0 {
                any = true;
                min_x = min_x.min(x);
                min_y = min_y.min(y);
                max_x = max_x.max(x + 1);
                max_y = max_y.max(y + 1);
            }
        }
    }
    any.then_some((min_x, min_y, max_x, max_y))
}

pub fn selection_overlay_rgba(mask: &[u8], color: [u8; 3], alpha: u8) -> Vec<u8> {
    let mut out = vec![0u8; mask.len() * 4];
    for (i, &m) in mask.iter().enumerate() {
        if m == 0 {
            continue;
        }
        let a = (u16::from(alpha) * u16::from(m) / 255) as u8;
        let o = i * 4;
        out[o] = color[0];
        out[o + 1] = color[1];
        out[o + 2] = color[2];
        out[o + 3] = a;
    }
    out
}

/// Transfer fine source texture onto the destination's surrounding colour and
/// shading. Read from the stroke-start image so dabs never sample their repairs.
pub fn heal_stamp(dst: &mut Pixmap, source: &Pixmap, pos: Pt, source_pos: Pt, brush: &Brush) {
    let strength = (brush.flow * brush.opacity).clamp(0.0, 1.0);
    if strength == 0.0 {
        return;
    }
    let radius = (brush.size * 0.5).clamp(1.0, 512.0);
    let extent = radius.ceil() as i32;
    let centre = (pos.x.round() as i32, pos.y.round() as i32);
    let sample_centre = (source_pos.x.round() as i32, source_pos.y.round() as i32);
    let inside = |image: &Pixmap, x: i32, y: i32| {
        x >= 0 && y >= 0 && x < image.width() as i32 && y < image.height() as i32
    };
    let basis = |dx: i32, dy: i32| {
        let x = dx as f64 / radius as f64;
        let y = dy as f64 / radius as f64;
        [1.0, x, y, x * x, x * y, y * y]
    };
    // Fit a gently curved colour field to the clean outer ring. A single mean
    // transfers the donor's lighting as a visible halo on shaded destinations.
    let mut equations = [[0.0_f64; 9]; 6];
    let stride = (extent / 24).max(1) as usize;
    for dy in (-extent..=extent).step_by(stride) {
        for dx in (-extent..=extent).step_by(stride) {
            let distance = ((dx * dx + dy * dy) as f32).sqrt() / radius;
            if !(0.65..=1.0).contains(&distance) || !inside(source, centre.0 + dx, centre.1 + dy) {
                continue;
            }
            let target = sample(source, centre.0 + dx, centre.1 + dy);
            let alpha = target[3] as f64 / 255.0;
            let terms = basis(dx, dy);
            for i in 0..6 {
                for j in 0..6 {
                    equations[i][j] += terms[i] * terms[j] * alpha;
                }
                for channel in 0..3 {
                    equations[i][6 + channel] += terms[i] * target[channel] as f64 / 255.0;
                }
            }
        }
    }
    if equations[0][0] < 1.0 {
        return;
    }
    let colour_field = heal_colour_field(equations);

    // An alpha-weighted summed-area table separates donor texture from broad
    // lighting in constant time per pixel, including at image/transparency edges.
    let blur = (radius * 0.25).ceil().max(2.0) as i32;
    let left = (sample_centre.0 - extent - blur).max(0);
    let top = (sample_centre.1 - extent - blur).max(0);
    let right = (sample_centre.0 + extent + blur + 1).min(source.width() as i32);
    let bottom = (sample_centre.1 + extent + blur + 1).min(source.height() as i32);
    if right <= left || bottom <= top {
        return;
    }
    let width = (right - left + 1) as usize;
    let height = (bottom - top + 1) as usize;
    let mut sums = vec![[0.0_f64; 4]; width * height];
    for y in 1..height {
        let mut row = [0.0; 4];
        for x in 1..width {
            let pixel = sample(source, left + x as i32 - 1, top + y as i32 - 1);
            for channel in 0..4 {
                row[channel] += pixel[channel] as f64;
                sums[y * width + x][channel] = sums[(y - 1) * width + x][channel] + row[channel];
            }
        }
    }
    let hardness = brush.hardness.clamp(0.0, 0.98);
    for dy in -extent..=extent {
        for dx in -extent..=extent {
            let distance = ((dx * dx + dy * dy) as f32).sqrt() / radius;
            let (x, y) = (centre.0 + dx, centre.1 + dy);
            let (sx, sy) = (sample_centre.0 + dx, sample_centre.1 + dy);
            if distance >= 1.0 || !inside(dst, x, y) || !inside(source, sx, sy) {
                continue;
            }
            let donor = sample(source, sx, sy);
            let target = sample(dst, x, y);
            if donor[3] == 0 || target[3] == 0 {
                continue;
            }
            let x0 = ((sx - blur).max(left) - left) as usize;
            let y0 = ((sy - blur).max(top) - top) as usize;
            let x1 = ((sx + blur + 1).min(right) - left) as usize;
            let y1 = ((sy + blur + 1).min(bottom) - top) as usize;
            let local: [f64; 4] = std::array::from_fn(|channel| {
                sums[y1 * width + x1][channel] + sums[y0 * width + x0][channel]
                    - sums[y0 * width + x1][channel]
                    - sums[y1 * width + x0][channel]
            });
            let edge = ((1.0 - distance) / (1.0 - hardness)).clamp(0.0, 1.0);
            let amount =
                (strength * edge * edge * (3.0 - 2.0 * edge) * donor[3] as f32 / 255.0) as f64;
            let terms = basis(dx, dy);
            let alpha = target[3] as f64;
            let mut repaired = target;
            for channel in 0..3 {
                let shading: f64 = colour_field
                    .iter()
                    .zip(terms)
                    .map(|(coefficient, term)| coefficient[channel] * term)
                    .sum();
                let texture =
                    donor[channel] as f64 / donor[3] as f64 - local[channel] / local[3].max(1.0);
                let corrected = (shading + texture).clamp(0.0, 1.0) * alpha;
                repaired[channel] = (target[channel] as f64 * (1.0 - amount) + corrected * amount)
                    .round()
                    .clamp(0.0, alpha) as u8;
            }
            set_px(dst, x, y, repaired);
        }
    }
}

fn heal_colour_field(mut equations: [[f64; 9]; 6]) -> [[f64; 3]; 6] {
    let mut fallback = [[0.0; 3]; 6];
    fallback[0] = std::array::from_fn(|channel| equations[0][6 + channel] / equations[0][0]);
    // Solve all three colour channels together; small or clipped brushes can
    // lack enough independent samples, in which case the mean is safer.
    for pivot in 0..6 {
        let best = (pivot..6)
            .max_by(|&a, &b| {
                equations[a][pivot]
                    .abs()
                    .total_cmp(&equations[b][pivot].abs())
            })
            .unwrap();
        if equations[best][pivot].abs() < 1e-6 {
            return fallback;
        }
        equations.swap(pivot, best);
        let scale = equations[pivot][pivot];
        for value in &mut equations[pivot][pivot..] {
            *value /= scale;
        }
        for row in 0..6 {
            if row == pivot {
                continue;
            }
            let scale = equations[row][pivot];
            for col in pivot..9 {
                equations[row][col] -= scale * equations[pivot][col];
            }
        }
    }
    std::array::from_fn(|term| std::array::from_fn(|channel| equations[term][6 + channel]))
}

pub fn heal_stroke(
    dst: &mut Pixmap,
    source: &Pixmap,
    from: Pt,
    to: Pt,
    source_offset: Pt,
    brush: &Brush,
) {
    let spacing = (brush.size * brush.spacing.max(0.05)).clamp(0.5, 64.0);
    let steps = ((to - from).length() / spacing).ceil().max(1.0) as usize;
    for step in 1..=steps {
        let at = from.lerp(to, step as f32 / steps as f32);
        heal_stamp(dst, source, at, at + source_offset, brush);
    }
}

fn color_dist(a: [u8; 4], b: [u8; 4]) -> f32 {
    let dr = a[0] as f32 - b[0] as f32;
    let dg = a[1] as f32 - b[1] as f32;
    let db = a[2] as f32 - b[2] as f32;
    let da = a[3] as f32 - b[3] as f32;
    (dr * dr + dg * dg + db * db + da * da).sqrt()
}

pub fn flood_fill(pm: &mut Pixmap, seed: Pt, color: Rgba, tolerance: f32) {
    flood_fill_clipped(pm, seed, color, tolerance, None);
}

pub fn flood_fill_clipped(
    pm: &mut Pixmap,
    seed: Pt,
    color: Rgba,
    tolerance: f32,
    clip: Option<&[u8]>,
) {
    let mask = wand_mask_clipped(pm, seed, tolerance, clip);
    let fill = tiny_skia::ColorU8::from_rgba(color.r, color.g, color.b, color.a).premultiply();
    for (pixel, selected) in pm.pixels_mut().iter_mut().zip(mask) {
        if selected != 0 {
            *pixel = fill;
        }
    }
}

/// Returns a mask (255 = selected) from a wand click.
pub fn wand_mask(pm: &Pixmap, seed: Pt, tolerance: f32) -> Vec<u8> {
    wand_mask_clipped(pm, seed, tolerance, None)
}

pub fn wand_mask_clipped(pm: &Pixmap, seed: Pt, tolerance: f32, clip: Option<&[u8]>) -> Vec<u8> {
    let w = pm.width() as i32;
    let h = pm.height() as i32;
    let mut mask = vec![0u8; (w * h) as usize];
    let x = seed.x.round() as i32;
    let y = seed.y.round() as i32;
    if x < 0 || y < 0 || x >= w || y >= h {
        return mask;
    }
    let seed_idx = (y * w + x) as usize;
    if clip.is_some_and(|clip| clip.get(seed_idx).copied().unwrap_or(0) == 0) {
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
        if clip.is_some_and(|clip| clip.get(idx).copied().unwrap_or(0) == 0) {
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

pub fn shift_mask(src: &[u8], w: u32, h: u32, dx: i32, dy: i32) -> Vec<u8> {
    let mut out = vec![0u8; src.len()];
    if src.len() != w as usize * h as usize {
        return out;
    }
    for y in 0..h as i32 {
        for x in 0..w as i32 {
            let sx = x - dx;
            let sy = y - dy;
            if sx >= 0 && sy >= 0 && (sx as u32) < w && (sy as u32) < h {
                out[(y as u32 * w + x as u32) as usize] =
                    src[(sy as u32 * w + sx as u32) as usize];
            }
        }
    }
    out
}

pub fn scale_mask(src: &[u8], w: u32, h: u32, sx: f32, sy: f32) -> Vec<u8> {
    let mut out = vec![0u8; src.len()];
    if src.len() != w as usize * h as usize || sx.abs() < 0.05 || sy.abs() < 0.05 {
        return src.to_vec();
    }
    let cx = (w as f32 - 1.0) * 0.5;
    let cy = (h as f32 - 1.0) * 0.5;
    for y in 0..h {
        for x in 0..w {
            let ix = (cx + (x as f32 - cx) / sx).round() as i32;
            let iy = (cy + (y as f32 - cy) / sy).round() as i32;
            if ix >= 0 && iy >= 0 && (ix as u32) < w && (iy as u32) < h {
                out[(y * w + x) as usize] = src[(iy as u32 * w + ix as u32) as usize];
            }
        }
    }
    out
}

pub fn feather_mask(src: &[u8], w: u32, h: u32, radius: u32) -> Vec<u8> {
    if radius == 0 || src.len() != w as usize * h as usize {
        return src.to_vec();
    }
    let mut cur = src.to_vec();
    let mut next = vec![0u8; cur.len()];
    let r = radius as i32;
    for y in 0..h as i32 {
        for x in 0..w as i32 {
            let mut sum = 0u32;
            let mut n = 0u32;
            for k in -r..=r {
                let xx = x + k;
                if xx >= 0 && (xx as u32) < w {
                    sum += cur[(y as u32 * w + xx as u32) as usize] as u32;
                    n += 1;
                }
            }
            next[(y as u32 * w + x as u32) as usize] = (sum / n.max(1)) as u8;
        }
    }
    cur.copy_from_slice(&next);
    for x in 0..w as i32 {
        for y in 0..h as i32 {
            let mut sum = 0u32;
            let mut n = 0u32;
            for k in -r..=r {
                let yy = y + k;
                if yy >= 0 && (yy as u32) < h {
                    sum += cur[(yy as u32 * w + x as u32) as usize] as u32;
                    n += 1;
                }
            }
            next[(y as u32 * w + x as u32) as usize] = (sum / n.max(1)) as u8;
        }
    }
    next
}

pub fn shear_mask(src: &[u8], w: u32, h: u32, kx: f32) -> Vec<u8> {
    let mut out = vec![0u8; src.len()];
    if src.len() != w as usize * h as usize {
        return out;
    }
    let cy = (h as f32 - 1.0) * 0.5;
    for y in 0..h {
        for x in 0..w {
            let sx = (x as f32 - kx * (y as f32 - cy)).round() as i32;
            if sx >= 0 && (sx as u32) < w {
                out[(y * w + x) as usize] = src[(y * w + sx as u32) as usize];
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn healing_keeps_source_texture_but_matches_the_destination_colour() {
        let mut source = Pixmap::new(32, 16).unwrap();
        for y in 0..16 {
            for x in 0..32 {
                set_px(
                    &mut source,
                    x,
                    y,
                    if x < 16 {
                        [60, 80, 100, 255]
                    } else {
                        [160, 170, 180, 255]
                    },
                );
            }
        }
        set_px(&mut source, 7, 8, [80, 100, 120, 255]);
        set_px(&mut source, 8, 8, [40, 60, 80, 255]);
        set_px(&mut source, 24, 8, [20, 20, 20, 255]);
        let mut repaired = source.clone();
        let brush = Brush {
            size: 10.0,
            hardness: 0.9,
            flow: 1.0,
            opacity: 1.0,
            ..Default::default()
        };
        heal_stamp(
            &mut repaired,
            &source,
            Pt::new(24.0, 8.0),
            Pt::new(7.0, 8.0),
            &brush,
        );
        assert_eq!(sample(&repaired, 24, 8), [180, 190, 200, 255]);
        assert_eq!(sample(&repaired, 25, 8), [140, 150, 160, 255]);
        assert_eq!(sample(&repaired, 30, 8), sample(&source, 30, 8));
        assert_eq!(sample(&source, 24, 8), [20, 20, 20, 255]);
        let mut clone = source.clone();
        clone_stamp(&mut clone, Pt::new(24.0, 8.0), Pt::new(7.0, 8.0), &brush);
        assert_ne!(sample(&clone, 24, 8), sample(&repaired, 24, 8));
    }

    #[test]
    fn healing_repairs_shaded_colour_without_a_donor_halo() {
        // A clean reference makes both blemish removal and the surrounding seam
        // measurable. The donor deliberately crosses a brighter, curved highlight.
        let (_, image) = crate::photo::sample_photo(1);
        let image = image.downscaled(480);
        let clean = crate::color::rgba_to_pixmap(image.w, image.h, &image.data).unwrap();
        let mut damaged = clean.clone();
        let centre = Pt::new(300.0, 190.0);
        stamp(
            &mut damaged,
            centre,
            &Brush {
                size: 38.0,
                hardness: 0.7,
                flow: 1.0,
                opacity: 1.0,
                color: Rgba::from_hex(0x342225),
                ..Default::default()
            },
            false,
        );
        let mut repaired = damaged.clone();
        heal_stamp(
            &mut repaired,
            &damaged,
            centre,
            Pt::new(220.0, 160.0),
            &Brush {
                size: 84.0,
                hardness: 0.55,
                flow: 1.0,
                opacity: 1.0,
                ..Default::default()
            },
        );
        let mean_error = |image: &Pixmap, inner: i32, outer: i32| {
            let mut error = 0.0;
            let mut count = 0;
            for dy in -outer..=outer {
                for dx in -outer..=outer {
                    let distance = dx * dx + dy * dy;
                    if distance < inner * inner || distance > outer * outer {
                        continue;
                    }
                    let expected = sample(&clean, 300 + dx, 190 + dy);
                    let actual = sample(image, 300 + dx, 190 + dy);
                    for channel in 0..3 {
                        error += actual[channel].abs_diff(expected[channel]) as f32;
                        count += 1;
                    }
                }
            }
            error / count as f32
        };
        let repair_error = mean_error(&repaired, 0, 19);
        let seam_error = mean_error(&repaired, 20, 42);
        assert!(repair_error < 4.0, "repair error {repair_error}");
        assert!(repair_error < mean_error(&damaged, 0, 19) * 0.12);
        assert!(seam_error < 2.0, "seam error {seam_error}");
        assert_eq!(sample(&repaired, 350, 190), sample(&clean, 350, 190));
    }

    #[test]
    fn healing_preserves_transparency_and_respects_zero_flow() {
        let mut source = Pixmap::new(24, 12).unwrap();
        source.fill(tiny_skia::Color::from_rgba8(120, 100, 80, 255));
        set_px(&mut source, 17, 6, [10, 10, 10, 128]);
        set_px(&mut source, 18, 6, [0; 4]);
        let mut repaired = source.clone();
        let mut brush = Brush {
            size: 8.0,
            hardness: 0.9,
            flow: 0.0,
            opacity: 1.0,
            ..Default::default()
        };
        heal_stamp(
            &mut repaired,
            &source,
            Pt::new(17.0, 6.0),
            Pt::new(5.0, 6.0),
            &brush,
        );
        assert_eq!(repaired, source);
        brush.flow = 1.0;
        heal_stamp(
            &mut repaired,
            &source,
            Pt::new(17.0, 6.0),
            Pt::new(5.0, 6.0),
            &brush,
        );
        assert_eq!(sample(&repaired, 17, 6), [60, 50, 40, 128]);
        assert_eq!(sample(&repaired, 18, 6), [0; 4]);
    }

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
    fn flood_fill_uses_connected_region_and_premultiplies_alpha() {
        let mut pm = Pixmap::new(3, 1).unwrap();
        pm.data_mut()[4..8].copy_from_slice(&[0, 0, 0, 255]);
        let mask = wand_mask(&pm, Pt::ZERO, 8.0);
        assert_eq!(mask, vec![255, 0, 0]);
        flood_fill(&mut pm, Pt::ZERO, Rgba::new(0, 255, 0, 128), 8.0);
        assert_eq!(pm.data(), &[0, 128, 0, 128, 0, 0, 0, 255, 0, 0, 0, 0]);
        flood_fill(&mut pm, Pt::new(2.0, 0.0), Rgba::BLACK, 8.0);
        assert_eq!(&pm.data()[8..], &[0, 0, 0, 255]);
    }

    #[test]
    fn smudge_pulls_colour_along_the_stroke_instead_of_stamping_a_disk() {
        let mut pm = Pixmap::new(24, 8).unwrap();
        for x in 0..6 {
            set_px(&mut pm, x, 4, [220, 30, 30, 255]);
        }
        let brush = Brush {
            size: 6.0,
            hardness: 0.85,
            flow: 0.9,
            opacity: 1.0,
            spacing: 0.2,
            ..Default::default()
        };
        smudge_stroke(&mut pm, Pt::new(3.0, 4.0), Pt::new(18.0, 4.0), &brush);
        let smeared = sample(&pm, 14, 4);
        assert!(
            smeared[0] > 80,
            "smudge should carry red along the stroke {smeared:?}"
        );
        assert!(smeared[1] < 80 && smeared[2] < 80);
        assert_eq!(sample(&pm, 22, 4), [0, 0, 0, 0]);
    }

    #[test]
    fn clone_stroke_tracks_the_source_offset() {
        let mut pm = Pixmap::new(20, 8).unwrap();
        set_px(&mut pm, 2, 4, [10, 200, 10, 255]);
        set_px(&mut pm, 4, 4, [10, 10, 200, 255]);
        let brush = Brush {
            size: 3.0,
            hardness: 1.0,
            flow: 1.0,
            opacity: 1.0,
            spacing: 0.4,
            ..Default::default()
        };
        clone_stamp(&mut pm, Pt::new(10.0, 4.0), Pt::new(2.0, 4.0), &brush);
        clone_stroke(
            &mut pm,
            Pt::new(10.0, 4.0),
            Pt::new(12.0, 4.0),
            Pt::new(2.0, 4.0),
            &brush,
        );
        assert_eq!(sample(&pm, 10, 4), [10, 200, 10, 255]);
        assert_eq!(sample(&pm, 12, 4), [10, 10, 200, 255]);
    }

    #[test]
    fn pixel_selection_helpers_combine_clip_and_bound() {
        let mut overlay = Pixmap::new(4, 1).unwrap();
        overlay
            .data_mut()
            .copy_from_slice(&[9, 9, 9, 255, 8, 8, 8, 255, 7, 7, 7, 255, 6, 6, 6, 255]);
        let mask = vec![0, 255, 255, 0];
        clip_overlay(&mut overlay, &mask);
        assert_eq!(overlay.data()[3], 0);
        assert!(overlay.data()[7] > 0);
        assert!(overlay.data()[11] > 0);
        assert_eq!(overlay.data()[15], 0);
        let added = combine_masks(Some(&[255, 0, 0, 0]), &[0, 255, 0, 0], PixelCombine::Add);
        assert_eq!(added, vec![255, 255, 0, 0]);
        let subtracted = combine_masks(
            Some(&[255, 255, 255, 0]),
            &[0, 255, 0, 0],
            PixelCombine::Subtract,
        );
        assert_eq!(subtracted, vec![255, 0, 255, 0]);
        assert_eq!(selected_count(&added), 2);
        let shifted = shift_mask(&[255, 0, 0, 0], 2, 2, 1, 0);
        assert_eq!(shifted, vec![0, 255, 0, 0]);
        let feathered = feather_mask(&[0, 255, 0, 0], 2, 2, 1);
        assert!(feathered.iter().any(|p| *p > 0 && *p < 255));
        let sheared = shear_mask(&[255, 0, 255, 0], 2, 2, 0.0);
        assert_eq!(sheared, vec![255, 0, 255, 0]);
        assert_eq!(selection_bounds(&added, 4, 1), Some((0, 0, 2, 1)));
        let original = Pixmap::new(2, 1).unwrap();
        let mut dst = original.clone();
        dst.data_mut()[..4].copy_from_slice(&[1, 2, 3, 255]);
        restrict_pixmap(&mut dst, &original, &[0, 255]);
        assert_eq!(&dst.data()[..4], &original.data()[..4]);
    }
}
