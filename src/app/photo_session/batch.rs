use super::*;
use std::io::Read;

const MAX_JOURNAL_BYTES: usize = 32 * 1024 * 1024;

#[derive(Clone, Debug, Default)]
pub struct BatchProgress {
    pub completed: usize,
    pub total: usize,
    pub failed: usize,
    pub cancelled: bool,
    pub current: String,
    pub errors: Vec<String>,
}

#[derive(Clone)]
pub(super) struct DiskChange {
    pub source: PathBuf,
    pub identity: photo::edits::SourceIdentity,
    pub path: PathBuf,
    pub before: Option<Vec<u8>>,
    pub after: Vec<u8>,
    pub before_params: DevelopParams,
    pub after_params: DevelopParams,
}

impl DiskChange {
    pub fn bytes(&self) -> usize {
        self.before.as_ref().map_or(0, Vec::len) + self.after.len()
    }
}

pub(super) struct LoadedSource {
    pub source: PathBuf,
    pub destination: PathBuf,
    pub identity: photo::edits::SourceIdentity,
    pub params: DevelopParams,
}

pub(super) enum BatchEvent {
    Progress(BatchProgress),
    Finished(Result<PhotoEdit, String>, BatchProgress),
}

pub(super) struct BatchJob {
    pub receiver: mpsc::Receiver<BatchEvent>,
    pub cancel: Arc<AtomicBool>,
    /// None applies new settings; Some(true/false) replays undo/redo.
    pub replay: Option<bool>,
    pub pending: Option<PhotoEdit>,
}

fn read_sidecar(path: &Path) -> Result<Option<Vec<u8>>, String> {
    match std::fs::File::open(path) {
        Ok(file) => {
            if !file.metadata().map_err(|e| e.to_string())?.is_file() {
                return Err("The settings destination is not a regular file.".into());
            }
            let mut bytes = Vec::new();
            file.take(64 * 1024 + 1)
                .read_to_end(&mut bytes)
                .map_err(|e| e.to_string())?;
            if bytes.len() > 64 * 1024 {
                return Err("Photo settings exceed 64 KiB.".into());
            }
            Ok(Some(bytes))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.to_string()),
    }
}

fn prepare(
    source: PathBuf,
    loaded: &[LoadedSource],
    snapshot: &photo::transfer::AdjustmentSnapshot,
    settings: Option<&[PathBuf]>,
) -> Result<DiskChange, String> {
    if settings.is_some_and(|paths| paths.len() > 1) {
        return Err("Multiple settings files match this original. Keep one .omaphoto file before applying a folder batch.".into());
    }
    let source = source.canonicalize().map_err(|e| e.to_string())?;
    let identity = photo::edits::SourceIdentity::read(&source)?;
    let open: Vec<_> = loaded.iter().filter(|p| p.source == source).collect();
    if open.iter().any(|p| p.identity != identity) {
        return Err("The source changed after opening; reopen it first.".into());
    }
    if open
        .windows(2)
        .any(|p| p[0].params != p[1].params || p[0].destination != p[1].destination)
    {
        return Err("This original is open with conflicting edits. Keep one development before applying a folder batch.".into());
    }
    let path = open.first().map_or_else(
        || {
            settings
                .and_then(|paths| paths.first())
                .cloned()
                .unwrap_or_else(|| photo::edits::sidecar_path(&source))
        },
        |p| p.destination.clone(),
    );
    // Match later PhotoImage imports without performing filesystem lookups on
    // the UI thread. Normalize the parent without following a sidecar symlink.
    let path = path
        .parent()
        .unwrap_or(Path::new("."))
        .canonicalize()
        .map_err(|e| e.to_string())?
        .join(
            path.file_name()
                .ok_or("The settings filename is missing.")?,
        );
    let before = read_sidecar(&path)?;
    // Validate the on-disk settings even when unsaved loaded edits provide the base.
    let persisted = if before.is_some() {
        photo::edits::load_file(&path, &identity)?
    } else {
        DevelopParams::default()
    };
    let before_params = open.first().map_or(persisted, |p| p.params.clone());
    let mut after_params = before_params.clone();
    snapshot
        .categories
        .apply(&snapshot.params, &mut after_params);
    let after = photo::edits::encode(&identity, &after_params)?;
    Ok(DiskChange {
        source,
        identity,
        path,
        before,
        after,
        before_params,
        after_params,
    })
}

