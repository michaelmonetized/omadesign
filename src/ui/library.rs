//! A quiet home for reusable colours and project artwork.
mod brand;
mod close;
mod palettes;
use brand::brand;
use palettes::palettes;

use super::{jobs, theme};
use crate::{
    app::{
        Studio,
        libraries::{
            Libraries, PaletteDraft, Sidebar, Source, file_stamp, palette_bytes, project_folder,
        },
    },
    color::Rgba,
    document::Fill,
    geom::Pt,
    palette::Palette,
};
use eframe::egui::{self, Color32, RichText, Sense, Stroke, Ui, vec2};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};
const SYNC: &str = "library-sync";
const PALETTE_ACTION: &str = "palette-action";
const BRAND_ACTION: &str = "brand-action";
const PREVIEW: &str = "brand-preview";
const PLACE: &str = "brand-place";
type Colours = Result<(Vec<Palette>, Option<u64>), String>;
struct Snapshot {
    source: Source,
    root: Option<PathBuf>,
    personal: Colours,
    project: Option<Colours>,
    catalog: Result<Option<crate::brand::Catalog>, String>,
}
fn read_colours(path: &Path, personal: bool) -> Colours {
    match palette_bytes(path)? {
        Some((data, stamp)) => {
            let text = std::str::from_utf8(&data).map_err(|_| "Palette file is not UTF-8")?;
            Ok((crate::palette::decode(text)?, Some(stamp)))
        }
        None => Ok((
            if personal {
                crate::palette::load()
            } else {
                vec![]
            },
            None,
        )),
    }
}

struct PaletteResult {
    root: Option<PathBuf>,
    kind: Result<PaletteChange, String>,
}
enum PaletteChange {
    Saved(Vec<Palette>, Option<u64>),
    Imported(Vec<Palette>),
    Exported,
}
struct BrandResult {
    root: PathBuf,
    message: String,
}
struct Placed {
    document: String,
    at: Pt,
    imported: crate::import::Imported,
}
#[derive(Clone)]
pub(super) struct BrandDrag {
    pub path: PathBuf,
}
#[derive(Clone, Default)]
struct Previews {
    root: Option<PathBuf>,
    catalog: Option<Arc<crate::brand::Catalog>>,
    textures: Arc<HashMap<String, Result<egui::TextureHandle, String>>>,
}
type PreviewBatch = (
    PathBuf,
    Vec<(String, Result<crate::photo::RgbaImage, String>)>,
);
fn start_palette(
    ctx: &egui::Context,
    root: Option<PathBuf>,
    work: impl FnOnce() -> Result<PaletteChange, String> + Send + 'static,
) {
    jobs::start(ctx, PALETTE_ACTION, move || {
        Ok(PaletteResult { root, kind: work() })
    });
}
fn draft_at<'a>(s: &'a mut Libraries, root: &Option<PathBuf>) -> &'a mut PaletteDraft {
    if let Some(root) = root {
        s.projects.entry(root.clone()).or_default()
    } else {
        &mut s.personal
    }
}

