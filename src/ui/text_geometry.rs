//! Editable path brackets and type geometry controls.
use crate::{
    app::Studio,
    geom::{Geom, Pt},
    text_geometry::{self, PathAlign},
};
use eframe::egui::{self, Color32, Rect, Sense, Stroke, Ui};
pub fn path_inspector(ui: &mut Ui, studio: &mut Studio) {
    let Some(run) = studio.selected_type() else {
        return;
    };
    let Some(mut on) = run.on_path.clone() else {
        return;
    };
    let before = on.clone();
    egui::CollapsingHeader::new("Type on path")
        .default_open(true)
        .show(ui, |ui| {
            let length = on.cache.as_ref().map(|p| p.length).unwrap_or(0.);
            ui.horizontal(|ui| {
                ui.label("Start %");
                ui.add(
                    egui::DragValue::new(&mut on.start)
                        .speed(0.001)
                        .custom_formatter(|v, _| format!("{:.1}", v * 100.))
                        .custom_parser(|s| s.parse::<f64>().ok().map(|v| v / 100.)),
                );
            });
            ui.horizontal(|ui| {
                ui.label("End %");
                ui.add(
                    egui::DragValue::new(&mut on.end)
                        .speed(0.001)
                        .custom_formatter(|v, _| format!("{:.1}", v * 100.))
                        .custom_parser(|s| s.parse::<f64>().ok().map(|v| v / 100.)),
                );
            });
            if length > 0. {
                let mut start = on.start * length;
                let mut end = on.end * length;
                ui.horizontal(|ui| {
                    ui.label("Start px");
                    if ui
                        .add(egui::DragValue::new(&mut start).speed(0.5))
                        .changed()
                    {
                        on.start = start / length;
                    }
                });
                ui.horizontal(|ui| {
                    ui.label("End px");
                    if ui.add(egui::DragValue::new(&mut end).speed(0.5)).changed() {
                        on.end = end / length;
                    }
                });
            }
            ui.checkbox(&mut on.flip, "Flip side and direction");
            ui.horizontal(|ui| {
                ui.label("Baseline shift");
                ui.add(
                    egui::DragValue::new(&mut on.baseline_shift)
                        .speed(0.25)
                        .suffix(" px"),
                );
            });
            ui.horizontal(|ui| {
                ui.label("Curve spacing");
                ui.add(
                    egui::DragValue::new(&mut on.spacing)
                        .speed(0.1)
                        .suffix(" px"),
                );
            });
            ui.label("Align to path");
            egui::ComboBox::from_id_salt("path-alignment")
                .selected_text(format!("{:?}", on.align))
                .show_ui(ui, |ui| {
                    for value in [
                        PathAlign::Baseline,
                        PathAlign::Center,
                        PathAlign::Top,
                        PathAlign::Bottom,
                    ] {
                        ui.selectable_value(&mut on.align, value, format!("{value:?}"));
                    }
                });
        });
    if before != on {
        studio.patch_type(|t| {
            t.on_path = Some(on.clone());
            t.layout = None;
        });
    }
}
pub fn menu(ui: &mut Ui, studio: &mut Studio) {
    frame_menu(ui, studio);
    if ui
        .add_enabled(
            studio.can_attach_text_path(),
            egui::Button::new("Attach text to path"),
        )
        .clicked()
    {
        studio.attach_text_path();
        ui.close();
    }
    let attached = studio.selected_type().is_some_and(|t| t.on_path.is_some());
    if ui
        .add_enabled(attached, egui::Button::new("Release text from path"))
        .clicked()
    {
        studio.release_text_path();
        ui.close();
    }
}
pub fn brackets(ui: &mut Ui, rect: Rect, studio: &mut Studio) -> bool {
    if studio.tool == crate::tools::Tool::Node {
        return false;
    }
    let painter = ui
        .ctx()
        .layer_painter(egui::LayerId::new(
            egui::Order::Foreground,
            egui::Id::new("text-geometry-brackets"),
        ))
        .with_clip_rect(rect);
    let mut handled = false;
    let selection = studio.selection.clone();
    for (layer, id) in selection {
        let Some(shape) = studio.doc.find_shape(layer, id) else {
            continue;
        };
        let Geom::Text(current) = shape.geom.clone() else {
            continue;
        };
        let Some(on) = current.on_path.as_ref() else {
            continue;
        };
        let Some(path) = on.cache.as_ref() else {
            continue;
        };
        let Some((_, span)) = text_geometry::path_interval(on) else {
            continue;
        };
        let view = studio.view;
        for (kind, distance) in [(0, 0.), (1, span), (2, span * 0.5)] {
            let Some((point, tangent)) = text_geometry::path_position(&current, distance) else {
                continue;
            };
            let to_screen = |p: Pt| {
                let p = view.to_screen(p);
                egui::pos2(rect.min.x + p.x, rect.min.y + p.y)
            };
            let center = to_screen(point)
                + egui::vec2(tangent.x, tangent.y)
                    * match kind {
                        0 => -6.,
                        1 => 6.,
                        _ => 0.,
                    };
            let color = if kind == 1 && current.layout.as_ref().is_some_and(|l| l.overflow) {
                Color32::from_rgb(245, 65, 75)
            } else {
                crate::ui::theme::select()
            };
            painter.line_segment(
                [
                    to_screen(point - tangent.perp() * 9. / view.scale),
                    to_screen(point + tangent.perp() * 9. / view.scale),
                ],
                Stroke::new(2., color),
            );
            painter.circle_filled(center, 3., color);
            if kind == 1 && current.layout.as_ref().is_some_and(|l| l.overflow) {
                painter.text(
                    center + egui::vec2(9., -8.),
                    egui::Align2::CENTER_CENTER,
                    "+",
                    egui::FontId::proportional(15.),
                    color,
                );
            }
            let response = ui.interact(
                Rect::from_center_size(center, egui::vec2(16., 22.)),
                egui::Id::new(("type-path-bracket", id, kind)),
                Sense::drag(),
            );
            let drag_id = egui::Id::new(("type-path-before", id));
            if response.drag_started() {
                ui.ctx()
                    .data_mut(|d| d.insert_temp(drag_id, current.clone()));
            }
            if response.dragged()
                && let Some(pos) = response.interact_pointer_pos()
            {
                let world =
                    view.pointer_to_world(Pt::new(rect.min.x, rect.min.y), Pt::new(pos.x, pos.y));
                let fraction = path.nearest_distance(world) / path.length.max(0.001);
                let mut next = on.clone();
                match kind {
                    0 => {
                        let length = on.end - on.start;
                        next.start = fraction;
                        if path.closed {
                            next.end = fraction + length;
                        }
                    }
                    1 => {
                        next.end = if path.closed && fraction < on.start {
                            fraction + 1.
                        } else {
                            fraction
                        }
                    }
                    _ => {
                        let (p, t) = path.point_and_tangent_at(path.nearest_distance(world));
                        next.flip = (world - p).dot(t.perp()) > 0.;
                    }
                }
                if let Some(s) = studio.doc.find_shape_mut(layer, id)
                    && let Geom::Text(t) = &mut s.geom
                {
                    t.on_path = Some(next);
                    t.layout = None;
                }
                studio.mark();
                handled = true;
            }
            if response.drag_stopped()
                && let Some(before) = ui
                    .ctx()
                    .data_mut(|d| d.remove_temp::<crate::geom::TypeRun>(drag_id))
                && let Some(s) = studio.doc.find_shape(layer, id)
            {
                let after = s.geom.clone();
                let rotation = s.rotation;
                studio.history.push(crate::document::Cmd::SetGeom {
                    layer,
                    id,
                    before: Geom::Text(before),
                    after,
                    rot_before: rotation,
                    rot_after: rotation,
                });
                studio.dirty = true;
                handled = true;
            }
            if response.hovered() {
                handled = true;
            }
        }
    }
    handled
}

