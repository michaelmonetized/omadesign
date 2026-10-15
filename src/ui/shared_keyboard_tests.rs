use super::{fields::*, welcome::keyboard_tests as keys, workspace::*};
use eframe::egui::{self, Event, Key, Modifiers};

#[test]
fn keyboard_shared_native_ids_survive_conditional_rows_and_keep_owner_boundaries() {
    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    let mut value = 1.234567_f64;
    let mut ids: Option<(i32, Vec<egui::Id>)> = None;
    for (preceding, owner) in [(false, 1), (true, 1), (false, 2), (true, 2)] {
        let mut current = Vec::new();
        keys::render(&ctx, vec![], |ui| {
            if preceding {
                ui.label("Conditional property");
                ui.button("Conditional action").clicked();
            }
            let result = number(
                ui,
                ("property", owner),
                "Stable amount",
                &mut value,
                -10.0..=10.0,
                "",
                None,
                Default::default(),
            );
            current.push(result.response.id);
            native_scope(ui, egui::Id::new(("precise-property", owner)), |ui| {
                current.push(
                    ui.add(egui::DragValue::new(&mut value).speed(0.01).max_decimals(6))
                        .id,
                );
            });
        });
        if let Some((previous_owner, previous)) = &ids {
            if *previous_owner == owner {
                assert_eq!(*previous, current);
            } else {
                assert!(previous.iter().zip(&current).all(|(a, b)| a != b));
            }
        }
        ids = Some((owner, current));
        assert_eq!(value, 1.234567);
    }
}

fn field_frame(
    ctx: &egui::Context,
    value: &mut f32,
    state: FieldState<'_>,
    events: Vec<Event>,
) -> (egui::FullOutput, [bool; 4]) {
    let mut flags = [false; 4];
    let output = keys::render(ctx, events, |ui| {
        let result = number(
            ui,
            "amount",
            "Native amount",
            value,
            -100.0..=100.0,
            " px",
            Some(4.),
            state,
        );
        flags[0] |= result.changed;
        flags[1] |= result.committed;
        flags[2] |= result.cancelled;
        flags[3] = number_is_valid(&result);
        let next = ui.button("Next field");
        super::welcome::focus_control(ui, &next);
    });
    (output, flags)
}

#[test]
fn keyboard_shared_number_rejects_native_drafts_and_retains_names_focus_and_mixed_state() {
    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    super::theme::apply(&ctx);
    let mut value = 4.;
    keys::tab_to(&ctx, "Native amount", |events| {
        field_frame(&ctx, &mut value, Default::default(), events).0
    });
    keys::replace("letters", |events| {
        field_frame(&ctx, &mut value, Default::default(), events).0
    });
    let (output, flags) = field_frame(&ctx, &mut value, Default::default(), vec![]);
    keys::associated_error(&output, "Native amount");
    keys::focused(&ctx, &output, "Native amount");
    assert_eq!(value, 4.);
    assert!(!flags[0] && !flags[1]);
    assert!(!flags[3]);
    let error = keys::named(&output, "Native amount")
        .1
        .error_message()
        .unwrap();
    assert_eq!(
        output
            .platform_output
            .accesskit_update
            .as_ref()
            .unwrap()
            .nodes
            .iter()
            .filter(|(id, _)| *id == error)
            .count(),
        1
    );
    let output = keys::replace(" − 1 2 ", |events| {
        field_frame(&ctx, &mut value, Default::default(), events).0
    });
    assert_eq!(value, -12.);
    assert_eq!(keys::named(&output, "Native amount").1.invalid(), None);
    let (_, flags) = field_frame(&ctx, &mut value, Default::default(), vec![]);
    assert!(flags[3]);
    let (output, flags) = field_frame(
        &ctx,
        &mut value,
        Default::default(),
        keys::key(Key::ArrowUp, Modifiers::NONE),
    );
    assert!(flags[0]);
    keys::focused(&ctx, &output, "Native amount");
    let (output, _) = field_frame(&ctx, &mut value, Default::default(), vec![]);
    keys::focused(&ctx, &output, "Native amount");
    field_frame(
        &ctx,
        &mut value,
        Default::default(),
        keys::key(Key::Enter, Modifiers::NONE),
    );
    let mixed = FieldState {
        mixed: true,
        ..Default::default()
    };
    let (output, flags) = field_frame(&ctx, &mut value, mixed, vec![]);
    assert_eq!(
        keys::named(&output, "Native amount").1.state_description(),
        Some("Mixed")
    );
    assert_eq!(
        keys::named(&output, "Native amount").1.value(),
        Some("Mixed")
    );
    assert!(!flags[0]);
    keys::tab_to(&ctx, "Native amount", |events| {
        field_frame(&ctx, &mut value, mixed, events).0
    });
    field_frame(
        &ctx,
        &mut value,
        mixed,
        keys::key(Key::Tab, Modifiers::NONE),
    );
    let (output, flags) = field_frame(&ctx, &mut value, mixed, vec![]);
    assert_eq!(keys::named(&output, "Native amount").1.invalid(), None);
    assert!(!flags[0]);
}

