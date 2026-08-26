use crate::document::{Fill, LayerBlend, LayerKind};
use crate::main_app::{ActiveOp, AtelierApp, Tool};
use eframe::egui::{Button, Color32, Layout, Panel, RichText, ScrollArea, Stroke, Ui, vec2};

pub fn tool_hint(tool: &Tool) -> &'static str {
    match tool {
        Tool::Select => "V | click to select - drag to move - Del removes",
        Tool::Rect => "R | drag to create a rectangle",
        Tool::Ellipse => "O | drag to create an ellipse",
        Tool::Pen => "P | click adds points - double-click or Enter closes - Esc cancels",
        Tool::Brush => "B | paint on the active pixel layer (pick one in Layers)",
        Tool::Text => "T | click on canvas to place text, edit it in Properties",
    }
}

fn tool_button(ui: &mut Ui, app: &mut AtelierApp, tool: Tool, label: &str) {
    let selected = std::mem::discriminant(&app.tool) == std::mem::discriminant(&tool);
    let text = RichText::new(format!(" {label} ")).strong();
    let btn = if selected {
        Button::new(text).fill(Color32::from_rgb(0x2F, 0x81, 0xF7))
    } else {
        Button::new(text)
    };
    if ui.add_sized([36.0, 30.0], btn).clicked() {
        app.tool = tool;
        app.op = None;
    }
}

pub fn left_toolbar(ui: &mut Ui, app: &mut AtelierApp) {
    Panel::left("tools")
        .resizable(false)
        .exact_size(48.0)
        .show(ui, |ui| {
            ui.add_space(6.0);
            for (t, l) in [
                (Tool::Select, "V"),
                (Tool::Rect, "R"),
                (Tool::Ellipse, "O"),
                (Tool::Pen, "P"),
                (Tool::Brush, "B"),
                (Tool::Text, "T"),
            ] {
                tool_button(ui, app, t, l);
                ui.add_space(2.0);
            }
        });
}

pub fn top_bar(ui: &mut Ui, app: &mut AtelierApp) {
    Panel::top("top").show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.heading(RichText::new("Atelier").strong());
            ui.separator();
            if ui.button("New").clicked() {
                app.new_document();
            }
            if ui.button("Open").clicked() {
                app.open_project();
            }
            if ui.button("Save").clicked() {
                app.save_project();
            }
            if ui
                .add_enabled(app.history.can_undo(), Button::new("Undo"))
                .clicked()
            {
                app.do_undo();
            }
            if ui
                .add_enabled(app.history.can_redo(), Button::new("Redo"))
                .clicked()
            {
                app.do_redo();
            }
            ui.separator();
            eframe::egui::ComboBox::from_id_salt("export-scale")
                .selected_text(format!("{}x", app.export_scale))
                .width(52.0)
                .show_ui(ui, |ui| {
                    for s in [1u32, 2, 3] {
                        ui.selectable_value(&mut app.export_scale, s, format!("{s}x"));
                    }
                });
            if ui.button("Export PNG").clicked() {
                app.export_png();
            }
            if ui.button("Export SVG").clicked() {
                app.export_svg();
            }
            ui.with_layout(Layout::right_to_left(eframe::egui::Align::Center), |ui| {
                if ui.button("Fit view").clicked() {
                    app.need_fit = true;
                }
                ui.label(format!("{:.0}%", app.view.scale * 100.0));
            });
        });
    });
}

pub fn status_bar(ui: &mut Ui, app: &mut AtelierApp) {
    Panel::bottom("status").show(ui, |ui| {
        ui.horizontal(|ui| {
            let hint = tool_hint(&app.tool);
            ui.label(RichText::new(hint).weak().small());
            ui.with_layout(Layout::right_to_left(eframe::egui::Align::Center), |ui| {
                if let Some(p) = app.cursor_world {
                    ui.label(
                        RichText::new(format!("x {:.0}  y {:.0}", p.x, p.y))
                            .weak()
                            .small(),
                    );
                }
                ui.label(RichText::new(&app.status).small());
            });
        });
    });
}

