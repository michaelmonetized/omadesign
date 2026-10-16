//! Float vector coverage and ordered paint within the authoritative layer compositor.
use super::{blend_pixel, coverage, paint::Paint};
use crate::{
    color::Blend,
    document::{Document, Fill, Layer, Shape, StrokeAlignment},
    geom::{Geom, Pt},
    resources::{self, CancellationCheck},
};
use tiny_skia::{Path, Transform};

fn supported(shape: &Shape) -> Result<(), String> {
    if shape.filters.active() {
        return Err(
            "Native vector gradients, patterns and effects require a float paint renderer".into(),
        );
    }
    let unsupported = |fill: &Fill| {
        matches!(
            fill,
            Fill::Pattern(_)
                | Fill::Gradient(crate::gradient::Gradient {
                    kind: crate::gradient::GradientKind::Shape
                        | crate::gradient::GradientKind::Conic,
                    ..
                })
        )
    };
    if unsupported(&shape.style.fill)
        || shape.style.stroke.as_ref().is_some_and(|stroke| {
            stroke.gradient.as_ref().is_some_and(|g| {
                matches!(
                    g.kind,
                    crate::gradient::GradientKind::Shape | crate::gradient::GradientKind::Conic
                )
            })
        })
    {
        return Err("Native vector patterns and shape-distance/conic gradients require a float paint renderer".into());
    }
    if !shape.artwork_masks.is_empty() || shape.mask.is_some() {
        return Err("Native vector masks and effects require a precise coverage renderer".into());
    }
    if shape.layout.frame || shape.layout.parent.is_some() || shape.layout.image.is_some() {
        return Err("Native vector frame clipping and placed shape images require a float coverage renderer".into());
    }
    if matches!(&shape.geom,Geom::Text(run) if run.svg_text.is_some() || run.frame.is_some() || run.on_path.is_some())
    {
        return Err("Native source, frame and path text require a float paint renderer preserving their layout tree".into());
    }
    if let Geom::Text(run) = &shape.geom {
        if run.spans.iter().any(|span| {
            span.fill
                .as_ref()
                .is_some_and(|fill| !matches!(fill, Fill::None | Fill::Solid(_)))
        }) || run.layout.as_ref().is_some_and(|layout| {
            layout.paint_ranges.iter().any(|(fill, _)| {
                fill.as_ref()
                    .is_some_and(|fill| !matches!(fill, Fill::None | Fill::Solid(_)))
            })
        }) {
            return Err("Native non-solid type span paint requires a float paint renderer preserving whole-object coordinates".into());
        }
    }
    if !shape.rotation.is_finite() || !shape.opacity.is_finite() || !shape.fill_opacity.is_finite()
    {
        return Err("Native vector appearance has nonfinite opacity or placement".into());
    }
    if let Some(stroke) = &shape.style.stroke {
        if !stroke.width.is_finite()
            || !stroke.dash_offset.is_finite()
            || stroke
                .dash
                .is_some_and(|(a, b)| !a.is_finite() || !b.is_finite())
        {
            return Err("Native vector stroke has nonfinite geometry".into());
        }
        if shape.geom.is_closed() && stroke.alignment != StrokeAlignment::Center {
            return Err(
                "Native inside/outside stroke alignment requires a float clip renderer".into(),
            );
        }
    }
    Ok(())
}

