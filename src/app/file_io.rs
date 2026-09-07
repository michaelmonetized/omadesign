//! Decode large documents away from the UI thread; apply results on their owner.
use super::*;
use std::sync::mpsc::{self, Receiver, TryRecvError};

#[derive(Clone, Copy)]
pub(super) enum ImportMode {
    Open,
    Place,
    Drop(Option<Pt>),
    PhotoToDesign,
    Export,
}

pub(super) struct ImportJob {
    receiver: Receiver<Result<Completed, String>>,
    path: PathBuf,
    mode: ImportMode,
    owner: String,
}
enum Completed {
    Imported(crate::import::Imported),
    Exported(Vec<String>),
}

impl Studio {
    pub(super) fn queue_import(&mut self, path: PathBuf, mode: ImportMode) {
        let kind = crate::import::classify(&path);
        if (matches!(kind, "raw" | "photo-settings")
            && matches!(mode, ImportMode::Open | ImportMode::Drop(_)))
            || (self.persona == Persona::Photo
                && kind == "raster"
                && matches!(mode, ImportMode::Open))
        {
            self.photo.import_file(&path);
            self.persona = Persona::Photo;
            self.show_welcome = false;
            self.tool = Tool::Hand;
            return;
        }
        if self.file_jobs.len() >= 4 {
            self.status = "Wait for the current files to finish opening".into();
            return;
        }
        let (tx, receiver) = mpsc::channel();
        let input = path.clone();
        std::thread::spawn(move || {
            let result = crate::import::open_any(&input).and_then(|imported| {
                if let crate::import::Imported::Photo(photo) = imported {
                    crate::formats::cli::photo_document(&photo)
                        .map(crate::import::Imported::Document)
                } else {
                    Ok(imported)
                }
            });
            let _ = tx.send(result.map(Completed::Imported));
        });
        self.status = format!(
            "Opening {}…",
            path.file_name().unwrap_or_default().to_string_lossy()
        );
        self.file_jobs.push(ImportJob {
            receiver,
            path,
            mode,
            owner: self.swap_id.clone(),
        });
    }

    pub fn send_photo_to_design(&mut self) {
        if self.file_jobs.len() >= 4 {
            self.photo.status = "Wait for the current files to finish".into();
            return;
        }
        let Some(photo) = self.photo.selected().cloned() else {
            return;
        };
        let path = photo
            .source
            .clone()
            .unwrap_or_else(|| PathBuf::from(&photo.name));
        let (tx, receiver) = mpsc::channel();
        std::thread::spawn(move || {
            let result = crate::formats::cli::photo_document(&photo)
                .map(crate::import::Imported::Document)
                .map(Completed::Imported);
            let _ = tx.send(result);
        });
        self.photo.status = "Developing full-resolution photo for Design…".into();
        self.file_jobs.push(ImportJob {
            receiver,
            path,
            mode: ImportMode::PhotoToDesign,
            owner: self.swap_id.clone(),
        });
    }