pub fn right_panel(ui: &mut Ui, app: &mut AtelierApp) {
    Panel::right("layers").show(ui, |ui| {
        layers_section(ui, app);
        ui.add_space(8.0);
        properties_section(ui, app);
    });
}

enum LayerAction {
    ToggleVisible(usize),
    Activate(usize),
    MoveUp(usize),
    MoveDown(usize),
}

fn layers_section(ui: &mut Ui, app: &mut AtelierApp) {
    ui.horizontal(|ui| {
        ui.strong("Layers");
        ui.with_layout(Layout::right_to_left(eframe::egui::Align::Center), |ui| {
            if ui.small_button("+ Pixel").clicked() {
                app.add_layer(true);
            }
            if ui.small_button("+ Vector").clicked() {
                app.add_layer(false);
            }
        });
    });

    let mut actions: Vec<LayerAction> = vec![];
    ScrollArea::vertical()
        .max_height(ui.available_height() * 0.55)
        .show(ui, |ui| {
            for li in (0..app.doc.layers.len()).rev() {
                ui.push_id(li as u64 + 7777u64, |ui| {
                    ui.horizontal(|ui| {
                        let is_active = app.active_layer == Some(li);
                        let layer = &app.doc.layers[li];
                        let eye = if layer.visible { "◉" } else { "○" };
                        if ui.button(eye).clicked() {
                            actions.push(LayerAction::ToggleVisible(li));
                        }
                        let fill = if is_active {
                            Color32::from_rgb(0x2A, 0x37, 0x4B)
                        } else {
                            Color32::TRANSPARENT
                        };
                        let name = layer.name.clone();
                        let tag = layer.kind.tag().to_string();
                        let sel_btn =
                            Button::new(RichText::new(format!("{tag} {} ", name)).size(12.0))
                                .fill(fill);
                        if ui
                            .add_sized(vec2(ui.available_width() - 56.0, 22.0), sel_btn)
                            .clicked()
                        {
                            actions.push(LayerAction::Activate(li));
                        }
                        if ui.small_button("↑").clicked() {
                            actions.push(LayerAction::MoveUp(li));
                        }
                        if ui.small_button("↓").clicked() {
                            actions.push(LayerAction::MoveDown(li));
                        }
                    });
                });
            }
        });

    for a in actions {
        match a {
            LayerAction::ToggleVisible(li) => {
                if let Some(l) = app.doc.layers.get_mut(li) {
                    l.visible = !l.visible;
                }
            }
            LayerAction::Activate(li) => app.active_layer = Some(li),
            LayerAction::MoveUp(li) => {
                if li + 1 < app.doc.layers.len() {
                    app.doc.layers.swap(li, li + 1);
                    if app.active_layer == Some(li) {
                        app.active_layer = Some(li + 1);
                    } else if app.active_layer == Some(li + 1) {
                        app.active_layer = Some(li);
                    }
                }
            }
            LayerAction::MoveDown(li) => {
                if li > 0 {
                    app.doc.layers.swap(li, li - 1);
                    if app.active_layer == Some(li) {
                        app.active_layer = Some(li - 1);
                    } else if app.active_layer == Some(li - 1) {
                        app.active_layer = Some(li);
                    }
                }
            }
        }
    }

    ui.horizontal(|ui| {
        if ui.small_button("Duplicate layer").clicked() {
            app.duplicate_active_layer();
        }
        if ui.small_button("Delete layer").clicked() {
            app.delete_active_layer();
        }
    });
}