pub fn frame_inspector(ui: &mut Ui, studio: &mut Studio) {
    use text_geometry::{Overflow, VAlign};
    let Some(run) = studio.selected_type() else {
        return;
    };
    let Some(mut frame) = run.frame.clone() else {
        return;
    };
    let before = frame.clone();
    let linked = run.thread.as_ref().is_some_and(|thread| thread.next.is_some());
    egui::CollapsingHeader::new("Text frame")
        .default_open(true)
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label("Width");
                ui.add(
                    egui::DragValue::new(&mut frame.size.x)
                        .range(1.0..=100000.)
                        .speed(1.),
                );
                ui.label("Height");
                ui.add(
                    egui::DragValue::new(&mut frame.size.y)
                        .range(1.0..=100000.)
                        .speed(1.),
                );
            });
            for (index, label) in ["Inset top", "Inset right", "Inset bottom", "Inset left"]
                .into_iter()
                .enumerate()
            {
                ui.horizontal(|ui| {
                    ui.label(label);
                    ui.add(
                        egui::DragValue::new(&mut frame.inset[index])
                            .range(0.0..=10000.)
                            .speed(0.5),
                    );
                });
            }
            ui.horizontal(|ui| {
                for (value, label) in [
                    (VAlign::Top, "Top"),
                    (VAlign::Center, "Center"),
                    (VAlign::Bottom, "Bottom"),
                    (VAlign::Justify, "Justify"),
                ] {
                    ui.selectable_value(&mut frame.valign, value, label);
                }
            });
            ui.add_enabled(
                frame.contour.is_none(),
                egui::Checkbox::new(&mut frame.auto_height, "Auto height"),
            );
            egui::ComboBox::from_id_salt("frame-overflow")
                .selected_text(format!("{:?}", frame.overflow))
                .show_ui(ui, |ui| {
                    for mode in [
                        Overflow::Visible,
                        Overflow::Clip,
                        Overflow::Ellipsis,
                        Overflow::Flow,
                    ] {
                        ui.add_enabled_ui(!linked || mode == Overflow::Flow, |ui| {
                            ui.selectable_value(&mut frame.overflow, mode, format!("{mode:?}"));
                        });
                    }
                });
            if frame.overflow == Overflow::Flow {
                ui.label("Last frame");
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut frame.terminal_overflow, Overflow::Clip, "Clip");
                    ui.selectable_value(
                        &mut frame.terminal_overflow,
                        Overflow::Ellipsis,
                        "Ellipsis",
                    );
                });
            }
            let mut limited = frame.max_lines.is_some();
            if ui.checkbox(&mut limited, "Limit lines").changed() {
                frame.max_lines = limited.then_some(1);
            }
            if let Some(max) = &mut frame.max_lines {
                ui.add(
                    egui::DragValue::new(max)
                        .range(1..=100000)
                        .prefix("Max lines "),
                );
            }
            if run.layout.as_ref().is_some_and(|l| l.overflow) {
                ui.colored_label(
                    Color32::from_rgb(235, 65, 75),
                    "+ Text overflows this frame",
                );
            }
        });
    if frame != before {
        studio.patch_frame_geometry(|t| {
            t.frame = Some(frame.clone());
            t.layout = None;
        });
    }
}

