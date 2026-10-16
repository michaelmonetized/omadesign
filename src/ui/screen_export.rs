//! Asset thumbnails, document export rows and source-bound asynchronous publication.
use super::{
    fields,
    icons::{self, ph},
    jobs, welcome,
};
use crate::{
    app::Studio,
    screen_export::{Catalog, Plan, ResultFile, Row, Scope},
};
use eframe::egui::{self, Id, Ui};
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};
const JOB: &str = "screen-export-batch";
const THUMB_JOB: &str = "screen-export-thumbnail";
fn run_id() -> Id {
    Id::new("screen-export-run")
}
fn state_id(studio: &Studio) -> Id {
    Id::new(("screen-export", &studio.swap_id))
}
#[derive(Clone)]
struct Run {
    document: String,
    revision: u64,
    valid: Arc<AtomicBool>,
}
#[derive(Clone, Default)]
struct State {
    base: Catalog,
    draft: Catalog,
    selected: Vec<u64>,
    boards: Vec<u64>,
    mode: u8,
    folder: String,
    error: String,
    initialized: bool,
    images: HashMap<u64, (u64, egui::TextureHandle)>,
    failed: HashMap<u64, u64>,
    sizes: HashMap<u64, String>,
}
struct WorkProgress {
    inner: Arc<jobs::Progress>,
    valid: Arc<AtomicBool>,
}
impl crate::ml::Progress for WorkProgress {
    fn report(&self, stage: &str, done: usize, total: usize) {
        crate::ml::Progress::report(self.inner.as_ref(), stage, done, total);
    }
    fn cancelled(&self) -> bool {
        !self.valid.load(Ordering::Relaxed) || self.inner.cancelled()
    }
}
type Thumb = (String, u64, u64, egui::ColorImage);

fn icon(ui: &mut Ui, id: Id, glyph: &str, name: &str, enabled: bool) -> bool {
    let response = fields::native_scope(ui, id, |ui| {
        ui.add_enabled(
            enabled,
            egui::Button::new(egui::RichText::new(glyph).font(icons::font(16.))).frame(false),
        )
    })
    .inner;
    welcome::name_control(ui, &response, id.with("label"), name);
    response.on_hover_text(name).clicked()
}
fn text(ui: &mut Ui, id: Id, value: &mut String, name: &str) -> egui::Response {
    let hint = match name {
        "Export filename prefix" => "Prefix",
        "Export row filename suffix" => "Suffix",
        "Export destination folder" => "Destination folder",
        _ => "Asset name",
    };
    let response = ui.add(
        egui::TextEdit::singleline(value)
            .id(id)
            .hint_text(hint)
            .desired_width(ui.available_width()),
    );
    welcome::name_control(ui, &response, id.with("label"), name);
    response
}

/// Open the actual Asset Export pane on its artboard export route.
/// Args: UI context and editor. Returns nothing; workspace and draft selection remain document-owned.
pub(super) fn open(ctx: &egui::Context, studio: &mut Studio) {
    let id = state_id(studio);
    let mut state = ctx.data(|d| d.get_temp::<State>(id)).unwrap_or_default();
    state.mode = 1;
    ctx.data_mut(|d| d.insert_temp(id, state));
    super::panel_workspace::open(ctx, studio, "asset-export");
}

