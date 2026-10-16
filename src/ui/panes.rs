//! Compact pane bodies call the same editing commands as the canvas and menus.
use super::icons::{self, ph};
use crate::{
    app::Studio,
    document::{Cmd, Style},
    geom::{Geom, Pt},
    tools::{Persona, Tool},
};
use eframe::egui::{self, Ui};

/// Draw one registered pane without replacing the editor's selection or history.
/// Args: pane UI, editor and stable pane ID. Returns nothing; real commands own every edit.
pub(super) fn show(ui: &mut Ui, studio: &mut Studio, panel: &str) {
    let key = egui::Id::new("dock-pane-content");
    let previous = ui.data(|d| d.get_temp::<bool>(key)).unwrap_or(false);
    ui.data_mut(|d| d.insert_temp(key, true));
    let view = egui::Id::new((&studio.swap_id, panel));
    super::fields::with_native_view(ui, view, |ui| contents(ui, studio, panel));
    ui.data_mut(|d| d.insert_temp(key, previous));
}

fn contents(ui: &mut Ui, studio: &mut Studio, panel: &str) {
    ui.set_min_width(0.0);
    ui.set_max_width(ui.available_width());
    ui.spacing_mut().item_spacing = egui::vec2(5.0, 5.0);
    if studio.file_dialog_pending() || studio.updates.freezing {
        ui.disable();
    }
    if super::studios::pane(ui, studio, panel) {
        return;
    }
    match panel {
        "align" => super::selection::arrange_panel(ui, studio),
        "pathfinder" => pathfinder(ui, studio),
        "swatches" => super::library::pane(ui, studio, crate::app::libraries::Sidebar::Palettes),
        "typography" => {
            super::library::pane(ui, studio, crate::app::libraries::Sidebar::Typography)
        }
        "assets" => super::library::pane(ui, studio, crate::app::libraries::Sidebar::Brand),
        "patterns" => super::svg_paint::show(ui, studio),
        "paragraph" => super::paragraph::show(ui, studio),
        "opentype" => {
            let font = studio
                .selected_type()
                .unwrap_or_else(|| studio.type_defaults())
                .font;
            super::opentype::show(ui, studio, &font);
        }
        "glyphs" => glyphs(ui, studio),
        "layout" => super::layout::inspector(ui, studio),
        "hierarchy" => super::layout::hierarchy_content(ui, studio),
        "variables" => super::layout::variables(ui, studio),
        "components" => super::layout_preview::component_editor(ui, studio),
        "prototype" => super::layout_preview::interaction_editor(ui, studio),
        "timeline" => super::timeline::content(ui, studio),
        "compositions" => super::motion_compositions::controls(ui, studio),
        "scene3d" => super::scene3d::pane(ui, studio),
        "photo-library" => super::photo::filmstrip(ui, studio),
        "photo-develop" => super::photo::develop_content(ui, studio),
        "histogram" => histogram(ui, studio),
        "presets" => photo_presets(ui, studio),
        "adjustments" => super::raster::inspector(ui, studio),
        "mask" => super::masking::inspector(ui, studio),
        "selection" => super::masking::selection_tools(ui, studio),
        "navigator" => navigator(ui, studio),
        "history" => history(ui, studio),
        "actions" => actions(ui, studio),
        "styles" => styles(ui, studio),
        "shapes" => shapes(ui, studio),
        "paths" => paths(ui, studio),
        "channels" => channels(ui, studio),
        "artboards" => artboards(ui, studio),
        "links" => links(ui, studio),
        "info" => info(ui, studio),
        _ => {}
    }
}

fn pathfinder(ui: &mut Ui, studio: &mut Studio) {
    ui.horizontal_wrapped(|ui| {
        for (index, op) in crate::boolean::BoolOp::all().into_iter().enumerate() {
            let icon = [ph::PLUS, ph::MINUS, ph::SELECTION, ph::X][index % 4];
            ui.add_enabled_ui(studio.selection.len() >= 2, |ui| {
                if icons::icon_button(ui, icon, op.name(), false) {
                    studio.pathfinder(op);
                }
            });
        }
        if icons::icon_button(ui, ph::PATH, "Simplify selected paths", false) {
            studio.simplify_selection();
        }
    });
}

