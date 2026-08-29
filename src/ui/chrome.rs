use crate::app::Studio;
use crate::boolean::BoolOp;
use crate::tools::{Persona, Tool};
use crate::ui::icons;
use crate::ui::theme::{accent, accent_dim, fg_weak};
use eframe::egui::{vec2, Button, Color32, Layout, Panel, RichText, Ui};

pub fn top_bar(ui: &mut Ui, studio: &mut Studio) {
    Panel::top("top").exact_size(40.0).show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.add_space(6.0);
            ui.label(RichText::new("omadesign").strong().color(accent()));
            ui.separator();
            ui.menu_button("File", |ui| {
                if ui.button("New…          Ctrl+N").clicked() {
                    studio.show_welcome = true;
                    ui.close();
                }
                if ui.button("Open…         Ctrl+O").clicked() {
                    studio.open();
                    ui.close();
                }
                if ui.button("Save           Ctrl+S").clicked() {
                    studio.save();
                    ui.close();
                }
                ui.separator();
                if ui.button("Export PNG…    Ctrl+E").clicked() {
                    studio.export_png();
                    ui.close();
                }
                if ui.button("Export JPEG…").clicked() {
                    studio.export_jpeg();
                    ui.close();
                }
                if ui.button("Export SVG…").clicked() {
                    studio.export_svg();
                    ui.close();
                }
            });
            ui.menu_button("Edit", |ui| {
                if ui
                    .add_enabled(studio.history.can_undo(), Button::new("Undo   Ctrl+Z"))
                    .clicked()
                {
                    studio.undo();
                    ui.close();
                }
                if ui
                    .add_enabled(
                        studio.history.can_redo(),
                        Button::new("Redo   Ctrl+Shift+Z"),
                    )
                    .clicked()
                {
                    studio.redo();
                    ui.close();
                }
                ui.separator();
                if ui.button("Duplicate   Ctrl+D").clicked() {
                    studio.duplicate_selection();
                    ui.close();
                }
                if ui.button("Delete").clicked() {
                    studio.delete_selection();
                    ui.close();
                }
            });
            ui.menu_button("Object", |ui| {
                ui.label(RichText::new("Boolean").small().color(fg_weak()));
                for op in BoolOp::all() {
                    if ui.button(op.name()).clicked() {
                        studio.apply_boolean(op);
                        ui.close();
                    }
                }
                ui.separator();
                ui.label(RichText::new("Compound").small().color(fg_weak()));
                if ui.button("Combine  Ctrl+E").clicked() {
                    studio.combine_selected();
                    ui.close();
                }
                if ui.button("Release  Ctrl+Shift+E").clicked() {
                    studio.release_compound();
                    ui.close();
                }
                ui.separator();
                if ui.button("Align left").clicked() {
                    studio.align_sel(crate::align::Align::Left);
                    ui.close();
                }
                if ui.button("Align centre").clicked() {
                    studio.align_sel(crate::align::Align::CenterX);
                    ui.close();
                }
                if ui.button("Align right").clicked() {
                    studio.align_sel(crate::align::Align::Right);
                    ui.close();
                }
                if ui.button("Align top").clicked() {
                    studio.align_sel(crate::align::Align::Top);
                    ui.close();
                }
                if ui.button("Align middle").clicked() {
                    studio.align_sel(crate::align::Align::CenterY);
                    ui.close();
                }
                if ui.button("Align bottom").clicked() {
                    studio.align_sel(crate::align::Align::Bottom);
                    ui.close();
                }
                ui.separator();
                if ui.button("Distribute horizontally").clicked() {
                    studio.distribute_sel(crate::align::Distribute::Horizontal);
                    ui.close();
                }
                if ui.button("Distribute vertically").clicked() {
                    studio.distribute_sel(crate::align::Distribute::Vertical);
                    ui.close();
                }
            });
            ui.menu_button("View", |ui| {
                if ui.button("Fit     Ctrl+0").clicked() {
                    studio.need_fit = true;
                    ui.close();
                }
                if ui.button("100%    Ctrl+1").clicked() {
                    studio.view.scale = 1.0;
                    ui.close();
                }
                ui.separator();
                ui.checkbox(&mut studio.show_rulers, "Rulers");
                ui.checkbox(&mut studio.doc.grid.visible, "Grid");
                ui.checkbox(&mut studio.snap.enabled, "Snapping");
                ui.checkbox(&mut studio.snap.grid, "Snap to grid");
                ui.checkbox(&mut studio.snap.guides, "Snap to guides");
                ui.checkbox(&mut studio.snap.objects, "Snap to objects");
                ui.separator();
                if ui.button("Keyboard shortcuts   F1").clicked() {
                    studio.show_shortcuts = true;
                    ui.close();
                }
            });

            ui.separator();
            for p in [Persona::Design, Persona::Pixel, Persona::Photo] {
                let on = studio.persona == p;
                let btn = Button::new(RichText::new(p.name()).strong()).fill(if on {
                    accent_dim()
                } else {
                    Color32::TRANSPARENT
                });
                if ui
                    .add_sized(vec2(72.0, 26.0), btn)
                    .on_hover_text(p.hint())
                    .clicked()
                {
                    studio.commit_type_edit();
                    studio.persona = p;
                    studio.op = None;
                    studio.tool = match p {
                        Persona::Design => Tool::Select,
                        Persona::Pixel => Tool::Brush,
                        Persona::Photo => Tool::Hand,
                    };
                    studio.show_welcome = false;
                }
            }

            ui.separator();
            if studio.persona != Persona::Photo {
                if ui
                    .add_enabled(studio.history.can_undo(), Button::new("Undo"))
                    .clicked()
                {
                    studio.undo();
                }
                if ui
                    .add_enabled(studio.history.can_redo(), Button::new("Redo"))
                    .clicked()
                {
                    studio.redo();
                }
                eframe::egui::ComboBox::from_id_salt("export-scale")
                    .selected_text(format!("{}×", studio.export_scale))
                    .width(48.0)
                    .show_ui(ui, |ui| {
                        for s in [1u32, 2, 3] {
                            ui.selectable_value(&mut studio.export_scale, s, format!("{s}×"));
                        }
                    });
            } else if ui.button("Place in Design").clicked() {
                studio.send_photo_to_design();
            }

            ui.with_layout(Layout::right_to_left(eframe::egui::Align::Center), |ui| {
                if ui.small_button("Fit").clicked() {
                    studio.need_fit = true;
                }
                ui.label(
                    RichText::new(format!("{:.0}%", studio.view.scale * 100.0))
                        .small()
                        .color(fg_weak()),
                );
                if studio.dirty {
                    ui.label(RichText::new("•").color(accent()));
                }
                ui.label(RichText::new(&studio.doc.name).small());
            });
        });
    });
}

