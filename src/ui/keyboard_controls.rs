use eframe::egui::{self, Key, Modifiers, Ui};

/// Decorate an existing precise motion widget without changing its native step or formatting.
/// Args: UI, owner, name, value, range, step, suffix and optional decimal limit.
/// Returns the actual response with accepted changes and owned draft validity; history remains app-owned.
pub(super) fn motion_number(
    ui: &mut Ui,
    owner: egui::Id,
    labels: (&str, &str),
    value: &mut f32,
    range: std::ops::RangeInclusive<f32>,
    step: f64,
    suffix: &str,
    decimals: Option<usize>,
) -> (super::fields::FieldResponse, bool) {
    let (label, purpose) = labels;
    let before = *value;
    let attempted = std::cell::RefCell::new(None);
    let limits = f64::from(*range.start())..=f64::from(*range.end());
    let response = super::fields::native_scope(ui, owner, |ui| {
        ui.horizontal(|ui| {
            let name = ui.label(label);
            let mut widget = egui::DragValue::new(value)
                .range(range)
                .speed(step)
                .suffix(suffix)
                .custom_parser(|text| super::welcome::parse_number(&attempted, text));
            if let Some(decimals) = decimals {
                widget = widget.max_decimals(decimals);
            }
            let response = ui.add(widget).labelled_by(name.id);
            response.widget_info(|| {
                let mut info = egui::WidgetInfo::drag_value(response.enabled(), f64::from(*value));
                info.label = Some(purpose.into());
                info
            });
            response
        })
        .inner
    })
    .inner;
    let cancelled = (response.has_focus() || response.lost_focus() || response.dragged())
        && ui.input(|input| input.key_pressed(Key::Escape));
    let valid = super::welcome::native_number_valid(
        ui,
        &response,
        owner,
        attempted.into_inner(),
        f64::from(*value),
        limits,
    );
    if !valid || cancelled {
        *value = before;
    }
    if !valid && response.enabled() {
        super::welcome::control_error(
            ui,
            &[&response],
            "Enter a finite number within the shown range",
        );
    }
    number_focus(ui, &response);
    super::welcome::focus_control(ui, &response);
    if response.enabled() {
        ui.data_mut(|data| data.insert_temp(owner.with("motion-number-valid"), valid));
    }
    (
        super::fields::FieldResponse {
            changed: valid && !cancelled && response.changed(),
            committed: valid && !cancelled && (response.lost_focus() || response.drag_stopped()),
            cancelled,
            response,
        },
        valid,
    )
}

/// Read a motion draft's own validation while its advanced controls are collapsed.
/// Args: context and stable field owner. Returns true until that widget rejects an edit.
pub(super) fn motion_number_is_valid(ctx: &egui::Context, owner: egui::Id) -> bool {
    ctx.data(|data| {
        data.get_temp::<bool>(owner.with("motion-number-valid"))
            .unwrap_or(true)
    })
}

/// Draw a native action with a stable property identity and visible keyboard focus.
/// Args: UI, property name and visible label. Returns the actual button response.
pub(super) fn button(ui: &mut Ui, id: &str, label: &str) -> egui::Response {
    let id = ui.make_persistent_id(id);
    let response = super::fields::native_scope(ui, id, |ui| ui.button(label)).inner;
    super::welcome::focus_control(ui, &response);
    response
}

/// Allocate the identity of a newly opened app-owned draft.
/// Args: context and dialog purpose. Returns an ID stable until that draft closes.
pub(super) fn draft_id(ctx: &egui::Context, purpose: &str) -> egui::Id {
    let key = egui::Id::new(("keyboard-draft-instance", purpose));
    let next = ctx.data_mut(|data| {
        let next = data
            .get_temp::<u64>(key)
            .unwrap_or_default()
            .wrapping_add(1);
        data.insert_temp(key, next);
        next
    });
    key.with(next)
}

/// Keep native numeric arrow edits in the focused field.
/// Args: UI and actual numeric response. Returns no value and never transfers focus.
pub(super) fn number_focus(ui: &Ui, response: &egui::Response) {
    if !response.has_focus() {
        return;
    }
    let stepped = ui.input(|input| input.events.iter().any(|event| matches!(event,
        egui::Event::Key { key: Key::ArrowUp | Key::ArrowDown, pressed: true, modifiers, .. } if !modifiers.any())));
    ui.memory_mut(|memory| {
        memory.set_focus_lock_filter(
            response.id,
            egui::EventFilter {
                horizontal_arrows: true,
                vertical_arrows: true,
                ..Default::default()
            },
        );
        if stepped {
            memory.move_focus(egui::FocusDirection::None);
        }
    });
}

/// Choose an existing panel target or operation through its native combo.
/// Args: UI, stable property, visible name, selected value and available choices.
/// Returns the native response, retaining focus after keyboard selection.
pub(super) fn choice<T: Clone + PartialEq>(
    ui: &mut Ui,
    id: &str,
    label: &str,
    value: &mut T,
    options: &[(T, String)],
) -> egui::Response {
    let id = ui.make_persistent_id(id);
    super::fields::native_scope(ui, id, |ui| {
        let label_response = ui.label(label);
        let before = value.clone();
        let mut selected = false;
        let mut response = egui::ComboBox::from_id_salt("value")
            .selected_text(
                options
                    .iter()
                    .find(|(key, _)| key == value)
                    .map_or("Choose…", |(_, name)| name.as_str()),
            )
            .show_ui(ui, |ui| {
                for (key, name) in options {
                    let item = ui.selectable_value(value, key.clone(), name);
                    super::welcome::focus_control(ui, &item);
                    selected |= item.clicked();
                }
            })
            .response
            .labelled_by(label_response.id);
        if selected {
            response.request_focus();
        }
        if response.enabled() && response.has_focus() {
            ui.memory_mut(|memory| {
                memory.set_focus_lock_filter(
                    response.id,
                    egui::EventFilter {
                        horizontal_arrows: true,
                        vertical_arrows: true,
                        ..Default::default()
                    },
                )
            });
            if !egui::ComboBox::is_open(ui.ctx(), response.id) && !options.is_empty() {
                let index = options
                    .iter()
                    .position(|(key, _)| key == value)
                    .unwrap_or(0);
                let next = ui.input_mut(|input| {
                    if input.consume_key(Modifiers::NONE, Key::ArrowDown)
                        || input.consume_key(Modifiers::NONE, Key::ArrowRight)
                    {
                        Some((index + 1).min(options.len() - 1))
                    } else if input.consume_key(Modifiers::NONE, Key::ArrowUp)
                        || input.consume_key(Modifiers::NONE, Key::ArrowLeft)
                    {
                        Some(index.saturating_sub(1))
                    } else if input.consume_key(Modifiers::NONE, Key::Home) {
                        Some(0)
                    } else if input.consume_key(Modifiers::NONE, Key::End) {
                        Some(options.len() - 1)
                    } else {
                        None
                    }
                });
                if let Some(index) = next {
                    *value = options[index].0.clone();
                    ui.memory_mut(|memory| memory.move_focus(egui::FocusDirection::None));
                }
            }
        }
        if *value != before {
            response.mark_changed();
        }
        response.widget_info(|| {
            let mut info =
                egui::WidgetInfo::labeled(egui::WidgetType::ComboBox, response.enabled(), label);
            info.current_text_value = options
                .iter()
                .find(|(key, _)| key == value)
                .map(|(_, name)| name.clone());
            info
        });
        super::welcome::focus_control(ui, &response);
        response
    })
    .inner
}
