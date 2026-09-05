use crate::app::Studio;
use crate::color::{Blend, Rgba};
use crate::document::{Cap, Fill, Join, Stroke as DocStroke};
use crate::geom::Geom;
use crate::tools::{Persona, Tool};
use crate::ui::icons::{self, ph};
use crate::ui::theme::{accent, accent_soft, bg_panel, bg_widget, border, fg, fg_weak};
use eframe::egui::{
    Color32, ComboBox, Frame, Layout, Margin, Panel, RichText, ScrollArea, Slider, Stroke, Ui, vec2,
};

pub fn right_panel(ui: &mut Ui, studio: &mut Studio) {
    if studio.paint_mask
        && studio
            .active_layer
            .and_then(|index| studio.doc.layers.get(index))
            .is_none_or(|layer| layer.mask.is_none())
    {
        studio.paint_mask = false;
    }
    Panel::right("studios")
        .resizable(true)
        .default_size(288.0)
        .size_range(256.0..=420.0)
        .frame(
            Frame::new()
                .fill(bg_panel())
                .inner_margin(Margin::symmetric(14, 14)),
        )
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing = vec2(8.0, 6.0);
            inspector_title(ui, studio);
            section_gap(ui);
            let design = studio.persona == Persona::Design;
            let motion = studio.persona == Persona::Motion;
            let paint = pixel_context(studio);
            let reshaping = studio.deformation.is_some();
            let typing = studio.type_edit.is_some()
                || studio.tool == Tool::Text
                || studio.selected_type().is_some();
            let properties_height = (ui.available_height() - 152.0).max(170.0);
            ScrollArea::vertical()
                .id_salt("properties-scroll")
                .max_height(properties_height)
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    if reshaping {
                        super::deform::inspector(ui, studio);
                        section_gap(ui);
                    }
                    if design && typing && !reshaping {
                        character_studio(ui, studio);
                        section_gap(ui);
                    }
                    if design
                        && !reshaping
                        && (!studio.selection.is_empty()
                            || !studio.artboard_sel.is_empty()
                            || matches!(
                                studio.tool,
                                Tool::Rect | Tool::Polygon | Tool::Star | Tool::Artboard
                            ))
                    {
                        if typing || ui.ctx().viewport_rect().height() < 760.0 {
                            eframe::egui::CollapsingHeader::new("Layout")
                                .show(ui, |ui| transform_studio(ui, studio, false));
                        } else {
                            transform_studio(ui, studio, true);
                        }
                        section_gap(ui);
                    }
                    if paint || studio.paint_mask {
                        super::masking::inspector(ui, studio);
                        section_gap(ui);
                        brush_studio(ui, studio);
                        section_gap(ui);
                    }
                    if !studio.paint_mask {
                        if matches!(studio.tool, Tool::Brush | Tool::Fill) {
                            paint_color_studio(ui, studio);
                        } else if !paint {
                            color_studio(ui, studio);
                        }
                    }
                    if studio.tool == Tool::Trace {
                        section_gap(ui);
                        trace_studio(ui, studio);
                    }
                    if design {
                        section_gap(ui);
                        eframe::egui::CollapsingHeader::new("Effects")
                            .show(ui, |ui| fx_studio(ui, studio));
                    }
                    if motion {
                        section_gap(ui);
                        motion_studio(ui, studio);
                    }
                });
            ui.add_space(10.0);
            ui.separator();
            ui.add_space(8.0);
            ScrollArea::vertical()
                .id_salt("layers-scroll")
                .auto_shrink([false, false])
                .show(ui, |ui| layers_studio(ui, studio));
        });
}

fn section_gap(ui: &mut Ui) {
    ui.add_space(12.0);
}

fn heading(ui: &mut Ui, title: &str) {
    ui.label(RichText::new(title).strong().size(12.0).color(fg()));
}

fn geometry_icon(geometry: &Geom) -> &'static str {
    match geometry {
        Geom::Rect { .. } => ph::RECTANGLE,
        Geom::Ellipse { .. } => ph::CIRCLE,
        Geom::Polygon { .. } => ph::HEXAGON,
        Geom::Star { .. } => ph::STAR,
        Geom::Line { .. } => ph::LINE_SEGMENT,
        Geom::Text(_) => ph::TEXT_T,
        Geom::Path { .. } | Geom::Poly { .. } => ph::PATH,
    }
}

fn pixel_context(studio: &Studio) -> bool {
    studio.persona == Persona::Pixel
        || matches!(
            studio.tool,
            Tool::Brush
                | Tool::Eraser
                | Tool::Fill
                | Tool::Clone
                | Tool::Heal
                | Tool::Smudge
                | Tool::Wand
        )
}

fn inspector_title(ui: &mut Ui, studio: &Studio) {
    let selected = studio
        .primary()
        .and_then(|(li, id)| studio.doc.find_shape(li, id));
    let selection_name =
        (studio.selection.len() > 1).then(|| format!("{} objects", studio.selection.len()));
    let (icon, name, description) = if studio.paint_mask {
        (
            ph::SELECTION,
            "Layer mask",
            "Paint to reveal or hide".into(),
        )
    } else if pixel_context(studio) {
        let (icon, name) = match studio.tool {
            Tool::Heal => (ph::BANDAIDS, "Healing brush"),
            Tool::Clone => (ph::COPY, "Clone brush"),
            Tool::Smudge => (ph::DROP, "Smudge brush"),
            Tool::Eraser => (ph::ERASER, "Eraser"),
            Tool::Fill => (ph::PAINT_BUCKET, "Fill"),
            Tool::Wand => (ph::MAGIC_WAND, "Magic wand"),
            _ => (ph::PAINT_BRUSH, studio.tool.label()),
        };
        let description = studio
            .active_layer
            .and_then(|index| studio.doc.layers.get(index))
            .filter(|layer| layer.kind.pixels().is_some())
            .map_or_else(
                || "Choose a pixel layer".into(),
                |layer| format!("Pixels · {}", layer.name),
            );
        (icon, name, description)
    } else if let Some(name) = &selection_name {
        (ph::STACK, name.as_str(), "Shared properties".into())
    } else if let Some(shape) = selected {
        (
            geometry_icon(&shape.geom),
            shape.name.as_str(),
            "Object properties".into(),
        )
    } else if !studio.artboard_sel.is_empty() {
        (ph::FRAME_CORNERS, "Artboard", "Canvas properties".into())
    } else if let Some(layer) = studio
        .primary()
        .and_then(|(li, _)| studio.doc.layers.get(li))
    {
        (ph::IMAGES, layer.name.as_str(), "Image properties".into())
    } else {
        (
            if studio.persona == Persona::Pixel {
                ph::PAINT_BRUSH
            } else {
                ph::SHAPES
            },
            studio.tool.label(),
            "Defaults for new objects".into(),
        )
    };
    ui.horizontal(|ui| {
        let (rect, _) = ui.allocate_exact_size(vec2(34.0, 38.0), eframe::egui::Sense::hover());
        ui.painter()
            .rect_filled(rect.shrink2(vec2(0.0, 2.0)), 9.0, accent_soft());
        ui.painter().text(
            rect.center(),
            eframe::egui::Align2::CENTER_CENTER,
            icon,
            icons::font(22.0),
            accent(),
        );
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 3.0;
            ui.add(eframe::egui::Label::new(RichText::new(name).size(14.0).strong()).truncate());
            ui.add(
                eframe::egui::Label::new(RichText::new(description).size(10.0).color(fg_weak()))
                    .truncate(),
            );
        });
    });
}

fn inspected_style(studio: &Studio) -> &crate::document::Style {
    studio
        .primary()
        .and_then(|(li, id)| studio.doc.find_shape(li, id))
        .map(|shape| &shape.style)
        .unwrap_or(&studio.style)
}

fn number_field<N: eframe::egui::emath::Numeric>(
    ui: &mut Ui,
    label: &str,
    value: &mut N,
    range: std::ops::RangeInclusive<N>,
    suffix: &str,
) -> bool {
    let width = ui.available_width();
    Frame::new()
        .fill(bg_widget())
        .corner_radius(6.0)
        .inner_margin(Margin::symmetric(8, 3))
        .show(ui, |ui| {
            ui.set_min_width((width - 16.0).max(24.0));
            ui.spacing_mut().interact_size.y = 20.0;
            ui.spacing_mut().button_padding = vec2(2.0, 1.0);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                ui.label(RichText::new(label).size(11.0).color(fg_weak()));
                ui.with_layout(Layout::right_to_left(eframe::egui::Align::Center), |ui| {
                    ui.add(
                        eframe::egui::DragValue::new(value)
                            .speed(0.5)
                            .range(range)
                            .max_decimals(2)
                            .min_decimals(0)
                            .suffix(suffix),
                    )
                    .changed()
                })
                .inner
            })
            .inner
        })
        .inner
}

fn bounds_fields(
    ui: &mut Ui,
    x: &mut f32,
    y: &mut f32,
    width: &mut f32,
    height: &mut f32,
    min_size: f32,
) -> bool {
    let mut changed = false;
    ui.columns(2, |columns| {
        changed |= number_field(&mut columns[0], "X", x, -100000.0..=100000.0, "");
        changed |= number_field(&mut columns[1], "Y", y, -100000.0..=100000.0, "");
    });
    ui.columns(2, |columns| {
        changed |= number_field(&mut columns[0], "W", width, min_size..=20000.0, "");
        changed |= number_field(&mut columns[1], "H", height, min_size..=20000.0, "");
    });
    changed
}

