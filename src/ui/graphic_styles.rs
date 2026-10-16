//! Native style controls keep the portable library separate from document history.
use super::{
    fields,
    icons::{self, ph},
    welcome,
};
use crate::{app::Studio, graphic_styles::Library};
use eframe::egui::{self, Id, Ui};
use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
};

#[derive(Clone, Default)]
struct State {
    path: PathBuf,
    book: Option<Library>,
    selected: Option<u64>,
    query: String,
    name: String,
    error: String,
}
#[derive(Clone, Default)]
struct Previews {
    stamp: Option<u64>,
    textures: HashMap<u64, egui::TextureHandle>,
    failed: HashSet<u64>,
}
type Rendered = (Option<u64>, u64, Result<egui::ColorImage, String>);
const JOB: &str = "graphic-style-thumbnail";

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
fn load(path: PathBuf) -> State {
    match Library::load(&path) {
        Ok(book) => State {
            path,
            book: Some(book),
            ..Default::default()
        },
        Err(error) => State {
            path,
            error,
            ..Default::default()
        },
    }
}
fn save(state: &mut State, mut after: Library) -> bool {
    match after.save(&state.path) {
        Ok(()) => {
            state.book = Some(after);
            state.error.clear();
            true
        }
        Err(error) => {
            state.error = error;
            false
        }
    }
}
fn capture(studio: &Studio, state: &mut State, replace: bool) {
    let result = (|| -> Result<(Library, u64, String), String> {
        let appearance = studio.capture_graphic_style()?;
        let mut after = state
            .book
            .clone()
            .ok_or("Reload the style library before saving")?;
        let (id, name) = if replace {
            let entry = after
                .entries
                .iter_mut()
                .find(|e| Some(e.id) == state.selected)
                .ok_or("Select the current graphic style")?;
            entry.appearance = appearance;
            (entry.id, entry.name.clone())
        } else {
            let name = format!("Style {}", after.next + 1);
            let id = after.add(name, appearance)?;
            (id, after.entries.last().unwrap().name.clone())
        };
        Ok((after, id, name))
    })();
    match result {
        Ok((after, id, name)) => {
            if save(state, after) {
                state.selected = Some(id);
                state.name = name;
            }
        }
        Err(error) => state.error = error,
    }
}

