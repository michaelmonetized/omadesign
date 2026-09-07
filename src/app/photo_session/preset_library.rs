use super::*;
use photo::presets::{self, Preset};

pub(super) struct PresetState {
    pub(super) path: PathBuf,
    loaded: bool,
    attempted: bool,
    job: Option<PresetJob>,
    error: String,
}

impl Default for PresetState {
    fn default() -> Self {
        Self {
            path: presets::library_path(),
            loaded: false,
            attempted: false,
            job: None,
            error: String::new(),
        }
    }
}

struct PresetJob {
    receiver: mpsc::Receiver<Result<Vec<Preset>, String>>,
    loading: bool,
    replace: bool,
}

impl PhotoSession {
    pub fn ensure_presets_loaded(&mut self) {
        if self.preset_state.attempted || self.preset_state.job.is_some() {
            return;
        }
        self.preset_state.attempted = true;
        let path = self.preset_state.path.clone();
        self.start_preset_job(true, true, move || {
            if path.exists() {
                presets::load_file(&path)
            } else {
                Ok(vec![])
            }
        });
    }

    pub fn reload_presets(&mut self) {
        if self.preset_state.job.is_none() {
            self.preset_state.attempted = false;
            self.ensure_presets_loaded();
        }
    }

    pub fn is_loading_presets(&self) -> bool {
        self.preset_state
            .job
            .as_ref()
            .is_some_and(|job| job.loading)
    }
    pub fn is_saving_presets(&self) -> bool {
        self.preset_state
            .job
            .as_ref()
            .is_some_and(|job| !job.loading)
    }
    pub fn preset_error(&self) -> &str {
        &self.preset_state.error
    }

    fn require_presets(&self) -> Result<(), String> {
        if self.preset_state.job.is_some() {
            return Err("Wait for the current preset operation.".into());
        }
        if !self.preset_state.loaded {
            return Err("Open the preset library before saving or importing presets.".into());
        }
        Ok(())
    }

    fn start_preset_job(
        &mut self,
        loading: bool,
        replace: bool,
        work: impl FnOnce() -> Result<Vec<Preset>, String> + Send + 'static,
    ) {
        let (tx, receiver) = mpsc::channel();
        if let Err(e) = std::thread::Builder::new()
            .name("photo-presets".into())
            .spawn(move || {
                let _ = tx.send(work());
            })
        {
            self.preset_state.error = format!("Could not start the preset operation: {e}");
            return;
        }
        self.preset_state.error.clear();
        self.preset_state.job = Some(PresetJob {
            receiver,
            loading,
            replace,
        });
    }

    fn persist_presets(&mut self, presets: Vec<Preset>) {
        let path = self.preset_state.path.clone();
        let current = self.presets.clone();
        self.start_preset_job(false, true, move || {
            let _lock = lock_library(&path)?;
            check_library(&path, &current)?;
            std::fs::create_dir_all(path.parent().unwrap()).map_err(|e| e.to_string())?;
            presets::save_file(&path, &presets)?;
            Ok(presets)
        });
    }

    pub fn save_preset(&mut self, name: &str, categories: Categories) -> Result<(), String> {
        self.require_presets()?;
        let image = self.selected().ok_or("Open a photo first.")?;
        let preset = Preset {
            name: name.trim().into(),
            params: image.develop.clone(),
            categories,
        };
        preset.validate()?;
        if self
            .presets
            .iter()
            .any(|p| p.name.eq_ignore_ascii_case(&preset.name))
        {
            return Err("A preset already uses this name. Choose another name or delete the old preset first.".into());
        }
        let mut presets = self.presets.clone();
        presets.push(preset);
        presets::encode(&presets)?;
        self.persist_presets(presets);
        Ok(())
    }

    pub fn delete_preset(&mut self, index: usize) -> Result<(), String> {
        self.require_presets()?;
        if index >= self.presets.len() {
            return Err("Choose a preset first.".into());
        }
        let mut presets = self.presets.clone();
        presets.remove(index);
        self.persist_presets(presets);
        Ok(())
    }

    pub fn import_presets(&mut self, path: PathBuf) -> Result<(), String> {
        self.require_presets()?;
        let current = self.presets.clone();
        let destination = self.preset_state.path.clone();
        self.start_preset_job(false, true, move || {
            let _lock = lock_library(&destination)?;
            check_library(&destination, &current)?;
            let presets = presets::merge(&current, &presets::load_file(&path)?)?;
            std::fs::create_dir_all(destination.parent().unwrap()).map_err(|e| e.to_string())?;
            presets::save_file(&destination, &presets)?;
            Ok(presets)
        });
        Ok(())
    }

