//! Palette drafts finish saving before the existing artwork quit flow begins.
use super::*;

const SAVE_ALL: &str = "library-save-on-quit";
struct SaveRequest {
    root: Option<PathBuf>,
    path: PathBuf,
    palettes: Vec<Palette>,
    stamp: Option<u64>,
    ready: bool,
    conflict: bool,
}
struct Saved {
    root: Option<PathBuf>,
    palettes: Vec<Palette>,
    result: Result<Option<u64>, String>,
}

fn dirty_collections(state: &Libraries) -> Vec<(Option<PathBuf>, &PaletteDraft)> {
    let mut drafts: Vec<_> = state
        .projects
        .iter()
        .filter(|(_, draft)| draft.dirty)
        .map(|(root, draft)| (Some(root.clone()), draft))
        .collect();
    drafts.sort_by(|a, b| a.0.cmp(&b.0));
    if state.personal.dirty {
        drafts.insert(0, (None, &state.personal));
    }
    drafts
}

fn save_requests(state: &Libraries) -> Vec<SaveRequest> {
    dirty_collections(state)
        .into_iter()
        .map(|(root, draft)| SaveRequest {
            path: root
                .as_ref()
                .map(|root| root.join(".omacolors"))
                .unwrap_or_else(crate::palette::path),
            root,
            palettes: draft.palettes.clone(),
            stamp: draft.stamp,
            ready: draft.loaded,
            conflict: draft.conflict,
        })
        .collect()
}

fn write_requests(requests: Vec<SaveRequest>) -> Vec<Saved> {
    let mut saved = Vec::with_capacity(requests.len());
    for request in requests {
        let result = (|| {
            if !request.ready {
                return Err(
                    "This collection has not finished loading. Try again when it is ready.".into(),
                );
            }
            if request.conflict || file_stamp(&request.path)? != request.stamp {
                return Err(format!(
                    "{} changed on disk. Cancel quitting, then reload it or export your colours as a copy.",
                    request.path.display(),
                ));
            }
            crate::palette::save_file(&request.path, &request.palettes)?;
            file_stamp(&request.path)
        })();
        saved.push(Saved {
            root: request.root,
            palettes: request.palettes,
            result,
        });
    }
    saved
}

fn finish_quit(ctx: &egui::Context, studio: &mut Studio) {
    studio.libraries.close_requested = false;
    studio.libraries.close_error.clear();
    if studio.has_unsaved_changes() {
        studio.pending_nav = Some(crate::app::PendingNav::Quit);
    } else {
        studio.allow_close = true;
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
    }
}

fn discard_drafts(state: &mut Libraries) {
    for draft in std::iter::once(&mut state.personal).chain(state.projects.values_mut()) {
        if draft.dirty {
            // If the following artwork dialog is cancelled, reload the saved
            // collection instead of exposing edits the user chose to discard.
            draft.dirty = false;
            draft.loaded = false;
            draft.conflict = false;
        }
    }
    state.next_sync = Instant::now();
}

