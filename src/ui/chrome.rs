use crate::app::Studio;
use crate::boolean::BoolOp;
use crate::geom::Pt;
use crate::tools::{Persona, Tool};
use crate::ui::icons::{self, ph};
use crate::ui::theme::{accent, accent_dim, fg, fg_weak};
use eframe::egui::{vec2, Button, Color32, Layout, Panel, RichText, Ui};

pub fn top_bar(ui: &mut Ui, studio: &mut Studio) {
    Panel::top("top").exact_size(38.0).show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.add_space(8.0);
            ui.label(RichText::new("omadesign").strong().size(14.0).color(accent()));
            ui.add_space(4.0);
            file_menu(ui, studio);
            edit_menu(ui, studio);
            object_menu(ui, studio);
            arrange_menu(ui, studio);
            view_menu(ui, studio);

            ui.separator();
            persona_tabs(ui, studio);

            ui.separator();
            if icons::icon_button(
                ui,
                ph::SHAPES,
                "Shape library",
                studio.show_shape_browser,
            ) {
                studio.show_shape_browser = !studio.show_shape_browser;
            }
            if icons::icon_button(
                ui,
                ph::IMAGES,
                "Free photos",
                studio.show_asset_browser,
            ) {
                studio.show_asset_browser = !studio.show_asset_browser;
            }

            if studio.persona == Persona::Photo {
                ui.separator();
                if ui.button("Place in Design").clicked() {
                    studio.send_photo_to_design();
                }
            }

            ui.with_layout(Layout::right_to_left(eframe::egui::Align::Center), |ui| {
                if ui.small_button("Fit").on_hover_text("Ctrl+0").clicked() {
                    studio.need_fit = true;
                }
                eframe::egui::ComboBox::from_id_salt("zoom-preset")
                    .selected_text(format!("{:.0}%", studio.view.scale * 100.0))
                    .width(64.0)
                    .show_ui(ui, |ui| {
                        for z in [0.25, 0.5, 1.0, 1.5, 2.0, 4.0] {
                            if ui
                                .selectable_label(
                                    (studio.view.scale - z).abs() < 0.01,
                                    format!("{:.0}%", z * 100.0),
                                )
                                .clicked()
                            {
                                studio.view.scale = z;
                                studio.mark();
                            }
                        }
                    });
                if studio.dirty {
                    ui.label(RichText::new("•").color(accent()).size(18.0));
                }
                ui.label(RichText::new(&studio.doc.name).small().color(fg()));
            });
        });
    });
}

