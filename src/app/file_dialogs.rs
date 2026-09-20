//! Keep native pickers off the event loop so Wayland/X11 can answer window pings.
use super::*;
use std::sync::mpsc::{self, TryRecvError};

type DialogPoll = Box<dyn FnMut(&egui::Context, &mut Studio) -> bool>;
pub(super) struct FileDialogJob {
    poll: DialogPoll,
}

impl Studio {
    pub fn file_dialog_pending(&self) -> bool {
        self.file_dialog.is_some()
    }

    /// A chooser owns the document that opened it. Cancellation never invokes
    /// the completion; a late result cannot mutate another document.
    pub fn request_file_dialog<T: Send + 'static>(
        &mut self,
        work: impl FnOnce() -> Option<T> + Send + 'static,
        complete: impl FnOnce(&egui::Context, &mut Studio, T) + 'static,
    ) -> bool {
        if self.file_dialog.is_some() {
            return false;
        }
        let (tx, receiver) = mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(work());
        });
        let owner = self.swap_id.clone();
        let mut complete = Some(complete);
        self.file_dialog = Some(FileDialogJob {
            poll: Box::new(move |ctx, studio| {
                match receiver.try_recv() {
                    Err(TryRecvError::Empty) => return false,
                    Err(TryRecvError::Disconnected) => {
                        studio.status =
                            "The file chooser stopped unexpectedly. Please try again.".into();
                    }
                    Ok(None) => {}
                    Ok(Some(value)) => {
                        if studio.swap_id == owner {
                            if let Some(complete) = complete.take() {
                                complete(ctx, studio, value);
                            }
                        } else {
                            studio.status =
                                "The destination document changed. Choose the file again.".into();
                        }
                    }
                }
                true
            }),
        });
        true
    }

    pub(super) fn poll_file_dialog(&mut self, ctx: &egui::Context) {
        if let Some(mut job) = self.file_dialog.take()
            && !(job.poll)(ctx, self)
        {
            self.file_dialog = Some(job);
        }
        if self.file_dialog.is_some() {
            // Some desktop portals do not deliver application input while their
            // window has focus. Continue pumping frames until the chooser ends.
            ctx.request_repaint_after(Duration::from_millis(40));
        }
    }

    fn finish_artwork_save(&mut self, ctx: &egui::Context, path: PathBuf) {
        match self.save_document(&path) {
            Ok(()) => {
                self.remember_path(&path);
                self.status = format!("saved {}", path.display());
                if self.pending_nav.is_some() {
                    self.execute_nav(ctx, true);
                }
            }
            Err(error) => self.status = format!("save failed: {error}"),
        }
    }

    pub(super) fn choose_artwork_save_path(&mut self) {
        let name = self.doc.name.clone();
        self.request_file_dialog(
            move || crate::project::dialog_save(&name),
            |ctx, studio, path| studio.finish_artwork_save(ctx, path),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };

    fn finish(studio: &mut Studio) {
        let ctx = egui::Context::default();
        let deadline = Instant::now() + Duration::from_secs(2);
        while studio.file_dialog_pending() {
            studio.poll_file_dialog(&ctx);
            assert!(Instant::now() < deadline, "chooser worker did not complete");
            std::thread::yield_now();
        }
    }

    #[test]
    fn native_chooser_returns_control_while_waiting_and_accepts_only_one_request() {
        let mut studio = Studio::default();
        let (tx, rx) = mpsc::channel();
        assert!(studio.request_file_dialog(
            move || rx.recv().ok(),
            |_, studio, text| studio.status = text
        ));
        assert!(!studio.request_file_dialog(|| Some(()), |_, _, _| panic!("duplicate chooser")));
        studio.poll_file_dialog(&egui::Context::default());
        assert!(studio.file_dialog_pending());
        tx.send("chosen".to_owned()).unwrap();
        finish(&mut studio);
        assert_eq!(studio.status, "chosen");
    }

    #[test]
    fn cancellation_and_changed_document_never_apply_a_result() {
        let applied = Arc::new(AtomicBool::new(false));
        let mut studio = Studio::default();
        let flag = applied.clone();
        studio.request_file_dialog(
            || None::<()>,
            move |_, _, _| flag.store(true, Ordering::SeqCst),
        );
        finish(&mut studio);
        assert!(!applied.load(Ordering::SeqCst));
        let flag = applied.clone();
        studio.request_file_dialog(
            || Some(()),
            move |_, _, _| flag.store(true, Ordering::SeqCst),
        );
        studio.swap_id = "another-document".into();
        finish(&mut studio);
        assert!(!applied.load(Ordering::SeqCst));
    }

    #[test]
    fn saving_an_untitled_tab_resumes_close_only_after_success() {
        let root = if let Some(root) = std::env::var_os("OMADESIGN_DIALOG_SAVE_TEST_DIR") {
            PathBuf::from(root)
        } else {
            let root = std::env::temp_dir().join(format!(
                "omadesign-dialog-save-{}-{}",
                std::process::id(),
                crate::document::next_id()
            ));
            std::fs::create_dir(&root).unwrap();
            let output = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "app::file_dialogs::tests::saving_an_untitled_tab_resumes_close_only_after_success", "--nocapture"])
                .env("OMADESIGN_DIALOG_SAVE_TEST_DIR", &root)
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
        let mut studio = Studio::default();
        studio.ensure_tabs();
        studio.dirty = true;
        studio.pending_nav = Some(PendingNav::CloseTab(0));
        studio.request_file_dialog(
            || None::<PathBuf>,
            |ctx, studio, path| {
                studio.finish_artwork_save(ctx, path);
            },
        );
        finish(&mut studio);
        assert!(studio.dirty && studio.pending_nav.is_some());
        assert!(!studio.allow_close);

        let invalid = root.join("missing-directory/failed.oma");
        studio.request_file_dialog(
            move || Some(invalid),
            |ctx, studio, path| {
                studio.finish_artwork_save(ctx, path);
            },
        );
        finish(&mut studio);
        assert!(studio.dirty && studio.pending_nav.is_some());
        assert!(studio.path.is_none());
        assert!(studio.status.starts_with("save failed:"));
        assert!(!studio.allow_close);

        let path = root.join("saved.oma");
        let chosen = path.clone();
        studio.request_file_dialog(
            move || Some(chosen),
            |ctx, studio, path| studio.finish_artwork_save(ctx, path),
        );
        assert!(studio.dirty);
        assert!(studio.pending_nav.is_some());
        finish(&mut studio);
        assert!(path.exists());
        assert!(studio.pending_nav.is_none());
        assert!(!studio.dirty);
        let _ = std::fs::remove_file(path);
    }
}