/// Draw actual appearance presets with source-bound previews and atomic disk mutations.
/// Args: pane UI and editor. Returns nothing; document changes use one existing command batch.
pub(super) fn show(ui: &mut Ui, studio: &mut Studio) {
    let path = ui
        .data(|d| d.get_temp::<PathBuf>(Id::new("graphic-style-file")))
        .unwrap_or_else(|| crate::palette::path().with_file_name("graphic-styles.json"));
    let owner = Id::new(("graphic-styles", &path));
    let mut state = ui
        .data_mut(|d| d.remove_temp::<State>(owner))
        .unwrap_or_else(|| load(path.clone()));
    let mut previews = ui
        .data_mut(|d| d.remove_temp::<Previews>(owner.with("previews")))
        .unwrap_or_default();
    let stamp = state.book.as_ref().and_then(|b| b.stamp);
    if previews.stamp != stamp {
        previews = Previews {
            stamp,
            ..Default::default()
        };
    }
    if let Some(Ok((source, id, result))) = super::jobs::poll::<Rendered>(ui.ctx(), JOB)
        && source == stamp
    {
        match result {
            Ok(pixels) => {
                previews.textures.insert(
                    id,
                    ui.ctx().load_texture(
                        format!("graphic-style-{source:?}-{id}"),
                        pixels,
                        egui::TextureOptions::LINEAR,
                    ),
                );
            }
            Err(error) => {
                previews.failed.insert(id);
                state.error = error;
            }
        }
    }
    let query = ui.add(
        egui::TextEdit::singleline(&mut state.query)
            .id(owner.with("query"))
            .hint_text("Find graphic styles")
            .desired_width(ui.available_width()),
    );
    welcome::name_control(ui, &query, owner.with("query-label"), "Find graphic styles");
    let entries = state
        .book
        .as_ref()
        .map_or_else(Vec::new, |b| b.entries.clone());
    let mut apply = None;
    for entry in entries
        .iter()
        .filter(|e| e.name.to_lowercase().contains(&state.query.to_lowercase()))
    {
        let response = fields::native_scope(ui, owner.with(entry.id), |ui| {
            ui.allocate_response(egui::vec2(ui.available_width(), 42.), egui::Sense::click())
        })
        .inner;
        welcome::name_control(
            ui,
            &response,
            owner.with((entry.id, "label")),
            &format!("Graphic style {}", entry.name),
        );
        response.widget_info(|| {
            egui::WidgetInfo::selected(
                egui::WidgetType::Button,
                ui.is_enabled(),
                state.selected == Some(entry.id),
                format!("Graphic style {}", entry.name),
            )
        });
        if state.selected == Some(entry.id) || response.hovered() {
            ui.painter().rect_filled(
                response.rect,
                3.,
                if state.selected == Some(entry.id) {
                    ui.visuals().selection.bg_fill
                } else {
                    ui.visuals().widgets.hovered.weak_bg_fill
                },
            );
        }
        if let Some(texture) = previews.textures.get(&entry.id) {
            let rect = egui::Rect::from_min_size(
                response.rect.min + egui::vec2(3., 3.),
                egui::vec2(36., 36.),
            );
            ui.painter().image(
                texture.id(),
                rect,
                egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1., 1.)),
                egui::Color32::WHITE,
            );
        } else if ui.is_rect_visible(response.rect)
            && !previews.failed.contains(&entry.id)
            && !super::jobs::is_running::<Rendered>(ui.ctx(), JOB)
        {
            let id = entry.id;
            let doc = entry.appearance.preview();
            super::jobs::start(ui.ctx(), JOB, move || {
                let result = super::chrome::document_previews::snapshot(&doc)
                    .and_then(|proxy| super::chrome::document_previews::render(&proxy, None));
                Ok((stamp, id, result))
            });
        }
        ui.painter()
            .with_clip_rect(response.rect.intersect(ui.clip_rect()))
            .text(
                response.rect.left_center() + egui::vec2(46., 0.),
                egui::Align2::LEFT_CENTER,
                &entry.name,
                egui::FontId::proportional(12.),
                ui.visuals().text_color(),
            );
        if response.clicked() {
            state.selected = Some(entry.id);
            state.name = entry.name.clone();
        }
        if response.double_clicked() {
            apply = Some(entry.appearance.clone());
        }
    }
    let selected = entries.iter().find(|e| Some(e.id) == state.selected);
    ui.horizontal_wrapped(|ui| {
        if icon(
            ui,
            owner.with("new"),
            ph::PLUS,
            "Capture selection as a graphic style",
            state.book.is_some() && entries.len() < 256 && studio.primary().is_some(),
        ) {
            capture(studio, &mut state, false);
        }
        if icon(
            ui,
            owner.with("apply"),
            ph::PALETTE,
            "Apply selected graphic style",
            selected.is_some() && !studio.selection.is_empty(),
        ) {
            apply = selected.map(|e| e.appearance.clone());
        }
        if icon(
            ui,
            owner.with("replace"),
            ph::ARROW_UP,
            "Redefine graphic style from selection",
            selected.is_some() && studio.primary().is_some(),
        ) {
            capture(studio, &mut state, true);
        }
        if icon(
            ui,
            owner.with("copy"),
            ph::COPY,
            "Duplicate graphic style",
            selected.is_some() && entries.len() < 256,
        ) && let Some(entry) = selected
            && let Some(mut after) = state.book.clone()
        {
            let name = format!("{} copy", entry.name.chars().take(59).collect::<String>());
            match after.add(name.clone(), entry.appearance.clone()) {
                Ok(id) => {
                    let name = after.entries.last().unwrap().name.clone();
                    if save(&mut state, after) {
                        state.selected = Some(id);
                        state.name = name;
                    }
                }
                Err(error) => state.error = error,
            }
        }
        if icon(
            ui,
            owner.with("delete"),
            ph::TRASH,
            "Delete graphic style",
            selected.is_some(),
        ) && let Some(mut after) = state.book.clone()
        {
            after.entries.retain(|e| Some(e.id) != state.selected);
            if save(&mut state, after) {
                state.selected = None;
                state.name.clear();
            }
        }
        if icon(
            ui,
            owner.with("reload"),
            ph::ARROW_CLOCKWISE,
            "Reload graphic styles",
            true,
        ) {
            state = load(path.clone());
        }
    });
    if let Some(id) = state.selected {
        let mut rename = false;
        ui.horizontal(|ui| {
            let response = ui.add(
                egui::TextEdit::singleline(&mut state.name)
                    .id(owner.with((id, "name")))
                    .desired_width((ui.available_width() - 28.).max(24.)),
            );
            welcome::name_control(
                ui,
                &response,
                owner.with((id, "name-label")),
                "Graphic style name",
            );
            rename = (response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)))
                || icon(
                    ui,
                    owner.with("rename"),
                    ph::PENCIL,
                    "Rename graphic style",
                    true,
                );
        });
        if rename
            && let Some(mut after) = state.book.clone()
            && let Some(entry) = after.entries.iter_mut().find(|e| e.id == id)
        {
            entry.name = state.name.trim().into();
            if save(&mut state, after) {
                state.name = state.name.trim().into();
            }
        }
    }
    if let Some(appearance) = apply {
        match studio.apply_graphic_style(&appearance) {
            Ok(_) => {
                state.error.clear();
                studio.status = "Graphic style applied".into();
            }
            Err(error) => state.error = error,
        }
    }
    if !state.error.is_empty() {
        ui.colored_label(ui.visuals().error_fg_color, &state.error);
    }
    ui.data_mut(|d| {
        d.insert_temp(owner.with("previews"), previews);
        d.insert_temp(owner, state);
    });
}

