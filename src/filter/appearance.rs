//! Appearance effects are composited against the real backdrop, independently of content.
use super::*;
use std::sync::Arc;
use tiny_skia::{Mask, PixmapPaint, Transform};
mod cache;

pub(crate) fn reset_appearance_admission() {
    cache::reset_admission();
}

impl Fx {
    pub fn appearance(&self) -> Option<(Blend, f32)> {
        match *self {
            Self::Shadow { blend, opacity, .. }
            | Self::InnerShadow { blend, opacity, .. }
            | Self::OuterGlow { blend, opacity, .. }
            | Self::InnerGlow { blend, opacity, .. }
            | Self::ColorOverlay { blend, opacity, .. } => Some((blend, opacity.clamp(0., 1.))),
            _ => None,
        }
    }
    pub fn appearance_mut(&mut self) -> Option<(&mut Blend, &mut f32)> {
        match self {
            Self::Shadow { blend, opacity, .. }
            | Self::InnerShadow { blend, opacity, .. }
            | Self::OuterGlow { blend, opacity, .. }
            | Self::InnerGlow { blend, opacity, .. }
            | Self::ColorOverlay { blend, opacity, .. } => Some((blend, opacity)),
            _ => None,
        }
    }
    pub fn outer(&self) -> bool {
        matches!(self, Self::Shadow { .. } | Self::OuterGlow { .. })
    }
}
impl FilterStack {
    pub fn independent(&self) -> bool {
        self.active()
            && !self.legacy_composite
            && self.items.iter().any(|f| f.appearance().is_some())
    }
    pub fn blends_backdrop(&self) -> bool {
        self.active()
            && self
                .items
                .iter()
                .any(|f| f.appearance().is_some_and(|(b, _)| b != Blend::Normal))
    }
    pub fn migrate_legacy(&mut self, owner: Blend) {
        if self.items.iter().any(|f| f.appearance().is_some()) {
            self.legacy_composite = true;
            for fx in &mut self.items {
                if let Some((blend, _)) = fx.appearance_mut() {
                    *blend = owner;
                }
                if let Fx::Shadow { knockout, .. } = fx {
                    *knockout = false;
                }
            }
        }
    }
}

/// Build only an effect's pixels. Source alpha is retained independently of Fill opacity.
pub fn effect_pixels(source: &Pixmap, fx: &Fx) -> Option<Pixmap> {
    let mut out = source.clone();
    match *fx {
        Fx::Shadow {
            dx,
            dy,
            blur: sigma,
            color,
            spread,
            knockout,
            ..
        } => {
            morphology(&mut out, false, spread.max(0.));
            // Preserve the established shadow kernel and integer rounding order.
            tint_alpha(&mut out, color);
            blur(&mut out, sigma);
            offset(&mut out, dx, dy);
            if knockout {
                let mut inverse = source.clone();
                invert_alpha(&mut inverse);
                clip_to_alpha(&mut out, &inverse);
            }
        }
        Fx::OuterGlow {
            blur: sigma,
            spread,
            color,
            ..
        } => {
            morphology(&mut out, false, spread.max(0.));
            super::blur_alpha(&mut out, sigma);
            tint_alpha(&mut out, color);
            let mut inverse = source.clone();
            invert_alpha(&mut inverse);
            clip_to_alpha(&mut out, &inverse);
        }
        Fx::InnerShadow {
            dx,
            dy,
            blur: sigma,
            color,
            choke,
            ..
        } => {
            invert_alpha(&mut out);
            morphology(&mut out, false, choke.max(0.));
            offset(&mut out, dx, dy);
            super::blur_alpha(&mut out, sigma);
            tint_alpha(&mut out, color);
            clip_to_alpha(&mut out, source);
        }
        Fx::InnerGlow {
            blur: sigma,
            choke,
            color,
            source: origin,
            ..
        } => {
            if origin == GlowSource::Edge {
                invert_alpha(&mut out);
            }
            morphology(&mut out, origin == GlowSource::Center, choke.max(0.));
            super::blur_alpha(&mut out, sigma);
            tint_alpha(&mut out, color);
            clip_to_alpha(&mut out, source);
        }
        Fx::ColorOverlay { color, .. } => tint_alpha(&mut out, color),
        _ => return None,
    }
    Some(out)
}

/// Pixel filters run before alpha-derived appearance effects. At full object opacity no
/// backdrop copy is required; interpolation is necessary only for partial group opacity.
pub fn composite(
    dst: &mut Pixmap,
    mut content: Pixmap,
    stack: &FilterStack,
    transform: Transform,
    blend: tiny_skia::BlendMode,
    opacity: f32,
    fill_opacity: f32,
    interior: bool,
    mask: Option<&Mask>,
) {
    if stack.active() {
        for fx in &stack.items {
            if fx.appearance().is_none() {
                super::apply_one(&mut content, fx);
            }
        }
    }
    composite_prepared(
        dst,
        content,
        stack,
        transform,
        blend,
        opacity,
        fill_opacity,
        interior,
        mask,
    );
}

