//! Docked editor panes. Workspace changes never enter document history.
use super::workspace::{Group, Place, Workspace};
use crate::{app::Studio, tools::Persona};
use eframe::egui::{self, Ui};

pub(super) const PANELS: &[(&str, &str)] = &[
    ("transform", "Transform"),
    ("align", "Align"),
    ("pathfinder", "Pathfinder"),
    ("appearance", "Appearance"),
    ("color", "Color"),
    ("color-guide", "Color Guide"),
    ("color-themes", "Color Themes"),
    ("recolor", "Recolor Artwork"),
    ("swatches", "Swatches"),
    ("stroke", "Stroke"),
    ("gradients", "Gradients"),
    ("patterns", "Patterns"),
    ("styles", "Styles"),
    ("symbols", "Symbols"),
    ("layers", "Layers"),
    ("paths", "Paths"),
    ("channels", "Channels"),
    ("navigator", "Navigator"),
    ("history", "History"),
    ("actions", "Actions"),
    ("shapes", "Shapes"),
    ("brushes", "Brushes"),
    ("character", "Character"),
    ("paragraph", "Paragraph"),
    ("opentype", "OpenType"),
    ("glyphs", "Glyphs"),
    ("typography", "Typography"),
    ("assets", "Assets"),
    ("links", "Links"),
    ("artboards", "Artboards"),
    ("info", "Info"),
    ("transparency", "Transparency"),
    ("effects", "Effects"),
    ("trace", "Image Trace"),
    ("mask", "Masks"),
    ("selection", "Selection"),
    ("layout", "Auto Layout"),
    ("hierarchy", "Hierarchy"),
    ("components", "Components"),
    ("variables", "Variables"),
    ("prototype", "Prototype"),
    ("timeline", "Timeline"),
    ("keyframes", "Keyframes"),
    ("compositions", "Compositions"),
    ("scene3d", "3D Scene"),
    ("photo-library", "Photo Library"),
    ("photo-develop", "Develop"),
    ("histogram", "Histogram"),
    ("presets", "Presets"),
    ("adjustments", "Adjustments"),
];

fn task(persona: Persona) -> &'static str {
    match persona {
        Persona::Layout => "Layout",
        Persona::Pixel => "Pixel",
        Persona::Motion => "Motion",
        Persona::Photo => "Photo",
        _ => "Design",
    }
}

fn group(panels: &[&str], place: Place, extent: f32, weight: f32) -> Group {
    Group {
        panels: panels.iter().map(|id| (*id).into()).collect(),
        place,
        extent,
        weight,
        ..Default::default()
    }
}

fn preset(name: &str) -> Workspace {
    let right = |panels: &[&str], weight| group(panels, Place::Right, 320.0, weight);
    let groups = match name {
        "Pixel" => vec![
            right(&["color", "swatches", "gradients", "patterns"], 0.25),
            right(&["brushes", "transform", "adjustments"], 0.25),
            right(&["layers", "channels", "paths"], 0.5),
        ],
        "Layout" => vec![
            right(&["layout", "transform", "align"], 0.4),
            right(&["appearance", "stroke", "variables"], 0.2),
            right(&["layers", "hierarchy", "components"], 0.4),
        ],
        "Motion" => vec![
            right(&["transform", "keyframes", "appearance"], 0.4),
            right(&["layers", "compositions", "scene3d"], 0.6),
            group(&["timeline"], Place::Bottom, 220.0, 1.0),
        ],
        "Photo" => vec![
            group(&["photo-library"], Place::Left, 200.0, 1.0),
            right(&["photo-develop", "presets"], 0.75),
            right(&["histogram", "history"], 0.25),
        ],
        _ => vec![
            right(&["transform", "align", "pathfinder"], 0.3),
            right(&["appearance", "stroke", "color", "swatches"], 0.35),
            right(&["layers", "paths", "channels"], 0.5),
        ],
    };
    Workspace {
        groups,
        ..Default::default()
    }
}

fn prepare(book: &mut super::workspace::Book, persona: Persona) {
    book.active = task(persona).into();
    for name in ["Design", "Pixel", "Layout", "Motion", "Photo"] {
        let layout = book
            .tasks
            .entry(name.into())
            .or_insert_with(|| preset(name));
        if layout
            .groups
            .iter()
            .flat_map(|g| &g.panels)
            .any(|p| p == "properties")
        {
            *layout = preset(name);
        }
        layout.normalize(&PANELS.iter().map(|p| p.0).collect::<Vec<_>>());
    }
}

