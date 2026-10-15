//! Small property controls. Applications own themes, values and commands.
use eframe::egui::{self, Response, Ui};
use std::cell::RefCell;
#[path = "native_scope.rs"]
mod scope;
pub use scope::native_scope;

#[derive(Clone, Copy, Default)]
pub struct FieldState<'a> {
    pub mixed: bool,
    pub locked: bool,
    pub error: Option<&'a str>,
}

pub struct FieldResponse {
    pub response: Response,
    pub changed: bool,
    pub committed: bool,
    pub cancelled: bool,
}

/// Check whether a numeric field's current native draft and domain state are valid.
/// Args: the response returned by number in this frame.
/// Returns false for rejected input or an associated application error; changed still owns edits.
pub fn number_is_valid(field: &FieldResponse) -> bool {
    field.response.ctx.data(|data| {
        data.get_temp::<bool>(field.response.id.with("oma-number-valid"))
            .unwrap_or(true)
    })
}

/// Parse the native egui number grammar without accepting nonfinite input.
/// Args: native draft. Returns the parsed finite value for egui's existing range handling.
fn parse_number(text: &str) -> Option<f64> {
    text.chars()
        .filter(|character| !character.is_whitespace())
        .map(|character| if character == '−' { '-' } else { character })
        .collect::<String>()
        .parse::<f64>()
        .ok()
        .filter(|value| value.is_finite())
}

