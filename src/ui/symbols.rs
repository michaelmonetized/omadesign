//! A symbol library uses the same editable definitions and instances as Layout components.
use super::{
    fields,
    icons::{self, ph},
    welcome,
};
use crate::{app::Studio, geom::Pt, layout_components as components};
use eframe::egui::{self, Id, Ui};

#[derive(Clone, Default)]
struct State {
    selected: Option<u64>,
    query: String,
    name: String,
    original: String,
}
#[derive(Clone)]
struct Drag {
    document: String,
    revision: u64,
    main: u64,
}

fn icon(ui: &mut Ui, id: Id, glyph: &str, label: &str, enabled: bool) -> bool {
    let response = fields::native_scope(ui, id, |ui| {
        ui.add_enabled(
            enabled,
            egui::Button::new(egui::RichText::new(glyph).font(icons::font(16.))).frame(false),
        )
    })
    .inner;
    welcome::name_control(ui, &response, id.with("label"), label);
    response.on_hover_text(label).clicked()
}
fn centered(studio: &Studio, main: u64, point: Pt) -> Pt {
    let size = components::definitions(&studio.doc)
        .into_iter()
        .find(|(_, id, _, _)| *id == main)
        .and_then(|(layer, id, _, _)| studio.doc.find_shape(layer, id))
        .map_or(Pt::ZERO, |s| s.geom.bbox().size());
    point - size * 0.5
}

