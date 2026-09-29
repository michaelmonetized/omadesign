//! Offline Real-ESRGAN. Only one padded tile enters ORT at a time. The last
//! pass resamples tiles in global coordinates straight into the exact output,
//! avoiding an oversized native-4x intermediate for 2x or custom factors.
pub mod cutout;
pub mod models;
use crate::ml::{Progress, runtime};
use image::RgbaImage;
use models::Model;
use ort::session::Session;

pub const MAX_PIXELS: u64 = 512_000_000;
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Settings {
    pub factor: f64,
    pub model: Model,
    pub tile: u32,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            factor: 2.,
            model: Model::General,
            tile: 192,
        }
    }
}

pub fn dimensions(w: u32, h: u32, factor: f64) -> Result<(u32, u32), String> {
    if !factor.is_finite() || !(1.0..=32.0).contains(&factor) || w == 0 || h == 0 {
        return Err("Choose a scale between 1× and 32× for a nonempty image".into());
    }
    let (w, h) = (
        (w as f64 * factor).round() as u64,
        (h as f64 * factor).round() as u64,
    );
    if w > 65536 || h > 65536 || w * h > MAX_PIXELS {
        return Err(
            "Output exceeds 512 megapixels or 65,536 pixels on one side. Choose a smaller scale."
                .into(),
        );
    }
    Ok((w as u32, h as u32))
}

fn passes(w: u32, h: u32, settings: Settings) -> Result<Vec<(u32, u32)>, String> {
    let target = dimensions(w, h, settings.factor)?;
    let mut size = (w, h);
    let mut result = vec![];
    while size != target {
        size = (
            (size.0 * settings.model.scale()).min(target.0),
            (size.1 * settings.model.scale()).min(target.1),
        );
        result.push(size);
    }
    Ok(result)
}

pub fn upscale(
    source: &RgbaImage,
    settings: Settings,
    progress: &dyn Progress,
) -> Result<RgbaImage, String> {
    let plan = passes(source.width(), source.height(), settings)?;
    progress.check()?;
    if plan.is_empty() {
        return Ok(source.clone());
    }
    progress.report("Loading Real-ESRGAN", 0, 1);
    runtime::initialize()?;
    let bytes = settings.model.read(progress)?;
    let mut session = Session::builder()
        .map_err(|e| e.to_string())?
        .with_intra_threads(std::thread::available_parallelism().map_or(2, |n| n.get().min(4)))
        .map_err(|e| e.to_string())?
        .with_memory_pattern(false)
        .map_err(|e| e.to_string())?
        .with_execution_providers([ort::ep::CPU::default().with_arena_allocator(false).build()])
        .map_err(|e| e.to_string())?
        .commit_from_memory(&bytes)
        .map_err(|e| e.to_string())?;
    drop(bytes);
    let tile = settings.tile.clamp(64, 512) as usize;
    let mut previous = source.dimensions();
    let total: usize = plan
        .iter()
        .map(|&next| {
            let n = (previous.0 as usize).div_ceil(tile) * (previous.1 as usize).div_ceil(tile);
            previous = next;
            n
        })
        .sum();
    let mut done = 0;
    let mut current = None;
    for (pass, target) in plan.iter().enumerate() {
        let input = current.as_ref().unwrap_or(source);
        let stage = format!("Upscaling · pass {} / {} · tiles", pass + 1, plan.len());
        let output = tiled(
            input,
            *target,
            settings,
            &mut done,
            total,
            &stage,
            progress,
            |input, w, h| {
                let scale = settings.model.scale() as usize;
                runtime::run_tensor(
                    &mut session,
                    input,
                    [1, 3, h, w],
                    [1, 3, h * scale, w * scale],
                    progress,
                )
            },
        )?;
        current = Some(output);
    }
    progress.check()?;
    Ok(current.unwrap())
}

