//! Optional source and placement controls for retained vector patterns.
use crate::{document::Fill, svg_source::SvgPattern};
use eframe::egui::{self, Ui};

#[derive(Clone)]
pub(super) struct SourceOwner {
    revision: u64,
    pub(super) instance: egui::Id,
}

/// Resolve an opaque UI lifetime for an existing retained-source owner.
/// Args: UI, semantic document/target key and current revision. Returns a stable draft instance.
pub(super) fn source_owner(ui: &Ui, key: egui::Id, revision: u64) -> SourceOwner {
    let mut owner = ui
        .data(|data| data.get_temp::<SourceOwner>(key))
        .unwrap_or_else(|| SourceOwner {
            revision,
            instance: egui::Id::new(uuid::Uuid::new_v4()),
        });
    if owner.revision != revision {
        owner = SourceOwner {
            revision,
            instance: egui::Id::new(uuid::Uuid::new_v4()),
        };
    }
    owner
}

/// Advance a retained-source draft after its own accepted typed command.
/// Args: UI, semantic key, draft lifetime and current revision. Returns nothing.
pub(super) fn retain_owner(ui: &Ui, key: egui::Id, mut owner: SourceOwner, revision: u64) {
    owner.revision = revision;
    ui.data_mut(|data| data.insert_temp(key, owner));
}

/// Decorate a retained-source native widget without replacing its value or command route.
/// Args: UI, native owner, purpose and widget. Returns its actual native response.
pub(super) fn control(
    ui: &mut Ui,
    owner: egui::Id,
    name: &str,
    widget: impl FnOnce(&mut Ui) -> egui::Response,
) -> egui::Response {
    let response = super::fields::native_scope(ui, owner, widget).inner;
    ui.ctx()
        .accesskit_node_builder(response.id, |node| node.set_label(name));
    super::welcome::focus_control(ui, &response);
    if response.gained_focus() {
        response.scroll_to_me_animation(None, egui::style::ScrollAnimation::none());
    }
    response
}

/// Name an existing precise pattern scalar and observe only accepted native input.
/// Args: UI, child salt, purpose, value and existing optional range. Returns accepted changed.
fn number(
    ui: &mut Ui,
    salt: &str,
    name: &str,
    value: &mut f32,
    range: Option<std::ops::RangeInclusive<f32>>,
) -> bool {
    let before = *value;
    let owner = ui.make_persistent_id(salt);
    let attempt = std::cell::RefCell::new(None);
    let response = control(ui, owner, name, |ui| {
        let mut widget = egui::DragValue::new(value)
            .speed(0.01)
            .custom_formatter(|value, _| (value as f32).to_string())
            .custom_parser(|text| super::welcome::parse_number(&attempt, text));
        if let Some(range) = range.clone() {
            widget = widget.range(range);
        }
        ui.add(widget)
    });
    let valid = super::welcome::native_number_valid(
        ui,
        &response,
        owner,
        attempt.into_inner(),
        *value as f64,
        range.map_or(f64::NEG_INFINITY..=f64::INFINITY, |range| {
            f64::from(*range.start())..=f64::from(*range.end())
        }),
    );
    let cancelled = (response.has_focus() || response.lost_focus())
        && ui.input(|input| input.key_pressed(egui::Key::Escape));
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
    super::keyboard_controls::number_focus(ui, &response);
    let changed = response.enabled() && valid && !cancelled && response.changed();
    crate::app::input_history::record(response, changed)
}

pub(super) fn editor(ui: &mut Ui, pattern: &mut SvgPattern, instance: egui::Id) -> bool {
    let mut changed = false;
    ui.collapsing("Vector pattern", |ui| {
      ui.push_id(instance, |ui| {
        ui.horizontal(|ui| {
            ui.label("Opacity");
            changed |= number(ui, "pattern-opacity", "Pattern opacity", &mut pattern.opacity, Some(0.0..=1.0));
        });
        for (pair_index, (label, pair)) in ["Axis X", "Axis Y", "Origin"].into_iter().zip(pattern.transform.chunks_mut(2)).enumerate() {
            ui.horizontal(|ui| {
                ui.label(label);
                for (component, value) in pair.iter_mut().enumerate() {
                    let axis = ['a', 'b', 'c', 'd', 'e', 'f'][pair_index * 2 + component];
                    changed |= number(ui, &format!("pattern-{axis}"), &format!("Pattern transform {axis}"), value, None);
                }
            });
        }
        let id = ui.make_persistent_id("pattern-source");
        let mut source = ui.data_mut(|d| d.get_temp::<String>(id)).unwrap_or_else(|| pattern.definitions.to_string());
        let label = ui.label("Vector definitions");
        let source_response = control(ui, id.with("editor"), "Vector definitions", |ui| ui.add(egui::TextEdit::multiline(&mut source).id(id.with("editor")).code_editor().desired_rows(8)).labelled_by(label.id));
        if source_response.changed() { ui.data_mut(|d| d.insert_temp(id, source.clone())); }
        let apply = control(ui, id.with("apply"), "Apply vector source", |ui| ui.button("Apply vector source"));
        if apply.clicked() {
            let candidate = format!("<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"10\" height=\"10\"><defs>{source}</defs><rect width=\"10\" height=\"10\" fill=\"url(#{})\"/></svg>", pattern.id);
            match usvg::Tree::from_str(&candidate, &crate::svg_source::options()) {
                Ok(tree) if tree.patterns().iter().any(|p| p.id() == pattern.id) => {
                    pattern.definitions = crate::svg_source::canonicalize(&source).unwrap_or_default().into();
                    ui.data_mut(|d| d.remove::<String>(id.with("error")));
                    changed = true;
                }
                _ => { ui.data_mut(|d| d.insert_temp(id.with("error"), "Keep a valid pattern with the same ID.".to_string())); },
            }
        }
        if let Some(error) = ui.data(|d| d.get_temp::<String>(id.with("error"))) {
            super::fields::native_scope(ui, id.with("error"), |ui| super::welcome::control_error(ui, &[&source_response, &apply], &error));
        }
      });
    });
    changed
}

pub(super) fn show(ui: &mut Ui, studio: &mut crate::app::Studio) {
    if !studio.fill_active {
        return;
    }
    let Fill::Pattern(mut pattern) = super::studios::inspected_style(studio).fill.clone() else {
        return;
    };
    let key = egui::Id::new((
        "svg-pattern-owner",
        &studio.swap_id,
        &studio.selection,
        studio.active_layer,
    ));
    let owner = source_owner(ui, key, studio.canvas_gen);
    studio.with_field_history(ui, "svg-pattern", |ui, studio| {
        let changed = ui
            .push_id(studio.primary(), |ui| {
                editor(ui, &mut pattern, owner.instance)
            })
            .inner;
        if changed {
            studio.set_fill(Fill::Pattern(pattern));
        }
    });
    retain_owner(ui, key, owner, studio.canvas_gen);
}