pub fn tick(ctx: &egui::Context, studio: &mut Studio) {
    let s = &mut studio.libraries;
    let source = Source {
        document: studio.swap_id.clone(),
        path: studio.path.clone(),
        folder: s.folders.get(&studio.swap_id).cloned(),
    };
    if s.source.as_ref() != Some(&source) {
        s.source = Some(source.clone());
        s.root = None;
        s.catalog = None;
        s.brand_name.clear();
        s.brand_message.clear();
        s.next_sync = Instant::now();
    }
    if let Some(result) = jobs::poll::<PaletteResult>(ctx, PALETTE_ACTION) {
        // A refresh started before a save must never replace the saved draft.
        jobs::cancel::<Snapshot>(ctx, SYNC);
        s.next_sync = Instant::now();
        match result {
            Ok(result) => {
                if s.close_requested
                    && let Err(error) = &result.kind
                {
                    s.close_error = error.clone();
                }
                let d = draft_at(s, &result.root);
                match result.kind {
                    Ok(PaletteChange::Saved(saved, stamp)) => {
                        d.dirty = d.palettes != saved;
                        d.stamp = stamp;
                        d.loaded = true;
                        d.conflict = false;
                        d.message = "Saved. Ready to share.".into();
                    }
                    Ok(PaletteChange::Imported(palettes)) => {
                        match crate::palette::merge(&mut d.palettes, palettes) {
                            Ok(n) => {
                                d.selected = d.palettes.len().saturating_sub(1);
                                d.selected_name();
                                d.dirty = true;
                                d.message = format!("Loaded {n} palettes. Save to keep them here.");
                            }
                            Err(e) => d.message = e,
                        }
                    }
                    Ok(PaletteChange::Exported) => {
                        d.message = "Exported a portable .omacolors file.".into()
                    }
                    Err(e) => d.message = e,
                }
            }
            Err(e) => {
                if s.close_requested {
                    s.close_error = e.clone();
                }
                studio.status = e;
            }
        }
    }
    if let Some(result) = jobs::poll::<BrandResult>(ctx, BRAND_ACTION) {
        match result {
            Ok(done) => {
                if s.root.as_ref() == Some(&done.root)
                    || s.catalog.as_ref().is_some_and(|c| c.root == done.root)
                {
                    s.brand_message = done.message;
                }
                // Completion belongs to its original folder, even after a tab switch.
                jobs::cancel::<Snapshot>(ctx, SYNC);
                s.next_sync = Instant::now();
            }
            Err(e) => {
                if s.close_requested {
                    s.close_error = e.clone();
                }
                studio.status = e;
            }
        }
    }
    if let Some(result) = jobs::poll::<Snapshot>(ctx, SYNC) {
        match result {
            Ok(snapshot) if s.source.as_ref() == Some(&snapshot.source) => {
                s.root = snapshot.root;
                match snapshot.personal {
                    Ok((p, stamp)) => s.personal.receive(p, stamp),
                    Err(e) => s.personal.message = e,
                }
                if let Some(root) = s.root.clone() {
                    let draft = s.projects.entry(root).or_default();
                    if let Some(result) = snapshot.project {
                        match result {
                            Ok((p, stamp)) => draft.receive(p, stamp),
                            Err(e) => draft.message = e,
                        }
                    }
                }
                match snapshot.catalog {
                    Ok(Some(catalog)) => {
                        if s.brand_name.is_empty()
                            || s.catalog.as_ref().is_some_and(|c| c.name == s.brand_name)
                        {
                            s.brand_name = catalog.name.clone();
                        }
                        s.catalog = Some(Arc::new(catalog));
                    }
                    Ok(None) => s.catalog = None,
                    Err(e) => {
                        s.catalog = None;
                        s.brand_message = e;
                    }
                }
            }
            Ok(_) => s.next_sync = Instant::now(),
            Err(e) => s.brand_message = e,
        }
    }
    if Instant::now() >= s.next_sync && !jobs::is_running::<Snapshot>(ctx, SYNC) {
        s.next_sync = Instant::now() + Duration::from_secs(3);
        jobs::start(ctx, SYNC, move || {
            let root = project_folder(&source);
            let personal = read_colours(&crate::palette::path(), true);
            let project = root
                .as_ref()
                .map(|r| read_colours(&r.join(".omacolors"), false));
            let catalog = match &root {
                Some(r) if r.join(".omabrand").is_dir() => crate::brand::scan(r).map(Some),
                _ => Ok(None),
            };
            Ok(Snapshot {
                source,
                root,
                personal,
                project,
                catalog,
            })
        });
    }
    if s.sidebar != Sidebar::Inspector {
        ctx.request_repaint_after(Duration::from_secs(3));
    }
    if ctx.input(|i| i.key_pressed(egui::Key::Escape)) && jobs::is_running::<Placed>(ctx, PLACE) {
        jobs::cancel::<Placed>(ctx, PLACE);
        studio.status = "Asset placement cancelled.".into();
    }
    if let Some(result) = jobs::poll::<Placed>(ctx, PLACE) {
        match result {
            Ok(place) if place.document == studio.swap_id && studio.pending_nav.is_none() => {
                if let Err(e) = studio.place_brand_imported(place.imported, place.at) {
                    studio.status = e;
                }
            }
            Ok(_) => {
                studio.status =
                    "Document changed. Drag the asset into the intended artboard again.".into()
            }
            Err(e) => studio.status = e,
        }
    }
    // Sidecars are configuration, never artwork or Lottie imports.
    let sidecars: Vec<_> = ctx.input(|i| {
        i.raw
            .dropped_files
            .iter()
            .filter(|f| {
                let p = f.path();
                p.file_name()
                    .is_some_and(|n| n == ".omacolors" || n == ".omabrand")
                    || (p.is_dir()
                        && (p.join(".omacolors").is_file() || p.join(".omabrand").is_dir()))
            })
            .cloned()
            .collect()
    });
    for f in &sidecars {
        let path = f.path();
        let folder = if path.is_dir() {
            path.to_path_buf()
        } else {
            path.parent().unwrap_or(Path::new(".")).to_path_buf()
        };
        studio.libraries.choose_folder(&studio.swap_id, folder);
        studio.libraries.sidebar = if path.file_name().is_some_and(|n| n == ".omacolors") {
            Sidebar::Palettes
        } else {
            Sidebar::Brand
        };
        studio.libraries.project_scope = true;
        studio.show_welcome = false;
    }
    if !sidecars.is_empty() {
        ctx.input_mut(|i| {
            i.raw
                .dropped_files
                .retain(|f| !sidecars.iter().any(|s| s.path() == f.path()))
        });
    }
    close::show(ctx, studio);
}