// Catmull-Rom, with a widened support when reducing native model output.
fn cubic(x: f64) -> f64 {
    let x = x.abs();
    if x < 1. {
        (1.5 * x - 2.5) * x * x + 1.
    } else if x < 2. {
        ((-0.5 * x + 2.5) * x - 4.) * x + 2.
    } else {
        0.
    }
}
fn weights(position: f64, ratio: f64, length: usize) -> Vec<(usize, f32)> {
    let support = ratio.max(1.);
    let mut weights: Vec<_> = (((position - 2. * support).ceil() as isize)
        ..=((position + 2. * support).floor() as isize))
        .map(|i| {
            (
                i.clamp(0, length as isize - 1) as usize,
                cubic((position - i as f64) / support) as f32,
            )
        })
        .filter(|(_, v)| *v != 0.)
        .collect();
    let sum: f32 = weights.iter().map(|(_, v)| v).sum();
    for (_, v) in &mut weights {
        *v /= sum;
    }
    weights
}

#[allow(clippy::too_many_arguments)]
fn tiled(
    source: &RgbaImage,
    target: (u32, u32),
    settings: Settings,
    done: &mut usize,
    total: usize,
    stage: &str,
    progress: &dyn Progress,
    mut infer: impl FnMut(Vec<f32>, usize, usize) -> Result<Vec<f32>, String>,
) -> Result<RgbaImage, String> {
    let (w, h) = (source.width() as usize, source.height() as usize);
    let (ow, oh) = (target.0 as usize, target.1 as usize);
    let scale = settings.model.scale() as usize;
    let tile = settings.tile.clamp(64, 512) as usize;
    // SRVGG has 34 convolutions. A 40px halo covers its full receptive field;
    // RRDB uses a larger 64px context. Trim all padded output before assembly.
    let pad = if settings.model == Model::General {
        40
    } else {
        64
    };
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(ow * oh * 4)
        .map_err(|_| "Not enough memory for the upscaled image")?;
    bytes.resize(ow * oh * 4, 0);
    let opaque = source.pixels().all(|p| p[3] == 255);
    progress.report(stage, *done, total);
    for y in (0..h).step_by(tile) {
        for x in (0..w).step_by(tile) {
            progress.check()?;
            let (endx, endy) = ((x + tile).min(w), (y + tile).min(h));
            // Native x2 RRDB pixel-unshuffle needs even coordinates and sizes.
            let (left, top) = (x.saturating_sub(pad) / 2 * 2, y.saturating_sub(pad) / 2 * 2);
            let (right, bottom) = (
                (endx + pad).min(w).next_multiple_of(2),
                (endy + pad).min(h).next_multiple_of(2),
            );
            let (tw, th) = (right - left, bottom - top);
            let area = tw * th;
            let mut input = vec![0.; 3 * area];
            for ty in 0..th {
                for tx in 0..tw {
                    let p = source
                        .get_pixel((left + tx).min(w - 1) as u32, (top + ty).min(h - 1) as u32);
                    for c in 0..3 {
                        input[c * area + ty * tw + tx] = p[c] as f32 / 255.;
                    }
                }
            }
            let values = infer(input, tw, th)?;
            let (nw, nh) = (tw * scale, th * scale);
            if values.len() != nw * nh * 3 {
                return Err("Invalid upscaler output shape".into());
            }
            let (x0, x1) = ((x * ow).div_ceil(w), (endx * ow).div_ceil(w));
            let (y0, y1) = ((y * oh).div_ceil(h), (endy * oh).div_ceil(h));
            let ratio_x = (w * scale) as f64 / ow as f64;
            let ratio_y = (h * scale) as f64 / oh as f64;
            let xweights: Vec<_> = (x0..x1)
                .map(|ox| {
                    weights(
                        (ox as f64 + 0.5) * ratio_x - 0.5 - (left * scale) as f64,
                        ratio_x,
                        nw,
                    )
                })
                .collect();
            for oy in y0..y1 {
                if oy % 32 == 0 {
                    progress.check()?;
                }
                let yw = weights(
                    (oy as f64 + 0.5) * ratio_y - 0.5 - (top * scale) as f64,
                    ratio_y,
                    nh,
                );
                for ox in x0..x1 {
                    let pixel = &mut bytes[(oy * ow + ox) * 4..][..4];
                    for c in 0..3 {
                        let mut value = 0.;
                        for &(sy, wy) in &yw {
                            for &(sx, wx) in &xweights[ox - x0] {
                                value += values[c * nw * nh + sy * nw + sx] * wy * wx;
                            }
                        }
                        pixel[c] = (value.clamp(0., 1.) * 255.).round() as u8;
                    }
                    pixel[3] = if opaque {
                        255
                    } else {
                        sample_alpha(source, ox, oy, ow, oh)
                    };
                }
            }
            *done += 1;
            progress.report(stage, *done, total);
        }
    }
    Ok(RgbaImage::from_raw(target.0, target.1, bytes).unwrap())
}

