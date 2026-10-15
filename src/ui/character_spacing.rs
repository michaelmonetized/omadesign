use crate::{
    app::Studio,
    geom::{CharSpan, KernMode, Leading},
};
use eframe::egui::{self, Id, Response, Ui};
fn common<T: PartialEq + Copy>(
    styles: &[CharSpan],
    get: impl Fn(&CharSpan) -> Option<T>,
) -> Option<T> {
    let first = get(styles.first()?);
    styles
        .iter()
        .all(|s| get(s) == first)
        .then_some(first)
        .flatten()
}
fn numeric(
    ui: &mut Ui,
    label: &str,
    value: Option<f32>,
    fallback: f32,
    range: std::ops::RangeInclusive<f32>,
    suffix: &str,
) -> Option<f32> {
    let label_id = ui.label(label).id;
    amount(ui, (label, label_id), value, fallback, range, suffix)
}

/// Read a finite field value.
/// Args: native number text. Returns a finite number, including the native minus and spacing forms.
fn parse_number(text: &str) -> Option<f32> {
    text.chars()
        .filter(|c| !c.is_whitespace())
        .map(|c| if c == '−' { '-' } else { c })
        .collect::<String>()
        .parse::<f32>()
        .ok()
        .filter(|value| value.is_finite())
}

/// Draw a named character amount.
/// Args: UI, purpose and visible label, common value, fallback, range and unit.
/// Returns a valid native widget edit; mixed and invalid text never writes the fallback.
fn amount(
    ui: &mut Ui,
    (purpose, label_id): (&str, Id),
    value: Option<f32>,
    fallback: f32,
    range: std::ops::RangeInclusive<f32>,
    suffix: &str,
) -> Option<f32> {
    let help = match purpose {
        "Tracking" => "Letter spacing (tracking), in thousandths of an em (1/1000 em)",
        "Horizontal scale" => "Horizontal glyph scale, as a percentage of its original width",
        "Vertical scale" => "Vertical glyph scale, as a percentage of its original height",
        "Pair kerning" => "Manual pair kerning, in thousandths of an em (1/1000 em)",
        "Leading amount" => "Line spacing in the selected leading mode",
        _ => purpose,
    };
    let id = ui.make_persistent_id(("character-amount", purpose));
    ui.scope_builder(
        egui::UiBuilder::new()
            .id(id)
            .layout(egui::Layout::top_down(egui::Align::Min)),
        |ui| {
            let before = value.unwrap_or(fallback);
            let mut number = before;
            let input_range = range.clone();
            let response = ui
                .add(
                    egui::DragValue::new(&mut number)
                        .range(range.clone())
                        .clamp_existing_to_range(false)
                        .speed(1.)
                        .suffix(suffix)
                        .custom_formatter(move |number, _| {
                            if value.is_none() {
                                "Mixed".into()
                            } else {
                                format!("{number:.1}")
                            }
                        })
                        .custom_parser(move |text| {
                            parse_number(text)
                                .filter(|value| input_range.contains(value))
                                .map(f64::from)
                        }),
                )
                .labelled_by(label_id)
                .on_hover_text(help);
            let text = ui.data(|data| data.get_temp::<String>(response.id));
            let entered = text
                .as_deref()
                .filter(|text| !(value.is_none() && *text == "Mixed"))
                .map(parse_number);
            let error = if !number.is_finite() || matches!(entered, Some(None)) {
                Some("Enter a finite number".to_owned())
            } else if !range.contains(&number)
                || entered.flatten().is_some_and(|value| !range.contains(&value))
            {
                Some(format!(
                    "Enter a value from {} to {}{}",
                    range.start(),
                    range.end(),
                    suffix
                ))
            } else {
                None
            };
            let changed = response.changed()
                && response.enabled()
                && error.is_none()
                && (number != before || entered.flatten().is_some());
            let error_id = error
                .as_ref()
                .map(|error| ui.colored_label(ui.visuals().error_fg_color, error).id);
            if let Some(error_id) = error_id {
                ui.ctx().accesskit_node_builder(error_id, |node| {
                    node.set_live(egui::accesskit::Live::Polite);
                    node.set_live_atomic();
                });
            }
            response.widget_info(|| {
                let mut info = egui::WidgetInfo::drag_value(response.enabled(), f64::from(number));
                info.label = Some(purpose.into());
                info
            });
            ui.ctx().accesskit_node_builder(response.id, |node| {
                node.set_description(help);
                if value.is_none() && !changed {
                    node.set_state_description("Mixed values");
                    node.clear_numeric_value();
                    if text.as_deref().is_none_or(|text| text == "Mixed") {
                        node.set_value("Mixed");
                    }
                }
                if let Some(error) = &error {
                    node.set_invalid(egui::accesskit::Invalid::True);
                    node.set_description(format!("{help}. {error}"));
                    node.clear_numeric_value();
                    if let Some(error_id) = error_id {
                        node.push_described_by(error_id.accesskit_id());
                        node.set_error_message(error_id.accesskit_id());
                    }
                }
            });
            focus_outline(ui, &response);
            changed.then_some(number)
        },
    )
    .inner
}