/// Retire stale sources and drain the one existing export worker even when the pane is hidden.
/// Args: context and current editor. Returns nothing; no late result edits a document.
pub(super) fn poll(ctx: &egui::Context, studio: &mut Studio) {
    if let Some(run) = ctx.data(|d| d.get_temp::<Run>(run_id())) {
        if run.document != studio.swap_id || run.revision != studio.canvas_gen {
            run.valid.store(false, Ordering::Relaxed);
        }
        if let Some(result) = jobs::poll::<Vec<ResultFile>>(ctx, JOB) {
            ctx.data_mut(|d| d.remove::<Run>(run_id()));
            if run.document == studio.swap_id {
                studio.status = match result {
                    Ok(files) => format!(
                        "Exported {} files · {}",
                        files.len(),
                        files
                            .iter()
                            .flat_map(|f| f.notes.iter())
                            .cloned()
                            .collect::<Vec<_>>()
                            .join(" · ")
                    ),
                    Err(error) => format!("Asset export: {error}"),
                };
            }
        } else {
            ctx.request_repaint_after(std::time::Duration::from_millis(40));
        }
    }
}
fn start(ctx: &egui::Context, studio: &Studio, state: &State) -> Result<(), String> {
    if ctx.data(|d| d.get_temp::<Run>(run_id())).is_some() {
        return Err("Wait for the current asset export to finish".into());
    }
    if studio.file_dialog_pending()
        || studio.updates.freezing
        || studio.op.is_some()
        || studio.type_edit.is_some()
    {
        return Err("Finish the current edit before exporting".into());
    }
    let scopes: Vec<Scope> = match state.mode {
        0 => studio
            .doc
            .screen_export
            .assets
            .iter()
            .filter(|a| state.selected.is_empty() || state.selected.contains(&a.id))
            .map(|a| Scope::Asset(a.id))
            .collect(),
        1 => state.boards.iter().map(|id| Scope::Artboard(*id)).collect(),
        _ => vec![Scope::Document],
    };
    let folder = PathBuf::from(&state.folder);
    if !folder.is_dir() {
        return Err("Choose an existing export folder".into());
    }
    let plan = Plan::new(&studio.doc, &scopes)?;
    let valid = Arc::new(AtomicBool::new(true));
    ctx.data_mut(|d| {
        d.insert_temp(
            run_id(),
            Run {
                document: studio.swap_id.clone(),
                revision: studio.canvas_gen,
                valid: valid.clone(),
            },
        )
    });
    jobs::start_with_progress(ctx, JOB, 1, move |inner| {
        plan.save(&folder, &WorkProgress { inner, valid })
    });
    Ok(())
}
fn thumbnail(ui: &Ui, studio: &Studio, state: &mut State, id: u64) -> Option<egui::TextureHandle> {
    if let Some(result) = jobs::poll::<Thumb>(ui.ctx(), THUMB_JOB)
        && let Ok((document, revision, asset, pixels)) = result
        && document == studio.swap_id
        && revision == studio.canvas_gen
    {
        state.images.insert(
            asset,
            (
                revision,
                ui.ctx().load_texture(
                    format!("asset-{document}-{asset}"),
                    pixels,
                    Default::default(),
                ),
            ),
        );
    }
    if state
        .images
        .get(&id)
        .is_none_or(|(r, _)| *r != studio.canvas_gen)
        && state.failed.get(&id) != Some(&studio.canvas_gen)
        && !jobs::is_running::<Thumb>(ui.ctx(), THUMB_JOB)
        && !ui.input(|i| i.pointer.any_down())
    {
        state.failed.insert(id, studio.canvas_gen);
        if let Some(asset) = studio.doc.screen_export.assets.iter().find(|a| a.id == id)
            && let Ok(document) = asset.document(&studio.doc)
        {
            let owner = studio.swap_id.clone();
            let revision = studio.canvas_gen;
            jobs::start(ui.ctx(), THUMB_JOB, move || {
                let proxy = super::chrome::document_previews::snapshot(&document)?;
                Ok((
                    owner,
                    revision,
                    id,
                    super::chrome::document_previews::render(&proxy, None)?,
                ))
            });
        }
    }
    state
        .images
        .get(&id)
        .filter(|(r, _)| *r == studio.canvas_gen)
        .map(|(_, texture)| texture.clone())
}

