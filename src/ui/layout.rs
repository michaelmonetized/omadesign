//! Native layout workspace. Controls edit persisted document properties.
use super::theme::{accent, bg_panel, fg_weak};
use crate::app::Studio;
use crate::document::Cmd;
use crate::geom::{Geom, TextAlign};
use crate::layout::{
    AutoStack, Constraint, Sizing, StackAlign, StackAxis, StackFlow, StackJustify,
};
use crate::tools::{Persona, Tool};
use eframe::egui::{self, ComboBox, DragValue, RichText, Ui};

fn label(ui: &mut Ui, text: &str) {
    ui.add_space(8.0);
    ui.label(RichText::new(text).strong().size(11.0));
}
fn sizing(ui: &mut Ui, id: &str, value: &mut Sizing) -> bool {
    let old = *value;
    ComboBox::from_id_salt(id)
        .width(80.0)
        .selected_text(format!("{value:?}"))
        .show_ui(ui, |ui| {
            for v in [Sizing::Fixed, Sizing::Hug, Sizing::Fill] {
                ui.selectable_value(value, v, format!("{v:?}"));
            }
        });
    old != *value
}

pub fn inspector(ui: &mut Ui, studio: &mut Studio) {
    ui.horizontal(|ui| {
        if ui
            .button("Auto layout")
            .on_hover_text("Shift+A · wrap and arrange selected objects")
            .clicked()
        {
            studio.auto_layout_selection();
        }
        if ui
            .button("Present")
            .on_hover_text("Open interactive responsive preview")
            .clicked()
        {
            super::layout_preview::start(ui.ctx(), studio);
        }
    });
    let Some((layer, id)) = studio.primary() else {
        ui.add_space(12.0);
        ui.label(RichText::new("Start with a frame").strong());
        ui.label(
            RichText::new("F to draw · T for text · Shift+A to arrange")
                .small()
                .color(fg_weak()),
        );
        if ui.button("Open Fieldwork starter").clicked() {
            studio.use_layout_template("layout-fieldwork", 1280.0, 900.0, 96.0);
        }
        for (name, w, h) in [
            ("Phone", 390.0, 844.0),
            ("Tablet", 768.0, 1024.0),
            ("Desktop", 1440.0, 900.0),
        ] {
            if ui.button(format!("{name}   {w:.0} × {h:.0}")).clicked() {
                studio.insert_layout_frame(name, w, h);
            }
        }
        return;
    };
    let Some(shape) = studio.doc.find_shape(layer, id).cloned() else {
        return;
    };
    let mut props = shape.layout.clone();
    let mut changed = false;
    egui::CollapsingHeader::new("Image fill")
        .default_open(shape.layout.image.is_some())
        .show(ui, |ui| {
            if super::jobs::is_running::<LoadedImage>(ui.ctx(), "layout-image-load") {
                ui.spinner();
                ui.label("Loading image…");
            } else if ui
                .button(if props.image.is_some() {
                    "Replace image…"
                } else {
                    "Choose image…"
                })
                .clicked()
            {
                let document = studio.swap_id.clone();
                studio.request_file_dialog(
                    || {
                        rfd::FileDialog::new()
                            .add_filter("Images", &["png", "jpg", "jpeg", "webp", "gif", "tiff"])
                            .pick_file()
                    },
                    move |ctx, _, path| {
                        super::jobs::start(ctx, "layout-image-load", move || {
                            let image = crate::layout_images::load(&path)?;
                            Ok(LoadedImage {
                                document,
                                layer,
                                id,
                                image,
                            })
                        });
                    },
                );
            }
            if let Some(image) = &mut props.image {
                ui.horizontal(|ui| {
                    for fit in [
                        crate::layout_images::ImageFit::Cover,
                        crate::layout_images::ImageFit::Contain,
                        crate::layout_images::ImageFit::Stretch,
                    ] {
                        changed |= ui
                            .selectable_value(&mut image.fit, fit, fit.name())
                            .changed();
                    }
                });
                ui.horizontal(|ui| {
                    ui.label("Focus");
                    changed |= ui
                        .add(
                            DragValue::new(&mut image.focal.x)
                                .range(0.0..=1.0)
                                .speed(0.01),
                        )
                        .changed();
                    changed |= ui
                        .add(
                            DragValue::new(&mut image.focal.y)
                                .range(0.0..=1.0)
                                .speed(0.01),
                        )
                        .changed();
                });
                if ui.small_button("Remove image").clicked() {
                    props.image = None;
                    changed = true;
                }
            }
        });
    label(ui, "Sizing");
    ui.horizontal(|ui| {
        ui.label("W");
        changed |= sizing(ui, "layout-width", &mut props.width);
        ui.label("H");
        changed |= sizing(ui, "layout-height", &mut props.height);
    });
    egui::CollapsingHeader::new("Min / max size").show(ui, |ui| {
        for (name, val) in [
            ("Min W", &mut props.min_width),
            ("Max W", &mut props.max_width),
            ("Min H", &mut props.min_height),
            ("Max H", &mut props.max_height),
        ] {
            ui.horizontal(|ui| {
                let mut enabled = val.is_some();
                if ui.checkbox(&mut enabled, name).changed() {
                    *val = enabled.then_some(1.0);
                    changed = true;
                }
                if let Some(v) = val {
                    changed |= ui.add(DragValue::new(v).range(1.0..=10000.0)).changed();
                }
            });
        }
    });
    if props.frame {
        changed |= ui.checkbox(&mut props.clip, "Clip content").changed();
        label(ui, "Responsive frame");
        ui.horizontal(|ui| {
            for (name, width) in [("Phone", 390.0), ("Tablet", 768.0), ("Desktop", 1440.0)] {
                if ui.small_button(name).clicked() {
                    studio.resize_layout_frame(width, shape.geom.bbox().height());
                }
            }
        });
        label(ui, "Layout");
        let mut enabled = props.stack.is_some();
        if ui
            .checkbox(&mut enabled, "Arrange children automatically")
            .changed()
        {
            props.stack = enabled.then(AutoStack::default);
            changed = true;
        }
        if let Some(stack) = &mut props.stack {
            ui.horizontal(|ui| {
                for (name, v) in [
                    ("Stack", StackFlow::Stack),
                    ("Wrap", StackFlow::Wrap),
                    ("Grid", StackFlow::Grid),
                ] {
                    changed |= ui.selectable_value(&mut stack.flow, v, name).changed();
                }
            });
            if stack.flow == StackFlow::Grid {
                ui.horizontal(|ui| {
                    ui.label("Columns");
                    changed |= ui
                        .add(DragValue::new(&mut stack.columns).range(1..=24))
                        .changed();
                });
            } else {
                ui.horizontal(|ui| {
                    changed |= ui
                        .selectable_value(&mut stack.direction, StackAxis::Horizontal, "Horizontal")
                        .changed();
                    changed |= ui
                        .selectable_value(&mut stack.direction, StackAxis::Vertical, "Vertical")
                        .changed();
                });
            }
            ui.horizontal(|ui| {
                ui.label("Align");
                ComboBox::from_id_salt("layout-align")
                    .selected_text(format!("{:?}", stack.align))
                    .show_ui(ui, |ui| {
                        for v in [
                            StackAlign::Start,
                            StackAlign::Center,
                            StackAlign::End,
                            StackAlign::Stretch,
                        ] {
                            changed |= ui
                                .selectable_value(&mut stack.align, v, format!("{v:?}"))
                                .changed();
                        }
                    });
            });
            ui.horizontal(|ui| {
                ui.label("Distribute");
                ComboBox::from_id_salt("layout-justify")
                    .selected_text(format!("{:?}", stack.justify))
                    .show_ui(ui, |ui| {
                        for (name, v) in [
                            ("Start", StackJustify::Start),
                            ("Center", StackJustify::Center),
                            ("End", StackJustify::End),
                            ("Space between", StackJustify::SpaceBetween),
                            ("Space around", StackJustify::SpaceAround),
                            ("Space evenly", StackJustify::SpaceEvenly),
                        ] {
                            changed |= ui.selectable_value(&mut stack.justify, v, name).changed();
                        }
                    });
            });
            ui.horizontal(|ui| {
                ui.label("Gap");
                changed |= ui
                    .add(DragValue::new(&mut stack.gap).range(0.0..=1000.0))
                    .changed();
                if stack.flow != StackFlow::Stack {
                    ui.label("Rows");
                    changed |= ui
                        .add(DragValue::new(&mut stack.cross_gap).range(0.0..=1000.0))
                        .changed();
                }
            });
            ui.label(
                RichText::new("Padding · top / right / bottom / left")
                    .small()
                    .color(fg_weak()),
            );
            ui.horizontal(|ui| {
                for p in &mut stack.padding {
                    changed |= ui
                        .add(
                            DragValue::new(p)
                                .range(0.0..=1000.0)
                                .max_decimals(0)
                                .speed(1),
                        )
                        .changed();
                }
            });
        }
    }
    if props.parent.is_some() {
        label(ui, "Position in frame");
        changed |= ui
            .checkbox(&mut props.absolute, "Absolute position")
            .changed();
        for (name, value) in [
            ("Horizontal", &mut props.constraint_x),
            ("Vertical", &mut props.constraint_y),
        ] {
            ui.horizontal(|ui| {
                ui.label(name);
                ComboBox::from_id_salt(name)
                    .selected_text(value.name())
                    .show_ui(ui, |ui| {
                        for v in Constraint::all() {
                            changed |= ui.selectable_value(value, v, v.name()).changed();
                        }
                    });
            });
        }
        ui.horizontal(|ui| {
            if ui.small_button("Earlier").clicked() {
                studio.reorder_layout_selection(false);
            }
            if ui.small_button("Later").clicked() {
                studio.reorder_layout_selection(true);
            }
            if ui.small_button("Detach from frame").clicked() {
                studio.reparent_layout_selection(None);
            }
        });
    }
    if props.frame {
        let mut locked = props.aspect_ratio.is_some();
        if ui.checkbox(&mut locked, "Lock aspect ratio").changed() {
            props.aspect_ratio =
                locked.then_some(shape.geom.bbox().width() / shape.geom.bbox().height().max(1.0));
            changed = true;
        }
    }
    if props.frame || matches!(shape.geom, Geom::Text(_)) {
        egui::CollapsingHeader::new("Breakpoint overrides").show(ui, |ui| {
            ui.label(
                RichText::new("Rules apply at or below this viewport width.")
                    .small()
                    .color(fg_weak()),
            );
            let mut remove = None;
            for (index, point) in props.breakpoints.iter_mut().enumerate() {
                ui.push_id(index, |ui| {
                    ui.horizontal(|ui| {
                        changed |= ui
                            .add(
                                DragValue::new(&mut point.max_width)
                                    .range(240.0..=3840.0)
                                    .prefix("≤ ")
                                    .suffix(" px"),
                            )
                            .changed();
                        if ui.small_button("×").clicked() {
                            remove = Some(index);
                        }
                    });
                    if props.frame {
                        let mut flow = point.flow;
                        ComboBox::from_id_salt("breakpoint-flow")
                            .selected_text(
                                flow.map_or("Inherit layout".into(), |v| format!("{v:?}")),
                            )
                            .show_ui(ui, |ui| {
                                ui.selectable_value(&mut flow, None, "Inherit layout");
                                for v in [StackFlow::Stack, StackFlow::Wrap, StackFlow::Grid] {
                                    ui.selectable_value(&mut flow, Some(v), format!("{v:?}"));
                                }
                            });
                        if flow != point.flow {
                            point.flow = flow;
                            changed = true;
                        }
                        ui.horizontal(|ui| {
                            for (name, axis) in [
                                ("Inherit", None),
                                ("Row", Some(StackAxis::Horizontal)),
                                ("Column", Some(StackAxis::Vertical)),
                            ] {
                                changed |= ui
                                    .selectable_value(&mut point.direction, axis, name)
                                    .changed();
                            }
                        });
                        ui.horizontal(|ui| {
                            let mut set = point.gap.is_some();
                            if ui.checkbox(&mut set, "Gap").changed() {
                                point.gap = set.then_some(16.0);
                                changed = true;
                            }
                            if let Some(v) = &mut point.gap {
                                changed |= ui.add(DragValue::new(v).range(0.0..=1000.0)).changed();
                            }
                        });
                        ui.horizontal(|ui| {
                            let mut set = point.padding.is_some();
                            if ui.checkbox(&mut set, "Padding").changed() {
                                point.padding = set.then_some([24.0; 4]);
                                changed = true;
                            }
                            if let Some(p) = &mut point.padding {
                                let mut n = p[0];
                                if ui.add(DragValue::new(&mut n).range(0.0..=1000.0)).changed() {
                                    *p = [n; 4];
                                    changed = true;
                                }
                            }
                        });
                        if point.flow == Some(StackFlow::Grid) {
                            let v = point.columns.get_or_insert(2);
                            changed |= ui
                                .add(DragValue::new(v).range(1..=24).prefix("Columns "))
                                .changed();
                        }
                    }
                    if let Geom::Text(run) = &shape.geom {
                        let size = point.text_size.get_or_insert(run.px);
                        changed |= ui
                            .add(DragValue::new(size).range(1.0..=500.0).suffix(" px type"))
                            .changed();
                    }
                    ui.separator();
                });
            }
            if let Some(index) = remove {
                props.breakpoints.remove(index);
                changed = true;
            }
            ui.horizontal(|ui| {
                for (name, width) in [("+ Phone", 600.0), ("+ Tablet", 1024.0)] {
                    if ui.button(name).clicked() {
                        props.breakpoints.push(crate::layout::LayoutBreakpoint {
                            max_width: width,
                            ..Default::default()
                        });
                        if let Geom::Text(run) = &shape.geom {
                            props.text_size.get_or_insert(run.px);
                        }
                        changed = true;
                    }
                }
            });
        });
    }
    if changed {
        studio.edit_layout(|l| {
        macro_rules! copy_changed {($($field:ident),*)=>{$(if shape.layout.$field!=props.$field{l.$field=props.$field.clone();})*};}
        copy_changed!(width,height,min_width,max_width,min_height,max_height,clip,absolute,constraint_x,constraint_y,stack,breakpoints,text_size,aspect_ratio,image);
    });
    }
    if let Geom::Text(mut run) = shape.geom.clone() {
        label(ui, "Paragraph");
        let before = run.clone();
        let mut wrap = run.wrap_width.is_some();
        if ui.checkbox(&mut wrap, "Wrap text to width").changed() {
            run.wrap_width = wrap.then_some(shape.world_bbox().width().max(100.0));
        }
        if let Some(width) = &mut run.wrap_width {
            ui.add(DragValue::new(width).range(1.0..=10000.0).prefix("Width "));
        }
        ui.horizontal(|ui| {
            for (name, v) in [
                ("Left", TextAlign::Start),
                ("Center", TextAlign::Center),
                ("Right", TextAlign::End),
            ] {
                ui.selectable_value(&mut run.align, v, name);
            }
        });
        if run != before {
            run.contours = crate::text::shape(&run);
            studio.commit(Cmd::SetGeom {
                layer,
                id,
                before: Geom::Text(before),
                after: Geom::Text(run),
                rot_before: shape.rotation,
                rot_after: shape.rotation,
            });
        }
    }
    super::layout_preview::interaction_editor(ui, studio);
    super::layout_preview::component_editor(ui, studio);
    variables(ui, studio);
    if shape.layout.frame {
        label(ui, "Export frame");
        ui.horizontal(|ui| {
            if ui.button("PNG").clicked() {
                studio.export_selected_frame_png();
            }
            if ui.button("SVG").clicked() {
                studio.export_selected_frame_svg();
            }
            if ui.button("HTML").clicked() {
                studio.export_selected_frame_html();
            }
        });
    }
}