/// Name a character mode control.
/// Args: UI, native response, visible label, purpose and selected mode. Returns no value.
fn mode_name(ui: &Ui, response: Response, label_id: Id, purpose: &str, selected: &str) {
    let response = response.labelled_by(label_id);
    response.widget_info(|| {
        let mut info =
            egui::WidgetInfo::labeled(egui::WidgetType::ComboBox, response.enabled(), purpose);
        info.current_text_value = Some(selected.into());
        info
    });
    ui.ctx().accesskit_node_builder(response.id, |node| {
        if selected == "Mixed" {
            node.set_state_description("Mixed values");
        }
    });
    focus_outline(ui, &response);
}

/// Draw the focused character control's outline.
/// Args: UI and native response. Returns no value; focus remains owned by the native widget.
fn focus_outline(ui: &Ui, response: &Response) {
    if response.has_focus() {
        ui.painter().rect_stroke(
            response.rect.expand(2.),
            3.,
            egui::Stroke::new(2., ui.visuals().selection.stroke.color),
            egui::StrokeKind::Outside,
        );
    }
}
pub fn show(ui: &mut Ui, studio: &mut Studio) {
    let values = studio.selected_character_metrics();
    let run = studio
        .selected_type()
        .unwrap_or_else(|| studio.type_defaults());
    let lead = common(&values, |s| s.leading);
    let kern = common(&values, |s| s.kerning);
    egui::Grid::new("character_spacing")
        .num_columns(2)
        .spacing([8., 7.])
        .show(ui, |ui| {
            let leading_label = ui.label("Leading").id;
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 4.;
                let selected = match lead {
                    Some(Leading::Auto(_)) => "Auto",
                    Some(Leading::Fixed(_)) => "Fixed",
                    None => "Mixed",
                };
                let mut choice = None;
                let mode = egui::ComboBox::from_id_salt("leading_mode")
                    .width(54.)
                    .selected_text(selected)
                    .show_ui(ui, |ui| {
                        let auto = ui
                            .selectable_label(matches!(lead, Some(Leading::Auto(_))), "Auto");
                        if auto.clicked() {
                            studio.patch_character(|s| s.leading = Some(Leading::Auto(120.)));
                            ui.close();
                            choice = Some(auto);
                        }
                        let fixed = ui
                            .selectable_label(matches!(lead, Some(Leading::Fixed(_))), "Fixed");
                        if fixed.clicked() {
                            studio.patch_character(|s| {
                                s.leading = Some(Leading::Fixed(lead.map_or_else(
                                    || run.line_height(),
                                    |value| value.pixels(run.px),
                                )))
                            });
                            ui.close();
                            choice = Some(fixed);
                        }
                    });
                if let Some(choice) = choice {
                    restore_edit_focus(ui, studio, &choice, &mode.response);
                }
                mode_name(ui, mode.response, leading_label, "Leading mode", selected);
                if let Some(number) = amount(
                    ui,
                    ("Leading amount", leading_label),
                    lead.map(|leading| match leading {
                        Leading::Auto(value) | Leading::Fixed(value) => value,
                    }),
                    120.,
                    1.0..=1000.0,
                    if matches!(lead, Some(Leading::Auto(_))) {
                        " %"
                    } else {
                        " px"
                    },
                ) {
                    studio.patch_character(|s| {
                        s.leading = Some(if matches!(lead, Some(Leading::Auto(_))) {
                            Leading::Auto(number)
                        } else {
                            Leading::Fixed(number)
                        })
                    });
                }
            });
            ui.end_row();
            let kerning_label = ui.label("Kerning").id;
            let selected = match kern {
                Some(KernMode::Metrics) => "Metrics",
                Some(KernMode::Optical) => "Optical",
                Some(KernMode::None) => "None",
                None => "Mixed",
            };
            let mut activated = None;
            let mode = egui::ComboBox::from_id_salt("kerning_mode")
                .selected_text(selected)
                .show_ui(ui, |ui| {
                    for (name, mode) in [
                        ("Metrics", KernMode::Metrics),
                        ("Optical", KernMode::Optical),
                        ("None", KernMode::None),
                    ] {
                        let choice = ui.selectable_label(kern == Some(mode), name);
                        if choice.clicked() {
                            studio.patch_character(|s| s.kerning = Some(mode));
                            ui.close();
                            activated = Some(choice);
                        }
                    }
                });
            if let Some(choice) = activated {
                restore_edit_focus(ui, studio, &choice, &mode.response);
            }
            mode_name(ui, mode.response, kerning_label, "Kerning mode", selected);
            ui.end_row();
            if let Some(edit) = studio.type_edit.clone().filter(|e| {
                e.caret == e.anchor && e.caret > 0 && e.caret < run.content.chars().count()
            }) {
                if let Some(value) = numeric(
                    ui,
                    "Pair kerning",
                    Some(*run.manual_kern.get(&edit.caret).unwrap_or(&0.)),
                    0.,
                    -1000.0..=10000.0,
                    " /1000",
                ) {
                    studio.patch_type(|r| {
                        r.manual_kern.insert(edit.caret, value);
                    });
                }
                ui.end_row();
            }
            if let Some(value) = numeric(
                ui,
                "Tracking",
                common(&values, |s| s.tracking),
                0.,
                -1000.0..=10000.0,
                " /1000",
            ) {
                studio.patch_character(|s| s.tracking = Some(value));
            }
            ui.end_row();
            if let Some(value) = numeric(
                ui,
                "Vertical scale",
                common(&values, |s| s.vscale),
                100.,
                1.0..=1000.0,
                " %",
            ) {
                studio.patch_character(|s| s.vscale = Some(value));
            }
            ui.end_row();
            if let Some(value) = numeric(
                ui,
                "Horizontal scale",
                common(&values, |s| s.hscale),
                100.,
                1.0..=1000.0,
                " %",
            ) {
                studio.patch_character(|s| s.hscale = Some(value));
            }
            ui.end_row();
            if let Some(value) = numeric(
                ui,
                "Baseline shift",
                common(&values, |s| s.baseline_shift),
                0.,
                -1000.0..=1000.0,
                " px",
            ) {
                studio.patch_character(|s| s.baseline_shift = Some(value));
            }
            ui.end_row();
        });
}