/// Open and focus one real editor pane.
/// Args: context, current editor and registered pane. Returns false for unknown panes.
pub(super) fn open(ctx: &egui::Context, studio: &mut Studio, panel: &str) -> bool {
    if !PANELS.iter().any(|p| p.0 == panel) {
        return false;
    }
    let key=egui::Id::new(("workspace-open",&studio.swap_id,task(studio.persona)));
    let mut requests=ctx.data(|d|d.get_temp::<Vec<String>>(key)).unwrap_or_default();
    requests.retain(|id|id!=panel);
    requests.push(panel.into());
    ctx.data_mut(|d|d.insert_temp(key,requests));
    ctx.request_repaint();
    true
}

fn apply_open_requests(ctx:&egui::Context,studio:&Studio,book:&mut super::workspace::Book) {
    let key=egui::Id::new(("workspace-open",&studio.swap_id,task(studio.persona)));
    if let Some(requests)=ctx.data_mut(|d|d.remove_temp::<Vec<String>>(key)) {
        let layout=book.tasks.get_mut(task(studio.persona)).unwrap();
        for panel in requests {layout.restore(&panel);layout.request_focus(ctx,"oma-workspace",&panel);}
        ctx.request_repaint();
    }
}

/// Draw the native Window menu with working pane and recovery commands.
/// Args: menu UI and editor. Returns nothing; only workspace preferences are changed.
pub(super) fn menu(ui: &mut Ui, studio: &mut Studio) {
    ui.menu_button("Window", |ui| {
        if ui.button("Reset workspace").clicked() {
            studio
                .panel_workspaces
                .tasks
                .insert(task(studio.persona).into(), preset(task(studio.persona)));
            studio.save_panel_workspaces();
            ui.close();
        }
        if ui.button("Arrange panels…   Ctrl+F6").clicked() {
            prepare(&mut studio.panel_workspaces, studio.persona);
            super::panel_keyboard::open(ui.ctx(), &studio.panel_workspaces, PANELS);
            ui.close();
        }
        ui.separator();
        egui::ScrollArea::vertical()
            .max_height(ui.available_height().min(520.0))
            .show(ui, |ui| {
                for &(id, label) in PANELS {
                    if ui.button(label).clicked() {
                        open(ui.ctx(), studio, id);
                        ui.close();
                    }
                }
            });
    });
}

/// Draw editor panes around the current canvas.
/// Args: root UI and editor. Returns nothing; document commands remain app-owned.
pub(super) fn show(ui: &mut Ui, studio: &mut Studio) {
    let center = panels(ui, studio);
    ui.scope_builder(egui::UiBuilder::new().max_rect(center), |ui| {
        super::canvas::show(ui, studio)
    });
}

/// Draw persistent pane groups around an application viewer.
/// Args: root UI and editor. Returns the visible canvas rectangle.
pub(super) fn panels(ui: &mut Ui, studio: &mut Studio) -> egui::Rect {
    let mut book = std::mem::take(&mut studio.panel_workspaces);
    let before = book.clone();
    prepare(&mut book, studio.persona);
    apply_open_requests(ui.ctx(),studio,&mut book);
    if let Some(request) = super::panel_keyboard::show(ui, &mut book, PANELS) {
        let layout = book.tasks.get_mut(task(studio.persona)).unwrap();
        if let Some((panel, size)) = request.size {
            layout.request_floating_size(ui.ctx(), "oma-workspace", &panel, size);
        }
        if let Some(panel) = request.focus {
            layout.request_focus(ui.ctx(), "oma-workspace", &panel);
        }
    }
    let center = book.tasks.get_mut(task(studio.persona)).unwrap().show(
        ui,
        "oma-workspace",
        PANELS,
        |ui, panel| {
            if panel == "layers" {
                super::studios::layers_content(ui, studio);
            } else {
                egui::ScrollArea::vertical()
                    .min_scrolled_height(0.0)
                    .id_salt(("pane-body", panel))
                    .auto_shrink([false, false])
                    .show(ui, |ui| super::panes::show(ui, studio, panel));
            }
        },
    );
    apply_open_requests(ui.ctx(),studio,&mut book);
    let changed = before != book;
    studio.panel_workspaces = book;
    let dirty_key = egui::Id::new("oma-workspaces-dirty");
    let dirty = ui
        .ctx()
        .data(|d| d.get_temp::<bool>(dirty_key))
        .unwrap_or(false)
        || changed;
    if dirty && !ui.input(|i| i.pointer.any_down()) {
        studio.save_panel_workspaces();
        ui.ctx().data_mut(|d| d.insert_temp(dirty_key, false));
    } else if dirty {
        ui.ctx().data_mut(|d| d.insert_temp(dirty_key, true));
    }
    center
}