pub fn left_toolbar(ui: &mut Ui, studio: &mut Studio) {
    Panel::left("tools")
        .resizable(false)
        .exact_size(48.0)
        .show(ui, |ui| {
            ui.add_space(8.0);
            let well = match studio.persona {
                Persona::Design => Tool::design_well(),
                Persona::Pixel => Tool::pixel_well(),
                Persona::Photo => Tool::photo_well(),
            };
            let mut last_group = "";
            for t in well.iter().copied() {
                let group = match t {
                    Tool::Select | Tool::Node => "sel",
                    Tool::Pen | Tool::Pencil => "path",
                    Tool::Rect | Tool::Ellipse | Tool::Polygon | Tool::Star | Tool::Line => "shape",
                    Tool::Text | Tool::Gradient | Tool::Eyedropper => "look",
                    Tool::Brush | Tool::Eraser | Tool::Fill | Tool::Clone | Tool::Smudge => "paint",
                    Tool::Marquee | Tool::EllipseMarquee | Tool::Lasso | Tool::Wand => "selpx",
                    Tool::Hand | Tool::Zoom | Tool::Crop => "nav",
                };
                if last_group != group && last_group != "" {
                    icons::well_separator(ui);
                }
                last_group = group;
                if icons::tool_button(ui, t, studio.tool == t) {
                    studio.set_tool(t);
                }
            }
        });
}

pub fn status_bar(ui: &mut Ui, studio: &mut Studio) {
    Panel::bottom("status").exact_size(22.0).show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.label(RichText::new(studio.tool.hint()).small().color(fg_weak()));
            ui.with_layout(Layout::right_to_left(eframe::egui::Align::Center), |ui| {
                if let Some(p) = studio.cursor {
                    ui.label(
                        RichText::new(format!("{:.0}  {:.0}", p.x, p.y))
                            .small()
                            .monospace()
                            .color(fg_weak()),
                    );
                }
                ui.label(RichText::new(&studio.status).small());
            });
        });
    });
}