fn color_row(ui: &mut Ui, studio: &mut Studio, fill: bool) {
    let style = inspected_style(studio);
    let source_fill = style.fill.clone();
    let source_stroke = style.stroke.clone();
    let original = if fill {
        match source_fill {
            Fill::Solid(c) | Fill::Linear { c0: c, .. } | Fill::Radial { c0: c, .. } => Some(c),
            Fill::None => None,
        }
    } else {
        source_stroke.as_ref().map(|stroke| stroke.color)
    };
    if original.is_none() {
        let label = if fill { "Add fill" } else { "Add stroke" };
        let mut add = false;
        ui.horizontal(|ui| {
            add |= icons::tiny_icon(ui, ph::PLUS, label, false);
            add |= ui
                .add(
                    eframe::egui::Button::new(RichText::new(label).size(11.0).color(fg_weak()))
                        .frame(false),
                )
                .clicked();
        });
        if add {
            let mut color = if fill {
                source_stroke
                    .as_ref()
                    .map(|stroke| stroke.color)
                    .unwrap_or(studio.brush.color)
            } else {
                match &source_fill {
                    Fill::Solid(color)
                    | Fill::Linear { c0: color, .. }
                    | Fill::Radial { c0: color, .. } => *color,
                    Fill::None => studio.brush.color,
                }
            };
            color.a = 255;
            studio.fill_active = fill;
            if fill {
                studio.set_fill(Fill::Solid(color));
                studio.brush.color = color;
            } else {
                let stroke = DocStroke {
                    color,
                    ..Default::default()
                };
                studio.style.stroke = Some(stroke.clone());
                apply_stroke(studio, Some(stroke));
            }
        }
        return;
    }
    let mut color = original.unwrap_or(studio.brush.color);
    let mut changed = false;
    let mut remove = false;
    let label = if fill { "Fill" } else { "Stroke" };
    ui.push_id(label, |ui| {
        let width = ui.available_width();
        Frame::new()
            .fill(bg_widget())
            .corner_radius(7.0)
            .inner_margin(Margin::symmetric(7, 3))
            .show(ui, |ui| {
                ui.set_min_width(width - 14.0);
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 5.0;
                    if ui
                        .add_sized(
                            vec2(42.0, 22.0),
                            eframe::egui::Button::new(RichText::new(label).size(11.0).color(
                                if studio.fill_active == fill {
                                    fg()
                                } else {
                                    fg_weak()
                                },
                            ))
                            .frame(false),
                        )
                        .clicked()
                    {
                        studio.fill_active = fill;
                    }
                    let mut swatch = color.to_egui();
                    ui.spacing_mut().interact_size = vec2(20.0, 20.0);
                    if ui.color_edit_button_srgba(&mut swatch).changed() {
                        color = Rgba::from_egui(swatch);
                        changed = true;
                        studio.fill_active = fill;
                    }
                    let input_id = ui.make_persistent_id(("hex", studio.primary()));
                    let mut hex = ui
                        .data(|d| d.get_temp::<(Option<Rgba>, String)>(input_id))
                        .filter(|(source, _)| *source == original)
                        .map(|(_, text)| text)
                        .unwrap_or_else(|| original.map(|c| c.hex()).unwrap_or_default());
                    let field_width = (ui.available_width() - 94.0).max(28.0);
                    let response = ui.add(
                        eframe::egui::TextEdit::singleline(&mut hex)
                            .id(input_id)
                            .desired_width(field_width)
                            .font(eframe::egui::TextStyle::Small)
                            .frame(Frame::NONE)
                            .hint_text("None"),
                    );
                    if response.lost_focus()
                        && let Some(value) = Rgba::parse_hex(&hex)
                    {
                        changed |= original != Some(value);
                        color = value;
                    }
                    ui.data_mut(|d| d.insert_temp(input_id, (original, hex)));
                    let mut opacity = f32::from(color.a) / 255.0 * 100.0;
                    if ui
                        .add(
                            eframe::egui::DragValue::new(&mut opacity)
                                .range(0.0..=100.0)
                                .speed(0.5)
                                .max_decimals(0)
                                .suffix("%"),
                        )
                        .on_hover_text(format!("{label} opacity"))
                        .changed()
                    {
                        color.a = (opacity / 100.0 * 255.0).round() as u8;
                        changed = true;
                    }
                    remove = icons::tiny_icon(
                        ui,
                        ph::MINUS,
                        &format!("Remove {}", label.to_lowercase()),
                        false,
                    );
                });
            });
    });
    if remove {
        if fill {
            studio.set_fill(Fill::None);
        } else {
            studio.style.stroke = None;
            apply_stroke(studio, None);
        }
    } else if changed {
        if fill {
            let fill = match source_fill {
                Fill::Linear { from, to, c1, .. } => Fill::Linear {
                    from,
                    to,
                    c0: color,
                    c1,
                },
                Fill::Radial { c1, .. } => Fill::Radial { c0: color, c1 },
                _ => Fill::Solid(color),
            };
            studio.set_fill(fill);
            studio.brush.color = color;
        } else {
            let mut stroke = source_stroke.unwrap_or_default();
            stroke.color = color;
            studio.style.stroke = Some(stroke.clone());
            apply_stroke(studio, Some(stroke));
        }
    }
}

fn color_studio(ui: &mut Ui, studio: &mut Studio) {
    ui.horizontal(|ui| {
        heading(ui, "Appearance");
        ui.with_layout(Layout::right_to_left(eframe::egui::Align::Center), |ui| {
            ui.menu_button("···", |ui| {
                if ui.button("Swap fill and stroke   X").clicked() {
                    studio.style = inspected_style(studio).clone();
                    studio.swap_fill_stroke();
                    ui.close();
                }
                if ui.button("Linear gradient").clicked() {
                    studio.set_fill(Fill::Linear {
                        from: [0.0, 0.0],
                        to: [1.0, 0.0],
                        c0: studio.gradient.0,
                        c1: studio.gradient.1,
                    });
                    ui.close();
                }
                if ui.button("Radial gradient").clicked() {
                    studio.set_fill(Fill::Radial {
                        c0: studio.gradient.0,
                        c1: studio.gradient.1,
                    });
                    ui.close();
                }
                if ui.button("Solid fill").clicked() {
                    studio.set_fill(Fill::Solid(studio.brush.color));
                    ui.close();
                }
            });
        });
    });
    color_row(ui, studio, true);
    color_row(ui, studio, false);
    let fill = inspected_style(studio).fill.clone();
    if let Fill::Linear { c0, c1, .. } | Fill::Radial { c0, c1 } = fill {
        ui.horizontal(|ui| {
            ui.label(RichText::new("Gradient").small().color(fg_weak()));
            let mut a = c0.to_egui();
            let mut b = c1.to_egui();
            let changed = ui.color_edit_button_srgba(&mut a).changed()
                | ui.color_edit_button_srgba(&mut b).changed();
            if changed {
                let c0 = Rgba::from_egui(a);
                let c1 = Rgba::from_egui(b);
                studio.gradient = (c0, c1);
                studio.set_fill(match fill {
                    Fill::Linear { from, to, .. } => Fill::Linear { from, to, c0, c1 },
                    _ => Fill::Radial { c0, c1 },
                });
            }
        });
    }
    ui.horizontal_wrapped(|ui| {
        ui.menu_button("Color library", |ui| {
            ui.set_width(236.0);
            heading(ui, "Quick colors");
            color_grid(ui, studio, false);
            if !studio.recent.is_empty() {
                ui.add_space(8.0);
                heading(ui, "Recently used");
                color_grid(ui, studio, true);
            }
            eframe::egui::CollapsingHeader::new("Saved palettes")
                .show(ui, |ui| palette_ui(ui, studio));
        });
        if inspected_style(studio).stroke.is_some() {
            ui.menu_button("Stroke details", |ui| {
                ui.set_width(236.0);
                stroke_studio(ui, studio);
            });
        }
    });
}

fn color_grid(ui: &mut Ui, studio: &mut Studio, recent: bool) {
    let count = if recent {
        studio.recent.len()
    } else {
        studio.swatches.len()
    };
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = vec2(6.0, 6.0);
        for index in 0..count {
            let color = if recent {
                studio.recent[index]
            } else {
                studio.swatches[index]
            };
            let (rect, response) =
                ui.allocate_exact_size(vec2(20.0, 20.0), eframe::egui::Sense::click());
            ui.painter().rect_filled(rect, 5.0, color.to_egui());
            ui.painter().rect_stroke(
                rect,
                5.0,
                Stroke::new(1.0, border()),
                eframe::egui::StrokeKind::Inside,
            );
            let response = response.on_hover_text(color.hex());
            if response.clicked() {
                if studio.fill_active {
                    studio.set_fill(Fill::Solid(color));
                    studio.brush.color = color;
                } else {
                    studio.style = inspected_style(studio).clone();
                    studio.set_stroke_color(color);
                }
            } else if response.secondary_clicked() {
                studio.style = inspected_style(studio).clone();
                studio.set_stroke_color(color);
            }
        }
    });
}