fn photo_preset() -> Workspace {
    preset("Photo")
}

pub(super) fn return_focus(ctx: &egui::Context, studio: &mut Studio) {
    prepare(&mut studio.panel_workspaces, studio.persona);
    if let Some(layout) = studio.panel_workspaces.tasks.get_mut(task(studio.persona)) {
        if let Some(panel) = layout.focus.clone() {
            layout.request_focus(ctx, "oma-workspace", &panel);
        }
    }
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
        assert_eq!(studio.persona, crate::tools::Persona::Pixel);
        assert_eq!(studio.panel_workspaces.active, "Pixel");
    }
}

#[cfg(test)]
mod human_qa_tests {
    use super::*;
    fn frame(ctx:&egui::Context, layout:&mut Workspace, size:egui::Vec2, events:Vec<egui::Event>)->egui::FullOutput {
        crate::ui::shared_fields_tests::headless(ctx,egui::RawInput{screen_rect:Some(egui::Rect::from_min_size(egui::Pos2::ZERO,size)),events,..Default::default()},|ui| {
            let center=layout.show(ui,"human-qa-dock",PANELS,|ui,_|{ui.label("Actual pane body");});
            assert!(center.width()>0.0&&center.height()>0.0);
        })
    }
    #[test]
    fn human_qa_docks_survive_shrink_and_recover_saved_sizes() {
        let ctx=egui::Context::default();
        let mut layout=preset("Motion");
        let original=layout.groups.clone();
        for size in [egui::vec2(1600.0,1000.0),egui::vec2(320.0,220.0),egui::vec2(1600.0,1000.0)] {
            frame(&ctx,&mut layout,size,vec![]);
            for (before,after) in original.iter().zip(&layout.groups) {
                assert_eq!(before.place,after.place);
                assert_eq!(before.extent,after.extent);
                assert_eq!(before.weight,after.weight);
            }
        }
        let encoded=serde_json::to_vec(&layout).unwrap();
        assert_eq!(layout,serde_json::from_slice(&encoded).unwrap());
    }
    #[test]
    fn human_qa_native_tab_drag_nests_and_reopens() {
        let ctx=egui::Context::default();ctx.enable_accesskit();
        let mut layout=Workspace::preset(&[("layers",Place::Left),("color",Place::Right)]);
        let size=egui::vec2(1200.0,800.0);
        frame(&ctx,&mut layout,size,vec![]);
        let output=frame(&ctx,&mut layout,size,vec![]);
        let center=|name| {
            let (_,node)=crate::ui::welcome::keyboard_tests::named(&output,name);
            let r=node.bounds().unwrap();egui::pos2(((r.x0+r.x1)*0.5) as f32,((r.y0+r.y1)*0.5) as f32)
        };
        let from=center("Layers");let to=center("Color");
        let pointer=|pos,pressed|egui::Event::PointerButton{pos,button:egui::PointerButton::Primary,pressed,modifiers:egui::Modifiers::NONE};
        frame(&ctx,&mut layout,size,vec![egui::Event::PointerMoved(from),pointer(from,true)]);
        frame(&ctx,&mut layout,size,vec![egui::Event::PointerMoved(from+egui::vec2(20.0,0.0))]);
        frame(&ctx,&mut layout,size,vec![egui::Event::PointerMoved(to)]);
        frame(&ctx,&mut layout,size,vec![pointer(to,false)]);
        assert_eq!(layout.groups.len(),1,"actual tab drag must nest panes");
        assert!(layout.groups[0].panels.iter().any(|p|p=="layers"));
        assert!(layout.groups[0].panels.iter().any(|p|p=="color"));
        let reopened:Workspace=serde_json::from_slice(&serde_json::to_vec(&layout).unwrap()).unwrap();
        assert_eq!(layout,reopened);
    }
}

