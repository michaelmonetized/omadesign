//! Restrict final isolated-surface blits without changing their sampling domain.
use crate::color::Blend;
use tiny_skia::{
    FilterQuality, IntRect, Paint, Pattern, Pixmap, PixmapPaint, PixmapRef, SpreadMode, Transform,
};

/// Bounds of every nonzero channel, including unusual RGB with zero alpha.
/// Scan only completed surfaces: filters must retain their original domain.
pub(super) fn bounds(source: PixmapRef<'_>) -> Option<IntRect> {
    let width = source.width() as usize;
    let height = source.height() as usize;
    let pixels = source.data().as_chunks::<4>().0;
    if pixels[0] != [0; 4] && pixels[pixels.len() - 1] != [0; 4] {
        return IntRect::from_xywh(0, 0, width as u32, height as u32);
    }
    let (mut left, mut top, mut right, mut bottom) = (width, height, 0, 0);
    for (y, row) in pixels.chunks_exact(width).enumerate() {
        let Some(first) = row.iter().position(|pixel| *pixel != [0; 4]) else {
            continue;
        };
        left = left.min(first);
        top = top.min(y);
        bottom = y + 1;
        if right < width {
            right = right.max(row.iter().rposition(|pixel| *pixel != [0; 4]).unwrap() + 1);
        }
    }
    IntRect::from_ltrb(left as i32, top as i32, right as i32, bottom as i32)
}

pub(super) fn draw(
    destination: &mut Pixmap,
    source: PixmapRef<'_>,
    bounds: Option<IntRect>,
    opacity: f32,
    blend: Blend,
    placement: Transform,
) {
    // Exhaustive on purpose: new modes must establish that a zero source leaves
    // the destination unchanged. Source/Clear/DestinationIn would be unsafe.
    match blend {
        Blend::Normal
        | Blend::Multiply
        | Blend::Screen
        | Blend::Overlay
        | Blend::Darken
        | Blend::Lighten
        | Blend::ColorDodge
        | Blend::ColorBurn
        | Blend::HardLight
        | Blend::SoftLight
        | Blend::Difference
        | Blend::Exclusion
        | Blend::Hue
        | Blend::Saturation
        | Blend::Color
        | Blend::Luminosity => {}
    }
    let paint = PixmapPaint {
        opacity: opacity.clamp(0.0, 1.0),
        blend_mode: blend.to_skia(),
        ..Default::default()
    };
    if placement.sx != 1.0
        || placement.sy != 1.0
        || placement.kx != 0.0
        || placement.ky != 0.0
        || !placement.tx.is_finite()
        || !placement.ty.is_finite()
        || placement.tx.fract() != 0.0
        || placement.ty.fract() != 0.0
    {
        destination.draw_pixmap(0, 0, source, &paint, placement, None);
        return;
    }
    let Some(bounds) = bounds else {
        return;
    };
    // This is tiny-skia's draw_pixmap paint, with its full original source and
    // transform. Only the integer fill domain changes; no pixel copy, new clamp
    // edge, fractional coordinate shift, or filter-resolution change is involved.
    destination.fill_rect(
        bounds.to_rect(),
        &Paint {
            shader: Pattern::new(
                source,
                SpreadMode::Pad,
                FilterQuality::Nearest,
                paint.opacity,
                Transform::identity(),
            ),
            blend_mode: paint.blend_mode,
            anti_alias: false,
            ..Default::default()
        },
        placement,
        None,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn random_pixels(width: u32, height: u32) -> Pixmap {
        let mut out = Pixmap::new(width, height).unwrap();
        let mut state = 0x638ac591_u32;
        for pixel in out.data_mut().as_chunks_mut::<4>().0 {
            state = state.wrapping_mul(1664525).wrapping_add(1013904223);
            let alpha = (state >> 24) as u8;
            *pixel = [
                (state as u8).min(alpha),
                ((state >> 8) as u8).min(alpha),
                ((state >> 16) as u8).min(alpha),
                alpha,
            ];
        }
        out
    }

    #[test]
    fn coverage_includes_edge_pixels_and_zero_alpha_rgb() {
        let mut source = Pixmap::new(31, 19).unwrap();
        assert_eq!(bounds(source.as_ref()), None);
        let index = (7 * 31 + 13) * 4;
        source.data_mut()[index..index + 4].copy_from_slice(&[17, 0, 0, 0]);
        assert_eq!(bounds(source.as_ref()), IntRect::from_xywh(13, 7, 1, 1));
        source.data_mut()[0] = 1;
        let last = source.data().len() - 1;
        source.data_mut()[last] = 1;
        assert_eq!(bounds(source.as_ref()), IntRect::from_xywh(0, 0, 31, 19));
    }

    #[test]
    fn all_app_blends_match_full_surface_with_opacity_and_integer_clipping() {
        let mut cases = vec![Pixmap::new(37, 29).unwrap(), random_pixels(37, 29)];
        let mut sparse = Pixmap::new(37, 29).unwrap();
        for (x, y, color) in [
            (6, 8, [3, 1, 2, 4]),
            (21, 19, [17, 31, 9, 47]),
            (10, 11, [0, 0, 0, 255]),
        ] {
            let index = (y * 37 + x) * 4;
            sparse.data_mut()[index..index + 4].copy_from_slice(&color);
        }
        cases.push(sparse);
        for (x, y) in [(0, 0), (36, 0), (0, 28), (36, 28)] {
            let mut edge = Pixmap::new(37, 29).unwrap();
            let index = (y * 37 + x) * 4;
            edge.data_mut()[index..index + 4].copy_from_slice(&[19, 7, 31, 63]);
            cases.push(edge);
        }
        for source in cases {
            let coverage = bounds(source.as_ref());
            for blend in Blend::ALL {
                for opacity in [0., 0.14, 0.5, 1.] {
                    for (x, y) in [(0., 0.), (7., 11.), (-13., -9.), (42., 37.)] {
                        let placement = Transform::from_translate(x, y);
                        let mut expected = random_pixels(53, 41);
                        let mut actual = expected.clone();
                        expected.draw_pixmap(
                            0,
                            0,
                            source.as_ref(),
                            &PixmapPaint {
                                opacity,
                                blend_mode: blend.to_skia(),
                                ..Default::default()
                            },
                            placement,
                            None,
                        );
                        draw(
                            &mut actual,
                            source.as_ref(),
                            coverage,
                            opacity,
                            blend,
                            placement,
                        );
                        assert_eq!(
                            actual.data(),
                            expected.data(),
                            "{blend:?}, opacity={opacity}, placement={x},{y}, bounds={coverage:?}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn noninteger_transforms_keep_the_original_full_surface_path() {
        let mut source = Pixmap::new(37, 29).unwrap();
        source.data_mut()[0..4].copy_from_slice(&[127, 15, 31, 159]);
        for placement in [
            Transform::from_translate(0.3, -0.7),
            Transform::from_row(0.9, 0.03, -0.06, 1.1, 4.25, -2.5),
        ] {
            let mut expected = random_pixels(53, 41);
            let mut actual = expected.clone();
            expected.draw_pixmap(
                0,
                0,
                source.as_ref(),
                &PixmapPaint::default(),
                placement,
                None,
            );
            draw(
                &mut actual,
                source.as_ref(),
                bounds(source.as_ref()),
                1.,
                Blend::Normal,
                placement,
            );
            assert_eq!(actual.data(), expected.data());
        }
    }
}