struct LoadedImage {
    document: String,
    layer: usize,
    id: u64,
    image: crate::layout_images::ImageFill,
}
pub fn poll_image(ctx: &egui::Context, studio: &mut Studio) {
    if studio.file_dialog_pending() {
        return;
    }
    if let Some(result) = super::jobs::poll::<LoadedImage>(ctx, "layout-image-load") {
        match result {
            Ok(image) if image.document == studio.swap_id => {
                if let Some(shape) = studio.doc.find_shape(image.layer, image.id) {
                    let before = shape.layout.clone();
                    let mut after = before.clone();
                    after.image = Some(image.image);
                    after.placeholder = false;
                    studio.commit(Cmd::SetLayout {
                        layer: image.layer,
                        id: image.id,
                        before,
                        after,
                    });
                    studio.status = "Image placed · embedded in this document".into();
                }
            }
            Ok(_) => studio.status = "Image load cancelled because the document changed".into(),
            Err(e) => studio.status = e,
        }
    }
}

fn variables(ui: &mut Ui, studio: &mut Studio) {
    use crate::layout_tokens::{DesignToken, TokenProperty, TokenValue};
    egui::CollapsingHeader::new("Design variables").show(ui, |ui| {
        let mut tokens = studio.doc.layout_tokens.clone();
        let mut changed = false;
        let mut remove = None;
        for t in &mut tokens {
            ui.push_id(t.id, |ui| {
                ui.horizontal(|ui| {
                    ui.add(egui::Label::new(&t.name).truncate());
                    match &mut t.value {
                        TokenValue::Color(c) => {
                            changed |= super::color_picker::color_edit(ui, "token", c);
                        }
                        TokenValue::Number(n) => {
                            changed |= ui.add(DragValue::new(n).range(0.0..=10000.0)).changed();
                        }
                    }
                    if ui
                        .small_button("×")
                        .on_hover_text("Remove variable, keeping current appearance")
                        .clicked()
                    {
                        remove = Some(t.id);
                    }
                });
            });
        }
        if changed {
            studio.set_layout_tokens(tokens);
        }
        if let Some(id) = remove {
            studio.remove_layout_token(id);
        }
        let key = egui::Id::new("new-design-variable-name");
        let mut name = ui
            .ctx()
            .data(|d| d.get_temp::<String>(key))
            .unwrap_or_default();
        ui.add(egui::TextEdit::singleline(&mut name).hint_text("Variable name"));
        ui.ctx().data_mut(|d| d.insert_temp(key, name.clone()));
        ui.horizontal(|ui| {
            if ui
                .add_enabled(!name.trim().is_empty(), egui::Button::new("+ Color"))
                .clicked()
            {
                let mut ts = studio.doc.layout_tokens.clone();
                let c = match studio.style.fill {
                    crate::document::Fill::Solid(c) => c,
                    _ => crate::color::Rgba::rgb(232, 106, 60),
                };
                ts.push(DesignToken::color(name.trim(), c));
                studio.set_layout_tokens(ts);
            }
            if ui
                .add_enabled(!name.trim().is_empty(), egui::Button::new("+ Number"))
                .clicked()
            {
                let mut ts = studio.doc.layout_tokens.clone();
                ts.push(DesignToken::number(name.trim(), 16.0));
                studio.set_layout_tokens(ts);
            }
        });
        if let Some((li, id)) = studio.primary()
            && let Some(shape) = studio.doc.find_shape(li, id).cloned()
        {
            for property in TokenProperty::all() {
                if !property.supports(&shape) {
                    continue;
                }
                let current = shape.layout.tokens.get(property);
                ui.horizontal(|ui| {
                    ui.label(property.name());
                    egui::ComboBox::from_id_salt(("token-bind", property.name()))
                        .selected_text(
                            studio
                                .doc
                                .layout_tokens
                                .iter()
                                .find(|t| Some(t.id) == current)
                                .map_or("Local value", |t| t.name.as_str()),
                        )
                        .show_ui(ui, |ui| {
                            if ui
                                .selectable_label(current.is_none(), "Local value")
                                .clicked()
                            {
                                studio.bind_layout_token(property, None);
                            }
                            for token in studio.doc.layout_tokens.clone() {
                                if property.accepts(token.value)
                                    && ui
                                        .selectable_label(current == Some(token.id), &token.name)
                                        .clicked()
                                {
                                    studio.bind_layout_token(property, Some(token.id));
                                }
                            }
                        });
                });
            }
        }
    });
}