pub fn wrap_inspector(ui: &mut Ui, studio: &mut Studio) {
    use text_geometry::{TextWrap, WrapMode};
    let target = studio
        .selected_layer
        .and_then(|id| studio.doc.layers.iter().position(|l| l.id == id))
        .map(|layer| (layer, crate::document::RASTER_ID))
        .or_else(|| studio.selection.first().copied());
    let Some((layer, id)) = target else { return };
    let Some(mut wrap) = (if id == crate::document::RASTER_ID {
        studio.doc.layers.get(layer).map(|l| l.text_wrap.clone())
    } else {
        studio
            .doc
            .find_shape(layer, id)
            .map(|s| s.text_wrap.clone())
    }) else {
        return;
    };
    let before = wrap.clone();
    egui::CollapsingHeader::new("Text wrap")
        .default_open(false)
        .show(ui, |ui| {
            egui::ComboBox::from_id_salt("text-wrap-mode")
                .selected_text(format!("{:?}", wrap.mode))
                .show_ui(ui, |ui| {
                    for (mode, label) in [
                        (WrapMode::None, "None"),
                        (WrapMode::BoundingBox, "Bounding box"),
                        (WrapMode::ObjectShape, "Object shape"),
                        (WrapMode::JumpObject, "Jump object"),
                    ] {
                        ui.selectable_value(&mut wrap.mode, mode, label);
                    }
                });
            if wrap.mode == WrapMode::ObjectShape {
                let mut offset = wrap.offset[0];
                ui.horizontal(|ui| {
                    ui.label("Offset");
                    if ui
                        .add(
                            egui::DragValue::new(&mut offset)
                                .range(0.0..=10000.)
                                .speed(0.5),
                        )
                        .changed()
                    {
                        wrap.offset = [offset; 4];
                    }
                });
            } else {
                for (i, label) in ["Top", "Right", "Bottom", "Left"].into_iter().enumerate() {
                    ui.horizontal(|ui| {
                        ui.label(label);
                        ui.add(
                            egui::DragValue::new(&mut wrap.offset[i])
                                .range(0.0..=10000.)
                                .speed(0.5),
                        );
                    });
                }
            }
            ui.checkbox(&mut wrap.invert, "Invert: flow inside object");
            ui.checkbox(&mut wrap.ignore_locked, "Ignore on locked layers");
            let mut above = studio.doc.text_wrap_above_only;
            if ui
                .checkbox(&mut above, "Wrap affects text above only")
                .changed()
            {
                studio.commit(crate::document::Cmd::SetTextWrapAbove {
                    before: studio.doc.text_wrap_above_only,
                    after: above,
                });
            }
        });
    if wrap != before {
        studio.commit(crate::document::Cmd::SetTextWrap {
            layer,
            id: if id == crate::document::RASTER_ID {
                None
            } else {
                Some(id)
            },
            before,
            after: wrap,
        });
    }
    let _ = TextWrap::default;
}

