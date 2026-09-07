use crate::geom::Pt;
use crate::photo::{self, DevelopParams, Histogram, PhotoImage, RgbaImage};
use eframe::egui;
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::path::{Path, PathBuf};
#[path = "photo_session/batch.rs"]
mod batch;
#[path = "photo_session/preset_library.rs"]
mod preset_library;
pub use batch::BatchProgress;
use photo::transfer::{AdjustmentSnapshot, Categories};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc};

const MAX_IMPORT_JOBS: usize = 2;
const MAX_FOLDER_FILES: usize = 10_000;
const MAX_UNDO: usize = 128;

struct ImportJob {
    cancelled: bool,
    path: PathBuf,
    receiver: mpsc::Receiver<Result<PhotoImage, String>>,
}

struct FolderListing {
    path: String,
    files: Vec<(String, String)>,
    truncated: bool,
    settings: BTreeMap<PathBuf, Vec<PathBuf>>,
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
    destination: PathBuf,
    identity: photo::edits::SourceIdentity,
    params: DevelopParams,
}

struct SavedSettings {
    request: SaveRequest,
    result: Result<PathBuf, String>,
}

#[derive(Clone)]
struct ImageChange {
    index: usize,
    source: Option<PathBuf>,
    before: DevelopParams,
    after: DevelopParams,
}

#[derive(Clone, Default)]
struct PhotoEdit {
    images: Vec<ImageChange>,
    disk: Vec<batch::DiskChange>,
}

impl PhotoEdit {
    fn is_empty(&self) -> bool {
        self.images.is_empty() && self.disk.is_empty()
    }
    fn bytes(&self) -> usize {
        self.disk.iter().map(batch::DiskChange::bytes).sum()
    }
}