fn properties_section(ui: &mut Ui, app: &mut AtelierApp) {
    ui.strong("Properties");

    if let Some((li, sid)) = app.selection {
        let mut fill_edit: Option<Fill> = None;
        let mut stroke_edit: Option<Option<Stroke>> = None;

        if let Some(layer) = app.doc.layers.get_mut(li)
            && let Some(shape) = layer.find_shape_by_id_mut(sid)
        {
            ui.add_space(4.0);
            ui.label(format!("Shape #{sid}"));

            // Fill mode combo
            let fill_label = match &shape.style.fill {
                Fill::None => "None",
                Fill::Solid(_) => "Solid",
                Fill::Linear { .. } => "Linear",
            };
            let mut new_fill_mode = fill_label.to_string();
            eframe::egui::ComboBox::from_id_salt("fill-mode")
                .selected_text(&new_fill_mode)
                .width(100.0)
                .show_ui(ui, |ui| {
                    for label in ["None", "Solid", "Linear"] {
                        if ui.selectable_value(&mut new_fill_mode, label.to_string(), label).changed() {
                            let new_fill = match new_fill_mode.as_str() {
                                "Solid" => Fill::Solid(app.default_gradient.0),
                                "Linear" => Fill::Linear {
                                    from: [0.0, 0.0],
                                    to: [1.0, 1.0],
                                    c0: app.default_gradient.0,
                                    c1: app.default_gradient.1,
                                },
                                _ => Fill::None,
                            };
                            fill_edit = Some(new_fill);
                        }
                    }
                });

            // Fill color / gradient controls
            match &mut shape.style.fill {
                Fill::None => {}
                Fill::Solid(c) => {
                    if ui.color_edit_button_srgba(c).changed() {
                        app.default_gradient.0 = *c;
                    }
                }
                Fill::Linear { c0, c1, .. } => {
                    ui.horizontal(|ui| {
                        ui.label("from");
                        if ui.color_edit_button_srgba(c0).changed() {
                            app.default_gradient.0 = *c0;
                        }
                        ui.label("to");
                        if ui.color_edit_button_srgba(c1).changed() {
                            app.default_gradient.1 = *c1;
                        }
                    });
                }
            }

            // Apply fill mode change
            if let Some(new_fill) = fill_edit {
                let before = shape.style.clone();
                shape.style.fill = new_fill;
                let after = shape.style.clone();
                app.history.push(crate::document::history::Cmd::SetStyle {
                    layer: li,
                    id: sid,
                    before,
                    after,
                });
            }

            // Stroke toggle + controls
            let has_stroke = shape.style.stroke.is_some();
            let mut new_has_stroke = has_stroke;
            if ui.checkbox(&mut new_has_stroke, "Stroke").changed() {
                if new_has_stroke {
                    stroke_edit = Some(Some(Stroke::new(2.0, Color32::WHITE)));
                } else {
                    stroke_edit = Some(None);
                }
            }
            if let Some(st) = shape.style.stroke.as_mut() {
                ui.horizontal(|ui| {
                    ui.color_edit_button_srgba(&mut st.color);
                    ui.add(eframe::egui::Slider::new(&mut st.width, 0.5..=32.0).text("width"));
                });
            }

            // Apply stroke change
            if let Some(new_stroke) = stroke_edit {
                let before = shape.style.clone();
                shape.style.stroke = new_stroke;
                let after = shape.style.clone();
                app.history.push(crate::document::history::Cmd::SetStyle {
                    layer: li,
                    id: sid,
                    before,
                    after,
                });
            }
        } else {
            app.selection = None;
        }
    }

    if let Some((li, sid)) = app.selection {
        let is_text = app
            .doc
            .layers
            .get(li)
            .and_then(|l| l.find_shape_by_id(sid))
            .is_some_and(|s| matches!(s.geom, crate::document::Geometry::Text { .. }));
        if is_text {
            if app.text_edit_sid != Some(sid) {
                if let Some(shape) = app.doc.layers.get(li).and_then(|l| l.find_shape_by_id(sid))
                    && let crate::document::Geometry::Text { content, px, .. } = &shape.geom
                {
                    app.text_buf = content.clone();
                    app.text_px = *px;
                }
                app.text_edit_sid = Some(sid);
                app.text_before = None;
            }
            ui.add_space(4.0);
            ui.label("Text");
            let resp_content = ui.text_edit_singleline(&mut app.text_buf);
            let mut px = app.text_px;
            let resp_px = ui.add(eframe::egui::Slider::new(&mut px, 8.0..=400.0).text("size"));
            if resp_content.changed() || resp_px.changed() {
                let buf = app.text_buf.clone();
                app.text_live_edit(&buf, px);
                app.text_px = px;
            }
            if resp_content.lost_focus() || resp_px.drag_stopped() {
                app.text_commit();
            }
            if !crate::text::available() {
                ui.label(RichText::new("no system font found").weak().small());
            }
        } else if app.text_edit_sid.is_some() {
            app.text_commit();
            app.text_edit_sid = None;
        }
    }

    if matches!(app.tool, Tool::Brush) || matches!(&app.op, Some(ActiveOp::Brushing { .. })) {
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.add(eframe::egui::Slider::new(&mut app.brush.size, 1.0..=128.0).text("Brush"));
        });
        ui.horizontal(|ui| {
            ui.add(eframe::egui::Slider::new(&mut app.brush.flow, 0.05..=1.0).text("Flow"));
            ui.color_edit_button_srgba(&mut app.brush.color);
        });
    }

    if let Some(li) = app.active_layer {
        let was_locked = app.doc.layers.get(li).map(|l| l.locked).unwrap_or(false);
        let has_mask = app.doc.layers.get(li).map(|l| l.mask.is_some()).unwrap_or(false);
        if let Some(l) = app.doc.layers.get_mut(li) {
            ui.add_space(4.0);
            let name = l.name.clone();
            let mut opacity = l.opacity;
            let mut locked = l.locked;
            ui.add(
                eframe::egui::Slider::new(&mut opacity, 0.0..=1.0).text(format!("{name} opacity")),
            );
            ui.checkbox(&mut locked, "Lock layer");
            l.opacity = opacity;
            l.locked = locked;

            // Blend mode dropdown
            let mut blend = l.blend;
            eframe::egui::ComboBox::from_id_salt("blend-mode")
                .selected_text(blend.name())
                .width(110.0)
                .show_ui(ui, |ui| {
                    for b in LayerBlend::ALL {
                        ui.selectable_value(&mut blend, b, b.name());
                    }
                });
            l.blend = blend;
        }

        // Mask toggle (outside borrow of layers)
        ui.horizontal(|ui| {
            if ui.small_button(if has_mask { "Remove Mask" } else { "Add Mask" }).clicked() {
                app.toggle_mask();
            }
            if has_mask {
                ui.checkbox(&mut app.edit_mask, "Edit mask");
            }
        });

        if !was_locked && app.doc.layers.get(li).map(|l| l.locked).unwrap_or(false) {
            app.selection = None;
        }
    }

    // Boolean ops (when two shapes selected)
    if let Some((li, a_id)) = app.selection {
        if let Some((_, b_id)) = app.bool_second {
            ui.add_space(4.0);
            ui.label(format!("Boolean: #{a_id} op #{b_id}"));
            ui.horizontal(|ui| {
                if ui.small_button("Union").clicked() {
                    app.apply_boolean(crate::boolean::BoolOp::Union);
                }
                if ui.small_button("Subtract").clicked() {
                    app.apply_boolean(crate::boolean::BoolOp::Subtract);
                }
                if ui.small_button("Intersect").clicked() {
                    app.apply_boolean(crate::boolean::BoolOp::Intersect);
                }
                if ui.small_button("XOR").clicked() {
                    app.apply_boolean(crate::boolean::BoolOp::Xor);
                }
            });
        } else if app.pending_bool.is_some() {
            ui.label(RichText::new("Click the second shape...").weak().small());
        }
    }

    if let Some(kind_tag) = app
        .active_layer
        .and_then(|li| app.doc.layers.get(li))
        .map(|l| l.kind.tag())
    {
        ui.label(
            RichText::new(format!("active layer type: {kind_tag}"))
                .weak()
                .small(),
        );
        let _ = LayerKind::Vector; // keep import used
    }
}