fn palette_ui(ui: &mut Ui, studio: &mut Studio) {
    if studio.palettes.is_empty() {
        studio.palettes = crate::palette::load();
    }
    if studio.palette_idx >= studio.palettes.len() {
        studio.palette_idx = 0;
    }
    ui.horizontal(|ui| {
        if ui.small_button("New").clicked() {
            let name = if studio.palette_name_buf.trim().is_empty() {
                format!("Palette {}", studio.palettes.len() + 1)
            } else {
                studio.palette_name_buf.trim().to_string()
            };
            if crate::palette::validate_name(&name).is_ok() {
                studio
                    .palettes
                    .push(crate::palette::Palette::new(name.clone(), vec![]));
                studio.palette_idx = studio.palettes.len() - 1;
                studio.palette_name_buf = name;
                let _ = crate::palette::save(&studio.palettes);
            }
        }
        if ui.small_button("Delete").clicked() && studio.palettes.len() > 1 {
            studio.palettes.remove(studio.palette_idx);
            studio.palette_idx = studio.palette_idx.min(studio.palettes.len() - 1);
            studio.palette_name_buf = studio.palettes[studio.palette_idx].name.clone();
            let _ = crate::palette::save(&studio.palettes);
        }
    });
    let names: Vec<String> = studio.palettes.iter().map(|p| p.name.clone()).collect();
    let cur_name = names.get(studio.palette_idx).cloned().unwrap_or_default();
    ComboBox::from_id_salt("palette-select")
        .selected_text(cur_name)
        .width(180.0)
        .show_ui(ui, |ui| {
            for (i, n) in names.iter().enumerate() {
                if ui.selectable_value(&mut studio.palette_idx, i, n).clicked()
                    && let Some(p) = studio.palettes.get(i)
                {
                    studio.palette_name_buf = p.name.clone();
                }
            }
        });
    ui.horizontal(|ui| {
        ui.add(
            eframe::egui::TextEdit::singleline(&mut studio.palette_name_buf)
                .desired_width(120.0)
                .hint_text("name"),
        );
        if ui.small_button("Rename").clicked() {
            let new_name = studio.palette_name_buf.trim().to_string();
            if crate::palette::validate_name(&new_name).is_ok()
                && let Some(p) = studio.palettes.get_mut(studio.palette_idx)
            {
                p.name = new_name;
                let _ = crate::palette::save(&studio.palettes);
            }
        }
    });
    if let Some(pal) = studio.palettes.get(studio.palette_idx).cloned() {
        ui.horizontal_wrapped(|ui| {
            for (idx, c) in pal.colors.iter().cloned().enumerate() {
                let (rect, resp) =
                    ui.allocate_exact_size(vec2(16.0, 16.0), eframe::egui::Sense::click());
                ui.painter().rect_filled(rect, 2.0, c.to_egui());
                if resp.clicked() {
                    if studio.fill_active {
                        studio.set_fill(Fill::Solid(c));
                        studio.brush.color = c;
                    } else {
                        studio.style = inspected_style(studio).clone();
                        studio.set_stroke_color(c);
                    }
                }
                if resp.secondary_clicked() {
                    if let Some(p) = studio.palettes.get_mut(studio.palette_idx)
                        && idx < p.colors.len()
                    {
                        p.colors.remove(idx);
                    }
                    let _ = crate::palette::save(&studio.palettes);
                }
            }
        });
        ui.horizontal(|ui| {
            if ui.small_button("+ Fill").clicked() {
                let col = match inspected_style(studio).fill {
                    Fill::Solid(c) => c,
                    Fill::Linear { c0, .. } | Fill::Radial { c0, .. } => c0,
                    Fill::None => studio.brush.color,
                };
                if let Some(p) = studio.palettes.get_mut(studio.palette_idx)
                    && !p.colors.contains(&col)
                {
                    p.colors.push(col);
                    let _ = crate::palette::save(&studio.palettes);
                }
            }
            if ui.small_button("Clear").clicked() {
                if let Some(p) = studio.palettes.get_mut(studio.palette_idx) {
                    p.colors.clear();
                }
                let _ = crate::palette::save(&studio.palettes);
            }
        });
        if pal.colors.is_empty() {
            ui.label(
                RichText::new(
                    "Empty. + Fill adds the current colour. Right-click a swatch to remove.",
                )
                .small()
                .color(fg_weak()),
            );
        }
    }
}

fn stroke_studio(ui: &mut Ui, studio: &mut Studio) {
    let mut stroke = inspected_style(studio).stroke.clone().unwrap_or_default();
    let mut changed = number_field(ui, "Width", &mut stroke.width, 0.0..=64.0, " px");
    ui.horizontal(|ui| {
        ui.label(RichText::new("Ends").small().color(fg_weak()));
        for (cap, label) in [
            (Cap::Butt, "Flat"),
            (Cap::Round, "Round"),
            (Cap::Square, "Square"),
        ] {
            changed |= ui.selectable_value(&mut stroke.cap, cap, label).changed();
        }
    });
    ui.horizontal(|ui| {
        ui.label(RichText::new("Joins").small().color(fg_weak()));
        for (join, label) in [
            (Join::Miter, "Sharp"),
            (Join::Round, "Round"),
            (Join::Bevel, "Bevel"),
        ] {
            changed |= ui.selectable_value(&mut stroke.join, join, label).changed();
        }
    });
    let mut dashed = stroke.dash.is_some();
    if ui.checkbox(&mut dashed, "Dashed line").changed() {
        stroke.dash = if dashed { Some((6.0, 4.0)) } else { None };
        changed = true;
    }
    if changed {
        studio.style.stroke = if stroke.width <= 0.01 {
            None
        } else {
            Some(stroke)
        };
        apply_stroke(studio, studio.style.stroke.clone());
    }
}

fn apply_stroke(studio: &mut Studio, stroke: Option<DocStroke>) {
    for (li, id) in studio.selection.clone() {
        if let Some(s) = studio.doc.find_shape(li, id) {
            let mut after = s.style.clone();
            after.stroke = stroke.clone();
            studio.commit(crate::document::Cmd::SetStyle {
                layer: li,
                id,
                before: s.style.clone(),
                after,
            });
        }
    }
}

fn character_studio(ui: &mut Ui, studio: &mut Studio) {
    heading(ui, "Typography");
    let live = studio.selected_type();
    let font = live
        .as_ref()
        .map(|t| t.font.clone())
        .unwrap_or_else(|| studio.text_font.clone());
    let label = crate::text::label_for(&font);
    let mut chosen = font.clone();
    let font_button = ui.add_sized(
        vec2(ui.available_width(), 28.0),
        eframe::egui::Button::new(RichText::new(&label).size(12.0)).truncate(),
    );
    eframe::egui::Popup::menu(&font_button).show(|ui| {
        ui.set_width(260.0);
        ui.add(
            eframe::egui::TextEdit::singleline(&mut studio.font_query)
                .hint_text("Search fonts")
                .desired_width(220.0),
        );
        let q = studio.font_query.to_ascii_lowercase();
        let recents = studio.font_recents.clone();
        let used = studio.used_fonts();
        let all = crate::text::all_fonts_cached();
        ScrollArea::vertical()
            .id_salt("font-list")
            .max_height(180.0)
            .show(ui, |ui| {
                if !recents.is_empty() {
                    ui.label(RichText::new("Recents").small().color(fg_weak()));
                    for p in recents.iter().take(5) {
                        let name = crate::text::label_for(p);
                        if ui.selectable_label(*p == font, &name).clicked() {
                            chosen = p.clone();
                        }
                    }
                }
                if !used.is_empty() {
                    ui.label(RichText::new("Used in document").small().color(fg_weak()));
                    for p in &used {
                        let name = crate::text::label_for(p);
                        if ui.selectable_label(*p == font, &name).clicked() {
                            chosen = p.clone();
                        }
                    }
                }
                ui.label(RichText::new("All").small().color(fg_weak()));
                for f in all.iter() {
                    if !q.is_empty() && !f.name.to_ascii_lowercase().contains(&q) {
                        continue;
                    }
                    let path = f.path.to_string_lossy().to_string();
                    let resp = ui.selectable_label(path == font, &f.name);
                    if path == font && studio.font_scroll_once {
                        resp.scroll_to_me(Some(eframe::egui::Align::Center));
                        studio.font_scroll_once = false;
                    }
                    if resp.clicked() {
                        chosen = path;
                    }
                }
            });
    });
    if chosen != font {
        let path = chosen.clone();
        studio.remember_font(&path);
        studio.patch_type(|t| t.font = path.clone());
        studio.text_font = path;
        studio.font_scroll_once = true;
    }
    eframe::egui::CollapsingHeader::new("Google Fonts")
        .default_open(false)
        .show(ui, |ui| google_fonts_ui(ui, studio));

    let mut px = live.as_ref().map(|t| t.px).unwrap_or(studio.text_px);
    if number_field(ui, "Size", &mut px, 8.0..=400.0, " px") {
        studio.patch_type(|t| t.px = px);
    }
    let mut track = live
        .as_ref()
        .map(|t| t.tracking)
        .unwrap_or(studio.text_tracking);
    if number_field(ui, "Letter spacing", &mut track, -40.0..=80.0, "") {
        studio.patch_type(|t| t.tracking = track);
    }
    let mut lead = live
        .as_ref()
        .map(|t| t.leading)
        .unwrap_or(studio.text_leading);
    if number_field(ui, "Line height", &mut lead, 0.0..=400.0, "") {
        studio.patch_type(|t| t.leading = lead);
    }
    eframe::egui::CollapsingHeader::new("OpenType features").show(ui, |ui| {
        let mut kern = live.as_ref().map(|t| t.kern).unwrap_or(studio.text_kern);
        let mut liga = live.as_ref().map(|t| t.liga).unwrap_or(studio.text_liga);
        let mut tnum = live.as_ref().map(|t| t.tnum).unwrap_or(studio.text_tnum);
        let mut smcp = live.as_ref().map(|t| t.smcp).unwrap_or(studio.text_smcp);
        ui.horizontal(|ui| {
            if ui.checkbox(&mut kern, "Kerning").changed() {
                studio.patch_type(|t| t.kern = kern);
            }
            if ui.checkbox(&mut liga, "Ligatures").changed() {
                studio.patch_type(|t| t.liga = liga);
            }
        });
        ui.horizontal(|ui| {
            if ui.checkbox(&mut tnum, "Tabular figs").changed() {
                studio.patch_type(|t| t.tnum = tnum);
            }
            if ui.checkbox(&mut smcp, "Small caps").changed() {
                studio.patch_type(|t| t.smcp = smcp);
            }
        });
    });
    if studio.type_edit.is_some() {
        ui.label(
            RichText::new("Typing on the canvas. Esc finishes.")
                .small()
                .color(accent()),
        );
    } else if live.is_none() {
        ui.label(
            RichText::new("Applies to the next type you place.")
                .small()
                .color(fg_weak()),
        );
    }
}