/// Composite content whose pixel filters have already been evaluated. Appearance
/// planes still see the real destination and preserve independent blend semantics.
pub(crate) fn composite_prepared(
    dst: &mut Pixmap,
    content: Pixmap,
    stack: &FilterStack,
    transform: Transform,
    blend: tiny_skia::BlendMode,
    opacity: f32,
    fill_opacity: f32,
    interior: bool,
    mask: Option<&Mask>,
) {
    composite_plane(
        dst,
        Plane::Owned(content),
        stack,
        transform,
        blend,
        opacity,
        fill_opacity,
        interior,
        mask,
    )
}
pub(crate) fn composite_cached(
    dst: &mut Pixmap,
    content: Arc<Pixmap>,
    stack: &FilterStack,
    transform: Transform,
    blend: tiny_skia::BlendMode,
    opacity: f32,
    fill_opacity: f32,
    interior: bool,
    mask: Option<&Mask>,
) {
    composite_plane(
        dst,
        Plane::Shared(content),
        stack,
        transform,
        blend,
        opacity,
        fill_opacity,
        interior,
        mask,
    )
}
enum Plane {
    Owned(Pixmap),
    Shared(Arc<Pixmap>),
}
impl std::ops::Deref for Plane {
    type Target = Pixmap;
    fn deref(&self) -> &Pixmap {
        match self {
            Self::Owned(p) => p,
            Self::Shared(p) => p,
        }
    }
}
impl std::ops::DerefMut for Plane {
    fn deref_mut(&mut self) -> &mut Pixmap {
        match self {
            Self::Owned(p) => p,
            Self::Shared(p) => Arc::make_mut(p),
        }
    }
}
fn composite_plane(
    dst: &mut Pixmap,
    mut content: Plane,
    stack: &FilterStack,
    transform: Transform,
    blend: tiny_skia::BlendMode,
    opacity: f32,
    fill_opacity: f32,
    interior: bool,
    mask: Option<&Mask>,
) {
    // Normal/100% stacks need one transformed canvas blit, like the historical
    // filter path. Build the source-over result locally without copying backdrop.
    if blend == tiny_skia::BlendMode::SourceOver
        && opacity >= 1.
        && fill_opacity >= 1.
        && !interior
        && mask.is_none()
        && stack.items.iter().all(|fx| {
            fx.appearance()
                .is_none_or(|(b, a)| b == Blend::Normal && a >= 1.)
        })
    {
        let appearance: Vec<_> = stack
            .items
            .iter()
            .filter(|fx| stack.active() && fx.appearance().is_some())
            .collect();
        if let [fx] = appearance.as_slice()
            && matches!(
                fx,
                Fx::Shadow {
                    knockout: false,
                    spread: 0.,
                    ..
                } | Fx::InnerShadow { choke: 0., .. }
            )
        {
            // This common legacy-compatible stack is exactly the original one-temp
            // filter algorithm, including its integer blur rounding and allocation cost.
            super::apply_one(&mut content, fx);
            dst.draw_pixmap(
                0,
                0,
                content.as_ref(),
                &PixmapPaint {
                    quality: tiny_skia::FilterQuality::Bilinear,
                    ..Default::default()
                },
                transform,
                None,
            );
            return;
        }
        if let Some(mut combined) = Pixmap::new(content.width(), content.height()) {
            for fx in stack.items.iter().filter(|fx| stack.active() && fx.outer()) {
                if let Some(effect) = effect_pixels(&content, fx) {
                    super::blit_over(&mut combined, &effect);
                }
            }
            super::blit_over(&mut combined, &content);
            for fx in stack
                .items
                .iter()
                .filter(|fx| stack.active() && !fx.outer())
            {
                if let Some(effect) = effect_pixels(&content, fx) {
                    super::blit_over(&mut combined, &effect);
                }
            }
            dst.draw_pixmap(
                0,
                0,
                combined.as_ref(),
                &PixmapPaint {
                    quality: tiny_skia::FilterQuality::Bilinear,
                    ..Default::default()
                },
                transform,
                None,
            );
        }
        return;
    }
    let original = (opacity < 1.).then(|| BackdropRegion::capture(dst, &content, transform));
    let draw = |dst: &mut Pixmap, src: &Pixmap, blend_mode, opacity, transform, mask| {
        dst.draw_pixmap(
            0,
            0,
            src.as_ref(),
            &PixmapPaint {
                blend_mode,
                opacity,
                quality: tiny_skia::FilterQuality::Bilinear,
            },
            transform,
            mask,
        );
    };
    let effects = match &content {
        Plane::Owned(pixels) => cache::render(pixels, stack),
        Plane::Shared(pixels) => cache::render_shared(pixels, stack),
    };
    for (fx, pixels) in effects.iter().filter(|(fx, _)| fx.outer()) {
        let (mode, alpha) = fx.appearance().unwrap();
        draw(dst, pixels, mode.to_skia(), alpha, transform, mask);
    }
    if interior {
        // Fill opacity fades only content. Interior effects remain independently visible.
        for byte in content.data_mut() {
            *byte = (*byte as f32 * fill_opacity.clamp(0., 1.)).round() as u8;
        }
        for (fx, pixels) in effects.iter().filter(|(fx, _)| !fx.outer()) {
            let (mode, alpha) = fx.appearance().unwrap();
            draw(
                &mut content,
                pixels,
                mode.to_skia(),
                alpha,
                Transform::identity(),
                None,
            );
        }
        draw(dst, &content, blend, 1., transform, mask);
    } else {
        draw(
            dst,
            &content,
            blend,
            fill_opacity.clamp(0., 1.),
            transform,
            mask,
        );
        for (fx, pixels) in effects.iter().filter(|(fx, _)| !fx.outer()) {
            let (mode, alpha) = fx.appearance().unwrap();
            draw(dst, pixels, mode.to_skia(), alpha, transform, mask);
        }
    }
    if let Some(original) = original {
        original.interpolate(dst, opacity);
    }
}