fn sample_alpha(source: &RgbaImage, x: usize, y: usize, w: usize, h: usize) -> u8 {
    let xw = weights(
        (x as f64 + 0.5) * source.width() as f64 / w as f64 - 0.5,
        1.,
        source.width() as usize,
    );
    let yw = weights(
        (y as f64 + 0.5) * source.height() as f64 / h as f64 - 0.5,
        1.,
        source.height() as usize,
    );
    let mut value = 0.;
    for (sy, wy) in yw {
        for &(sx, wx) in &xw {
            value += source.get_pixel(sx as u32, sy as u32)[3] as f32 * wx * wy;
        }
    }
    value.round().clamp(0., 255.) as u8
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ml::NoProgress;
    #[test]
    fn exact_dimensions_passes_and_limits() {
        assert_eq!(dimensions(73, 51, 2.5).unwrap(), (183, 128));
        assert_eq!(
            passes(
                73,
                51,
                Settings {
                    factor: 5.,
                    ..Default::default()
                }
            )
            .unwrap(),
            vec![(292, 204), (365, 255)]
        );
        assert_eq!(
            passes(
                73,
                51,
                Settings {
                    factor: 5.,
                    model: Model::Native2,
                    ..Default::default()
                }
            )
            .unwrap(),
            vec![(146, 102), (292, 204), (365, 255)]
        );
        for scale in [f64::NAN, f64::INFINITY, 0., 33.] {
            assert!(dimensions(10, 10, scale).is_err());
        }
        assert!(dimensions(6000, 4000, 4.).is_ok());
        assert!(dimensions(20000, 20000, 4.).is_err());
    }
    #[test]
    fn padded_tiles_cover_odd_dimensions_without_seams_and_preserve_alpha() {
        let source = RgbaImage::from_fn(139, 73, |x, y| {
            image::Rgba([80, 120, 200, ((x + y) % 256) as u8])
        });
        for factor in [2., 4., 2.7] {
            let target = dimensions(139, 73, factor).unwrap();
            let settings = Settings {
                factor,
                tile: 64,
                ..Default::default()
            };
            let result = tiled(
                &source,
                target,
                settings,
                &mut 0,
                6,
                "test",
                &NoProgress,
                |_, w, h| {
                    Ok([80., 120., 200.]
                        .iter()
                        .flat_map(|v| vec![v / 255.; w * h * 16])
                        .collect())
                },
            )
            .unwrap();
            assert_eq!(result.dimensions(), target);
            for (x, y, p) in result.enumerate_pixels() {
                assert_eq!(&p.0[..3], &[80, 120, 200]);
                assert_eq!(
                    p[3],
                    sample_alpha(
                        &source,
                        x as usize,
                        y as usize,
                        target.0 as usize,
                        target.1 as usize
                    )
                );
            }
        }
    }
    #[test]
    fn bundled_model_is_verified() {
        Model::General.read(&NoProgress).unwrap();
    }
}
