//! Editable path brackets and type geometry controls.
use crate::{
    app::Studio,
    geom::{Geom, Pt},
    text_geometry::{self, PathAlign},
};
use eframe::egui::{self, Color32, Rect, Sense, Stroke, Ui};

/// Check the current text or wrap owner before inspector edits.
/// Args: editor. Returns false for busy, hidden, locked or missing owners, including a linked story head.
pub(super) fn edits_allowed(studio: &Studio) -> bool {
    if studio.file_dialog_pending()
        || studio.updates.freezing
        || studio.channel_target.is_some()
        || studio.quick_mask_painting()
        || studio.op.is_some()
        || studio.free_transform.is_some()
        || studio.deformation.is_some()
    {
        return false;
    }
    if studio.selected_layer.is_some_and(|id| {
        !studio
            .doc
            .layers
            .iter()
            .position(|layer| layer.id == id)
            .is_some_and(|index| studio.doc.layer_editable(index))
    }) {
        return false;
    }
    studio.selection.iter().all(|&(layer, id)| {
        let editable = |(layer, id)| {
            studio.doc.layer_editable(layer)
                && (id == crate::document::RASTER_ID
                    || studio
                        .doc
                        .find_shape(layer, id)
                        .is_some_and(|shape| shape.visible && !shape.locked))
        };
        editable((layer, id))
            && (id == crate::document::RASTER_ID || editable(studio.story_head((layer, id))))
    })
}

pub fn path_inspector(ui: &mut Ui, studio: &mut Studio) {
    let owner = egui::Id::new(("text-path-fields", &studio.swap_id, &studio.selection));
    let allowed = edits_allowed(studio);
    ui.push_id(owner, |ui| {
        ui.add_enabled_ui(allowed, |ui| path_controls(ui, studio))
    });
}

fn path_controls(ui: &mut Ui, studio: &mut Studio) {
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
            let mut start_percent = f64::from(on.start) * 100.;
            let mut end_percent = f64::from(on.end) * 100.;
            let range = f64::from(f32::MIN) * 100. ..=f64::from(f32::MAX) * 100.;
            if super::fields::number(
                ui,
                "path-start-percent",
                "Path start (%)",
                &mut start_percent,
                range.clone(),
                " %",
                None,
                Default::default(),
            )
            .changed
            {
                on.start = (start_percent / 100.) as f32;
            }
            if super::fields::number(
                ui,
                "path-end-percent",
                "Path end (%)",
                &mut end_percent,
                range,
                " %",
                None,
                Default::default(),
            )
            .changed
            {
                on.end = (end_percent / 100.) as f32;
            }
            if length > 0. {
                let mut start = f64::from(on.start) * f64::from(length);
                let mut end = f64::from(on.end) * f64::from(length);
                let range = f64::from(f32::MIN) * f64::from(length)
                    ..=f64::from(f32::MAX) * f64::from(length);
                if super::fields::number(
                    ui,
                    "path-start-pixels",
                    "Path start (px)",
                    &mut start,
                    range.clone(),
                    " px",
                    None,
                    Default::default(),
                )
                .changed
                {
                    on.start = (start / f64::from(length)) as f32;
                }
                if super::fields::number(
                    ui,
                    "path-end-pixels",
                    "Path end (px)",
                    &mut end,
                    range,
                    " px",
                    None,
                    Default::default(),
                )
                .changed
                {
                    on.end = (end / f64::from(length)) as f32;
                }
            }
            ui.checkbox(&mut on.flip, "Flip side and direction");
            super::fields::number(
                ui,
                "path-baseline-shift",
                "Path baseline shift (px)",
                &mut on.baseline_shift,
                f32::MIN..=f32::MAX,
                " px",
                None,
                Default::default(),
            );
            super::fields::number(
                ui,
                "path-curve-spacing",
                "Path curve spacing (px)",
                &mut on.spacing,
                f32::MIN..=f32::MAX,
                " px",
                None,
                Default::default(),
            );
            let label = ui.label("Align to path");
            let response = egui::ComboBox::from_id_salt("path-alignment")
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
                })
                .response
                .labelled_by(label.id);
            ui.ctx()
                .accesskit_node_builder(response.id, |node| node.set_label("Align text to path"));
        });
    if ui.is_enabled() && before != on {
        studio.patch_type(|t| {
            t.on_path = Some(on.clone());
            t.layout = None;
        });
    }
}
pub fn menu(ui: &mut Ui, studio: &mut Studio) {
    super::type_commands::menu(ui, studio);
    ui.separator();
    if ui.add_enabled(edits_allowed(studio) && studio.has_outline_text_targets(), egui::Button::new("Create outlines")).clicked() {
        studio.create_text_outlines();
        ui.close();
    }
    ui.separator();
    frame_menu(ui, studio);
    if ui
        .add_enabled(
            edits_allowed(studio) && studio.can_attach_text_path(),
            egui::Button::new("Attach text to path"),
        )
        .clicked()
    {
        studio.attach_text_path();
        ui.close();
    }
    let attached =
        edits_allowed(studio) && studio.primary().is_some_and(|hit| studio.text_structure_editable(hit))
            && studio.selected_type().is_some_and(|t| t.on_path.is_some());
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
    let owner = egui::Id::new(("text-frame-fields", &studio.swap_id, &studio.selection));
    let allowed = edits_allowed(studio);
    ui.push_id(owner, |ui| {
        ui.add_enabled_ui(allowed, |ui| frame_controls(ui, studio))
    });
}

