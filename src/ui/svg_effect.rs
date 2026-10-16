//! Edit retained SVG graph source without flattening its vector children.
use eframe::egui::{self, Ui};

pub(super) fn show(ui: &mut Ui, studio: &mut crate::app::Studio) {
    let Some(index) = std::iter::once(studio.active_layer.unwrap_or(0))
        .chain(studio.doc.layer_ancestors(studio.active_layer.unwrap_or(0)))
        .find(|&i| {
            studio
                .doc
                .layers
                .get(i)
                .is_some_and(|l| l.svg_filter.is_some())
        })
    else {
        return;
    };
    let before = studio.doc.layers[index].svg_filter.clone();
    let mut graph = before.clone().unwrap();
    let mut changed = false;
    ui.push_id(("svg-effect", studio.doc.layers[index].id), |ui| {
        ui.collapsing("SVG filter graph", |ui| {
            ui.label(format!("{} source filter(s), editable vector children", graph.filters.len()));
            for note in &graph.reductions { ui.label(note); }
            let id = ui.make_persistent_id("source");
            let mut source = ui.data(|d| d.get_temp::<String>(id)).unwrap_or_else(|| graph.definitions.to_string());
            ui.add_enabled_ui(studio.doc.layer_editable(index), |ui| {
                if ui.add(egui::TextEdit::multiline(&mut source).code_editor().desired_rows(9)).changed() { ui.data_mut(|d| d.insert_temp(id, source.clone())); }
                if ui.button("Apply filter source").clicked() {
                    let filters = graph.filters.iter().map(|s| format!("url(#{s})")).collect::<Vec<_>>().join(" ");
                    let candidate = format!("<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"10\" height=\"10\"><defs>{source}</defs><g filter=\"{filters}\"><rect width=\"10\" height=\"10\"/></g></svg>");
                    if let Ok(tree) = usvg::Tree::from_str(&candidate, &crate::svg_source::options()) {
                        if graph.filters.iter().all(|id| tree.filters().iter().any(|f| f.id() == id)) {
                            graph.reductions = crate::svg_source::reductions(&candidate);
                            graph.definitions = crate::svg_source::canonicalize(&source).unwrap_or_default().into();
                            ui.data_mut(|d| d.remove::<String>(id.with("error")));
                            changed = true;
                        }
                    }
                    if !changed { ui.data_mut(|d| d.insert_temp(id.with("error"), "Keep a valid graph with the same filter IDs.".to_string())); }
                }
            });
            if let Some(error) = ui.data(|d| d.get_temp::<String>(id.with("error"))) { ui.label(error); }
            ui.collapsing("Original filter source", |ui| { let mut source = graph.original_definitions.as_ref(); ui.add(egui::TextEdit::multiline(&mut source).code_editor().desired_rows(6).interactive(false)); });
        });
    });
    if changed {
        studio.commit(crate::document::Cmd::SetSvgFilter {
            index,
            before,
            after: Some(graph),
        });
    }
}