/// Draw live assets and shared export rows using native fields and ordinary typed commands.
/// Args: pane UI and editor. Returns nothing; artwork is never changed by collection or export.
pub(super) fn show(ui: &mut Ui, studio: &mut Studio) {
    let owner = state_id(studio);
    let mut state = ui
        .data_mut(|d| d.remove_temp::<State>(owner))
        .unwrap_or_default();
    if !state.initialized || state.base != studio.doc.screen_export {
        state.base = studio.doc.screen_export.clone();
        state.draft = state.base.clone();
        state.sizes.clear();
        state
            .selected
            .retain(|id| state.base.assets.iter().any(|a| a.id == *id));
        if !state.initialized {
            state.boards = studio.doc.artboards.iter().map(|b| b.id).collect();
        }
        state.initialized = true;
    }
    state
        .boards
        .retain(|id| studio.doc.artboards.iter().any(|b| b.id == *id));
    state
        .images
        .retain(|id, _| state.base.assets.iter().any(|a| a.id == *id));
    let running = ui.data(|d| d.get_temp::<Run>(run_id()));
    let pending = state.draft != state.base;
    let editing = running.is_none() && studio.doc.collaboration.is_none();
    let scope = fields::native_scope(ui, owner.with("mode"), |ui| {
        egui::ComboBox::from_id_salt("scope")
            .selected_text(["Assets", "Artboards", "Full document"][state.mode.min(2) as usize])
            .show_ui(ui, |ui| {
                for (mode, name) in [(0, "Assets"), (1, "Artboards"), (2, "Full document")] {
                    ui.selectable_value(&mut state.mode, mode, name);
                }
            })
    })
    .inner
    .response;
    welcome::name_control(
        ui,
        &scope,
        owner.with("scope-label"),
        "Export for Screens scope",
    );
    if state.mode == 0 {
        for asset in state.base.assets.clone() {
            let preview = thumbnail(ui, studio, &mut state, asset.id);
            ui.horizontal(|ui| {
                if let Some(texture) = preview {
                    ui.add(egui::Image::new(&texture).fit_to_exact_size(egui::vec2(38., 38.)));
                }
                let response = fields::native_scope(ui, owner.with((asset.id, "select")), |ui| {
                    ui.add(
                        egui::Button::new(&asset.name)
                            .selected(state.selected.contains(&asset.id))
                            .min_size(egui::vec2(ui.available_width(), 38.)),
                    )
                })
                .inner;
                welcome::name_control(
                    ui,
                    &response,
                    owner.with((asset.id, "select-label")),
                    &format!("Export asset {}", asset.name),
                );
                if response.clicked() {
                    if ui.input(|i| i.modifiers.shift || i.modifiers.command) {
                        if state.selected.contains(&asset.id) {
                            state.selected.retain(|id| *id != asset.id);
                        } else {
                            state.selected.push(asset.id);
                        }
                    } else {
                        state.selected = vec![asset.id];
                    }
                }
                if let Err(error) = asset.selection(&studio.doc) {
                    response.on_hover_text(error);
                }
            });
        }
        ui.horizontal_wrapped(|ui| {
            for (single, glyph, label) in [
                (false, ph::PLUS, "Collect selection as separate assets"),
                (true, ph::STACK, "Collect selection as one asset"),
            ] {
                if icon(
                    ui,
                    owner.with(("collect", single)),
                    glyph,
                    label,
                    editing && !pending,
                ) {
                    let result = studio
                        .capture_export_assets()
                        .and_then(|target| studio.collect_export_assets(&target, single));
                    match result {
                        Ok(ids) => {
                            state.selected = ids;
                            state.base = studio.doc.screen_export.clone();
                            state.draft = state.base.clone();
                            state.error.clear();
                        }
                        Err(error) => state.error = error,
                    }
                }
            }
            if icon(
                ui,
                owner.with("remove"),
                ph::TRASH,
                "Remove selected export assets",
                editing && !pending && !state.selected.is_empty(),
            ) {
                let mut after = state.base.clone();
                after.assets.retain(|a| !state.selected.contains(&a.id));
                match studio.set_screen_export(&state.base, after) {
                    Ok(_) => state.selected.clear(),
                    Err(error) => state.error = error,
                }
            }
        });
        if state.selected.len() == 1
            && let Some(asset) = state
                .draft
                .assets
                .iter_mut()
                .find(|a| a.id == state.selected[0])
        {
            let response = ui
                .add_enabled_ui(editing, |ui| {
                    text(
                        ui,
                        owner.with((asset.id, "name")),
                        &mut asset.name,
                        "Export asset name",
                    )
                })
                .inner;
            if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                match studio.set_screen_export(&state.base, state.draft.clone()) {
                    Ok(_) => state.error.clear(),
                    Err(error) => state.error = error,
                }
            }
        }
    } else if state.mode == 1 {
        for board in &studio.doc.artboards {
            let mut checked = state.boards.contains(&board.id);
            let response = ui.checkbox(&mut checked, &board.name);
            welcome::name_control(
                ui,
                &response,
                owner.with((board.id, "board-label")),
                &format!("Export artboard {}", board.name),
            );
            if response.changed() {
                state.boards.retain(|id| *id != board.id);
                if checked {
                    state.boards.push(board.id);
                }
            }
        }
    }
    ui.separator();
    ui.add_enabled_ui(editing, |ui| {
        text(
            ui,
            owner.with("prefix"),
            &mut state.draft.prefix,
            "Export filename prefix",
        )
        .on_hover_text("Filename prefix");
        let mut remove = None;
        for row in &mut state.draft.rows {
            let size = state
                .sizes
                .entry(row.id)
                .or_insert_with(|| row.resolution().to_string());
            let mut size_error = None;
            let mut size_response = None;
            ui.horizontal_wrapped(|ui| {
                let response = fields::native_scope(ui, owner.with((row.id, "size")), |ui| {
                    ui.add_enabled(
                        row.format.raster(),
                        egui::TextEdit::singleline(size)
                            .id(owner.with((row.id, "size-input")))
                            .desired_width(76.),
                    )
                })
                .inner;
                welcome::name_control(
                    ui,
                    &response,
                    owner.with((row.id, "scale-label")),
                    "Export row size",
                );
                if row.format.raster() {
                    match crate::screen_export::Resolution::parse(size) {
                        Ok(value) => {
                            if response.changed() {
                                row.set_resolution(value);
                            }
                        }
                        Err(error) => size_error = Some(error),
                    }
                }
                response
                    .clone()
                    .on_hover_text("0.5x · 512w · 512h · 300ppi");
                size_response = Some(response);
                let previous = row.format;
                let response = fields::native_scope(ui, owner.with((row.id, "format")), |ui| {
                    egui::ComboBox::from_id_salt("format")
                        .width(100.)
                        .selected_text(row.format.label())
                        .show_ui(ui, |ui| {
                            for format in crate::export::Format::ALL {
                                ui.selectable_value(&mut row.format, format, format.label());
                            }
                        })
                })
                .inner
                .response;
                if previous != row.format && !row.format.raster() {
                    row.scale = 1;
                    row.export_resolution = None;
                    *size = "1x".into();
                    size_error = None;
                }
                welcome::name_control(
                    ui,
                    &response,
                    owner.with((row.id, "format-label")),
                    "Export row format",
                );
                if icon(
                    ui,
                    owner.with((row.id, "remove")),
                    ph::X,
                    "Remove export format row",
                    true,
                ) {
                    remove = Some(row.id);
                }
            });
            if let (Some(error), Some(response)) = (size_error, size_response) {
                welcome::control_error(ui, &[&response], &error);
            }
            text(
                ui,
                owner.with((row.id, "suffix")),
                &mut row.suffix,
                "Export row filename suffix",
            )
            .on_hover_text("Filename suffix");
        }
        if state.draft.rows.len() > 1
            && let Some(id) = remove
        {
            state.draft.rows.retain(|r| r.id != id);
            state.sizes.remove(&id);
        }
        ui.horizontal_wrapped(|ui| {
            if icon(
                ui,
                owner.with("add-row"),
                ph::PLUS,
                "Add export format row",
                state.draft.rows.len() < 16,
            ) {
                let scale = (state.draft.rows.len() + 1).min(8) as u32;
                state.draft.rows.push(Row {
                    id: crate::document::next_id(),
                    scale,
                    suffix: format!("@{scale}x"),
                    ..Default::default()
                });
            }
            if icon(
                ui,
                owner.with("save"),
                ph::FLOPPY_DISK,
                "Save export settings",
                state.draft != state.base
                    && state
                        .sizes
                        .values()
                        .all(|text| crate::screen_export::Resolution::parse(text).is_ok()),
            ) {
                match studio.set_screen_export(&state.base, state.draft.clone()) {
                    Ok(_) => state.error.clear(),
                    Err(error) => state.error = error,
                }
            }
            if icon(
                ui,
                owner.with("revert"),
                ph::ARROW_CLOCKWISE,
                "Discard pending export settings",
                state.draft != state.base,
            ) {
                state.draft = state.base.clone();
                state.sizes.clear();
                state.error.clear();
            }
        });
    });
    ui.separator();
    ui.add_enabled_ui(running.is_none(), |ui| {
        text(
            ui,
            owner.with("folder"),
            &mut state.folder,
            "Export destination folder",
        )
        .on_hover_text("Existing destination folder");
        ui.horizontal_wrapped(|ui| {
            if icon(
                ui,
                owner.with("choose"),
                ph::FOLDER_OPEN,
                "Choose export folder",
                true,
            ) {
                let key = owner;
                studio.request_file_dialog(
                    || rfd::FileDialog::new().pick_folder(),
                    move |ctx, _, path| {
                        ctx.data_mut(|d| {
                            let mut s = d.get_temp::<State>(key).unwrap_or_default();
                            s.folder = path.to_string_lossy().into_owned();
                            d.insert_temp(key, s);
                        })
                    },
                );
            }
            if icon(
                ui,
                owner.with("export"),
                ph::ARROW_UP,
                "Export chosen assets or artboards",
                state.draft.validate().is_ok()
                    && state
                        .sizes
                        .values()
                        .all(|text| crate::screen_export::Resolution::parse(text).is_ok()),
            ) {
                let applied = if state.draft == studio.doc.screen_export {
                    Ok(false)
                } else {
                    studio.set_screen_export(&state.base, state.draft.clone())
                };
                match applied.and_then(|_| start(ui.ctx(), studio, &state)) {
                    Ok(()) => state.error.clear(),
                    Err(error) => state.error = error,
                }
            }
        });
    });
    if let Some(run) = running {
        let (done, total) =
            jobs::frame_progress::<Vec<ResultFile>>(ui.ctx(), JOB).unwrap_or((0, 1));
        ui.add(
            egui::ProgressBar::new(done as f32 / total.max(1) as f32).text(jobs::stage::<
                Vec<ResultFile>,
            >(
                ui.ctx(), JOB
            )),
        );
        if icon(ui, owner.with("cancel"), ph::X, "Cancel asset export", true) {
            run.valid.store(false, Ordering::Relaxed);
        }
    }
    if let Err(error) = state.draft.validate() {
        ui.colored_label(ui.visuals().error_fg_color, error);
    }
    if !state.error.is_empty() {
        ui.colored_label(ui.visuals().error_fg_color, &state.error);
    }
    if studio.doc.collaboration.is_some() {
        ui.label("Asset metadata is available in a local document copy.");
    }
    ui.data_mut(|d| d.insert_temp(owner, state));
}

#[cfg(test)]
mod tests;
