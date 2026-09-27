//! Appearance effects are composited against the real backdrop, independently of content.
use super::*;
use tiny_skia::{Mask, PixmapPaint, Transform};

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
            blur(&mut out, sigma);
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
            blur(&mut out, sigma);
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
            blur(&mut out, sigma);
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
    let original = (opacity < 1.).then(|| dst.data().to_vec());
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
    let effects: Vec<_> = if stack.active() {
        stack
            .items
            .iter()
            .filter_map(|fx| Some((fx, effect_pixels(&content, fx)?)))
            .collect()
    } else {
        vec![]
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
        let opacity = opacity.clamp(0., 1.);
        for (out, before) in dst.data_mut().iter_mut().zip(original) {
            *out = (before as f32 + (*out as f32 - before as f32) * opacity).round() as u8;
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