pub fn hierarchy(ui: &mut Ui, studio: &mut Studio) {
    if studio.persona != Persona::Layout || studio.show_welcome {
        return;
    }
    egui::Panel::left("layout-hierarchy")
        .resizable(true)
        .default_size(200.0)
        .size_range(170.0..=320.0)
        .frame(egui::Frame::new().fill(bg_panel()).inner_margin(12))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("Layers").strong());
                if ui.small_button("+ Frame").clicked() {
                    studio.insert_layout_frame("Frame", 390.0, 844.0);
                }
            });
            ui.add_space(8.0);
            egui::ScrollArea::vertical().show(ui, |ui| {
                let mut rows = Vec::new();
                let roots: Vec<_> = studio
                    .doc
                    .layers
                    .iter()
                    .enumerate()
                    .filter(|(_, layer)| layer.parent.is_none())
                    .map(|(li, _)| li)
                    .rev()
                    .collect();
                for li in roots {
                    layer_row(ui, studio, li, 0, &mut rows);
                }
                if let Some(drop) = super::layer_drag::drop_target(ui, studio, &rows) {
                    drop.apply(studio);
                }
            });
        });
}

fn layer_row(
    ui: &mut Ui,
    studio: &mut Studio,
    li: usize,
    depth: usize,
    rows: &mut Vec<(super::layer_drag::TreeRow, egui::Rect)>,
) {
    if depth > 64 {
        return;
    }
    let layer = &studio.doc.layers[li];
    let id = layer.id;
    let is_group = layer.is_group;
    let name = layer.name.clone();
    let key = egui::Id::new(("layout-layer-open", id));
    let mut open = ui.ctx().data(|d| d.get_temp::<bool>(key)).unwrap_or(true);
    let row = ui
        .horizontal(|ui| {
            ui.add_space((depth as f32 * 10.).min(70.));
            if ui.small_button(if open { "▾" } else { "▸" }).clicked() {
                open = !open;
                ui.ctx().data_mut(|d| d.insert_temp(key, open));
            }
            ui.add(
                egui::Button::selectable(
                    studio.selected_layer == Some(id),
                    format!("{}  {name}", if is_group { "▱" } else { "≡" }),
                )
                .sense(egui::Sense::click_and_drag())
                .truncate(),
            )
        })
        .inner;
    rows.push((super::layer_drag::TreeRow::Layer(li), row.rect));
    if row.clicked() || super::layer_drag::source(&row, studio, li) {
        if !studio.apply_pending_item_mask(li, None) {
            let previous = ui
                .input(|i| i.modifiers.shift)
                .then(|| studio.selection.clone());
            studio.activate_layer_tree(li);
            if let Some(previous) = previous {
                for item in previous {
                    if !studio.selection.contains(&item) {
                        studio.selection.push(item);
                    }
                }
            }
            studio.tool = Tool::Select;
        }
    }
    row.context_menu(|ui| {
        if ui.button("Mask from item…").clicked() {
            studio.begin_item_mask(li, None);
            ui.close();
        }
        if ui
            .add_enabled(studio.layer_unlocked(li), egui::Button::new("Delete layer"))
            .clicked()
        {
            studio.delete_layer_tree(li);
            ui.close();
        }
    });
    if li >= studio.doc.layers.len() {
        return;
    }
    if open {
        if is_group {
            let children: Vec<_> = studio
                .doc
                .layers
                .iter()
                .enumerate()
                .filter(|(_, l)| l.parent == Some(id))
                .map(|(i, _)| i)
                .rev()
                .collect();
            for child in children {
                layer_row(ui, studio, child, depth + 1, rows);
            }
        } else {
            let objects: Vec<_> = studio.doc.layers[li]
                .kind
                .shapes()
                .into_iter()
                .flatten()
                .filter(|s| s.layout.parent.is_none())
                .map(|s| s.id)
                .rev()
                .collect();
            for object in objects {
                tree_row(ui, studio, li, object, depth + 1, rows);
            }
        }
    }
}

