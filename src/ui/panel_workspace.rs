//! Application panel registry. Canvas and document commands remain outside docking.
use super::workspace::{Place, Workspace};
use crate::app::Studio;
use eframe::egui::{self, Ui};

const PANELS: &[(&str, &str)] = &[
    ("properties", "Properties"),
    ("layers", "Layers"),
    ("timeline", "Timeline"),
    ("hierarchy", "Layout hierarchy"),
    ("photo-library", "Photo library"),
    ("photo-develop", "Photo develop"),
];

/// Draw task panels around the document canvas.
/// Args: root UI and studio. Returns no value; saved layout changes never enter artwork undo.
#[allow(deprecated)]
pub(super) fn show(ui: &mut Ui, studio: &mut Studio) {
    let center = panels(ui, studio);
    ui.scope_builder(egui::UiBuilder::new().max_rect(center), |ui| {
        super::canvas::show(ui, studio)
    });
}

/// Draw the registered panels around an app-owned viewer.
/// Args: root UI and studio. Returns the remaining center rectangle.
pub(super) fn panels(ui: &mut Ui, studio: &mut Studio) -> egui::Rect {
    let mut book = std::mem::take(&mut studio.panel_workspaces);
    let before = book.clone();
    let presets = [
        (
            "Design",
            Workspace::preset(&[("properties", Place::Right), ("layers", Place::Left)]),
        ),
        (
            "Layout",
            Workspace::preset(&[("properties", Place::Right), ("hierarchy", Place::Left)]),
        ),
        (
            "Pixel",
            Workspace::preset(&[("properties", Place::Right), ("layers", Place::Right)]),
        ),
        (
            "Motion",
            Workspace::preset(&[
                ("properties", Place::Right),
                ("layers", Place::Left),
                ("timeline", Place::Bottom),
            ]),
        ),
        ("Photo", photo_preset()),
    ];
    for (_, layout) in &mut book.tasks {
        layout.normalize(&PANELS.iter().map(|p| p.0).collect::<Vec<_>>());
    }
    let persona_key = egui::Id::new("oma-workspace-persona");
    let persona_name = match studio.persona {
        crate::tools::Persona::Layout => "Layout",
        crate::tools::Persona::Pixel => "Pixel",
        crate::tools::Persona::Motion => "Motion",
        crate::tools::Persona::Photo => "Photo",
        _ => "Design",
    };
    let previous_persona: Option<String> = ui.ctx().data_mut(|data| data.get_temp(persona_key));
    if previous_persona.as_deref() != Some(persona_name) {
        book.active = persona_name.into();
    }
    book.toolbar_for(ui, "oma-workspace", &presets, PANELS);
    let active_persona = match book.active.as_str() {
        "Layout" => crate::tools::Persona::Layout,
        "Pixel" => crate::tools::Persona::Pixel,
        "Motion" => crate::tools::Persona::Motion,
        "Photo" => crate::tools::Persona::Photo,
        _ => crate::tools::Persona::Design,
    };
    if studio.persona != active_persona {
        studio.switch_persona(active_persona);
    }
    ui.ctx()
        .data_mut(|data| data.insert_temp(persona_key, book.active.clone()));
    let active = book.active.clone();
    if let Some(request) = super::panel_keyboard::show(ui, &mut book, PANELS) {
        let layout = book.tasks.get_mut(&active).unwrap();
        layout.normalize(&PANELS.iter().map(|panel| panel.0).collect::<Vec<_>>());
        if let Some((panel, size)) = request.size {
            layout.request_floating_size(ui.ctx(), "oma-workspace", &panel, size);
        }
        if let Some(panel) = request.focus {
            layout.request_focus(ui.ctx(), "oma-workspace", &panel);
        }
    }
    let center =
        book.tasks
            .get_mut(&active)
            .unwrap()
            .show(ui, "oma-workspace", PANELS, |ui, panel| match panel {
                "properties" => super::studios::inspector_content(ui, studio),
                "layers" => super::studios::layers_content(ui, studio),
                "timeline" => super::timeline::content(ui, studio),
                "hierarchy" => super::layout::hierarchy_content(ui, studio),
                "photo-library" => super::photo::filmstrip(ui, studio),
                "photo-develop" => super::photo::develop_content(ui, studio),
                _ => {}
            });
    let changed = before != book;
    studio.panel_workspaces = book;
    let dirty_key = egui::Id::new("oma-workspaces-dirty");
    let dirty = ui
        .ctx()
        .data_mut(|data| data.get_temp::<bool>(dirty_key))
        .unwrap_or(false)
        || changed;
    if dirty && !ui.input(|input| input.pointer.any_down()) {
        studio.save_panel_workspaces();
        ui.ctx().data_mut(|data| data.insert_temp(dirty_key, false));
    } else if dirty {
        ui.ctx().data_mut(|data| data.insert_temp(dirty_key, true));
    }
    center
}