/// Draw a property field.
/// Args: UI, stable identity, label, value, range, unit, reset value and selection state.
/// Returns the focus, change, commit and cancel signals; the caller owns document edits.
pub fn number<N: egui::emath::Numeric>(
    ui: &mut Ui,
    id: impl std::hash::Hash + std::fmt::Debug,
    label: &str,
    value: &mut N,
    range: std::ops::RangeInclusive<N>,
    unit: &str,
    reset: Option<N>,
    state: FieldState<'_>,
) -> FieldResponse {
    let before = *value;
    let enabled = ui.is_enabled() && !state.locked;
    let mut reset_clicked = false;
    let key = ui.make_persistent_id(id);
    let draft_key = key.with("invalid-native-number");
    let attempted = RefCell::new(None);
    #[cfg(not(feature = "oma-ui-egui032"))]
    let arrow_step = ui.input(|input| input.events.iter().any(|event| matches!(event,
        egui::Event::Key { key: egui::Key::ArrowUp | egui::Key::ArrowDown, pressed: true, modifiers, .. }
            if !modifiers.any())));
    native_scope(ui, key, |ui| {
        ui.vertical(|ui| {
            ui.set_min_width(0.0);
            let label_response = ui.add(egui::Label::new(label).wrap());
            let mut response = ui
                .add_enabled_ui(enabled, |ui| {
                    ui.horizontal(|ui| {
                        let width = (ui.available_width()
                            - if reset.is_some() { 48.0 } else { 0.0 })
                        .max(24.0);
                        let mut drag = egui::DragValue::new(value)
                            .range(range)
                            .clamp_existing_to_range(false)
                            .speed(0.5)
                            .max_decimals(2)
                            .suffix(unit)
                            .custom_parser(|text| {
                                attempted.replace(Some(text.to_owned()));
                                parse_number(text)
                            });
                        if state.mixed {
                            drag = drag.custom_formatter(|_, _| "Mixed".into());
                        }
                        let mut response = ui
                            .add_sized([width, 24.0], drag)
                            .labelled_by(label_response.id)
                            .on_hover_text(label);
                        if let Some(reset) = reset {
                            let reset_response = ui
                                .small_button("Reset")
                                .on_hover_text(format!("Reset {label}"));
                            reset_response.widget_info(|| {
                                egui::WidgetInfo::labeled(
                                    egui::WidgetType::Button,
                                    reset_response.enabled(),
                                    format!("Reset {label}"),
                                )
                            });
                            if reset_response.clicked() {
                                reset_clicked = true;
                                *value = reset;
                                response.mark_changed();
                            }
                        }
                        response.widget_info(|| {
                            let mut info =
                                egui::WidgetInfo::drag_value(response.enabled(), value.to_f64());
                            info.label = Some(label.into());
                            info
                        });
                        response
                    })
                    .inner
                })
                .inner;
            let cancelled = (response.has_focus() || response.lost_focus() || response.dragged())
                && ui.input(|input| input.key_pressed(egui::Key::Escape));
            if cancelled || !enabled {
                *value = before;
            }
            let invalid_action = enabled
                && ui.input(|input| {
                    input
                        .accesskit_action_requests(response.id, egui::accesskit::Action::SetValue)
                        .any(|request| {
                            matches!(request.data,
                    Some(egui::accesskit::ActionData::NumericValue(value)) if !value.is_finite())
                        })
                });
            let invalid_value = !value.to_f64().is_finite() || invalid_action;
            let mut invalid_draft = ui.data(|data| data.get_temp::<String>(draft_key));
            if cancelled || reset_clicked {
                invalid_draft = None;
            } else if enabled {
                if let Some(text) = attempted.into_inner() {
                    let untouched_mixed = state.mixed
                        && text == "Mixed"
                        && !response.changed()
                        && invalid_draft.is_none();
                    invalid_draft =
                        (parse_number(&text).is_none() && !untouched_mixed).then_some(text);
                } else if response.changed() && !invalid_value {
                    invalid_draft = None;
                }
            }
            ui.data_mut(|data| {
                if let Some(text) = &invalid_draft {
                    data.insert_temp(draft_key, text.clone());
                } else {
                    data.remove::<String>(draft_key);
                }
            });
            let invalid = invalid_value || invalid_draft.is_some();
            if invalid {
                *value = before;
            }
            let changed = response.changed() && !cancelled && !invalid && enabled;
            ui.data_mut(|data| {
                data.insert_temp(
                    response.id.with("oma-number-valid"),
                    !invalid && state.error.is_none(),
                );
            });
            ui.ctx().accesskit_node_builder(response.id, |node| {
                node.set_label(label);
                if state.mixed {
                    node.set_state_description("Mixed");
                    node.clear_numeric_value();
                    if !changed && !invalid {
                        node.set_value("Mixed");
                    }
                }
            });
            if let Some(error) = state.error.or(invalid.then_some("Enter a finite number")) {
                let error = ui.colored_label(ui.visuals().error_fg_color, error);
                ui.ctx().accesskit_node_builder(error.id, |node| {
                    node.set_live(egui::accesskit::Live::Polite);
                    node.set_live_atomic();
                });
                ui.ctx().accesskit_node_builder(response.id, |node| {
                    node.set_invalid(egui::accesskit::Invalid::True);
                    node.clear_numeric_value();
                    node.set_error_message(egui::accesskit::NodeId(error.id.value()));
                    node.push_described_by(egui::accesskit::NodeId(error.id.value()));
                });
            }
            if response.has_focus() {
                ui.memory_mut(|memory| {
                    memory.set_focus_lock_filter(
                        response.id,
                        egui::EventFilter {
                            horizontal_arrows: true,
                            vertical_arrows: true,
                            tab: false,
                            escape: false,
                        },
                    );
                    #[cfg(not(feature = "oma-ui-egui032"))]
                    if arrow_step {
                        memory.move_focus(egui::FocusDirection::None);
                    }
                });
                response.scroll_to_me(Some(egui::Align::Center));
                ui.painter().rect_stroke(
                    response.rect.expand(2.),
                    3.,
                    egui::Stroke::new(2., ui.visuals().selection.stroke.color),
                    egui::StrokeKind::Outside,
                );
            }
            let committed = !cancelled
                && !invalid
                && enabled
                && (reset_clicked
                    || response.drag_stopped()
                    || response.lost_focus()
                    || (changed && !response.dragged() && !response.has_focus()));
            FieldResponse {
                response,
                changed,
                committed,
                cancelled,
            }
        })
        .inner
    })
    .inner
}

/// Draw an application action.
/// Args: UI, label and enabled state. Returns the native response for focus and activation.
pub fn action(ui: &mut Ui, label: &str, enabled: bool) -> Response {
    ui.add_enabled(enabled, egui::Button::new(label))
}

/// Draw a panel tab.
/// Args: UI, label and active state. Returns the native response for selection and focus.
pub fn tab(ui: &mut Ui, label: &str, active: bool) -> Response {
    ui.selectable_label(active, label)
}

/// Draw a named panel section.
/// Args: UI, title and application content. Returns the application's result.
pub fn section<R>(ui: &mut Ui, title: &str, content: impl FnOnce(&mut Ui) -> R) -> R {
    ui.group(|ui| {
        ui.label(egui::RichText::new(title).strong());
        content(ui)
    })
    .inner
}