fn frame_controls(ui: &mut Ui, studio: &mut Studio) {
    use text_geometry::{Overflow, VAlign};
    let Some(run) = studio.selected_type() else {
        return;
    };
    let Some(mut frame) = run.frame.clone() else {
        return;
    };
    let before = frame.clone();
    let linked = run
        .thread
        .as_ref()
        .is_some_and(|thread| thread.next.is_some());
    egui::CollapsingHeader::new("Text frame")
        .default_open(true)
        .show(ui, |ui| {
            ui.columns(2, |columns| {
                let field = super::fields::number(
                    &mut columns[0],
                    "frame-width",
                    "Text frame width (px)",
                    &mut frame.size.x,
                    1.0..=100000.,
                    " px",
                    None,
                    Default::default(),
                );
                crate::app::input_history::record(field.response, field.changed);
                let field = super::fields::number(
                    &mut columns[1],
                    "frame-height",
                    "Text frame height (px)",
                    &mut frame.size.y,
                    1.0..=100000.,
                    " px",
                    None,
                    Default::default(),
                );
                crate::app::input_history::record(field.response, field.changed);
            });
            for (index, label) in ["Inset top", "Inset right", "Inset bottom", "Inset left"]
                .into_iter()
                .enumerate()
            {
                let field = super::fields::number(
                    ui,
                    label,
                    &format!("Text frame {label} (px)"),
                    &mut frame.inset[index],
                    0.0..=10000.,
                    " px",
                    None,
                    Default::default(),
                );
                crate::app::input_history::record(field.response, field.changed);
            }
            ui.horizontal(|ui| {
                for (value, label, glyph) in [
                    (VAlign::Top, "Top", Some(super::icons::ph::ALIGN_TOP)),
                    (
                        VAlign::Center,
                        "Center",
                        Some(super::icons::ph::ALIGN_CENTER_V),
                    ),
                    (
                        VAlign::Bottom,
                        "Bottom",
                        Some(super::icons::ph::ALIGN_BOTTOM),
                    ),
                    (VAlign::Justify, "Justify", None),
                ] {
                    let selected = frame.valign == value;
                    let (rect, _) = ui.allocate_exact_size(egui::vec2(30., 28.), Sense::hover());
                    let response = ui.interact(
                        rect,
                        ui.make_persistent_id(("frame-valign", label)),
                        Sense::click(),
                    );
                    response.widget_info(|| {
                        egui::WidgetInfo::selected(
                            egui::WidgetType::Button,
                            ui.is_enabled(),
                            selected,
                            format!("Text frame vertical alignment: {label}"),
                        )
                    });
                    super::welcome::focus_control(ui, &response);
                    if selected || response.hovered() || response.has_focus() {
                        ui.painter().rect_filled(
                            rect.shrink(1.),
                            6.,
                            if selected {
                                super::theme::accent_soft()
                            } else {
                                super::theme::bg_widget_hover()
                            },
                        );
                    }
                    let color = if selected {
                        super::theme::accent()
                    } else {
                        super::theme::fg_weak()
                    };
                    if let Some(glyph) = glyph {
                        ui.painter().text(
                            rect.center(),
                            egui::Align2::CENTER_CENTER,
                            glyph,
                            super::icons::font(18.),
                            color,
                        );
                    } else {
                        // Evenly spread baselines between the top and bottom of the frame.
                        for y in [rect.top() + 5., rect.center().y, rect.bottom() - 5.] {
                            ui.painter().line_segment(
                                [
                                    egui::pos2(rect.left() + 7., y),
                                    egui::pos2(rect.right() - 7., y),
                                ],
                                Stroke::new(1.4, color),
                            );
                        }
                        for x in [rect.left() + 4., rect.right() - 4.] {
                            ui.painter().line_segment(
                                [
                                    egui::pos2(x, rect.top() + 4.),
                                    egui::pos2(x, rect.bottom() - 4.),
                                ],
                                Stroke::new(0.8, color),
                            );
                        }
                    }
                    if response
                        .on_hover_text(format!("Vertical alignment: {label}"))
                        .clicked()
                    {
                        frame.valign = value;
                    }
                }
            });
            ui.add_enabled(
                frame.contour.is_none(),
                egui::Checkbox::new(&mut frame.auto_height, "Auto height"),
            );
            let label = ui.label("Text frame overflow");
            let response = egui::ComboBox::from_id_salt("frame-overflow")
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
                })
                .response
                .labelled_by(label.id);
            ui.ctx()
                .accesskit_node_builder(response.id, |node| node.set_label("Text frame overflow"));
            super::welcome::focus_control(ui, &response);
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
                let field = super::fields::number(
                    ui,
                    "frame-max-lines",
                    "Text frame maximum lines",
                    max,
                    1..=100000,
                    "",
                    None,
                    Default::default(),
                );
                crate::app::input_history::record(field.response, field.changed);
            }
            if run.layout.as_ref().is_some_and(|l| l.overflow) {
                ui.colored_label(
                    Color32::from_rgb(235, 65, 75),
                    "+ Text overflows this frame",
                );
            }
        });
    if ui.is_enabled() && frame != before {
        studio.patch_frame_geometry(|t| {
            t.frame = Some(frame.clone());
            t.layout = None;
        });
    }
}