#[cfg(test)]
mod split_tests {
    use super::*;
    #[test]
    fn human_qa_bottom_dock_drop_splits_along_its_real_horizontal_axis() {
        let ctx=egui::Context::default();ctx.enable_accesskit();
        let mut layout=Workspace::preset(&[("color",Place::Left),("timeline",Place::Bottom)]);
        let mut areas=std::collections::HashMap::new();
        let mut frame=|events|crate::ui::shared_fields_tests::headless(&ctx,egui::RawInput{screen_rect:Some(egui::Rect::from_min_size(egui::Pos2::ZERO,egui::vec2(1200.0,800.0))),events,..Default::default()},|ui|{
            layout.show(ui,"split-test",PANELS,|ui,panel|{areas.insert(panel.to_owned(),ui.available_rect_before_wrap());ui.label(panel);});
        });
        frame(vec![]);let output=frame(vec![]);
        let (_,node)=crate::ui::welcome::keyboard_tests::named(&output,"Color");let r=node.bounds().unwrap();let from=egui::pos2(((r.x0+r.x1)*0.5) as f32,((r.y0+r.y1)*0.5) as f32);
        drop(frame);let target=areas["timeline"];let to=egui::pos2(target.left()+16.0,target.center().y);
        let pointer=|pos,pressed|egui::Event::PointerButton{pos,button:egui::PointerButton::Primary,pressed,modifiers:egui::Modifiers::NONE};
        for events in [vec![egui::Event::PointerMoved(from),pointer(from,true)],vec![egui::Event::PointerMoved(from+egui::vec2(20.0,0.0))],vec![egui::Event::PointerMoved(to)],vec![pointer(to,false)],vec![]] {
            let _=crate::ui::shared_fields_tests::headless(&ctx,egui::RawInput{screen_rect:Some(egui::Rect::from_min_size(egui::Pos2::ZERO,egui::vec2(1200.0,800.0))),events,..Default::default()},|ui|{layout.show(ui,"split-test",PANELS,|ui,panel|{areas.insert(panel.to_owned(),ui.available_rect_before_wrap());ui.label(panel);});});
        }
        assert_eq!(layout.groups.len(),2);assert!(layout.groups.iter().all(|group|group.place==Place::Bottom));
        let color=areas["color"];let timeline=areas["timeline"];assert!(color.right()<=timeline.left());assert_eq!(color.top(),timeline.top());assert_eq!(color.bottom(),timeline.bottom());
        assert_eq!(layout,serde_json::from_slice(&serde_json::to_vec(&layout).unwrap()).unwrap());
    }
}

#[cfg(test)]
mod tab_visibility_tests {
    use super::*;
    #[test]
    fn human_qa_opening_a_far_tab_reveals_its_actual_native_control() {
        let ctx=egui::Context::default();ctx.enable_accesskit();
        let ids=["transform","align","pathfinder","appearance","color","swatches","stroke","gradients","patterns","transparency"];
        let mut layout=Workspace{groups:vec![group(&ids,Place::Right,320.0,1.0)],..Default::default()};
        let mut output=None;
        for step in 0..5 {
            if step==1 {assert!(layout.request_focus(&ctx,"tab-reveal","transparency"));}
            output=Some(crate::ui::shared_fields_tests::headless(&ctx,egui::RawInput{screen_rect:Some(egui::Rect::from_min_size(egui::Pos2::ZERO,egui::vec2(1200.0,800.0))),..Default::default()},|ui|{layout.show(ui,"tab-reveal",PANELS,|ui,_|{ui.label("Pane body");});}));
        }
        let output=output.unwrap();let (id,node)=crate::ui::welcome::keyboard_tests::named(&output,"Transparency");
        assert_eq!(ctx.memory(|m|m.focused()).map(|id|id.accesskit_id()),Some(id));
        let bounds=node.bounds().unwrap();assert!(bounds.x0>=880.0&&bounds.x1<=1200.0,"the actual active tab must be visible: {bounds:?}");
    }
}