fn navigator(ui: &mut Ui, studio: &mut Studio) {
    if studio.persona == Persona::Photo {
        photo_navigator(ui, studio);
        return;
    }
    let edge = ui.available_width().min(260.0).max(48.0);
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(edge, edge), egui::Sense::click_and_drag());
    if let Some(texture) = super::pane_previews::image(ui, studio, None, None) {
        ui.painter().image(
            texture.id(),
            rect,
            egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
            egui::Color32::WHITE,
        );
    }
    let bounds = super::chrome::document_previews::bounds(&studio.doc);
    let scale = (120.0 / bounds.width().max(1.0)).min(120.0 / bounds.height().max(1.0));
    if let Some(canvas) = studio.canvas_rect {
        let from_world = |world: Pt| {
            rect.center()
                + egui::vec2(
                    (world.x - bounds.center().x) * scale,
                    (world.y - bounds.center().y) * scale,
                ) * (edge / 128.0)
        };
        let min = Pt::new(
            (canvas.left() - studio.view.offset.x) / studio.view.scale,
            (canvas.top() - studio.view.offset.y) / studio.view.scale,
        );
        let max = Pt::new(
            (canvas.right() - studio.view.offset.x) / studio.view.scale,
            (canvas.bottom() - studio.view.offset.y) / studio.view.scale,
        );
        let visible = egui::Rect::from_min_max(from_world(min), from_world(max)).intersect(rect);
        ui.painter().rect_stroke(
            visible,
            0.0,
            egui::Stroke::new(1.0, super::theme::accent()),
            egui::StrokeKind::Inside,
        );
        if (response.clicked() || response.dragged())
            && let Some(pointer) = response.interact_pointer_pos()
        {
            let delta = (pointer - rect.center()) / (scale * edge / 128.0);
            let world = bounds.center() + Pt::new(delta.x, delta.y);
            studio.view.offset =
                Pt::new(canvas.center().x, canvas.center().y) - world * studio.view.scale;
            ui.ctx().request_repaint();
        }
    }
    ui.horizontal(|ui| {
        if icons::tiny_icon(ui, ph::FRAME_CORNERS, "Fit artwork", false) {
            studio.fit_view();
        }
        let mut zoom = studio.view.scale * 100.0;
        if ui
            .add(
                egui::DragValue::new(&mut zoom)
                    .range(1.0..=6400.0)
                    .suffix("%")
                    .speed(1.0),
            )
            .changed()
        {
            let anchor = studio
                .canvas_rect
                .map_or(Pt::ZERO, |r| Pt::new(r.center().x, r.center().y));
            studio.zoom_by(zoom / 100.0 / studio.view.scale.max(0.001), anchor);
        }
    });
}

fn photo_navigator(ui: &mut Ui, studio: &mut Studio) {
    let texture = if studio.photo.show_original {
        studio.photo.orig_tex.clone()
    } else {
        studio
            .photo
            .tex
            .clone()
            .or_else(|| studio.photo.orig_tex.clone())
    };
    let Some(texture) = texture else {
        return;
    };
    let size = texture.size_vec2();
    let factor = (ui.available_width().min(260.0) / size.x).min(200.0 / size.y);
    let (rect, response) = ui.allocate_exact_size(size * factor, egui::Sense::click_and_drag());
    ui.painter().image(
        texture.id(),
        rect,
        egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
        egui::Color32::WHITE,
    );
    if let Some(canvas) = ui
        .ctx()
        .read_response(egui::Id::new("studio-photo-canvas"))
        .map(|r| r.rect)
    {
        let full = studio.photo.selected().map_or(size, |image| {
            let (w, h) = image.dimensions();
            let (w, h) = if studio.photo.show_original {
                (w, h)
            } else {
                image.develop.output_dim(w, h)
            };
            egui::vec2(w as f32, h as f32)
        });
        let visible = full * studio.photo.fit_scale * studio.photo.view_scale;
        let origin = canvas.center() + studio.photo.view_offset - visible * 0.5;
        let map = |point: egui::Pos2| rect.min + (point - origin) / visible * rect.size();
        let viewport = egui::Rect::from_min_max(map(canvas.min), map(canvas.max)).intersect(rect);
        ui.painter().rect_stroke(
            viewport,
            0.0,
            egui::Stroke::new(1.0, super::theme::accent()),
            egui::StrokeKind::Inside,
        );
        if (response.clicked() || response.dragged())
            && let Some(pointer) = response.interact_pointer_pos()
        {
            studio.photo.view_offset = -((pointer - rect.center()) / rect.size()) * visible;
        }
    }
    ui.horizontal(|ui| {
        if icons::tiny_icon(ui, ph::FRAME_CORNERS, "Fit photo", false) {
            studio.photo.view_scale = 1.0;
            studio.photo.view_offset = egui::Vec2::ZERO;
        }
        ui.add(
            egui::DragValue::new(&mut studio.photo.view_scale)
                .range(0.1..=20.0)
                .speed(0.01)
                .prefix("×"),
        );
    });
}

