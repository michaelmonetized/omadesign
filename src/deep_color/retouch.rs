//! Native clone, smudge and healing edits on straight authored samples.
use crate::{document::Pixels, geom::Pt, paint::Brush, tools::Tool};

/// Apply a retouch segment without converting authored pixels to bytes.
/// Args: immutable donor, working pixels, segment, source offset, tool and brush.
/// Returns an edited working buffer retaining its native depth and metadata.
pub fn stroke(
    donor: &Pixels,
    working: &Pixels,
    from: Pt,
    to: Pt,
    offset: Pt,
    tool: Tool,
    brush: &Brush,
) -> Result<Pixels, String> {
    if !matches!(tool, Tool::Clone | Tool::Smudge | Tool::Heal)
        || [from.x, from.y, to.x, to.y, offset.x, offset.y]
            .iter()
            .any(|v| !v.is_finite() || v.abs() > 1e7)
        || [
            brush.size,
            brush.flow,
            brush.opacity,
            brush.spacing,
            brush.hardness,
        ]
        .iter()
        .any(|v| !v.is_finite())
        || donor.w != working.w
        || donor.h != working.h
    {
        return Err("Invalid native retouch coordinates, tool or image dimensions".into());
    }
    let distance = (to - from).length();
    if tool == Tool::Smudge && distance < 0.01 {
        return Ok(working.clone());
    }
    let steps = (distance / (brush.size * brush.spacing.max(0.05)).clamp(0.5, 64.))
        .ceil()
        .clamp(1., 8192.) as usize;
    let mut out = working.clone();
    for step in 1..=steps {
        let a = from.lerp(to, (step - 1) as f32 / steps as f32);
        let b = from.lerp(to, step as f32 / steps as f32);
        dab(donor, &mut out, a, b, offset, tool, brush)?;
    }
    Ok(out)
}
fn premul(p: [f32; 4]) -> [f64; 4] {
    [
        p[0] as f64 * p[3] as f64,
        p[1] as f64 * p[3] as f64,
        p[2] as f64 * p[3] as f64,
        p[3] as f64,
    ]
}
/// Interpolate straight samples through their premultiplied colors.
/// Args: original samples, edited samples and coverage. Returns straight RGBA.
pub(crate) fn mix(a: [f32; 4], b: [f32; 4], t: f32) -> [f32; 4] {
    if t <= 0. {
        return a;
    }
    if t >= 1. {
        return b;
    }
    let alpha = a[3] * (1. - t) + b[3] * t;
    let mut out = [0., 0., 0., alpha];
    for c in 0..3 {
        out[c] = if alpha > 1e-12 {
            (a[c] * a[3] * (1. - t) + b[c] * b[3] * t) / alpha
        } else {
            a[c]
        };
    }
    out
}

