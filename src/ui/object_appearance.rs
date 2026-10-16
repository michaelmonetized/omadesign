use super::*;
use crate::document::Cmd;

pub(super) fn show(ui: &mut Ui, studio: &mut Studio) {
    studio.with_field_history(ui, "object-appearance", |ui, studio| {
        let id = ui.make_persistent_id(("object-appearance", &studio.swap_id, &studio.selection));
        ui.scope_builder(eframe::egui::UiBuilder::new().id(id), |ui| {
            if studio.file_dialog_pending()
                || studio.updates.freezing
                || studio.op.is_some()
                || studio.free_transform.is_some()
                || studio.deformation.is_some()
            {
                ui.disable();
            }
            controls(ui, studio);
        });
    });
}

/// Edit the current appearance through existing document commands.
/// Args: owner-scoped UI and editor. Returns without changing unavailable artwork.
fn controls(ui: &mut Ui, studio: &mut Studio) {
    let editable: std::collections::HashSet<_> = studio.selectable_objects().into_iter().collect();
    let objects: Vec<_> = studio
        .selection
        .iter()
        .filter_map(|&(li, id)| {
            studio.doc.find_shape(li, id).map(|s| {
                (
                    li,
                    id,
                    s.blend,
                    s.opacity,
                    s.fill_opacity,
                    s.blend_interior,
                    editable.contains(&(li, id)),
                )
            })
        })
        .collect();
    let Some(&(_, _, original_blend, original_opacity, original_fill, original_interior, _)) =
        objects.first()
    else {
        return;
    };
    if !objects.iter().any(|object| object.6) {
        ui.disable();
    }
    ui.add_space(8.);
    let mut blend = original_blend;
    let mixed = objects.iter().any(|s| s.2 != blend);
    let mut blend_changed = false;
    ui.horizontal(|ui| {
        let label = ui.label("Object blend");
        let response = ComboBox::from_id_salt("object-blend")
            .selected_text(if mixed { "Mixed" } else { blend.name() })
            .show_ui(ui, |ui| {
                for mode in Blend::ALL {
                    if ui
                        .selectable_label(!mixed && blend == mode, mode.name())
                        .clicked()
                    {
                        blend = mode;
                        blend_changed = true;
                    }
                }
            });
        if blend_changed {
            response.response.request_focus();
        }
        crate::ui::welcome::name_control(ui, &response.response, label.id, "Object blend");
    });
    let mut opacity = original_opacity * 100.;
    let mixed_opacity = objects
        .iter()
        .any(|s| (s.3 - original_opacity).abs() > 0.0001);
    let opacity_changed = percent(ui, "Object opacity", &mut opacity, mixed_opacity);
    let mut fill = original_fill * 100.;
    let fill_changed = percent(
        ui,
        "Fill opacity",
        &mut fill,
        objects.iter().any(|s| s.4 != original_fill),
    );
    let mut interior = original_interior;
    let response = ui.checkbox(&mut interior, "Blend interior effects as group").on_hover_text("Blend inner effects with the content before applying Object blend. Fill opacity affects content only.");
    crate::ui::welcome::focus_control(ui, &response);
    let interior_changed = response.changed();
    if !ui.is_enabled() {
        return;
    }
    let mut commands = vec![];
    for (layer, id, b, o, f, i, editable) in objects {
        if !editable {
            continue;
        }
        if blend_changed && blend != b {
            commands.push(Cmd::SetBlend {
                layer,
                id,
                before: b,
                after: blend,
            });
        }
        if opacity_changed && opacity / 100. != o {
            commands.push(Cmd::SetOpacity {
                layer,
                id,
                before: o,
                after: opacity / 100.,
            });
        }
        if fill_changed && fill / 100. != f {
            commands.push(Cmd::SetFillOpacity {
                layer,
                id: Some(id),
                before: f,
                after: fill / 100.,
            });
        }
        if interior_changed && interior != i {
            commands.push(Cmd::SetBlendInterior {
                layer,
                id: Some(id),
                before: i,
                after: interior,
            });
        }
    }
    if !commands.is_empty() {
        studio.commit(Cmd::Batch(commands));
    }
}
/// Name a canonical appearance percentage and observe accepted edits.
/// Args: UI, property name, value and mixed state. Returns the authoritative change flag.
fn percent(ui: &mut Ui, label: &str, value: &mut f32, mixed: bool) -> bool {
    let result = crate::ui::fields::number(
        ui,
        label,
        label,
        value,
        0.0..=100.0,
        "%",
        None,
        crate::ui::fields::FieldState {
            mixed,
            ..Default::default()
        },
    );
    crate::ui::welcome::focus_control(ui, &result.response);
    ui.ctx().accesskit_node_builder(result.response.id, |node| {
        node.set_label(format!("{label} (%)"))
    });
    crate::app::input_history::record(result.response, result.changed)
}