pub fn frame_menu(ui: &mut Ui, studio: &mut Studio) {
    if ui
        .add_enabled(
            studio.can_text_inside_shape(),
            egui::Button::new("Text inside shape"),
        )
        .clicked()
    {
        studio.text_inside_shape();
        ui.close();
    }
    let live = studio.selected_type();
    if ui
        .add_enabled(
            live.as_ref().is_some_and(|t| t.frame.is_none()),
            egui::Button::new("Convert to area text"),
        )
        .clicked()
    {
        studio.convert_text_frame(true);
        ui.close();
    }
    if ui
        .add_enabled(
            live.as_ref().is_some_and(|t| t.frame.is_some()),
            egui::Button::new("Convert to point text"),
        )
        .clicked()
    {
        studio.convert_text_frame(false);
        ui.close();
    }
    if let Some(&(layer, id)) = studio.selection.first()
        && live
            .as_ref()
            .is_some_and(|t| t.thread.as_ref().is_some_and(|t| t.next.is_some()))
        && ui.button("Break outgoing text thread").clicked()
    {
        studio.break_text_thread((layer, id));
        ui.close();
    }
}

pub fn show_threads(ctx: &egui::Context) -> bool {
    ctx.data(|d| {
        d.get_temp::<bool>(egui::Id::new("show-text-threads"))
            .unwrap_or(false)
    })
}
pub fn thread_view_menu(ui: &mut Ui) {
    let mut show = show_threads(ui.ctx());
    if ui.checkbox(&mut show, "Show text threads").changed() {
        ui.ctx()
            .data_mut(|d| d.insert_temp(egui::Id::new("show-text-threads"), show));
    }
}