/// Draw source-bound symbol thumbnails and ordinary component commands.
/// Args: pane and current editor. Returns nothing; every artwork mutation remains undoable.
pub(super) fn show(ui: &mut Ui, studio: &mut Studio) {
    let owner = Id::new(("symbol-library", &studio.swap_id));
    let mut state = ui.data(|d| d.get_temp::<State>(owner)).unwrap_or_default();
    let definitions = components::definitions(&studio.doc);
    if state
        .selected
        .is_some_and(|id| !definitions.iter().any(|p| p.1 == id))
    {
        state.selected = None;
        state.name.clear();
        state.original.clear();
    }
    let query = ui.add(
        egui::TextEdit::singleline(&mut state.query)
            .id(owner.with("query"))
            .hint_text("Find symbols")
            .desired_width(ui.available_width()),
    );
    welcome::name_control(ui, &query, owner.with("query-label"), "Find symbols");
    let mut place = None;
    for (layer, id, name, _) in &definitions {
        if !name.to_lowercase().contains(&state.query.to_lowercase()) {
            continue;
        }
        let response = fields::native_scope(ui, owner.with(id), |ui| {
            ui.allocate_response(
                egui::vec2(ui.available_width(), 42.),
                egui::Sense::click_and_drag(),
            )
        })
        .inner;
        let selected = state.selected == Some(*id);
        response.widget_info(|| {
            egui::WidgetInfo::selected(
                egui::WidgetType::Button,
                ui.is_enabled(),
                selected,
                format!("Symbol {name}"),
            )
        });
        welcome::name_control(
            ui,
            &response,
            owner.with((id, "label")),
            &format!("Symbol {name}"),
        );
        if selected || response.hovered() {
            ui.painter().rect_filled(
                response.rect,
                3.,
                if selected {
                    ui.visuals().selection.bg_fill
                } else {
                    ui.visuals().widgets.hovered.weak_bg_fill
                },
            );
        }
        let thumb =
            egui::Rect::from_min_size(response.rect.min + egui::vec2(3., 3.), egui::vec2(36., 36.));
        if ui.is_rect_visible(response.rect)
            && let Some(texture) = super::pane_previews::image(ui, studio, Some(*layer), Some(*id))
        {
            ui.painter().image(
                texture.id(),
                thumb,
                egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1., 1.)),
                egui::Color32::WHITE,
            );
        }
        ui.painter()
            .with_clip_rect(response.rect.intersect(ui.clip_rect()))
            .text(
                response.rect.left_center() + egui::vec2(46., 0.),
                egui::Align2::LEFT_CENTER,
                name,
                egui::FontId::proportional(12.),
                ui.visuals().text_color(),
            );
        response.dnd_set_drag_payload(Drag {
            document: studio.swap_id.clone(),
            revision: studio.canvas_gen,
            main: *id,
        });
        if response.clicked() {
            state.selected = Some(*id);
            state.name = name.clone();
            state.original = name.clone();
        }
        if response.double_clicked() {
            place = Some(*id);
        }
    }
    ui.horizontal_wrapped(|ui| {
        if icon(
            ui,
            owner.with("new"),
            ph::PLUS,
            "Create symbol from selected artwork",
            !studio.selection.is_empty(),
        ) {
            if studio.create_vector_symbol("") {
                state.selected = studio.primary().map(|p| p.1);
                state.name = studio
                    .primary()
                    .and_then(|(l, id)| studio.doc.find_shape(l, id))
                    .map_or(String::new(), |s| s.name.clone());
                state.original = state.name.clone();
            }
        }
        if icon(
            ui,
            owner.with("place"),
            ph::COPY,
            "Place selected symbol instance",
            state.selected.is_some(),
        ) {
            place = state.selected;
        }
        if icon(
            ui,
            owner.with("edit"),
            ph::PENCIL,
            "Select symbol definition for editing",
            state.selected.is_some(),
        ) && let Some(id) = state.selected
            && let Some((layer, _, _, _)) = definitions.iter().find(|p| p.1 == id)
        {
            studio.enter_group_item((*layer, id));
            studio.zoom_to_objects(true);
        }
        if icon(
            ui,
            owner.with("duplicate"),
            ph::STACK,
            "Duplicate selected symbol independently",
            state.selected.is_some(),
        ) && let Some(main) = state.selected
            && let Some(id) = studio.duplicate_vector_symbol(main)
        {
            state.selected = Some(id);
            state.name = studio
                .primary()
                .and_then(|(li, id)| studio.doc.find_shape(li, id))
                .map_or(String::new(), |s| s.name.clone());
            state.original = state.name.clone();
        }
        if icon(
            ui,
            owner.with("select-instances"),
            ph::SELECTION,
            "Select all editable symbol instances",
            state.selected.is_some(),
        ) && let Some(main) = state.selected
        {
            studio.select_vector_symbol_instances(main);
        }
        let instance = studio
            .selection
            .iter()
            .any(|&(layer, id)| components::instance_ancestor(&studio.doc, layer, id).is_some());
        if icon(
            ui,
            owner.with("reset"),
            ph::ARROW_CLOCKWISE,
            "Reset selected symbol instance overrides",
            instance,
        ) {
            studio.reset_vector_symbol_instances();
        }
        if icon(
            ui,
            owner.with("break"),
            ph::VECTOR_TWO,
            "Break selected instance links",
            instance,
        ) {
            studio.break_vector_symbol_link();
        }
        if icon(
            ui,
            owner.with("delete"),
            ph::TRASH,
            "Remove symbol definition and retain instance artwork",
            state.selected.is_some(),
        ) && let Some(id) = state.selected
        {
            studio.remove_vector_symbol(id);
        }
    });
    if let Some(main) = place {
        let point = studio
            .canvas_rect
            .map(|rect| {
                studio.view.pointer_to_world(
                    Pt::new(rect.min.x, rect.min.y),
                    Pt::new(rect.center().x, rect.center().y),
                )
            })
            .unwrap_or(Pt::new(studio.doc.width * 0.5, studio.doc.height * 0.5));
        studio.place_vector_symbol(main, centered(studio, main, point));
    }
    if let Some(id) = state.selected
        && let Some((_, _, name, _)) = definitions.iter().find(|p| p.1 == id)
    {
        if *name != state.original {
            state.original = name.clone();
            state.name = name.clone();
        }
        ui.horizontal(|ui| {
            let response = ui.add(
                egui::TextEdit::singleline(&mut state.name)
                    .id(owner.with((id, "name")))
                    .desired_width((ui.available_width() - 32.).max(28.)),
            );
            welcome::name_control(ui, &response, owner.with((id, "name-label")), "Symbol name");
            if icon(
                ui,
                owner.with((id, "rename")),
                ph::PENCIL,
                "Rename symbol",
                state.name.trim() != name,
            ) || (response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)))
            {
                if studio.rename_vector_symbol(id, &state.original, &state.name) {
                    state.original = state.name.trim().into();
                    state.name = state.original.clone();
                }
            }
        });
    }
    ui.data_mut(|d| d.insert_temp(owner, state));
}