#[test]
fn keyboard_shared_integer_rejects_nonfinite_native_actions_before_conversion() {
    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    let mut value = 2_u32;
    let mut accepted = false;
    let output = keys::render(&ctx, vec![], |ui| {
        number(
            ui,
            "count",
            "Native count",
            &mut value,
            1..=8,
            "",
            None,
            Default::default(),
        );
    });
    let id = keys::named(&output, "Native count").0;
    for invalid in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let output = keys::render(
            &ctx,
            keys::action(
                id,
                egui::accesskit::Action::SetValue,
                Some(egui::accesskit::ActionData::NumericValue(invalid)),
            ),
            |ui| {
                accepted |= number(
                    ui,
                    "count",
                    "Native count",
                    &mut value,
                    1..=8,
                    "",
                    None,
                    Default::default(),
                )
                .changed;
            },
        );
        assert_eq!(value, 2);
        assert!(!accepted);
        keys::associated_error(&output, "Native count");
    }
    keys::render(
        &ctx,
        keys::action(
            id,
            egui::accesskit::Action::SetValue,
            Some(egui::accesskit::ActionData::NumericValue(5.)),
        ),
        |ui| {
            accepted |= number(
                ui,
                "count",
                "Native count",
                &mut value,
                1..=8,
                "",
                None,
                Default::default(),
            )
            .changed;
        },
    );
    assert_eq!(value, 5);
    assert!(accepted);
    accepted = false;
    let output = keys::render(
        &ctx,
        keys::action(
            id,
            egui::accesskit::Action::SetValue,
            Some(egui::accesskit::ActionData::NumericValue(f64::NAN)),
        ),
        |ui| {
            accepted |= number(
                ui,
                "count",
                "Native count",
                &mut value,
                1..=8,
                "",
                None,
                FieldState {
                    locked: true,
                    ..Default::default()
                },
            )
            .changed;
        },
    );
    assert_eq!(value, 5);
    assert!(!accepted);
    assert!(keys::named(&output, "Native count").1.is_disabled());
}

const PANELS: &[(&str, &str)] = &[("properties", "Properties"), ("layers", "Layers")];

fn workspace_frame(
    ctx: &egui::Context,
    layout: &mut Workspace,
    events: Vec<Event>,
) -> egui::FullOutput {
    keys::render(ctx, events, |ui| {
        layout.show(ui, "native-test-workspace", PANELS, |ui, panel| {
            ui.label(format!("{panel} body"));
        });
    })
}

