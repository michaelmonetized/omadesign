//! Background ownership for the optional durable photo catalog.
use super::*;
use photo::catalog::Catalog;

#[derive(Clone)]
pub struct Row {
    pub id: String,
    pub path: PathBuf,
    pub name: String,
    pub rating: u8,
    pub keywords: BTreeSet<String>,
    pub stack: Option<String>,
}

#[derive(Default)]
pub struct CatalogState {
    pub visible: bool,
    pub rows: Vec<Row>,
    pub selected: BTreeSet<String>,
    pub path: Option<PathBuf>,
    pub notes: Vec<String>,
    pub error: String,
    catalog: Option<Catalog>,
    job: Option<mpsc::Receiver<Outcome>>,
    sync_failed: bool,
    bound: BTreeSet<PathBuf>,
    synced_revision: Option<u64>,
}

struct Outcome {
    catalog: Option<Catalog>,
    photo: Option<PhotoImage>,
    message: String,
    error: String,
    sync: bool,
}

pub enum Command {
    Open(PathBuf),
    Import(PathBuf),
    Rating(Vec<String>, u8),
    Keywords(Vec<String>, Vec<String>),
    Stack(Vec<String>, Option<String>),
    Relink(String, PathBuf),
    Load(String),
    Sync(Vec<(String, DevelopParams)>),
    Checkpoint,
}

impl CatalogState {
    pub fn busy(&self) -> bool {
        self.job.is_some()
    }

    /// Transfer exclusive catalog ownership to one filesystem worker.
    /// Args: a catalog operation. Returns immediately; poll publishes only that worker's result.
    fn start(&mut self, command: Command) {
        if self.busy() {
            return;
        }
        if !matches!(command, Command::Open(_)) && self.catalog.is_none() {
            self.error = "Open or create a catalog first.".into();
            return;
        }
        let catalog = self.catalog.take();
        let (sender, receiver) = mpsc::channel();
        self.error.clear();
        self.job = Some(receiver);
        std::thread::spawn(move || {
            let mut outcome = Outcome {
                catalog,
                photo: None,
                message: String::new(),
                error: String::new(),
                sync: matches!(command, Command::Sync(_)),
            };
            let result = (|| -> Result<String, String> {
                if let Command::Open(path) = &command {
                    outcome.catalog = Some(Catalog::open(path)?);
                    return Ok("Photo catalog opened".into());
                }
                let catalog = outcome
                    .catalog
                    .as_mut()
                    .ok_or("No photo catalog is open.")?;
                match command {
                    Command::Open(_) => unreachable!(),
                    Command::Import(path) => {
                        let report = catalog.import_folder(path);
                        if report.error_count > 0 {
                            outcome.error = report.errors.join("\n");
                        }
                        Ok(format!(
                            "Imported {} · skipped {} · {} need attention{}",
                            report.imported,
                            report.skipped,
                            report.error_count,
                            if report.stopped {
                                " · import stopped"
                            } else {
                                ""
                            }
                        ))
                    }
                    Command::Rating(ids, rating) => {
                        for id in ids {
                            catalog.set_rating(&id, rating)?;
                        }
                        Ok("Catalog ratings saved".into())
                    }
                    Command::Keywords(ids, keywords) => {
                        for id in ids {
                            catalog.set_keywords(&id, &keywords)?;
                        }
                        Ok("Catalog keywords saved".into())
                    }
                    Command::Stack(ids, stack) => {
                        for id in ids {
                            catalog.set_stack(&id, stack.clone())?;
                        }
                        Ok("Catalog stack saved".into())
                    }
                    Command::Relink(id, path) => {
                        catalog.relink(&id, path)?;
                        Ok("Original relinked · saved edits retained".into())
                    }
                    Command::Load(id) => {
                        outcome.photo = Some(catalog.load_photo(&id)?);
                        Ok("Catalog photo and saved edits opened".into())
                    }
                    Command::Sync(edits) => {
                        for (id, params) in edits {
                            catalog.set_develop(&id, &params)?;
                        }
                        Ok("Catalog edits saved".into())
                    }
                    Command::Checkpoint => {
                        catalog.checkpoint()?;
                        Ok("Catalog compacted".into())
                    }
                }
            })();
            match result {
                Ok(message) => outcome.message = message,
                Err(error) => outcome.error = error,
            }
            let _ = sender.send(outcome);
        });
    }
}

impl PhotoSession {
    pub fn catalog_command(&mut self, command: Command) {
        if self.catalog.busy() {
            return;
        }
        if matches!(command, Command::Open(_)) && self.catalog.catalog.is_some() {
            self.catalog.error = "Close the current catalog before opening another.".into();
            return;
        }
        self.catalog.visible = true;
        self.catalog.start(command);
    }

