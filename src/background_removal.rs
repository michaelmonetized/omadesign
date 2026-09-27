//! Global-context, boundary-only matting. Inference and refinement are separate
//! so a UI can cache RawMask and refine repeatedly without loading a model.
mod refine;
use crate::ml::{Progress, models::Model, runtime::Inference};
pub use refine::{Settings, refine};
use std::sync::Arc;

#[derive(Clone)]
pub struct Source {
    pub w: u32,
    pub h: u32,
    pub rgba: Arc<Vec<u8>>,
    pub selection: Option<Arc<Vec<u8>>>,
    pub existing_mask: Option<Arc<Vec<u8>>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Region {
    pub x: usize,
    pub y: usize,
    pub w: usize,
    pub h: usize,
}

impl Source {
    pub fn region(&self) -> Result<Region, String> {
        let (w, h) = (self.w as usize, self.h as usize);
        let n = w
            .checked_mul(h)
            .filter(|n| *n > 0 && *n <= 64_000_000)
            .ok_or("Background removal supports layers up to 64 million pixels")?;
        if self.rgba.len() != n * 4
            || self
                .existing_mask
                .as_ref()
                .is_some_and(|m| m.len() != n * 4)
        {
            return Err("Invalid layer or mask dimensions".into());
        }
        if let Some(selection) = &self.selection {
            if selection.len() != n {
                return Err("The pixel selection does not match this layer".into());
            }
            let (mut x0, mut y0, mut x1, mut y1) = (w, h, 0, 0);
            for (i, value) in selection.iter().enumerate() {
                if *value == 0 {
                    continue;
                }
                let (x, y) = (i % w, i / w);
                x0 = x0.min(x);
                y0 = y0.min(y);
                x1 = x1.max(x + 1);
                y1 = y1.max(y + 1);
            }
            if x0 >= x1 || y0 >= y1 {
                return Err(
                    "The selection is empty. Select the subject or clear the selection.".into(),
                );
            }
            Ok(Region {
                x: x0,
                y: y0,
                w: x1 - x0,
                h: y1 - y0,
            })
        } else {
            Ok(Region { x: 0, y: 0, w, h })
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct Stats {
    pub tiles: usize,
    pub edge_tiles: usize,
    pub inference_calls: usize,
}

pub struct RawMask {
    pub region: Region,
    pub rough: Vec<f32>,
    pub values: Vec<f32>,
    pub stats: Stats,
}

#[derive(Clone, Copy, Debug)]
struct Letterbox {
    w: usize,
    h: usize,
    x: usize,
    y: usize,
}
impl Letterbox {
    fn new(w: usize, h: usize, size: usize) -> Self {
        let scale = size as f64 / w.max(h) as f64;
        let w = (w as f64 * scale).round().clamp(1., size as f64) as usize;
        let h = (h as f64 * scale).round().clamp(1., size as f64) as usize;
        Self {
            w,
            h,
            x: (size - w) / 2,
            y: (size - h) / 2,
        }
    }
}

/// Returns CHW normalized data. Global passes letterbox, tiles copy at 1:1 and
/// pad incomplete edges; neither stretches the source's aspect ratio.
fn input(source: &Source, region: Region, model: Model, global: bool, intensity: f32) -> Vec<f32> {
    let size = model.input_size();
    let fit = if global {
        Letterbox::new(region.w, region.h, size)
    } else {
        Letterbox {
            w: region.w,
            h: region.h,
            x: 0,
            y: 0,
        }
    };
    let mut crop = Vec::with_capacity(region.w * region.h * 4);
    for y in region.y..region.y + region.h {
        let start = (y * source.w as usize + region.x) * 4;
        crop.extend_from_slice(&source.rgba[start..start + region.w * 4]);
    }
    let crop = image::RgbaImage::from_raw(region.w as u32, region.h as u32, crop)
        .expect("validated region");
    let scaled = if global {
        image::imageops::resize(
            &crop,
            fit.w as u32,
            fit.h as u32,
            image::imageops::FilterType::Triangle,
        )
    } else {
        crop
    };
    let (mean, std) = model.normalization();
    let plane = size * size;
    // Replicate boundary colors into padding to avoid a salient black frame.
    // One source-relative intensity scale, shared by all tiles, keeps the
    // upstream normalization's dark-image handling without tile-local changes.
    let mut output = vec![0.; 3 * plane];
    for y in 0..size {
        for x in 0..size {
            let sx = x.saturating_sub(fit.x).min(fit.w - 1);
            let sy = y.saturating_sub(fit.y).min(fit.h - 1);
            let rgba = scaled.get_pixel(sx as u32, sy as u32).0;
            for c in 0..3 {
                output[c * plane + y * size + x] =
                    (rgba[c] as f32 / intensity * (rgba[3] as f32 / 255.) - mean[c]) / std[c];
            }
        }
    }
    output
}

pub(super) fn sample(values: &[f32], w: usize, h: usize, x: f32, y: f32) -> f32 {
    let x = x.clamp(0., (w - 1) as f32);
    let y = y.clamp(0., (h - 1) as f32);
    let (ix, iy) = (x.floor() as usize, y.floor() as usize);
    let (fx, fy) = (x - ix as f32, y - iy as f32);
    let at = |x: usize, y: usize| values[y.min(h - 1) * w + x.min(w - 1)];
    (at(ix, iy) * (1. - fx) + at(ix + 1, iy) * fx) * (1. - fy)
        + (at(ix, iy + 1) * (1. - fx) + at(ix + 1, iy + 1) * fx) * fy
}

fn tiles(region: Region, size: usize) -> Vec<Region> {
    let stride = size * 3 / 4;
    let starts = |length: usize| {
        let mut out = vec![0];
        while out.last().unwrap() + size < length {
            out.push(out.last().unwrap() + stride);
        }
        out
    };
    let mut result = Vec::new();
    for y in starts(region.h) {
        for x in starts(region.w) {
            result.push(Region {
                x,
                y,
                w: size.min(region.w - x),
                h: size.min(region.h - y),
            });
        }
    }
    result
}

fn is_edge(rough: &[f32], w: usize, tile: Region) -> bool {
    let (mut min, mut max) = (1.0f32, 0.0f32);
    for y in tile.y..tile.y + tile.h {
        for &p in &rough[y * w + tile.x..y * w + tile.x + tile.w] {
            min = min.min(p);
            max = max.max(p);
        }
    }
    min < 0.98 && max > 0.02
}
fn weight(x: usize, y: usize, size: usize) -> f32 {
    let hann = |p: usize| {
        ((std::f32::consts::PI * (p as f32 + 0.5) / size as f32)
            .sin()
            .powi(2))
        .max(0.001)
    };
    hann(x) * hann(y)
}

pub fn infer(source: &Source, model: Model, progress: &dyn Progress) -> Result<RawMask, String> {
    let region = source.region()?;
    let mut engine = Inference::load(model, progress)?;
    let size = model.input_size();
    progress.report("Finding the subject", 0, 1);
    let mut intensity = 1u8;
    for y in region.y..region.y + region.h {
        progress.check()?;
        let start = (y * source.w as usize + region.x) * 4;
        for p in source.rgba[start..start + region.w * 4].chunks_exact(4) {
            intensity = intensity.max(p[0]).max(p[1]).max(p[2]);
        }
    }
    let intensity = intensity as f32;
    let global = engine.run(input(source, region, model, true, intensity), 1, progress)?;
    let fit = Letterbox::new(region.w, region.h, size);
    let mut rough = vec![0.; region.w * region.h];
    for y in 0..region.h {
        progress.check()?;
        for x in 0..region.w {
            rough[y * region.w + x] = sample(
                &global,
                size,
                size,
                fit.x as f32 + (x as f32 + 0.5) * fit.w as f32 / region.w as f32 - 0.5,
                fit.y as f32 + (y as f32 + 0.5) * fit.h as f32 / region.h as f32 - 0.5,
            );
        }
    }
    if region.w.max(region.h) <= size {
        return Ok(RawMask {
            region,
            values: rough.clone(),
            rough,
            stats: Stats {
                tiles: 1,
                edge_tiles: 0,
                inference_calls: 1,
            },
        });
    }
    let tiles = tiles(region, size);
    let mut stats = Stats {
        tiles: tiles.len(),
        edge_tiles: 0,
        inference_calls: 1,
    };
    let mut sum = vec![0.; rough.len()];
    let mut weights = vec![0.; rough.len()];
    let mut edges = Vec::new();
    // Confident tiles contribute the same coarse probability map at full size.
    for &tile in &tiles {
        progress.check()?;
        if is_edge(&rough, region.w, tile) {
            edges.push(tile);
            continue;
        }
        for y in 0..tile.h {
            for x in 0..tile.w {
                let i = (tile.y + y) * region.w + tile.x + x;
                let w = weight(x, y, size);
                sum[i] += rough[i] * w;
                weights[i] += w;
            }
        }
    }
    stats.edge_tiles = edges.len();
    for (chunk_index, chunk) in edges.chunks(engine.batch_size).enumerate() {
        progress.report(
            "Refining subject edge tiles",
            chunk_index * engine.batch_size,
            edges.len(),
        );
        progress.check()?;
        let mut batch = Vec::with_capacity(chunk.len() * 3 * size * size);
        for tile in chunk {
            batch.extend(input(
                source,
                Region {
                    x: region.x + tile.x,
                    y: region.y + tile.y,
                    ..*tile
                },
                model,
                false,
                intensity,
            ));
        }
        let predicted = engine.run(batch, chunk.len(), progress)?;
        stats.inference_calls += 1;
        for (tile, detail) in chunk.iter().zip(predicted.chunks_exact(size * size)) {
            for y in 0..tile.h {
                for x in 0..tile.w {
                    let i = (tile.y + y) * region.w + tile.x + x;
                    let w = weight(x, y, size);
                    // The global subject remains the authority in confident areas,
                    // even inside an edge tile. Local saliency can only add detail
                    // near that subject's boundary, not invent a subject in the sky.
                    let confidence = (rough[i] * (1. - rough[i]) * 4.).clamp(0., 1.);
                    let p = rough[i] + confidence * (detail[y * size + x] - rough[i]);
                    sum[i] += p * w;
                    weights[i] += w;
                }
            }
        }
    }
    progress.report("Refining subject edge tiles", edges.len(), edges.len());
    for (value, w) in sum.iter_mut().zip(weights) {
        *value = (*value / w.max(f32::MIN_POSITIVE)).clamp(0., 1.);
    }
    progress.check()?;
    Ok(RawMask {
        region,
        rough,
        values: sum,
        stats,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn letterbox_preserves_portrait_landscape_and_extreme_aspect_ratios() {
        for (w, h) in [(4000, 2000), (2000, 4000), (1, 4000)] {
            let f = Letterbox::new(w, h, 320);
            assert_eq!(f.w.max(f.h), 320);
            assert!((f.w as f64 / f.h as f64 - w as f64 / h as f64).abs() < 0.01);
        }
    }
    #[test]
    fn tile_blend_covers_partial_edges_without_seams_or_probability_stretch() {
        let region = Region {
            x: 0,
            y: 0,
            w: 911,
            h: 731,
        };
        let mut sum = vec![0.; region.w * region.h];
        let mut ws = sum.clone();
        for t in tiles(region, 320) {
            for y in 0..t.h {
                for x in 0..t.w {
                    let i = (t.y + y) * region.w + t.x + x;
                    let w = weight(x, y, 320);
                    sum[i] += 0.37 * w;
                    ws[i] += w;
                }
            }
        }
        assert!(
            sum.iter()
                .zip(ws)
                .all(|(s, w)| w > 0. && (s / w - 0.37).abs() < 1e-5)
        );
    }
    #[test]
    fn only_boundary_tiles_require_inference() {
        let t = Region {
            x: 0,
            y: 0,
            w: 320,
            h: 320,
        };
        for p in [0., 0.01, 0.99, 1.] {
            assert!(!is_edge(&vec![p; 320 * 320], 320, t));
        }
        let mut map = vec![0.; 320 * 320];
        map[0] = 1.;
        assert!(is_edge(&map, 320, t));
    }
}