struct DownloadedFont {
    path: std::path::PathBuf,
    family: String,
    document: String,
    selection: Vec<(usize, u64)>,
}

fn google_fonts_ui(ui: &mut Ui, studio: &mut Studio) {
    use crate::ui::jobs;
    const CATALOG: &str = "google-font-catalog";
    const DOWNLOAD: &str = "google-font-download";
    if let Some(result) = jobs::poll::<Vec<crate::google_fonts::GoogleFont>>(ui.ctx(), CATALOG) {
        match result {
            Ok(catalog) => {
                studio.google_status = format!("{} families", catalog.len());
                studio.google_catalog = catalog;
            }
            Err(error) => studio.google_status = error,
        }
    }
    if let Some(result) = jobs::poll::<DownloadedFont>(ui.ctx(), DOWNLOAD) {
        match result {
            Ok(font) => {
                if studio.swap_id == font.document && studio.selection == font.selection {
                    let chosen = font.path.to_string_lossy().into_owned();
                    studio.patch_type(|text| text.font = chosen.clone());
                    studio.text_font = chosen;
                }
                studio.google_status = format!("Installed {}", font.family);
            }
            Err(error) => studio.google_status = error,
        }
    }
    if !studio.google_catalog_loaded {
        studio.google_catalog_loaded = true;
        studio.google_status = "Loading font families…".into();
        jobs::start(ui.ctx(), CATALOG, || Ok(crate::google_fonts::catalog()));
    }
    let loading = jobs::is_running::<Vec<crate::google_fonts::GoogleFont>>(ui.ctx(), CATALOG);
    let downloading = jobs::is_running::<DownloadedFont>(ui.ctx(), DOWNLOAD);
    ui.horizontal(|ui| {
        ui.add(
            eframe::egui::TextEdit::singleline(&mut studio.google_query)
                .hint_text("Search fonts")
                .desired_width(140.0),
        );
        if ui
            .add_enabled(!loading, eframe::egui::Button::new("Refresh"))
            .clicked()
        {
            studio.google_status = "Refreshing font families…".into();
            jobs::start(ui.ctx(), CATALOG, crate::google_fonts::fetch_catalog);
        }
    });
    ui.horizontal(|ui| {
        ui.label(RichText::new("Variant").small().color(fg_weak()));
        ComboBox::from_id_salt("google-variant")
            .selected_text(&studio.google_variant)
            .width(100.0)
            .show_ui(ui, |ui| {
                for (variant, label) in [
                    ("regular", "Regular"),
                    ("italic", "Italic"),
                    ("700", "Bold"),
                    ("700italic", "Bold italic"),
                ] {
                    ui.selectable_value(&mut studio.google_variant, variant.to_owned(), label);
                }
            });
    });
    let mut use_font = None;
    let mut download = None;
    let filtered = crate::google_fonts::search(&studio.google_catalog, &studio.google_query);
    let installed_fonts = crate::text::all_fonts_cached();
    ScrollArea::vertical()
        .id_salt("google-font-results")
        .max_height(180.0)
        .show(ui, |ui| {
            for font in filtered {
                ui.horizontal(|ui| {
                    ui.add(
                        eframe::egui::Label::new(RichText::new(&font.family).small()).truncate(),
                    );
                    let path =
                        crate::google_fonts::installed_path(&font.family, &studio.google_variant);
                    let installed = installed_fonts.iter().any(|face| face.path == path);
                    ui.with_layout(Layout::right_to_left(eframe::egui::Align::Center), |ui| {
                        if installed {
                            if ui.small_button("Use").clicked() {
                                use_font = Some((font.family.clone(), path));
                            }
                        } else if ui
                            .add_enabled(!downloading, eframe::egui::Button::new("Download"))
                            .clicked()
                        {
                            download = Some(font.clone());
                        }
                    });
                });
            }
        });
    if let Some((family, path)) = use_font {
        let chosen = path.to_string_lossy().into_owned();
        studio.patch_type(|text| text.font = chosen.clone());
        studio.text_font = chosen;
        studio.google_status = format!("Using {family}");
    }
    if let Some(font) = download {
        let variant = studio.google_variant.clone();
        let document = studio.swap_id.clone();
        let selection = studio.selection.clone();
        studio.google_status = format!("Downloading {}…", font.family);
        jobs::start(ui.ctx(), DOWNLOAD, move || {
            let path =
                crate::google_fonts::download(&font.family, &variant, std::slice::from_ref(&font))?;
            Ok(DownloadedFont {
                path,
                family: font.family,
                document,
                selection,
            })
        });
    }
    if !studio.google_status.is_empty() {
        ui.label(
            RichText::new(&studio.google_status)
                .small()
                .color(fg_weak()),
        );
    }
}

fn motion_studio(ui: &mut Ui, studio: &mut Studio) {
    heading(ui, "Motion");
    ui.label(
        RichText::new("Rest pose is Design. Keys are offsets.")
            .small()
            .color(fg_weak()),
    );
    ui.add_space(4.0);
    if studio.selection.is_empty() {
        ui.label(
            RichText::new("Select a shape, then Key.")
                .small()
                .color(fg_weak()),
        );
        return;
    }
    let Some((_, id)) = studio.primary() else {
        return;
    };
    let pose = studio.live_pose(id);
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(format!("X  {:+.1}", pose.dx))
                .small()
                .monospace(),
        );
        if ui.small_button("Key").clicked() {
            studio.key_prop(id, crate::motion::Prop::X, pose.dx);
        }
    });
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(format!("Y  {:+.1}", pose.dy))
                .small()
                .monospace(),
        );
        if ui.small_button("Key").clicked() {
            studio.key_prop(id, crate::motion::Prop::Y, pose.dy);
        }
    });
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(format!("R  {:+.1}°", pose.rotation.to_degrees()))
                .small()
                .monospace(),
        );
        if ui.small_button("Key").clicked() {
            studio.key_prop(id, crate::motion::Prop::Rotation, pose.rotation);
        }
    });
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(format!("S  {:.2}", pose.scale))
                .small()
                .monospace(),
        );
        if ui.small_button("Key").clicked() {
            studio.key_prop(id, crate::motion::Prop::Scale, pose.scale);
        }
    });
    let mut op = pose.opacity.unwrap_or_else(|| {
        studio
            .doc
            .find_shape(studio.primary().map(|p| p.0).unwrap_or(0), id)
            .map(|s| s.opacity)
            .unwrap_or(1.0)
    });
    ui.horizontal(|ui| {
        if ui
            .add(Slider::new(&mut op, 0.0..=1.0).text("Opacity"))
            .changed()
        {
            studio.key_prop(id, crate::motion::Prop::Opacity, op);
        }
    });
}