fn dab(
    donor: &Pixels,
    working: &mut Pixels,
    from: Pt,
    pos: Pt,
    offset: Pt,
    tool: Tool,
    brush: &Brush,
) -> Result<(), String> {
    let strength = (brush.flow * brush.opacity).clamp(0., 1.);
    if strength <= 0. {
        return Ok(());
    }
    let radius = (brush.size * 0.5).clamp(1., 512.);
    let extent = radius.ceil() as i32;
    let cx = pos.x.round() as i32;
    let cy = pos.y.round() as i32;
    let inside = |p: &Pixels, x: i32, y: i32| x >= 0 && y >= 0 && x < p.w as i32 && y < p.h as i32;
    let at = |p: &Pixels, x: i32, y: i32| {
        if inside(p, x, y) {
            p.sample(y as usize * p.w as usize + x as usize)
        } else {
            [0.; 4]
        }
    };
    let basis = |dx: i32, dy: i32| {
        let x = dx as f64 / radius as f64;
        let y = dy as f64 / radius as f64;
        [1., x, y, x * x, x * y, y * y]
    };
    let mut field = [[0.; 3]; 6];
    let blur = (radius * 0.25).ceil().max(2.) as i32;
    let sx = (pos.x + offset.x).round() as i32;
    let sy = (pos.y + offset.y).round() as i32;
    let left = (sx - extent - blur).clamp(0, donor.w as i32);
    let top = (sy - extent - blur).clamp(0, donor.h as i32);
    let right = (sx + extent + blur + 1).clamp(left, donor.w as i32);
    let bottom = (sy + extent + blur + 1).clamp(top, donor.h as i32);
    let width = (right - left + 1) as usize;
    let mut sums =
        crate::resources::global().allocate_now::<[f64; 4], _>(0, &super::storage::check)?;
    if tool == Tool::Heal {
        let mut equations = [[0_f64; 9]; 6];
        let stride = (extent / 24).max(1) as usize;
        for dy in (-extent..=extent).step_by(stride) {
            for dx in (-extent..=extent).step_by(stride) {
                let d = ((dx * dx + dy * dy) as f32).sqrt() / radius;
                if !(0.65..=1.).contains(&d) {
                    continue;
                }
                let p = premul(at(donor, cx + dx, cy + dy));
                let terms = basis(dx, dy);
                for i in 0..6 {
                    for j in 0..6 {
                        equations[i][j] += terms[i] * terms[j] * p[3];
                    }
                    for c in 0..3 {
                        equations[i][6 + c] += terms[i] * p[c];
                    }
                }
            }
        }
        if equations[0][0] < 1. {
            return Ok(());
        }
        field = crate::paint::heal_colour_field(equations);
        if right <= left || bottom <= top {
            return Ok(());
        }
        let count = width
            .checked_mul((bottom - top + 1) as usize)
            .ok_or("Healing scratch overflows")?;
        sums = crate::resources::global()
            .allocate_now::<[f64; 4], _>(count, &super::storage::check)?;
        for y in 1..=(bottom - top) as usize {
            let mut row = [0.; 4];
            for x in 1..width {
                let p = premul(at(donor, left + x as i32 - 1, top + y as i32 - 1));
                for c in 0..4 {
                    row[c] += p[c];
                    sums.as_mut_slice()[y * width + x][c] = sums[(y - 1) * width + x][c] + row[c];
                }
            }
        }
    }
    let input = working.clone();
    let mut samples = working.copy_samples(&super::storage::check)?;
    let delta = pos - from;
    let distance = delta.length();
    let dir = if distance > 1e-6 {
        delta * (1. / distance)
    } else {
        Pt::ZERO
    };
    let pull = radius * (0.25 + 0.55 * brush.flow.clamp(0., 1.));
    for dy in -extent..=extent {
        for dx in -extent..=extent {
            let x = cx + dx;
            let y = cy + dy;
            if !inside(working, x, y) {
                continue;
            }
            let d = ((dx * dx + dy * dy) as f32).sqrt() / radius;
            if d >= 1. {
                continue;
            }
            let edge = ((1. - d) / (1. - brush.hardness.clamp(0., 0.98))).clamp(0., 1.);
            let amount = strength * edge * edge * (3. - 2. * edge);
            let original = at(&input, x, y);
            let source = if tool == Tool::Smudge {
                super::sample_at(
                    &input,
                    x as f32 + 0.5 - dir.x * pull,
                    y as f32 + 0.5 - dir.y * pull,
                )
            } else {
                super::sample_at(donor, x as f32 + 0.5 + offset.x, y as f32 + 0.5 + offset.y)
            };
            let value = if tool == Tool::Heal {
                let (x, y) = (sx + dx, sy + dy);
                if !inside(donor, x, y) || source[3] <= 0. || original[3] <= 0. {
                    continue;
                }
                let x0 = ((x - blur).max(left) - left) as usize;
                let y0 = ((y - blur).max(top) - top) as usize;
                let x1 = ((x + blur + 1).min(right) - left) as usize;
                let y1 = ((y + blur + 1).min(bottom) - top) as usize;
                let local: [f64; 4] = std::array::from_fn(|c| {
                    sums[y1 * width + x1][c] + sums[y0 * width + x0][c]
                        - sums[y0 * width + x1][c]
                        - sums[y1 * width + x0][c]
                });
                if local[3] <= 1e-12 {
                    continue;
                }
                let terms = basis(dx, dy);
                let mut out = original;
                let t = amount * source[3];
                for c in 0..3 {
                    let shading: f64 = field.iter().zip(terms).map(|(f, t)| f[c] * t).sum();
                    let texture = source[c] as f64 - local[c] / local[3];
                    out[c] = original[c] * (1. - t) + (shading + texture) as f32 * t;
                }
                out
            } else {
                mix(original, source, amount)
            };
            let i = (y as usize * working.w as usize + x as usize) * 4;
            samples.as_mut_slice()[i..i + 4].copy_from_slice(&value);
        }
    }
    working.set_sample_allocation(std::sync::Arc::new(samples))
}

/// Find a connected fill region from authoritative straight samples.
/// Args: image, seed, tolerance in the brush's 0–510 scale and optional selection coverage.
/// Returns admitted coverage, retaining sub-byte color differences and soft selection weights.
pub fn flood_region(
    pixels: &Pixels,
    seed: Pt,
    tolerance: f32,
    selection: Option<&[u8]>,
) -> Result<std::sync::Arc<crate::resources::Allocation<u8>>, String> {
    use super::storage;
    use crate::resources;
    let count = storage::count(pixels.w, pixels.h, 1)?;
    if !seed.x.is_finite() || !seed.y.is_finite() || !tolerance.is_finite() || !(0. ..=510.).contains(&tolerance)
        || selection.is_some_and(|s| s.len() != count) {
        return Err("Invalid native fill seed, tolerance or selection".into());
    }
    let mut region = resources::global().allocate_now::<u8,_>(count, &storage::check)?;
    let (x,y) = (seed.x.round() as i64, seed.y.round() as i64);
    if x < 0 || y < 0 || x >= pixels.w as i64 || y >= pixels.h as i64 {
        return Ok(std::sync::Arc::new(region));
    }
    let first = y as usize * pixels.w as usize + x as usize;
    if selection.is_some_and(|s| s[first] == 0) {return Ok(std::sync::Arc::new(region));}
    let target = pixels.sample(first);
    let mut queue = resources::global().allocate_now::<usize,_>(count, &storage::check)?;
    queue.as_mut_slice()[0] = first;region.as_mut_slice()[first] = 1;
    let (mut read, mut write) = (0,1);
    while read < write {
        let i = queue[read];read += 1;
        let sample = pixels.sample(i);
        let distance = sample.into_iter().zip(target).map(|(a,b)| ((a-b)*255.).powi(2)).sum::<f32>();
        if distance > tolerance * tolerance || selection.is_some_and(|s| s[i] == 0) {region.as_mut_slice()[i] = 2;continue;}
        region.as_mut_slice()[i] = 255;
        let x = i % pixels.w as usize;
        for next in [
            (x > 0).then(|| i-1),
            (x+1 < pixels.w as usize).then(|| i+1),
            (i >= pixels.w as usize).then(|| i-pixels.w as usize),
            (i+(pixels.w as usize) < count).then(|| i+pixels.w as usize),
        ].into_iter().flatten() {
            if region[next] == 0 {region.as_mut_slice()[next] = 1;queue.as_mut_slice()[write] = next;write += 1;}
        }
    }
    for (i, v) in region.as_mut_slice().iter_mut().enumerate() {*v = if *v == 255 {selection.map_or(255,|s|s[i])} else {0};}
    Ok(std::sync::Arc::new(region))
}
