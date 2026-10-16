use eframe::egui::{self, Id, Response, Ui};
use std::ops::RangeInclusive;

/// Check ordinary inspector availability without changing its typed command gates.
/// Args: editor. Returns true while an operation, layer, selected object or its parent prevents editing.
pub(in crate::ui) fn blocked(studio: &crate::app::Studio) -> bool {
    if studio.file_dialog_pending()
        || studio.pending_nav.is_some()
        || studio.updates.freezing
        || studio.op.is_some()
        || studio.deformation.is_some()
        || studio.free_transform.is_some()
    {
        return true;
    }
    studio.selection.iter().any(|&(layer, id)| {
        if !studio.doc.layer_editable(layer) {
            return true;
        }
        let mut current = Some(id);
        let mut seen = std::collections::HashSet::new();
        let mut root = id;
        while let Some(id) = current {
            let Some(shape) = studio.doc.find_shape(layer, id) else {
                return true;
            };
            if !seen.insert(id) || !shape.visible || shape.locked {
                return true;
            }
            root = id;
            current = shape.layout.parent;
        }
        crate::layout::descendants(&studio.doc, layer, root)
            .into_iter()
            .any(|id| {
                studio
                    .doc
                    .find_shape(layer, id)
                    .is_some_and(|shape| shape.locked)
            })
    })
}

/// Decorate one original layout number without narrowing its type or native configuration.
/// Args: UI, property, purpose, value, range, optional native step, prefix and suffix.
/// Returns the actual response with only accepted native changes; callers own history.
pub(super) fn number<N: egui::emath::Numeric + std::fmt::Display>(
    ui: &mut Ui,
    property: impl std::hash::Hash + std::fmt::Debug,
    name: &str,
    value: &mut N,
    range: RangeInclusive<N>,
    speed: Option<f64>,
    prefix: &str,
    suffix: &str,
) -> crate::ui::fields::FieldResponse {
    let owner = ui.make_persistent_id(property);
    let before = *value;
    let limits = range.start().to_f64()..=range.end().to_f64();
    let attempted = std::cell::RefCell::new(None);
    let response = crate::ui::fields::native_scope(ui, owner, |ui| {
        let label = ui.label(name);
        let mut widget = egui::DragValue::new(value)
            .range(range)
            .clamp_existing_to_range(false)
            .prefix(prefix)
            .suffix(suffix)
            .custom_formatter(|value, _| N::from_f64(value).to_string())
            .custom_parser(|text| crate::ui::welcome::parse_number(&attempted, text));
        if let Some(speed) = speed {
            widget = widget.speed(speed);
        }
        let response = ui.add(widget).labelled_by(label.id);
        response.widget_info(|| {
            let mut info = egui::WidgetInfo::drag_value(response.enabled(), value.to_f64());
            info.label = Some(name.into());
            info
        });
        response
    })
    .inner;
    let cancelled = (response.has_focus() || response.lost_focus() || response.dragged())
        && ui.input(|input| input.key_pressed(egui::Key::Escape));
    let edited = response.dragged()
        || ui.input(|input| {
            input.events.iter().any(|event| match event {
                egui::Event::Text(_)
                | egui::Event::Paste(_)
                | egui::Event::Cut
                | egui::Event::Ime(_) => response.has_focus() || response.lost_focus(),
                egui::Event::Key {
                    key, pressed: true, ..
                } => {
                    response.has_focus()
                        && !matches!(key, egui::Key::Tab | egui::Key::Enter | egui::Key::Escape)
                }
                egui::Event::AccessKitActionRequest(request) => {
                    request.target_node == response.id.accesskit_id()
                        && matches!(
                            request.action,
                            egui::accesskit::Action::SetValue
                                | egui::accesskit::Action::Increment
                                | egui::accesskit::Action::Decrement
                        )
                }
                _ => false,
            })
        });
    let valid = crate::ui::welcome::native_number_valid(
        ui,
        &response,
        owner,
        attempted.into_inner(),
        value.to_f64(),
        if edited && response.changed() {
            limits
        } else {
            f64::NEG_INFINITY..=f64::INFINITY
        },
    );
    if !valid || cancelled || !edited || !response.enabled() {
        *value = before;
    }
    ui.ctx().accesskit_node_builder(response.id, |node| {
        node.set_label(name);
        if valid {
            node.set_numeric_value(value.to_f64());
        } else {
            node.clear_numeric_value();
        }
    });
    if !valid && response.enabled() {
        crate::ui::welcome::control_error(
            ui,
            &[&response],
            "Enter a finite number within the shown range",
        );
    }
    crate::ui::keyboard_controls::number_focus(ui, &response);
    crate::ui::welcome::focus_control(ui, &response);
    crate::ui::fields::FieldResponse {
        changed: response.enabled() && edited && valid && !cancelled && response.changed(),
        committed: valid && !cancelled && (response.lost_focus() || response.drag_stopped()),
        cancelled,
        response,
    }
}