/// Return focus after a character mode choice.
/// Args: inspector UI, studio, activated choice and actual mode response. Returns no value.
/// Keyboard and accessibility choices return to the mode; pointer choices resume live canvas typing.
fn restore_edit_focus(ui: &Ui, studio: &Studio, choice: &Response, mode: &Response) {
    if !choice.clicked_by(egui::PointerButton::Primary) {
        mode.request_focus();
    } else if studio.type_edit.is_some() {
        ui.memory_mut(|m| m.request_focus(egui::Id::new("studio-canvas")));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{Event, Key, Modifiers, accesskit};

    const CONTROLS: [&str; 7] = [
        "Leading mode",
        "Leading amount",
        "Kerning mode",
        "Tracking",
        "Vertical scale",
        "Horizontal scale",
        "Baseline shift",
    ];

    /// Render the real character inspector without pointer input.
    /// Args: context, studio and native events. Returns paint and AccessKit output for this frame.
    fn frame(ctx: &egui::Context, studio: &mut Studio, events: Vec<Event>) -> egui::FullOutput {
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(360., 600.),
                )),
                events,
                ..Default::default()
            },
            |ui| show(ui, studio),
        );
        output.textures_delta.clear();
        output
    }

    /// Send a native key tap.
    /// Args: key and modifiers. Returns a press and release without synthesizing widget responses.
    fn key(key: Key, modifiers: Modifiers) -> Vec<Event> {
        [true, false]
            .into_iter()
            .map(|pressed| Event::Key {
                key,
                physical_key: Some(key),
                pressed,
                repeat: false,
                modifiers,
            })
            .collect()
    }

    /// Advance native keyboard focus.
    /// Args: context, studio and direction. Returns the settled frame, including delayed Shift-Tab focus.
    fn tab(ctx: &egui::Context, studio: &mut Studio, backwards: bool) -> egui::FullOutput {
        frame(
            ctx,
            studio,
            key(
                Key::Tab,
                if backwards {
                    Modifiers::SHIFT
                } else {
                    Modifiers::NONE
                },
            ),
        );
        frame(ctx, studio, vec![])
    }

    /// Find a named native control in the emitted tree.
    /// Args: frame output and purpose. Returns the control's identity and semantic node.
    fn named<'a>(output: &'a egui::FullOutput, name: &str) -> (accesskit::NodeId, &'a accesskit::Node) {
        output
            .platform_output
            .accesskit_update
            .as_ref()
            .unwrap()
            .nodes
            .iter()
            .find_map(|(id, node)| (node.label() == Some(name)).then_some((*id, node)))
            .unwrap_or_else(|| panic!("missing native control: {name}"))
    }

    /// Check native focus and its visible outline.
    /// Args: context, rendered output and purpose. Returns no value; checks tree and paint evidence.
    fn assert_focused(ctx: &egui::Context, output: &egui::FullOutput, name: &str) {
        let (id, node) = named(output, name);
        let focused = ctx.memory(|memory| memory.focused());
        let actual = focused.and_then(|id| {
            output.platform_output.accesskit_update.as_ref().unwrap().nodes.iter()
                .find(|(node_id, _)| *node_id == id.accesskit_id())
                .map(|(_, node)| (node.label(), node.value(), node.role()))
        });
        assert_eq!(
            focused.map(|id| id.accesskit_id()),
            Some(id),
            "expected {name}; focused {focused:?}; native node {actual:?}"
        );
        assert_eq!(
            output.platform_output.accesskit_update.as_ref().unwrap().focus,
            id
        );
        let bounds = node.bounds().unwrap();
        let outline = egui::Rect::from_min_max(
            egui::pos2(bounds.x0 as f32, bounds.y0 as f32),
            egui::pos2(bounds.x1 as f32, bounds.y1 as f32),
        )
        .expand(2.);
        fn painted(shape: &egui::Shape, outline: egui::Rect) -> bool {
            match shape {
                egui::Shape::Rect(rect) => {
                    rect.rect == outline && rect.stroke.width == 2. && rect.stroke.color.a() > 0
                }
                egui::Shape::Vec(shapes) => shapes.iter().any(|shape| painted(shape, outline)),
                _ => false,
            }
        }
        assert!(
            output.shapes.iter().any(|shape| painted(&shape.shape, outline)),
            "{name} needs a visible focus outline"
        );
    }

    /// Follow Tab to an existing control or popup choice.
    /// Args: context, studio and purpose. Returns the focused frame without requesting focus directly.
    fn tab_to(ctx: &egui::Context, studio: &mut Studio, name: &str) -> egui::FullOutput {
        let mut output = frame(ctx, studio, vec![]);
        for _ in 0..16 {
            let id = named(&output, name).0;
            if ctx.memory(|memory| memory.focused()).map(|id| id.accesskit_id()) == Some(id) {
                return output;
            }
            output = tab(ctx, studio, false);
        }
        panic!("Tab did not reach {name}");
    }

    /// Replace the focused native text buffer.
    /// Args: context, studio and text. Returns the edited frame after native Select All and text input.
    fn replace(ctx: &egui::Context, studio: &mut Studio, text: &str) -> egui::FullOutput {
        frame(
            ctx,
            studio,
            key(
                Key::A,
                Modifiers { ctrl: true, command: true, ..Modifiers::NONE },
            ),
        );
        frame(ctx, studio, vec![Event::Text(text.into())])
    }

    /// Create a live text selection through the real editor command.
    /// Args: none. Returns a studio with selected text and a live type session.
    fn text_fixture() -> Studio {
        let mut studio = Studio::new();
        studio.show_welcome = false;
        studio.text_font =
            concat!(env!("CARGO_MANIFEST_DIR"), "/tests/assets/fonts/EBGaramond.ttf").into();
        studio.place_text(crate::geom::Pt::new(40., 80.));
        assert!(studio.type_edit.is_some());
        studio
    }

    #[test]
    fn character_spacing_tab_edits_native_fields_and_paints_focus() {
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        crate::ui::theme::apply(&ctx);
        let mut studio = Studio::new();
        let initial = frame(&ctx, &mut studio, vec![]);
        let ids: std::collections::HashSet<_> =
            CONTROLS.iter().map(|name| named(&initial, name).0).collect();
        assert_eq!(ids.len(), CONTROLS.len());
        for (name, text) in CONTROLS.into_iter().zip([
            None,
            Some("135"),
            None,
            Some("24"),
            Some("85"),
            Some("110"),
            Some("−3"),
        ]) {
            let output = tab(&ctx, &mut studio, false);
            assert_focused(&ctx, &output, name);
            if let Some(text) = text {
                frame(&ctx, &mut studio, vec![Event::Text(text.into())]);
            }
        }
        let metrics = &studio.selected_character_metrics()[0];
        assert_eq!(metrics.leading, Some(Leading::Auto(135.)));
        assert_eq!(metrics.tracking, Some(24.));
        assert_eq!(metrics.vscale, Some(85.));
        assert_eq!(metrics.hscale, Some(110.));
        assert_eq!(metrics.baseline_shift, Some(-3.));
        let output = tab(&ctx, &mut studio, true);
        assert_focused(&ctx, &output, "Horizontal scale");
        frame(&ctx, &mut studio, key(Key::ArrowUp, Modifiers::NONE));
        assert_eq!(studio.selected_character_metrics()[0].hscale, Some(111.));
        assert_eq!(studio.selected_character_metrics()[0].vscale, Some(85.));
        frame(&ctx, &mut studio, key(Key::Enter, Modifiers::NONE));
        assert_eq!(ctx.memory(|memory| memory.focused()), None);
        assert_eq!(studio.selected_character_metrics()[0].hscale, Some(111.));
    }

    #[test]
    fn character_spacing_pair_row_never_steals_tracking_keyboard_identity() {
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        let mut studio = text_fixture();
        let initial = frame(&ctx, &mut studio, vec![]);
        let ids: Vec<_> = CONTROLS.iter().map(|name| named(&initial, name).0).collect();
        tab_to(&ctx, &mut studio, "Tracking");
        frame(&ctx, &mut studio, vec![Event::Text("72".into())]);
        let edit = studio.type_edit.as_mut().unwrap();
        edit.caret = 2;
        edit.anchor = 2;
        edit.pending_style = None;
        let output = frame(&ctx, &mut studio, vec![]);
        for (name, id) in CONTROLS.iter().zip(&ids) {
            assert_eq!(named(&output, name).0, *id, "{name}");
        }
        assert_focused(&ctx, &output, "Tracking");
        assert!(!ids.contains(&named(&output, "Pair kerning").0));
        frame(&ctx, &mut studio, key(Key::ArrowUp, Modifiers::NONE));
        assert_eq!(studio.selected_character_metrics()[0].tracking, Some(73.));
        assert!(studio.selected_type().unwrap().manual_kern.is_empty());
        let output = tab(&ctx, &mut studio, true);
        assert_focused(&ctx, &output, "Pair kerning");
        frame(&ctx, &mut studio, key(Key::ArrowDown, Modifiers::NONE));
        assert_eq!(studio.selected_type().unwrap().manual_kern.get(&2), Some(&-1.));
        let output = tab(&ctx, &mut studio, false);
        assert_focused(&ctx, &output, "Tracking");
        let edit = studio.type_edit.as_mut().unwrap();
        edit.caret = 4;
        edit.anchor = 4;
        let output = frame(&ctx, &mut studio, vec![]);
        assert_focused(&ctx, &output, "Tracking");
        assert_eq!(named(&output, "Tracking").0, ids[3]);
        frame(&ctx, &mut studio, key(Key::ArrowDown, Modifiers::NONE));
        assert_eq!(studio.selected_character_metrics()[0].tracking, Some(72.));
        let run = studio.selected_type().unwrap();
        assert_eq!(run.content, "Type");
        assert_eq!(run.manual_kern.get(&2), Some(&-1.));
    }

    #[test]
    fn character_spacing_live_text_mode_choices_restore_keyboard_focus() {
        for accessibility_choice in [false, true] {
            let ctx = egui::Context::default();
            ctx.enable_accesskit();
            let mut studio = text_fixture();
            let activate = |target_node| {
                if accessibility_choice {
                    vec![Event::AccessKitActionRequest(accesskit::ActionRequest {
                        action: accesskit::Action::Click,
                        target_tree: accesskit::TreeId::ROOT,
                        target_node,
                        data: None,
                    })]
                } else {
                    key(Key::Enter, Modifiers::NONE)
                }
            };
            frame(&ctx, &mut studio, vec![]);
            let output = tab(&ctx, &mut studio, false);
            assert_focused(&ctx, &output, "Leading mode");
            frame(&ctx, &mut studio, key(Key::Space, Modifiers::NONE));
            let output = tab_to(&ctx, &mut studio, "Fixed");
            frame(&ctx, &mut studio, activate(named(&output, "Fixed").0));
            let output = frame(&ctx, &mut studio, vec![]);
            assert_focused(&ctx, &output, "Leading mode");
            assert_eq!(named(&output, "Leading mode").1.value(), Some("Fixed"));
            let output = tab(&ctx, &mut studio, false);
            assert_focused(&ctx, &output, "Leading amount");
            frame(&ctx, &mut studio, vec![Event::Text("96".into())]);
            assert!(studio
                .selected_character_metrics()
                .iter()
                .all(|span| span.leading == Some(Leading::Fixed(96.))));
            let output = tab(&ctx, &mut studio, false);
            assert_focused(&ctx, &output, "Kerning mode");
            frame(&ctx, &mut studio, key(Key::Space, Modifiers::NONE));
            let output = tab_to(&ctx, &mut studio, "Optical");
            frame(&ctx, &mut studio, activate(named(&output, "Optical").0));
            let output = frame(&ctx, &mut studio, vec![]);
            assert_focused(&ctx, &output, "Kerning mode");
            assert_eq!(named(&output, "Kerning mode").1.value(), Some("Optical"));
            let output = tab(&ctx, &mut studio, false);
            assert_focused(&ctx, &output, "Tracking");
            frame(&ctx, &mut studio, vec![Event::Text("31".into())]);
            assert!(studio.selected_character_metrics().iter().all(|span| {
                span.tracking == Some(31.) && span.kerning == Some(KernMode::Optical)
            }));
            assert!(studio.type_edit.is_some());
            assert_eq!(studio.selected_type().unwrap().content, "Type");
        }
    }

    #[test]
    fn character_spacing_mixed_invalid_and_error_nodes_follow_native_text_edits() {
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        let mut studio = text_fixture();
        studio.patch_type(|run| {
            run.set_character_style(0, 1, |span| {
                span.tracking = Some(10.);
                span.leading = Some(Leading::Auto(120.));
                span.kerning = Some(KernMode::Metrics);
            });
            run.set_character_style(1, 4, |span| {
                span.tracking = Some(40.);
                span.leading = Some(Leading::Fixed(25.));
                span.kerning = Some(KernMode::Optical);
            });
        });
        let before = studio.selected_type().unwrap();
        let output = frame(&ctx, &mut studio, vec![]);
        for name in ["Leading mode", "Leading amount", "Kerning mode", "Tracking"] {
            let node = named(&output, name).1;
            assert_eq!(node.value(), Some("Mixed"), "{name}");
            assert_eq!(node.state_description(), Some("Mixed values"), "{name}");
            assert_eq!(node.invalid(), None, "mixed is a valid selection state");
            assert_eq!(node.numeric_value(), None);
        }
        tab_to(&ctx, &mut studio, "Tracking");
        assert_eq!(studio.selected_type().unwrap(), before);
        for (text, message) in [
            ("letters", "Enter a finite number"),
            ("NaN", "Enter a finite number"),
            ("10001", "Enter a value from -1000 to 10000 /1000"),
        ] {
            let output = replace(&ctx, &mut studio, text);
            let node = named(&output, "Tracking").1;
            assert_eq!(node.invalid(), Some(accesskit::Invalid::True));
            assert_eq!(node.numeric_value(), None);
            assert!(node.description().unwrap().contains(message));
            let error = node.error_message().expect("invalid fields link their error");
            assert!(node.described_by().contains(&error));
            let error_node = &output
                .platform_output
                .accesskit_update
                .as_ref()
                .unwrap()
                .nodes
                .iter()
                .find(|(id, _)| *id == error)
                .unwrap()
                .1;
            assert_eq!(error_node.value(), Some(message));
            assert_eq!(error_node.live(), Some(accesskit::Live::Polite));
            assert!(error_node.is_live_atomic());
            assert_eq!(
                studio.selected_type().unwrap(),
                before,
                "invalid text must not write a fallback"
            );
            assert_focused(&ctx, &output, "Tracking");
        }
        replace(&ctx, &mut studio, "24");
        let output = frame(&ctx, &mut studio, vec![]);
        let node = named(&output, "Tracking").1;
        assert_eq!(node.invalid(), None);
        assert_eq!(node.state_description(), None);
        assert!(node.described_by().is_empty());
        assert_eq!(node.error_message(), None);
        assert_eq!(node.numeric_value(), Some(24.));
        assert!(
            studio.selected_character_metrics().iter().all(|span| span.tracking == Some(24.))
        );
        assert_eq!(studio.selected_type().unwrap().content, before.content);
    }

    #[test]
    fn character_spacing_keyboard_modes_and_accesskit_actions_keep_property_names() {
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        let mut studio = Studio::new();
        let output = frame(&ctx, &mut studio, vec![]);
        let tree = output.platform_output.accesskit_update.as_ref().unwrap();
        for (name, label) in CONTROLS.into_iter().zip([
            "Leading",
            "Leading",
            "Kerning",
            "Tracking",
            "Vertical scale",
            "Horizontal scale",
            "Baseline shift",
        ]) {
            let node = named(&output, name).1;
            assert!(node.supports_action(accesskit::Action::Focus));
            assert!(node.labelled_by().iter().any(|label_id| {
                tree.nodes.iter().any(|(id, node)| id == label_id && node.value() == Some(label))
            }));
            assert_eq!(
                node.role(),
                if name.ends_with("mode") {
                    accesskit::Role::ComboBox
                } else {
                    accesskit::Role::SpinButton
                }
            );
        }
        tab_to(&ctx, &mut studio, "Leading mode");
        frame(&ctx, &mut studio, key(Key::Space, Modifiers::NONE));
        tab_to(&ctx, &mut studio, "Fixed");
        frame(&ctx, &mut studio, key(Key::Enter, Modifiers::NONE));
        let output = frame(&ctx, &mut studio, vec![]);
        assert_eq!(named(&output, "Leading mode").1.value(), Some("Fixed"));
        assert!(matches!(
            studio.selected_character_metrics()[0].leading,
            Some(Leading::Fixed(_))
        ));
        tab_to(&ctx, &mut studio, "Kerning mode");
        frame(&ctx, &mut studio, key(Key::Space, Modifiers::NONE));
        tab_to(&ctx, &mut studio, "Optical");
        frame(&ctx, &mut studio, key(Key::Enter, Modifiers::NONE));
        let output = frame(&ctx, &mut studio, vec![]);
        assert_eq!(named(&output, "Kerning mode").1.value(), Some("Optical"));
        assert_eq!(studio.selected_character_metrics()[0].kerning, Some(KernMode::Optical));
        let (tracking, node) = named(&output, "Tracking");
        for action in [
            accesskit::Action::SetValue,
            accesskit::Action::Increment,
            accesskit::Action::Decrement,
        ] {
            assert!(node.supports_action(action));
        }
        for (action, data, expected) in [
            (
                accesskit::Action::SetValue,
                Some(accesskit::ActionData::NumericValue(64.)),
                64.,
            ),
            (accesskit::Action::Increment, None, 65.),
            (accesskit::Action::Decrement, None, 64.),
        ] {
            frame(
                &ctx,
                &mut studio,
                vec![Event::AccessKitActionRequest(accesskit::ActionRequest {
                    action,
                    target_tree: accesskit::TreeId::ROOT,
                    target_node: tracking,
                    data,
                })],
            );
            assert_eq!(studio.selected_character_metrics()[0].tracking, Some(expected));
            assert_eq!(studio.selected_character_metrics()[0].vscale, Some(100.));
        }
    }
}