/// Place only the actual dragged definition in its original document and revision.
/// Args: canvas UI, current editor and real canvas response. Returns whether a symbol drag owns input.
pub(super) fn canvas_drop(ui: &Ui, studio: &mut Studio, response: &egui::Response) -> bool {
    let dragging = egui::DragAndDrop::has_payload_of_type::<Drag>(ui.ctx());
    if dragging
        && let Some(payload) = response.dnd_release_payload::<Drag>()
        && let Some(point) = ui.input(|i| i.pointer.interact_pos())
    {
        if payload.document == studio.swap_id && payload.revision == studio.canvas_gen {
            let at = studio.view.pointer_to_world(
                Pt::new(response.rect.min.x, response.rect.min.y),
                Pt::new(point.x, point.y),
            );
            if studio.place_vector_symbol(payload.main, centered(studio, payload.main, at))
                && response.enabled()
                && ui.ctx().memory(|m| m.allows_interaction(response.layer_id))
            {
                response.request_focus();
            }
        } else {
            studio.status = "The dragged symbol changed. Drag the current definition again".into();
        }
    }
    if dragging && response.contains_pointer() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Copy);
    }
    dragging
}

#[cfg(test)]
mod tests {
    use super::super::welcome::keyboard_tests as keys;
    use super::*;
    use egui::{Event, Key, Modifiers};
    #[test]
    fn craft_symbol_instance_commands_native_duplicate_select_and_break() {
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        super::super::theme::apply(&ctx);
        let mut studio = crate::app::symbols::tests::fixture();
        let mut frame = |events| {
            keys::render(&ctx, events, |ui| {
                ui.set_max_width(256.);
                show(ui, &mut studio);
            })
        };
        frame(vec![]);
        frame(vec![]);
        for label in [
            "Create symbol from selected artwork",
            "Duplicate selected symbol independently",
            "Place selected symbol instance",
            "Place selected symbol instance",
            "Select all editable symbol instances",
        ] {
            keys::tab_to(&ctx, label, &mut frame);
            frame(keys::key(Key::Space, Modifiers::NONE));
            frame(vec![]);
        }
        drop(frame);
        assert_eq!(components::definitions(&studio.doc).len(), 2);
        assert_eq!(studio.selection.len(), 2);
        let roots = studio.selection.clone();
        for &(li, root) in &roots {
            assert!(matches!(
                studio.doc.find_shape(li, root).unwrap().layout.component,
                Some(components::ComponentBinding::Instance { .. })
            ));
        }
        let before = crate::project::encode(&studio.doc).unwrap();
        let history = studio.history.len();
        let mut frame = |events| {
            keys::render(&ctx, events, |ui| {
                ui.set_max_width(256.);
                show(ui, &mut studio);
            })
        };
        frame(vec![]);
        keys::tab_to(&ctx, "Break selected instance links", &mut frame);
        frame(keys::key(Key::Space, Modifiers::NONE));
        frame(vec![]);
        drop(frame);
        assert_eq!(studio.history.len(), history + 1);
        assert!(roots.iter().all(|&(li, root)| {
            studio
                .doc
                .find_shape(li, root)
                .unwrap()
                .layout
                .component
                .is_none()
        }));
        let saved = crate::project::encode(&studio.doc).unwrap();
        assert_eq!(
            crate::project::encode(&crate::project::decode(&saved).unwrap()).unwrap(),
            saved
        );
        studio.undo();
        assert_eq!(crate::project::encode(&studio.doc).unwrap(), before);
    }
    #[test]
    fn craft_symbol_native_create_place_rename_and_reopen() {
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        super::super::theme::apply(&ctx);
        let mut studio = crate::app::symbols::tests::fixture();
        let mut frame = |events| {
            keys::render(&ctx, events, |ui| {
                ui.set_max_width(304.);
                show(ui, &mut studio);
            })
        };
        frame(vec![]);
        frame(vec![]);
        keys::tab_to(&ctx, "Create symbol from selected artwork", &mut frame);
        frame(keys::key(Key::Space, Modifiers::NONE));
        frame(vec![]);
        keys::tab_to(&ctx, "Place selected symbol instance", &mut frame);
        frame(keys::key(Key::Space, Modifiers::NONE));
        frame(vec![]);
        keys::tab_to(&ctx, "Symbol name", &mut frame);
        frame(keys::key(Key::A, Modifiers::CTRL | Modifiers::COMMAND));
        frame(vec![Event::Paste("Native badge".into())]);
        frame(keys::key(Key::Enter, Modifiers::NONE));
        drop(frame);
        let definitions = components::definitions(&studio.doc);
        assert_eq!(definitions.len(), 1);
        assert_eq!(definitions[0].2, "Native badge");
        assert!(
            studio.doc.layers[0]
                .kind
                .shapes()
                .unwrap()
                .iter()
                .any(|s| matches!(
                    s.layout.component,
                    Some(components::ComponentBinding::Instance { .. })
                ))
        );
        let saved = crate::project::encode(&studio.doc).unwrap();
        assert_eq!(
            crate::project::encode(&crate::project::decode(&saved).unwrap()).unwrap(),
            saved
        );
    }