fn window(output: &egui::FullOutput) -> (egui::accesskit::NodeId, egui::Rect) {
    let (id, node) = output
        .platform_output
        .accesskit_update
        .as_ref()
        .unwrap()
        .nodes
        .iter()
        .find(|(_, node)| {
            node.role() == egui::accesskit::Role::Window && node.label() == Some("Properties")
        })
        .expect("native floating window");
    let rect = node.bounds().unwrap();
    (
        *id,
        egui::Rect::from_min_max(
            egui::pos2(rect.x0 as f32, rect.y0 as f32),
            egui::pos2(rect.x1 as f32, rect.y1 as f32),
        ),
    )
}

fn painted_window(output: &egui::FullOutput, layout: &Workspace) -> egui::Rect {
    let saved = layout
        .groups
        .iter()
        .find(|group| {
            group.place == Place::Floating && group.panels.contains(&"properties".to_string())
        })
        .unwrap();
    let actual = egui::Rect::from_min_size(
        egui::pos2(saved.rect[0], saved.rect[1]),
        egui::vec2(saved.rect[2], saved.rect[3]),
    );
    fn matches(shape: &egui::Shape, rect: egui::Rect) -> bool {
        match shape {
            egui::Shape::Rect(shape) => {
                shape.fill.a() > 0
                    && (shape.rect.min - rect.min).length() <= 1.
                    && (shape.rect.size() - rect.size()).length() <= 1.
            }
            egui::Shape::Vec(shapes) => shapes.iter().any(|shape| matches(shape, rect)),
            _ => false,
        }
    }
    assert!(
        output
            .shapes
            .iter()
            .any(|shape| shape.clip_rect.expand(1.).contains_rect(actual)
                && matches(&shape.shape, actual)),
        "native frame must paint its returned footprint {actual:?}"
    );
    actual
}

#[test]
fn keyboard_shared_floating_size_renders_once_then_allows_native_resize_and_reopen() {
    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    let mut layout = Workspace::preset(&[("properties", Place::Floating), ("layers", Place::Left)]);
    let mut output = workspace_frame(&ctx, &mut layout, vec![]);
    for _ in 0..3 {
        output = workspace_frame(&ctx, &mut layout, vec![]);
    }
    let original_id = window(&output).0;
    assert!(!layout.request_floating_size(
        &ctx,
        "native-test-workspace",
        "layers",
        egui::vec2(480., 300.)
    ));
    assert!(!layout.request_floating_size(
        &ctx,
        "native-test-workspace",
        "missing",
        egui::vec2(480., 300.)
    ));
    assert!(!layout.request_floating_size(
        &ctx,
        "native-test-workspace",
        "properties",
        egui::vec2(f32::NAN, 300.)
    ));
    assert!(layout.request_floating_size(
        &ctx,
        "native-test-workspace",
        "properties",
        egui::vec2(480., 300.)
    ));
    let output = workspace_frame(&ctx, &mut layout, vec![]);
    let (id, _) = window(&output);
    let requested = painted_window(&output, &layout);
    assert_eq!(id, original_id);
    assert!((requested.width() - 480.).abs() <= 1., "{requested:?}");
    assert!((requested.height() - 300.).abs() <= 1., "{requested:?}");
    let output = workspace_frame(&ctx, &mut layout, vec![]);
    assert_eq!(painted_window(&output, &layout), requested);
    assert_eq!(
        window(&output).1,
        requested,
        "native window bounds follow the rendered footprint"
    );
    let start = requested.right_bottom() - egui::vec2(2., 2.);
    let end = start + egui::vec2(48., 36.);
    workspace_frame(
        &ctx,
        &mut layout,
        vec![
            Event::PointerMoved(start),
            Event::PointerButton {
                pos: start,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: Modifiers::NONE,
            },
        ],
    );
    workspace_frame(&ctx, &mut layout, vec![Event::PointerMoved(end)]);
    let output = workspace_frame(
        &ctx,
        &mut layout,
        vec![Event::PointerButton {
            pos: end,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: Modifiers::NONE,
        }],
    );
    let resized = painted_window(&output, &layout);
    assert!(resized.width() > requested.width() + 20., "{resized:?}");
    assert!(resized.height() > requested.height() + 10., "{resized:?}");
    let mut book = Book::default();
    book.active = "Design".into();
    book.tasks.insert("Design".into(), layout);
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(".artifacts")
        .join(format!(
            "keyboard-workspace-{}-{}",
            std::process::id(),
            crate::document::next_id()
        ));
    let path = root.join("workspaces.json");
    book.write(&path).unwrap();
    let mut reopened = Book::read(&path).unwrap();
    std::fs::remove_dir_all(&root).unwrap();
    assert_eq!(book, reopened);
    let fresh = egui::Context::default();
    fresh.enable_accesskit();
    let layout = reopened.tasks.get_mut("Design").unwrap();
    let mut output = workspace_frame(&fresh, layout, vec![]);
    for _ in 0..3 {
        output = workspace_frame(&fresh, layout, vec![]);
    }
    let actual = painted_window(&output, layout);
    assert!((actual.width() - resized.width()).abs() <= 1.);
    assert!((actual.height() - resized.height()).abs() <= 1.);
}

