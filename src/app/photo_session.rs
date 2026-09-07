use crate::geom::Pt;
use crate::photo::{self, DevelopParams, Histogram, PhotoImage, RgbaImage};
use eframe::egui;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc};

const MAX_IMPORT_JOBS: usize = 2;
const MAX_FOLDER_FILES: usize = 10_000;
const MAX_UNDO: usize = 128;

struct ImportJob {
    path: PathBuf,
    receiver: mpsc::Receiver<Result<PhotoImage, String>>,
}

struct FolderListing {
    path: String,
    files: Vec<(String, String)>,
    truncated: bool,
}

struct FolderJob {
    receiver: mpsc::Receiver<Result<FolderListing, String>>,
    cancel: Arc<AtomicBool>,
}

struct SaveJob {
    receiver: mpsc::Receiver<Result<Vec<SavedSettings>, String>>,
}

struct SaveRequest {
    index: usize,
    source: PathBuf,
    identity: photo::edits::SourceIdentity,
    params: DevelopParams,
}

struct SavedSettings {
    request: SaveRequest,
    result: Result<PathBuf, String>,
}

struct EditHistory {
    undo: Vec<DevelopParams>,
    redo: Vec<DevelopParams>,
    saved: DevelopParams,
    coalescing: bool,
}

pub struct PhotoSession {
    pub images: Vec<PhotoImage>,
    pub selected: Option<usize>,
    pub folder: String,
    pub folder_files: Vec<(String, String)>,
    pub view_scale: f32,
    pub view_offset: egui::Vec2,
    pub fit_scale: f32,
    pub show_original: bool,
    pub crop_drag: Option<(Pt, Pt)>,
    pub adjusted: Option<RgbaImage>,
    pub hists: [Histogram; 4],
    pub tex: Option<egui::TextureHandle>,
    pub orig_tex: Option<egui::TextureHandle>,
    pub thumbs: Vec<egui::TextureHandle>,
    pub sel_version: u64,
    pub built_version: u64,
    pub orig_built: u64,
    pub dirty: bool,
    pub samples_loaded: bool,
    pub status: String,
    import_jobs: Vec<ImportJob>,
    folder_job: Option<FolderJob>,
    save_job: Option<SaveJob>,
    save_error: String,
    history: Vec<EditHistory>,
}

impl Default for PhotoSession {
    fn default() -> Self {
        Self::new()
    }
}

impl PhotoSession {
    pub fn new() -> Self {
        Self {
            images: vec![],
            selected: None,
            folder: String::new(),
            folder_files: vec![],
            view_scale: 1.0,
            view_offset: egui::Vec2::ZERO,
            fit_scale: 1.0,
            show_original: false,
            crop_drag: None,
            adjusted: None,
            hists: Default::default(),
            tex: None,
            orig_tex: None,
            thumbs: vec![],
            sel_version: 0,
            built_version: 0,
            orig_built: u64::MAX,
            dirty: true,
            samples_loaded: false,
            status: "Open a folder, drop photos, or load samples".into(),
            import_jobs: vec![],
            folder_job: None,
            save_job: None,
            save_error: String::new(),
            history: vec![],
        }
    }

    pub fn selected(&self) -> Option<&PhotoImage> {
        self.selected.and_then(|i| self.images.get(i))
    }

    pub fn selected_mut(&mut self) -> Option<&mut PhotoImage> {
        self.selected.and_then(|i| self.images.get_mut(i))
    }

    pub fn select_image(&mut self, index: usize) {
        if index < self.images.len() {
            self.finish_edit();
            self.selected = Some(index);
            self.view_scale = 1.0;
            self.view_offset = egui::Vec2::ZERO;
            self.crop_drag = None;
            self.sel_version += 1;
            self.dirty = true;
        }
    }