pub fn wrap_inspector(ui: &mut Ui, studio: &mut Studio) {
    let owner = egui::Id::new((
        "text-wrap-fields",
        &studio.swap_id,
        &studio.selection,
        studio.selected_layer,
    ));
    let allowed = edits_allowed(studio);
    ui.push_id(owner, |ui| {
        ui.add_enabled_ui(allowed, |ui| wrap_controls(ui, studio))
    });
}

fn wrap_controls(ui: &mut Ui, studio: &mut Studio) {
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
            ui.horizontal(|ui| {
                for (mode, label) in [
                    (WrapMode::None, "None"),
                    (WrapMode::BoundingBox, "Bounding box"),
                    (WrapMode::ObjectShape, "Object shape"),
                    (WrapMode::JumpObject, "Jump object"),
                ] {
                    let selected = wrap.mode == mode;
                    let (rect, _) = ui.allocate_exact_size(egui::vec2(30., 28.), Sense::hover());
                    let response = ui.interact(
                        rect,
                        ui.make_persistent_id(("text-wrap-mode", label)),
                        Sense::click(),
                    );
                    response.widget_info(|| {
                        egui::WidgetInfo::selected(
                            egui::WidgetType::Button,
                            ui.is_enabled(),
                            selected,
                            format!("Text wrap: {label}"),
                        )
                    });
                    super::welcome::focus_control(ui, &response);
                    if selected || response.hovered() || response.has_focus() {
                        ui.painter().rect_filled(
                            rect.shrink(1.),
                            6.,
                            if selected {
                                super::theme::accent_soft()
                            } else {
                                super::theme::bg_widget_hover()
                            },
                        );
                    }
                    let color = if selected {
                        super::theme::accent()
                    } else {
                        super::theme::fg_weak()
                    };
                    let center = rect.center();
                    for offset in [-9.0_f32, -4.5, 0., 4.5, 9.] {
                        let y = center.y + offset;
                        if mode == WrapMode::None || offset.abs() > 6. {
                            ui.painter().line_segment(
                                [
                                    egui::pos2(rect.left() + 5., y),
                                    egui::pos2(rect.right() - 5., y),
                                ],
                                Stroke::new(1.2, color),
                            );
                        } else if mode != WrapMode::JumpObject {
                            let gap = if mode == WrapMode::ObjectShape {
                                (36. - offset * offset).sqrt() + 2.
                            } else {
                                8.
                            };
                            ui.painter().line_segment(
                                [
                                    egui::pos2(rect.left() + 5., y),
                                    egui::pos2(center.x - gap, y),
                                ],
                                Stroke::new(1.2, color),
                            );
                            ui.painter().line_segment(
                                [
                                    egui::pos2(center.x + gap, y),
                                    egui::pos2(rect.right() - 5., y),
                                ],
                                Stroke::new(1.2, color),
                            );
                        }
                    }
                    if mode == WrapMode::ObjectShape {
                        ui.painter()
                            .circle_stroke(center, 5., Stroke::new(1.1, color));
                    } else if mode != WrapMode::None {
                        ui.painter().rect_stroke(
                            Rect::from_center_size(center, egui::vec2(10., 11.)),
                            1.,
                            Stroke::new(1.1, color),
                            egui::StrokeKind::Inside,
                        );
                    }
                    if response
                        .on_hover_text(format!("Text wrap: {label}"))
                        .clicked()
                    {
                        wrap.mode = mode;
                    }
                }
            });
            if wrap.mode == WrapMode::ObjectShape {
                let mut offset = wrap.offset[0];
                if super::fields::number(
                    ui,
                    "wrap-offset",
                    "Text wrap outline offset (px)",
                    &mut offset,
                    0.0..=10000.,
                    " px",
                    None,
                    Default::default(),
                )
                .changed
                {
                    wrap.offset = [offset; 4];
                }
            } else {
                for (i, label) in ["Top", "Right", "Bottom", "Left"].into_iter().enumerate() {
                    super::fields::number(
                        ui,
                        label,
                        &format!("Text wrap {label} offset (px)"),
                        &mut wrap.offset[i],
                        0.0..=10000.,
                        " px",
                        None,
                        Default::default(),
                    );
                }
            }
            ui.checkbox(&mut wrap.invert, "Invert: flow inside object");
            ui.checkbox(&mut wrap.ignore_locked, "Ignore on locked layers");
            let mut above = studio.doc.text_wrap_above_only;
            if ui
                .checkbox(&mut above, "Wrap affects text above only")
                .changed()
                && ui.is_enabled()
            {
                studio.commit(crate::document::Cmd::SetTextWrapAbove {
                    before: studio.doc.text_wrap_above_only,
                    after: above,
                });
            }
        });
    if ui.is_enabled() && wrap != before {
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
    let allowed = edits_allowed(studio);
    ui.add_enabled_ui(allowed, |ui| frame_actions(ui, studio));
}