fn command_name(cmd: &Cmd) -> &'static str {
    match cmd {
        Cmd::Batch(commands) => commands.first().map_or("Edit", command_name),
        Cmd::AddShape { .. } => "Create object",
        Cmd::RemoveShapes { .. } => "Delete objects",
        Cmd::SetGeom { .. }
        | Cmd::SetGeoms { .. }
        | Cmd::MoveShapes { .. }
        | Cmd::SetRasterXform { .. } => "Transform",
        Cmd::SetStyle { .. } => "Appearance",
        Cmd::SetOpacity { .. } | Cmd::SetFillOpacity { .. } | Cmd::SetBlend { .. } => {
            "Transparency"
        }
        Cmd::AddLayer { .. } => "New layer",
        Cmd::RemoveLayer { .. } => "Delete layer",
        Cmd::SetLayerMeta { .. } => "Layer properties",
        Cmd::ReorderLayer { .. } | Cmd::ReorderShape { .. } => "Reorder",
        Cmd::SetLayerParent { .. } => "Group",
        Cmd::SetMotion { .. } => "Animation",
        Cmd::SetLayout { .. } => "Layout",
        Cmd::SetLayoutTokens { .. } => "Variable",
        Cmd::SetFilters { .. } | Cmd::SetShapeFilters { .. } | Cmd::SetSvgFilter { .. } => {
            "Effects"
        }
        Cmd::SetLayerMask { .. } | Cmd::SetShapeMask { .. } | Cmd::SetArtworkMasks { .. } => "Mask",
        Cmd::Pixels { .. } | Cmd::NativePixels { .. } | Cmd::PaintRaster { .. } => "Paint",
        Cmd::SetArtboards { .. } => "Artboard",
        _ => "Edit",
    }
}

fn history(ui: &mut Ui, studio: &mut Studio) {
    if studio.persona == Persona::Photo {
        ui.horizontal(|ui| {
            if ui
                .add_enabled(
                    studio.photo.can_undo(),
                    egui::Button::new(egui::RichText::new(ph::ARROW_UP).font(icons::font(16.0))),
                )
                .on_hover_text("Undo photo edit")
                .clicked()
            {
                studio.photo.undo();
            }
            if ui
                .add_enabled(
                    studio.photo.can_redo(),
                    egui::Button::new(egui::RichText::new(ph::ARROW_DOWN).font(icons::font(16.0))),
                )
                .on_hover_text("Redo photo edit")
                .clicked()
            {
                studio.photo.redo();
            }
        });
        let steps = studio.photo.history_steps();
        for (index, count) in steps.iter().enumerate().rev() {
            if ui
                .selectable_label(
                    index + 1 == steps.len(),
                    format!(
                        "{}  Adjust {count} photo{}",
                        index + 1,
                        if *count == 1 { "" } else { "s" }
                    ),
                )
                .clicked()
            {
                while studio.photo.history_steps().len() > index + 1 && studio.photo.can_undo() {
                    studio.photo.undo();
                }
            }
        }
        return;
    }
    ui.horizontal(|ui| {
        ui.add_enabled_ui(
            studio.history.can_undo() || studio.doc.collaboration.is_some(),
            |ui| {
                if icons::tiny_icon(ui, ph::ARROW_UP, "Undo", false) {
                    studio.undo();
                }
            },
        );
        ui.add_enabled_ui(
            studio.history.can_redo() || studio.doc.collaboration.is_some(),
            |ui| {
                if icons::tiny_icon(ui, ph::ARROW_DOWN, "Redo", false) {
                    studio.redo();
                }
            },
        );
    });
    let entries: Vec<_> = studio
        .history
        .entries()
        .iter()
        .enumerate()
        .map(|(index, cmd)| (index, command_name(cmd)))
        .collect();
    let mut target = None;
    for (index, label) in entries.iter().rev() {
        if ui
            .selectable_label(
                *index + 1 == entries.len(),
                format!("{}  {label}", index + 1),
            )
            .clicked()
        {
            target = Some(index + 1);
        }
    }
    if let Some(target) = target {
        while studio.history.len() > target {
            let before = studio.history.len();
            studio.undo();
            if studio.history.len() == before {
                break;
            }
        }
    }
}