    pub fn import_file(&mut self, path: &Path) {
        if self.import_jobs.iter().any(|job| job.path == path) {
            return;
        }
        if self.import_jobs.len() >= MAX_IMPORT_JOBS {
            self.status = "Wait for the current photos to finish opening.".into();
            return;
        }
        let path = path.to_path_buf();
        let input = path.clone();
        let (tx, receiver) = mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(PhotoImage::load(&input));
        });
        self.status = format!(
            "Opening {}…",
            path.file_name().unwrap_or_default().to_string_lossy()
        );
        self.import_jobs.push(ImportJob { path, receiver });
    }

    pub fn import_photo(&mut self, image: PhotoImage) {
        let name = image.name.clone();
        self.images.push(image);
        self.ensure_history();
        self.select_image(self.images.len() - 1);
        self.status = format!("Opened {name}");
    }

    pub fn import_image(&mut self, name: String, full: RgbaImage) {
        self.import_photo(PhotoImage::from_full(name, full));
    }

    pub fn import_bytes(&mut self, name: String, bytes: &[u8]) {
        match photo::decode_bytes(bytes) {
            Some(full) => {
                let name = name.split('/').next_back().unwrap_or(&name).to_string();
                self.import_photo(PhotoImage::from_full(name, full));
            }
            None => self.status = "failed to decode image".into(),
        }
    }

    pub fn import_samples(&mut self) {
        for kind in 0..3 {
            let (name, full) = photo::sample_photo(kind);
            self.images.push(PhotoImage::from_full(name, full));
        }
        self.ensure_history();
        self.select_image(0);
        self.samples_loaded = true;
        self.status = "sample photos loaded".into();
    }

    pub fn set_folder(&mut self, path: &str) {
        if let Some(job) = self.folder_job.take() {
            job.cancel.store(true, Ordering::Relaxed);
        }
        let path = path.to_owned();
        let cancel = Arc::new(AtomicBool::new(false));
        let worker_cancel = cancel.clone();
        let (tx, receiver) = mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(list_folder(&path, &worker_cancel));
        });
        self.folder_job = Some(FolderJob { receiver, cancel });
        self.status = "Reading photo folder…".into();
    }

    /// Complete background reads and writes even when another persona is shown.
    pub fn poll(&mut self, ctx: &egui::Context) {
        let mut index = 0;
        while index < self.import_jobs.len() {
            let Some(result) = completed(&self.import_jobs[index].receiver) else {
                index += 1;
                continue;
            };
            self.import_jobs.remove(index);
            match result {
                Ok(image) => self.import_photo(image),
                Err(error) => self.status = error,
            }
        }
        if let Some(result) = self
            .folder_job
            .as_ref()
            .and_then(|job| completed(&job.receiver))
        {
            self.folder_job = None;
            match result {
                Ok(listing) => {
                    self.folder = listing.path;
                    self.folder_files = listing.files;
                    self.status = if listing.truncated {
                        format!(
                            "Showing the first {MAX_FOLDER_FILES} photos. Open a smaller folder to see the rest."
                        )
                    } else {
                        format!("{} photos in this folder", self.folder_files.len())
                    };
                }
                Err(error) => self.status = error,
            }
        }
        if let Some(result) = self
            .save_job
            .as_ref()
            .and_then(|job| completed(&job.receiver))
        {
            self.save_job = None;
            match result {
                Ok(results) => {
                    let mut errors = vec![];
                    let mut count = 0;
                    self.ensure_history();
                    for saved in results {
                        match saved.result {
                            Ok(_) => {
                                if self
                                    .images
                                    .get(saved.request.index)
                                    .and_then(|image| image.source.as_ref())
                                    == Some(&saved.request.source)
                                {
                                    self.history[saved.request.index].saved = saved.request.params;
                                }
                                count += 1;
                            }
                            Err(error) => errors.push(format!(
                                "{}: {error}",
                                saved
                                    .request
                                    .source
                                    .file_name()
                                    .unwrap_or_default()
                                    .to_string_lossy()
                            )),
                        }
                    }
                    self.save_error = errors.join("\n");
                    self.status = if errors.is_empty() {
                        format!(
                            "Saved settings for {count} photo{}",
                            if count == 1 { "" } else { "s" }
                        )
                    } else {
                        self.save_error.clone()
                    };
                }
                Err(error) => {
                    self.save_error = error.clone();
                    self.status = error;
                }
            }
        }
        if self.is_loading() || self.is_saving() || self.folder_job.is_some() {
            ctx.request_repaint_after(std::time::Duration::from_millis(40));
        }
    }

    pub fn is_loading(&self) -> bool {
        !self.import_jobs.is_empty()
    }

    pub fn is_saving(&self) -> bool {
        self.save_job.is_some()
    }

    pub fn save_settings(&mut self) {
        let Some(index) = self.selected else {
            return;
        };
        self.save_indices(&[index]);
    }

    pub fn save_all_settings(&mut self) {
        let indices = self.unsaved_indices();
        self.save_indices(&indices);
    }

    fn save_indices(&mut self, indices: &[usize]) {
        if self.is_saving() || indices.is_empty() {
            return;
        }
        self.ensure_history();
        let mut requests: Vec<SaveRequest> = vec![];
        for &index in indices {
            let Some(image) = self.images.get(index) else {
                continue;
            };
            let (Some(source), Some(identity)) = (&image.source, &image.source_identity) else {
                self.save_error = "Open a photo from disk to save development settings.".into();
                self.status = self.save_error.clone();
                return;
            };
            if requests
                .iter()
                .any(|request| request.source == *source && request.params != image.develop)
            {
                self.save_error = format!(
                    "{} is open with different edits. Cancel quitting and keep one development before saving all settings.",
                    image.name
                );
                self.status = self.save_error.clone();
                return;
            }
            requests.push(SaveRequest {
                index,
                source: source.clone(),
                identity: identity.clone(),
                params: image.develop.clone(),
            });
        }
        let (tx, receiver) = mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(Ok(write_settings(requests)));
        });
        self.save_error.clear();
        self.save_job = Some(SaveJob { receiver });
        self.status = "Saving photo settings…".into();
    }

    pub fn save_error(&self) -> &str {
        &self.save_error
    }

    pub fn clear_save_error(&mut self) {
        self.save_error.clear();
    }

    pub fn report_write_error(&mut self, error: String) {
        self.status = error.clone();
        self.save_error = error;
    }

    pub fn settings_dirty(&self) -> bool {
        self.selected
            .and_then(|index| self.images.get(index).map(|image| (index, image)))
            .is_some_and(|(index, image)| {
                self.history
                    .get(index)
                    .map_or(!image.develop.is_default(), |history| {
                        history.saved != image.develop
                    })
            })
    }

    pub fn has_unsaved_settings(&self) -> bool {
        !self.unsaved_indices().is_empty()
    }

    fn unsaved_indices(&self) -> Vec<usize> {
        self.images
            .iter()
            .enumerate()
            .filter_map(|(index, image)| {
                (image.source.is_some()
                    && self
                        .history
                        .get(index)
                        .map_or(!image.develop.is_default(), |history| {
                            history.saved != image.develop
                        }))
                .then_some(index)
            })
            .collect()
    }

    pub fn unsaved_names(&self) -> Vec<String> {
        self.unsaved_indices()
            .into_iter()
            .map(|index| self.images[index].name.clone())
            .collect()
    }

    pub fn discard_settings(&mut self) {
        if self.is_saving() {
            return;
        }
        self.ensure_history();
        for (image, history) in self.images.iter_mut().zip(&mut self.history) {
            if image.source.is_some() {
                image.develop = history.saved.clone();
                history.undo.clear();
                history.redo.clear();
                history.coalescing = false;
            }
        }
        self.save_error.clear();
        self.crop_drag = None;
        self.dirty = true;
        self.sel_version += 1;
    }

    fn ensure_history(&mut self) {
        while self.history.len() < self.images.len() {
            self.history.push(EditHistory {
                undo: vec![],
                redo: vec![],
                saved: self.images[self.history.len()].develop.clone(),
                coalescing: false,
            });
        }
    }

    /// Call after a change, with the parameters captured before it. Repeated
    /// frames of one pointer gesture form one undo step; finish_edit ends it.
    pub fn record_edit(&mut self, before: DevelopParams, coalesce: bool) {
        self.ensure_history();
        let Some(index) = self.selected.filter(|index| *index < self.images.len()) else {
            return;
        };
        if self.images[index].develop == before {
            return;
        }
        let history = &mut self.history[index];
        if !coalesce || !history.coalescing {
            if history.undo.len() == MAX_UNDO {
                history.undo.remove(0);
            }
            history.undo.push(before);
        }
        history.redo.clear();
        history.coalescing = coalesce;
        self.dirty = true;
        self.sel_version += 1;
    }

    pub fn finish_edit(&mut self) {
        if let Some(history) = self.selected.and_then(|index| self.history.get_mut(index)) {
            history.coalescing = false;
        }
    }

    pub fn can_undo(&self) -> bool {
        self.selected
            .and_then(|index| self.history.get(index))
            .is_some_and(|history| !history.undo.is_empty())
    }

    pub fn can_redo(&self) -> bool {
        self.selected
            .and_then(|index| self.history.get(index))
            .is_some_and(|history| !history.redo.is_empty())
    }

    pub fn undo(&mut self) {
        self.ensure_history();
        let Some(index) = self.selected.filter(|index| *index < self.images.len()) else {
            return;
        };
        let history = &mut self.history[index];
        if let Some(params) = history.undo.pop() {
            history
                .redo
                .push(std::mem::replace(&mut self.images[index].develop, params));
            history.coalescing = false;
            self.dirty = true;
            self.sel_version += 1;
        }
    }

    pub fn redo(&mut self) {
        self.ensure_history();
        let Some(index) = self.selected.filter(|index| *index < self.images.len()) else {
            return;
        };
        let history = &mut self.history[index];
        if let Some(params) = history.redo.pop() {
            history
                .undo
                .push(std::mem::replace(&mut self.images[index].develop, params));
            history.coalescing = false;
            self.dirty = true;
            self.sel_version += 1;
        }
    }

    pub fn rebuild(&mut self) {
        let Some(img) = self.selected() else {
            return;
        };
        let out = img.render_preview();
        self.hists = photo::histograms(&out);
        self.adjusted = Some(out);
        self.dirty = false;
    }
}

