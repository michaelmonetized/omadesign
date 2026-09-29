use super::{RawMask, Source};
use crate::ml::Progress;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Settings {
    pub radius: usize,
    pub epsilon: f32,
    pub shift: i32,
    pub contrast: f32,
    pub intersect: bool,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            radius: 12,
            epsilon: 1e-4,
            shift: 0,
            contrast: 1.,
            intersect: false,
        }
    }
}

/// Separable clipped box mean, O(pixels) regardless of radius. Accumulate in
/// f64 so subtracting moments at epsilon=1e-4 remains numerically stable.
fn mean(
    input: &[f32],
    w: usize,
    h: usize,
    r: usize,
    progress: &dyn Progress,
) -> Result<Vec<f32>, String> {
    let mut temp = vec![0.; input.len()];
    let mut out = vec![0.; input.len()];
    for y in 0..h {
        progress.check()?;
        let row = &input[y * w..(y + 1) * w];
        let mut sum: f64 = row[..(r + 1).min(w)].iter().map(|v| *v as f64).sum();
        for x in 0..w {
            temp[y * w + x] =
                (sum / (x.saturating_add(r).min(w - 1) - x.saturating_sub(r) + 1) as f64) as f32;
            if x >= r {
                sum -= row[x - r] as f64;
            }
            if x + r + 1 < w {
                sum += row[x + r + 1] as f64;
            }
        }
    }
    for x in 0..w {
        if x % 64 == 0 {
            progress.check()?;
        }
        let mut sum: f64 = (0..(r + 1).min(h)).map(|y| temp[y * w + x] as f64).sum();
        for y in 0..h {
            out[y * w + x] =
                (sum / (y.saturating_add(r).min(h - 1) - y.saturating_sub(r) + 1) as f64) as f32;
            if y >= r {
                sum -= temp[(y - r) * w + x] as f64;
            }
            if y + r + 1 < h {
                sum += temp[(y + r + 1) * w + x] as f64;
            }
        }
    }
    Ok(out)
}

fn guided(
    guide: &[f32],
    mask: &[f32],
    w: usize,
    h: usize,
    r: usize,
    eps: f32,
    progress: &dyn Progress,
) -> Result<Vec<f32>, String> {
    if r == 0 {
        return Ok(mask.to_vec());
    }
    let mi = mean(guide, w, h, r, progress)?;
    progress.report("Guided matte", 1, 7);
    let mp = mean(mask, w, h, r, progress)?;
    progress.report("Guided matte", 2, 7);
    let mut a = mean(
        &guide
            .iter()
            .zip(mask)
            .map(|(i, p)| i * p)
            .collect::<Vec<_>>(),
        w,
        h,
        r,
        progress,
    )?;
    let corr = mean(
        &guide.iter().map(|i| i * i).collect::<Vec<_>>(),
        w,
        h,
        r,
        progress,
    )?;
    progress.report("Guided matte", 3, 7);
    for k in 0..a.len() {
        a[k] = (a[k] - mi[k] * mp[k]) / ((corr[k] - mi[k] * mi[k]).max(0.) + eps);
    }
    drop(corr);
    let b: Vec<f32> = mp
        .iter()
        .zip(a.iter().zip(&mi))
        .map(|(p, (a, i))| p - a * i)
        .collect();
    drop(mi);
    drop(mp);
    let ma = mean(&a, w, h, r, progress)?;
    drop(a);
    progress.report("Guided matte", 4, 7);
    let mb = mean(&b, w, h, r, progress)?;
    progress.report("Guided matte", 5, 7);
    Ok(ma
        .into_iter()
        .zip(mb)
        .zip(guide)
        .map(|((a, b), i)| (a * i + b).clamp(0., 1.))
        .collect())
}

fn extrema(
    input: &[f32],
    w: usize,
    h: usize,
    r: usize,
    grow: bool,
    progress: &dyn Progress,
) -> Result<Vec<f32>, String> {
    // Monotonic queues give square erosion/dilation without a radius-squared loop.
    let pass = |input: &[f32], horizontal: bool| -> Result<Vec<f32>, String> {
        let mut out = vec![0.; input.len()];
        let (lines, len) = if horizontal { (h, w) } else { (w, h) };
        for line in 0..lines {
            progress.check()?;
            let at = |p: usize| {
                if horizontal {
                    line * w + p
                } else {
                    p * w + line
                }
            };
            let mut queue = std::collections::VecDeque::<usize>::new();
            let mut next = 0;
            for p in 0..len {
                while next <= (p + r).min(len - 1) {
                    while queue.back().is_some_and(|&q| {
                        if grow {
                            input[at(q)] <= input[at(next)]
                        } else {
                            input[at(q)] >= input[at(next)]
                        }
                    }) {
                        queue.pop_back();
                    }
                    queue.push_back(next);
                    next += 1;
                }
                while queue.front().is_some_and(|&q| q < p.saturating_sub(r)) {
                    queue.pop_front();
                }
                out[at(p)] = input[at(*queue.front().unwrap())];
            }
        }
        Ok(out)
    };
    pass(&pass(input, true)?, false)
}