    #[test]
    fn craft_symbol_actual_row_drag_reaches_canvas_without_another_payload_handler_taking_it() {
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        super::super::theme::apply(&ctx);
        let mut studio = crate::app::symbols::tests::fixture();
        assert!(studio.create_vector_symbol("Badge"));
        let frame = |s: &mut Studio, events| {
            keys::render(&ctx, events, |ui| {
                ui.columns(2, |columns| {
                    show(&mut columns[0], s);
                    super::super::canvas::show(&mut columns[1], s);
                });
            })
        };
        frame(&mut studio, vec![]);
        let output = frame(&mut studio, vec![]);
        let (_, node) = keys::named(&output, "Symbol Badge");
        let bounds = node.bounds().unwrap();
        let start = egui::pos2(
            ((bounds.x0 + bounds.x1) * 0.5) as f32,
            ((bounds.y0 + bounds.y1) * 0.5) as f32,
        );
        let target = studio.canvas_rect.unwrap().center();
        let button = |pos, pressed| Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        };
        frame(
            &mut studio,
            vec![Event::PointerMoved(start), button(start, true)],
        );
        frame(
            &mut studio,
            vec![Event::PointerMoved(start + egui::vec2(14., 0.))],
        );
        frame(&mut studio, vec![Event::PointerMoved(target)]);
        frame(&mut studio, vec![button(target, false)]);
        assert!(
            studio.doc.layers[0]
                .kind
                .shapes()
                .unwrap()
                .iter()
                .any(|s| matches!(
                    s.layout.component,
                    Some(components::ComponentBinding::Instance { .. })
                ))
        );
        assert_eq!(
            studio.history.len(),
            2,
            "creation and the actual drop are separate atomic commands"
        );
        assert_eq!(
            ctx.memory(|m| m.focused()),
            Some(Id::new("studio-canvas")),
            "an accepted canvas drop returns real keyboard ownership to the canvas"
        );
    }
}