fn artboard_transform(ui: &mut Ui, studio: &mut Studio) {
    let Some(id) = studio.artboard_sel.first().copied() else {
        ui.label(
            RichText::new("Draw or click an artboard")
                .small()
                .color(fg_weak()),
        );
        if ui.button("Wrap selection in artboard").clicked() {
            studio.wrap_selection_artboard();
        }
        return;
    };
    let Some(idx) = studio.doc.artboards.iter().position(|a| a.id == id) else {
        return;
    };
    let mut a = studio.doc.artboards[idx].clone();
    if studio.artboard_rename.as_ref().map(|(i, _)| *i) == Some(id) {
        if let Some((_, buf)) = studio.artboard_rename.as_mut() {
            let r = ui.add(eframe::egui::TextEdit::singleline(buf).desired_width(160.0));
            if r.lost_focus() {
                let name = buf.trim().to_string();
                studio.artboard_rename = None;
                if !name.is_empty() {
                    a.name = studio.doc.unique_artboard_name(&name);
                    let mut after = studio.doc.artboards.clone();
                    if let Some(slot) = after.iter_mut().find(|x| x.id == id) {
                        *slot = a.clone();
                    }
                    studio.commit_artboards(after);
                }
            }
        }
    } else if ui.button(&a.name).on_hover_text("Rename").clicked() {
        studio.artboard_rename = Some((id, a.name.clone()));
    }
    let mut x = a.origin.x;
    let mut y = a.origin.y;
    let mut w = a.size.x;
    let mut h = a.size.y;
    let mut deg = a.rotation.to_degrees();
    let mut changed = bounds_fields(ui, &mut x, &mut y, &mut w, &mut h, 8.0);
    changed |= number_field(ui, "Rotation", &mut deg, -180.0..=180.0, "°");
    if changed {
        let orig = a.clone();
        a.origin = crate::geom::Pt::new(x, y);
        a.size = crate::geom::Pt::new(w, h);
        a.rotation = crate::document::Artboard::snap_rotation(deg.to_radians());
        let snaps = studio.snapshot_artboard_contents(&orig);
        let mut after = studio.doc.artboards.clone();
        if let Some(slot) = after.iter_mut().find(|x| x.id == id) {
            *slot = a.clone();
        }
        studio.commit_artboards(after);
        studio.apply_artboard_contents(&orig, &a, &snaps);
        studio.commit_mapped_contents(snaps);
    }
    ui.horizontal(|ui| {
        if ui.button("Clone").clicked() {
            studio.clone_artboard(id);
        }
        if ui.button("Delete").clicked() {
            studio.delete_artboards();
        }
    });
}

fn raster_transform(ui: &mut Ui, studio: &mut Studio, li: usize) {
    let Some(layer) = studio.doc.layers.get(li) else {
        return;
    };
    let Some((origin, size, rot)) = layer.kind.raster_xform() else {
        return;
    };
    let mut x = origin.x;
    let mut y = origin.y;
    let mut w = size.x;
    let mut h = size.y;
    let mut deg = rot.to_degrees();
    let mut changed = bounds_fields(ui, &mut x, &mut y, &mut w, &mut h, 1.0);
    changed |= number_field(ui, "Rotation", &mut deg, -180.0..=180.0, "°");
    if changed {
        studio.commit(crate::document::Cmd::SetRasterXform {
            layer: li,
            before: (origin, size, rot),
            after: (
                crate::geom::Pt::new(x, y),
                crate::geom::Pt::new(w, h),
                deg.to_radians(),
            ),
        });
    }
}

fn edit_shape_geometry(studio: &mut Studio, layer: usize, id: u64, edit: impl FnOnce(&mut Geom)) {
    let Some(shape) = studio.doc.find_shape(layer, id) else {
        return;
    };
    let mut after = shape.geom.clone();
    edit(&mut after);
    studio.commit(crate::document::Cmd::SetGeom {
        layer,
        id,
        before: shape.geom.clone(),
        after,
        rot_before: shape.rotation,
        rot_after: shape.rotation,
    });
}

fn transform_studio(ui: &mut Ui, studio: &mut Studio, title: bool) {
    if title {
        heading(ui, "Layout");
    }
    if studio.tool == Tool::Artboard
        || (studio.selection.is_empty() && !studio.artboard_sel.is_empty())
    {
        artboard_transform(ui, studio);
        return;
    }
    if studio.selection.is_empty() {
        match studio.tool {
            Tool::Polygon => {
                number_field(ui, "Sides", &mut studio.polygon_sides, 3..=12, "");
            }
            Tool::Star => {
                number_field(ui, "Points", &mut studio.star_points, 3..=12, "");
                number_field(ui, "Inner radius", &mut studio.star_inner, 0.15..=0.8, "");
            }
            Tool::Rect => {
                number_field(
                    ui,
                    "Corner radius",
                    &mut studio.rect_radius,
                    0.0..=80.0,
                    " px",
                );
            }
            _ => {}
        }
        return;
    }
    let Some((layer, id)) = studio.primary() else {
        return;
    };
    if id == crate::document::RASTER_ID {
        raster_transform(ui, studio, layer);
        return;
    }
    let Some(shape) = studio.doc.find_shape(layer, id) else {
        return;
    };
    let bounds = shape.world_bbox();
    let rotation = shape.rotation;
    let opacity = shape.opacity;
    let polygon = if let Geom::Polygon { sides, .. } = shape.geom {
        Some(sides)
    } else {
        None
    };
    let star = if let Geom::Star { points, inner, .. } = shape.geom {
        Some((points, inner))
    } else {
        None
    };
    let radius = if let Geom::Rect { radius, .. } = shape.geom {
        Some(radius)
    } else {
        None
    };
    let compound = matches!(&shape.geom, Geom::Poly { contours, .. } if contours.len() > 1);
    let mut x = bounds.min.x;
    let mut y = bounds.min.y;
    let mut width = bounds.width().max(1.0);
    let mut height = bounds.height().max(1.0);
    let changed = bounds_fields(ui, &mut x, &mut y, &mut width, &mut height, 1.0);
    if changed {
        let destination = crate::geom::Bounds {
            min: crate::geom::Pt::new(x, y),
            max: crate::geom::Pt::new(x + width, y + height),
        };
        edit_shape_geometry(studio, layer, id, |geometry| {
            geometry.map_into(bounds, destination)
        });
    }
    let mut degrees = rotation.to_degrees();
    let mut opacity_percent = opacity * 100.0;
    let mut rotation_changed = false;
    let mut opacity_changed = false;
    ui.columns(2, |columns| {
        rotation_changed =
            number_field(&mut columns[0], "Rotate", &mut degrees, -180.0..=180.0, "°");
        opacity_changed = number_field(
            &mut columns[1],
            "Opacity",
            &mut opacity_percent,
            0.0..=100.0,
            "%",
        );
    });
    if rotation_changed && let Some(shape) = studio.doc.find_shape(layer, id) {
        studio.commit(crate::document::Cmd::SetGeom {
            layer,
            id,
            before: shape.geom.clone(),
            after: shape.geom.clone(),
            rot_before: rotation,
            rot_after: degrees.to_radians(),
        });
    }
    if opacity_changed {
        studio.commit(crate::document::Cmd::SetOpacity {
            layer,
            id,
            before: opacity,
            after: opacity_percent / 100.0,
        });
    }
    ui.horizontal(|ui| {
        ui.label(RichText::new("Flip").small().color(fg_weak()));
        if ui.small_button("Horizontal").clicked() {
            studio.flip_selection(true);
        }
        if ui.small_button("Vertical").clicked() {
            studio.flip_selection(false);
        }
    });
    if let Some(mut count) = polygon
        && number_field(ui, "Sides", &mut count, 3..=16, "")
    {
        edit_shape_geometry(studio, layer, id, |geometry| {
            if let Geom::Polygon { sides, .. } = geometry {
                *sides = count;
            }
        });
    }
    if let Some((mut count, mut radius)) = star {
        let mut changed = false;
        let mut inner_percent = radius * 100.0;
        ui.columns(2, |columns| {
            changed |= number_field(&mut columns[0], "Points", &mut count, 3..=16, "");
            changed |= number_field(
                &mut columns[1],
                "Inner",
                &mut inner_percent,
                15.0..=85.0,
                "%",
            );
        });
        radius = inner_percent / 100.0;
        if changed {
            edit_shape_geometry(studio, layer, id, |geometry| {
                if let Geom::Star { points, inner, .. } = geometry {
                    *points = count;
                    *inner = radius;
                }
            });
        }
    }
    if let Some(mut value) = radius
        && number_field(ui, "Corner radius", &mut value, 0.0..=200.0, " px")
    {
        edit_shape_geometry(studio, layer, id, |geometry| {
            if let Geom::Rect { radius, .. } = geometry {
                *radius = value;
            }
        });
    }
    if studio.selection.len() >= 2 {
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            for (icon, tip, alignment) in [
                (ph::ALIGN_LEFT, "Align left", crate::align::Align::Left),
                (
                    ph::ALIGN_CENTER_H,
                    "Align centre",
                    crate::align::Align::CenterX,
                ),
                (ph::ALIGN_RIGHT, "Align right", crate::align::Align::Right),
                (ph::ALIGN_TOP, "Align top", crate::align::Align::Top),
                (
                    ph::ALIGN_CENTER_V,
                    "Align middle",
                    crate::align::Align::CenterY,
                ),
                (
                    ph::ALIGN_BOTTOM,
                    "Align bottom",
                    crate::align::Align::Bottom,
                ),
            ] {
                if icons::tiny_icon(ui, icon, tip, false) {
                    studio.align_sel(alignment);
                }
            }
        });
        ui.horizontal_wrapped(|ui| {
            for operation in crate::boolean::BoolOp::all() {
                if ui.small_button(operation.name()).clicked() {
                    studio.apply_boolean_multi(operation);
                }
            }
            if ui.small_button("Combine").on_hover_text("Ctrl+G").clicked() {
                studio.combine_selected();
            }
        });
    }
    if compound
        && ui
            .small_button("Release compound")
            .on_hover_text("Ctrl+Shift+G")
            .clicked()
    {
        studio.release_compound();
    }
}