fn frame_actions(ui: &mut Ui, studio: &mut Studio) {
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
    let structural = studio.primary().is_some_and(|hit| studio.text_structure_editable(hit));
    if ui
        .add_enabled(
            structural && live.as_ref().is_some_and(|t| t.frame.is_none()),
            egui::Button::new("Convert to area text"),
        )
        .clicked()
    {
        studio.convert_text_frame(true);
        ui.close();
    }
    if ui
        .add_enabled(
            structural && live.as_ref().is_some_and(|t| t.frame.is_some()),
            egui::Button::new("Convert to point text"),
        )
        .clicked()
    {
        studio.convert_text_frame(false);
        ui.close();
    }
    if let Some(&(layer, id)) = studio.selection.first()
        && structural
        && live
            .as_ref()
            .is_some_and(|t| t.thread.as_ref().is_some_and(|t| t.next.is_some()))
        && ui.button("Break outgoing text thread").clicked()
    {
        studio.break_text_thread((layer, id));
        ui.close();
    }
    let targets: Vec<_> = studio
        .selection
        .iter()
        .copied()
        .filter(|&(layer, id)| {
            studio
                .doc
                .find_shape(layer, id)
                .is_some_and(|shape| matches!(&shape.geom, Geom::Text(run) if run.frame.is_some()))
        })
        .collect();
    let available = studio.selection.len() == 2 && targets.len() == 2
        && targets.iter().all(|&hit| studio.text_structure_editable(hit));
    let response = ui.add_enabled(available, egui::Button::new("Thread selected text frames"));
    let error_id =
        ui.make_persistent_id(("thread-selection-error", &studio.swap_id, &studio.selection));
    if available {
        ui.ctx().accesskit_node_builder(response.id, |node| {
            node.set_description(format!(
                "Flow from frame #{} into frame #{} in selection order",
                targets[0].1, targets[1].1
            ))
        });
    }
    if ui.is_enabled() && available && response.clicked() {
        match studio.thread_text_frames(targets[0], targets[1]) {
            Ok(()) => {
                ui.data_mut(|data| data.remove::<String>(error_id));
                ui.close();
            }
            Err(error) => {
                ui.data_mut(|data| data.insert_temp(error_id, error));
            }
        }
    }
    if let Some(error) = ui.data(|data| data.get_temp::<String>(error_id)) {
        super::welcome::control_error(ui, &[&response], &error);
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

#[cfg(test)]
mod keyboard_tests {
    use super::*;
    use crate::ui::welcome::keyboard_tests as keys;
    use egui::{Event, Key, Modifiers};

    fn frame(ctx: &egui::Context, studio: &mut Studio, events: Vec<Event>) -> egui::FullOutput {
        keys::render(ctx, events, |ui| {
            if !studio.updates.freezing && !studio.file_dialog_pending() {
                studio.handle_shortcuts(ui.ctx());
            }
            ui.menu_button("Text actions", |ui| menu(ui, studio));
            super::super::paragraph::show(ui, studio);
            frame_inspector(ui, studio);
            path_inspector(ui, studio);
            wrap_inspector(ui, studio);
            ui.interact(
                ui.available_rect_before_wrap(),
                egui::Id::new("studio-canvas"),
                Sense::click(),
            );
            super::super::keyboard_authoring::show(ui.ctx(), studio);
        })
    }

    fn editor() -> (egui::Context, Studio) {
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        crate::ui::theme::apply(&ctx);
        let mut studio = Studio::new();
        studio.doc = crate::document::Document::new("Keyboard type geometry", 128., 96., 72.);
        studio.show_welcome = false;
        studio.text_font = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/assets/fonts/EBGaramond.ttf"
        )
        .into();
        (ctx, studio)
    }

    fn text(ctx: &egui::Context, studio: &mut Studio, content: &str) -> (usize, u64) {
        frame(ctx, studio, keys::key(Key::F8, Modifiers::NONE));
        frame(ctx, studio, keys::key(Key::T, Modifiers::NONE));
        frame(ctx, studio, keys::key(Key::Enter, Modifiers::NONE));
        frame(ctx, studio, vec![]);
        assert!(super::super::keyboard_authoring::is_open(ctx));
        frame(
            ctx,
            studio,
            keys::key(Key::Enter, Modifiers::CTRL | Modifiers::COMMAND),
        );
        frame(ctx, studio, vec![]);
        assert!(studio.type_edit.is_some());
        frame(
            ctx,
            studio,
            keys::key(Key::A, Modifiers::CTRL | Modifiers::COMMAND),
        );
        frame(ctx, studio, vec![Event::Text(content.into())]);
        frame(ctx, studio, keys::key(Key::Escape, Modifiers::NONE));
        frame(ctx, studio, vec![]);
        assert!(studio.type_edit.is_none());
        studio.primary().unwrap()
    }

    fn action(ctx: &egui::Context, studio: &mut Studio, label: &str) {
        keys::tab_to(ctx, "Text actions", |events| frame(ctx, studio, events));
        frame(ctx, studio, keys::key(Key::Enter, Modifiers::NONE));
        frame(ctx, studio, vec![]);
        keys::tab_to(ctx, label, |events| frame(ctx, studio, events));
        frame(ctx, studio, keys::key(Key::Enter, Modifiers::NONE));
        frame(ctx, studio, vec![]);
    }

    #[test]
    fn keyboard_text_fields_edit_created_paragraphs_and_reject_locked_or_busy_targets() {
        let (ctx, mut studio) = editor();
        assert!(studio.selection.is_empty());
        let hit = text(&ctx, &mut studio, "Alpha\nBeta");
        keys::tab_to(&ctx, "Wrap text to width", |events| {
            frame(&ctx, &mut studio, events)
        });
        frame(&ctx, &mut studio, keys::key(Key::Space, Modifiers::NONE));
        let output = keys::tab_to(&ctx, "Text wrap width (px)", |events| {
            frame(&ctx, &mut studio, events)
        });
        let width_id = keys::named(&output, "Text wrap width (px)").0;
        keys::replace("96", |events| frame(&ctx, &mut studio, events));
        frame(&ctx, &mut studio, keys::key(Key::Enter, Modifiers::NONE));
        assert_eq!(studio.selected_type().unwrap().wrap_width, Some(96.));
        keys::tab_to(&ctx, "Justification", |events| {
            frame(&ctx, &mut studio, events)
        });
        frame(&ctx, &mut studio, keys::key(Key::Enter, Modifiers::NONE));
        frame(&ctx, &mut studio, vec![]);
        keys::tab_to(&ctx, "Word spacing desired (%)", |events| {
            frame(&ctx, &mut studio, events)
        });
        keys::replace("120", |events| frame(&ctx, &mut studio, events));
        frame(&ctx, &mut studio, keys::key(Key::Enter, Modifiers::NONE));
        assert_eq!(
            crate::text::paragraph_style(&studio.selected_type().unwrap(), 0).word_spacing[1],
            120.
        );
        let output = frame(&ctx, &mut studio, vec![]);
        assert_eq!(keys::named(&output, "Text wrap width (px)").0, width_id);
        let before = studio.doc.find_shape(hit.0, hit.1).unwrap().geom.clone();
        studio.doc.find_shape_mut(hit.0, hit.1).unwrap().locked = true;
        let output = frame(&ctx, &mut studio, keys::key(Key::ArrowUp, Modifiers::NONE));
        assert!(
            keys::named(&output, "Word spacing desired (%)")
                .1
                .is_disabled()
        );
        assert_eq!(studio.doc.find_shape(hit.0, hit.1).unwrap().geom, before);
        studio.doc.find_shape_mut(hit.0, hit.1).unwrap().locked = false;
        studio.updates.freezing = true;
        let output = frame(&ctx, &mut studio, keys::key(Key::ArrowUp, Modifiers::NONE));
        assert!(keys::named(&output, "Text wrap width (px)").1.is_disabled());
        assert_eq!(studio.doc.find_shape(hit.0, hit.1).unwrap().geom, before);
    }

    #[test]
    fn keyboard_text_frames_create_edit_thread_save_reopen_and_undo_without_pointer() {
        let (ctx, mut studio) = editor();
        assert!(studio.selection.is_empty());
        let first = text(&ctx, &mut studio, "First story.");
        action(&ctx, &mut studio, "Convert to area text");
        let output = keys::tab_to(&ctx, "Text frame width (px)", |events| {
            frame(&ctx, &mut studio, events)
        });
        let first_width = keys::named(&output, "Text frame width (px)").0;
        keys::replace("80", |events| frame(&ctx, &mut studio, events));
        frame(&ctx, &mut studio, keys::key(Key::Enter, Modifiers::NONE));
        assert_eq!(studio.selected_type().unwrap().frame.unwrap().size.x, 80.);
        let second = text(&ctx, &mut studio, "Second story.");
        action(&ctx, &mut studio, "Convert to area text");
        let output = frame(&ctx, &mut studio, vec![]);
        assert_ne!(keys::named(&output, "Text frame width (px)").0, first_width);
        frame(&ctx, &mut studio, keys::key(Key::F8, Modifiers::NONE));
        frame(&ctx, &mut studio, keys::key(Key::PageUp, Modifiers::NONE));
        assert_eq!(studio.selection, vec![first]);
        frame(
            &ctx,
            &mut studio,
            keys::key(Key::PageDown, Modifiers::SHIFT),
        );
        assert_eq!(studio.selection, vec![first, second]);
        let history = studio.history.len();
        action(&ctx, &mut studio, "Thread selected text frames");
        assert_eq!(studio.history.len(), history + 1);
        let source = match &studio.doc.find_shape(first.0, first.1).unwrap().geom {
            Geom::Text(run) => run,
            _ => unreachable!(),
        };
        assert_eq!(source.thread.as_ref().unwrap().next, Some(second.1));
        assert!(
            source.content.contains("First story.") && source.content.contains("Second story.")
        );
        let target = match &studio.doc.find_shape(second.0, second.1).unwrap().geom {
            Geom::Text(run) => run,
            _ => unreachable!(),
        };
        assert_eq!(target.thread.as_ref().unwrap().prev, Some(first.1));
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join(".artifacts")
            .join(format!(
                "keyboard-text-frames-{}-{}",
                std::process::id(),
                crate::document::next_id()
            ));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("threaded.oma");
        crate::project::save_to(&studio.doc, &path).unwrap();
        let reopened = crate::project::load_from(&path).unwrap();
        let Geom::Text(source) = &reopened.find_shape(first.0, first.1).unwrap().geom else {
            unreachable!()
        };
        assert_eq!(source.thread.as_ref().unwrap().next, Some(second.1));
        assert!(source.content.contains("Second story."));
        std::fs::remove_dir_all(root).unwrap();
        frame(&ctx, &mut studio, keys::key(Key::F8, Modifiers::NONE));
        frame(
            &ctx,
            &mut studio,
            keys::key(Key::Z, Modifiers::CTRL | Modifiers::COMMAND),
        );
        let Geom::Text(source) = &studio.doc.find_shape(first.0, first.1).unwrap().geom else {
            unreachable!()
        };
        assert!(source.thread.is_none());
        assert_eq!(source.content, "First story.");
    }
}

#[cfg(test)]
#[path = "type_inspector_tests.rs"]
mod responsive_tests;
