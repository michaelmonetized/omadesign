//! The Find Font dialog owns discovery, partial results and explicit file addition.
use super::find_font::control;
use crate::{
    app::Studio,
    text::font_recovery::{self as recovery, Found, Need, Search},
};
use eframe::egui::{self, Id, Ui};
use std::{
    collections::BTreeSet,
    path::PathBuf,
    sync::{Arc, Mutex},
};
const SCAN: &str = "type-find-font-search";
const ADD: &str = "type-find-font-add";
#[derive(Clone)]
pub(super) struct State {
    folder: String,
    seconds: u32,
    state: Arc<Mutex<Search>>,
    selected: BTreeSet<PathBuf>,
    seen: usize,
    scanning: bool,
    adding: bool,
    error: Option<String>,
}
impl Default for State {
    fn default() -> Self {
        Self {
            folder: String::new(),
            seconds: 60,
            state: Default::default(),
            selected: Default::default(),
            seen: 0,
            scanning: false,
            adding: false,
            error: None,
        }
    }
}
#[derive(Clone)]
struct Pick {
    owner: String,
    instance: Id,
    path: PathBuf,
}
/// Stop only this dialog's pending scan and installation.
/// Args: dialog context. Returns no value; completed explicit copies remain installed.
pub(super) fn cancel(ctx: &egui::Context) {
    super::jobs::cancel::<Search>(ctx, SCAN);
    super::jobs::cancel::<Vec<PathBuf>>(ctx, ADD);
}
/// Show the real folder path, partial search results, Stop and explicit Add actions.
/// Args: current owner, native dialog, missing needs and local state. Returns true after verified addition.
pub(super) fn show(
    ui: &mut Ui,
    s: &mut Studio,
    instance: Id,
    needs: Vec<Need>,
    state: &mut State,
) -> bool {
    let ctx = ui.ctx().clone();
    let owner = Id::new(("find-font-files", &s.swap_id, instance));
    let pick_key = owner.with("picked-folder");
    if let Some(pick) = ctx.data_mut(|data| {
        let pick = data.get_temp::<Pick>(pick_key);
        data.remove::<Pick>(pick_key);
        pick
    }) {
        if pick.owner == s.swap_id && pick.instance == instance {
            state.folder = pick.path.to_string_lossy().into_owned();
        }
    }
    if state.scanning
        && let Some(result) = super::jobs::poll::<Search>(&ctx, SCAN)
    {
        state.scanning = false;
        if let Err(error) = result {
            state.error = Some(error);
        }
    }
    let mut added = false;
    if state.adding
        && let Some(result) = super::jobs::poll::<Vec<PathBuf>>(&ctx, ADD)
    {
        state.adding = false;
        match result {
            Ok(_) => added = true,
            Err(error) => state.error = Some(error),
        }
    }
    let snapshot = state
        .state
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone();
    for found in snapshot.hits.iter().skip(state.seen) {
        state.selected.insert(found.path.clone());
    }
    state.seen = snapshot.hits.len();
    ui.separator();
    ui.label("Find missing fonts in a folder");
    ui.add_enabled_ui(!state.scanning && !state.adding, |ui| {
        let label = ui.label("Folder");
        let field = control(ui, owner.with("folder"), "Font search folder", |ui| {
            ui.add(egui::TextEdit::singleline(&mut state.folder).desired_width(f32::INFINITY))
                .labelled_by(label.id)
        });
        if let Some(error) = &state.error {
            super::welcome::control_error(ui, &[&field], error);
        }
        ui.horizontal_wrapped(|ui| {
            if control(ui, owner.with("browse"), "Browse for fonts…", |ui| {
                ui.button("Browse…")
            })
            .clicked()
            {
                let document = s.swap_id.clone();
                s.request_file_dialog(crate::project::dialog_folder, move |ctx, s, path| {
                    if s.swap_id == document && super::find_font::instance_current(ctx, instance) {
                        ctx.data_mut(|data| {
                            data.insert_temp(
                                pick_key,
                                Pick {
                                    owner: document.clone(),
                                    instance,
                                    path,
                                },
                            )
                        });
                    }
                });
            }
            let label = ui.label("Seconds");
            control(ui, owner.with("seconds"), "Font search time limit", |ui| {
                ui.add(
                    egui::DragValue::new(&mut state.seconds)
                        .range(1..=600)
                        .speed(1.),
                )
                .labelled_by(label.id)
            });
            if control(ui, owner.with("search"), "Search font files", |ui| {
                ui.add_enabled(
                    !needs.is_empty() && !state.folder.trim().is_empty(),
                    egui::Button::new("Search"),
                )
            })
            .clicked()
            {
                state.state = Default::default();
                state.seen = 0;
                state.selected.clear();
                state.error = None;
                state.scanning = true;
                let folder = PathBuf::from(&state.folder);
                let seconds = state.seconds;
                let shared = state.state.clone();
                super::jobs::start_with_progress(&ctx, SCAN, 10_000, move |progress| {
                    recovery::search(&folder, &needs, seconds, &shared, progress.as_ref())
                });
            }
        });
    });
    if state.scanning {
        ui.spinner();
        ui.label(format!(
            "{} folders, {} font files checked",
            snapshot.folders, snapshot.read
        ));
        if control(ui, owner.with("stop"), "Stop font search", |ui| {
            ui.button("Stop")
        })
        .clicked()
        {
            super::jobs::cancel::<Search>(&ctx, SCAN);
            state.scanning = false;
        }
        ctx.request_repaint_after(std::time::Duration::from_millis(100));
    }
    if let Some(end) = &snapshot.end {
        ui.label(format!(
            "{end}: {} files found; {} skipped, {} unreadable",
            snapshot.hits.len(),
            snapshot.skipped,
            snapshot.unreadable
        ));
    }
    egui::ScrollArea::vertical()
        .id_salt(owner.with("results"))
        .max_height(150.)
        .min_scrolled_height(0.)
        .show(ui, |ui| {
            ui.add_enabled_ui(!state.adding, |ui| {
                for found in &snapshot.hits {
                    let mut selected = state.selected.contains(&found.path);
                    let label = format!("{} — {}", found.names.join(", "), found.path.display());
                    if control(ui, owner.with(&found.path), &label, |ui| {
                        ui.checkbox(&mut selected, &label)
                    })
                    .changed()
                    {
                        if selected {
                            state.selected.insert(found.path.clone());
                        } else {
                            state.selected.remove(&found.path);
                        }
                    }
                }
            });
        });
    let files: Vec<Found> = snapshot
        .hits
        .into_iter()
        .filter(|f| state.selected.contains(&f.path))
        .collect();
    if control(ui, owner.with("add"), "Add selected font files", |ui| {
        ui.add_enabled(
            !state.scanning && !state.adding && !files.is_empty(),
            egui::Button::new("Add selected font files"),
        )
    })
    .clicked()
    {
        state.adding = true;
        state.error = None;
        let destination = crate::project::data_dir().join("fonts");
        super::jobs::start_with_progress(&ctx, ADD, files.len() as u32, move |progress| {
            recovery::install(&files, &destination, progress.as_ref())
        });
    }
    if state.adding {
        ui.spinner();
        ui.label("Adding font files…");
    }
    added
}
