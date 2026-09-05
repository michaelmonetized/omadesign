use super::*;
use std::sync::mpsc::{self, Receiver, TryRecvError};

const IDLE_DELAY: Duration = Duration::from_secs(1);
const RETRY_DELAY: Duration = Duration::from_secs(10);

pub(super) struct RecoveryJob {
    id: String,
    revision: Instant,
    result: Receiver<Result<crate::project::PreparedSwap, String>>,
}

pub(super) struct RecoverySource<'a> {
    pub doc: &'a Document,
    pub path: &'a Option<PathBuf>,
    pub id: &'a str,
    pub revision: Instant,
}

impl Studio {
    pub fn tick_swap(&mut self, ctx: &egui::Context) {
        if let Some(job) = &self.recovery_job {
            let result = match job.result.try_recv() {
                Ok(result) => result,
                Err(TryRecvError::Empty) => return,
                Err(TryRecvError::Disconnected) => Err("recovery worker stopped".into()),
            };
            let job = self.recovery_job.take().unwrap();
            let result =
                result.and_then(|saved| self.finish_recovery(&job.id, job.revision, saved));
            if let Err(e) = result {
                self.status = format!("Recovery could not be saved: {e}");
                self.recovery_retry = Some(Instant::now() + RETRY_DELAY);
            }
        }

        // A successful snapshot covers exactly this revision. Pointer motion,
        // repainting and tab switching must not repeatedly compress the file.
        let Some(source) = self.pending_recovery() else {
            return;
        };
        let now = Instant::now();
        let deadline = (source.revision + IDLE_DELAY).max(self.recovery_retry.unwrap_or(now));
        if deadline > now {
            ctx.request_repaint_after(deadline - now);
            return;
        }

        self.start_recovery(ctx, crate::project::swap_path(source.id));
    }

    fn start_recovery(&mut self, ctx: &egui::Context, destination: PathBuf) {
        let Some(source) = self.pending_recovery() else {
            return;
        };
        let revision = source.revision;
        let meta = crate::project::SwapMeta {
            id: source.id.to_owned(),
            original: source.path.clone(),
            name: source.doc.name.clone(),
            saved_at: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0),
            doc: source.doc.clone(),
        };
        let (sender, result) = mpsc::channel();
        let ctx = ctx.clone();
        self.recovery_job = Some(RecoveryJob {
            id: meta.id.clone(),
            revision,
            result,
        });
        self.recovery_retry = None;
        std::thread::spawn(move || {
            let prepared = crate::project::prepare_swap(meta, destination);
            let _ = sender.send(prepared);
            ctx.request_repaint();
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manual_save_rejects_older_recovery_after_another_edit() {
        let ctx = egui::Context::default();
        let mut studio = Studio::new();
        studio.doc = Document::new("Before saving", 8.0, 8.0, 72.0);
        studio.dirty = true;
        let id = studio.swap_id.clone();
        let directory = std::env::temp_dir().join(crate::project::new_swap_id());
        std::fs::create_dir(&directory).unwrap();
        let destination = directory.join("recovery.oma.swp");
        let project = directory.join("document.oma");
        studio.start_recovery(&ctx, destination.clone());

        studio.doc.name = "Saved version".into();
        studio.mark();
        studio.save_document(&project).unwrap();
        assert_eq!(studio.swap_id, id, "saving must preserve document identity");
        studio.doc.name = "Edited after saving".into();
        studio.dirty = true;
        studio.mark();

        let job = studio.recovery_job.take().unwrap();
        let prepared = job
            .result
            .recv_timeout(Duration::from_secs(10))
            .unwrap()
            .unwrap();
        studio
            .finish_recovery(&job.id, job.revision, prepared)
            .unwrap();
        assert!(
            !destination.exists(),
            "a pre-save snapshot must not be published after saving"
        );
        assert!(
            studio.pending_recovery().is_some(),
            "the subsequent edit still needs recovery"
        );

        studio.start_recovery(&ctx, destination.clone());
        let job = studio.recovery_job.take().unwrap();
        let prepared = job
            .result
            .recv_timeout(Duration::from_secs(10))
            .unwrap()
            .unwrap();
        studio
            .finish_recovery(&job.id, job.revision, prepared)
            .unwrap();
        assert_eq!(
            crate::project::load_swap(&destination).unwrap().name,
            "Edited after saving"
        );
        assert_eq!(
            crate::project::load_from(&project).unwrap().name,
            "Saved version"
        );
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn recovery_saves_once_per_revision_and_discards_closed_document_results() {
        let ctx = egui::Context::default();
        let mut studio = Studio::new();
        studio.doc = Document::new("Recover me", 8.0, 8.0, 72.0);
        studio.dirty = true;
        let directory = std::env::temp_dir().join(crate::project::new_swap_id());
        let destination = directory.join("recovery.oma.swp");

        studio.show_welcome = false;
        studio.new_tab();
        assert!(!studio.dirty);
        studio.start_recovery(&ctx, destination.clone());
        let job = studio.recovery_job.take().unwrap();
        let prepared = job
            .result
            .recv_timeout(Duration::from_secs(10))
            .unwrap()
            .unwrap();
        assert!(!destination.exists(), "workers must not publish snapshots");
        studio
            .finish_recovery(&job.id, job.revision, prepared)
            .unwrap();
        assert_eq!(
            crate::project::load_swap(&destination).unwrap().name,
            "Recover me"
        );
        studio.tick_swap(&ctx);
        assert!(
            studio.recovery_job.is_none(),
            "unchanged documents need no more work"
        );

        studio.switch_tab(0);
        studio.mark();
        studio.start_recovery(&ctx, destination.clone());
        studio.close_tab(studio.active_tab);
        std::fs::remove_file(&destination).unwrap();
        let job = studio.recovery_job.take().unwrap();
        let prepared = job
            .result
            .recv_timeout(Duration::from_secs(10))
            .unwrap()
            .unwrap();
        studio
            .finish_recovery(&job.id, job.revision, prepared)
            .unwrap();
        assert!(
            !destination.exists(),
            "discard must not resurrect recovery files"
        );
        assert_eq!(std::fs::read_dir(&directory).unwrap().count(), 0);
        std::fs::remove_dir(directory).unwrap();
    }
}