    /// Close only after pending edits have reached the journal.
    /// Args: none. Returns without dropping an active worker or unsaved catalog adjustments.
    pub fn close_catalog(&mut self) {
        if self.catalog.busy() {
            return;
        }
        self.catalog.sync_failed = false;
        if self.sync_catalog_edits() {
            return;
        }
        self.catalog = CatalogState::default();
    }

    pub fn retry_catalog_save(&mut self) {
        self.catalog.sync_failed = false;
        self.catalog.synced_revision = None;
        self.sync_catalog_edits();
    }

    fn sync_catalog_edits(&mut self) -> bool {
        if self.catalog.busy() || self.catalog.sync_failed || self.coalescing.is_some() {
            return false;
        }
        if self.catalog.synced_revision == Some(self.edit_revision) {
            return false;
        }
        let Some(catalog) = &self.catalog.catalog else {
            return false;
        };
        let by_path = catalog
            .entries
            .values()
            .map(|entry| (&entry.path, entry))
            .collect::<BTreeMap<_, _>>();
        let edits = self
            .images
            .iter()
            .filter_map(|image| {
                if !self.catalog.bound.contains(image.source.as_ref()?) {
                    return None;
                }
                let entry = by_path.get(image.source.as_ref()?)?;
                (entry.develop != image.develop).then(|| (entry.id.clone(), image.develop.clone()))
            })
            .collect::<Vec<_>>();
        self.catalog.synced_revision = Some(self.edit_revision);
        if edits.is_empty() {
            return false;
        }
        self.catalog.start(Command::Sync(edits));
        true
    }