fn file_menu(ui: &mut Ui, studio: &mut Studio) {
    ui.menu_button("File", |ui| {
        if ui.button("New…                  Ctrl+N").clicked() {
            studio.new_tab_welcome();
            ui.close();
        }
        if ui.button("Open…                 Ctrl+O").clicked() {
            studio.open();
            ui.close();
        }
        ui.menu_button("Open recent", |ui| {
            if studio.recents.is_empty() {
                ui.label(RichText::new("No recent files").small().color(fg_weak()));
            }
            let recents = studio.recents.clone();
            for p in recents {
                let name = p
                    .file_name()
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_else(|| p.display().to_string());
                if ui.button(&name).on_hover_text(p.display().to_string()).clicked() {
                    studio.open_path(p);
                    ui.close();
                }
            }
        });
        ui.separator();
        if ui.button("Save                   Ctrl+S").clicked() {
            studio.save();
            ui.close();
        }
        if ui.button("Save as…         Ctrl+Shift+S").clicked() {
            studio.save_as();
            ui.close();
        }
        ui.separator();
        if ui.button("Place…           Ctrl+Shift+P").clicked() {
            studio.begin_place();
            ui.close();
        }
        ui.separator();
        ui.label(RichText::new("Export").small().color(fg_weak()));
        ui.horizontal(|ui| {
            ui.label(RichText::new("Scale").small().color(fg_weak()));
            for s in [1u32, 2, 3] {
                if ui
                    .selectable_label(studio.export_scale == s, format!("{s}×"))
                    .clicked()
                {
                    studio.export_scale = s;
                }
            }
        });
        if ui.button("Export PNG…            Ctrl+E").clicked() {
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
        if ui.button("Export animated SVG…").clicked() {
            studio.export_animated_svg();
            ui.close();
        }
        if ui.button("Export Lottie…").clicked() {
            studio.export_lottie();
            ui.close();
        }
        ui.separator();
        if ui.button("Import Lottie…").clicked() {
            studio.import_lottie();
            ui.close();
        }
    });
}

fn edit_menu(ui: &mut Ui, studio: &mut Studio) {
    ui.menu_button("Edit", |ui| {
        if ui
            .add_enabled(studio.history.can_undo(), Button::new("Undo                 Ctrl+Z"))
            .clicked()
        {
            studio.undo();
            ui.close();
        }
        if ui
            .add_enabled(
                studio.history.can_redo(),
                Button::new("Redo         Ctrl+Shift+Z"),
            )
            .clicked()
        {
            studio.redo();
            ui.close();
        }
        ui.separator();
        if ui.button("Cut                    Ctrl+X").clicked() {
            studio.cut_selection(ui.ctx());
            ui.close();
        }
        if ui.button("Copy                   Ctrl+C").clicked() {
            studio.copy_selection(ui.ctx());
            ui.close();
        }
        if ui.button("Paste                  Ctrl+V").clicked() {
            studio.paste_clipboard(None);
            ui.close();
        }
        if ui.button("Duplicate              Ctrl+D").clicked() {
            studio.duplicate_selection();
            ui.close();
        }
        if ui.button("Delete").clicked() {
            studio.delete_selection();
            ui.close();
        }
        ui.separator();
        if ui.button("Select all             Ctrl+A").clicked() {
            studio.selection = studio
                .doc
                .layers
                .iter()
                .enumerate()
                .flat_map(|(li, l)| {
                    let mut ids: Vec<(usize, u64)> = l
                        .kind
                        .shapes()
                        .into_iter()
                        .flatten()
                        .map(move |s| (li, s.id))
                        .collect();
                    if l.kind.is_placed_raster() {
                        ids.push((li, crate::document::RASTER_ID));
                    }
                    ids
                })
                .collect();
            ui.close();
        }
        ui.separator();
        if ui.button("Copy style         Ctrl+Alt+C").clicked() {
            studio.copy_style();
            ui.close();
        }
        if ui
            .add_enabled(
                studio.style_clip.is_some(),
                Button::new("Paste style        Ctrl+Alt+V"),
            )
            .clicked()
        {
            studio.paste_style();
            ui.close();
        }
    });
}

fn object_menu(ui: &mut Ui, studio: &mut Studio) {
    ui.menu_button("Object", |ui| {
        ui.label(RichText::new("Boolean").small().color(fg_weak()));
        for op in BoolOp::all() {
            if ui
                .add_enabled(studio.selection.len() >= 2, Button::new(op.name()))
                .clicked()
            {
                studio.apply_boolean(op);
                ui.close();
            }
        }
        ui.separator();
        if ui
            .add_enabled(
                studio.selection.len() >= 2,
                Button::new("Combine              Ctrl+G"),
            )
            .clicked()
        {
            studio.combine_selected();
            ui.close();
        }
        if ui
            .button("Release         Ctrl+Shift+G")
            .clicked()
        {
            studio.release_compound();
            ui.close();
        }
        ui.separator();
        if ui.button("Flip horizontal").clicked() {
            flip(studio, true);
            ui.close();
        }
        if ui.button("Flip vertical").clicked() {
            flip(studio, false);
            ui.close();
        }
        ui.separator();
        if ui.button("Convert to path").clicked() {
            if let Some((li, id)) = studio.primary() {
                studio.ensure_path(li, id);
            }
            ui.close();
        }
        if ui
            .button("Trace to vector")
            .on_hover_text("Trace the active pixel layer. U selects the tool.")
            .clicked()
        {
            studio.trace_active_raster();
            ui.close();
        }
        if ui
            .add_enabled(!studio.node_sel.is_empty(), Button::new("Break path at point"))
            .clicked()
        {
            studio.break_node();
            ui.close();
        }
        ui.separator();
        if ui.button("Swap fill / stroke          X").clicked() {
            studio.swap_fill_stroke();
            ui.close();
        }
        ui.separator();
        if ui
            .add_enabled(
                !studio.selection.is_empty(),
                Button::new("Wrap selection in artboard"),
            )
            .clicked()
        {
            studio.wrap_selection_artboard();
            ui.close();
        }
        if ui
            .add_enabled(
                studio.artboard_sel.len() == 1,
                Button::new("Clone artboard"),
            )
            .clicked()
        {
            if let Some(&id) = studio.artboard_sel.first() {
                studio.clone_artboard(id);
            }
            ui.close();
        }
    });
}

fn flip(studio: &mut Studio, horizontal: bool) {
    studio.flip_selection(horizontal);
}

fn arrange_menu(ui: &mut Ui, studio: &mut Studio) {
    ui.menu_button("Arrange", |ui| {
        ui.label(RichText::new("Align").small().color(fg_weak()));
        if ui.button("Left").clicked() {
            studio.align_sel(crate::align::Align::Left);
            ui.close();
        }
        if ui.button("Centre").clicked() {
            studio.align_sel(crate::align::Align::CenterX);
            ui.close();
        }
        if ui.button("Right").clicked() {
            studio.align_sel(crate::align::Align::Right);
            ui.close();
        }
        if ui.button("Top").clicked() {
            studio.align_sel(crate::align::Align::Top);
            ui.close();
        }
        if ui.button("Middle").clicked() {
            studio.align_sel(crate::align::Align::CenterY);
            ui.close();
        }
        if ui.button("Bottom").clicked() {
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
        ui.separator();
        ui.label(RichText::new("Order").small().color(fg_weak()));
        if ui.button("Bring to front     Ctrl+Shift+]").clicked() {
            studio.bring_to_front();
            ui.close();
        }
        if ui.button("Bring forward            Ctrl+]").clicked() {
            studio.bring_forward();
            ui.close();
        }
        if ui.button("Send backward            Ctrl+[").clicked() {
            studio.send_backward();
            ui.close();
        }
        if ui.button("Send to back       Ctrl+Shift+[").clicked() {
            studio.send_to_back();
            ui.close();
        }
    });
}

fn view_menu(ui: &mut Ui, studio: &mut Studio) {
    ui.menu_button("View", |ui| {
        if ui.button("Zoom in                 Ctrl++").clicked() {
            let at = studio.cursor.map(|c| studio.view.to_screen(c)).unwrap_or(Pt::ZERO);
            studio.zoom_by(1.25, at);
            ui.close();
        }
        if ui.button("Zoom out                Ctrl+-").clicked() {
            let at = studio.cursor.map(|c| studio.view.to_screen(c)).unwrap_or(Pt::ZERO);
            studio.zoom_by(1.0 / 1.25, at);
            ui.close();
        }
        if ui.button("Fit artboard            Ctrl+0").clicked() {
            studio.need_fit = true;
            ui.close();
        }
        if ui.button("Fit selection").clicked() {
            studio.zoom_to_objects(true);
            ui.close();
        }
        if ui.button("100%                    Ctrl+1").clicked() {
            studio.view.scale = 1.0;
            studio.mark();
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
        if ui.button("Keyboard shortcuts          F1").clicked() {
            studio.show_shortcuts = true;
            ui.close();
        }
    });
}

fn persona_tabs(ui: &mut Ui, studio: &mut Studio) {
    for p in [Persona::Design, Persona::Pixel, Persona::Photo, Persona::Motion] {
        let on = studio.persona == p;
        let btn = Button::new(RichText::new(p.name()).strong()).fill(if on {
            accent_dim()
        } else {
            Color32::TRANSPARENT
        });
        if ui
            .add_sized(vec2(68.0, 24.0), btn)
            .on_hover_text(p.hint())
            .clicked()
        {
            studio.commit_type_edit();
            studio.persona = p;
            studio.op = None;
            studio.playing = false;
            studio.tool = match p {
                Persona::Design => Tool::Select,
                Persona::Pixel => Tool::Brush,
                Persona::Photo => Tool::Hand,
                Persona::Motion => Tool::Select,
            };
            studio.show_welcome = false;
        }
    }
}

pub fn doc_tabs(ui: &mut Ui, studio: &mut Studio) {
    studio.ensure_tabs();
    Panel::left("doc-tabs")
        .resizable(false)
        .exact_size(150.0)
        .show(ui, |ui| {
            ui.add_space(6.0);
            let n = studio.tab_count();
            let mut switch = None;
            let mut close = None;
            for i in 0..n {
                let (title, dirty) = studio.tab_title(i);
                let on = i == studio.active_tab;
                let mut label = if dirty {
                    format!("• {title}")
                } else {
                    title.clone()
                };
                if label.chars().count() > 18 {
                    label = format!("{}…", label.chars().take(16).collect::<String>());
                }
                let tip = if dirty {
                    format!("{title} (unsaved)")
                } else {
                    title.clone()
                };
                ui.push_id(i, |ui| {
                    let resp = ui.add_sized(
                        vec2(138.0, 22.0),
                        Button::new(RichText::new(label).small().strong()).fill(if on {
                            accent_dim()
                        } else {
                            Color32::TRANSPARENT
                        }),
                    );
                    if resp.clicked() {
                        switch = Some(i);
                    }
                    if resp.secondary_clicked() {
                        close = Some(i);
                    }
                    resp.on_hover_text(format!("{tip}\nRight-click closes"));
                });
            }
            if icons::tiny_icon(ui, ph::PLUS, "New tab  Ctrl+N", false) {
                studio.new_tab();
            }
            if let Some(i) = switch {
                studio.switch_tab(i);
            }
            if let Some(i) = close {
                studio.request_close_tab(i);
            }
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
                Persona::Motion => Tool::motion_well(),
            };
            let mut last_group = "";
            for t in well.iter().copied() {
                let group = match t {
                    Tool::Select | Tool::Node => "sel",
                    Tool::Pen | Tool::Pencil => "path",
                    Tool::Rect | Tool::Ellipse | Tool::Polygon | Tool::Star | Tool::Line | Tool::Artboard => "shape",
                    Tool::Text | Tool::Gradient | Tool::Eyedropper | Tool::Trace => "look",
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
    Panel::bottom("status").exact_size(24.0).show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.add_space(6.0);
            ui.label(
                RichText::new(format!("{}  {}", studio.tool.label(), studio.tool.key()))
                    .small()
                    .strong()
                    .color(accent()),
            );
            ui.separator();
            ui.label(RichText::new(&studio.status).small().color(fg()));
            ui.with_layout(Layout::right_to_left(eframe::egui::Align::Center), |ui| {
                ui.add_space(8.0);
                ui.label(
                    RichText::new(format!(
                        "{:.0} × {:.0}  {} dpi",
                        studio.doc.width, studio.doc.height, studio.doc.dpi as i32
                    ))
                    .small()
                    .monospace()
                    .color(fg_weak()),
                );
                if !studio.selection.is_empty() {
                    ui.separator();
                    ui.label(
                        RichText::new(format!(
                            "{} selected",
                            studio.selection.len()
                        ))
                        .small()
                        .color(fg_weak()),
                    );
                }
                if let Some(p) = studio.cursor {
                    ui.separator();
                    ui.label(
                        RichText::new(format!("{:.0}  {:.0}", p.x, p.y))
                            .small()
                            .monospace()
                            .color(fg_weak()),
                    );
                }
            });
        });
    });
}
