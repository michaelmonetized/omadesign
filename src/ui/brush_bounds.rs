use crate::app::Studio;
use crate::deep_color::surface::Surface;
use crate::document::{LayerKind, Pixels};
use crate::geom::{Bounds, Pt};
use tiny_skia::Point;

const MAX_PIXELS: u64 = 64_000_000;

/// Copy a pixel grid into new bounds without resampling.
/// Takes RGBA bytes, old/new dimensions, and the old grid's new offset;
/// returns transparent-padded bytes, or None if allocation fails.
fn reframe(
    data: &[u8],
    old: (u32, u32),
    new: (u32, u32),
    offset: (i64, i64),
) -> Option<crate::resources::Allocation<u8>> {
    let len = (new.0 as usize)
        .checked_mul(new.1 as usize)?
        .checked_mul(4)?;
    let mut output = crate::resources::global()
        .allocate_now::<u8, _>(len, &crate::deep_color::storage::check)
        .ok()?;
    let left = offset.0.max(0);
    let right = (offset.0 + i64::from(old.0)).min(i64::from(new.0));
    if left >= right {
        return Some(output);
    }
    for y in offset.1.max(0)..(offset.1 + i64::from(old.1)).min(i64::from(new.1)) {
        let source = ((y - offset.1) as usize * old.0 as usize + (left - offset.0) as usize) * 4;
        let target = (y as usize * new.0 as usize + left as usize) * 4;
        let count = (right - left) as usize * 4;
        output.as_mut_slice()[target..target + count]
            .copy_from_slice(&data[source..source + count]);
    }
    Some(output)
}