// Every appearance plane has the source dimensions. Only its transformed
// rectangle can touch the backdrop; two extra pixels cover bilinear sampling
// and scan-conversion rounding. Avoid a full viewport copy/pass for every fade.
struct BackdropRegion {
    x: usize,
    y: usize,
    width: usize,
    pixels: Vec<u8>,
}
impl BackdropRegion {
    fn capture(dst: &Pixmap, source: &Pixmap, transform: Transform) -> Self {
        let (w, h) = (source.width() as f32, source.height() as f32);
        let mut corners = [
            tiny_skia::Point::from_xy(0., 0.),
            tiny_skia::Point::from_xy(w, 0.),
            tiny_skia::Point::from_xy(w, h),
            tiny_skia::Point::from_xy(0., h),
        ];
        transform.map_points(&mut corners);
        let (x, y, right, bottom) = if corners.iter().all(|p| p.x.is_finite() && p.y.is_finite()) {
            let min_x = corners.iter().map(|p| p.x).fold(f32::INFINITY, f32::min);
            let min_y = corners.iter().map(|p| p.y).fold(f32::INFINITY, f32::min);
            let max_x = corners
                .iter()
                .map(|p| p.x)
                .fold(f32::NEG_INFINITY, f32::max);
            let max_y = corners
                .iter()
                .map(|p| p.y)
                .fold(f32::NEG_INFINITY, f32::max);
            (
                (min_x.floor() - 2.).clamp(0., dst.width() as f32) as usize,
                (min_y.floor() - 2.).clamp(0., dst.height() as f32) as usize,
                (max_x.ceil() + 2.).clamp(0., dst.width() as f32) as usize,
                (max_y.ceil() + 2.).clamp(0., dst.height() as f32) as usize,
            )
        } else {
            (0, 0, dst.width() as usize, dst.height() as usize)
        };
        let width = right.saturating_sub(x);
        let stride = dst.width() as usize * 4;
        let mut pixels = Vec::with_capacity(width * bottom.saturating_sub(y) * 4);
        for row in y..bottom {
            pixels.extend_from_slice(&dst.data()[row * stride + x * 4..row * stride + right * 4]);
        }
        Self {
            x,
            y,
            width,
            pixels,
        }
    }
    fn interpolate(self, dst: &mut Pixmap, opacity: f32) {
        if self.width == 0 {
            return;
        }
        let opacity = opacity.clamp(0., 1.);
        let stride = dst.width() as usize * 4;
        for (row, before) in self.pixels.chunks_exact(self.width * 4).enumerate() {
            let start = (self.y + row) * stride + self.x * 4;
            for (out, before) in dst.data_mut()[start..start + self.width * 4]
                .iter_mut()
                .zip(before)
            {
                *out = (*before as f32 + (*out as f32 - *before as f32) * opacity).round() as u8;
            }
        }
    }
}