fn fx_stack_editor(ui: &mut Ui, stack: &mut crate::filter::FilterStack, salt: &str) {
    ui.horizontal(|ui| {
        ui.checkbox(&mut stack.enabled, "Enabled");
        ComboBox::from_id_salt(format!("fx-add-{salt}"))
            .selected_text("Add")
            .width(72.0)
            .show_ui(ui, |ui| {
                for (name, make) in crate::filter::Fx::catalog() {
                    if ui.selectable_label(false, *name).clicked() {
                        stack.items.push(make());
                    }
                }
            });
    });
    let mut remove = None;
    let mut bump = None;
    for (i, fx) in stack.items.iter_mut().enumerate() {
        ui.add_space(4.0);
        ui.group(|ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new(fx.name()).small().strong().color(accent()));
                ui.with_layout(Layout::right_to_left(eframe::egui::Align::Center), |ui| {
                    if ui.small_button("−").clicked() {
                        remove = Some(i);
                    }
                    if ui.small_button("↓").clicked() {
                        bump = Some((i, 1));
                    }
                    if ui.small_button("↑").clicked() {
                        bump = Some((i, -1));
                    }
                });
            });
            match fx {
                crate::filter::Fx::Blur { std } => {
                    ui.add(Slider::new(std, 0.0..=80.0).text("Blur"));
                }
                crate::filter::Fx::Shadow {
                    dx,
                    dy,
                    blur,
                    color,
                }
                | crate::filter::Fx::InnerShadow {
                    dx,
                    dy,
                    blur,
                    color,
                } => {
                    ui.add(Slider::new(dx, -80.0..=80.0).text("Offset X"));
                    ui.add(Slider::new(dy, -80.0..=80.0).text("Offset Y"));
                    ui.add(Slider::new(blur, 0.0..=80.0).text("Blur"));
                    ui.horizontal(|ui| {
                        let mut rgb = [color.r, color.g, color.b];
                        if ui.color_edit_button_srgb(&mut rgb).changed() {
                            color.r = rgb[0];
                            color.g = rgb[1];
                            color.b = rgb[2];
                        }
                    });
                }
                crate::filter::Fx::Offset { dx, dy } => {
                    ui.add(Slider::new(dx, -200.0..=200.0).text("Offset X"));
                    ui.add(Slider::new(dy, -200.0..=200.0).text("Offset Y"));
                }
                crate::filter::Fx::Morphology { erode, radius } => {
                    ui.checkbox(erode, "Erode");
                    ui.add(Slider::new(radius, 0.0..=40.0).text("Radius"));
                }
                crate::filter::Fx::Saturate { amount } => {
                    ui.add(Slider::new(amount, 0.0..=3.0).text("Amount"));
                }
                crate::filter::Fx::HueRotate { degrees } => {
                    ui.add(Slider::new(degrees, -180.0..=180.0).text("Angle"));
                }
                crate::filter::Fx::Brightness { amount } => {
                    ui.add(Slider::new(amount, 0.0..=3.0).text("Amount"));
                }
                crate::filter::Fx::Contrast { amount } => {
                    ui.add(Slider::new(amount, 0.0..=3.0).text("Amount"));
                }
                crate::filter::Fx::Invert { amount } => {
                    ui.add(Slider::new(amount, 0.0..=1.0).text("amount"));
                }
                crate::filter::Fx::ColorMatrix { values } => {
                    ui.add(Slider::new(&mut values[0], -2.0..=2.0).text("m00"));
                }
                crate::filter::Fx::Turbulence {
                    fractal,
                    base,
                    octaves,
                    seed,
                } => {
                    ui.checkbox(fractal, "fractalNoise");
                    ui.add(Slider::new(base, 0.001..=0.5).text("baseFrequency"));
                    ui.add(Slider::new(octaves, 1..=8).text("numOctaves"));
                    ui.add(Slider::new(seed, 0..=9999).text("seed"));
                }
                crate::filter::Fx::Displacement { scale, x_ch, y_ch } => {
                    ui.add(Slider::new(scale, 0.0..=120.0).text("scale"));
                    ui.add(Slider::new(x_ch, 0..=3).text("xChannel"));
                    ui.add(Slider::new(y_ch, 0..=3).text("yChannel"));
                }
            }
        });
    }
    if let Some(i) = remove {
        stack.items.remove(i);
    }
    if let Some((i, dir)) = bump {
        let j = i as i32 + dir;
        if j >= 0 && (j as usize) < stack.items.len() {
            stack.items.swap(i, j as usize);
        }
    }
}

fn fx_studio(ui: &mut Ui, studio: &mut Studio) {
    let shape_target = studio.primary().and_then(|(li, id)| {
        if id == crate::document::RASTER_ID {
            None
        } else {
            studio.doc.find_shape(li, id).map(|_| (li, id))
        }
    });
    if let Some((li, id)) = shape_target {
        ui.label(RichText::new("Object").small().color(fg_weak()));
        let mut stack = studio
            .doc
            .find_shape(li, id)
            .map(|s| s.filters.clone())
            .unwrap_or_default();
        fx_stack_editor(ui, &mut stack, &format!("obj-{id}"));
        studio.commit_shape_filters(li, id, stack);
        ui.add_space(6.0);
        ui.separator();
        ui.label(RichText::new("Layer").small().color(fg_weak()));
    }
    let Some(li) = studio.active_layer else {
        ui.label(RichText::new("Select a layer").small().color(fg_weak()));
        return;
    };
    if li >= studio.doc.layers.len() {
        return;
    }
    let mut stack = studio.doc.layers[li].filters.clone();
    fx_stack_editor(ui, &mut stack, "layer");
    if stack != studio.doc.layers[li].filters {
        studio.commit_filters(li, stack);
    }
}

fn trace_studio(ui: &mut Ui, studio: &mut Studio) {
    heading(ui, "Trace");
    ui.label(
        RichText::new("Raster to vector on the active pixel layer.")
            .small()
            .color(fg_weak()),
    );
    ui.add(Slider::new(&mut studio.trace_opts.colors, 1..=12).text("Colours"));
    if studio.trace_opts.colors <= 1 {
        ui.add(Slider::new(&mut studio.trace_opts.threshold, 0.05..=0.95).text("Threshold"));
    }
    ui.add(Slider::new(&mut studio.trace_opts.smoothness, 0.2..=8.0).text("Smoothness"));
    ui.add(Slider::new(&mut studio.trace_opts.min_area, 1.0..=64.0).text("Min area"));
    ui.checkbox(&mut studio.trace_opts.ignore_white, "Ignore white");
    ui.add_space(4.0);
    if ui.button("Trace to vector").clicked() {
        studio.trace_active_raster();
    }
}

fn brush_studio(ui: &mut Ui, studio: &mut Studio) {
    if matches!(studio.tool, Tool::Fill | Tool::Wand) {
        heading(ui, studio.tool.label());
        number_field(ui, "Tolerance", &mut studio.fill_tolerance, 0.0..=180.0, "");
        return;
    }
    if !matches!(
        studio.tool,
        Tool::Brush | Tool::Eraser | Tool::Clone | Tool::Heal | Tool::Smudge
    ) {
        return;
    }
    heading(
        ui,
        if studio.tool == Tool::Heal {
            "Healing brush"
        } else {
            "Brush"
        },
    );
    number_field(ui, "Size", &mut studio.brush.size, 1.0..=256.0, " px");
    if studio.tool == Tool::Smudge {
        let mut strength = studio.brush.flow * 100.0;
        number_field(ui, "Strength", &mut strength, 5.0..=100.0, "%");
        studio.brush.flow = strength / 100.0;
        return;
    }
    let mut hardness = studio.brush.hardness * 100.0;
    let mut opacity = studio.brush.opacity * 100.0;
    let mut flow = studio.brush.flow * 100.0;
    ui.columns(2, |columns| {
        number_field(&mut columns[0], "Edge", &mut hardness, 0.0..=100.0, "%");
        number_field(&mut columns[1], "Opacity", &mut opacity, 5.0..=100.0, "%");
    });
    studio.brush.hardness = hardness / 100.0;
    studio.brush.opacity = opacity / 100.0;
    number_field(ui, "Flow", &mut flow, 5.0..=100.0, "%");
    studio.brush.flow = flow / 100.0;
    super::masking::retouch_hint(ui, studio);
}

fn paint_color_studio(ui: &mut Ui, studio: &mut Studio) {
    heading(ui, "Color");
    let mut color = match studio.style.fill {
        Fill::Solid(color) if studio.tool == Tool::Fill => color.to_egui(),
        _ => studio.brush.color.to_egui(),
    };
    ui.horizontal(|ui| {
        if ui.color_edit_button_srgba(&mut color).changed() {
            studio.brush.color = Rgba::from_egui(color);
            studio.style.fill = Fill::Solid(studio.brush.color);
        }
        ui.label(
            RichText::new(Rgba::from_egui(color).hex())
                .small()
                .color(fg_weak()),
        );
    });
}

