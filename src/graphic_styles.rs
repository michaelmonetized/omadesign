//! Portable appearance presets retain their paint, effects and embedded resources.
use crate::{
    color::Blend,
    document::{Document, Layer, Shape, Style},
    filter::FilterStack,
    geom::{Geom, Pt},
    layout_images::ImageFill,
};
use serde::{Deserialize, Serialize};
use std::{
    io::{self, Write},
    path::Path,
};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) struct Appearance {
    pub style: Style,
    pub filters: FilterStack,
    pub opacity: f32,
    pub fill_opacity: f32,
    pub blend: Blend,
    pub blend_interior: bool,
    pub image: Option<ImageFill>,
}
impl Appearance {
    /// Capture the appearance without geometry, hierarchy, masks or object identity.
    /// Args: source object. Returns self-contained paint and effect values.
    pub fn capture(shape: &Shape) -> Self {
        Self {
            style: shape.style.clone(),
            filters: shape.filters.clone(),
            opacity: shape.opacity,
            fill_opacity: shape.fill_opacity,
            blend: shape.blend,
            blend_interior: shape.blend_interior,
            image: shape.layout.image.clone(),
        }
    }
    /// Apply only appearance values to an existing object.
    /// Args: destination object. Returns nothing; geometry, text, masks and IDs stay intact.
    pub fn apply(&self, shape: &mut Shape) {
        shape.style = self.style.clone();
        shape.filters = self.filters.clone();
        shape.opacity = self.opacity;
        shape.fill_opacity = self.fill_opacity;
        shape.blend = self.blend;
        shape.blend_interior = self.blend_interior;
        shape.layout.image = self.image.clone();
    }
    /// Build a small preview through the ordinary document renderer.
    /// Args: none. Returns a transparent square with this appearance and no source object.
    pub fn preview(&self) -> Document {
        let mut doc = Document::new("Graphic style", 100., 100., 72.);
        doc.transparent = true;
        let mut shape = Shape::new(
            Geom::Rect {
                origin: Pt::splat(16.),
                size: Pt::splat(68.),
                radius: 5.,
            },
            self.style.clone(),
        );
        self.apply(&mut shape);
        let mut layer = Layer::vector("Appearance");
        *layer.kind.shapes_mut().unwrap() = vec![shape];
        doc.layers = vec![layer];
        doc
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct Entry {
    pub id: u64,
    pub name: String,
    pub appearance: Appearance,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub(crate) struct Library {
    pub entries: Vec<Entry>,
    pub next: u64,
    #[serde(skip)]
    pub stamp: Option<u64>,
}
#[derive(Serialize, Deserialize)]
struct File {
    version: u8,
    library: Library,
}

struct Output(Vec<u8>);
impl Write for Output {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.len() > crate::palette::MAX_FILE_BYTES.saturating_sub(self.0.len()) {
            return Err(io::Error::other("Graphic style library exceeds 4 MiB"));
        }
        self.0.try_reserve(bytes.len()).map_err(io::Error::other)?;
        self.0.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
fn encoded<T: Serialize>(value: &T) -> Result<Vec<u8>, String> {
    let mut output = Output(Vec::new());
    serde_json::to_writer(&mut output, value).map_err(|e| e.to_string())?;
    Ok(output.0)
}
fn validate(book: &Library) -> Result<(), String> {
    if book.entries.len() > 256 || book.next == u64::MAX {
        return Err("Graphic style library is full".into());
    }
    let mut ids = std::collections::HashSet::new();
    let mut names = std::collections::HashSet::new();
    for entry in &book.entries {
        if entry.id == 0
            || entry.id > book.next
            || !ids.insert(entry.id)
            || !names.insert(entry.name.to_lowercase())
            || entry.name.trim().is_empty()
            || entry.name.chars().count() > 64
            || entry.name.chars().any(char::is_control)
        {
            return Err(
                "Graphic styles need unique IDs and printable names of at most 64 characters"
                    .into(),
            );
        }
        validate_appearance(&entry.appearance)?;
    }
    Ok(())
}
/// Check captured or imported numeric data without changing its values.
/// Args: complete appearance. Returns an error for unrepresentable numbers or invalid opacity.
pub(crate) fn validate_appearance(value: &Appearance) -> Result<(), String> {
    let _: Appearance = serde_json::from_slice(&encoded(value)?)
        .map_err(|e| format!("Invalid graphic style: {e}"))?;
    if !(0. ..=1.).contains(&value.opacity) || !(0. ..=1.).contains(&value.fill_opacity) {
        return Err("Graphic style opacity must be between zero and one".into());
    }
    if let crate::document::Fill::Pattern(pattern) = &value.style.fill {
        if pattern.id.is_empty()
            || pattern
                .id
                .chars()
                .any(|c| c.is_whitespace() || matches!(c, '<' | '>' | '"' | '\'' | '&'))
        {
            return Err("The graphic style has an invalid pattern reference".into());
        }
        let source = format!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" xmlns:xlink=\"http://www.w3.org/1999/xlink\">{}</svg>",
            pattern.definitions
        );
        let xml = roxmltree::Document::parse(&source)
            .map_err(|e| format!("Invalid graphic style pattern: {e}"))?;
        if !xml.descendants().any(|n| {
            n.is_element()
                && n.tag_name().name() == "pattern"
                && n.attribute("id") == Some(pattern.id.as_str())
        }) {
            return Err("The graphic style's pattern definition is missing".into());
        }
    }
    if let Some(image) = &value.image {
        crate::layout_images::pixmap(image)?;
    }
    Ok(())
}
impl Library {
    /// Read the actual bounded file, refusing unknown versions and preserving unreadable bytes.
    /// Args: file path. Returns a loaded library and the fingerprint of those exact bytes.
    pub fn load(path: &Path) -> Result<Self, String> {
        let Some((bytes, stamp)) = crate::app::libraries::palette_bytes(path)? else {
            return Ok(Self::default());
        };
        let file: File = serde_json::from_slice(&bytes)
            .map_err(|e| format!("Cannot read graphic styles: {e}"))?;
        if file.version != 1 {
            return Err(
                "Unsupported graphic style library version; original file preserved".into(),
            );
        }
        validate(&file.library)?;
        Ok(Self {
            stamp: Some(stamp),
            ..file.library
        })
    }
    /// Write one valid library only if the loaded file is still current.
    /// Args: destination. Returns success after atomic replacement and updates the saved fingerprint.
    pub fn save(&mut self, path: &Path) -> Result<(), String> {
        validate(self)?;
        if crate::app::libraries::file_stamp(path)? != self.stamp {
            return Err("Graphic styles changed on disk. Reload before saving".into());
        }
        let bytes = encoded(&File {
            version: 1,
            library: self.clone(),
        })?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        crate::export::write_atomic(path, &bytes, &crate::ml::NoProgress)?;
        self.stamp = crate::app::libraries::file_stamp(path)?;
        Ok(())
    }
    /// Append a captured appearance with a stable library-local ID.
    /// Args: printable name and appearance. Returns the assigned ID without writing the file.
    pub fn add(&mut self, name: String, appearance: Appearance) -> Result<u64, String> {
        let name = name.trim();
        if name.is_empty() || name.chars().count() > 64 || name.chars().any(char::is_control) {
            return Err("Use a printable graphic style name of at most 64 characters".into());
        }
        let mut unique = name.to_owned();
        for index in 2..=258 {
            if !self
                .entries
                .iter()
                .any(|e| e.name.to_lowercase() == unique.to_lowercase())
            {
                break;
            }
            let suffix = format!(" ({index})");
            unique = format!(
                "{}{}",
                name.chars().take(64 - suffix.len()).collect::<String>(),
                suffix
            );
        }
        let next = self
            .next
            .checked_add(1)
            .ok_or("Graphic style library is full")?;
        let mut after = self.clone();
        after.next = next;
        after.entries.push(Entry {
            id: next,
            name: unique,
            appearance,
        });
        validate(&after)?;
        *self = after;
        Ok(next)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{color::Rgba, document::Fill};
    fn sample() -> Appearance {
        let mut shape = Shape::new(
            Geom::Rect {
                origin: Pt::ZERO,
                size: Pt::splat(20.),
                radius: 0.,
            },
            Style::default(),
        );
        shape.opacity = 0.61;
        shape.fill_opacity = 0.73;
        shape.blend = Blend::Multiply;
        shape
            .filters
            .items
            .push(crate::filter::Fx::Blur { std: 1.25 });
        shape.style.fill = Fill::Pattern(crate::svg_source::SvgPattern {
            definitions: "<pattern id=\"ink\" width=\".25\" height=\".25\" patternContentUnits=\"objectBoundingBox\"><path d=\"M0 0H.25V.25H0Z\" fill=\"#12ab34\"/></pattern>".into(),
            id: "ink".into(), transform: [1., 0.2, -0.1, 1., 0.1, 0.3], opacity: 0.81,
        });
        Appearance::capture(&shape)
    }
    #[test]
    fn craft_graphic_style_library_roundtrip_retains_appearance_resources_and_rejects_stale_save() {
        let folder =
            std::env::temp_dir().join(format!("graphic-styles-{}", crate::document::next_id()));
        let path = folder.join("styles.json");
        let value = sample();
        let mut book = Library::load(&path).unwrap();
        let id = book
            .add("Pattern and shadow".into(), value.clone())
            .unwrap();
        book.save(&path).unwrap();
        let mut cold = Library::load(&path).unwrap();
        assert_eq!(cold.entries[0].id, id);
        assert_eq!(cold.entries[0].appearance, value);
        cold.entries[0].name = "Other app edit".into();
        cold.save(&path).unwrap();
        let current = std::fs::read(&path).unwrap();
        book.entries[0].name = "Stale overwrite".into();
        assert!(book.save(&path).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), current);
        assert_eq!(
            Library::load(&path).unwrap().entries[0].name,
            "Other app edit"
        );
        std::fs::remove_dir_all(folder).unwrap();
    }
    #[test]
    fn craft_graphic_style_library_invalid_future_and_oversize_preserve_the_original_file() {
        let folder = std::env::temp_dir().join(format!(
            "graphic-styles-refusal-{}",
            crate::document::next_id()
        ));
        std::fs::create_dir_all(&folder).unwrap();
        let path = folder.join("styles.json");
        for source in [
            "{broken",
            "{\"version\":2,\"library\":{\"entries\":[],\"next\":0}}",
        ] {
            std::fs::write(&path, source).unwrap();
            assert!(Library::load(&path).is_err());
            assert_eq!(std::fs::read_to_string(&path).unwrap(), source);
        }
        std::fs::remove_file(&path).unwrap();
        let mut book = Library::default();
        book.add("Keep".into(), sample()).unwrap();
        book.save(&path).unwrap();
        let original = std::fs::read(&path).unwrap();
        let mut invalid: serde_json::Value = serde_json::from_slice(&original).unwrap();
        invalid["library"]["entries"][0]["appearance"]["style"]["fill"]["Pattern"]["id"] =
            "missing".into();
        let corrupt = serde_json::to_vec(&invalid).unwrap();
        std::fs::write(&path, &corrupt).unwrap();
        assert!(Library::load(&path).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), corrupt);
        std::fs::write(&path, &original).unwrap();
        book.entries[0].appearance.opacity = f32::NAN;
        assert!(book.save(&path).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), original);
        book.entries[0].appearance = sample();
        let Fill::Pattern(pattern) = &mut book.entries[0].appearance.style.fill else {
            panic!()
        };
        pattern.definitions = "x".repeat(crate::palette::MAX_FILE_BYTES).into();
        assert!(book.save(&path).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), original);
        let mut solid = sample();
        solid.style.fill = Fill::Solid(Rgba::WHITE);
        assert!(validate_appearance(&solid).is_ok());
        std::fs::remove_dir_all(folder).unwrap();
    }
}