fn write(change: &DiskChange, undo: bool) -> Result<(), String> {
    if photo::edits::SourceIdentity::read(&change.source)? != change.identity {
        return Err("The original changed; reopen it before changing its saved settings.".into());
    }
    let expected = if undo {
        Some(&change.after)
    } else {
        change.before.as_ref()
    };
    if read_sidecar(&change.path)?.as_ref() != expected {
        return Err("Saved settings changed outside this batch; they were preserved.".into());
    }
    let wanted = if undo {
        change.before.as_ref()
    } else {
        Some(&change.after)
    };
    match wanted {
        Some(bytes) => crate::formats::write_atomic(&change.path, bytes).map_err(|e| e.to_string()),
        None => std::fs::remove_file(&change.path).map_err(|e| e.to_string()),
    }
}

pub(super) fn apply_folder(
    folder: String,
    snapshot: photo::transfer::AdjustmentSnapshot,
    loaded: Vec<LoadedSource>,
    cancel: &AtomicBool,
    tx: &mpsc::SyncSender<BatchEvent>,
) -> (Result<PhotoEdit, String>, BatchProgress) {
    apply_folder_with_budget(folder, snapshot, loaded, cancel, tx, MAX_JOURNAL_BYTES)
}

fn apply_folder_with_budget(
    folder: String,
    snapshot: photo::transfer::AdjustmentSnapshot,
    loaded: Vec<LoadedSource>,
    cancel: &AtomicBool,
    tx: &mpsc::SyncSender<BatchEvent>,
    journal_budget: usize,
) -> (Result<PhotoEdit, String>, BatchProgress) {
    let mut progress = BatchProgress::default();
    let listing = match list_folder(&folder, cancel) {
        Ok(listing) if !listing.truncated => listing,
        Ok(_) => {
            return (
                Err(format!(
                    "This folder exceeds {MAX_FOLDER_FILES} photos. Split it into smaller folders before applying settings."
                )),
                progress,
            );
        }
        Err(error) => return (Err(error), progress),
    };
    progress.total = listing.files.len();
    let mut prepared = Vec::new();
    let mut bytes = 0;
    // Reserve the complete undo record before the first write. No RAW pixels are decoded.
    for (name, path) in listing.files {
        if cancel.load(Ordering::Relaxed) {
            progress.cancelled = true;
            return (Ok(PhotoEdit::default()), progress);
        }
        progress.current = format!("Checking {name}");
        let _ = tx.send(BatchEvent::Progress(progress.clone()));
        let source = PathBuf::from(path);
        let settings = listing.settings.get(&source).map(Vec::as_slice);
        match prepare(source, &loaded, &snapshot, settings) {
            Ok(change) => {
                bytes += change.bytes();
                if bytes > journal_budget {
                    return (Err("This folder needs more than 32 MiB of undo data. No settings were written; choose a smaller folder.".into()), progress);
                }
                prepared.push(change);
            }
            Err(error) => {
                progress.failed += 1;
                progress.completed += 1;
                progress.errors.push(format!("{name}: {error}"));
            }
        }
    }
    let mut edit = PhotoEdit::default();
    for change in prepared {
        if cancel.load(Ordering::Relaxed) {
            progress.cancelled = true;
            break;
        }
        progress.current = change
            .source
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        match write(&change, false) {
            Ok(()) => edit.disk.push(change),
            Err(error) => {
                progress.failed += 1;
                progress
                    .errors
                    .push(format!("{}: {error}", progress.current));
            }
        }
        progress.completed += 1;
        let _ = tx.send(BatchEvent::Progress(progress.clone()));
    }
    (Ok(edit), progress)
}

pub(super) fn replay(
    edit: &PhotoEdit,
    undo: bool,
    cancel: &AtomicBool,
    tx: &mpsc::SyncSender<BatchEvent>,
) -> (Result<PhotoEdit, String>, BatchProgress) {
    let mut progress = BatchProgress {
        total: edit.disk.len(),
        ..Default::default()
    };
    let mut changed = PhotoEdit::default();
    for disk in &edit.disk {
        if cancel.load(Ordering::Relaxed) {
            progress.cancelled = true;
            break;
        }
        progress.current = disk
            .source
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        match write(disk, undo) {
            Ok(()) => changed.disk.push(disk.clone()),
            Err(error) => {
                progress.failed += 1;
                progress
                    .errors
                    .push(format!("{}: {error}", progress.current));
            }
        }
        progress.completed += 1;
        let _ = tx.send(BatchEvent::Progress(progress.clone()));
    }
    changed.images = edit
        .images
        .iter()
        .filter(|image| {
            image
                .source
                .as_ref()
                .is_none_or(|source| changed.disk.iter().any(|d| &d.source == source))
        })
        .cloned()
        .collect();
    (Ok(changed), progress)
}