    pub(super) fn poll_file_jobs(&mut self, ctx: &egui::Context) {
        let mut i = 0;
        while i < self.file_jobs.len() {
            let result = match self.file_jobs[i].receiver.try_recv() {
                Ok(r) => r,
                Err(TryRecvError::Empty) => {
                    i += 1;
                    continue;
                }
                Err(TryRecvError::Disconnected) => {
                    Err("The file worker stopped unexpectedly".into())
                }
            };
            let job = self.file_jobs.remove(i);
            match result {
                Err(error) => self.status = error,
                Ok(Completed::Exported(notes)) => {
                    self.status = format!(
                        "Exported {}{}",
                        job.path.display(),
                        if notes.is_empty() {
                            ""
                        } else {
                            " — see conversion notes"
                        }
                    );
                    self.transfer_notes = notes;
                    self.show_import_notes = !self.transfer_notes.is_empty();
                }
                Ok(Completed::Imported(imported)) => {
                    // Opening a document always creates a tab. Placement belongs to its original tab.
                    if matches!(job.mode, ImportMode::Place | ImportMode::Drop(_))
                        && job.owner != self.swap_id
                        && !matches!(imported, crate::import::Imported::Document(_))
                    {
                        self.status="The destination tab changed. Place the file again in the intended tab.".into();
                        continue;
                    }
                    match imported {
                        crate::import::Imported::Document(doc) => {
                            self.transfer_notes = doc.import_notes.clone();
                            if matches!(job.mode, ImportMode::PhotoToDesign) {
                                if job.owner != self.swap_id {
                                    self.photo.status =
                                        "The destination tab changed. Place the photo again."
                                            .into();
                                    continue;
                                }
                                if self.current_is_blank() {
                                    self.open_document(doc, None);
                                    self.dirty = true;
                                } else {
                                    self.prepare_file_placement(
                                        PendingPlace::Document(doc),
                                        ImportMode::Place,
                                    );
                                    self.commit_place_at(Pt::new(
                                        self.doc.width * 0.5,
                                        self.doc.height * 0.5,
                                    ));
                                }
                                self.persona = Persona::Design;
                                self.tool = Tool::Select;
                                self.need_fit = true;
                                self.status = "Photo placed in Design".into();
                            } else if matches!(job.mode, ImportMode::Place) {
                                if job.owner != self.swap_id {
                                    self.status =
                                        "The destination tab changed. Place the file again.".into();
                                    continue;
                                }
                                self.prepare_file_placement(
                                    PendingPlace::Document(doc),
                                    ImportMode::Place,
                                );
                                self.show_import_notes = !self.transfer_notes.is_empty();
                            } else {
                                self.open_document(doc, Some(job.path));
                            }
                        }
                        crate::import::Imported::Svg { name, svg } => {
                            self.prepare_file_placement(PendingPlace::Svg { name, svg }, job.mode);
                        }
                        crate::import::Imported::Raster { name, image } => {
                            self.prepare_file_placement(
                                PendingPlace::Raster { name, image },
                                job.mode,
                            );
                        }
                        crate::import::Imported::Photo(photo) => {
                            self.photo.import_photo(*photo);
                            self.persona = Persona::Photo;
                            self.tool = Tool::Hand;
                            self.show_welcome = false;
                        }
                    }
                }
            }
        }
        if !self.file_jobs.is_empty() {
            ctx.request_repaint_after(std::time::Duration::from_millis(40));
        }
    }
    fn prepare_file_placement(&mut self, pending: PendingPlace, mode: ImportMode) {
        if matches!(mode, ImportMode::Open) && !self.current_is_blank() {
            self.new_tab();
        }
        self.persona = Persona::Design;
        self.show_welcome = false;
        self.tool = Tool::Select;
        self.status = format!("Click or drag to place {}", pending.name());
        self.pending_place = Some(pending);
        match mode {
            ImportMode::Open => {
                self.commit_place_at(Pt::new(self.doc.width * 0.5, self.doc.height * 0.5))
            }
            ImportMode::Drop(at) => self.commit_place_at(
                at.unwrap_or(Pt::new(self.doc.width * 0.5, self.doc.height * 0.5)),
            ),
            _ => {}
        }
    }

    pub fn export_layered(&mut self, extension: &str) {
        self.end_deform(false);
        self.end_pixel_stroke(false);
        self.commit_type_edit();
        let Some(path) = crate::project::dialog_export(&extension.to_ascii_uppercase(), extension)
        else {
            return;
        };
        if self.file_jobs.len() >= 4 {
            self.status = "Wait for the current files to finish".into();
            return;
        }
        let doc = self.doc.clone();
        let extension = extension.to_string();
        let output = path.clone();
        let (tx, receiver) = mpsc::channel();
        std::thread::spawn(move || {
            let result = (|| {
                let (bytes, notes) = match extension.as_str() {
                    "psd" | "psb" => {
                        let output = crate::formats::psd::encode(&doc, extension == "psb")?;
                        (output.bytes, output.warnings)
                    }
                    "pdf" => crate::formats::pdf::write(&doc)?,
                    "ora" => crate::formats::openraster::write(&doc)?,
                    _ => return Err("Unsupported layered export".into()),
                };
                crate::formats::write_atomic(&output, &bytes)?;
                Ok(Completed::Exported(notes))
            })();
            let _ = tx.send(result);
        });
        self.status = format!("Exporting {}…", path.display());
        self.file_jobs.push(ImportJob {
            receiver,
            path,
            mode: ImportMode::Export,
            owner: self.swap_id.clone(),
        });
    }

