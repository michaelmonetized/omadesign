//! Session state for portable libraries. Sidecars never enter document history.
use crate::palette::Palette;
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::Arc,
    time::Instant,
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Sidebar {
    #[default]
    Inspector,
    Palettes,
    Brand,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Source {
    pub document: String,
    pub path: Option<PathBuf>,
    pub folder: Option<PathBuf>,
}
#[derive(Clone, Default)]
pub struct PaletteDraft {
    pub palettes: Vec<Palette>,
    pub selected: usize,
    pub query: String,
    pub name: String,
    pub hex: String,
    pub dirty: bool,
    pub conflict: bool,
    pub loaded: bool,
    pub stamp: Option<u64>,
    pub message: String,
}
impl PaletteDraft {
    pub fn selected_name(&mut self) {
        self.selected = self.selected.min(self.palettes.len().saturating_sub(1));
        self.name = self
            .palettes
            .get(self.selected)
            .map(|p| p.name.clone())
            .unwrap_or_default();
    }
    pub fn receive(&mut self, palettes: Vec<Palette>, stamp: Option<u64>) {
        if self.dirty {
            self.loaded = true;
            self.conflict |= self.stamp != stamp;
            return;
        }
        if !self.loaded || self.stamp != stamp {
            self.palettes = palettes;
            self.stamp = stamp;
            self.loaded = true;
            self.selected_name();
            self.conflict = false;
            self.message.clear();
        }
    }
}
pub struct Libraries {
    pub sidebar: Sidebar,
    pub project_scope: bool,
    pub source: Option<Source>,
    pub folders: HashMap<String, PathBuf>,
    pub root: Option<PathBuf>,
    pub personal: PaletteDraft,
    pub projects: HashMap<PathBuf, PaletteDraft>,
    pub catalog: Option<Arc<crate::brand::Catalog>>,
    pub typography: Option<Arc<crate::typography::LoadedKit>>,
    pub typography_name: String,
    pub typography_query: String,
    pub typography_role: String,
    pub typography_selected: usize,
    pub typography_message: String,
    pub typography_refresh: bool,
    pub typography_edit_stamp: Option<u128>,
    pub brand_query: String,
    pub brand_kind: String,
    pub brand_name: String,
    pub brand_message: String,
    pub next_sync: Instant,
    pub close_requested: bool,
    pub close_error: String,
}
impl Default for Libraries {
    fn default() -> Self {
        let mut personal = PaletteDraft {
            palettes: vec![],
            ..Default::default()
        };
        personal.selected_name();
        Self {
            sidebar: Sidebar::Inspector,
            project_scope: false,
            source: None,
            folders: HashMap::new(),
            root: None,
            personal,
            projects: HashMap::new(),
            catalog: None,
            typography: None,
            typography_name: String::new(),
            typography_query: String::new(),
            typography_role: String::new(),
            typography_selected: 0,
            typography_message: String::new(),
            typography_refresh: false,
            typography_edit_stamp: None,
            brand_query: String::new(),
            brand_kind: "All".into(),
            brand_name: String::new(),
            brand_message: String::new(),
            next_sync: Instant::now(),
            close_requested: false,
            close_error: String::new(),
        }
    }
}
impl Libraries {
    pub fn draft(&mut self) -> &mut PaletteDraft {
        if self.project_scope
            && let Some(root) = &self.root
        {
            self.projects.entry(root.clone()).or_default()
        } else {
            &mut self.personal
        }
    }
    pub fn palette_path(&self) -> Option<PathBuf> {
        if self.project_scope {
            self.root.as_ref().map(|p| p.join(".omacolors"))
        } else {
            Some(crate::palette::path())
        }
    }
    pub fn choose_folder(&mut self, document: &str, folder: PathBuf) {
        let folder = if folder.file_name().is_some_and(|s| s == ".omabrand") {
            folder.parent().unwrap_or(&folder).to_path_buf()
        } else {
            folder
        };
        self.folders.insert(document.into(), folder);
        self.source = None;
        self.next_sync = Instant::now();
    }
}
/// Read and fingerprint the same bounded bytes, including same-size external edits.
pub fn palette_bytes(path: &Path) -> Result<Option<(Vec<u8>, u64)>, String> {
    use std::{
        hash::{Hash, Hasher},
        io::Read,
    };
    let file = match std::fs::File::open(path) {
        Ok(file) => file,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(format!("Could not read {}: {e}", path.display())),
    };
    let mut data = Vec::new();
    file.take(crate::palette::MAX_FILE_BYTES as u64 + 1)
        .read_to_end(&mut data)
        .map_err(|e| format!("Could not read {}: {e}", path.display()))?;
    if data.len() > crate::palette::MAX_FILE_BYTES {
        return Err("Palette file is larger than 4 MiB".into());
    }
    let mut h = std::collections::hash_map::DefaultHasher::new();
    data.hash(&mut h);
    Ok(Some((data, h.finish())))
}
pub fn file_stamp(path: &Path) -> Result<Option<u64>, String> {
    Ok(palette_bytes(path)?.map(|(_, stamp)| stamp))
}
pub fn project_folder(source: &Source) -> Option<PathBuf> {
    if let Some(folder) = &source.folder {
        return Some(folder.clone());
    }
    let start = source.path.as_ref()?.parent()?;
    for dir in start.ancestors() {
        if dir.join(".omacolors").is_file()
            || dir.join(".omabrand").is_dir()
            || dir.join(".omatype").is_file()
        {
            return Some(dir.to_path_buf());
        }
    }
    // A saved document can start its own kit beside the artwork.
    Some(start.to_path_buf())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn dirty_palettes_survive_refresh_and_conflict_is_visible() {
        let mut d = PaletteDraft::default();
        d.receive(vec![Palette::new("Original", vec![])], Some(1));
        d.palettes[0].name = "Unsaved edit".into();
        d.dirty = true;
        d.receive(vec![Palette::new("External edit", vec![])], Some(2));
        assert_eq!(d.palettes[0].name, "Unsaved edit");
        assert!(d.conflict);
    }
    #[test]
    fn project_drafts_are_shared_by_folder_not_document() {
        let mut s = Libraries {
            project_scope: true,
            root: Some(PathBuf::from("/a")),
            ..Default::default()
        };
        s.draft().palettes.push(Palette::new("A", vec![]));
        s.draft().dirty = true;
        s.root = Some(PathBuf::from("/b"));
        assert!(s.draft().palettes.is_empty());
        s.root = Some(PathBuf::from("/a"));
        assert_eq!(s.draft().palettes[0].name, "A");
        assert!(s.draft().dirty);
    }
}