/// Full-resolution grayscale RGBA mask. Pixels outside a selection keep their
/// previous mask (or white), and feathered selection values blend that mask.
pub fn refine(
    source: &Source,
    raw: &RawMask,
    settings: Settings,
    progress: &dyn Progress,
) -> Result<Vec<u8>, String> {
    let region = source.region()?;
    if region != raw.region || raw.values.len() != region.w * region.h {
        return Err("Cached mask belongs to a different selection".into());
    }
    if !settings.epsilon.is_finite() || !settings.contrast.is_finite() {
        return Err("Invalid matte settings".into());
    }
    progress.check()?;
    progress.report("Guided matte", 0, 7);
    let mut guide = Vec::with_capacity(raw.values.len());
    for y in 0..region.h {
        for x in 0..region.w {
            let k = ((region.y + y) * source.w as usize + region.x + x) * 4;
            let p = &source.rgba[k..k + 4];
            guide.push(
                (p[0] as f32 * 0.2126 + p[1] as f32 * 0.7152 + p[2] as f32 * 0.0722) / 255.
                    * (p[3] as f32 / 255.),
            );
        }
    }
    let mut mask = guided(
        &guide,
        &raw.values,
        region.w,
        region.h,
        settings.radius.min(64),
        settings.epsilon.clamp(1e-6, 1.),
        progress,
    )?;
    drop(guide);
    if settings.shift != 0 {
        mask = extrema(
            &mask,
            region.w,
            region.h,
            settings.shift.unsigned_abs().min(64) as usize,
            settings.shift > 0,
            progress,
        )?;
    }
    progress.report("Preparing editable mask", 6, 7);
    let mut output = source.existing_mask.as_ref().map_or_else(
        || vec![255; source.w as usize * source.h as usize * 4],
        |m| m.as_ref().clone(),
    );
    for y in 0..region.h {
        progress.check()?;
        for x in 0..region.w {
            let i = (region.y + y) * source.w as usize + region.x + x;
            let selection = source.selection.as_ref().map_or(1., |s| s[i] as f32 / 255.);
            if selection == 0. {
                continue;
            }
            let old = &output[i * 4..i * 4 + 4];
            let original =
                ((old[0] as f32 * 0.2126 + old[1] as f32 * 0.7152 + old[2] as f32 * 0.0722) / 255.)
                    * (old[3] as f32 / 255.);
            let mut amount = ((mask[y * region.w + x] - 0.5) * settings.contrast.clamp(0.1, 4.)
                + 0.5)
                .clamp(0., 1.);
            if settings.intersect {
                amount *= original;
            }
            let v = ((original + (amount - original) * selection) * 255.)
                .round()
                .clamp(0., 255.) as u8;
            output[i * 4..i * 4 + 4].copy_from_slice(&[v, v, v, 255]);
        }
    }
    progress.report("Ready", 7, 7);
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ml::NoProgress;
    #[test]
    fn box_means_match_brute_force_at_borders() {
        let v: Vec<_> = (0..35).map(|i| (i % 9) as f32 / 9.).collect();
        for r in [0, 1, 3, 12] {
            let out = mean(&v, 7, 5, r, &NoProgress).unwrap();
            for y in 0usize..5 {
                for x in 0usize..7 {
                    let mut sum = 0.;
                    let mut n = 0.;
                    for yy in y.saturating_sub(r)..=(y + r).min(4) {
                        for xx in x.saturating_sub(r)..=(x + r).min(6) {
                            sum += v[yy * 7 + xx];
                            n += 1.;
                        }
                    }
                    assert!((out[y * 7 + x] - sum / n).abs() < 1e-5);
                }
            }
        }
    }
    #[test]
    fn guided_filter_retains_constant_mattes_and_follows_full_resolution_edges() {
        let guide: Vec<_> = (0..256)
            .map(|i| if i % 32 < 16 { 0. } else { 1. })
            .collect();
        let mask: Vec<_> = (0..256)
            .map(|i| {
                if i % 32 < 14 {
                    0.
                } else if i % 32 > 17 {
                    1.
                } else {
                    0.5
                }
            })
            .collect();
        let result = guided(&guide, &mask, 32, 8, 12, 1e-4, &NoProgress).unwrap();
        assert!(
            result[15] < 0.25 && result[16] > 0.75,
            "guided edge: {} to {}",
            result[15],
            result[16]
        );
        let constant = guided(&guide, &vec![0.37; 256], 32, 8, 12, 1e-4, &NoProgress).unwrap();
        assert!(constant.iter().all(|v| (v - 0.37).abs() < 1e-4));
    }
    #[test]
    fn selection_replace_intersect_and_shift_are_non_destructive() {
        let source = Source {
            w: 4,
            h: 1,
            rgba: std::sync::Arc::new(vec![255; 16]),
            selection: Some(std::sync::Arc::new(vec![0, 128, 255, 0])),
            existing_mask: Some(std::sync::Arc::new([128, 128, 128, 255].repeat(4))),
        };
        let raw = RawMask {
            region: source.region().unwrap(),
            rough: vec![0., 1.],
            values: vec![0., 1.],
            stats: Default::default(),
        };
        let result = refine(
            &source,
            &raw,
            Settings {
                radius: 0,
                ..Default::default()
            },
            &NoProgress,
        )
        .unwrap();
        assert_eq!(
            result,
            [
                128, 128, 128, 255, 64, 64, 64, 255, 255, 255, 255, 255, 128, 128, 128, 255
            ]
        );
        let result = refine(
            &source,
            &raw,
            Settings {
                radius: 0,
                intersect: true,
                ..Default::default()
            },
            &NoProgress,
        )
        .unwrap();
        assert_eq!(result[8], 128);
        assert_eq!(
            extrema(&[0., 1., 0.], 3, 1, 1, true, &NoProgress).unwrap(),
            vec![1.; 3]
        );
        assert_eq!(
            extrema(&[1., 0., 1.], 3, 1, 1, false, &NoProgress).unwrap(),
            vec![0.; 3]
        );
        assert!(source.rgba.iter().all(|v| *v == 255));
    }
}