/// Render one unfiltered vector layer with native geometry coverage and one layer-opacity application.
/// Args: immutable document/layer, straight float destination, extent/scale and cancellation.
/// Returns after admitted intermediate buffers are composited, or a precise atomic refusal.
pub(super) fn render(
    doc: &Document,
    layer: &Layer,
    out: &mut [[f32; 4]],
    w: u32,
    h: u32,
    scale: u32,
    cancel: &dyn CancellationCheck,
) -> Result<(), String> {
    cancel.check()?;
    if layer.filters.active() {
        return Err(
            "Native vector gradients, patterns and effects require a float paint renderer".into(),
        );
    }
    let shapes = layer
        .kind
        .shapes()
        .ok_or("Native vector layer has no geometry")?;
    for shape in shapes {
        supported(shape)?;
    }
    if shapes
        .iter()
        .all(|s| !s.visible || s.guide || s.opacity <= 0.)
    {
        return Ok(());
    }
    let count = crate::deep_color::storage::count(w, h, 1)?;
    let mut pixels = resources::global().allocate_now::<[f32; 4], _>(count, cancel)?;
    for shape in shapes
        .iter()
        .filter(|s| s.visible && !s.guide && s.opacity > 0.)
    {
        cancel.check()?;
        let Some(path) = shape.get_cached_path(96) else {
            continue;
        };
        let bounds = path.bounds();
        let padding = shape
            .style
            .stroke
            .as_ref()
            .map_or(0., |s| s.width.max(0.) * 5.);
        let left = ((bounds.left() - padding) * scale as f32)
            .floor()
            .max(0.)
            .min(w as f32) as u32;
        let top = ((bounds.top() - padding) * scale as f32)
            .floor()
            .max(0.)
            .min(h as f32) as u32;
        let right = ((bounds.right() + padding) * scale as f32)
            .ceil()
            .max(0.)
            .min(w as f32) as u32;
        let bottom = ((bounds.bottom() + padding) * scale as f32)
            .ceil()
            .max(0.)
            .min(h as f32) as u32;
        if right <= left || bottom <= top {
            continue;
        }
        let (width, height) = (right - left, bottom - top);
        let mut item = resources::global().allocate_now::<[f32; 4], _>(
            crate::deep_color::storage::count(width, height, 1)?,
            cancel,
        )?;
        let transform = Transform::from_translate(-(left as f32), -(top as f32))
            .pre_scale(scale as f32, scale as f32);
        let mut draw = |path: &Path,
                        paint: &Paint,
                        nonzero: bool,
                        transform: Transform|
         -> Result<(), String> {
            coverage::rows(path, transform, nonzero, width, height, cancel, |y, row| {
                for (x, &coverage) in row.iter().enumerate() {
                    if x % 4096 == 0 {
                        cancel.check()?;
                    }
                    if coverage <= 0. {
                        continue;
                    }
                    let point = Pt::new(
                        (left as f32 + x as f32 + 0.5) / scale as f32,
                        (top as f32 + y as f32 + 0.5) / scale as f32,
                    );
                    let source = paint.sample(point)?;
                    let dest = &mut item.as_mut_slice()[y as usize * width as usize + x];
                    *dest =
                        blend_pixel(*dest, source, Blend::Normal, coverage.clamp(0., 1.) as f32);
                }
                Ok(())
            })
        };
        if let Geom::Text(run) = &shape.geom
            && crate::text::has_character_paint(run)
        {
            let center = shape.geom.bbox().center();
            let rotation =
                Transform::from_rotate_at(shape.rotation.to_degrees(), center.x, center.y);
            for (fill, path) in crate::text::paint_paths(run, &shape.style.fill) {
                if !fill.is_none() {
                    let paint =
                        Paint::new(&fill, shape.geom.bbox(), shape.rotation, &doc.color, cancel)?;
                    draw(&path, &paint, true, transform.pre_concat(rotation))?;
                }
            }
        } else if shape.geom.is_closed() && !shape.style.fill.is_none() {
            let paint = Paint::new(
                &shape.style.fill,
                shape.geom.bbox(),
                shape.rotation,
                &doc.color,
                cancel,
            )?;
            draw(&path, &paint, shape.geom.nonzero_winding(), transform)?;
        }
        if let Some(stroke) = &shape.style.stroke
            && stroke.width > 0.
        {
            let dashed;
            let path = if let Some((on, off)) = stroke.dash {
                let dash = tiny_skia::StrokeDash::new(vec![on, off], stroke.dash_offset)
                    .ok_or("Native stroke dash is invalid")?;
                dashed = path
                    .dash(&dash, scale as f32)
                    .ok_or("Native stroke dash geometry could not be prepared")?;
                &dashed
            } else {
                path.as_ref()
            };
            let outline = path
                .stroke(
                    &tiny_skia::Stroke {
                        width: stroke.width,
                        line_cap: stroke.cap.to_skia(),
                        line_join: stroke.join.to_skia(),
                        ..Default::default()
                    },
                    scale as f32,
                )
                .ok_or("Native stroke outline could not be prepared")?;
            let paint = if let Some(gradient) = &stroke.gradient {
                Paint::from_gradient(
                    gradient,
                    shape.geom.bbox(),
                    shape.rotation,
                    &doc.color,
                    cancel,
                )?
            } else {
                Paint::new(
                    &Fill::Solid(stroke.color),
                    shape.geom.bbox(),
                    shape.rotation,
                    &doc.color,
                    cancel,
                )?
            };
            draw(&outline, &paint, true, transform)?;
        }
        for y in 0..height {
            cancel.check()?;
            for x in 0..width {
                let dst = &mut pixels.as_mut_slice()
                    [(top + y) as usize * w as usize + (left + x) as usize];
                let source = item[y as usize * width as usize + x as usize];
                *dst = blend_pixel(
                    *dst,
                    source,
                    shape.blend,
                    (shape.opacity * shape.fill_opacity).clamp(0., 1.),
                );
            }
        }
    }
    for (i, (dst, &source)) in out.iter_mut().zip(pixels.iter()).enumerate() {
        if i % 4096 == 0 {
            cancel.check()?;
        }
        let point = Pt::new(
            ((i % w as usize) as f32 + 0.5) / scale as f32,
            ((i / w as usize) as f32 + 0.5) / scale as f32,
        );
        *dst = blend_pixel(
            *dst,
            source,
            layer.blend,
            layer.opacity * layer.fill_opacity * super::mask(layer, point, None),
        );
    }
    cancel.check()
}