/// Keep the existing Photo panel widths in its workspace preset.
/// Args: none. Returns an ordinary workspace with the two Photo panels.
fn photo_preset() -> Workspace {
    let mut layout = Workspace::preset(&[
        ("photo-library", Place::Left),
        ("photo-develop", Place::Right),
    ]);
    for group in &mut layout.groups {
        group.extent = if group.panels.iter().any(|id| id == "photo-library") {
            200.0
        } else {
            288.0
        };
    }
    layout
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn shared_workspace_keeps_canvas_visible_at_all_scales() {
        for scale in [1.0, 1.5, 2.0] {
            let ctx = egui::Context::default();
            ctx.set_pixels_per_point(scale);
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1000.0 / scale, 700.0 / scale),
                )),
                ..Default::default()
            };
            let mut layout = Workspace::preset(&[
                ("properties", Place::Right),
                ("layers", Place::Left),
                ("timeline", Place::Bottom),
            ]);
            let _ = crate::ui::shared_fields_tests::headless(&ctx, input, |ui| {
                let center = layout.show(ui, "test", PANELS, |ui, _| {
                    ui.label("Panel body");
                });
                assert!(center.width() >= 240.0);
                assert!(center.height() >= 160.0);
            });
        }
    }
}

#[cfg(test)]
mod persistence_tests {
    use super::*;
    #[test]
    fn shared_workspaces_reopen_task_choices_and_keep_artwork_unchanged() {
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join(".artifacts")
            .join(format!("shared-ui-workspace-test-{}", std::process::id()));
        let path = root.join("workspaces.json");
        let mut book = super::super::workspace::Book::default();
        book.active = "Motion".into();
        let mut motion = Workspace::preset(&[
            ("properties", Place::Right),
            ("layers", Place::Left),
            ("timeline", Place::Bottom),
        ]);
        motion.move_panel("properties", Place::Floating);
        motion.tab_with("layers", "properties");
        motion.close("timeline");
        book.tasks.insert("Motion".into(), motion);
        book.tasks.insert(
            "Design".into(),
            Workspace::preset(&[("properties", Place::Right)]),
        );
        book.write(&path).unwrap();
        let mut reopened = super::super::workspace::Book::read(&path).unwrap();
        assert_eq!(book, reopened);
        reopened
            .tasks
            .get_mut("Motion")
            .unwrap()
            .restore("timeline");
        assert_eq!(reopened.tasks["Design"], book.tasks["Design"]);
        assert!(
            reopened.tasks["Motion"]
                .groups
                .iter()
                .any(|g| g.panels.iter().any(|p| p == "timeline"))
        );
        let mut studio = Studio::new();
        studio.doc = crate::document::Document::new("Workspace ownership", 100.0, 100.0, 72.0);
        studio.show_welcome = false;
        let before = crate::project::encode(&studio.doc).unwrap();
        let ctx = egui::Context::default();
        let _ = crate::ui::shared_fields_tests::headless(
            &ctx,
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1280.0, 800.0),
                )),
                ..Default::default()
            },
            |ui| show(ui, &mut studio),
        );
        assert_eq!(before, crate::project::encode(&studio.doc).unwrap());
        assert!(!studio.dirty);
        std::fs::remove_dir_all(root).unwrap();
    }
}

#[cfg(test)]
mod persona_tests {
    use super::*;
    #[test]
    fn shared_workspaces_follow_personas_and_preserve_layout_node_routes() {
        let ctx = egui::Context::default();
        let mut studio = Studio::new();
        studio.show_welcome = false;
        for (persona, name) in [
            (crate::tools::Persona::Layout, "Layout"),
            (crate::tools::Persona::Motion, "Motion"),
            (crate::tools::Persona::Pixel, "Pixel"),
        ] {
            studio.persona = persona;
            let _ = crate::ui::shared_fields_tests::headless(
                &ctx,
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1280.0, 800.0),
                    )),
                    ..Default::default()
                },
                |ui| show(ui, &mut studio),
            );
            assert_eq!(studio.panel_workspaces.active, name);
            assert_eq!(studio.persona, persona);
            if name == "Layout" {
                assert!(
                    studio.panel_workspaces.tasks[name]
                        .groups
                        .iter()
                        .any(|group| group.panels.iter().any(|panel| panel == "hierarchy"))
                );
            }
        }
        studio.panel_workspaces.active = "Design".into();
        let _ = crate::ui::shared_fields_tests::headless(
            &ctx,
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1280.0, 800.0),
                )),
                ..Default::default()
            },
            |ui| show(ui, &mut studio),
        );
        assert_eq!(studio.persona, crate::tools::Persona::Design);
    }
}
