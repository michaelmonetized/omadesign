//! Decode large documents away from the UI thread; apply results on their owner.
use super::*;
use std::sync::mpsc::{self, Receiver, TryRecvError};

#[derive(Clone, Copy)]
pub(super) enum ImportMode {
    Open,
    Place,
    Drop(Option<Pt>),
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
        if self.file_jobs.len() >= 4 {
            self.status = "Wait for the current files to finish opening".into();
            return;
        }
        let (tx, receiver) = mpsc::channel();
        let input = path.clone();
        std::thread::spawn(move || {
            let _ = tx.send(crate::import::open_any(&input).map(Completed::Imported));
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
                            if matches!(job.mode, ImportMode::Place) {
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
                            if self.persona == Persona::Photo
                                && matches!(job.mode, ImportMode::Open)
                            {
                                self.photo.import_image(name, image);
                                self.show_welcome = false;
                                continue;
                            }
                            self.prepare_file_placement(
                                PendingPlace::Raster { name, image },
                                job.mode,
                            );
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