fn object_name(ui: &mut Ui, name: &str, width: f32, strong: bool) -> eframe::egui::Response {
    let (rect, response) = ui.allocate_exact_size(vec2(width, 28.0), eframe::egui::Sense::click());
    if response.hovered() {
        ui.painter()
            .rect_filled(rect, 4.0, crate::ui::theme::bg_widget_hover());
    }
    let text = RichText::new(name)
        .size(11.0)
        .color(if strong { fg() } else { fg_weak() });
    let galley = eframe::egui::WidgetText::from(text).into_galley(
        ui,
        Some(eframe::egui::TextWrapMode::Truncate),
        width - 4.0,
        eframe::egui::TextStyle::Small,
    );
    let position = eframe::egui::pos2(rect.left() + 2.0, rect.center().y - galley.size().y / 2.0);
    ui.painter().galley(position, galley, fg());
    response.widget_info(|| {
        eframe::egui::WidgetInfo::labeled(
            eframe::egui::WidgetType::SelectableLabel,
            ui.is_enabled(),
            name,
        )
    });
    response
}

fn object_icon(ui: &mut Ui, icon: &str, color: Color32) {
    let (rect, _) = ui.allocate_exact_size(vec2(20.0, 24.0), eframe::egui::Sense::hover());
    ui.painter().text(
        rect.center(),
        eframe::egui::Align2::CENTER_CENTER,
        icon,
        icons::font(15.0),
        color,
    );
}

