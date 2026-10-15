use super::Depth;
use crate::{
    color::Blend,
    document::{Document, Layer, Pixels},
    geom::Pt,
};

/// Composite straight RGBA in floating-point precision.
/// Args: backdrop, source, blend mode and source coverage. Returns straight RGBA.
pub fn blend_pixel(b: [f32; 4], mut s: [f32; 4], mode: Blend, coverage: f32) -> [f32; 4] {
    let ab = b[3].clamp(0., 1.);
    s[3] = (s[3] * coverage).clamp(0., 1.);
    let a = s[3] + ab * (1. - s[3]);
    if a <= 0. {
        return [0.; 4];
    }
    let bc = [b[0], b[1], b[2]];
    let sc = [s[0], s[1], s[2]];
    let mixed = match mode {
        Blend::Hue => set_lum(set_sat(sc, sat(bc)), lum(bc)),
        Blend::Saturation => set_lum(set_sat(bc, sat(sc)), lum(bc)),
        Blend::Color => set_lum(sc, lum(bc)),
        Blend::Luminosity => set_lum(bc, lum(sc)),
        _ => std::array::from_fn(|c| match mode {
            Blend::Normal => s[c],
            Blend::Multiply => b[c] * s[c],
            Blend::Screen => b[c] + s[c] - b[c] * s[c],
            Blend::Overlay => hard_light(s[c], b[c]),
            Blend::HardLight => hard_light(b[c], s[c]),
            Blend::Darken => b[c].min(s[c]),
            Blend::Lighten => b[c].max(s[c]),
            Blend::ColorDodge => {
                if b[c] <= 0. {
                    0.
                } else if s[c] >= 1. {
                    1.
                } else {
                    (b[c] / (1. - s[c])).min(1.)
                }
            }
            Blend::ColorBurn => {
                if b[c] >= 1. {
                    1.
                } else if s[c] <= 0. {
                    0.
                } else {
                    1. - ((1. - b[c]) / s[c]).min(1.)
                }
            }
            Blend::SoftLight => {
                if s[c] <= 0.5 {
                    b[c] - (1. - 2. * s[c]) * b[c] * (1. - b[c])
                } else {
                    let d = if b[c] <= 0.25 {
                        ((16. * b[c] - 12.) * b[c] + 4.) * b[c]
                    } else {
                        b[c].sqrt()
                    };
                    b[c] + (2. * s[c] - 1.) * (d - b[c])
                }
            }
            Blend::Difference => (b[c] - s[c]).abs(),
            Blend::Exclusion => b[c] + s[c] - 2. * b[c] * s[c],
            _ => s[c],
        }),
    };
    let mut out = [0., 0., 0., a];
    for c in 0..3 {
        out[c] = (s[3] * (1. - ab) * s[c] + s[3] * ab * mixed[c] + (1. - s[3]) * ab * b[c]) / a;
    }
    out
}
fn hard_light(b: f32, s: f32) -> f32 {
    if s <= 0.5 {
        2. * b * s
    } else {
        1. - 2. * (1. - b) * (1. - s)
    }
}
fn lum(c: [f32; 3]) -> f32 {
    c[0] * 0.3 + c[1] * 0.59 + c[2] * 0.11
}
fn sat(c: [f32; 3]) -> f32 {
    c.into_iter().fold(f32::NEG_INFINITY, f32::max) - c.into_iter().fold(f32::INFINITY, f32::min)
}
fn set_lum(mut c: [f32; 3], l: f32) -> [f32; 3] {
    let d = l - lum(c);
    for v in &mut c {
        *v += d;
    }
    let n = c.into_iter().fold(f32::INFINITY, f32::min);
    let x = c.into_iter().fold(f32::NEG_INFINITY, f32::max);
    if n < 0. {
        for v in &mut c {
            *v = l + (*v - l) * l / (l - n);
        }
    }
    if x > 1. {
        for v in &mut c {
            *v = l + (*v - l) * (1. - l) / (x - l);
        }
    }
    c
}
fn set_sat(c: [f32; 3], s: f32) -> [f32; 3] {
    let lo = c.into_iter().fold(f32::INFINITY, f32::min);
    let hi = c.into_iter().fold(f32::NEG_INFINITY, f32::max);
    c.map(|v| {
        if hi > lo {
            (v - lo) * s / (hi - lo)
        } else {
            0.
        }
    })
}

