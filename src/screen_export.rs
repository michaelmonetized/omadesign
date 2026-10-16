//! Document-owned asset references and export rows share the existing scope and format writers.
use crate::{
    document::Document,
    export::{self, Format, Source},
    ml::Progress,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
};

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Object {
    pub layer: u64,
    pub shape: u64,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Asset {
    pub id: u64,
    pub name: String,
    pub objects: Vec<Object>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Row {
    pub id: u64,
    pub scale: u32,
    pub format: Format,
    pub suffix: String,
}
impl Default for Row {
    fn default() -> Self {
        Self {
            id: 1,
            scale: 1,
            format: Format::Png,
            suffix: String::new(),
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Catalog {
    pub assets: Vec<Asset>,
    pub rows: Vec<Row>,
    pub prefix: String,
}
impl Default for Catalog {
    fn default() -> Self {
        Self {
            assets: vec![],
            rows: vec![Row::default()],
            prefix: String::new(),
        }
    }
}
fn filename_part(text: &str, empty: bool) -> bool {
    (empty || !text.trim().is_empty())
        && text.len() <= 80
        && text.trim() == text
        && !text
            .chars()
            .any(|c| c.is_control() || "/\\:*?\"<>|".contains(c))
        && !text.ends_with('.')
        && !matches!(text, "." | "..")
}
impl Catalog {
    /// Preserve the legacy wire when no collection or export preference has been saved.
    /// Args: none. Returns whether the catalog is the unchanged default.
    pub fn is_empty(&self) -> bool {
        self == &Self::default()
    }

    /// Validate portable names, stable identities and bounded export metadata.
    /// Args: none. Returns an error without removing stale artwork references.
    pub fn validate(&self) -> Result<(), String> {
        if self.assets.len() > 256 || !(1..=16).contains(&self.rows.len()) {
            return Err("Export collections support 256 assets and 1–16 format rows".into());
        }
        if !filename_part(&self.prefix, true) {
            return Err("Use a portable export prefix of at most 80 bytes".into());
        }
        let mut ids = HashSet::new();
        let mut names = HashSet::new();
        let mut total = 0usize;
        for asset in &self.assets {
            if asset.id == 0
                || asset.id >= u64::MAX - 200_000
                || !ids.insert(asset.id)
                || !filename_part(&asset.name, false)
                || !names.insert(asset.name.to_lowercase())
            {
                return Err(
                    "Export asset names and identifiers must be unique and portable".into(),
                );
            }
            total = total
                .checked_add(asset.objects.len())
                .ok_or("Too many asset objects")?;
            let mut objects = HashSet::new();
            if asset.objects.is_empty()
                || asset.objects.len() > 1024
                || total > 16_384
                || asset
                    .objects
                    .iter()
                    .any(|o| o.layer == 0 || !objects.insert(o))
            {
                return Err(
                    "Export assets require 1–1024 distinct artwork references, at most 16384 total"
                        .into(),
                );
            }
        }
        ids.clear();
        for row in &self.rows {
            if row.id == 0
                || row.id >= u64::MAX - 200_000
                || !ids.insert(row.id)
                || !(1..=8).contains(&row.scale)
                || !filename_part(&row.suffix, true)
            {
                return Err(
                    "Use unique export rows, a scale from 1× to 8× and a portable suffix".into(),
                );
            }
            if !row.format.raster() && row.scale != 1 {
                return Err(
                    "Layered and vector export rows use 1×; PNG, JPEG and TIFF support 1×–8×"
                        .into(),
                );
            }
        }
        Ok(())
    }
}
impl Asset {
    /// Resolve live artwork through stable layer and object identities.
    /// Args: current document. Returns original scope indices or a named stale/hidden-target error.
    pub fn selection(&self, doc: &Document) -> Result<Vec<(usize, u64)>, String> {
        let mut selected = vec![];
        for object in &self.objects {
            let index = doc
                .layers
                .iter()
                .position(|l| l.id == object.layer)
                .ok_or_else(|| format!("Asset {} references a removed layer", self.name))?;
            if !doc.layer_visible(index) {
                return Err(format!("Asset {} references hidden artwork", self.name));
            }
            if object.shape == 0 {
                if doc.layers[index].kind.pixels().is_none() {
                    return Err(format!(
                        "Asset {} no longer references raster artwork",
                        self.name
                    ));
                }
            } else {
                let shape = doc
                    .find_shape(index, object.shape)
                    .ok_or_else(|| format!("Asset {} references a removed object", self.name))?;
                if !shape.visible {
                    return Err(format!("Asset {} references hidden artwork", self.name));
                }
                let mut parent = shape.layout.parent;
                for _ in 0..64 {
                    let Some(id) = parent else { break };
                    let owner = doc
                        .find_shape(index, id)
                        .ok_or("Asset has a missing frame")?;
                    if !owner.visible {
                        return Err(format!("Asset {} has a hidden parent", self.name));
                    }
                    parent = owner.layout.parent;
                }
            }
            selected.push((index, object.shape));
        }
        Ok(selected)
    }

    /// Fit the actual selected layer tree with the existing native export scope.
    /// Args: current document. Returns a transparent source with its shared resources intact.
    pub fn document(&self, doc: &Document) -> Result<Document, String> {
        doc.validate_hierarchy()?;
        let mut output = export::target::selection(doc, &self.selection(doc)?)?;
        output.screen_export = Catalog::default();
        Ok(output)
    }
}

#[derive(Clone, Debug)]
pub enum Scope {
    Document,
    Artboard(u64),
    Asset(u64),
}
#[derive(Clone)]
struct File {
    scope: Scope,
    row: Row,
    name: String,
}
pub struct Plan {
    document: Document,
    files: Vec<File>,
}
#[derive(Clone, Debug)]
pub struct ResultFile {
    pub path: PathBuf,
    pub notes: Vec<String>,
}
impl Plan {
    /// Capture an immutable batch and reject missing targets or duplicate output names.
    /// Args: document and chosen document/artboard/asset scopes. Returns a source-owned export plan.
    pub fn new(doc: &Document, scopes: &[Scope]) -> Result<Self, String> {
        doc.validate_hierarchy()?;
        doc.screen_export.validate()?;
        if scopes.is_empty() || scopes.len() > 512 {
            return Err("Choose 1–512 assets or artboards to export".into());
        }
        let mut files = vec![];
        let mut names = HashSet::new();
        for scope in scopes {
            let name = match scope {
                Scope::Document => &doc.name,
                Scope::Artboard(id) => {
                    &doc.artboards
                        .iter()
                        .find(|b| b.id == *id)
                        .ok_or("That artboard no longer exists")?
                        .name
                }
                Scope::Asset(id) => {
                    &doc.screen_export
                        .assets
                        .iter()
                        .find(|a| a.id == *id)
                        .ok_or("That asset no longer exists")?
                        .name
                }
            };
            if !filename_part(name, false) {
                return Err(format!(
                    "Rename {name:?} to a portable filename before exporting"
                ));
            }
            for row in &doc.screen_export.rows {
                let name = format!(
                    "{}{}{}.{}",
                    doc.screen_export.prefix,
                    name,
                    row.suffix,
                    row.format.extension()
                );
                if name.len() > 240 || !names.insert(name.to_lowercase()) {
                    return Err("Export rows produce duplicate or oversized filenames. Give each format/scale a distinct suffix".into());
                }
                files.push(File {
                    scope: scope.clone(),
                    row: row.clone(),
                    name,
                });
            }
        }
        let plan = Self {
            document: doc.clone(),
            files,
        };
        for file in &plan.files {
            let source = plan.source(&file.scope)?;
            export::dimensions(
                &source,
                export::Settings {
                    format: file.row.format,
                    scale: file.row.scale,
                    ai: None,
                },
            )?;
        }
        Ok(plan)
    }
    fn source(&self, scope: &Scope) -> Result<Source, String> {
        let mut doc = match scope {
            Scope::Document => self.document.clone(),
            Scope::Artboard(id) => export::target::artboard(&self.document, *id)?,
            Scope::Asset(id) => self
                .document
                .screen_export
                .assets
                .iter()
                .find(|a| a.id == *id)
                .ok_or("That asset no longer exists")?
                .document(&self.document)?,
        };
        doc.screen_export = Catalog::default();
        doc.collaboration = None;
        Ok(Source::Document(doc))
    }

    /// Encode through current admitted writers and publish a collision-safe batch.
    /// Args: existing destination folder and progress/cancellation. Returns final paths after every file succeeds.
    pub fn save(&self, folder: &Path, progress: &dyn Progress) -> Result<Vec<ResultFile>, String> {
        progress.check()?;
        if !cfg!(unix) {
            return Err("Collision-safe batch publication is unavailable on this platform".into());
        }
        if !folder.is_dir() {
            return Err("Choose an existing export folder".into());
        }
        for file in &self.files {
            if folder
                .join(&file.name)
                .try_exists()
                .map_err(|e| e.to_string())?
            {
                return Err(format!(
                    "{} already exists. Choose another folder or prefix",
                    file.name
                ));
            }
        }
        let stage = folder.join(format!(
            ".omadesign-assets-{}-{}",
            std::process::id(),
            crate::document::next_id()
        ));
        std::fs::create_dir(&stage).map_err(|e| e.to_string())?;
        let mut published = vec![];
        let result = (|| {
            let mut outputs = vec![];
            for (index, file) in self.files.iter().enumerate() {
                progress.check()?;
                progress.report("Exporting assets", index, self.files.len());
                let notes = export::save(
                    &self.source(&file.scope)?,
                    export::Settings {
                        format: file.row.format,
                        scale: file.row.scale,
                        ai: None,
                    },
                    &stage.join(&file.name),
                    progress,
                )?;
                outputs.push(ResultFile {
                    path: folder.join(&file.name),
                    notes,
                });
            }
            progress.check()?;
            for (file, output) in self.files.iter().zip(&outputs) {
                progress.check()?;
                let staged = stage.join(&file.name);
                std::fs::hard_link(&staged, &output.path).map_err(|e| {
                    format!(
                        "Cannot publish {} without replacing existing files: {e}",
                        file.name
                    )
                })?;
                published.push((staged, output.path.clone()));
            }
            Ok(outputs)
        })();
        if result.is_err() {
            for (staged, path) in &published {
                if same_file(staged, path) {
                    let _ = std::fs::remove_file(path);
                }
            }
        }
        let _ = std::fs::remove_dir_all(stage);
        result
    }
}
#[cfg(unix)]
fn same_file(a: &Path, b: &Path) -> bool {
    use std::os::unix::fs::MetadataExt;
    std::fs::symlink_metadata(a)
        .ok()
        .zip(std::fs::symlink_metadata(b).ok())
        .is_some_and(|(a, b)| a.dev() == b.dev() && a.ino() == b.ino())
}
#[cfg(not(unix))]
fn same_file(_a: &Path, _b: &Path) -> bool {
    false
}

#[cfg(test)]
mod tests;