pub struct PhotoSession {
    pub images: Vec<PhotoImage>,
    pub selected: Option<usize>,
    pub selection: BTreeSet<usize>,
    selection_anchor: Option<usize>,
    pub copied_adjustments: Option<AdjustmentSnapshot>,
    pub edit_revision: u64,
    undo_edits: Vec<PhotoEdit>,
    redo_edits: Vec<PhotoEdit>,
    coalescing: Option<usize>,
    import_queue: VecDeque<PathBuf>,
    import_errors: Vec<String>,
    batch_job: Option<batch::BatchJob>,
    batch_progress: Option<BatchProgress>,
    pub presets: Vec<photo::presets::Preset>,
    preset_state: preset_library::PresetState,
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
    saved: Vec<DevelopParams>,
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
            selection: BTreeSet::new(),
            selection_anchor: None,
            copied_adjustments: None,
            edit_revision: 0,
            undo_edits: vec![],
            redo_edits: vec![],
            coalescing: None,
            import_queue: VecDeque::new(),
            import_errors: vec![],
            batch_job: None,
            batch_progress: None,
            presets: vec![],
            preset_state: Default::default(),
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
            saved: vec![],
        }
    }

    pub fn selected(&self) -> Option<&PhotoImage> {
        self.selected.and_then(|i| self.images.get(i))
    }

    pub fn selected_mut(&mut self) -> Option<&mut PhotoImage> {
        self.selected.and_then(|i| self.images.get_mut(i))
    }

    pub fn select_image(&mut self, index: usize) {
        self.select_with(index, false, false);
    }

    pub fn select_with(&mut self, index: usize, ctrl: bool, shift: bool) {
        if index >= self.images.len() || self.is_batching() {
            return;
        }
        self.finish_edit();
        if shift {
            let anchor = self
                .selection_anchor
                .unwrap_or(index)
                .min(self.images.len() - 1);
            if !ctrl {
                self.selection.clear();
            }
            self.selection.extend(anchor.min(index)..=anchor.max(index));
        } else if ctrl {
            if !self.selection.remove(&index) {
                self.selection.insert(index);
            }
            self.selection_anchor = Some(index);
        } else {
            self.selection.clear();
            self.selection.insert(index);
            self.selection_anchor = Some(index);
        }
        if self.selected != Some(index) {
            self.selected = Some(index);
            self.view_scale = 1.0;
            self.view_offset = egui::Vec2::ZERO;
            self.crop_drag = None;
            self.sel_version += 1;
            self.dirty = true;
        }
    }

    pub fn select_all_images(&mut self) {
        self.selection.extend(0..self.images.len());
    }
    pub fn deselect_all_images(&mut self) {
        self.selection.clear();
        self.selection_anchor = self.selected;
    }
    pub fn selected_count(&self) -> usize {
        self.selection.len()
    }

    pub fn import_file(&mut self, path: &Path) {
        if self.is_batching() {
            self.status = "Finish or cancel the folder batch before opening more photos.".into();
            return;
        }
        if self.import_jobs.iter().any(|job| job.path == path)
            || self.import_queue.iter().any(|queued| queued == path)
        {
            return;
        }
        if self.import_queue.len() >= MAX_FOLDER_FILES {
            self.status = format!(
                "The opening queue is full ({MAX_FOLDER_FILES} photos). Wait before adding more."
            );
            return;
        }
        if !self.is_loading() {
            self.import_errors.clear();
        }
        self.import_queue.push_back(path.to_path_buf());
        self.start_imports();
    }

    pub fn import_errors(&self) -> &[String] {
        &self.import_errors
    }

    pub fn queued_imports(&self) -> usize {
        self.import_queue.len() + self.import_jobs.len()
    }
    pub fn cancel_imports(&mut self) {
        self.import_queue.clear();
        for job in &mut self.import_jobs {
            job.cancelled = true;
        }
        self.status = "Photo opening cancelled.".into();
    }

    fn start_imports(&mut self) {
        while self.import_jobs.len() < MAX_IMPORT_JOBS {
            let Some(path) = self.import_queue.pop_front() else {
                break;
            };
            let input = path.clone();
            let (tx, receiver) = mpsc::channel();
            if let Err(error) =
                std::thread::Builder::new()
                    .name("photo-open".into())
                    .spawn(move || {
                        let _ = tx.send(PhotoImage::load(&input));
                    })
            {
                self.status = format!("Could not start opening the photo: {error}");
                continue;
            }
            self.status = format!(
                "Opening {}… ({} queued)",
                path.file_name().unwrap_or_default().to_string_lossy(),
                self.import_queue.len()
            );
            self.import_jobs.push(ImportJob {
                path,
                receiver,
                cancelled: false,
            });
        }
    }

    pub fn import_photo(&mut self, image: PhotoImage) {
        let name = image.name.clone();
        self.images.push(image);
        self.ensure_saved();
        self.select_with(self.images.len() - 1, true, false);
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
        self.ensure_saved();
        self.select_image(0);
        self.samples_loaded = true;
        self.status = "sample photos loaded".into();
    }

    pub fn set_folder(&mut self, path: &str) {
        if self.is_batching() {
            self.status = "Finish or cancel the folder batch first.".into();
            return;
        }
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
        self.poll_batch();
        self.poll_presets();
        let was_loading = self.is_loading();
        let mut index = 0;
        while index < self.import_jobs.len() {
            let Some(result) = completed(&self.import_jobs[index].receiver) else {
                index += 1;
                continue;
            };
            let job = self.import_jobs.remove(index);
            if job.cancelled {
                continue;
            }
            match result {
                Ok(image) => {
                    self.import_photo(image);
                    if photo::edits::is_sidecar(&job.path) {
                        self.status = "Photo and saved settings restored".into();
                    }
                }
                Err(error) => {
                    self.import_errors.push(format!(
                        "{}: {error}",
                        job.path.file_name().unwrap_or_default().to_string_lossy()
                    ));
                    self.status = error;
                }
            }
        }
        self.start_imports();
        if was_loading && !self.is_loading() && !self.import_errors.is_empty() {
            self.status = format!(
                "{} photo{} could not be opened. {}",
                self.import_errors.len(),
                if self.import_errors.len() == 1 {
                    ""
                } else {
                    "s"
                },
                self.import_errors[0]
            );
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
                    self.ensure_saved();
                    for saved in results {
                        match saved.result {
                            Ok(_) => {
                                if self.images.get(saved.request.index).is_some_and(|image| {
                                    image.source.as_ref() == Some(&saved.request.source)
                                        && image.source_identity.as_ref()
                                            == Some(&saved.request.identity)
                                        && image.settings_path.as_ref()
                                            == Some(&saved.request.destination)
                                }) {
                                    self.saved[saved.request.index] = saved.request.params;
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
                        let status = format!(
                            "Saved settings for {count} photo{}",
                            if count == 1 { "" } else { "s" }
                        );
                        if self.has_unsaved_settings() {
                            format!("{status}. Newer edits are still unsaved.")
                        } else {
                            status
                        }
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
        if self.is_loading()
            || self.is_saving()
            || self.folder_job.is_some()
            || self.is_loading_presets()
            || self.is_saving_presets()
        {
            ctx.request_repaint_after(std::time::Duration::from_millis(40));
        }
    }

    pub fn is_loading(&self) -> bool {
        !self.import_jobs.is_empty() || !self.import_queue.is_empty()
    }

    pub fn is_saving(&self) -> bool {
        self.save_job.is_some() || self.is_batching() || self.is_saving_presets()
    }

    pub fn save_settings(&mut self) {
        let Some(index) = self.selected else {
            return;
        };
        self.save_indices(&[index]);
    }

    pub fn save_selected_settings(&mut self) {
        let indices = self.selection.iter().copied().collect::<Vec<_>>();
        self.save_indices(&indices);
    }

    pub fn save_all_settings(&mut self) {
        let indices = self.unsaved_indices();
        self.save_indices(&indices);
    }

    fn save_indices(&mut self, indices: &[usize]) {
        if self.is_saving() || indices.is_empty() {
            return;
        }
        self.ensure_saved();
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
                destination: image
                    .settings_path
                    .clone()
                    .unwrap_or_else(|| photo::edits::sidecar_path(source)),
                identity: identity.clone(),
                params: image.develop.clone(),
            });
        }
        let (tx, receiver) = mpsc::channel();
        if let Err(error) = std::thread::Builder::new()
            .name("photo-save".into())
            .spawn(move || {
                let _ = tx.send(Ok(write_settings(requests)));
            })
        {
            self.report_write_error(format!("Could not start saving photo settings: {error}"));
            return;
        }
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
                self.saved
                    .get(index)
                    .map_or(!image.develop.is_default(), |saved| *saved != image.develop)
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
                        .saved
                        .get(index)
                        .map_or(!image.develop.is_default(), |saved| *saved != image.develop))
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
        self.ensure_saved();
        for (image, saved) in self.images.iter_mut().zip(&mut self.saved) {
            if image.source.is_some() {
                image.develop = saved.clone();
            }
        }
        self.undo_edits.clear();
        self.redo_edits.clear();
        self.coalescing = None;
        self.save_error.clear();
        self.crop_drag = None;
        self.dirty = true;
        self.sel_version += 1;
    }

    fn ensure_saved(&mut self) {
        while self.saved.len() < self.images.len() {
            self.saved
                .push(self.images[self.saved.len()].develop.clone());
        }
    }

    fn push_edit(&mut self, edit: PhotoEdit) {
        if edit.is_empty() {
            return;
        }
        self.undo_edits.push(edit);
        self.redo_edits.clear();
        while self.undo_edits.len() > MAX_UNDO
            || self.undo_edits.iter().map(PhotoEdit::bytes).sum::<usize>() > 64 * 1024 * 1024
        {
            self.undo_edits.remove(0);
        }
        self.edit_revision += 1;
        self.dirty = true;
        self.sel_version += 1;
    }

    /// Repeated pointer frames form one undo step; explicit batches update edit_revision.
    pub fn record_edit(&mut self, before: DevelopParams, coalesce: bool) {
        self.ensure_saved();
        let Some(index) = self.selected.filter(|i| *i < self.images.len()) else {
            return;
        };
        let after = self.images[index].develop.clone();
        if after == before {
            return;
        }
        if coalesce && self.coalescing == Some(index) {
            if let Some(change) = self
                .undo_edits
                .last_mut()
                .and_then(|edit| edit.images.first_mut())
            {
                change.after = after;
            }
            self.redo_edits.clear();
            self.edit_revision += 1;
            self.dirty = true;
            self.sel_version += 1;
        } else {
            self.push_edit(PhotoEdit {
                images: vec![ImageChange {
                    index,
                    source: self.images[index].source.clone(),
                    before,
                    after,
                }],
                disk: vec![],
            });
        }
        self.coalescing = coalesce.then_some(index);
    }

    pub fn finish_edit(&mut self) {
        self.coalescing = None;
    }
    pub fn can_undo(&self) -> bool {
        !self.undo_edits.is_empty() && !self.is_saving()
    }
    pub fn can_redo(&self) -> bool {
        !self.redo_edits.is_empty() && !self.is_saving()
    }
    pub fn undo(&mut self) {
        self.replay_edit(true);
    }
    pub fn redo(&mut self) {
        self.replay_edit(false);
    }

    fn restore_images(&mut self, edit: &PhotoEdit, undo: bool) {
        for change in &edit.images {
            if let Some(image) = self
                .images
                .get_mut(change.index)
                .filter(|i| i.source == change.source)
            {
                image.develop = if undo { &change.before } else { &change.after }.clone();
            }
        }
        for disk in &edit.disk {
            // Saved baselines are disk settings, which may differ from pre-batch unsaved edits.
            let before: DevelopParams = disk
                .before
                .as_ref()
                .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(bytes).ok())
                .and_then(|value| serde_json::from_value(value["develop"].clone()).ok())
                .unwrap_or_default();
            let (expected, restored) = if undo {
                (&disk.after_params, &before)
            } else {
                (&before, &disk.after_params)
            };
            for (index, image) in self.images.iter_mut().enumerate() {
                if image.source.as_ref() == Some(&disk.source)
                    && image.settings_path.as_ref() == Some(&disk.path)
                {
                    // Photos opened after the batch are absent from its image
                    // journal. Follow the restored file only if they still show
                    // that file's previous state; later edits stay dirty.
                    let recorded = edit.images.iter().any(|change| change.index == index);
                    if !recorded && &image.develop == expected {
                        image.develop = restored.clone();
                    }
                    self.saved[index] = restored.clone();
                }
            }
        }
        self.crop_drag = None;
        self.edit_revision += 1;
        self.dirty = true;
        self.sel_version += 1;
    }

    fn replay_edit(&mut self, undo: bool) {
        if self.is_saving() {
            return;
        }
        self.finish_edit();
        self.ensure_saved();
        let Some(edit) = (if undo {
            &mut self.undo_edits
        } else {
            &mut self.redo_edits
        })
        .pop() else {
            return;
        };
        if edit.disk.is_empty() {
            self.restore_images(&edit, undo);
            (if undo {
                &mut self.redo_edits
            } else {
                &mut self.undo_edits
            })
            .push(edit);
        } else {
            self.start_replay(edit, undo);
        }
    }

    pub fn copy_adjustments(&mut self) -> bool {
        let Some(image) = self.selected() else {
            return false;
        };
        self.copied_adjustments = Some(AdjustmentSnapshot {
            name: image.name.chars().take(80).collect(),
            params: image.develop.clone(),
            categories: Categories::default(),
        });
        self.status = "Adjustments copied. Crop and rotation are excluded.".into();
        true
    }

    pub fn apply_adjustments(&mut self, snapshot: &AdjustmentSnapshot) -> Result<usize, String> {
        if self.is_batching() {
            return Err("Finish the folder batch first.".into());
        }
        snapshot.validate()?;
        self.finish_edit();
        self.ensure_saved();
        let mut edit = PhotoEdit::default();
        for &index in &self.selection {
            let Some(image) = self.images.get_mut(index) else {
                continue;
            };
            let before = image.develop.clone();
            snapshot
                .categories
                .apply(&snapshot.params, &mut image.develop);
            if image.develop != before {
                edit.images.push(ImageChange {
                    index,
                    source: image.source.clone(),
                    before,
                    after: image.develop.clone(),
                });
            }
        }
        let count = edit.images.len();
        self.push_edit(edit);
        self.status = format!(
            "Applied adjustments to {count} photo{}; save settings to keep them on disk.",
            if count == 1 { "" } else { "s" }
        );
        Ok(count)
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
            let result = photo::edits::save_to(
                &request.destination,
                &request.source,
                &request.identity,
                &request.params,
            );
            SavedSettings { request, result }
        })
        .collect()
}

