//! Source-independent development snapshots. Geometry is deliberately opt-in.
use super::DevelopParams;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Categories {
    pub light: bool,
    pub color: bool,
    pub detail: bool,
    pub curve: bool,
    pub hsl: bool,
    pub split_tone: bool,
    pub crop: bool,
    pub rotation: bool,
}

impl Default for Categories {
    fn default() -> Self {
        Self {
            light: true,
            color: true,
            detail: true,
            curve: true,
            hsl: true,
            split_tone: true,
            crop: false,
            rotation: false,
        }
    }
}

impl Categories {
    pub fn any(self) -> bool {
        self.light
            || self.color
            || self.detail
            || self.curve
            || self.hsl
            || self.split_tone
            || self.crop
            || self.rotation
    }

    pub fn apply(self, source: &DevelopParams, target: &mut DevelopParams) {
        if self.light {
            target.exposure = source.exposure;
            target.contrast = source.contrast;
            target.highlights = source.highlights;
            target.shadows = source.shadows;
            target.whites = source.whites;
            target.blacks = source.blacks;
        }
        if self.color {
            target.temperature = source.temperature;
            target.tint = source.tint;
            target.saturation = source.saturation;
            target.vibrance = source.vibrance;
            target.hue = source.hue;
        }
        if self.detail {
            target.clarity = source.clarity;
            target.dehaze = source.dehaze;
            target.grain = source.grain;
            target.vignette = source.vignette;
        }
        if self.curve {
            target.curve = source.curve;
        }
        if self.hsl {
            target.hsl = source.hsl;
        }
        if self.split_tone {
            target.split_shadow = source.split_shadow;
            target.split_highlight = source.split_highlight;
            target.split_balance = source.split_balance;
        }
        if self.crop {
            target.crop = source.crop;
        }
        if self.rotation {
            target.rotate = source.rotate;
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AdjustmentSnapshot {
    pub name: String,
    pub params: DevelopParams,
    #[serde(default)]
    pub categories: Categories,
}

impl AdjustmentSnapshot {
    pub fn validate(&self) -> Result<(), String> {
        let name = self.name.trim();
        if name.is_empty() || name.chars().count() > 120 || name.chars().any(char::is_control) {
            return Err("Give this preset a name between 1 and 120 characters.".into());
        }
        if !self.categories.any() {
            return Err("Choose at least one adjustment category.".into());
        }
        super::edits::validate(&self.params)
    }
}
