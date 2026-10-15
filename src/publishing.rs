//! Publishing information retained alongside editable artboards and text stories.
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// IDML styles and master sources survive native save and reopen.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Publishing {
    pub facing_pages: bool,
    pub swatches: BTreeMap<String, crate::color::Rgba>,
    /// Top, left, bottom and right bleed in document units.
    pub bleed: [f32; 4],
    pub styles: Vec<PublishingStyle>,
    pub masters: Vec<MasterSpread>,
    pub master_stories: BTreeMap<String, String>,
    pub master_layers: BTreeMap<String, String>,
    pub master_assets: BTreeMap<String, Vec<u8>>,
    pub page_sides: BTreeMap<u64, String>,
    pub page_masters: BTreeMap<u64, String>,
    pub links: BTreeMap<u64, String>,
    /// Locked master previews are generated from the retained source, not exported twice.
    pub inherited_layers: Vec<u64>,
    pub inherited_fingerprints: BTreeMap<u64, String>,
    pub inherited_pages: BTreeMap<u64, u64>,
}
impl Publishing {
    /// Detect publishing data. Takes this record and returns whether it is empty.
    pub fn is_empty(&self) -> bool {
        self == &Self::default()
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PublishingStyle {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub attributes: BTreeMap<String, String>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct MasterSpread {
    pub id: String,
    pub name: String,
    /// Original standard IDML XML retains master geometry and page relationships.
    pub xml: String,
}
