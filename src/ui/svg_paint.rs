//! Optional source and placement controls for retained vector patterns.
use crate::{document::Fill, svg_source::SvgPattern};
use eframe::egui::{self, Ui};

pub(super) fn editor(ui: &mut Ui, pattern: &mut SvgPattern) -> bool {
    let mut changed = false;
    ui.collapsing("Vector pattern", |ui| {
        ui.horizontal(|ui| {
            ui.label("Opacity");
            changed |= ui.add(egui::DragValue::new(&mut pattern.opacity).range(0.0..=1.0).speed(0.01)).changed();
        });
        for (label, pair) in ["Axis X", "Axis Y", "Origin"].into_iter().zip(pattern.transform.chunks_mut(2)) {
            ui.horizontal(|ui| {
                ui.label(label);
                for value in pair { changed |= ui.add(egui::DragValue::new(value).speed(0.01)).changed(); }
            });
        }
        let id = ui.make_persistent_id("pattern-source");
        let mut source = ui.data_mut(|d| d.get_temp::<String>(id)).unwrap_or_else(|| pattern.definitions.to_string());
        ui.label("Vector definitions");
        if ui.add(egui::TextEdit::multiline(&mut source).code_editor().desired_rows(8)).changed() { ui.data_mut(|d| d.insert_temp(id, source.clone())); }
        if ui.button("Apply vector source").clicked() {
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
        if let Some(error) = ui.data(|d| d.get_temp::<String>(id.with("error"))) { ui.label(error); }
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
    let changed = ui
        .push_id(studio.primary(), |ui| editor(ui, &mut pattern))
        .inner;
    if changed {
        studio.set_fill(Fill::Pattern(pattern));
    }
}