    pub(super) fn import_notes_window(&mut self, ctx: &egui::Context) {
        if !self.show_import_notes {
            return;
        }
        egui::Window::new("Conversion notes")
            .open(&mut self.show_import_notes)
            .default_width(540.)
            .show(ctx, |ui| {
                ui.label("Review these changes before continuing with the converted document.");
                ui.separator();
                egui::ScrollArea::vertical()
                    .max_height(420.)
                    .show(ui, |ui| {
                        if self.transfer_notes.is_empty() {
                            ui.label("No conversion changes were reported.");
                        }
                        for note in &self.transfer_notes {
                            ui.label(format!("• {note}"));
                            ui.add_space(5.);
                        }
                    });
            });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestFolder(PathBuf);
    impl TestFolder {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "omadesign-raw-file-io-{}-{}",
                std::process::id(),
                crate::document::next_id()
            ));
            std::fs::create_dir(&path).unwrap();
            Self(path)
        }
    }
    impl Drop for TestFolder {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn finish_jobs(studio: &mut Studio) {
        let ctx = egui::Context::default();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while !studio.file_jobs.is_empty() || studio.photo.is_loading() {
            studio.poll_file_jobs(&ctx);
            studio.photo.poll(&ctx);
            assert!(
                std::time::Instant::now() < deadline,
                "File import worker did not finish"
            );
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
    }

    fn wide_raw_photo() -> crate::photo::PhotoImage {
        crate::photo::PhotoImage::from_raw(
            "Wide RAW".into(),
            crate::formats::raw::RawImage {
                width: 1800,
                height: 12,
                pixels: [5000, 10000, 15000].repeat(1800 * 12),
                metadata: Default::default(),
                warnings: vec![],
            },
        )
    }

    #[test]
    fn raw_open_uses_photo_worker_once_and_failed_raw_keeps_existing_photo() {
        let folder = TestFolder::new();
        let source = include_bytes!("../formats/raw/synthetic.dng");
        let path = folder.0.join("Camera.DNG");
        std::fs::write(&path, source).unwrap();
        let mut studio = Studio::new();
        studio.open_path(path.clone());
        studio.open_path(path.clone());
        assert!(studio.persona == Persona::Photo);
        assert!(!studio.show_welcome);
        assert!(studio.photo.is_loading());
        assert!(
            studio.file_jobs.is_empty(),
            "RAW should not also create a Design import job"
        );
        finish_jobs(&mut studio);
        assert_eq!(
            studio.photo.images.len(),
            1,
            "repeated Open while loading must not decode twice"
        );
        let selected = studio.photo.selected().unwrap();
        assert!(selected.raw.is_some());
        assert_eq!(selected.dimensions(), (64, 48));
        assert_eq!(
            selected.source.as_ref(),
            Some(&path.canonicalize().unwrap())
        );
        assert!(studio.pending_place.is_none());
        let pixels = selected.raw.as_ref().unwrap().pixels.clone();
        let broken = folder.0.join("Broken.CR3");
        std::fs::write(&broken, b"unsupported camera data").unwrap();
        studio.open_path(broken);
        finish_jobs(&mut studio);
        assert_eq!(studio.photo.images.len(), 1);
        assert_eq!(studio.photo.selected, Some(0));
        assert_eq!(
            studio
                .photo
                .selected()
                .unwrap()
                .raw
                .as_ref()
                .unwrap()
                .pixels,
            pixels
        );
        assert!(studio.photo.status.to_ascii_lowercase().contains("raw"));
        assert_eq!(std::fs::read(&path).unwrap().as_slice(), source.as_slice());
        assert!(!crate::photo::edits::sidecar_path(&path).exists());
    }

    #[test]
    fn queued_design_image_open_keeps_its_destination_after_persona_switch() {
        let folder = TestFolder::new();
        let path = folder.0.join("Placed.png");
        let image = RgbaImage::new(13, 9, [80, 120, 160, 255].repeat(13 * 9)).unwrap();
        std::fs::write(&path, image.encode_png().unwrap()).unwrap();
        let mut studio = Studio::new();
        assert!(studio.persona == Persona::Design);
        studio.open_path(path);
        assert_eq!(studio.file_jobs.len(), 1);
        studio.persona = Persona::Photo;
        finish_jobs(&mut studio);
        assert!(studio.persona == Persona::Design);
        assert!(studio.photo.images.is_empty());
        let placed = studio.doc.layers.last().unwrap().kind.pixels().unwrap();
        assert_eq!((placed.w, placed.h), (13, 9));
        assert_eq!(placed.data, image.data);
        assert!(studio.pending_place.is_none());
    }

    #[test]
    fn settings_open_and_drop_restore_photo_without_replacing_dirty_artwork() {
        let folder = TestFolder::new();
        let source = folder.0.join("photo.png");
        let pixels = RgbaImage::new(13, 9, [80, 120, 160, 255].repeat(13 * 9)).unwrap();
        let original = pixels.encode_png().unwrap();
        std::fs::write(&source, &original).unwrap();
        let params = crate::photo::DevelopParams {
            exposure: -0.7,
            rotate: 90,
            ..Default::default()
        };
        let settings = crate::photo::edits::save(
            &source,
            &crate::photo::edits::SourceIdentity::read(&source).unwrap(),
            &params,
        )
        .unwrap();
        for drop in [false, true] {
            let mut studio = Studio::new();
            studio.doc.name = "Keep this artwork".into();
            studio.dirty = true;
            let artwork = serde_json::to_vec(&studio.doc).unwrap();
            if drop {
                studio.ingest_dropped(&settings, None);
            } else {
                studio.open_path(settings.clone());
            }
            finish_jobs(&mut studio);
            assert!(studio.persona == Persona::Photo);
            assert!(!studio.show_welcome);
            assert!(studio.pending_place.is_none() && studio.file_jobs.is_empty());
            assert_eq!(studio.photo.selected().unwrap().develop, params);
            assert_eq!(
                studio.photo.selected().unwrap().source.as_ref(),
                Some(&source)
            );
            assert!(studio.dirty);
            assert_eq!(serde_json::to_vec(&studio.doc).unwrap(), artwork);
            assert_eq!(std::fs::read(&source).unwrap(), original);
        }
    }

    #[test]
    fn photo_to_design_uses_full_resolution_and_preserves_destination_and_undo() {
        let root = if let Some(root) = std::env::var_os("OMADESIGN_RAW_PLACEMENT_TEST_DIR") {
            PathBuf::from(root)
        } else {
            let root = TestFolder::new();
            let output = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "app::file_io::tests::photo_to_design_uses_full_resolution_and_preserves_destination_and_undo", "--nocapture"])
                .env("OMADESIGN_RAW_PLACEMENT_TEST_DIR", &root.0)
                .env("XDG_CONFIG_HOME", root.0.join("config"))
                .env("XDG_STATE_HOME", root.0.join("state"))
                .env("XDG_DATA_HOME", root.0.join("data"))
                .output().unwrap();
            assert!(
                output.status.success(),
                "{}\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            return;
        };
        assert!(root.exists());

        let mut blank = Studio::new();
        let photo = wide_raw_photo();
        assert_eq!(photo.preview.w, 1600);
        blank.photo.import_photo(photo.clone());
        blank.persona = Persona::Photo;
        blank.send_photo_to_design();
        finish_jobs(&mut blank);
        assert!(blank.persona == Persona::Design);
        assert_eq!((blank.doc.width, blank.doc.height), (1800.0, 12.0));
        assert_eq!(blank.doc.layers.len(), 1);
        let pixels = blank.doc.layers[0].kind.pixels().unwrap();
        assert_eq!((pixels.w, pixels.h), (1800, 12));
        assert!(blank.dirty && blank.path.is_none());
        assert_eq!(blank.photo.selected().unwrap().dimensions(), (1800, 12));

        let mut existing = Studio::new();
        existing.doc = Document::new("Existing artwork", 800.0, 600.0, 72.0);
        existing.dirty = true;
        let before = serde_json::to_value(&existing.doc.layers).unwrap();
        existing.photo.import_photo(photo.clone());
        existing.persona = Persona::Photo;
        existing.send_photo_to_design();
        finish_jobs(&mut existing);
        assert_eq!((existing.doc.width, existing.doc.height), (800.0, 600.0));
        let placed = existing.doc.layers.last().unwrap().kind.pixels().unwrap();
        assert_eq!((placed.w, placed.h), (1800, 12));
        assert!(existing.pending_place.is_none());
        existing.undo();
        assert_eq!(serde_json::to_value(&existing.doc.layers).unwrap(), before);

        let mut changed_tab = Studio::new();
        changed_tab.dirty = true;
        changed_tab.photo.import_photo(photo);
        changed_tab.send_photo_to_design();
        changed_tab.new_tab();
        let destination = serde_json::to_value(&changed_tab.doc).unwrap();
        finish_jobs(&mut changed_tab);
        assert_eq!(serde_json::to_value(&changed_tab.doc).unwrap(), destination);
        assert!(changed_tab.photo.status.contains("destination tab changed"));
        assert!(changed_tab.pending_place.is_none());
    }

    #[test]
    fn foreign_layered_open_saves_a_new_native_document_and_preserves_source() {
        // Opening and saving update recents/recovery, so exercise that complete
        // path in a child process with an isolated application configuration.
        let root = if let Some(root) = std::env::var_os("OMADESIGN_FILE_IO_TEST_DIR") {
            PathBuf::from(root)
        } else {
            let root = std::env::temp_dir().join(format!(
                "oma-file-io-test-{}-{}",
                std::process::id(),
                crate::document::next_id()
            ));
            std::fs::create_dir(&root).unwrap();
            let output = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "app::file_io::tests::foreign_layered_open_saves_a_new_native_document_and_preserves_source", "--nocapture"])
                .env("OMADESIGN_FILE_IO_TEST_DIR", &root)
                .env("XDG_CONFIG_HOME", root.join("config"))
                .env("XDG_STATE_HOME", root.join("state"))
                .env("XDG_DATA_HOME", root.join("data"))
                .output().unwrap();
            let _ = std::fs::remove_dir_all(root);
            assert!(
                output.status.success(),
                "{}\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            return;
        };
        let source = include_bytes!("../formats/psd/fixtures/independent-rle.psd");
        let input = root.join("Artwork.psd");
        std::fs::write(&input, source).unwrap();
        let imported = crate::import::open_any(&input).unwrap_or_else(|e| panic!("{e}"));
        let mut studio = Studio::new();
        studio.doc = Document::new("Existing work", 8.0, 8.0, 72.0);
        studio.dirty = true;
        studio.ensure_tabs();
        let previous = serde_json::to_value(&studio.doc).unwrap();
        let (tx, receiver) = mpsc::channel();
        assert!(tx.send(Ok(Completed::Imported(imported))).is_ok());
        studio.file_jobs.push(ImportJob {
            receiver,
            path: input.clone(),
            mode: ImportMode::Open,
            owner: studio.swap_id.clone(),
        });
        studio.poll_file_jobs(&egui::Context::default());
        assert_eq!(studio.tab_count(), 2);
        assert!(studio.dirty);
        assert!(
            studio.path.is_none(),
            "Save must not target the original PSD"
        );
        assert_eq!((studio.doc.width, studio.doc.height), (4.0, 3.0));
        assert_eq!(studio.doc.layers.len(), 3);
        let native = root.join("Artwork.oma");
        studio.save_document(&native).unwrap();
        assert!(!studio.dirty);
        assert_eq!(studio.path.as_ref(), Some(&native));
        assert_eq!(std::fs::read(&input).unwrap().as_slice(), source.as_slice());
        let restored = crate::project::load_from(&native).unwrap();
        assert_eq!(restored.layers.len(), 3);
        assert_eq!(restored.layers[1].parent, Some(restored.layers[2].id));
        studio.switch_tab(0);
        assert_eq!(serde_json::to_value(&studio.doc).unwrap(), previous);
        assert!(studio.dirty);
    }
}
