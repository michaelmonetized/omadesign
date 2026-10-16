//! Named properties edit existing native fields and retain ordinary instance overrides.
use crate::app::{ComponentPropertyEdit as Edit, Studio};
use crate::document::Fill;
use crate::geom::Geom;
use crate::layout_components::{
    self as components, ComponentBinding, PropertyKind as Kind, PropertyValue as Value,
};
use eframe::egui::{self, Ui};

#[derive(Clone)]
struct TextDraft {
    value: String,
    before: String,
}

fn property_name_error(name: &str) -> Option<&'static str> {
    (name.trim().is_empty() || name.chars().count() > 120)
        .then_some("Use a property name from 1 to 120 characters")
}

fn text(
    ui: &mut Ui,
    id: impl std::hash::Hash + std::fmt::Debug,
    name: &str,
    value: &str,
    validate: fn(&str) -> Option<&'static str>,
) -> Option<String> {
    let key = ui.make_persistent_id(id);
    let mut draft = ui
        .data(|data| data.get_temp::<TextDraft>(key))
        .unwrap_or_else(|| TextDraft {
            value: value.into(),
            before: value.into(),
        });
    let stale = ui
        .data(|data| data.get_temp::<bool>(key.with("stale")))
        .unwrap_or(false);
    let label = ui.label(name);
    let response = ui
        .add(egui::TextEdit::singleline(&mut draft.value).id(key))
        .labelled_by(label.id);
    super::welcome::name_control(ui, &response, label.id, name);
    let invalid = validate(&draft.value);
    if let Some(message) = invalid {
        super::welcome::control_error(ui, &[&response], message);
    } else if stale {
        super::welcome::control_error(
            ui,
            &[&response],
            "The property changed. Focus the field to edit its current value.",
        );
    }
    let commit = response.lost_focus()
        || (response.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)));
    if ui.input(|i| i.key_pressed(egui::Key::Escape)) && response.has_focus() {
        ui.data_mut(|d| {
            d.remove::<TextDraft>(key);
            d.remove::<bool>(key.with("stale"));
        });
        response.surrender_focus();
        return None;
    }
    if invalid.is_some() {
        ui.data_mut(|data| data.insert_temp(key, draft));
        return None;
    }
    if commit {
        ui.data_mut(|d| d.remove::<TextDraft>(key));
        if draft.before != value {
            ui.data_mut(|d| d.insert_temp(key.with("stale"), true));
            None
        } else {
            (draft.value != value).then_some(draft.value)
        }
    } else {
        if response.has_focus() {
            ui.data_mut(|d| d.remove::<bool>(key.with("stale")));
            ui.data_mut(|d| d.insert_temp(key, draft));
        }
        None
    }
}