    pub fn export_preset(&mut self, index: usize, path: PathBuf) -> Result<(), String> {
        self.require_presets()?;
        let preset = self
            .presets
            .get(index)
            .cloned()
            .ok_or("Choose a preset first.")?;
        let library = self.preset_state.path.clone();
        self.start_preset_job(false, false, move || {
            if path
                .canonicalize()
                .ok()
                .is_some_and(|path| Some(path) == library.canonicalize().ok())
            {
                return Err(
                    "Export to a separate .omapreset file; this path is your preset library."
                        .into(),
                );
            }
            presets::save_file(&path, &[preset])?;
            Ok(vec![])
        });
        Ok(())
    }

    pub fn apply_preset(&mut self, index: usize) -> Result<usize, String> {
        let preset = self
            .presets
            .get(index)
            .cloned()
            .ok_or("Choose a preset first.")?;
        self.apply_adjustments(&preset)
    }

    pub(super) fn poll_presets(&mut self) {
        let Some(result) = self
            .preset_state
            .job
            .as_ref()
            .and_then(|job| completed(&job.receiver))
        else {
            return;
        };
        let job = self.preset_state.job.take().unwrap();
        match result {
            Ok(presets) => {
                if job.replace {
                    self.presets = presets;
                    self.preset_state.loaded = true;
                }
                self.status = if job.loading {
                    "Photo presets loaded"
                } else {
                    "Photo presets saved"
                }
                .into();
            }
            Err(error) => {
                self.preset_state.error = error.clone();
                self.status = error;
            }
        }
    }
}

// A separate lock inode survives atomic replacement of the JSON file. The OS
// releases it if another app window exits; no stale-lock recovery is required.
fn lock_library(path: &Path) -> Result<std::fs::File, String> {
    std::fs::create_dir_all(path.parent().unwrap_or(Path::new("."))).map_err(|e| e.to_string())?;
    let mut lock_path = path.as_os_str().to_owned();
    lock_path.push(".lock");
    let file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(PathBuf::from(lock_path))
        .map_err(|e| format!("Could not open the preset library lock: {e}"))?;
    file.lock()
        .map_err(|e| format!("Could not lock the preset library: {e}"))?;
    Ok(file)
}

fn check_library(path: &Path, expected: &[Preset]) -> Result<(), String> {
    let actual = match std::fs::metadata(path) {
        Ok(_) => presets::load_file(path)?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => vec![],
        Err(e) => return Err(e.to_string()),
    };
    if actual != expected {
        return Err("Photo presets changed on disk. Reload the library before saving.".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wait(session: &mut PhotoSession) {
        let start = std::time::Instant::now();
        while session.is_loading_presets() || session.is_saving_presets() {
            session.poll(&egui::Context::default());
            assert!(start.elapsed().as_secs() < 5);
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
    }

    #[test]
    fn preset_library_persists_exports_imports_and_preserves_external_changes() {
        let fixture = super::super::tests::Fixture::new();
        let path = fixture.0.join("library.omapreset");
        let mut session = PhotoSession::new();
        session.preset_state.path = path.clone();
        session.import_photo(super::super::tests::image());
        session.selected_mut().unwrap().develop.exposure = 1.25;
        session.ensure_presets_loaded();
        wait(&mut session);
        session.save_preset("Warm", Categories::default()).unwrap();
        assert!(session.is_saving());
        wait(&mut session);
        assert_eq!(session.presets.len(), 1);
        let protected = std::fs::read(&path).unwrap();
        session.export_preset(0, path.clone()).unwrap();
        wait(&mut session);
        assert!(session.preset_error().contains("separate"));
        assert_eq!(std::fs::read(&path).unwrap(), protected);
        let shared = fixture.0.join("shared.omapreset");
        session.export_preset(0, shared.clone()).unwrap();
        wait(&mut session);
        let mut reopened = PhotoSession::new();
        reopened.preset_state.path = path.clone();
        reopened.ensure_presets_loaded();
        wait(&mut reopened);
        assert_eq!(reopened.presets, session.presets);
        reopened.delete_preset(0).unwrap();
        wait(&mut reopened);
        reopened.import_presets(shared).unwrap();
        wait(&mut reopened);
        assert_eq!(reopened.presets, session.presets);
        let external = vec![Preset {
            name: "External".into(),
            ..session.presets[0].clone()
        }];
        presets::save_file(&path, &external).unwrap();
        let original_bytes = std::fs::read(&path).unwrap();
        session.delete_preset(0).unwrap();
        wait(&mut session);
        assert!(session.preset_error().contains("changed on disk"));
        assert_eq!(session.presets.len(), 1);
        assert_eq!(std::fs::read(&path).unwrap(), original_bytes);
        session.reload_presets();
        wait(&mut session);
        assert_eq!(session.presets, external);
        std::fs::write(&path, b"broken JSON").unwrap();
        session.delete_preset(0).unwrap();
        wait(&mut session);
        assert!(!session.preset_error().is_empty());
        assert_eq!(std::fs::read(path).unwrap(), b"broken JSON");
    }
}