pub(super) fn show(ctx: &egui::Context, studio: &mut Studio) {
    if let Some(result) = jobs::poll::<Vec<Saved>>(ctx, SAVE_ALL) {
        jobs::cancel::<Snapshot>(ctx, SYNC);
        studio.libraries.next_sync = Instant::now();
        match result {
            Ok(results) => {
                let mut errors = Vec::new();
                for saved in results {
                    let draft = draft_at(&mut studio.libraries, &saved.root);
                    match saved.result {
                        Ok(stamp) => {
                            draft.dirty = draft.palettes != saved.palettes;
                            draft.stamp = stamp;
                            draft.loaded = true;
                            draft.conflict = false;
                            draft.message = "Saved. Ready to share.".into();
                        }
                        Err(error) => {
                            draft.message = error.clone();
                            errors.push(error);
                        }
                    }
                }
                studio.libraries.close_error = errors.join("\n");
            }
            Err(error) => studio.libraries.close_error = error,
        }
    }
    if !studio.libraries.close_requested {
        return;
    }
    let busy = jobs::is_running::<Vec<Saved>>(ctx, SAVE_ALL)
        || super::typography::busy(ctx)
        || jobs::is_running::<PaletteResult>(ctx, PALETTE_ACTION)
        || jobs::is_running::<BrandResult>(ctx, BRAND_ACTION);
    let collections = dirty_collections(&studio.libraries);
    if collections.is_empty() && !busy && studio.libraries.close_error.is_empty() {
        finish_quit(ctx, studio);
        return;
    }
    let names: Vec<_> = collections
        .iter()
        .map(|(root, _)| {
            root.as_ref()
                .map(|root| root.display().to_string())
                .unwrap_or_else(|| "Personal palettes".into())
        })
        .collect();
    let can_save = !collections.is_empty();
    let mut cancel = false;
    let mut discard = false;
    let mut save = false;
    let dialog = egui::Modal::new(egui::Id::new("unsaved-palette-collections")).show(ctx, |ui| {
        ui.set_width((ctx.viewport_rect().width() - 64.0).clamp(240.0, 420.0));
        ui.heading("Save your palette collections?");
        ui.add_space(6.0);
        ui.label("Your artwork has its own save step after this.");
        egui::ScrollArea::vertical()
            .max_height(140.0)
            .show(ui, |ui| {
                for name in &names {
                    ui.label(name);
                }
                if !studio.libraries.close_error.is_empty() {
                    ui.label(&studio.libraries.close_error);
                }
            });
        if busy {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label("Finishing library changes…");
            });
        }
        ui.add_space(12.0);
        ui.horizontal(|ui| {
            cancel = ui.button("Cancel").clicked();
            discard = ui
                .add_enabled(!busy, egui::Button::new("Discard"))
                .clicked();
            save = ui
                .add_enabled(!busy && can_save, egui::Button::new("Save all"))
                .clicked();
        });
    });
    if cancel || dialog.should_close() {
        studio.libraries.close_requested = false;
        studio.libraries.close_error.clear();
    } else if discard {
        discard_drafts(&mut studio.libraries);
        finish_quit(ctx, studio);
    } else if save {
        let requests = save_requests(&studio.libraries);
        studio.libraries.close_error.clear();
        jobs::start(ctx, SAVE_ALL, move || Ok(write_requests(requests)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    struct Temp(PathBuf);
    impl Drop for Temp {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn quit_waits_for_all_palette_writes_preserves_conflicts_and_then_checks_artwork() {
        let temp = Temp(std::env::temp_dir().join(format!(
            "omadesign-palette-close-{}-{}",
            std::process::id(),
            crate::document::next_id()
        )));
        let a = temp.0.join("a");
        let b = temp.0.join("b");
        let original = vec![Palette::new("Original", vec![Rgba::BLACK])];
        let edited = vec![Palette::new("Edited", vec![Rgba::WHITE])];
        let external = vec![Palette::new("External", vec![Rgba::TRANSPARENT])];
        let ctx = egui::Context::default();
        let mut studio = Studio::new();
        studio.dirty = true;
        studio.libraries.close_requested = true;
        for root in [&a, &b] {
            let path = root.join(".omacolors");
            crate::palette::save_file(&path, &original).unwrap();
            studio.libraries.projects.insert(
                root.clone(),
                PaletteDraft {
                    palettes: edited.clone(),
                    loaded: true,
                    dirty: true,
                    stamp: file_stamp(&path).unwrap(),
                    ..Default::default()
                },
            );
        }
        let requests = save_requests(&studio.libraries);
        let (release, wait) = mpsc::channel();
        jobs::start(&ctx, SAVE_ALL, move || {
            wait.recv_timeout(Duration::from_secs(5))
                .map_err(|error| error.to_string())?;
            Ok(write_requests(requests))
        });
        let frame = |studio: &mut Studio| {
            let mut output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        vec2(960.0, 640.0),
                    )),
                    ..Default::default()
                },
                |ui| show(ui.ctx(), studio),
            );
            output.textures_delta.clear();
        };
        frame(&mut studio);
        assert!(
            !studio.allow_close && studio.pending_nav.is_none(),
            "quit must wait for its worker"
        );
        studio.libraries.root = Some(temp.0.join("another-project"));
        crate::palette::save_file(b.join(".omacolors"), &external).unwrap();
        release.send(()).unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        while jobs::is_running::<Vec<Saved>>(&ctx, SAVE_ALL) {
            assert!(Instant::now() < deadline, "palette worker did not finish");
            frame(&mut studio);
            std::thread::sleep(Duration::from_millis(1));
        }
        assert_eq!(
            crate::palette::load_file(a.join(".omacolors")).unwrap(),
            edited
        );
        assert_eq!(
            crate::palette::load_file(b.join(".omacolors")).unwrap(),
            external
        );
        assert!(!studio.libraries.projects[&a].dirty);
        assert!(studio.libraries.projects[&b].dirty);
        assert!(studio.libraries.close_error.contains("changed on disk"));
        assert!(
            !studio.allow_close && studio.pending_nav.is_none(),
            "failed saves cannot advance quitting"
        );
        assert!(studio.libraries.close_requested);

        studio.libraries.close_requested = false;
        frame(&mut studio);
        assert!(
            studio.pending_nav.is_none(),
            "cancelling quit keeps the document open"
        );

        // After the user explicitly discards the conflicted palette, the dirty
        // artwork still receives its normal save prompt instead of closing.
        discard_drafts(&mut studio.libraries);
        finish_quit(&ctx, &mut studio);
        assert!(!studio.allow_close);
        assert!(matches!(
            studio.pending_nav,
            Some(crate::app::PendingNav::Quit)
        ));
        assert!(!studio.libraries.projects[&b].loaded);

        // A later Save all uses the newly accepted disk revision and advances
        // to the artwork prompt only after the successful worker completes.
        studio.pending_nav = None;
        studio.libraries.close_requested = true;
        let draft = studio.libraries.projects.get_mut(&b).unwrap();
        draft.loaded = true;
        draft.dirty = true;
        draft.stamp = file_stamp(&b.join(".omacolors")).unwrap();
        let requests = save_requests(&studio.libraries);
        jobs::start(&ctx, SAVE_ALL, move || Ok(write_requests(requests)));
        let deadline = Instant::now() + Duration::from_secs(5);
        while jobs::is_running::<Vec<Saved>>(&ctx, SAVE_ALL) {
            assert!(Instant::now() < deadline, "retry did not finish");
            frame(&mut studio);
            std::thread::sleep(Duration::from_millis(1));
        }
        assert_eq!(
            crate::palette::load_file(b.join(".omacolors")).unwrap(),
            edited
        );
        assert!(matches!(
            studio.pending_nav,
            Some(crate::app::PendingNav::Quit)
        ));
        assert!(!studio.allow_close && !studio.libraries.projects[&b].dirty);
    }
}