pub fn area_input(studio: &mut Studio, response: &egui::Response, rect: Rect) -> bool {
    let ctx = &response.ctx;
    let loaded_id = egui::Id::new("loaded-text-port");
    let loaded = ctx.data(|d| d.get_temp::<(usize, u64)>(loaded_id));
    if loaded.is_some() && ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        ctx.data_mut(|d| d.remove::<(usize, u64)>(loaded_id));
        return true;
    }
    if let Some(source) = loaded
        && response.clicked()
        && let Some(pos) = response.interact_pointer_pos()
    {
        let point = studio
            .view
            .pointer_to_world(Pt::new(rect.min.x, rect.min.y), Pt::new(pos.x, pos.y));
        let target = studio
            .doc
            .layers
            .iter()
            .enumerate()
            .rev()
            .find_map(|(l, layer)| {
                layer.kind.shapes()?.iter().rev().find_map(|s| {
                    if let Geom::Text(t) = &s.geom
                        && text_geometry::frame_bounds(t).is_some_and(|b| b.contains(point))
                    {
                        Some((l, s.id))
                    } else {
                        None
                    }
                })
            });
        let target = if let Some(target) = target {
            target
        } else {
            let Some(target) = studio.empty_thread_frame(source, point) else {
                return true;
            };
            target
        };
        ctx.data_mut(|d| d.remove::<(usize, u64)>(loaded_id));
        if let Err(error) = studio.thread_text_frames(source, target) {
            studio.status = error;
        }
        return true;
    }
    if studio.tool != crate::tools::Tool::Text || studio.type_edit.is_some() {
        return false;
    }
    let drag_id = egui::Id::new("area-text-drag-start");
    if response.drag_started()
        && let Some(pos) = ctx.input(|i| i.pointer.press_origin())
    {
        let start = studio
            .view
            .pointer_to_world(Pt::new(rect.min.x, rect.min.y), Pt::new(pos.x, pos.y));
        let on_text = studio
            .doc
            .layers
            .iter()
            .filter_map(|l| l.kind.shapes())
            .flatten()
            .any(|s| matches!(s.geom, Geom::Text(_)) && s.world_bbox().contains(start));
        if !on_text {
            ctx.data_mut(|d| d.insert_temp(drag_id, start));
        }
    }
    let Some(start) = ctx.data(|d| d.get_temp::<Pt>(drag_id)) else {
        return false;
    };
    let Some(pos) = response.interact_pointer_pos() else {
        return true;
    };
    let mut end = studio
        .view
        .pointer_to_world(Pt::new(rect.min.x, rect.min.y), Pt::new(pos.x, pos.y));
    let mods = ctx.input(|i| i.modifiers);
    if mods.shift {
        let delta = end - start;
        let side = delta.x.abs().max(delta.y.abs());
        end = start + Pt::new(delta.x.signum() * side, delta.y.signum() * side);
    }
    let beginning = if mods.alt {
        start - (end - start)
    } else {
        start
    };
    let bounds = crate::geom::Bounds {
        min: beginning.min(end),
        max: beginning.max(end),
    };
    let screen = |p: Pt| {
        let p = studio.view.to_screen(p);
        rect.min + egui::vec2(p.x, p.y)
    };
    ctx.layer_painter(egui::LayerId::new(
        egui::Order::Foreground,
        egui::Id::new("area-text-preview"),
    ))
    .rect_stroke(
        Rect::from_min_max(screen(bounds.min), screen(bounds.max)),
        0.,
        Stroke::new(1., crate::ui::theme::select()),
        egui::StrokeKind::Inside,
    );
    if response.drag_stopped() {
        ctx.data_mut(|d| d.remove::<Pt>(drag_id));
        if bounds.width() > 2. && bounds.height() > 2. {
            studio.place_area_text(bounds);
        }
    }
    true
}

