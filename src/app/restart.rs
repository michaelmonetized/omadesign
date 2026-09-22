//! Lossless local restart snapshots, kept beside normal .oma.swp recovery files.
use super::*;
use serde::{Deserialize, Serialize};
use std::path::Path;
#[derive(Serialize, Deserialize)]
pub(super) struct RestartTab {
    pub swap_id: String,
    pub old_swap_id: String,
    pub dirty: bool,
    pub view: View,
    pub selection: Vec<(usize, u64)>,
    pub active_layer: Option<usize>,
    pub selected_layer: Option<u64>,
    pub individual_object: Option<(usize, u64)>,
    pub expanded: HashSet<u64>,
    pub pixel_selection: Option<Vec<u8>>,
    pub selection_space: Option<(u32, u32, [f32; 6])>,
    pub welcome: bool,
    pub playhead: f32,
}
#[derive(Serialize, Deserialize)]
pub(super) struct RestartSession {
    format: u8,
    pub tabs: Vec<RestartTab>,
    pub active: usize,
    persona: Persona,
    tool: Tool,
    photos: photo_session::RestartPhotos,
    personal: libraries::PaletteDraft,
    projects: HashMap<PathBuf, libraries::PaletteDraft>,
    folders: HashMap<String, PathBuf>,
}
pub(super) struct RestartSnapshot {
    session: RestartSession,
    documents: Vec<crate::project::SwapMeta>,
    photos: Vec<crate::photo::PhotoImage>,
}
impl RestartSnapshot {
    pub fn write(self, root: &Path) -> Result<PathBuf, String> {
        std::fs::create_dir_all(root).map_err(|e| e.to_string())?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(root, std::fs::Permissions::from_mode(0o700))
                .map_err(|e| e.to_string())?;
        }
        let directory = root.join(format!("restart-{}", crate::project::new_swap_id()));
        std::fs::create_dir(&directory).map_err(|e| e.to_string())?;
        for doc in self.documents {
            let path = root.join(format!("{}.oma.swp", doc.id));
            crate::project::prepare_swap(doc, path)?.commit()?;
        }
        for (i, photo) in self.photos.into_iter().enumerate() {
            let bytes = if let Some(raw) = photo.raw {
                // 16-bit linear camera pixels survive even if the original file disappeared.
                gzip_json(raw.as_ref())?
            } else {
                photo
                    .full
                    .encode_png()
                    .ok_or("Could not preserve original photo pixels")?
            };
            crate::formats::write_atomic(&directory.join(format!("photo-{i}")), &bytes)?;
        }
        let manifest = directory.join("session.json.gz");
        crate::formats::write_atomic(&manifest, &gzip_json(&self.session)?)?;
        Ok(manifest)
    }
}
fn gzip_json(value: &impl Serialize) -> Result<Vec<u8>, String> {
    let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
    serde_json::to_writer(&mut encoder, value).map_err(|e| e.to_string())?;
    encoder.finish().map_err(|e| e.to_string())
}
pub(super) fn read_gzip<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T, String> {
    let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    serde_json::from_reader(flate2::read::GzDecoder::new(file)).map_err(|e| e.to_string())
}
impl Studio {
    pub(super) fn restart_snapshot(&mut self) -> RestartSnapshot {
        self.commit_type_edit();
        self.end_pixel_stroke(true);
        self.end_deform(true);
        self.photo.finish_edit();
        self.ensure_tabs();
        let (tabs, documents) = self.restart_tabs();
        RestartSnapshot {
            session: RestartSession {
                format: 1,
                tabs,
                active: self.active_tab,
                persona: self.persona,
                tool: self.tool,
                photos: self.photo.restart_state(),
                personal: self.libraries.personal.clone(),
                projects: self.libraries.projects.clone(),
                folders: self.libraries.folders.clone(),
            },
            documents,
            photos: self.photo.images.clone(),
        }
    }
    pub fn restore_restart(&mut self, manifest: &Path) -> Result<(), String> {
        self.restore_restart_from(manifest, &crate::project::swap_dir())
    }
    fn restore_restart_from(&mut self, manifest: &Path, root: &Path) -> Result<(), String> {
        let root = root.canonicalize().map_err(|e| e.to_string())?;
        let manifest = manifest.canonicalize().map_err(|e| e.to_string())?;
        if !manifest.starts_with(&root)
            || manifest.file_name().is_none_or(|n| n != "session.json.gz")
        {
            return Err("Restart manifest must be in Omadesign's recovery directory".into());
        }
        let session: RestartSession = read_gzip(&manifest)?;
        if session.format != 1
            || session.tabs.is_empty()
            || session.active >= session.tabs.len()
            || session.tabs.len() > 1024
        {
            return Err("Unsupported restart session".into());
        }
        let mut documents = Vec::new();
        for tab in &session.tabs {
            if !tab.swap_id.bytes().all(|c| c.is_ascii_hexdigit()) || tab.swap_id.len() > 128 {
                return Err("Invalid recovery document".into());
            }
            documents.push(crate::project::load_swap(
                &root.join(format!("{}.oma.swp", tab.swap_id)),
            )?);
        }
        let photo = PhotoSession::restore_restart(session.photos, manifest.parent().unwrap())?;
        for tab in &session.tabs {
            if tab.old_swap_id != tab.swap_id
                && tab.old_swap_id.bytes().all(|c| c.is_ascii_hexdigit())
                && tab.old_swap_id.len() <= 128
            {
                let _ = std::fs::remove_file(root.join(format!("{}.oma.swp", tab.old_swap_id)));
            }
        }
        // No live state is replaced until every document and original photo loads.
        self.restore_restart_tabs(session.tabs, documents, session.active);
        self.photo = photo;
        self.persona = session.persona;
        self.tool = session.tool;
        self.libraries.personal = session.personal;
        self.libraries.projects = session.projects;
        self.libraries.folders = session.folders;
        self.status = "Update complete. Your workspace has been restored.".into();
        // Keep the manifest and photo originals as a fallback until the user has
        // safely resumed. Document snapshots remain normal recoverable swaps.
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn restart_preserves_tabs_dirty_state_pixels_photos_and_palette_drafts() {
        let fixture = std::env::var_os("OMADESIGN_QA_RESTART_ROOT").map(PathBuf::from);
        let root = fixture
            .clone()
            .unwrap_or_else(|| std::env::temp_dir().join(crate::project::new_swap_id()));
        let mut s = Studio::new();
        s.show_welcome = false;
        s.doc.name = "Unsaved one".into();
        s.dirty = true;
        s.view.scale = 2.5;
        s.new_tab();
        s.doc.name = "Saved two".into();
        s.path = Some(root.join("two.oma"));
        s.dirty = false;
        s.photo.import_image(
            "Clipboard photo".into(),
            crate::photo::RgbaImage::new(2, 1, vec![8, 9, 10, 255, 20, 30, 40, 255]).unwrap(),
        );
        s.photo.selected_mut().unwrap().develop.exposure = 1.5;
        s.libraries
            .personal
            .palettes
            .push(crate::palette::Palette::new("Unwritten palette", vec![]));
        s.libraries.personal.dirty = true;
        s.persona = Persona::Photo;
        let manifest = s.restart_snapshot().write(&root).unwrap();
        let mut restored = Studio::new();
        restored.restore_restart_from(&manifest, &root).unwrap();
        assert_eq!(restored.tab_count(), 2);
        assert_eq!(restored.active_tab, 1);
        assert!(!restored.dirty);
        assert_eq!(restored.doc.name, "Saved two");
        assert_eq!(restored.path, s.path);
        assert_eq!(restored.persona, Persona::Photo);
        assert!(restored.photo.settings_dirty());
        assert_eq!(
            restored.photo.images[0].full.data,
            s.photo.images[0].full.data
        );
        assert_eq!(restored.photo.images[0].develop.exposure, 1.5);
        assert!(restored.libraries.personal.dirty);
        restored.switch_tab(0);
        assert!(restored.dirty);
        assert_eq!(restored.doc.name, "Unsaved one");
        assert_eq!(restored.view.scale, 2.5);
        if fixture.is_some() {
            std::fs::write(
                root.join("manifest-path.txt"),
                manifest.to_string_lossy().as_bytes(),
            )
            .unwrap();
        } else {
            std::fs::remove_dir_all(root).unwrap();
        }
    }
    #[test]
    fn restart_keeps_full_precision_camera_pixels_without_the_source_file() {
        let root = std::env::temp_dir().join(crate::project::new_swap_id());
        let mut s = Studio::new();
        let raw = crate::formats::raw::RawImage {
            width: 2,
            height: 1,
            pixels: vec![1, 256, 65535, 300, 400, 500],
            metadata: Default::default(),
            warnings: vec![],
        };
        let mut photo = crate::photo::PhotoImage::from_raw("Camera original".into(), raw);
        photo.source = Some(root.join("missing-camera-file.dng"));
        s.photo.import_photo(photo);
        s.photo.selected_mut().unwrap().develop.exposure = 2.;
        let manifest = s.restart_snapshot().write(&root).unwrap();
        let mut restored = Studio::new();
        restored.restore_restart_from(&manifest, &root).unwrap();
        assert_eq!(
            restored.photo.images[0].raw.as_ref().unwrap().pixels,
            vec![1, 256, 65535, 300, 400, 500]
        );
        assert!(restored.photo.has_unsaved_settings());
        assert_eq!(restored.photo.images[0].develop.exposure, 2.);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn damaged_snapshot_never_replaces_the_running_workspace() {
        let root = std::env::temp_dir().join(crate::project::new_swap_id());
        let mut s = Studio::new();
        let manifest = s.restart_snapshot().write(&root).unwrap();
        let session: RestartSession = read_gzip(&manifest).unwrap();
        std::fs::remove_file(root.join(format!("{}.oma.swp", session.tabs[0].swap_id))).unwrap();
        s.doc.name = "Keep me".into();
        assert!(s.restore_restart_from(&manifest, &root).is_err());
        assert_eq!(s.doc.name, "Keep me");
        std::fs::remove_dir_all(root).unwrap();
    }
}