impl PhotoSession {
    pub fn batch_progress(&self) -> Option<&BatchProgress> {
        self.batch_progress.as_ref()
    }
    pub fn is_batching(&self) -> bool {
        self.batch_job.is_some()
    }
    pub fn cancel_folder_batch(&mut self) {
        if let Some(job) = &self.batch_job {
            job.cancel.store(true, Ordering::Relaxed);
        }
    }

    pub fn start_folder_batch(&mut self, snapshot: AdjustmentSnapshot) -> Result<(), String> {
        snapshot.validate()?;
        if self.is_saving() || self.is_loading() || self.folder_job.is_some() {
            return Err("Wait for the current Photo operation to finish.".into());
        }
        if self.folder.is_empty() {
            return Err("Open a Photo folder first.".into());
        }
        self.finish_edit();
        self.crop_drag = None;
        self.ensure_saved();
        let folder = self.folder.clone();
        let loaded = self
            .images
            .iter()
            .filter_map(|image| {
                Some(LoadedSource {
                    source: image.source.clone()?,
                    destination: image.settings_path.clone()?,
                    identity: image.source_identity.clone()?,
                    params: image.develop.clone(),
                })
            })
            .collect();
        let cancel = Arc::new(AtomicBool::new(false));
        let worker_cancel = cancel.clone();
        let (tx, receiver) = mpsc::sync_channel(8);
        std::thread::Builder::new()
            .name("photo-folder-settings".into())
            .spawn(move || {
                let (result, progress) =
                    apply_folder(folder, snapshot, loaded, &worker_cancel, &tx);
                let _ = tx.send(BatchEvent::Finished(result, progress));
            })
            .map_err(|e| format!("Could not start the folder batch: {e}"))?;
        self.batch_progress = Some(BatchProgress::default());
        self.batch_job = Some(BatchJob {
            receiver,
            cancel,
            replay: None,
            pending: None,
        });
        self.save_error.clear();
        self.status = "Checking folder settings before applying…".into();
        Ok(())
    }

    pub(super) fn start_replay(&mut self, edit: PhotoEdit, undo: bool) {
        self.crop_drag = None;
        let work = edit.clone();
        let cancel = Arc::new(AtomicBool::new(false));
        let worker_cancel = cancel.clone();
        let (tx, receiver) = mpsc::sync_channel(8);
        if let Err(e) = std::thread::Builder::new()
            .name("photo-folder-undo".into())
            .spawn(move || {
                let (result, progress) = replay(&work, undo, &worker_cancel, &tx);
                let _ = tx.send(BatchEvent::Finished(result, progress));
            })
        {
            (if undo {
                &mut self.undo_edits
            } else {
                &mut self.redo_edits
            })
            .push(edit);
            self.report_write_error(format!("Could not start restoring settings: {e}"));
            return;
        }
        self.batch_progress = Some(BatchProgress::default());
        self.batch_job = Some(BatchJob {
            receiver,
            cancel,
            replay: Some(undo),
            pending: Some(edit),
        });
        self.status = if undo {
            "Restoring previous folder settings…"
        } else {
            "Reapplying folder settings…"
        }
        .into();
    }