#[derive(Clone, Copy, serde::Serialize, serde::Deserialize)]
enum Step {
    Duplicate,
    Group,
    Ungroup,
    FlipHorizontal,
    FlipVertical,
    Outline,
    Front,
    Back,
}
impl Step {
    const ALL: [Self; 8] = [
        Self::Duplicate,
        Self::Group,
        Self::Ungroup,
        Self::FlipHorizontal,
        Self::FlipVertical,
        Self::Outline,
        Self::Front,
        Self::Back,
    ];
    fn name(self) -> &'static str {
        match self {
            Self::Duplicate => "Duplicate",
            Self::Group => "Group",
            Self::Ungroup => "Ungroup",
            Self::FlipHorizontal => "Flip horizontal",
            Self::FlipVertical => "Flip vertical",
            Self::Outline => "Create outlines",
            Self::Front => "Bring to front",
            Self::Back => "Send to back",
        }
    }
    fn apply(self, studio: &mut Studio) {
        match self {
            Self::Duplicate => studio.duplicate_selection(),
            Self::Group => studio.group_selected(),
            Self::Ungroup => studio.ungroup_selected(),
            Self::FlipHorizontal => studio.flip_selection(true),
            Self::FlipVertical => studio.flip_selection(false),
            Self::Outline => {
                for (layer, id) in studio.selection.clone() {
                    studio.convert_object_to_path(layer, id);
                }
            }
            Self::Front => studio.bring_to_front(),
            Self::Back => studio.send_to_back(),
        }
    }
}
#[derive(Clone, Default, serde::Serialize, serde::Deserialize)]
struct Action {
    name: String,
    steps: Vec<Step>,
}
fn actions(ui: &mut Ui, studio: &mut Studio) {
    let key = egui::Id::new("editor-actions");
    let mut actions = ui
        .ctx()
        .data_mut(|d| d.get_persisted::<Vec<Action>>(key))
        .unwrap_or_default();
    let mut remove = None;
    for (index, action) in actions.iter_mut().enumerate() {
        ui.push_id(index, |ui| {
            ui.horizontal(|ui| {
                if icons::tiny_icon(ui, ph::PLAY, "Play action on the selection", false) {
                    for step in &action.steps {
                        step.apply(studio);
                    }
                }
                ui.add(
                    egui::TextEdit::singleline(&mut action.name)
                        .desired_width((ui.available_width() - 30.0).max(30.0)),
                );
                if icons::tiny_icon(ui, ph::TRASH, "Delete action", false) {
                    remove = Some(index);
                }
            });
            for (step_index, step) in action.steps.iter().enumerate() {
                ui.label(format!("{}  {}", step_index + 1, step.name()));
            }
            ui.menu_button("+", |ui| {
                for step in Step::ALL {
                    if ui
                        .add_enabled(action.steps.len() < 32, egui::Button::new(step.name()))
                        .clicked()
                    {
                        action.steps.push(step);
                        ui.close();
                    }
                }
            });
        });
    }
    if let Some(index) = remove {
        actions.remove(index);
    }
    if actions.len() < 32 && icons::tiny_icon(ui, ph::PLUS, "New action", false) {
        actions.push(Action {
            name: format!("Action {}", actions.len() + 1),
            steps: vec![],
        });
    }
    ui.ctx().data_mut(|d| d.insert_persisted(key, actions));
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
struct SavedStyle {
    name: String,
    style: Style,
}
fn styles(ui: &mut Ui, studio: &mut Studio) {
    let key = egui::Id::new("editor-graphic-styles");
    let mut styles = ui
        .ctx()
        .data_mut(|d| d.get_persisted::<Vec<SavedStyle>>(key))
        .unwrap_or_default();
    let mut remove = None;
    for (index, saved) in styles.iter_mut().enumerate() {
        ui.push_id(index, |ui| {
            ui.horizontal(|ui| {
                if icons::tiny_icon(ui, ph::PALETTE, &saved.name, false) {
                    let before = studio.style_clip.replace(saved.style.clone());
                    studio.paste_style();
                    studio.style_clip = before;
                }
                ui.add(
                    egui::TextEdit::singleline(&mut saved.name)
                        .desired_width((ui.available_width() - 30.0).max(30.0)),
                );
                if icons::tiny_icon(ui, ph::TRASH, "Delete style", false) {
                    remove = Some(index);
                }
            });
        });
    }
    if let Some(index) = remove {
        styles.remove(index);
    }
    if styles.len() < 64
        && icons::tiny_icon(ui, ph::PLUS, "Save selection appearance as a style", false)
    {
        styles.push(SavedStyle {
            name: format!("Style {}", styles.len() + 1),
            style: super::studios::inspected_style(studio).clone(),
        });
    }
    ui.ctx().data_mut(|d| d.insert_persisted(key, styles));
}

fn shapes(ui: &mut Ui, studio: &mut Studio) {
    type Loaded = (String, Geom, String, Option<(usize, u64)>);
    const JOB: &str = "pane-shape-place";
    if let Some(result) = super::jobs::poll::<Loaded>(ui.ctx(), JOB) {
        match result {
            Ok((document, geometry, name, target)) if document == studio.swap_id => {
                studio.place_palette_shape(geometry, name, target)
            }
            Ok(_) => {}
            Err(error) => studio.status = error,
        }
    }
    ui.add(
        egui::TextEdit::singleline(&mut studio.shape_query)
            .hint_text("Search shapes")
            .desired_width(ui.available_width()),
    );
    let entries = crate::shape_browser::search(&studio.shape_query, &studio.shape_lib, 80);
    let columns = (ui.available_width() / 48.0).floor().max(1.0) as usize;
    egui::Grid::new("pane-shapes")
        .num_columns(columns)
        .show(ui, |ui| {
            for (index, icon) in entries.iter().enumerate() {
                let glyph = crate::shape_browser::phosphor_char(icon.name)
                    .map(|c| c.to_string())
                    .unwrap_or_else(|| "◇".into());
                if icons::icon_button(ui, &glyph, icon.name, false)
                    && !super::jobs::is_running::<Loaded>(ui.ctx(), JOB)
                {
                    let icon = icon.clone();
                    let document = studio.swap_id.clone();
                    let target = studio.asset_frame_target();
                    super::jobs::start(ui.ctx(), JOB, move || {
                        let geom = crate::shape_browser::icon_to_geom(&icon, 256.0)?;
                        Ok((document, geom, icon.name.to_owned(), target))
                    });
                }
                if (index + 1) % columns == 0 {
                    ui.end_row();
                }
            }
        });
}

fn paths(ui: &mut Ui, studio: &mut Studio) {
    let rows: Vec<_> = studio
        .doc
        .layers
        .iter()
        .enumerate()
        .flat_map(|(layer, item)| {
            item.kind
                .shapes()
                .unwrap_or_default()
                .iter()
                .filter(|shape| {
                    matches!(
                        shape.geom,
                        Geom::Path { .. } | Geom::Paths { .. } | Geom::Poly { .. }
                    )
                })
                .map(move |shape| (layer, shape.id, shape.name.clone(), shape.locked))
        })
        .collect();
    for (layer, id, name, locked) in rows {
        if ui
            .add_enabled(
                !locked && studio.doc.layer_editable(layer),
                egui::Button::new(name).selected(studio.selection.contains(&(layer, id))),
            )
            .clicked()
        {
            studio.commit_type_edit();
            studio.active_layer = Some(layer);
            studio.selection = vec![(layer, id)];
            studio.selected_layer = None;
            studio.set_tool(Tool::Node);
        }
    }
    ui.horizontal(|ui| {
        if icons::tiny_icon(ui, ph::PATH, "Convert selection to paths", false) {
            for (layer, id) in studio.selection.clone() {
                studio.convert_object_to_path(layer, id);
            }
        }
        if icons::tiny_icon(ui, ph::SELECTION, "Load path as pixel selection", false) {
            if let Some((layer, id)) = studio.primary() {
                studio.select_item_outline(layer, Some(id));
            }
        }
    });
}

fn channels(ui: &mut Ui, studio: &mut Studio) {
    let _ = super::pane_previews::image(ui, studio, None, None);
    let Some(pixels) = super::pane_previews::pixels(ui.ctx(), studio) else {
        ui.spinner();
        return;
    };
    let key = ui.make_persistent_id("channel-preview");
    let mut channel = ui
        .data(|d| d.get_temp::<usize>(key.with("selected")))
        .unwrap_or(0);
    ui.horizontal_wrapped(|ui| {
        for (at, name) in ["RGB", "Red", "Green", "Blue", "Alpha"]
            .into_iter()
            .enumerate()
        {
            if ui.selectable_label(channel == at, name).clicked() {
                channel = at;
            }
        }
    });
    ui.data_mut(|d| d.insert_temp(key.with("selected"), channel));
    let cached = ui.data(|d| d.get_temp::<(String, u64, usize, egui::TextureHandle)>(key));
    let texture = if let Some((document, generation, selected, texture)) =
        cached.filter(|(document, generation, selected, _)| {
            *document == studio.swap_id && *generation == studio.canvas_gen && *selected == channel
        }) {
        let _ = (document, generation, selected);
        texture
    } else {
        let mut image = (*pixels).clone();
        if channel > 0 {
            for pixel in &mut image.pixels {
                let value = pixel.to_array()[channel - 1];
                *pixel = egui::Color32::from_gray(value);
            }
        }
        let texture = ui
            .ctx()
            .load_texture("channel-preview", image, egui::TextureOptions::LINEAR);
        ui.data_mut(|d| {
            d.insert_temp(
                key,
                (
                    studio.swap_id.clone(),
                    studio.canvas_gen,
                    channel,
                    texture.clone(),
                ),
            )
        });
        texture
    };
    let edge = ui.available_width().min(256.0);
    ui.add(egui::Image::new(&texture).fit_to_exact_size(egui::vec2(edge, edge)));
}

#[derive(Clone, Default)]
struct GlyphPage {
    font: usize,
    query: String,
    page: usize,
    textures: std::collections::HashMap<char, egui::TextureHandle>,
}
fn glyphs(ui: &mut Ui, studio: &mut Studio) {
    let mut run = studio
        .selected_type()
        .unwrap_or_else(|| studio.type_defaults());
    if let Some(font) = studio
        .selected_character_metrics()
        .first()
        .and_then(|s| s.font.clone())
    {
        run.font = font;
    }
    let Some(bytes) = crate::text::font_bytes_for(&run) else {
        return;
    };
    let font = std::sync::Arc::as_ptr(&bytes) as usize;
    let key = egui::Id::new("glyph-page");
    let mut state = ui
        .data_mut(|d| d.remove_temp::<GlyphPage>(key))
        .unwrap_or_default();
    if state.font != font {
        state = GlyphPage {
            font,
            ..Default::default()
        };
    }
    let characters_key = egui::Id::new(("font-codepoints", font));
    let characters = ui
        .data(|d| d.get_temp::<std::sync::Arc<Vec<char>>>(characters_key))
        .unwrap_or_else(|| {
            let mut values = std::collections::BTreeSet::new();
            if let Ok(face) = rustybuzz::ttf_parser::Face::parse(&bytes, 0) {
                if let Some(cmap) = face.tables().cmap {
                    for subtable in cmap.subtables {
                        if subtable.is_unicode() {
                            subtable.codepoints(|point| {
                                if let Some(c) = char::from_u32(point) {
                                    if !c.is_control() && face.glyph_index(c).is_some() {
                                        values.insert(c);
                                    }
                                }
                            });
                        }
                    }
                }
            }
            let values = std::sync::Arc::new(values.into_iter().collect::<Vec<_>>());
            ui.data_mut(|d| d.insert_temp(characters_key, values.clone()));
            values
        });
    if ui
        .add(
            egui::TextEdit::singleline(&mut state.query)
                .hint_text("Glyph or U+ codepoint")
                .desired_width(ui.available_width()),
        )
        .changed()
    {
        state.page = 0;
        state.textures.clear();
    }
    let query = state.query.trim();
    let filtered: Vec<_> = characters
        .iter()
        .copied()
        .filter(|c| {
            query.is_empty()
                || query.contains(*c)
                || format!("U+{:04X}", *c as u32).contains(&query.to_uppercase())
        })
        .collect();
    let count = filtered.len().div_ceil(96).max(1);
    state.page = state.page.min(count - 1);
    ui.horizontal(|ui| {
        ui.add_enabled_ui(state.page > 0, |ui| {
            if ui
                .small_button("‹")
                .on_hover_text("Previous glyphs")
                .clicked()
            {
                state.page -= 1;
                state.textures.clear();
            }
        });
        ui.label(format!("{} / {count}", state.page + 1));
        ui.add_enabled_ui(state.page + 1 < count, |ui| {
            if ui.small_button("›").on_hover_text("Next glyphs").clicked() {
                state.page += 1;
                state.textures.clear();
            }
        });
    });
    let columns = (ui.available_width() / 34.0).floor().max(1.0) as usize;
    let mut choose = None;
    egui::Grid::new("glyph-grid")
        .num_columns(columns)
        .show(ui, |ui| {
            for (index, c) in filtered
                .into_iter()
                .skip(state.page * 96)
                .take(96)
                .enumerate()
            {
                let texture = state.textures.entry(c).or_insert_with(|| {
                    let preview = crate::geom::TypeRun {
                        font: run.font.clone(),
                        content: c.to_string(),
                        px: 30.0,
                        ..Default::default()
                    };
                    let mut pm = tiny_skia::Pixmap::new(32, 40).unwrap();
                    if let Some(path) = crate::text::outline_path(&preview) {
                        let bounds = path.bounds();
                        let scale =
                            (26.0 / bounds.width().max(1.0)).min(34.0 / bounds.height().max(1.0));
                        let tx = 16.0 - (bounds.left() + bounds.width() * 0.5) * scale;
                        let ty = 20.0 - (bounds.top() + bounds.height() * 0.5) * scale;
                        let mut paint = tiny_skia::Paint::default();
                        let [r, g, b, a] = super::theme::fg().to_array();
                        paint.set_color_rgba8(r, g, b, a);
                        paint.anti_alias = true;
                        pm.fill_path(
                            &path,
                            &paint,
                            tiny_skia::FillRule::EvenOdd,
                            tiny_skia::Transform::from_row(scale, 0.0, 0.0, scale, tx, ty),
                            None,
                        );
                    }
                    ui.ctx().load_texture(
                        format!("glyph-{font}-{}", c as u32),
                        egui::ColorImage::from_rgba_premultiplied([32, 40], pm.data()),
                        egui::TextureOptions::LINEAR,
                    )
                });
                let response = ui
                    .add(egui::Button::image(
                        egui::Image::new(&*texture).fit_to_exact_size(egui::vec2(28.0, 35.0)),
                    ))
                    .on_hover_text(format!("{c} · U+{:04X}", c as u32));
                response.widget_info(|| {
                    egui::WidgetInfo::labeled(
                        egui::WidgetType::Button,
                        response.enabled(),
                        format!("Insert {c}"),
                    )
                });
                if response.clicked() {
                    choose = Some(c);
                }
                if (index + 1) % columns == 0 {
                    ui.end_row();
                }
            }
        });
    ui.data_mut(|d| d.insert_temp(key, state));
    if let Some(c) = choose {
        if studio.type_edit.is_none() {
            if let Some(hit) = studio.primary() {
                if let Some(shape) = studio.doc.find_shape(hit.0, hit.1) {
                    let point = shape.world_bbox().max;
                    studio.begin_type_edit(hit, point);
                }
            }
        }
        if studio.type_edit.is_some() {
            studio.type_insert(&c.to_string());
            studio.commit_type_edit();
        }
    }
}

fn artboards(ui: &mut Ui, studio: &mut Studio) {
    let boards = studio.doc.artboards.clone();
    for board in boards {
        if ui
            .selectable_label(studio.artboard_sel.contains(&board.id), &board.name)
            .clicked()
        {
            studio.artboard_sel = vec![board.id];
            studio.selection.clear();
            studio.set_tool(Tool::Artboard);
            studio.fit_view();
        }
    }
    ui.horizontal(|ui| {
        if icons::tiny_icon(ui, ph::PLUS, "Wrap selection in an artboard", false) {
            studio.wrap_selection_artboard();
        }
        if icons::tiny_icon(ui, ph::COPY, "Duplicate artboard", false) {
            if let Some(id) = studio.artboard_sel.first().copied() {
                studio.clone_artboard(id);
            }
        }
        if icons::tiny_icon(ui, ph::TRASH, "Delete artboard", false) {
            studio.delete_artboards();
        }
    });
}

fn links(ui: &mut Ui, studio: &mut Studio) {
    let layers: Vec<_> = studio
        .doc
        .layers
        .iter()
        .enumerate()
        .filter(|(_, l)| l.psd_smart.is_some() || l.source_attachment.is_some())
        .map(|(i, l)| (i, l.name.clone()))
        .collect();
    for (index, name) in layers {
        ui.collapsing(name, |ui| super::psd_smart::inspector(ui, studio, index));
    }
    for source in &studio.doc.sources {
        ui.label(&source.name)
            .on_hover_text(format!("{} · original retained", source.id));
    }
}

fn info(ui: &mut Ui, studio: &mut Studio) {
    egui::Grid::new("document-info").show(ui, |ui| {
        for (label, value) in [
            ("Document", studio.doc.name.clone()),
            (
                "Size",
                format!("{} × {} px", studio.doc.width, studio.doc.height),
            ),
            ("Resolution", format!("{} dpi", studio.doc.dpi)),
            ("Layers", studio.doc.layers.len().to_string()),
            (
                "Objects",
                studio
                    .doc
                    .layers
                    .iter()
                    .map(|l| l.kind.shapes().map_or(0, |s| s.len()))
                    .sum::<usize>()
                    .to_string(),
            ),
        ] {
            ui.label(label);
            ui.label(value);
            ui.end_row();
        }
    });
    super::document_color::menu(ui, studio);
}

fn histogram(ui: &mut Ui, studio: &mut Studio) {
    if studio.persona == Persona::Photo {
        super::photo::draw_hist(ui, &studio.photo.hists);
        return;
    }
    let _ = super::pane_previews::image(ui, studio, None, None);
    let Some(image) = super::pane_previews::pixels(ui.ctx(), studio) else {
        ui.spinner();
        return;
    };
    let mut hist = [0u32; 256];
    for pixel in &image.pixels {
        let [r, g, b, _] = pixel.to_array();
        let value = (u32::from(r) * 54 + u32::from(g) * 183 + u32::from(b) * 19) / 256;
        hist[value as usize] += 1;
    }
    let maximum = hist.iter().copied().max().unwrap_or(1).max(1) as f32;
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), 100.0),
        egui::Sense::hover(),
    );
    response.on_hover_text("Display preview histogram");
    for (i, value) in hist.into_iter().enumerate() {
        let x = rect.left() + i as f32 * rect.width() / 256.0;
        ui.painter().line_segment(
            [
                egui::pos2(x, rect.bottom()),
                egui::pos2(x, rect.bottom() - value as f32 / maximum * rect.height()),
            ],
            egui::Stroke::new(1.0, super::theme::fg_weak()),
        );
    }
}

fn photo_presets(ui: &mut Ui, studio: &mut Studio) {
    for preset in studio.photo.presets.clone() {
        if ui.selectable_label(false, &preset.name).clicked() {
            if let Err(error) = studio.photo.apply_adjustments(&preset) {
                studio.status = error;
            }
        }
    }
    if icons::tiny_icon(ui, ph::PLUS, "Manage Photo presets", false) {
        super::photo::preset_library(ui.ctx(), studio);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn human_qa_registered_panes_draw_without_editing_artwork() {
        let ctx = egui::Context::default();
        let mut studio = Studio::new();
        studio.show_welcome = false;
        let before = crate::project::encode(&studio.doc).unwrap();
        for &(panel, _) in super::super::panel_workspace::PANELS {
            if panel == "layers" {
                continue;
            }
            let _ = super::super::shared_fields_tests::headless(
                &ctx,
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(340.0, 650.0),
                    )),
                    ..Default::default()
                },
                |ui| show(ui, &mut studio, panel),
            );
            assert_eq!(
                before,
                crate::project::encode(&studio.doc).unwrap(),
                "opening {panel} changed artwork"
            );
        }
    }
}