#[cfg(test)]
mod tests {
    use super::super::welcome::keyboard_tests as keys;
    use super::*;
    use egui::{Event, Key, Modifiers};
    fn context(path: &std::path::Path) -> egui::Context {
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        super::super::theme::apply(&ctx);
        ctx.data_mut(|d| d.insert_temp(Id::new("graphic-style-file"), path.to_owned()));
        ctx
    }
    #[test]
    fn craft_graphic_style_native_capture_paste_rename_cold_load_apply_and_undo() {
        let folder = std::env::temp_dir().join(format!(
            "native-graphic-styles-{}",
            crate::document::next_id()
        ));
        let path = folder.join("styles.json");
        let ctx = context(&path);
        let mut studio = crate::app::symbols::tests::fixture();
        let mut frame = |events| {
            keys::render(&ctx, events, |ui| {
                ui.set_max_width(256.);
                show(ui, &mut studio);
            })
        };
        frame(vec![]);
        frame(vec![]);
        keys::tab_to(&ctx, "Capture selection as a graphic style", &mut frame);
        frame(keys::key(Key::Space, Modifiers::NONE));
        frame(vec![]);
        keys::tab_to(&ctx, "Graphic style name", &mut frame);
        frame(keys::key(Key::A, Modifiers::CTRL | Modifiers::COMMAND));
        frame(vec![Event::Paste("Cold style Ω".into())]);
        frame(keys::key(Key::Enter, Modifiers::NONE));
        frame(vec![]);
        drop(frame);
        let saved = Library::load(&path).unwrap();
        assert_eq!(saved.entries.len(), 1);
        assert_eq!(saved.entries[0].name, "Cold style Ω");
        assert_eq!(studio.history.len(), 0);
        let cold = context(&path);
        let mut destination = crate::app::symbols::tests::fixture();
        let original = crate::project::encode(&destination.doc).unwrap();
        let mut frame = |events| {
            keys::render(&cold, events, |ui| {
                ui.set_max_width(256.);
                show(ui, &mut destination);
            })
        };
        frame(vec![]);
        frame(vec![]);
        keys::tab_to(&cold, "Graphic style Cold style Ω", &mut frame);
        frame(keys::key(Key::Space, Modifiers::NONE));
        frame(vec![]);
        keys::tab_to(&cold, "Apply selected graphic style", &mut frame);
        frame(keys::key(Key::Space, Modifiers::NONE));
        frame(vec![]);
        drop(frame);
        assert_eq!(destination.history.len(), 1);
        for &(layer, id) in &destination.selection {
            assert_eq!(
                crate::graphic_styles::Appearance::capture(
                    destination.doc.find_shape(layer, id).unwrap()
                ),
                saved.entries[0].appearance
            );
        }
        let applied = crate::project::encode(&destination.doc).unwrap();
        assert_eq!(
            crate::project::encode(&crate::project::decode(&applied).unwrap()).unwrap(),
            applied
        );
        destination.undo();
        assert_eq!(crate::project::encode(&destination.doc).unwrap(), original);
        std::fs::remove_dir_all(folder).unwrap();
    }
}
