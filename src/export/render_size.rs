//! Pixel extents and drawing scale share one checked resolution calculation.
use crate::document::Document;

#[derive(Clone, Copy, Debug)]
pub(crate) struct RenderSize {
    pub factor: f32,
    pub width: u32,
    pub height: u32,
}
impl RenderSize {
    /// Preserve the integer export's original floating-point rounding.
    /// Args: source document and legacy integer scale. Returns its unchanged pixel extent.
    pub fn legacy(doc: &Document, scale: u32) -> Self {
        let factor = scale.clamp(1, 8) as f32;
        Self {
            factor,
            width: (doc.width * factor).round().max(1.) as u32,
            height: (doc.height * factor).round().max(1.) as u32,
        }
    }

    /// Resolve a positive finite fractional factor without silently clamping it.
    /// Args: source document and requested pixels per document unit. Returns checked rounded dimensions.
    pub fn new(doc: &Document, factor: f64) -> Result<Self, String> {
        if !factor.is_finite()
            || factor <= 0.
            || !(factor as f32).is_finite()
            || factor as f32 <= 0.
        {
            return Err("Export scale must be a positive finite number".into());
        }
        let dimension = |value: f32| {
            let pixels = (value as f64 * factor).round().max(1.);
            if !value.is_finite() || value <= 0. || !pixels.is_finite() || pixels > u32::MAX as f64
            {
                Err("Export dimensions exceed the addressable pixel range".to_owned())
            } else {
                Ok(pixels as u32)
            }
        };
        Ok(Self {
            factor: factor as f32,
            width: dimension(doc.width)?,
            height: dimension(doc.height)?,
        })
    }
}