    pub(super) fn poll_catalog(&mut self, ctx: &egui::Context) {
        if let Some(receiver) = &self.catalog.job {
            match receiver.try_recv() {
                Ok(outcome) => {
                    self.catalog.job = None;
                    self.catalog.sync_failed = outcome.sync && !outcome.error.is_empty();
                    if !outcome.error.is_empty() {
                        self.status = outcome.error.clone();
                        self.catalog.error = outcome.error;
                    } else {
                        self.status = outcome.message;
                    }
                    if let Some(catalog) = outcome.catalog {
                        self.ensure_saved();
                        if outcome.sync {
                            for (index, image) in self.images.iter().enumerate() {
                                if let Some(entry) = catalog.entries.values().find(|entry| {
                                    image.source.as_ref() == Some(&entry.path)
                                        && self.catalog.bound.contains(&entry.path)
                                }) {
                                    self.saved[index] = entry.develop.clone();
                                }
                            }
                        }
                        let previous = self
                            .catalog
                            .rows
                            .iter()
                            .map(|row| (&row.id, &row.path))
                            .collect::<BTreeMap<_, _>>();
                        for entry in catalog.entries.values() {
                            if let Some(old) = previous.get(&entry.id)
                                && **old != entry.path
                            {
                                for image in &mut self.images {
                                    if image.source.as_ref() == Some(*old) {
                                        self.catalog.bound.remove(*old);
                                        self.catalog.bound.insert(entry.path.clone());
                                        image.source = Some(entry.path.clone());
                                        image.source_identity = Some(entry.source_identity.clone());
                                        image.settings_path = Some(entry.settings_path.clone());
                                    }
                                }
                            }
                        }
                        self.catalog.rows = catalog
                            .entries
                            .values()
                            .map(|entry| Row {
                                id: entry.id.clone(),
                                path: entry.path.clone(),
                                name: entry.name.clone(),
                                rating: entry.rating,
                                keywords: entry.keywords.clone(),
                                stack: entry.stack.clone(),
                            })
                            .collect();
                        self.catalog.path = Some(catalog.path().to_owned());
                        self.catalog.notes = catalog.recovery_notes.clone();
                        self.catalog.catalog = Some(catalog);
                    }
                    if let Some(photo) = outcome.photo {
                        self.catalog.synced_revision = None;
                        if let Some(source) = &photo.source {
                            self.catalog.bound.insert(source.clone());
                        }
                        if let Some(index) = self
                            .images
                            .iter()
                            .position(|image| image.source == photo.source)
                        {
                            if self
                                .saved
                                .get(index)
                                .is_some_and(|saved| saved == &self.images[index].develop)
                            {
                                self.images[index] = photo;
                                self.saved[index] = self.images[index].develop.clone();
                                self.dirty = true;
                                self.sel_version += 1;
                            }
                            self.select_image(index);
                        } else {
                            self.import_photo(photo);
                        }
                    }
                }
                Err(mpsc::TryRecvError::Empty) => {}
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.catalog.job = None;
                    self.catalog.error="The catalog worker stopped. Reopen the catalog to recover its durable journal.".into();
                    self.catalog.sync_failed = true;
                }
            }
        }
        self.sync_catalog_edits();
        if self.catalog.busy() {
            ctx.request_repaint_after(std::time::Duration::from_millis(40));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use photo::local::LocalAdjustment;
    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            let path = Path::new(env!("CARGO_MANIFEST_DIR"))
                .join(".artifacts")
                .join(format!(
                    "photo-catalog-session-{}-{}",
                    std::process::id(),
                    crate::document::next_id()
                ));
            std::fs::create_dir_all(path.join("originals")).unwrap();
            Self(path)
        }
        fn source(&self) -> PathBuf {
            let path = self.0.join("originals/photo.png");
            std::fs::write(
                &path,
                RgbaImage::new(8, 6, [60, 90, 120, 255].repeat(48))
                    .unwrap()
                    .encode_png()
                    .unwrap(),
            )
            .unwrap();
            path.canonicalize().unwrap()
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
        while session.catalog.busy() {
            session.poll_catalog(&ctx);
            assert!(start.elapsed().as_secs() < 30, "catalog worker timeout");
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert!(
            session.catalog.error.is_empty(),
            "{}",
            session.catalog.error
        );
    }
    #[test]
    fn catalog_session_saves_local_edits_restarts_and_relinks_without_sidecar() {
        let fixture = Fixture::new();
        let source = fixture.source();
        let directory = fixture.0.join("catalog");
        let mut session = PhotoSession::new();
        session.catalog_command(Command::Open(directory.clone()));
        wait(&mut session);
        session.catalog_command(Command::Import(fixture.0.join("originals")));
        wait(&mut session);
        let id = session.catalog.rows[0].id.clone();
        session.catalog_command(Command::Load(id.clone()));
        wait(&mut session);
        let before = session.images[0].develop.clone();
        session.images[0].develop.optics.profile.distortion[0] = 0.08;
        session.images[0]
            .develop
            .local_adjustments
            .push(LocalAdjustment {
                exposure: 0.75,
                ..Default::default()
            });
        session.record_edit(before, false);
        session.finish_edit();
        session.poll_catalog(&egui::Context::default());
        wait(&mut session);
        let expected = session.images[0].develop.clone();
        assert!(
            !session.has_unsaved_settings(),
            "journaled edits must be treated as saved at quit"
        );
        session.catalog_command(Command::Rating(vec![id.clone()], 5));
        wait(&mut session);
        session.catalog_command(Command::Keywords(vec![id.clone()], vec!["Client".into()]));
        wait(&mut session);
        session.catalog_command(Command::Stack(vec![id.clone()], Some("Selects".into())));
        wait(&mut session);
        session.close_catalog();
        assert!(session.catalog.path.is_none());
        drop(session);
        let moved = fixture.0.join("moved.png");
        std::fs::copy(&source, &moved).unwrap();
        std::fs::remove_file(&source).unwrap();
        let mut reopened = PhotoSession::new();
        reopened.catalog_command(Command::Open(directory));
        wait(&mut reopened);
        assert_eq!(reopened.catalog.rows[0].rating, 5);
        assert!(reopened.catalog.rows[0].keywords.contains("Client"));
        assert_eq!(reopened.catalog.rows[0].stack.as_deref(), Some("Selects"));
        reopened.catalog_command(Command::Relink(id.clone(), moved.clone()));
        wait(&mut reopened);
        reopened.catalog_command(Command::Load(id));
        wait(&mut reopened);
        assert_eq!(reopened.images[0].develop, expected);
        assert_eq!(reopened.images[0].source.as_ref(), Some(&moved));
        assert!(!photo::edits::sidecar_path(&moved).exists());
        reopened.save_settings();
        let start = std::time::Instant::now();
        let ctx = egui::Context::default();
        while reopened.is_saving() {
            reopened.poll(&ctx);
            assert!(start.elapsed().as_secs() < 30);
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert_eq!(PhotoImage::load(&moved).unwrap().develop, expected);
    }

    #[test]
    fn opening_catalog_does_not_overwrite_stored_edits_with_unbound_folder_photo() {
        let fixture = Fixture::new();
        let source = fixture.source();
        let directory = fixture.0.join("catalog");
        let mut catalog = Catalog::open(&directory).unwrap();
        catalog.import_folder(fixture.0.join("originals"));
        let id = catalog.entries.keys().next().unwrap().clone();
        let expected = DevelopParams {
            exposure: 1.25,
            ..Default::default()
        };
        catalog.set_develop(&id, &expected).unwrap();
        drop(catalog);
        let mut session = PhotoSession::new();
        session.import_photo(PhotoImage::load(&source).unwrap());
        session.catalog_command(Command::Open(directory));
        wait(&mut session);
        assert_eq!(
            session.catalog.catalog.as_ref().unwrap().entries[&id].develop,
            expected
        );
        session.catalog_command(Command::Load(id));
        wait(&mut session);
        assert_eq!(session.images.len(), 1);
        assert_eq!(session.images[0].develop, expected);
    }
}