pub fn tabs(ui: &mut Ui, studio: &mut Studio) {
    ui.horizontal(|ui| {
        for (tab, name) in [
            (Sidebar::Inspector, "Inspect"),
            (Sidebar::Palettes, "Palettes"),
            (Sidebar::Brand, "Brand"),
        ] {
            ui.selectable_value(&mut studio.libraries.sidebar, tab, name);
        }
    });
    ui.add_space(10.);
}
pub fn show(ui: &mut Ui, studio: &mut Studio) {
    let mut state = std::mem::take(&mut studio.libraries);
    match state.sidebar {
        Sidebar::Palettes => palettes(ui, studio, &mut state),
        Sidebar::Brand => brand(ui, studio, &mut state),
        _ => {}
    }
    studio.libraries = state;
}
fn project_picker(ui: &mut Ui, studio: &Studio, s: &mut Libraries) {
    ui.horizontal_wrapped(|ui| {
        let name = s
            .root
            .as_ref()
            .and_then(|r| r.file_name())
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "Choose a project".into());
        if ui
            .small_button(name)
            .on_hover_text(
                s.root
                    .as_ref()
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|| "Link a folder to this document".into()),
            )
            .clicked()
            && let Some(folder) = crate::project::dialog_folder()
        {
            s.choose_folder(&studio.swap_id, folder);
        }
        if jobs::is_running::<Snapshot>(ui.ctx(), SYNC) {
            ui.spinner();
        }
    });
}
fn note(ui: &mut Ui, text: &str) {
    if !text.is_empty() {
        ui.add_space(5.);
        ui.label(RichText::new(text).small().color(theme::fg_weak()));
    }
}
fn checker(ui: &Ui, rect: egui::Rect) {
    ui.painter().rect_filled(rect, 6., theme::bg_widget());
    let cell = 8.;
    for y in 0..(rect.height() / cell).ceil() as usize {
        for x in 0..(rect.width() / cell).ceil() as usize {
            if (x + y) % 2 == 0 {
                let r = egui::Rect::from_min_size(
                    rect.min + vec2(x as f32 * cell, y as f32 * cell),
                    vec2(cell, cell),
                )
                .intersect(rect);
                ui.painter().rect_filled(r, 0., theme::bg_panel());
            }
        }
    }
}
pub(super) fn queue_place(ctx: &egui::Context, studio: &mut Studio, path: PathBuf, at: Pt) {
    if jobs::is_running::<Placed>(ctx, PLACE) {
        studio.status = "An asset is still loading. Place the next one when it is ready.".into();
        return;
    }
    let document = studio.swap_id.clone();
    studio.status = "Loading brand artwork…".into();
    jobs::start(ctx, PLACE, move || {
        Ok(Placed {
            document,
            at,
            imported: crate::brand::load_asset(&path)?,
        })
    });
}
pub(super) fn canvas_drop(ui: &Ui, studio: &mut Studio, response: &egui::Response) -> bool {
    let dragging = egui::DragAndDrop::has_payload_of_type::<BrandDrag>(ui.ctx());
    if let Some(asset) = response.dnd_release_payload::<BrandDrag>()
        && let Some(pos) = ui.ctx().input(|i| i.pointer.interact_pos())
    {
        let at = studio.view.pointer_to_world(
            Pt::new(response.rect.min.x, response.rect.min.y),
            Pt::new(pos.x, pos.y),
        );
        queue_place(ui.ctx(), studio, asset.path.clone(), at);
        return true;
    }
    if dragging && response.contains_pointer() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Copy);
    }
    dragging
}
pub fn ready(ctx: &egui::Context, studio: &Studio) -> bool {
    if studio.libraries.sidebar == Sidebar::Inspector {
        return true;
    }
    (studio.libraries.root.is_some()
        || (studio.libraries.sidebar == Sidebar::Palettes
            && !studio.libraries.project_scope
            && studio.libraries.personal.loaded))
        && !jobs::is_running::<Snapshot>(ctx, SYNC)
        && !jobs::is_running::<PreviewBatch>(ctx, PREVIEW)
}

#[cfg(test)]
#[path = "library_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "library_layout_tests.rs"]
mod layout_tests;