fn focus_frame(
    ctx: &egui::Context,
    layout: &mut Workspace,
    modal: &mut bool,
    events: Vec<Event>,
) -> egui::FullOutput {
    keys::render(ctx, events, |ui| {
        let response = ui.button("Change panel layout");
        super::welcome::focus_control(ui, &response);
        if response.clicked() {
            *modal = true;
        }
        layout.show(ui, "native-focus-workspace", PANELS, |ui, panel| {
            ui.label(panel);
        });
        if *modal {
            egui::Modal::new(egui::Id::new("native-panel-return-dialog")).show(ctx, |ui| {
                for apply in [true, false] {
                    let response = ui.button(if apply { "Apply panel" } else { "Cancel panel" });
                    super::welcome::focus_control(ui, &response);
                    if response.clicked() {
                        if apply {
                            layout.move_panel("properties", Place::Bottom);
                        }
                        assert!(layout.request_focus(ctx, "native-focus-workspace", "properties"));
                        *modal = false;
                    }
                }
            });
        }
    })
}

#[test]
fn keyboard_shared_repeated_panel_focus_returns_to_native_tab_after_apply_and_cancel() {
    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    let mut layout = Workspace::preset(&[("properties", Place::Right), ("layers", Place::Left)]);
    let mut modal = false;
    assert!(layout.request_focus(&ctx, "native-focus-workspace", "properties"));
    let mut output = focus_frame(&ctx, &mut layout, &mut modal, vec![]);
    for _ in 0..3 {
        output = focus_frame(&ctx, &mut layout, &mut modal, vec![]);
    }
    keys::focused(&ctx, &output, "Properties");
    let properties = keys::named(&output, "Properties").0;
    for button in ["Apply panel", "Cancel panel", "Cancel panel"] {
        keys::tab_to(&ctx, "Change panel layout", |events| {
            focus_frame(&ctx, &mut layout, &mut modal, events)
        });
        focus_frame(
            &ctx,
            &mut layout,
            &mut modal,
            keys::key(Key::Enter, Modifiers::NONE),
        );
        assert!(modal);
        keys::tab_to(&ctx, button, |events| {
            focus_frame(&ctx, &mut layout, &mut modal, events)
        });
        focus_frame(
            &ctx,
            &mut layout,
            &mut modal,
            keys::key(Key::Enter, Modifiers::NONE),
        );
        assert!(!modal);
        for _ in 0..3 {
            output = focus_frame(&ctx, &mut layout, &mut modal, vec![]);
        }
        keys::focused(&ctx, &output, "Properties");
        assert_eq!(keys::named(&output, "Properties").0, properties);
        assert_eq!(layout.focus.as_deref(), Some("properties"));
    }
    layout.close("properties");
    let before = layout.clone();
    assert!(!layout.request_focus(&ctx, "native-focus-workspace", "properties"));
    assert!(!layout.request_focus(&ctx, "native-focus-workspace", "missing"));
    assert_eq!(layout, before);
}