#[cfg(test)]
mod region_tests {
    use super::*;
    #[test]
    fn bounded_opacity_matches_full_backdrop_with_transforms_masks_and_blends() {
        let mut source = Pixmap::new(19, 23).unwrap();
        source.fill(tiny_skia::Color::from_rgba8(210, 80, 40, 170));
        let stack = FilterStack {
            items: vec![Fx::ColorOverlay {
                color: Rgba::new(26, 102, 204, 178),
                blend: Blend::Multiply,
                opacity: 0.6,
            }],
            ..Default::default()
        };
        let mask = Mask::new(180, 130).unwrap();
        let mut mask = mask;
        for (i, value) in mask.data_mut().iter_mut().enumerate() {
            *value = (i % 256) as u8;
        }
        for transform in [
            Transform::from_translate(51.37, 36.91),
            Transform::from_translate(-13.4, 118.2),
            Transform::from_row(1.2, 0.7, -0.4, 0.8, 73.2, 26.7),
            Transform::from_row(-1.3, 0.4, 0.2, -0.9, 82.1, 50.6),
            Transform::from_translate(500., -200.),
        ] {
            for interior in [false, true] {
                for masked in [false, true] {
                    let mut before = Pixmap::new(180, 130).unwrap();
                    before.fill(tiny_skia::Color::from_rgba8(80, 160, 110, 210));
                    let mut expected = before.clone();
                    let mask = masked.then_some(&mask);
                    composite_prepared(
                        &mut expected,
                        source.clone(),
                        &stack,
                        transform,
                        tiny_skia::BlendMode::Screen,
                        1.,
                        0.7,
                        interior,
                        mask,
                    );
                    for (out, old) in expected.data_mut().iter_mut().zip(before.data()) {
                        *out = (*old as f32 + (*out as f32 - *old as f32) * 0.37).round() as u8;
                    }
                    let mut actual = before;
                    composite_prepared(
                        &mut actual,
                        source.clone(),
                        &stack,
                        transform,
                        tiny_skia::BlendMode::Screen,
                        0.37,
                        0.7,
                        interior,
                        mask,
                    );
                    assert_eq!(
                        actual.data(),
                        expected.data(),
                        "{transform:?}, interior={interior}, masked={masked}"
                    );
                }
            }
        }
    }
}

/// Shadow-only/filter-only SVG primitives. No BackgroundImage dependency.
pub fn svg_effect_primitives(fx: &Fx, input: &str, out: &str) -> String {
    let (dx, dy, sigma, radius, color, invert, knockout) = match *fx {
        Fx::Shadow {
            dx,
            dy,
            blur,
            spread,
            color,
            knockout,
            ..
        } => (dx, dy, blur, spread, color, false, knockout),
        Fx::OuterGlow {
            blur,
            spread,
            color,
            ..
        } => (0., 0., blur, spread, color, false, true),
        Fx::InnerShadow {
            dx,
            dy,
            blur,
            choke,
            color,
            ..
        } => (dx, dy, blur, choke, color, true, false),
        Fx::InnerGlow {
            blur,
            choke,
            color,
            source,
            ..
        } => (
            0.,
            0.,
            blur,
            choke,
            color,
            source == GlowSource::Edge,
            false,
        ),
        Fx::ColorOverlay { color, .. } => (0., 0., 0., 0., color, false, false),
        _ => return String::new(),
    };
    let morphology = if matches!(
        fx,
        Fx::InnerGlow {
            source: GlowSource::Center,
            ..
        }
    ) {
        "erode"
    } else {
        "dilate"
    };
    let mut xml = format!(
        "<feColorMatrix in=\"{input}\" type=\"matrix\" values=\"0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 {} {}\" result=\"{out}-alpha\"/>",
        if invert { -1 } else { 1 },
        if invert { 1 } else { 0 }
    );
    xml += &format!(
        "<feMorphology in=\"{out}-alpha\" operator=\"{morphology}\" radius=\"{}\"/><feOffset dx=\"{dx}\" dy=\"{dy}\"/><feGaussianBlur stdDeviation=\"{}\" result=\"{out}-blur\"/><feFlood flood-color=\"#{:02x}{:02x}{:02x}\" flood-opacity=\"{}\"/><feComposite in2=\"{out}-blur\" operator=\"in\" result=\"{out}\"/>",
        radius.max(0.),
        sigma.max(0.),
        color.r,
        color.g,
        color.b,
        color.a as f32 / 255.
    );
    if knockout || (!fx.outer() && !matches!(fx, Fx::ColorOverlay { .. })) {
        xml += &format!(
            "<feComposite in=\"{out}\" in2=\"{input}\" operator=\"{}\" result=\"{out}\"/>",
            if knockout { "out" } else { "in" }
        );
    }
    xml
}
