//! Float vector coverage and ordered paint within the authoritative layer compositor.
use super::{blend_pixel, coverage, paint::Paint, shape_mask::ShapeMask};
use crate::{
    color::Blend,
    document::{Document, Fill, Layer, Shape, StrokeAlignment},
    geom::{Geom, Pt},
    resources::{self, CancellationCheck},
};
use tiny_skia::{Path, Transform};

fn supported(shape: &Shape, cancel: &dyn CancellationCheck) -> Result<(), String> {
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
    if !shape.artwork_masks.is_empty() {
        return Err("Native live artwork masks require a precise coverage renderer".into());
    }
    if shape.layout.image.is_some() {
        return Err("Native placed shape images require a float coverage renderer".into());
    }
    if shape.layout.frame
        && (shape.opacity != 1.
            || shape.fill_opacity != 1.
            || shape.blend != Blend::Normal
            || shape.mask.is_some())
    {
        return Err(
            "Native frame subtree opacity, blend and masks require float appearance isolation"
                .into(),
        );
    }
    if let Some(mask) = &shape.mask {
        ShapeMask::validate(mask, cancel)?;
        if (!shape.style.fill.is_none()
            && shape.style.stroke.as_ref().is_some_and(|s| s.width > 0.))
            || matches!(&shape.geom,Geom::Text(run) if crate::text::has_character_paint(run))
        {
            return Err("Masked fill/stroke and character-paint combinations require joint float appearance isolation".into());
        }
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
        supported(shape, cancel)?;
    }
    if shapes
        .iter()
        .any(|s| s.layout.frame || s.layout.parent.is_some())
    {
        let mut ids = resources::global().allocate_now::<u64, _>(shapes.len(), cancel)?;
        for (target, shape) in ids.as_mut_slice().iter_mut().zip(shapes) {
            *target = shape.id;
        }
        ids.as_mut_slice().sort_unstable();
        if ids.windows(2).any(|p| p[0] == p[1]) {
            return Err("Native frame clips require unique shape identifiers".into());
        }
    }
    if shapes
        .iter()
        .all(|s| !s.visible || s.guide || s.opacity <= 0.)
    {
        return Ok(());
    }
    let count = crate::deep_color::storage::count(w, h, 1)?;
    let mut pixels = resources::global().allocate_now::<[f32; 4], _>(count, cancel)?;
    let layer_index = doc
        .layers
        .iter()
        .position(|l| l.id == layer.id)
        .ok_or("Missing native vector layer")?;
    let mut clips: [Option<coverage::Clip>; 64] = std::array::from_fn(|_| None);
    draw_children(
        doc,
        layer_index,
        shapes,
        None,
        pixels.as_mut_slice(),
        &mut clips,
        0,
        0,
        w,
        h,
        scale,
        cancel,
    )?;
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

/// Traverse shared frame ownership in painter order without averaging geometric masks.
/// Args: original document/shapes, parent ID, admitted layer, bounded clip stack, extent and cancellation.
/// Returns after painting visible subtrees or a named depth/admission error.
fn draw_children(
    doc: &Document,
    layer_index: usize,
    shapes: &[Shape],
    parent: Option<u64>,
    pixels: &mut [[f32; 4]],
    clips: &mut [Option<coverage::Clip>; 64],
    clip_count: usize,
    depth: usize,
    w: u32,
    h: u32,
    scale: u32,
    cancel: &dyn CancellationCheck,
) -> Result<(), String> {
    if depth > 64 {
        return Err("Native frame traversal exceeds the supported hierarchy depth".into());
    }
    for shape in shapes
        .iter()
        .filter(|s| s.layout.parent == parent && s.visible && !s.guide && s.opacity > 0.)
    {
        cancel.check()?;
        paint_shape(
            doc,
            layer_index,
            shape,
            pixels,
            &clips[..clip_count],
            w,
            h,
            scale,
            cancel,
        )?;
        if !shape.layout.frame {
            continue;
        }
        let mut count = clip_count;
        if shape.layout.clip {
            if count == clips.len() {
                return Err("Native frame clips exceed the supported hierarchy depth".into());
            }
            let Some(path) = shape.get_cached_path(96) else {
                continue;
            };
            clips[count] = Some(coverage::Clip {
                path,
                transform: crate::compositor::shape_parent_transform(doc, layer_index, shape.id),
                nonzero: true,
            });
            count += 1;
        }
        draw_children(
            doc,
            layer_index,
            shapes,
            Some(shape.id),
            pixels,
            clips,
            count,
            depth + 1,
            w,
            h,
            scale,
            cancel,
        )?;
        if count > clip_count {
            clips[clip_count] = None;
        }
    }
    Ok(())
}

/// Paint one shape through jointly intersected frame paths and original bitmap samples.
/// Args: shared placement, admitted layer, frame clip stack, extent and cancellation. Returns after one shape appearance composite.
fn paint_shape(
    doc: &Document,
    layer_index: usize,
    shape: &Shape,
    pixels: &mut [[f32; 4]],
    clips: &[Option<coverage::Clip>],
    w: u32,
    h: u32,
    scale: u32,
    cancel: &dyn CancellationCheck,
) -> Result<(), String> {
    cancel.check()?;
    if shape.style.fill.is_none()
        && !shape.style.stroke.as_ref().is_some_and(|s| s.width > 0.)
        && !matches!(&shape.geom,Geom::Text(run) if crate::text::has_character_paint(run))
    {
        return Ok(());
    }
    let Some(path) = shape.get_cached_path(96) else {
        return Ok(());
    };
    let bounds = path.bounds();
    let parent = crate::compositor::shape_parent_transform(doc, layer_index, shape.id);
    let inverse_parent = parent
        .invert()
        .ok_or("Native frame placement is singular")?;
    let padding = shape
        .style
        .stroke
        .as_ref()
        .map_or(0., |s| s.width.max(0.) * 5.);
    let mut corners = [
        tiny_skia::Point::from_xy(bounds.left() - padding, bounds.top() - padding),
        tiny_skia::Point::from_xy(bounds.right() + padding, bounds.top() - padding),
        tiny_skia::Point::from_xy(bounds.right() + padding, bounds.bottom() + padding),
        tiny_skia::Point::from_xy(bounds.left() - padding, bounds.bottom() + padding),
    ];
    parent.map_points(&mut corners);
    if corners.iter().any(|p| !p.x.is_finite() || !p.y.is_finite()) {
        return Err("Native frame placement has nonfinite bounds".into());
    }
    let left = corners
        .iter()
        .map(|p| p.x)
        .fold(f32::INFINITY, f32::min)
        .mul_add(scale as f32, 0.)
        .floor()
        .clamp(0., w as f32) as u32;
    let right = corners
        .iter()
        .map(|p| p.x)
        .fold(f32::NEG_INFINITY, f32::max)
        .mul_add(scale as f32, 0.)
        .ceil()
        .clamp(0., w as f32) as u32;
    let top = corners
        .iter()
        .map(|p| p.y)
        .fold(f32::INFINITY, f32::min)
        .mul_add(scale as f32, 0.)
        .floor()
        .clamp(0., h as f32) as u32;
    let bottom = corners
        .iter()
        .map(|p| p.y)
        .fold(f32::NEG_INFINITY, f32::max)
        .mul_add(scale as f32, 0.)
        .ceil()
        .clamp(0., h as f32) as u32;
    if right <= left || bottom <= top {
        return Ok(());
    }
    let (width, height) = (right - left, bottom - top);
    let mut item = resources::global().allocate_now::<[f32; 4], _>(
        crate::deep_color::storage::count(width, height, 1)?,
        cancel,
    )?;
    let view = Transform::from_translate(-(left as f32), -(top as f32))
        .pre_scale(scale as f32, scale as f32);
    let transform = view.pre_concat(parent);
    let mask = shape
        .mask
        .as_ref()
        .map(|pixels| {
            ShapeMask::new(
                pixels,
                transform.pre_concat(crate::compositor::shape_mask_transform(shape)),
            )
        })
        .transpose()?;
    let mut draw =
        |path: &Path, paint: &Paint, nonzero: bool, transform: Transform| -> Result<(), String> {
            let consume = |y: u32, row: &[f64]| {
                for (x, &coverage) in row.iter().enumerate() {
                    if x % 4096 == 0 {
                        cancel.check()?;
                    }
                    if coverage <= 0. {
                        continue;
                    }
                    let mut point = tiny_skia::Point::from_xy(
                        (left as f32 + x as f32 + 0.5) / scale as f32,
                        (top as f32 + y as f32 + 0.5) / scale as f32,
                    );
                    inverse_parent.map_point(&mut point);
                    let source = paint.sample(Pt::new(point.x, point.y))?;
                    let dest = &mut item.as_mut_slice()[y as usize * width as usize + x];
                    *dest =
                        blend_pixel(*dest, source, Blend::Normal, coverage.clamp(0., 1.) as f32);
                }
                Ok(())
            };
            if clips.is_empty() && mask.is_none() {
                coverage::rows(path, transform, nonzero, width, height, cancel, consume)
            } else {
                coverage::rows_clipped(
                    path,
                    transform,
                    nonzero,
                    clips,
                    view,
                    mask.as_ref(),
                    width,
                    height,
                    cancel,
                    consume,
                )
            }
        };
    if let Geom::Text(run) = &shape.geom
        && crate::text::has_character_paint(run)
    {
        let center = shape.geom.bbox().center();
        let rotation = Transform::from_rotate_at(shape.rotation.to_degrees(), center.x, center.y);
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
            let dst = &mut pixels[(top + y) as usize * w as usize + (left + x) as usize];
            let source = item[y as usize * width as usize + x as usize];
            *dst = blend_pixel(
                *dst,
                source,
                shape.blend,
                (shape.opacity * shape.fill_opacity).clamp(0., 1.),
            );
        }
    }

    Ok(())
}