pub fn frame_ports(ui: &mut Ui, rect: Rect, studio: &mut Studio) -> bool {
    let mut handled = false;
    let view = studio.view;
    let to_screen = |p: Pt| {
        let p = view.to_screen(p);
        rect.min + egui::vec2(p.x, p.y)
    };
    let painter = ui
        .ctx()
        .layer_painter(egui::LayerId::new(
            egui::Order::Foreground,
            egui::Id::new("text-frame-ports"),
        ))
        .with_clip_rect(rect);
    let all = show_threads(ui.ctx());
    let point_widgets: Vec<_> = studio
        .selection
        .iter()
        .filter_map(|&(layer, id)| {
            studio
                .doc
                .find_shape(layer, id)
                .and_then(|shape| match &shape.geom {
                    Geom::Text(run) if run.frame.is_none() && run.on_path.is_none() => {
                        Some((layer, id, shape.world_bbox()))
                    }
                    _ => None,
                })
        })
        .collect();
    for (layer, id, bounds) in point_widgets {
        let center = to_screen(Pt::new(bounds.max.x, bounds.center().y));
        let response = ui.interact(
            Rect::from_center_size(center, egui::vec2(10., 14.)),
            egui::Id::new(("point-side-widget", id)),
            Sense::click(),
        );
        painter.rect_filled(response.rect, 1., crate::ui::theme::select());
        if response.double_clicked() {
            studio.selection = vec![(layer, id)];
            studio.convert_text_frame(true);
            handled = true;
        }
    }
    let frames: Vec<_> = studio
        .doc
        .layers
        .iter()
        .enumerate()
        .flat_map(|(li, l)| {
            l.kind.shapes().unwrap_or(&[]).iter().filter_map(move |s| {
                if let Geom::Text(t) = &s.geom
                    && t.frame.is_some()
                {
                    Some((li, s.id, t.clone()))
                } else {
                    None
                }
            })
        })
        .collect();
    for (layer, id, run) in &frames {
        if !all
            && !studio.selection.contains(&(*layer, *id))
            && studio
                .type_edit
                .as_ref()
                .is_none_or(|e| run.thread.as_ref().is_none_or(|t| t.story != e.id))
        {
            continue;
        }
        let bounds = text_geometry::frame_bounds(run).unwrap();
        let color = crate::ui::theme::select();
        painter.rect_stroke(
            Rect::from_min_max(to_screen(bounds.min), to_screen(bounds.max)),
            0.,
            Stroke::new(0.7, color),
            egui::StrokeKind::Inside,
        );
        for (output, point) in [
            (false, bounds.min + Pt::new(0., 12. / view.scale)),
            (true, bounds.max - Pt::new(0., 12. / view.scale)),
        ] {
            let center = to_screen(point);
            let port = Rect::from_center_size(center, egui::vec2(11., 11.));
            let overflow = output && run.layout.as_ref().is_some_and(|l| l.overflow);
            let ink = if overflow {
                Color32::from_rgb(240, 55, 75)
            } else {
                color
            };
            painter.rect_filled(port, 1., crate::ui::theme::bg_panel());
            painter.rect_stroke(port, 1., Stroke::new(1.5, ink), egui::StrokeKind::Inside);
            if overflow {
                painter.text(
                    center,
                    egui::Align2::CENTER_CENTER,
                    "+",
                    egui::FontId::proportional(12.),
                    ink,
                );
            }
            let response = ui.interact(
                port.expand(2.),
                egui::Id::new(("text-port", id, output)),
                Sense::click(),
            );
            if response.clicked() {
                handled = true;
                if output {
                    ui.ctx().data_mut(|d| {
                        d.insert_temp(egui::Id::new("loaded-text-port"), (*layer, *id))
                    });
                    studio.status =
                        "click a frame or empty canvas to continue this story · Esc cancels".into();
                } else {
                    ui.ctx()
                        .data_mut(|d| d.insert_temp(egui::Id::new("show-text-threads"), true));
                }
            }
            if response.hovered() {
                handled = true;
            }
            response.context_menu(|ui| {
                if output && ui.button("Break outgoing thread").clicked() {
                    studio.break_text_thread((*layer, *id));
                    ui.close();
                }
            });
        }
        let side = to_screen(Pt::new(bounds.max.x, bounds.center().y));
        let response = ui.interact(
            Rect::from_center_size(side, egui::vec2(10., 14.)),
            egui::Id::new(("area-side-widget", id)),
            Sense::click(),
        );
        painter.rect_filled(response.rect, 1., color);
        if response.double_clicked() {
            studio.selection = vec![(*layer, *id)];
            studio.convert_text_frame(false);
            handled = true;
        }
        if all
            && let Some(next) = run.thread.as_ref().and_then(|t| t.next)
            && let Some((_, _, next_run)) = frames.iter().find(|(_, id, _)| *id == next)
            && let Some(next_bounds) = text_geometry::frame_bounds(next_run)
        {
            painter.line_segment(
                [to_screen(bounds.max), to_screen(next_bounds.min)],
                Stroke::new(1., color),
            );
        }
    }
    handled
}