/// Render document pixels without an integer color intermediate.
/// Args: document and integer export scale. Returns an authoritative float RGBA buffer.
pub fn composite(doc: &Document, scale: u32) -> Result<Pixels, String> {
    doc.validate_hierarchy()?;
    if doc
        .layers
        .iter()
        .filter_map(|l| l.kind.shapes())
        .flatten()
        .any(|s| s.mask.as_ref().is_some_and(|m| m.native.is_some()))
    {
        return Err("Native vector-shape masks need a precise coverage renderer; save .oma to preserve their original samples.".into());
    }
    let scale = scale.clamp(1, 8);
    let w = (doc.width * scale as f32).round() as u32;
    let h = (doc.height * scale as f32).round() as u32;
    let count = super::storage::count(w, h, 4)?;
    let mut out =
        crate::resources::global().allocate_now::<f32, _>(count, &super::storage::check)?;
    if !doc.transparent {
        out.as_mut_slice().fill(1.);
    }
    render(
        doc,
        None,
        out.as_mut_slice().as_chunks_mut::<4>().0,
        w,
        h,
        scale,
        0,
    )?;
    Pixels::from_sample_allocation(w, h, Depth::F32, std::sync::Arc::new(out))
}
fn render(
    doc: &Document,
    parent: Option<u64>,
    out: &mut [[f32; 4]],
    w: u32,
    h: u32,
    scale: u32,
    depth: usize,
) -> Result<(), String> {
    if depth >= 64 {
        return Err("Native composite group depth exceeds limit".into());
    }
    for layer in doc
        .layers
        .iter()
        .filter(|l| l.parent == parent && l.visible)
    {
        if layer.opacity <= 0. {
            continue;
        }
        if !layer.artwork_masks.is_empty() {
            return Err(
                "High-depth live artwork mask rendering requires explicit raster conversion".into(),
            );
        }
        if layer.is_group {
            if layer.filters.active() {
                return Err("High-depth group effects require explicit raster conversion".into());
            }
            let pass = layer.pass_through;
            let mut temp = if pass {
                crate::resources::global().copy_now(out, &super::storage::check)?
            } else {
                crate::resources::global()
                    .allocate_now::<[f32; 4], _>(out.len(), &super::storage::check)?
            };
            render(
                doc,
                Some(layer.id),
                temp.as_mut_slice(),
                w,
                h,
                scale,
                depth + 1,
            )?;
            for (i, (dst, src)) in out.iter_mut().zip(temp.iter().copied()).enumerate() {
                let p = Pt::new(
                    (i % w as usize) as f32 / scale as f32,
                    (i / w as usize) as f32 / scale as f32,
                );
                let amount = layer.opacity * layer.fill_opacity * mask(layer, p, None);
                if pass {
                    let a = dst[3] * (1. - amount) + src[3] * amount;
                    for c in 0..3 {
                        dst[c] = if a > 0. {
                            (dst[c] * dst[3] * (1. - amount) + src[c] * src[3] * amount) / a
                        } else {
                            0.
                        };
                    }
                    dst[3] = a;
                } else {
                    *dst = blend_pixel(*dst, src, layer.blend, amount);
                }
            }
        } else if let Some(pixels) = layer.kind.pixels() {
            if layer.filters.active() {
                return Err("Apply pixel effects before high-depth export; native layer effect rendering is not yet supported".into());
            }
            let Some(inv) = crate::compositor::layer_pixel_transform(layer).invert() else {
                continue;
            };
            for (i, dst) in out.iter_mut().enumerate() {
                let p = Pt::new(
                    (i % w as usize) as f32 / scale as f32 + 0.5 / scale as f32,
                    (i / w as usize) as f32 / scale as f32 + 0.5 / scale as f32,
                );
                let mut local = tiny_skia::Point::from_xy(p.x, p.y);
                inv.map_point(&mut local);
                let src = sample(pixels, local.x, local.y);
                *dst = blend_pixel(
                    *dst,
                    src,
                    layer.blend,
                    layer.opacity * layer.fill_opacity * mask(layer, p, Some((local.x, local.y))),
                );
            }
        } else {
            if layer.filters.active() || layer.kind.shapes().is_some_and(|shapes| shapes.iter().any(|shape| {
                shape.filters.active()
                    || !matches!(&shape.style.fill, crate::document::Fill::None | crate::document::Fill::Solid(_))
                    || shape.style.stroke.as_ref().is_some_and(|stroke| stroke.gradient.is_some())
            })) {
                return Err("Native vector gradients, patterns and effects require a float paint renderer. Save .oma to retain the authored document; use an explicit 8-bit copy for this export.".into());
            }
            let mut single = Document::new("Native vector coverage", 1., 1., doc.dpi);
            single.width = doc.width;
            single.height = doc.height;
            single.artboards = doc.artboards.clone();
            single.transparent = true;
            let mut vector = layer.clone();
            vector.parent = None;
            vector.opacity = 1.;
            vector.fill_opacity = 1.;
            vector.mask = None;
            vector.blend = Blend::Normal;
            single.layers = vec![vector];
            let pm = crate::compositor::render_export(&single, scale)?;
            let pixels = Pixels::try_from_pixmap(&pm)?;
            for (i, dst) in out.iter_mut().enumerate() {
                let p = Pt::new(
                    ((i % w as usize) as f32 + 0.5) / scale as f32,
                    ((i / w as usize) as f32 + 0.5) / scale as f32,
                );
                *dst = blend_pixel(
                    *dst,
                    pixels.sample(i),
                    layer.blend,
                    layer.opacity * layer.fill_opacity * mask(layer, p, None),
                );
            }
        }
    }
    Ok(())
}
pub(super) fn sample(pixels: &Pixels, x: f32, y: f32) -> [f32; 4] {
    if x < 0. || y < 0. || x >= pixels.w as f32 || y >= pixels.h as f32 {
        return [0.; 4];
    }
    let x = x - 0.5;
    let y = y - 0.5;
    let ix = x.floor() as i32;
    let iy = y.floor() as i32;
    let mut out = [0.; 4];
    for (dx, wx) in [
        (0, 1. - x.fract().rem_euclid(1.)),
        (1, x.fract().rem_euclid(1.)),
    ] {
        for (dy, wy) in [
            (0, 1. - y.fract().rem_euclid(1.)),
            (1, y.fract().rem_euclid(1.)),
        ] {
            let px = (ix + dx).clamp(0, pixels.w as i32 - 1) as usize;
            let py = (iy + dy).clamp(0, pixels.h as i32 - 1) as usize;
            let p = pixels.sample(py * pixels.w as usize + px);
            let a = p[3].clamp(0., 1.);
            for c in 0..3 {
                out[c] += p[c] * a * wx * wy;
            }
            out[3] += a * wx * wy;
        }
    }
    if out[3] > 0. {
        for c in 0..3 {
            out[c] /= out[3];
        }
    }
    out
}
fn mask(layer: &Layer, p: Pt, local: Option<(f32, f32)>) -> f32 {
    let Some(mask) = &layer.mask else {
        return 1.;
    };
    let (x, y) = if let Some(local) = local {
        local
    } else if layer.mask_size.x > 0. && layer.mask_size.y > 0. {
        (
            (p.x - layer.mask_origin.x) * mask.w as f32 / layer.mask_size.x,
            (p.y - layer.mask_origin.y) * mask.h as f32 / layer.mask_size.y,
        )
    } else {
        (p.x, p.y)
    };
    let m = sample(mask, x, y);
    ((m[0] * 0.2126 + m[1] * 0.7152 + m[2] * 0.0722) * m[3]).clamp(0., 1.)
}
