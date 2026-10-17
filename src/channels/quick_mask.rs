//! Quick Mask stores editable coverage; its overlay never becomes artwork.
use super::{Channel, Plane};
use crate::{color::Rgba, resources::Allocation};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Indicates {
    #[default]
    Masked,
    Selected,
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Options {
    pub color: Rgba,
    pub opacity: f32,
    pub indicates: Indicates,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            color: Rgba::rgb(255, 0, 0),
            opacity: 0.5,
            indicates: Indicates::Masked,
        }
    }
}
impl Options {
    /// Validate authored display options before accepting a command or file.
    /// Args: none. Returns a readable error without clamping stored values.
    pub fn validate(self) -> Result<(), String> {
        if !self.opacity.is_finite() || !(0.0..=1.0).contains(&self.opacity) {
            return Err("Quick Mask opacity must be between zero and one".into());
        }
        Ok(())
    }
    /// Read overlay coverage without changing its actual selection values.
    /// Args: unit scalar sample. Returns the display-only colored fraction.
    pub fn coverage(self, sample: f32) -> f32 {
        let value = match self.indicates {
            Indicates::Masked => 1. - sample,
            Indicates::Selected => sample,
        };
        value * self.opacity * f32::from(self.color.a) / 255.
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct QuickMask {
    pub channel: Channel,
    pub unselected: bool,
    pub options: Options,
}
impl QuickMask {
    /// Validate stored identity, placement and options without rescanning accepted samples.
    /// Args: saved alpha channels. Returns structural errors, including an ambiguous shared identity.
    pub(crate) fn validate_metadata(&self, channels: &[Channel]) -> Result<(), String> {
        super::validate_metadata(std::slice::from_ref(&self.channel))?;
        if channels.iter().any(|channel| channel.id == self.channel.id) {
            return Err("Quick Mask and saved channel IDs must be distinct".into());
        }
        self.options.validate()
    }
    /// Validate the complete stored mask before publication or use.
    /// Args: saved channels. Returns errors without changing mask or artwork.
    pub(crate) fn validate(&self, channels: &[Channel]) -> Result<(), String> {
        self.validate_metadata(channels)?;
        self.channel.plane.validate()
    }
    /// Distinguish an untouched no-selection white mask from an explicit Select All.
    /// Args: none. Returns true only for the original empty masked-areas state.
    pub(crate) fn leaves_unselected(&self) -> bool {
        self.channel.plane.samples.iter().all(|v| *v == 0.)
            || self.unselected
                && self.options.indicates == Indicates::Masked
                && self
                    .channel
                    .plane
                    .samples
                    .iter()
                    .all(|v| *v == 1.)
    }
}

#[derive(Clone, Debug)]
pub struct Selection {
    pub values: Arc<Allocation<f32>>,
    pub width: u32,
    pub height: u32,
    pub transform: [f32; 6],
}
impl Selection {
    /// Read the original selection's document placement.
    /// Args: none. Returns its stored transform for exact Undo restoration.
    pub fn placement(&self) -> tiny_skia::Transform {
        let [a, b, c, d, e, f] = self.transform;
        tiny_skia::Transform::from_row(a, b, c, d, e, f)
    }
    /// Keep a produced mask's actual grid with its admitted full-depth owner.
    /// Args: scalar plane and converted selection owner. Returns selection history state without copying samples.
    pub(crate) fn from_plane(plane: &Plane, values: Arc<Allocation<f32>>) -> Self {
        Self {
            values,
            width: plane.width,
            height: plane.height,
            transform: plane.transform,
        }
    }
}
#[derive(Clone, Debug)]
pub struct SelectionChange {
    pub before: Option<Selection>,
    pub after: Option<Selection>,
}

#[cfg(test)]
#[path = "quick_mask_tests.rs"]
mod tests;