    pub(super) fn poll_batch(&mut self) {
        loop {
            let event = match self.batch_job.as_ref().map(|job| job.receiver.try_recv()) {
                Some(Ok(event)) => event,
                Some(Err(mpsc::TryRecvError::Disconnected)) => {
                    let job = self.batch_job.take().unwrap();
                    if let (Some(undo), Some(edit)) = (job.replay, job.pending) {
                        (if undo {
                            &mut self.undo_edits
                        } else {
                            &mut self.redo_edits
                        })
                        .push(edit);
                    }
                    self.report_write_error("The folder worker stopped unexpectedly. Check its saved settings before retrying.".into());
                    break;
                }
                _ => break,
            };
            match event {
                BatchEvent::Progress(progress) => self.batch_progress = Some(progress),
                BatchEvent::Finished(result, progress) => {
                    let job = self.batch_job.take().unwrap();
                    self.ensure_saved();
                    match result {
                        Ok(mut edit) => {
                            if let Some(undo) = job.replay {
                                let mut pending = job.pending.unwrap();
                                pending.disk.retain(|disk| {
                                    !edit.disk.iter().any(|done| done.path == disk.path)
                                });
                                pending.images.retain(|image| {
                                    !edit.images.iter().any(|done| done.index == image.index)
                                });
                                self.restore_images(&edit, undo);
                                if !pending.is_empty() {
                                    (if undo {
                                        &mut self.undo_edits
                                    } else {
                                        &mut self.redo_edits
                                    })
                                    .push(pending);
                                }
                                if !edit.is_empty() {
                                    (if undo {
                                        &mut self.redo_edits
                                    } else {
                                        &mut self.undo_edits
                                    })
                                    .push(edit);
                                }
                            } else {
                                for disk in &edit.disk {
                                    for (index, image) in self.images.iter_mut().enumerate() {
                                        if image.source.as_ref() == Some(&disk.source) {
                                            // A background result never erases newer in-memory edits.
                                            if image.develop == disk.before_params {
                                                edit.images.push(ImageChange {
                                                    index,
                                                    source: image.source.clone(),
                                                    before: image.develop.clone(),
                                                    after: disk.after_params.clone(),
                                                });
                                                image.develop = disk.after_params.clone();
                                            }
                                            self.saved[index] = disk.after_params.clone();
                                        }
                                    }
                                }
                                self.push_edit(edit);
                            }
                            let written = progress.completed.saturating_sub(progress.failed);
                            self.status = format!(
                                "{}: {written} of {} settings saved, {} failed.{}",
                                if progress.cancelled {
                                    "Batch cancelled"
                                } else {
                                    "Batch finished"
                                },
                                progress.total,
                                progress.failed,
                                if written > 0 {
                                    " Undo is available."
                                } else {
                                    ""
                                }
                            );
                            self.save_error = progress.errors.join("\n");
                        }
                        Err(error) => {
                            if let (Some(undo), Some(edit)) = (job.replay, job.pending) {
                                (if undo {
                                    &mut self.undo_edits
                                } else {
                                    &mut self.redo_edits
                                })
                                .push(edit);
                            }
                            self.report_write_error(error);
                        }
                    }
                    self.batch_progress = Some(progress);
                    break;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn undo_budget_is_reserved_before_any_folder_settings_are_written() {
        let fixture = super::super::tests::Fixture::new();
        let original = super::super::tests::image().full.encode_png().unwrap();
        for name in ["first.png", "second.png"] {
            std::fs::write(fixture.0.join(name), &original).unwrap();
        }
        let snapshot = AdjustmentSnapshot {
            name: "Light".into(),
            params: DevelopParams {
                exposure: 1.0,
                ..Default::default()
            },
            categories: Categories::default(),
        };
        let first = prepare(fixture.0.join("first.png"), &[], &snapshot, None).unwrap();
        let (tx, _rx) = mpsc::sync_channel(8);
        let (result, _) = apply_folder_with_budget(
            fixture.0.to_string_lossy().into_owned(),
            snapshot,
            vec![],
            &AtomicBool::new(false),
            &tx,
            first.bytes() + 1,
        );
        assert!(result.err().unwrap().contains("No settings were written"));
        assert!(!fixture.0.join("first.png.omaphoto").exists());
        assert!(!fixture.0.join("second.png.omaphoto").exists());
    }

    #[test]
    fn cancelling_after_writes_keeps_completed_changes_undoable() {
        let fixture = super::super::tests::Fixture::new();
        let original = super::super::tests::image().full.encode_png().unwrap();
        for index in 0..20 {
            std::fs::write(fixture.0.join(format!("{index:02}.png")), &original).unwrap();
        }
        let folder = fixture.0.to_string_lossy().into_owned();
        let snapshot = AdjustmentSnapshot {
            name: "Light".into(),
            params: DevelopParams {
                exposure: 2.0,
                ..Default::default()
            },
            categories: Categories::default(),
        };
        let cancel = Arc::new(AtomicBool::new(false));
        let worker_cancel = cancel.clone();
        let (tx, rx) = mpsc::sync_channel(0);
        let worker = std::thread::spawn(move || {
            let (result, progress) = apply_folder(folder, snapshot, vec![], &worker_cancel, &tx);
            let _ = tx.send(BatchEvent::Finished(result, progress));
        });
        let (edit, progress) = loop {
            match rx.recv().unwrap() {
                BatchEvent::Progress(progress) if progress.completed >= 2 => {
                    cancel.store(true, Ordering::Relaxed)
                }
                BatchEvent::Finished(result, progress) => break (result.unwrap(), progress),
                _ => {}
            }
        };
        worker.join().unwrap();
        assert!(progress.cancelled);
        assert!((2..20).contains(&edit.disk.len()));
        assert_eq!(edit.disk.len(), progress.completed);
        let mut session = PhotoSession::new();
        session.push_edit(edit);
        session.undo();
        let deadline = std::time::Instant::now();
        while session.is_saving() {
            session.poll(&egui::Context::default());
            assert!(deadline.elapsed().as_secs() < 5);
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        for index in 0..20 {
            assert_eq!(
                std::fs::read(fixture.0.join(format!("{index:02}.png"))).unwrap(),
                original
            );
            assert!(!fixture.0.join(format!("{index:02}.png.omaphoto")).exists());
        }
        assert!(session.can_redo());
    }
}