fn completed<T>(receiver: &mpsc::Receiver<Result<T, String>>) -> Option<Result<T, String>> {
    match receiver.try_recv() {
        Ok(result) => Some(result),
        Err(mpsc::TryRecvError::Empty) => None,
        Err(mpsc::TryRecvError::Disconnected) => {
            Some(Err("The photo worker stopped unexpectedly.".into()))
        }
    }
}

fn write_settings(requests: Vec<SaveRequest>) -> Vec<SavedSettings> {
    requests
        .into_iter()
        .map(|request| {
            let result = photo::edits::save(&request.source, &request.identity, &request.params);
            SavedSettings { request, result }
        })
        .collect()
}

fn list_folder(path: &str, cancel: &AtomicBool) -> Result<FolderListing, String> {
    let read = std::fs::read_dir(path)
        .map_err(|error| format!("Could not read the photo folder: {error}"))?;
    let mut files = vec![];
    let mut truncated = false;
    for entry in read {
        if cancel.load(Ordering::Relaxed) {
            return Err("Folder scan cancelled.".into());
        }
        let entry =
            entry.map_err(|error| format!("Could not inspect the photo folder: {error}"))?;
        let path = entry.path();
        if !matches!(crate::import::classify(&path), "raster" | "raw") || !path.is_file() {
            continue;
        }
        if files.len() >= MAX_FOLDER_FILES {
            truncated = true;
            break;
        }
        files.push((
            entry.file_name().to_string_lossy().into_owned(),
            path.to_string_lossy().into_owned(),
        ));
    }
    files.sort_by(|a, b| {
        a.0.to_lowercase()
            .cmp(&b.0.to_lowercase())
            .then(a.0.cmp(&b.0))
    });
    Ok(FolderListing {
        path: path.into(),
        files,
        truncated,
    })
}