fn tree_row(
    ui: &mut Ui,
    studio: &mut Studio,
    li: usize,
    id: u64,
    depth: usize,
    rows: &mut Vec<(super::layer_drag::TreeRow, egui::Rect)>,
) {
    if depth > 64 {
        return;
    }
    let Some(shape) = studio.doc.find_shape(li, id) else {
        return;
    };
    let name = shape.name.clone();
    let icon = if shape.layout.component.is_some() {
        "◇"
    } else if shape.layout.frame {
        "▣"
    } else if matches!(shape.geom, Geom::Text(_)) {
        "T"
    } else {
        "▪"
    };
    let children = crate::layout::children(&studio.doc, li, id);
    let selected = studio.selection.contains(&(li, id));
    let key = egui::Id::new(("layout-tree-open", id));
    let mut open = ui
        .ctx()
        .data(|d| d.get_temp::<bool>(key))
        .unwrap_or(depth == 0);
    let row = ui
        .horizontal(|ui| {
            ui.add_space((depth as f32 * 10.0).min(70.0));
            if !children.is_empty() {
                if ui.small_button(if open { "▾" } else { "▸" }).clicked() {
                    open = !open;
                    ui.ctx().data_mut(|d| d.insert_temp(key, open));
                }
            } else {
                ui.add_space(18.0);
            }
            ui.add(
                egui::Button::selectable(
                    selected,
                    RichText::new(format!("{icon}  {name}")).color(if selected {
                        accent()
                    } else {
                        super::theme::fg()
                    }),
                )
                .sense(egui::Sense::click_and_drag())
                .truncate(),
            )
        })
        .inner;
    rows.push((super::layer_drag::TreeRow::Object(li, id), row.rect));
    if row.double_clicked() {
        studio.enter_group_item((li, id));
    } else if row.clicked() || (super::layer_drag::object_source(&row, studio, li, id) && !selected)
    {
        if !studio.apply_pending_item_mask(li, Some(id)) {
            let hits = studio.selection_for_hit((li, id));
            if studio.individual_object != Some((li, id)) {
                studio.individual_object = None;
            }
            if !ui.input(|i| i.modifiers.shift) {
                studio.selection.clear();
            }
            for hit in hits {
                if !studio.selection.contains(&hit) {
                    studio.selection.push(hit);
                }
            }
            studio.active_layer = Some(li);
            studio.selected_layer = None;
            studio.tool = Tool::Select;
        }
    }
    row.context_menu(|ui| {
        if ui.button("Select").clicked() {
            studio.selection = vec![(li, id)];
            ui.close();
        }
        if ui
            .add_enabled(
                studio
                    .doc
                    .find_shape(li, id)
                    .is_some_and(|s| s.layout.frame),
                egui::Button::new("Nest selection here"),
            )
            .clicked()
        {
            studio.reparent_layout_selection(Some(id));
            ui.close();
        }
        if ui.button("Duplicate").clicked() {
            studio.selection = vec![(li, id)];
            studio.duplicate_selection();
            ui.close();
        }
        if ui.button("Mask from item…").clicked() {
            studio.begin_item_mask(li, Some(id));
            ui.close();
        }
        if ui
            .add_enabled(
                studio.pixel_sel.is_some(),
                egui::Button::new("Mask from selection"),
            )
            .clicked()
        {
            studio.mask_object_from_selection(li, id);
            ui.close();
        }
        if ui.button("Delete object").clicked() {
            studio.delete_object_item(li, id);
            ui.close();
        }
    });
    if open {
        for child in children.into_iter().rev() {
            tree_row(ui, studio, li, child, depth + 1, rows);
        }
    }
}