pub(super) fn inspector(ui: &mut Ui, studio: &mut Studio) {
    let Some(target) = studio.component_property_target() else {
        return;
    };
    let (layer, root) = (target.layer, target.root);
    let Some((_, selected)) = studio.primary() else {
        return;
    };
    let Some(shape) = studio.doc.find_shape(layer, root) else {
        return;
    };
    let main = matches!(shape.layout.component, Some(ComponentBinding::Main { .. }));
    let properties = components::properties(&studio.doc, root);
    let document = studio.swap_id.clone();
    let blocked = super::layout::keyboard::blocked(studio);
    let mut edit = None;
    let mut select = None;
    egui::CollapsingHeader::new("Named properties")
        .default_open(!properties.is_empty())
        .show(ui, |ui| {
            ui.add_enabled_ui(!blocked, |ui| {
                ui.push_id(
                    (
                        document,
                        studio.selection.clone(),
                        layer,
                        root,
                        selected,
                        "named-properties",
                    ),
                    |ui| {
                        for property in properties {
                            let row = ui.make_persistent_id(("named-property", property.id));
                            super::fields::native_scope(ui, row, |ui| {
                                match components::property_value(
                                    &studio.doc,
                                    layer,
                                    root,
                                    property.id,
                                ) {
                                    Ok(Value::Text(value)) => {
                                        if let Some(value) =
                                            text(ui, "value", &property.name, &value, |_| None)
                                        {
                                            edit = Some(Edit::Set {
                                                id: property.id,
                                                value: Value::Text(value),
                                            });
                                        }
                                    }
                                    Ok(Value::Visibility(mut value)) => {
                                        let field = ui.make_persistent_id("visibility");
                                        if super::svg_paint::control(
                                            ui,
                                            field,
                                            &property.name,
                                            |ui| ui.checkbox(&mut value, &property.name),
                                        )
                                        .changed()
                                        {
                                            edit = Some(Edit::Set {
                                                id: property.id,
                                                value: Value::Visibility(value),
                                            });
                                        }
                                    }
                                    Ok(Value::Opacity(mut value)) => {
                                        studio.with_field_history(
                                            ui,
                                            ("named-opacity", root, property.id),
                                            |ui, studio| {
                                                value *= 100.;
                                                let response = super::fields::number(
                                                    ui,
                                                    "value",
                                                    &property.name,
                                                    &mut value,
                                                    0.0..=100.0,
                                                    " %",
                                                    None,
                                                    Default::default(),
                                                );
                                                if crate::app::input_history::record(
                                                    response.response,
                                                    response.changed,
                                                ) {
                                                    studio.apply_component_property(
                                                        target.clone(),
                                                        Edit::Set {
                                                            id: property.id,
                                                            value: Value::Opacity(value / 100.),
                                                        },
                                                    );
                                                }
                                            },
                                        );
                                    }
                                    Ok(Value::Fill(Fill::Solid(color))) => {
                                        if let Some(value) = text(
                                            ui,
                                            "value",
                                            &format!("{} · hex", property.name),
                                            &color.hex(),
                                            |text| {
                                                crate::color::Rgba::parse_hex(text.trim())
                                                    .is_none()
                                                    .then_some("Enter a hex color, such as #336699")
                                            },
                                        ) {
                                            if let Some(color) =
                                                crate::color::Rgba::parse_hex(value.trim())
                                            {
                                                edit = Some(Edit::Set {
                                                    id: property.id,
                                                    value: Value::Fill(Fill::Solid(color)),
                                                });
                                            } else {
                                                studio.status =
                                                    "Enter a hex color, such as #336699".into();
                                            }
                                        }
                                    }
                                    Ok(Value::Fill(_)) => {
                                        ui.label(&property.name);
                                        if super::keyboard_controls::button(
                                            ui,
                                            "edit-paint",
                                            "Edit paint on object",
                                        )
                                        .clicked()
                                        {
                                            select = components::property_object(
                                                &studio.doc,
                                                layer,
                                                root,
                                                property.id,
                                            )
                                            .ok();
                                        }
                                    }
                                    Ok(Value::Variant(mut current)) => {
                                        let old = current;
                                        let family = components::family_of(&studio.doc, current);
                                        let choices = components::definitions(&studio.doc)
                                            .into_iter()
                                            .filter(|(_, id, _, _)| {
                                                components::family_of(&studio.doc, *id) == family
                                            })
                                            .map(|(_, id, _, variant)| (id, variant))
                                            .collect::<Vec<_>>();
                                        super::keyboard_controls::choice(
                                            ui,
                                            "value",
                                            &property.name,
                                            &mut current,
                                            &choices,
                                        );
                                        if current != old {
                                            edit = Some(Edit::Set {
                                                id: property.id,
                                                value: Value::Variant(current),
                                            });
                                        }
                                    }
                                    Err(error) => {
                                        ui.label(&property.name);
                                        ui.colored_label(ui.visuals().error_fg_color, error);
                                    }
                                }
                                if main {
                                    if let Some(name) = text(
                                        ui,
                                        "name",
                                        "Property name",
                                        &property.name,
                                        property_name_error,
                                    ) {
                                        edit = Some(Edit::Rename {
                                            id: property.id,
                                            name,
                                        });
                                    }
                                    if super::keyboard_controls::button(
                                        ui,
                                        "remove",
                                        &format!("Remove {}", property.name),
                                    )
                                    .clicked()
                                    {
                                        edit = Some(Edit::Remove { id: property.id });
                                    }
                                }
                            });
                        }
                        if main {
                            ui.separator();
                            ui.label("Expose selected object field");
                            let key = ui.make_persistent_id("new-name");
                            let mut name =
                                ui.data(|d| d.get_temp::<String>(key)).unwrap_or_default();
                            let label = ui.label("New property name");
                            let response = ui
                                .add(egui::TextEdit::singleline(&mut name).id(key))
                                .labelled_by(label.id);
                            super::welcome::name_control(
                                ui,
                                &response,
                                label.id,
                                "New property name",
                            );
                            if !name.is_empty()
                                && let Some(message) = property_name_error(&name)
                            {
                                super::welcome::control_error(ui, &[&response], message);
                            }
                            ui.data_mut(|d| d.insert_temp(key, name.clone()));
                            if let Some(shape) = studio.doc.find_shape(layer, selected) {
                                let mut kinds = vec![Kind::Visibility, Kind::Fill, Kind::Opacity];
                                if matches!(shape.geom, Geom::Text(_)) {
                                    kinds.insert(0, Kind::Text);
                                }
                                if matches!(
                                    shape.layout.component,
                                    Some(ComponentBinding::Instance { .. })
                                ) {
                                    kinds.push(Kind::Variant);
                                }
                                for kind in kinds {
                                    if ui
                                        .add_enabled_ui(
                                            property_name_error(&name).is_none(),
                                            |ui| {
                                                super::keyboard_controls::button(
                                                    ui,
                                                    kind.name(),
                                                    &format!("Expose {}", kind.name()),
                                                )
                                            },
                                        )
                                        .inner
                                        .clicked()
                                    {
                                        edit = Some(Edit::Expose {
                                            object: selected,
                                            name: name.clone(),
                                            kind,
                                        });
                                    }
                                }
                            }
                        }
                    },
                );
            });
        });
    if let Some(edit) = edit {
        studio.apply_component_property(target, edit);
    }
    if let Some(id) = select {
        studio.selection = vec![(layer, id)];
    }
}