fn list_folder(path: &str, cancel: &AtomicBool) -> Result<FolderListing, String> {
    let read = std::fs::read_dir(path)
        .map_err(|error| format!("Could not read the photo folder: {error}"))?;
    let mut files = vec![];
    let mut truncated = false;
    let mut settings: BTreeMap<PathBuf, Vec<PathBuf>> = BTreeMap::new();
    let mut settings_count = 0;
    for entry in read {
        if cancel.load(Ordering::Relaxed) {
            return Err("Folder scan cancelled.".into());
        }
        let entry =
            entry.map_err(|error| format!("Could not inspect the photo folder: {error}"))?;
        let path = entry.path();
        if photo::edits::is_sidecar(&path) {
            settings_count += 1;
            if settings_count > MAX_FOLDER_FILES * 2 {
                return Err(
                    "This folder contains too many settings files. Choose a smaller folder.".into(),
                );
            }
            settings
                .entry(path.with_extension(""))
                .or_default()
                .push(path);
            continue;
        }
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
        settings,
    })
}

impl Drop for PhotoSession {
    fn drop(&mut self) {
        if let Some(job) = &self.batch_job {
            job.cancel.store(true, Ordering::Relaxed);
        }
        if let Some(job) = &self.folder_job {
            job.cancel.store(true, Ordering::Relaxed);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    pub(super) fn image() -> PhotoImage {
        PhotoImage::from_full(
            "Photo".into(),
            RgbaImage::new(4, 3, [64, 96, 128, 255].repeat(12)).unwrap(),
        )
    }

    #[test]
    fn selective_transfer_has_one_batch_undo_and_keeps_target_geometry() {
        let mut session = PhotoSession::new();
        for _ in 0..4 {
            session.import_photo(image());
        }
        session.select_image(0);
        session.selected_mut().unwrap().develop = DevelopParams {
            exposure: 1.5,
            temperature: 20.0,
            rotate: 90,
            crop: Some([0.1, 0.2, 0.9, 0.8]),
            ..Default::default()
        };
        assert!(session.copy_adjustments());
        let snapshot = session.copied_adjustments.clone().unwrap();
        session.selected_mut().unwrap().develop.exposure = -2.0;
        session.select_image(1);
        session.select_with(3, false, true);
        assert_eq!(session.selection, [1, 2, 3].into_iter().collect());
        assert_eq!(session.selected, Some(3));
        session.images[2].develop.rotate = 270;
        session.images[2].develop.crop = Some([0.2, 0.1, 0.7, 0.9]);
        let before: Vec<_> = session.images.iter().map(|i| i.develop.clone()).collect();
        assert_eq!(session.apply_adjustments(&snapshot).unwrap(), 3);
        assert_eq!(session.images[0].develop.exposure, -2.0);
        assert_eq!(session.images[1].develop.exposure, 1.5);
        assert_eq!(session.images[2].develop.rotate, 270);
        assert_eq!(session.images[2].develop.crop, before[2].crop);
        assert_eq!(session.undo_edits.len(), 1);
        session.undo();
        assert_eq!(
            session
                .images
                .iter()
                .map(|i| i.develop.clone())
                .collect::<Vec<_>>(),
            before
        );
        session.redo();
        assert_eq!(session.images[3].develop.temperature, 20.0);
        session.select_with(2, true, false);
        assert_eq!(session.selection, [1, 3].into_iter().collect());
        assert_eq!(session.selected, Some(2));
        session.deselect_all_images();
        assert_eq!(session.apply_adjustments(&snapshot).unwrap(), 0);
        assert_eq!(session.selected, Some(2));
        session.select_all_images();
        assert_eq!(session.selected_count(), 4);
    }

    #[test]
    fn opening_more_than_two_photos_queues_every_request_and_reports_failures() {
        let fixture = Fixture::new();
        let original = image().full.encode_png().unwrap();
        let mut session = PhotoSession::new();
        for index in 0..7 {
            let path = fixture.0.join(format!("{index}.png"));
            std::fs::write(&path, &original).unwrap();
            session.import_file(&path);
        }
        let invalid = fixture.0.join("invalid.png");
        std::fs::write(&invalid, b"not an image").unwrap();
        session.import_file(&invalid);
        assert_eq!(session.import_jobs.len(), 2);
        assert_eq!(session.queued_imports(), 8);
        wait(&mut session);
        assert_eq!(session.images.len(), 7);
        assert_eq!(session.selected_count(), 7);
        assert_eq!(session.import_errors().len(), 1);
        assert!(session.status.contains("could not be opened"));
        assert_eq!(
            session
                .images
                .iter()
                .map(|image| image.name.clone())
                .collect::<BTreeSet<_>>()
                .len(),
            7
        );
    }

    #[test]
    fn folder_apply_saves_without_loading_pixels_and_undo_restores_unsaved_loaded_edits() {
        let fixture = Fixture::new();
        let original = image().full.encode_png().unwrap();
        for name in ["first.png", "second.png", "invalid.png", "ambiguous.png"] {
            std::fs::write(fixture.0.join(name), &original).unwrap();
        }
        std::fs::write(fixture.0.join("invalid.png.omaphoto"), b"invalid settings").unwrap();
        let second = fixture.0.join("second.png");
        let upper = fixture.0.join("second.png.OMAPHOTO");
        let existing = DevelopParams {
            rotate: 90,
            crop: Some([0.1, 0.2, 0.8, 0.9]),
            ..Default::default()
        };
        photo::edits::save_to(
            &upper,
            &second,
            &photo::edits::SourceIdentity::read(&second).unwrap(),
            &existing,
        )
        .unwrap();
        let original_settings = std::fs::read(&upper).unwrap();
        let ambiguous = fixture.0.join("ambiguous.png");
        let identity = photo::edits::SourceIdentity::read(&ambiguous).unwrap();
        let first_settings =
            photo::edits::save(&ambiguous, &identity, &DevelopParams::default()).unwrap();
        let second_settings = fixture.0.join("ambiguous.png.OMAPHOTO");
        photo::edits::save_to(&second_settings, &ambiguous, &identity, &existing).unwrap();
        let ambiguous_bytes = [
            std::fs::read(&first_settings).unwrap(),
            std::fs::read(&second_settings).unwrap(),
        ];
        let mut session = PhotoSession::new();
        session.import_photo(PhotoImage::load(&fixture.0.join("first.png")).unwrap());
        session.images[0].develop.temperature = 12.0;
        session.folder = fixture.0.to_string_lossy().into_owned();
        let snapshot = AdjustmentSnapshot {
            name: "Light".into(),
            params: DevelopParams {
                exposure: 1.0,
                ..Default::default()
            },
            categories: Categories {
                color: false,
                ..Default::default()
            },
        };
        session.start_folder_batch(snapshot).unwrap();
        assert!(session.is_saving());
        wait(&mut session);
        assert_eq!(session.images.len(), 1);
        assert_eq!(session.batch_progress().unwrap().failed, 2);
        assert_eq!(session.batch_progress().unwrap().completed, 4);
        assert!(session.save_error().contains("Multiple settings files"));
        assert_eq!(session.images[0].develop.temperature, 12.0);
        assert_eq!(session.images[0].develop.exposure, 1.0);
        assert!(!session.has_unsaved_settings());
        assert_eq!(PhotoImage::load(&upper).unwrap().develop.exposure, 1.0);
        assert_eq!(
            PhotoImage::load(&upper).unwrap().develop.crop,
            existing.crop
        );
        assert_eq!(
            PhotoImage::load(&upper).unwrap().develop.rotate,
            existing.rotate
        );
        assert!(!fixture.0.join("second.png.omaphoto").exists());
        session.undo();
        wait(&mut session);
        assert_eq!(session.images[0].develop.temperature, 12.0);
        assert_eq!(session.images[0].develop.exposure, 0.0);
        assert!(session.has_unsaved_settings());
        assert!(!fixture.0.join("first.png.omaphoto").exists());
        assert_eq!(std::fs::read(&upper).unwrap(), original_settings);
        session.redo();
        wait(&mut session);
        assert_eq!(session.images[0].develop.exposure, 1.0);
        assert!(!session.has_unsaved_settings());
        let external = DevelopParams {
            exposure: -3.0,
            ..Default::default()
        };
        photo::edits::save_to(
            &upper,
            &second,
            &photo::edits::SourceIdentity::read(&second).unwrap(),
            &external,
        )
        .unwrap();
        session.undo();
        wait(&mut session);
        assert_eq!(PhotoImage::load(&upper).unwrap().develop, external);
        assert!(session.save_error().contains("outside this batch"));
        for name in ["first.png", "second.png", "invalid.png", "ambiguous.png"] {
            assert_eq!(std::fs::read(fixture.0.join(name)).unwrap(), original);
        }
        assert_eq!(std::fs::read(first_settings).unwrap(), ambiguous_bytes[0]);
        assert_eq!(std::fs::read(second_settings).unwrap(), ambiguous_bytes[1]);
        assert_eq!(
            std::fs::read(fixture.0.join("invalid.png.omaphoto")).unwrap(),
            b"invalid settings"
        );
    }

    #[test]
    fn folder_replay_updates_later_imports_without_erasing_newer_edits() {
        let fixture = Fixture::new();
        let source = fixture.0.join("photo.png");
        std::fs::write(&source, image().full.encode_png().unwrap()).unwrap();
        let original = DevelopParams {
            temperature: 14.0,
            ..Default::default()
        };
        let sidecar = photo::edits::save(
            &source,
            &photo::edits::SourceIdentity::read(&source).unwrap(),
            &original,
        )
        .unwrap();
        let snapshot = AdjustmentSnapshot {
            name: "Bright".into(),
            params: DevelopParams {
                exposure: 2.0,
                ..Default::default()
            },
            categories: Categories {
                color: false,
                ..Default::default()
            },
        };
        let mut session = PhotoSession::new();
        session.folder = fixture.0.to_string_lossy().into_owned();
        session.start_folder_batch(snapshot).unwrap();
        wait(&mut session);
        assert!(session.images.is_empty());
        session.import_file(&sidecar);
        wait(&mut session);
        assert_eq!(session.images[0].develop.exposure, 2.0);
        let revision = session.edit_revision;
        session.undo();
        wait(&mut session);
        assert_eq!(session.images[0].develop, original);
        assert!(!session.has_unsaved_settings());
        assert!(session.edit_revision > revision);
        session.import_file(&sidecar);
        wait(&mut session);
        session.redo();
        wait(&mut session);
        assert!(
            session
                .images
                .iter()
                .all(|image| image.develop.exposure == 2.0)
        );
        assert!(!session.has_unsaved_settings());
        session.undo();
        wait(&mut session);
        // Simulate a later in-memory adjustment arriving during replay. The
        // normal UI is disabled then; backend reconciliation still preserves it.
        session.redo();
        session.images[1].develop.exposure = -1.0;
        wait(&mut session);
        assert_eq!(session.images[0].develop.exposure, 2.0);
        assert_eq!(session.images[1].develop.exposure, -1.0);
        assert!(session.has_unsaved_settings());
        assert_eq!(PhotoImage::load(&sidecar).unwrap().develop.exposure, 2.0);
    }

    pub(super) struct Fixture(pub(super) PathBuf);
    impl Fixture {
        pub(super) fn new() -> Self {
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
    fn edit_history_coalesces_gestures_across_photo_switches() {
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

    #[test]
    fn saved_sidecars_reopen_raster_and_raw_after_moving_the_pair() {
        let fixture = Fixture::new();
        for (name, original) in [
            ("Photo.png", image().full.encode_png().unwrap()),
            (
                "Camera.DNG",
                include_bytes!("../formats/raw/synthetic.dng").to_vec(),
            ),
        ] {
            let folder = fixture.0.join(name);
            std::fs::create_dir(&folder).unwrap();
            let source = folder.join(name);
            std::fs::write(&source, &original).unwrap();
            let mut session = PhotoSession::new();
            session.import_file(&source);
            wait(&mut session);
            let params = DevelopParams {
                exposure: -0.7,
                rotate: 90,
                crop: Some([0.0, 0.0, 0.75, 1.0]),
                ..Default::default()
            };
            let before = session.selected().unwrap().develop.clone();
            session.selected_mut().unwrap().develop = params.clone();
            session.record_edit(before, false);
            let rendered = session.selected().unwrap().render_full();
            session.save_settings();
            wait(&mut session);
            assert!(session.save_error().is_empty());
            assert!(!session.settings_dirty());
            drop(session);

            let moved = fixture.0.join(format!("moved-{name}"));
            std::fs::rename(folder, &moved).unwrap();
            let source = moved.join(name);
            let lower = photo::edits::sidecar_path(&source);
            let settings = lower.with_extension("OMAPHOTO");
            std::fs::rename(&lower, &settings).unwrap();
            let mut reopened = PhotoSession::new();
            reopened.import_file(&settings);
            wait(&mut reopened);
            let loaded = reopened.selected().unwrap();
            assert_eq!(loaded.develop, params);
            assert_eq!(loaded.render_full(), rendered);
            assert_eq!(loaded.source.as_ref(), Some(&source));
            assert_eq!(loaded.settings_path.as_ref(), Some(&settings));
            assert_eq!(loaded.raw.is_some(), name.ends_with("DNG"));
            assert!(!reopened.settings_dirty());

            let before = reopened.selected().unwrap().develop.clone();
            reopened.selected_mut().unwrap().develop.exposure = 0.4;
            reopened.record_edit(before, false);
            reopened.save_settings();
            wait(&mut reopened);
            assert!(reopened.save_error().is_empty());
            assert!(!reopened.settings_dirty());
            assert_eq!(PhotoImage::load(&settings).unwrap().develop.exposure, 0.4);
            assert!(
                !lower.exists(),
                "Save must keep the opened sidecar's spelling"
            );
            let export = moved.join("developed.tif");
            crate::formats::cli::run(&[
                "--convert".into(),
                settings.to_string_lossy().into_owned(),
                "--output".into(),
                export.to_string_lossy().into_owned(),
            ])
            .unwrap()
            .unwrap();
            let exported = image::open(export).unwrap();
            assert_eq!(
                exported.color().bits_per_pixel(),
                if name.ends_with("DNG") { 64 } else { 32 }
            );
            assert_eq!(std::fs::read(&source).unwrap(), original);
        }
    }

    #[test]
    fn invalid_or_orphaned_sidecar_does_not_replace_current_photo_or_edits() {
        let fixture = Fixture::new();
        let source = fixture.0.join("photo.png");
        std::fs::write(&source, image().full.encode_png().unwrap()).unwrap();
        let settings = photo::edits::save(
            &source,
            &photo::edits::SourceIdentity::read(&source).unwrap(),
            &DevelopParams {
                exposure: 0.7,
                ..Default::default()
            },
        )
        .unwrap();
        let mut session = PhotoSession::new();
        session.import_photo(image());
        let before = session.selected().unwrap().develop.clone();
        session.selected_mut().unwrap().develop.exposure = -0.5;
        session.record_edit(before, false);
        let expected = session.selected().unwrap().render_full();
        std::fs::write(&settings, b"not valid settings").unwrap();
        session.import_file(&settings);
        wait(&mut session);
        assert!(session.status.contains("Could not read photo settings"));
        assert_eq!(session.images.len(), 1);
        assert_eq!(session.selected, Some(0));
        assert!(session.settings_dirty());
        assert_eq!(session.selected().unwrap().render_full(), expected);
        let original = PhotoImage::load(&source).unwrap();
        assert!(original.develop.is_default());
        assert!(
            original
                .notes
                .iter()
                .any(|note| note.contains("not loaded"))
        );

        std::fs::remove_file(&source).unwrap();
        session.import_file(&settings);
        wait(&mut session);
        assert!(session.status.contains("original photo, photo.png"));
        assert_eq!(session.images.len(), 1);
        assert_eq!(session.selected().unwrap().render_full(), expected);
    }

    #[test]
    fn failed_save_preserves_edits_and_recovers_after_the_destination_is_fixed() {
        let fixture = Fixture::new();
        let source = fixture.0.join("photo.png");
        let original = image().full.encode_png().unwrap();
        std::fs::write(&source, &original).unwrap();
        let mut session = PhotoSession::new();
        session.import_file(&source);
        wait(&mut session);
        let before = session.selected().unwrap().develop.clone();
        session.selected_mut().unwrap().develop.exposure = 0.5;
        session.record_edit(before, false);
        let destination = photo::edits::sidecar_path(&source);
        std::fs::create_dir(&destination).unwrap();
        session.save_settings();
        assert!(session.is_saving());
        // Switching photos must not redirect the pending write or clear its edits.
        session.import_photo(image());
        wait(&mut session);
        assert!(
            session
                .save_error()
                .contains("Could not save photo settings")
        );
        assert_eq!(session.selected, Some(1));
        assert!(session.has_unsaved_settings());
        assert_eq!(std::fs::read(&source).unwrap(), original);
        assert!(std::fs::read_dir(&fixture.0).unwrap().all(|entry| {
            !entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with(".omadesign-export-")
        }));
        std::fs::remove_dir(destination).unwrap();
        session.select_image(0);
        session.save_settings();
        wait(&mut session);
        assert!(session.save_error().is_empty());
        assert!(!session.settings_dirty());
        assert_eq!(PhotoImage::load(&source).unwrap().develop.exposure, 0.5);
        assert_eq!(std::fs::read(&source).unwrap(), original);
    }
}
