use super::*;
use crate::document::Cmd;

pub(super) fn show(ui: &mut Ui, studio: &mut Studio) {
    studio.with_field_history(ui, "object-appearance", |ui, studio| {
        show_contents(ui, studio)
    });
}

fn show_contents(ui: &mut Ui, studio: &mut Studio) {
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
                    studio.doc.layer_editable(li) && !s.locked,
                )
            })
        })
        .collect();
    let Some(&(_, _, original_blend, original_opacity, original_fill, original_interior, _)) =
        objects.first()
    else {
        return;
    };
    ui.add_space(8.);
    let mut blend = original_blend;
    let mixed = objects.iter().any(|s| s.2 != blend);
    let mut blend_changed = false;
    ui.horizontal(|ui| {
        ui.label("Object blend");
        ComboBox::from_id_salt("object-blend")
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
    let interior_changed = ui.checkbox(&mut interior, "Blend interior effects as group").on_hover_text("Blend inner effects with the content before applying Object blend. Fill opacity affects content only.").changed();
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
fn percent(ui: &mut Ui, label: &str, value: &mut f32, mixed: bool) -> bool {
    let mut changed = false;
    ui.push_id(label, |ui| {
        ui.horizontal(|ui| {
            ui.label(label);
            let control = eframe::egui::DragValue::new(value)
                .range(0.0..=100.0)
                .suffix("%")
                .speed(0.5);
            changed = crate::app::input_history::changed(ui.add(if mixed {
                control.custom_formatter(|_, _| "Mixed".into())
            } else {
                control
            }));
        })
    });
    changed
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
        let label = labels.iter().find(|(text, _)| text == name).unwrap().1;
        labels.iter().find(|(_, rect)| rect.min.x > label.max.x && (rect.center().y - label.center().y).abs() < 3.).unwrap().1.center()
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