#[derive(Clone)]
pub(super) struct Rows<T> {
    expected: Vec<T>,
    revision: u64,
    instance: Id,
    widgets: Vec<Id>,
    key: Id,
}

impl<T: Clone + PartialEq + Send + Sync + 'static> Rows<T> {
    /// Resolve the lifetime of an existing index-addressed layout list.
    /// Args: UI, semantic list name and stored rows. Returns opaque owners; replacements retire focus and drafts.
    pub(super) fn begin(ui: &Ui, name: &str, values: &[T], revision: u64) -> Self {
        let key = ui.make_persistent_id(("layout-list-rows", name));
        let mut owner = ui
            .data(|data| data.get_temp::<Self>(key))
            .unwrap_or_else(|| Self {
                expected: values.to_vec(),
                revision,
                instance: crate::ui::keyboard_controls::draft_id(ui.ctx(), "layout-list"),
                widgets: vec![],
                key,
            });
        if owner.expected.as_slice() != values || owner.revision != revision {
            owner.retire(ui);
        }
        owner.revision = revision;
        owner.widgets.clear();
        owner
    }

    /// Get an opaque owner for a row in this current list lifetime.
    /// Args: current row index. Returns a transient native scope, never a serialized model identity.
    pub(super) fn id(&self, index: usize) -> Id {
        self.instance.with(index)
    }

    /// Observe a real native response for focus retirement.
    /// Args: actual response. Returns it unchanged.
    pub(super) fn watch(&mut self, response: Response) -> Response {
        self.widgets.push(response.id);
        response
    }

    fn retire(&mut self, ui: &Ui) {
        if let Some(focused) = ui.memory(|memory| memory.focused())
            && self.widgets.contains(&focused)
        {
            ui.memory_mut(|memory| memory.surrender_focus(focused));
        }
        self.instance = crate::ui::keyboard_controls::draft_id(ui.ctx(), "layout-list");
        self.widgets.clear();
    }

    /// Retain the result of this row's own accepted draft changes.
    /// Args: UI, current rows and whether this frame changed their structure. Returns no value.
    pub(super) fn finish(mut self, ui: &Ui, values: &[T], structural: bool) {
        if structural {
            self.retire(ui);
        }
        self.expected = values.to_vec();
        let key = self.key;
        ui.data_mut(|data| data.insert_temp(key, self));
    }

    /// Advance list ownership only after its own typed inspector command was accepted.
    /// Args: UI, semantic list name and resulting document revision. Returns no value.
    pub(super) fn accepted(ui: &Ui, name: &str, revision: u64) {
        let key = ui.make_persistent_id(("layout-list-rows", name));
        ui.data_mut(|data| {
            if let Some(mut owner) = data.get_temp::<Self>(key) {
                owner.revision = revision;
                data.insert_temp(key, owner);
            }
        });
    }

    /// Render the existing canonical track field and observe its accepted response once.
    /// Args: UI, property, name, value, range and unit. Returns the sole accepted history adapter result.
    pub(super) fn shared_number<N: egui::emath::Numeric>(
        &mut self,
        ui: &mut Ui,
        property: impl std::hash::Hash + std::fmt::Debug,
        name: &str,
        value: &mut N,
        range: RangeInclusive<N>,
        unit: &str,
    ) -> bool {
        let field = crate::ui::fields::number(
            ui,
            property,
            name,
            value,
            range,
            unit,
            None,
            Default::default(),
        );
        self.widgets.push(field.response.id);
        crate::app::input_history::record(field.response, field.changed)
    }

    /// Observe an original native layout number and pass its accepted flag to the sole history adapter.
    /// Args: UI, property, purpose, value, native range, step, prefix and suffix. Returns accepted changed.
    pub(super) fn number<N: egui::emath::Numeric + std::fmt::Display>(
        &mut self,
        ui: &mut Ui,
        property: impl std::hash::Hash + std::fmt::Debug,
        name: &str,
        value: &mut N,
        range: RangeInclusive<N>,
        speed: Option<f64>,
        prefix: &str,
        suffix: &str,
    ) -> bool {
        let field = number(ui, property, name, value, range, speed, prefix, suffix);
        self.widgets.push(field.response.id);
        crate::app::input_history::record(field.response, field.changed)
    }
}