fn layers_studio(ui: &mut Ui, studio: &mut Studio) {
    // Reveal a newly selected object once; a manual collapse stays collapsed.
    let primary = studio.primary();
    let reveal_id = ui.make_persistent_id("reveal-selection");
    let previous = ui
        .data(|d| d.get_temp::<Option<(usize, u64)>>(reveal_id))
        .flatten();
    if previous != primary {
        if let Some((li, _)) = primary
            && let Some(layer) = studio.doc.layers.get(li)
        {
            studio.layer_expanded.insert(layer.id);
        }
        ui.data_mut(|d| d.insert_temp(reveal_id, primary));
    }
    ui.horizontal(|ui| {
        heading(ui, "Layers");
        ui.label(
            RichText::new(studio.doc.layers.len().to_string())
                .size(10.0)
                .color(fg_weak()),
        );
        ui.with_layout(Layout::right_to_left(eframe::egui::Align::Center), |ui| {
            ui.menu_button("···", |ui| {
                if ui.button("New vector layer").clicked() {
                    studio.add_layer(false);
                    ui.close();
                }
                if ui.button("New pixel layer").clicked() {
                    studio.add_layer(true);
                    ui.close();
                }
                ui.separator();
                if ui.button("Delete active layer").clicked() {
                    studio.delete_layer();
                    ui.close();
                }
            });
            if icons::tiny_icon(ui, ph::PLUS, "New vector layer", false) {
                studio.add_layer(false);
            }
        });
    });

    let mut activate = None;
    let mut mask_action = None;
    let mut vis = None;
    let mut lock = None;
    let mut up = None;
    let mut down = None;
    let mut start_rename = None;
    let mut toggle_expand = None;
    let mut pick_shape: Option<(usize, u64)> = None;
    let mut vis_shape: Option<(usize, u64)> = None;
    let mut lock_shape: Option<(usize, u64)> = None;
    let mut start_shape_rename: Option<(usize, u64)> = None;
    let mut shape_up: Option<(usize, usize)> = None;
    let mut shape_down: Option<(usize, usize)> = None;
    let n = studio.doc.layers.len();
    if let Some(i) = studio.active_layer.filter(|&i| i < n) {
        ui.horizontal(|ui| {
            let mut blend = studio.doc.layers[i].blend;
            ComboBox::from_id_salt("active-layer-blend")
                .selected_text(blend.name())
                .width((ui.available_width() - 76.0).max(100.0))
                .show_ui(ui, |ui| {
                    for value in Blend::ALL {
                        ui.selectable_value(&mut blend, value, value.name());
                    }
                });
            let mut opacity = studio.doc.layers[i].opacity * 100.0;
            ui.add(
                eframe::egui::DragValue::new(&mut opacity)
                    .range(0.0..=100.0)
                    .suffix("%")
                    .speed(0.5)
                    .max_decimals(0),
            )
            .on_hover_text("Layer opacity");
            let layer = &studio.doc.layers[i];
            if blend != layer.blend || (opacity / 100.0 - layer.opacity).abs() > 0.0001 {
                studio.commit(crate::document::Cmd::SetLayerMeta {
                    index: i,
                    name: layer.name.clone(),
                    visible: layer.visible,
                    locked: layer.locked,
                    opacity: opacity / 100.0,
                    blend,
                    before: (
                        layer.name.clone(),
                        layer.visible,
                        layer.locked,
                        layer.opacity,
                        layer.blend,
                    ),
                });
            }
        });
        ui.add_space(8.0);
    }
    for i in (0..n).rev() {
        ui.push_id(studio.doc.layers[i].id, |ui| {
            let active = studio.active_layer == Some(i);
            Frame::new()
                .fill(if active {
                    bg_widget()
                } else {
                    Color32::TRANSPARENT
                })
                .corner_radius(5.0)
                .inner_margin(Margin::symmetric(3, 2))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 2.0;
                        let layer = &studio.doc.layers[i];
                        let expanded = studio.layer_expanded.contains(&layer.id);
                        let has_children =
                            layer.kind.shapes().is_some_and(|shapes| !shapes.is_empty())
                                || layer.kind.is_placed_raster();
                        if has_children {
                            if icons::tiny_icon(
                                ui,
                                if expanded {
                                    ph::CARET_DOWN
                                } else {
                                    ph::CARET_RIGHT
                                },
                                "Show objects",
                                false,
                            ) {
                                toggle_expand = Some(layer.id);
                            }
                        } else {
                            ui.add_space(22.0);
                        }
                        object_icon(
                            ui,
                            if layer.kind.shapes().is_some() {
                                ph::STACK
                            } else {
                                ph::IMAGES
                            },
                            fg_weak(),
                        );
                        let name_width = (ui.available_width()
                            - if layer.mask.is_some() { 74.0 } else { 50.0 })
                        .max(42.0);
                        if studio.layer_rename.as_ref().map(|(index, _)| *index) == Some(i) {
                            if let Some((_, buffer)) = studio.layer_rename.as_mut() {
                                let response = ui.add(
                                    eframe::egui::TextEdit::singleline(buffer)
                                        .desired_width(name_width)
                                        .font(eframe::egui::TextStyle::Small),
                                );
                                if response.lost_focus() {
                                    start_rename = Some(usize::MAX);
                                }
                            }
                        } else {
                            let response = object_name(ui, &layer.name, name_width, layer.visible);
                            if response.clicked() {
                                activate = Some(i);
                            }
                            if response.double_clicked() {
                                start_rename = Some(i);
                            }
                            response
                                .on_hover_text(format!("{} · {}", layer.name, layer.kind.tag()))
                                .context_menu(|ui| {
                                    if ui.button("Rename").clicked() {
                                        start_rename = Some(i);
                                        ui.close();
                                    }
                                    if ui
                                        .add_enabled(
                                            i + 1 < n,
                                            eframe::egui::Button::new("Move up"),
                                        )
                                        .clicked()
                                    {
                                        up = Some(i);
                                        ui.close();
                                    }
                                    if ui
                                        .add_enabled(i > 0, eframe::egui::Button::new("Move down"))
                                        .clicked()
                                    {
                                        down = Some(i);
                                        ui.close();
                                    }
                                    ui.separator();
                                    ui.menu_button("Layer mask", |ui| {
                                        if let Some(action) = super::masking::menu(ui, studio, i) {
                                            mask_action = Some((i, action));
                                        }
                                    });
                                });
                        }
                        if layer.mask.is_some()
                            && icons::tiny_icon(
                                ui,
                                ph::SELECTION,
                                "Paint layer mask",
                                active && studio.paint_mask,
                            )
                        {
                            mask_action = Some((i, super::masking::Action::Edit(true)));
                        }
                        if icons::tiny_icon(
                            ui,
                            if layer.visible {
                                ph::EYE
                            } else {
                                ph::EYE_SLASH
                            },
                            "Layer visibility",
                            !layer.visible,
                        ) {
                            vis = Some(i);
                        }
                        if icons::tiny_icon(
                            ui,
                            if layer.locked {
                                ph::LOCK
                            } else {
                                ph::LOCK_OPEN
                            },
                            "Lock layer",
                            layer.locked,
                        ) {
                            lock = Some(i);
                        }
                    });
                });
            if studio.layer_expanded.contains(&studio.doc.layers[i].id) {
                if studio.doc.layers[i].kind.is_placed_raster() {
                    ui.horizontal(|ui| {
                        ui.add_space(28.0);
                        if ui
                            .selectable_label(
                                studio.selection.contains(&(i, crate::document::RASTER_ID)),
                                &studio.doc.layers[i].name,
                            )
                            .clicked()
                        {
                            pick_shape = Some((i, crate::document::RASTER_ID));
                        }
                    });
                }
                if let Some(shapes) = studio.doc.layers[i].kind.shapes() {
                    for (index, shape) in shapes.iter().enumerate().rev() {
                        ui.push_id(shape.id, |ui| {
                            Frame::new()
                                .fill(if studio.selection.contains(&(i, shape.id)) {
                                    accent_soft()
                                } else {
                                    Color32::TRANSPARENT
                                })
                                .corner_radius(5.0)
                                .inner_margin(Margin::symmetric(3, 1))
                                .show(ui, |ui| {
                                    ui.horizontal(|ui| {
                                        ui.spacing_mut().item_spacing.x = 2.0;
                                        ui.add_space(24.0);
                                        object_icon(
                                            ui,
                                            geometry_icon(&shape.geom),
                                            if studio.selection.contains(&(i, shape.id)) {
                                                accent()
                                            } else {
                                                fg_weak()
                                            },
                                        );
                                        let name_width = (ui.available_width() - 50.0).max(42.0);
                                        if studio
                                            .shape_rename
                                            .as_ref()
                                            .map(|(layer, id, _)| (*layer, *id))
                                            == Some((i, shape.id))
                                        {
                                            if let Some((_, _, buffer)) =
                                                studio.shape_rename.as_mut()
                                            {
                                                let response = ui.add(
                                                    eframe::egui::TextEdit::singleline(buffer)
                                                        .desired_width(name_width)
                                                        .font(eframe::egui::TextStyle::Small),
                                                );
                                                if response.lost_focus() {
                                                    start_shape_rename = Some((usize::MAX, 0));
                                                }
                                            }
                                        } else {
                                            let response = object_name(
                                                ui,
                                                &shape.name,
                                                name_width,
                                                studio.selection.contains(&(i, shape.id))
                                                    && shape.visible,
                                            );
                                            if response.clicked() {
                                                pick_shape = Some((i, shape.id));
                                            }
                                            if response.double_clicked() {
                                                start_shape_rename = Some((i, shape.id));
                                            }
                                            response.on_hover_text(&shape.name).context_menu(
                                                |ui| {
                                                    if ui.button("Rename").clicked() {
                                                        start_shape_rename = Some((i, shape.id));
                                                        ui.close();
                                                    }
                                                    if ui
                                                        .add_enabled(
                                                            index + 1 < shapes.len(),
                                                            eframe::egui::Button::new("Move up"),
                                                        )
                                                        .clicked()
                                                    {
                                                        shape_up = Some((i, index));
                                                        ui.close();
                                                    }
                                                    if ui
                                                        .add_enabled(
                                                            index > 0,
                                                            eframe::egui::Button::new("Move down"),
                                                        )
                                                        .clicked()
                                                    {
                                                        shape_down = Some((i, index));
                                                        ui.close();
                                                    }
                                                },
                                            );
                                        }
                                        if icons::tiny_icon(
                                            ui,
                                            if shape.visible {
                                                ph::EYE
                                            } else {
                                                ph::EYE_SLASH
                                            },
                                            "Object visibility",
                                            !shape.visible,
                                        ) {
                                            vis_shape = Some((i, shape.id));
                                        }
                                        if icons::tiny_icon(
                                            ui,
                                            if shape.locked {
                                                ph::LOCK
                                            } else {
                                                ph::LOCK_OPEN
                                            },
                                            "Lock object",
                                            shape.locked,
                                        ) {
                                            lock_shape = Some((i, shape.id));
                                        }
                                    });
                                });
                        });
                    }
                }
            }
        });
    }
    if let Some(i) = start_rename {
        if i == usize::MAX {
            if let Some((idx, name)) = studio.layer_rename.take()
                && let Some(l) = studio.doc.layers.get(idx)
            {
                let trimmed = name.trim().to_string();
                if !trimmed.is_empty() && trimmed != l.name {
                    studio.commit(crate::document::Cmd::SetLayerMeta {
                        index: idx,
                        name: trimmed,
                        visible: l.visible,
                        locked: l.locked,
                        opacity: l.opacity,
                        blend: l.blend,
                        before: (l.name.clone(), l.visible, l.locked, l.opacity, l.blend),
                    });
                }
            }
        } else {
            studio.layer_rename = Some((i, studio.doc.layers[i].name.clone()));
        }
    }
    if let Some(i) = activate {
        if studio.active_layer != Some(i) {
            studio.paint_mask = false;
        }
        studio.active_layer = Some(i);
    }
    if let Some(i) = vis {
        let l = &studio.doc.layers[i];
        studio.commit(crate::document::Cmd::SetLayerMeta {
            index: i,
            name: l.name.clone(),
            visible: !l.visible,
            locked: l.locked,
            opacity: l.opacity,
            blend: l.blend,
            before: (l.name.clone(), l.visible, l.locked, l.opacity, l.blend),
        });
    }
    if let Some(i) = lock {
        let l = &studio.doc.layers[i];
        studio.commit(crate::document::Cmd::SetLayerMeta {
            index: i,
            name: l.name.clone(),
            visible: l.visible,
            locked: !l.locked,
            opacity: l.opacity,
            blend: l.blend,
            before: (l.name.clone(), l.visible, l.locked, l.opacity, l.blend),
        });
    }
    if let Some(i) = up
        && i + 1 < n
    {
        studio.commit(crate::document::Cmd::ReorderLayer { from: i, to: i + 1 });
        studio.active_layer = Some(i + 1);
    }
    if let Some(i) = down
        && i > 0
    {
        studio.commit(crate::document::Cmd::ReorderLayer { from: i, to: i - 1 });
        studio.active_layer = Some(i - 1);
    }
    if let Some(id) = toggle_expand
        && !studio.layer_expanded.remove(&id)
    {
        studio.layer_expanded.insert(id);
    }
    if let Some((li, id)) = pick_shape {
        if studio.active_layer != Some(li) {
            studio.paint_mask = false;
        }
        studio.selection = vec![(li, id)];
        studio.active_layer = Some(li);
        studio.artboard_sel.clear();
    }
    if let Some((li, id)) = vis_shape
        && let Some(s) = studio.doc.find_shape(li, id)
    {
        studio.commit(crate::document::Cmd::SetShapeMeta {
            layer: li,
            id,
            name: s.name.clone(),
            visible: !s.visible,
            locked: s.locked,
            before: (s.name.clone(), s.visible, s.locked),
        });
    }
    if let Some((li, id)) = lock_shape
        && let Some(s) = studio.doc.find_shape(li, id)
    {
        studio.commit(crate::document::Cmd::SetShapeMeta {
            layer: li,
            id,
            name: s.name.clone(),
            visible: s.visible,
            locked: !s.locked,
            before: (s.name.clone(), s.visible, s.locked),
        });
    }
    if let Some((li, id)) = start_shape_rename {
        if li == usize::MAX {
            if let Some((l, sid, name)) = studio.shape_rename.take()
                && let Some(s) = studio.doc.find_shape(l, sid)
            {
                let trimmed = name.trim().to_string();
                if !trimmed.is_empty() && trimmed != s.name {
                    studio.commit(crate::document::Cmd::SetShapeMeta {
                        layer: l,
                        id: sid,
                        name: trimmed,
                        visible: s.visible,
                        locked: s.locked,
                        before: (s.name.clone(), s.visible, s.locked),
                    });
                }
            }
        } else if let Some(s) = studio.doc.find_shape(li, id) {
            studio.shape_rename = Some((li, id, s.name.clone()));
        }
    }
    if let Some((li, from)) = shape_up
        && let Some(shapes) = studio.doc.layers.get(li).and_then(|l| l.kind.shapes())
        && from + 1 < shapes.len()
    {
        studio.commit(crate::document::Cmd::ReorderShape {
            layer: li,
            from,
            to: from + 1,
        });
    }
    if let Some((li, from)) = shape_down
        && from > 0
    {
        studio.commit(crate::document::Cmd::ReorderShape {
            layer: li,
            from,
            to: from - 1,
        });
    }
    if let Some((index, action)) = mask_action {
        action.run(studio, index);
    }
}

#[test]
fn inspector_keeps_its_width_across_frames_and_personas() {
    for scene in ["design", "type", "pixel", "motion", "masking", "healing"] {
        let ctx = eframe::egui::Context::default();
        crate::ui::theme::apply(&ctx);
        let mut studio = Studio::new();
        crate::shots::apply(&mut studio, scene).unwrap();
        for _ in 0..8 {
            let mut width = 0.0;
            let mut output = ctx.run_ui(
                eframe::egui::RawInput {
                    screen_rect: Some(eframe::egui::Rect::from_min_size(
                        eframe::egui::Pos2::ZERO,
                        vec2(960.0, 640.0),
                    )),
                    ..Default::default()
                },
                |ui| {
                    right_panel(ui, &mut studio);
                    width = 960.0 - ui.available_width();
                },
            );
            output.textures_delta.clear();
            assert!(width <= 304.0, "{scene} inspector grew to {width}px");
        }
    }
}
