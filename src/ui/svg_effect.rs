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
    let key = egui::Id::new((
        "svg-effect-owner",
        &studio.swap_id,
        studio.doc.layers[index].id,
    ));
    let owner = super::svg_paint::source_owner(ui, key, studio.canvas_gen);
    let editable = studio.layer_unlocked(index) && studio.doc.layer_visible(index);
    ui.push_id(("svg-effect", studio.doc.layers[index].id), |ui| {
        ui.collapsing("SVG filter graph", |ui| {
          ui.push_id(owner.instance, |ui| {
            ui.label(format!("{} source filter(s), editable vector children", graph.filters.len()));
            for note in &graph.reductions { ui.label(note); }
            let id = ui.make_persistent_id("source");
            let mut source = ui.data(|d| d.get_temp::<String>(id)).unwrap_or_else(|| graph.definitions.to_string());
            let mut affected = Vec::new();
            ui.add_enabled_ui(editable, |ui| {
                let label = ui.label("Filter source");
                let response = super::svg_paint::control(ui, id.with("editor"), "Filter source", |ui| ui.add(egui::TextEdit::multiline(&mut source).id(id.with("editor")).code_editor().lock_focus(false).desired_rows(9).desired_width(ui.available_width().min((ui.clip_rect().right() - ui.next_widget_position().x).max(24.)))).labelled_by(label.id));
                if response.changed() { ui.data_mut(|d| d.insert_temp(id, source.clone())); }
                affected.push(response);
                let apply = super::svg_paint::control(ui, id.with("apply"), "Apply filter source", |ui| ui.button("Apply filter source"));
                let accepted = apply.clicked();
                affected.push(apply);
                if accepted && editable && ui.is_enabled() {
                    let filters = graph.filters.iter().map(|s| format!("url(#{s})")).collect::<Vec<_>>().join(" ");
                    let candidate = format!("<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"10\" height=\"10\"><defs>{source}</defs><g filter=\"{filters}\"><rect width=\"10\" height=\"10\"/></g></svg>");
                    if let Ok(tree) = usvg::Tree::from_str(&candidate, &crate::svg_source::options()) {
                        if graph.filters.iter().all(|id| tree.filters().iter().any(|f| f.id() == id)) {
                            graph.reductions = crate::svg_source::reductions(&candidate);
                            graph.definitions = crate::svg_source::canonicalize(&source).unwrap_or_default().into();
                            ui.data_mut(|d| d.remove::<String>(id.with("error")));
                            ui.data_mut(|d| d.remove::<bool>(id.with("reveal-error")));
                            changed = true;
                        }
                    }
                    if !changed { ui.data_mut(|d| {
                        d.insert_temp(id.with("error"), "Keep a valid graph with the same filter IDs.".to_string());
                        d.insert_temp(id.with("reveal-error"), true);
                    }); }
                }
            });
            if let Some(error) = ui.data(|d| d.get_temp::<String>(id.with("error"))) {
                let fields = affected.iter().collect::<Vec<_>>();
                super::fields::native_scope(ui, id.with("error"), |ui| {
                    ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
                    if ui.data(|d| d.get_temp::<bool>(id.with("reveal-error"))).unwrap_or(false) {
                        if super::welcome::reveal_control_error(ui, &fields, &error) {
                            ui.data_mut(|d| d.remove::<bool>(id.with("reveal-error")));
                        }
                    } else {
                        super::welcome::control_error(ui, &fields, &error);
                    }
                });
            }
            ui.collapsing("Original filter source", |ui| {
                let mut source = graph.original_definitions.as_ref();
                let original = id.with("original-source");
                super::svg_paint::control(ui, original, "Original filter source", |ui| ui.add(egui::TextEdit::multiline(&mut source).id(original).code_editor().lock_focus(false).desired_rows(6).interactive(false).desired_width(ui.available_width().min((ui.clip_rect().right() - ui.next_widget_position().x).max(24.)))));
            });
          });
        });
    });
    if changed {
        studio.commit(crate::document::Cmd::SetSvgFilter {
            index,
            before,
            after: Some(graph),
        });
    }
    super::svg_paint::retain_owner(ui, key, owner, studio.canvas_gen);
}