/// Make room for a brush segment, keeping existing pixels at their world positions.
/// Takes the studio, target layer, world endpoints, working overlay/preview, and
/// whether this is the first dab; returns an error if safe growth is impossible.
pub(super) fn ensure(
    studio: &mut Studio,
    layer: usize,
    from: Pt,
    to: Pt,
    overlay: &mut Surface,
    preview: &mut Option<Surface>,
    first: bool,
) -> Result<(), &'static str> {
    let from = studio.mask_point(layer, from);
    let to = studio.mask_point(layer, to);
    if ![from.x, from.y, to.x, to.y].iter().all(|v| v.is_finite()) {
        return Err("Brush position is outside the supported pixel range");
    }
    let radius = studio.brush.stroke_radius();
    let mut bounds = [
        (from.x.min(to.x) - radius).floor(),
        (from.y.min(to.y) - radius).floor(),
        (from.x.max(to.x) + radius).ceil(),
        (from.y.max(to.y) + radius).ceil(),
    ];
    if !bounds
        .iter()
        .all(|v| v.is_finite() && v.abs() < i32::MAX as f32)
    {
        return Err("Brush position is outside the supported pixel range");
    }
    let target = &studio.doc.layers[layer];
    let pixels = target
        .kind
        .pixels()
        .ok_or("Choose a pixel layer to paint")?;
    if studio.pixel_sel.is_some() {
        let transform = crate::compositor::layer_pixel_transform(target);
        let Some(source) = studio.pixel_sel_space else {
            return Ok(());
        };
        if source.w == pixels.w && source.h == pixels.h && source.transform == transform {
            return Ok(());
        }
        let inverse = transform
            .invert()
            .ok_or("This pixel layer has no paintable area")?;
        let mut corners = [
            Point::from_xy(0., 0.),
            Point::from_xy(source.w as f32, 0.),
            Point::from_xy(0., source.h as f32),
            Point::from_xy(source.w as f32, source.h as f32),
        ];
        inverse
            .pre_concat(source.transform)
            .map_points(&mut corners);
        let mut selected = Bounds::from_pt(Pt::new(corners[0].x, corners[0].y));
        for corner in &corners[1..] {
            selected.union_pt(Pt::new(corner.x, corner.y));
        }
        bounds[0] = bounds[0].max(selected.min.x.floor() - 1.);
        bounds[1] = bounds[1].max(selected.min.y.floor() - 1.);
        bounds[2] = bounds[2].min(selected.max.x.ceil() + 1.);
        bounds[3] = bounds[3].min(selected.max.y.ceil() + 1.);
        if bounds[0] >= bounds[2] || bounds[1] >= bounds[3] {
            return Ok(());
        }
    }
    if bounds[0] >= 0.0
        && bounds[1] >= 0.0
        && bounds[2] <= pixels.w as f32
        && bounds[3] <= pixels.h as f32
    {
        return Ok(());
    }
    let rebase =
        first && target.mask.is_none() && studio.pixel_sel.is_none() && pixels.is_invisible();
    let (mut left, mut top, mut right, mut bottom) = (
        bounds[0] as i64,
        bounds[1] as i64,
        bounds[2] as i64,
        bounds[3] as i64,
    );
    if !rebase {
        left = left.min(0);
        top = top.min(0);
        right = right.max(i64::from(
            pixels.w.max(target.mask.as_ref().map_or(0, |m| m.w)),
        ));
        bottom = bottom.max(i64::from(
            pixels.h.max(target.mask.as_ref().map_or(0, |m| m.h)),
        ));
    }
    let exact = (left, top, right, bottom);
    if left < 0 || rebase {
        left = left.div_euclid(64) * 64;
    }
    if top < 0 || rebase {
        top = top.div_euclid(64) * 64;
    }
    if right > i64::from(pixels.w) || rebase {
        right = (right + 63).div_euclid(64) * 64;
    }
    if bottom > i64::from(pixels.h) || rebase {
        bottom = (bottom + 63).div_euclid(64) * 64;
    }
    if pixels.native.is_none() && (right - left) as u64 * (bottom - top) as u64 > MAX_PIXELS {
        (left, top, right, bottom) = exact;
    }
    if (right - left) as u64 * (bottom - top) as u64 > MAX_PIXELS {
        return Err("Pixel layer reached 64 megapixels. Add a pixel layer to keep painting here");
    }
    let (w, h) = (
        u32::try_from(right - left).map_err(|_| "Pixel width overflows")?,
        u32::try_from(bottom - top).map_err(|_| "Pixel height overflows")?,
    );
    let offset = (-left, -top);
    let allocation = "Not enough memory to extend this pixel layer";
    let resized = if pixels.native.is_some() {
        pixels
            .reframed(w, h, offset.0, offset.1)
            .map_err(|_| allocation)?
    } else {
        Pixels::from_byte_allocation(
            w,
            h,
            std::sync::Arc::new(
                reframe(&pixels.data, (pixels.w, pixels.h), (w, h), offset).ok_or(allocation)?,
            ),
        )
        .map_err(|_| allocation)?
    };
    let overlay_data = reframe(
        overlay.data(),
        (overlay.width(), overlay.height()),
        (w, h),
        offset,
    )
    .ok_or(allocation)?;
    let resized_overlay = Surface::from_allocation(w, h, overlay_data).map_err(|_| allocation)?;
    let resized_preview = if let Some(pm) = preview {
        let data =
            reframe(pm.data(), (pm.width(), pm.height()), (w, h), offset).ok_or(allocation)?;
        Some(Surface::from_allocation(w, h, data).map_err(|_| allocation)?)
    } else {
        None
    };
    let mask = if let Some(mask) = &target.mask {
        Some(if mask.native.is_some() {
            mask.reframed(w, h, offset.0, offset.1)
                .map_err(|_| allocation)?
        } else {
            Pixels::from_byte_allocation(
                w,
                h,
                std::sync::Arc::new(
                    reframe(&mask.data, (mask.w, mask.h), (w, h), offset).ok_or(allocation)?,
                ),
            )
            .map_err(|_| allocation)?
        })
    } else {
        None
    };
    let original_transform = crate::compositor::layer_pixel_transform(target);
    let (_, dimensions, _) = target.kind.raster_xform().ok_or(allocation)?;
    let new_size = Pt::new(
        dimensions.x * w as f32 / pixels.w as f32,
        dimensions.y * h as f32 / pixels.h as f32,
    );
    let target = &mut studio.doc.layers[layer];
    if let LayerKind::Raster {
        pixels,
        size,
        shear,
        ..
    } = &mut target.kind
    {
        *pixels = resized;
        *shear = *shear / dimensions.y.max(1.0) * new_size.y.max(1.0);
        *size = new_size;
    }
    let mut old_corner = Point::from_xy(left as f32, top as f32);
    original_transform.map_point(&mut old_corner);
    let mut new_corner = Point::from_xy(0.0, 0.0);
    crate::compositor::layer_pixel_transform(target).map_point(&mut new_corner);
    if let LayerKind::Raster { origin, .. } = &mut target.kind {
        *origin = *origin + Pt::new(old_corner.x - new_corner.x, old_corner.y - new_corner.y);
    }
    target.mask = mask;
    *overlay = resized_overlay;
    *preview = resized_preview;
    studio.mark();
    Ok(())
}