impl Drop for PhotoSession {
    fn drop(&mut self) {
        if let Some(job) = &self.folder_job {
            job.cancel.store(true, Ordering::Relaxed);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn image() -> PhotoImage {
        PhotoImage::from_full(
            "Photo".into(),
            RgbaImage::new(4, 3, [64, 96, 128, 255].repeat(12)).unwrap(),
        )
    }

    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "omadesign-photo-session-{}-{}",
                std::process::id(),
                crate::document::next_id()
            ));
            std::fs::create_dir(&path).unwrap();
            Self(path)
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn wait(session: &mut PhotoSession) {
        let ctx = egui::Context::default();
        let start = std::time::Instant::now();
        while session.is_loading() || session.is_saving() || session.folder_job.is_some() {
            session.poll(&ctx);
            assert!(
                start.elapsed() < std::time::Duration::from_secs(5),
                "Photo worker did not finish"
            );
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
    }

    #[test]
    fn edit_history_coalesces_one_gesture_and_stays_with_its_photo() {
        let mut session = PhotoSession::new();
        session.import_photo(image());
        for exposure in [0.5, 1.0, 1.5] {
            let before = session.selected().unwrap().develop.clone();
            session.selected_mut().unwrap().develop.exposure = exposure;
            session.record_edit(before, true);
        }
        session.finish_edit();
        assert!(session.settings_dirty());
        session.undo();
        assert_eq!(session.selected().unwrap().develop.exposure, 0.0);
        assert!(!session.settings_dirty());
        session.redo();
        assert_eq!(session.selected().unwrap().develop.exposure, 1.5);
        session.import_photo(image());
        session.undo();
        assert_eq!(session.selected().unwrap().develop.exposure, 0.0);
        session.select_image(0);
        session.undo();
        assert_eq!(session.selected().unwrap().develop.exposure, 0.0);
    }

    #[test]
    fn photo_folder_lists_raw_case_insensitively_and_failed_import_keeps_selection() {
        let fixture = Fixture::new();
        for name in [
            "photo.DNG",
            "photo.NEF",
            "photo.CR3",
            "photo.JPG",
            "notes.txt",
            "drawing.oma",
        ] {
            std::fs::write(fixture.0.join(name), b"invalid image").unwrap();
        }
        std::fs::create_dir(fixture.0.join("folder.ARW")).unwrap();
        let mut session = PhotoSession::new();
        session.import_photo(image());
        session.set_folder(fixture.0.to_str().unwrap());
        wait(&mut session);
        assert_eq!(session.folder_files.len(), 4);
        session.import_file(&fixture.0.join("photo.JPG"));
        wait(&mut session);
        assert_eq!(session.images.len(), 1);
        assert_eq!(session.selected, Some(0));
        assert_ne!(session.status, "Opened Photo");
    }

    #[test]
    fn saving_a_snapshot_does_not_mark_later_edits_saved() {
        let fixture = Fixture::new();
        let source = fixture.0.join("photo.png");
        std::fs::write(&source, image().full.encode_png().unwrap()).unwrap();
        let mut session = PhotoSession::new();
        session.import_file(&source);
        wait(&mut session);
        let before = session.selected().unwrap().develop.clone();
        session.selected_mut().unwrap().develop.exposure = 0.5;
        session.record_edit(before, false);
        session.save_settings();
        let before = session.selected().unwrap().develop.clone();
        session.selected_mut().unwrap().develop.exposure = 1.0;
        session.record_edit(before, false);
        wait(&mut session);
        assert!(session.settings_dirty());
        assert_eq!(PhotoImage::load(&source).unwrap().develop.exposure, 0.5);
        session.undo();
        assert!(!session.settings_dirty());
        assert_eq!(session.selected().unwrap().develop.exposure, 0.5);
    }

    #[test]
    fn save_all_keeps_each_photos_settings_and_source_separate() {
        let fixture = Fixture::new();
        let original = image().full.encode_png().unwrap();
        let mut session = PhotoSession::new();
        for (name, exposure) in [("first.png", -0.5), ("second.png", 1.0)] {
            let path = fixture.0.join(name);
            std::fs::write(&path, &original).unwrap();
            session.import_photo(PhotoImage::load(&path).unwrap());
            let before = session.selected().unwrap().develop.clone();
            session.selected_mut().unwrap().develop.exposure = exposure;
            session.record_edit(before, false);
        }
        session.save_all_settings();
        assert!(session.is_saving());
        wait(&mut session);
        assert!(!session.has_unsaved_settings());
        assert!(session.save_error().is_empty());
        for (name, exposure) in [("first.png", -0.5), ("second.png", 1.0)] {
            let path = fixture.0.join(name);
            assert_eq!(std::fs::read(&path).unwrap(), original);
            assert_eq!(PhotoImage::load(&path).unwrap().develop.exposure, exposure);
        }
    }
}
