use crate::{
    app::Studio,
    ui::{jobs, welcome::actions::template_root},
};
use eframe::egui::{self, Ui};
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};
const JOB: &str = "personal-template-list";
#[derive(Clone, Default)]
struct State {
    paths: Vec<PathBuf>,
    refreshed: Option<Instant>,
    error: String,
}
pub(super) fn show(ui: &mut Ui, studio: &mut Studio) {
    let id = egui::Id::new(JOB);
    let mut state = ui
        .ctx()
        .data(|d| d.get_temp::<State>(id))
        .unwrap_or_default();
    if let Some(result) = jobs::poll::<Vec<PathBuf>>(ui.ctx(), JOB) {
        state.refreshed = Some(Instant::now());
        match result {
            Ok(paths) => {
                state.paths = paths;
                state.error.clear();
            }
            Err(error) => state.error = error,
        }
    }
    if state
        .refreshed
        .is_none_or(|at| at.elapsed() > Duration::from_secs(3))
        && !jobs::is_running::<Vec<PathBuf>>(ui.ctx(), JOB)
    {
        jobs::start(ui.ctx(), JOB, || {
            let root = template_root();
            let entries = match std::fs::read_dir(root) {
                Ok(entries) => entries,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(vec![]),
                Err(e) => return Err(e.to_string()),
            };
            let mut paths: Vec<_> = entries
                .filter_map(Result::ok)
                .filter(|e| {
                    e.file_type().is_ok_and(|t| t.is_file())
                        && e.path().extension().is_some_and(|x| x == "oma")
                        && !e.file_name().to_string_lossy().starts_with('.')
                })
                .map(|e| e.path())
                .collect();
            paths.sort();
            Ok(paths)
        });
    }
    if !state.paths.is_empty() {
        egui::CollapsingHeader::new("Your templates")
            .default_open(true)
            .show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .id_salt("personal-templates")
                    .max_height(150.)
                    .show(ui, |ui| {
                        ui.horizontal_wrapped(|ui| {
                            for path in &state.paths {
                                if ui
                                    .button(path.file_stem().unwrap_or_default().to_string_lossy())
                                    .on_hover_text("Create an editable copy")
                                    .clicked()
                                {
                                    studio.open_personal_template(path.clone());
                                }
                            }
                        });
                    });
            });
        ui.separator();
    }
    if !state.error.is_empty() {
        ui.weak(&state.error);
    }
    ui.ctx().data_mut(|d| d.insert_temp(id, state));
}