#[cfg(test)]
mod keyboard_tests {
    use super::*;
    use crate::ui::welcome::keyboard_tests as keys;
    use eframe::egui::{self, Event, Key, Modifiers};

    fn frame(ctx: &egui::Context, studio: &mut Studio, events: Vec<Event>) -> egui::FullOutput {
        keys::render(ctx, events, |ui| {
            if !studio.updates.freezing && !studio.file_dialog_pending() {
                studio.handle_shortcuts(ctx);
            }
            show(ui, studio);
            crate::ui::canvas::show(ui, studio);
            crate::ui::keyboard_authoring::show(ctx, studio);
        })
    }

    #[test]
    fn keyboard_created_artwork_appearance_uses_current_owner_and_rejects_locked_or_busy_edits() {
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        crate::ui::theme::apply(&ctx);
        let mut studio = Studio::new();
        studio.doc = crate::document::Document::new("Keyboard object appearance", 64., 64., 72.);
        studio.show_welcome = false;
        assert!(studio.selection.is_empty());
        frame(&ctx, &mut studio, vec![]);
        frame(&ctx, &mut studio, keys::key(Key::F8, Modifiers::NONE));
        frame(&ctx, &mut studio, keys::key(Key::R, Modifiers::NONE));
        frame(&ctx, &mut studio, keys::key(Key::Enter, Modifiers::NONE));
        frame(&ctx, &mut studio, vec![]);
        frame(
            &ctx,
            &mut studio,
            keys::key(Key::Enter, Modifiers::CTRL | Modifiers::COMMAND),
        );
        frame(&ctx, &mut studio, vec![]);
        let hit = studio.primary().unwrap();
        let output = keys::tab_to(&ctx, "Object opacity (%)", |events| {
            frame(&ctx, &mut studio, events)
        });
        keys::focused(&ctx, &output, "Object opacity (%)");
        let opacity = keys::named(&output, "Object opacity (%)").0;
        keys::replace("25", |events| frame(&ctx, &mut studio, events));
        frame(&ctx, &mut studio, keys::key(Key::Enter, Modifiers::NONE));
        assert_eq!(studio.doc.find_shape(hit.0, hit.1).unwrap().opacity, 0.25);
        let output = keys::tab_to(&ctx, "Fill opacity (%)", |events| {
            frame(&ctx, &mut studio, events)
        });
        assert_ne!(keys::named(&output, "Fill opacity (%)").0, opacity);
        keys::replace("40", |events| frame(&ctx, &mut studio, events));
        frame(&ctx, &mut studio, keys::key(Key::Enter, Modifiers::NONE));
        assert_eq!(
            studio.doc.find_shape(hit.0, hit.1).unwrap().fill_opacity,
            0.4
        );
        studio.updates.freezing = true;
        let history = studio.history.len();
        let output = frame(&ctx, &mut studio, vec![]);
        assert!(keys::named(&output, "Object opacity (%)").1.is_disabled());
        frame(&ctx, &mut studio, vec![Event::Text("90".into())]);
        assert_eq!(studio.history.len(), history);
        studio.updates.freezing = false;
        studio.doc.layers[hit.0].locked = true;
        let output = frame(&ctx, &mut studio, vec![]);
        assert!(keys::named(&output, "Fill opacity (%)").1.is_disabled());
        assert_eq!(studio.history.len(), history);
        assert_eq!(studio.doc.find_shape(hit.0, hit.1).unwrap().opacity, 0.25);
        studio.doc.layers[hit.0].locked = false;
        let output = frame(&ctx, &mut studio, vec![]);
        assert_eq!(keys::named(&output, "Object opacity (%)").0, opacity);
        frame(&ctx, &mut studio, keys::key(Key::F8, Modifiers::NONE));
        frame(
            &ctx,
            &mut studio,
            keys::key(Key::Z, Modifiers::CTRL | Modifiers::COMMAND),
        );
        assert_eq!(
            studio.doc.find_shape(hit.0, hit.1).unwrap().fill_opacity,
            1.
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use eframe::egui::{self, Event, Modifiers, PointerButton, Pos2, Rect};
    fn labels(ctx: &egui::Context, studio: &mut Studio, events: Vec<Event>) -> Vec<(String, Rect)> {
        render(ctx, studio, events, false)
    }
    fn render(
        ctx: &egui::Context,
        studio: &mut Studio,
        events: Vec<Event>,
        layer_controls: bool,
    ) -> Vec<(String, Rect)> {
        ctx.enable_accesskit();
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(300., 650.))),
                events,
                ..Default::default()
            },
            |ui| {
                studio.handle_shortcuts(ui.ctx());
                if layer_controls {
                    layers_studio(ui, studio)
                } else {
                    show(ui, studio)
                }
            },
        );
        fn visit(shape: &egui::Shape, out: &mut Vec<(String, Rect)>) {
            match shape {
                egui::Shape::Text(t) => out.push((
                    t.galley.text().into(),
                    t.galley.rect.translate(t.pos.to_vec2()),
                )),
                egui::Shape::Vec(ss) => {
                    for s in ss {
                        visit(s, out)
                    }
                }
                _ => {}
            }
        }
        output.textures_delta.clear();
        let mut found = vec![];
        if let Some(tree) = &output.platform_output.accesskit_update {
            for (_, node) in &tree.nodes {
                if let (Some(label), Some(bounds)) = (node.label(), node.bounds()) {
                    if node.supports_action(egui::accesskit::Action::SetValue) {
                        found.push((
                            format!("native:{label}"),
                            Rect::from_min_max(
                                egui::pos2(bounds.x0 as f32, bounds.y0 as f32),
                                egui::pos2(bounds.x1 as f32, bounds.y1 as f32),
                            ),
                        ));
                    }
                }
            }
        }
        for s in output.shapes {
            visit(&s.shape, &mut found);
        }
        found
    }
    fn fixture() -> Studio {
        let mut s = Studio::new();
        let mut l = crate::document::Layer::vector("Separate layer controls");
        for n in 0..3 {
            let mut shape = crate::document::Shape::new(
                Geom::Rect {
                    origin: crate::geom::Pt::ZERO,
                    size: crate::geom::Pt::splat(50.),
                    radius: 0.,
                },
                Default::default(),
            );
            shape.opacity = if n == 0 { 0.2 } else { 0.8 };
            shape.blend = if n == 0 {
                Blend::Multiply
            } else {
                Blend::Screen
            };
            shape.locked = n == 2;
            s.selection.push((0, shape.id));
            l.kind.shapes_mut().unwrap().push(shape);
        }
        l.blend = Blend::Color;
        s.doc.layers = vec![l];
        s.active_layer = Some(0);
        s
    }
    fn value_at(labels: &[(String, Rect)], name: &str) -> Pos2 {
        labels
            .iter()
            .find(|(text, _)| {
                text == &format!("native:{name}") || text == &format!("native:{name} (%)")
            })
            .unwrap_or_else(|| panic!("missing actual numeric {name}: {labels:?}"))
            .1
            .center()
    }

    fn pointer(pos: Pos2, pressed: bool) -> Vec<Event> {
        vec![Event::PointerMoved(pos), Event::PointerButton { pos, button: PointerButton::Primary, pressed, modifiers: Modifiers::NONE }]
    }

    #[test]
    fn field_input_object_drags_keep_only_endpoints_and_separate_gestures() {
        let ctx = egui::Context::default();
        crate::ui::theme::apply(&ctx);
        let mut studio = fixture();
        let initial: Vec<_> = studio.doc.layers[0].kind.shapes().unwrap().iter().map(|s| s.opacity).collect();
        let mut finals = Vec::new();
        for _ in 0..2 {
            let pos = value_at(&labels(&ctx, &mut studio, vec![]), "Object opacity");
            labels(&ctx, &mut studio, pointer(pos, true));
            for dx in [10., 20., 30., 40.] {
                labels(&ctx, &mut studio, vec![Event::PointerMoved(pos + egui::vec2(dx, 0.))]);
            }
            labels(&ctx, &mut studio, pointer(pos + egui::vec2(40., 0.), false));
            finals.push(studio.doc.layers[0].kind.shapes().unwrap().iter().map(|s| s.opacity).collect::<Vec<_>>());
        }
        assert_eq!(studio.history.len(), 2);
        assert_ne!(finals[0], finals[1]);
        assert_eq!(finals[1][2], initial[2], "locked object");
        studio.undo();
        assert_eq!(studio.doc.layers[0].kind.shapes().unwrap().iter().map(|s| s.opacity).collect::<Vec<_>>(), finals[0]);
        studio.undo();
        assert_eq!(studio.doc.layers[0].kind.shapes().unwrap().iter().map(|s| s.opacity).collect::<Vec<_>>(), initial);
        studio.redo(); studio.redo();
        assert_eq!(studio.doc.layers[0].kind.shapes().unwrap().iter().map(|s| s.opacity).collect::<Vec<_>>(), finals[1]);
        fn leaves(command: Cmd) -> usize {
            match command { Cmd::Batch(commands) => commands.into_iter().map(leaves).sum(), _ => 1 }
        }
        assert_eq!(leaves(studio.history.clone().undo().unwrap()), 2, "one final command per editable object");
    }

    #[test]
    fn field_input_layer_drags_do_not_merge_with_each_other_or_unrelated_edits() {
        let ctx = egui::Context::default();
        crate::ui::theme::apply(&ctx);
        let mut studio = fixture();
        let mut values = vec![studio.doc.layers[0].opacity];
        for _ in 0..2 {
            let pos = value_at(&render(&ctx, &mut studio, vec![], true), "Layer opacity");
            render(&ctx, &mut studio, pointer(pos, true), true);
            for dx in [-10., -20., -30.] {
                render(&ctx, &mut studio, vec![Event::PointerMoved(pos + egui::vec2(dx, 0.))], true);
            }
            render(&ctx, &mut studio, pointer(pos + egui::vec2(-30., 0.), false), true);
            values.push(studio.doc.layers[0].opacity);
        }
        assert_eq!(studio.history.len(), 2);
        let layer = &studio.doc.layers[0];
        studio.commit(Cmd::SetLayerMeta { index: 0, name: "Independent name".into(), visible: layer.visible, locked: layer.locked, opacity: layer.opacity, blend: layer.blend,
            before: (layer.name.clone(), layer.visible, layer.locked, layer.opacity, layer.blend) });
        assert_eq!(studio.history.len(), 3);
        studio.undo();
        assert_eq!(studio.doc.layers[0].opacity, values[2]);
        studio.undo();
        assert_eq!(studio.doc.layers[0].opacity, values[1]);
        studio.undo();
        assert_eq!(studio.doc.layers[0].opacity, values[0]);
        studio.redo(); studio.redo();
        assert_eq!(studio.doc.layers[0].opacity, values[2]);
    }

    #[test]
    fn field_input_opacity_redo_keeps_final_value_after_first_frame_layout_reconciliation() {
        let ctx = egui::Context::default();
        crate::ui::theme::apply(&ctx);
        let mut studio = fixture();
        let mut frame = crate::document::Shape::new(
            Geom::Rect {
                origin: crate::geom::Pt::splat(100.),
                size: crate::geom::Pt::splat(250.),
                radius: 0.,
            },
            Default::default(),
        );
        frame.layout.frame = true;
        frame.layout.stack = Some(Default::default());
        let frame_id = frame.id;
        for shape in studio.doc.layers[0].kind.shapes_mut().unwrap() {
            shape.layout.parent = Some(frame_id);
        }
        studio.doc.layers[0].kind.shapes_mut().unwrap().push(frame);
        let initial = crate::project::encode(&studio.doc).unwrap();
        let first = studio.selection[0];
        let original_bounds = studio.doc.find_shape(first.0, first.1).unwrap().geom.bbox();
        let at = value_at(&labels(&ctx, &mut studio, vec![]), "Object opacity");
        labels(&ctx, &mut studio, pointer(at, true));
        let mut first_frame_value = None;
        for dx in [10., 20., 30., 40.] {
            labels(
                &ctx,
                &mut studio,
                vec![Event::PointerMoved(at + egui::vec2(dx, 0.))],
            );
            first_frame_value
                .get_or_insert(studio.doc.find_shape(first.0, first.1).unwrap().opacity);
        }
        labels(&ctx, &mut studio, pointer(at + egui::vec2(40., 0.), false));
        let final_value = studio.doc.find_shape(first.0, first.1).unwrap().opacity;
        assert_ne!(Some(final_value), first_frame_value);
        assert_ne!(
            studio.doc.find_shape(first.0, first.1).unwrap().geom.bbox(),
            original_bounds,
            "real layout reconciliation moved the child"
        );
        assert_eq!(studio.history.len(), 1);
        let final_document = crate::project::encode(&studio.doc).unwrap();
        studio.undo();
        assert_eq!(crate::project::encode(&studio.doc).unwrap(), initial);
        studio.redo();
        assert_eq!(
            studio.doc.find_shape(first.0, first.1).unwrap().opacity,
            final_value
        );
        assert_eq!(crate::project::encode(&studio.doc).unwrap(), final_document);
    }

    #[test]
    fn object_appearance_visible_short_text_motion_and_mixed_edits_are_one_undo_step() {
        let ctx = egui::Context::default();
        crate::ui::theme::apply(&ctx);
        let mut s = fixture();
        for persona in [Persona::Design, Persona::Layout, Persona::Motion] {
            s.persona = persona;
            s.tool = Tool::Text;
            let out = labels(&ctx, &mut s, vec![]);
            for label in ["Object blend", "Object opacity", "Fill opacity", "Mixed"] {
                assert!(out.iter().any(|(t, _)| t == label), "{persona:?}: {label}");
            }
        }
        s.persona = Persona::Design;
        let out = labels(&ctx, &mut s, vec![]);
        let combo = out.iter().find(|(t, _)| t == "Mixed").unwrap().1.center();
        for pressed in [true, false] {
            labels(
                &ctx,
                &mut s,
                vec![
                    Event::PointerMoved(combo),
                    Event::PointerButton {
                        pos: combo,
                        button: PointerButton::Primary,
                        pressed,
                        modifiers: Modifiers::NONE,
                    },
                ],
            );
        }
        let out = labels(&ctx, &mut s, vec![]);
        let normal = out.iter().find(|(t, _)| t == "Normal").unwrap().1.center();
        for pressed in [true, false] {
            labels(
                &ctx,
                &mut s,
                vec![
                    Event::PointerMoved(normal),
                    Event::PointerButton {
                        pos: normal,
                        button: PointerButton::Primary,
                        pressed,
                        modifiers: Modifiers::NONE,
                    },
                ],
            );
        }
        let shapes = s.doc.layers[0].kind.shapes().unwrap();
        assert_eq!(shapes[0].blend, Blend::Normal);
        assert_eq!(shapes[1].blend, Blend::Normal);
        assert_eq!(shapes[2].blend, Blend::Screen);
        assert_eq!(s.doc.layers[0].blend, Blend::Color);
        let command = s.history.clone().undo().unwrap();
        assert!(
            matches!(command, Cmd::Batch(commands) if commands.iter().all(|c| matches!(c, Cmd::SetBlend {..})))
        );
        s.undo();
        let shapes = s.doc.layers[0].kind.shapes().unwrap();
        assert_eq!(shapes[0].blend, Blend::Multiply);
        assert_eq!(shapes[1].blend, Blend::Screen);
    }
    #[test]
    fn layer_controls_label_and_issue_layer_metadata_without_touching_object_blends() {
        let ctx = egui::Context::default();
        crate::ui::theme::apply(&ctx);
        let mut s = fixture();
        let initial: Vec<_> = s.doc.layers[0]
            .kind
            .shapes()
            .unwrap()
            .iter()
            .map(|s| s.blend)
            .collect();
        let out = render(&ctx, &mut s, vec![], true);
        for label in ["Layer blend", "Layer opacity"] {
            assert!(out.iter().any(|(s, _)| s == label));
        }
        let pos = out.iter().find(|(s, _)| s == "Color").unwrap().1.center();
        for pressed in [true, false] {
            render(
                &ctx,
                &mut s,
                vec![
                    Event::PointerMoved(pos),
                    Event::PointerButton {
                        pos,
                        button: PointerButton::Primary,
                        pressed,
                        modifiers: Modifiers::NONE,
                    },
                ],
                true,
            );
        }
        let out = render(&ctx, &mut s, vec![], true);
        let pos = out.iter().find(|(s, _)| s == "Normal").unwrap().1.center();
        for pressed in [true, false] {
            render(
                &ctx,
                &mut s,
                vec![
                    Event::PointerMoved(pos),
                    Event::PointerButton {
                        pos,
                        button: PointerButton::Primary,
                        pressed,
                        modifiers: Modifiers::NONE,
                    },
                ],
                true,
            );
        }
        assert_eq!(s.doc.layers[0].blend, Blend::Normal);
        assert_eq!(
            s.doc.layers[0]
                .kind
                .shapes()
                .unwrap()
                .iter()
                .map(|s| s.blend)
                .collect::<Vec<_>>(),
            initial
        );
        assert!(matches!(
            s.history.clone().undo().unwrap(),
            Cmd::SetLayerMeta { .. }
        ));
    }
}
