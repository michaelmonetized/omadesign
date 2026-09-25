use crate::app::{CreateKind, NodeHit, ObjSnap, Op, Studio, from_egui, to_egui};
use crate::compositor::{self, Draft};
use crate::document::{Fill, RASTER_ID};
use crate::geom::{Anchor, Bounds, Geom, Pt, insert_anchor};
use crate::paint;
use crate::tools::{Persona, Tool};
use crate::ui::theme::{accent, fg_weak, select, select_fill};
use eframe::egui::{
    Color32, ColorImage, PointerButton, Pos2, Rect, Sense, Shape, Stroke, TextureOptions, Ui, Vec2,
    pos2, vec2,
};
use std::collections::BTreeSet;

pub fn show(ui: &mut Ui, studio: &mut Studio) {
    let (_, rect) = ui.allocate_space(ui.available_size());
    let resp = ui.interact(
        rect,
        eframe::egui::Id::new("studio-canvas"),
        Sense::click_and_drag(),
    );
    studio.canvas_rect = Some(rect);
    if studio.type_edit.is_some()
        && (resp.clicked() || ui.ctx().memory(|memory| memory.focused().is_none()))
    {
        resp.request_focus();
    }

    if studio.need_fit {
        studio.view.fit(
            studio.doc.size(),
            crate::geom::Bounds {
                min: Pt::ZERO,
                max: Pt::new(rect.width(), rect.height()),
            },
        );
        studio.need_fit = false;
    }

    let ctx = ui.ctx().clone();
    let space = ctx.input(|i| i.key_down(eframe::egui::Key::Space));
    let space_pan = space && !studio.is_motion();
    let scroll = ctx.input(|i| i.smooth_scroll_delta);
    let zoom_delta = ctx.input(|i| i.zoom_delta());
    let pointer = resp.hover_pos();
    let origin = Pt::new(rect.min.x, rect.min.y);
    let local = pointer
        .map(|hp| from_egui(hp) - origin)
        .or_else(|| studio.cursor.map(|w| studio.view.to_screen(w)))
        .unwrap_or_else(|| Pt::new(rect.width() * 0.5, rect.height() * 0.5));
    let alt = ctx.input(|i| i.modifiers.alt);
    let ctrl = ctx.input(|i| i.modifiers.ctrl || i.modifiers.command);
    // Pinch (Event::Zoom), Ctrl+scroll, Alt+scroll, or scroll while Z is active.
    let canvas_input = resp.hovered() || resp.dragged();
    let pinch = (zoom_delta - 1.0).abs() > 1e-4;
    let scroll_zoom = scroll.y.abs() > 0.0 && studio.tool == Tool::Zoom;
    if canvas_input && pinch {
        studio.view.zoom_at(local, zoom_delta);
        ctx.request_repaint();
    } else if canvas_input && scroll_zoom {
        studio.view.zoom_at(local, (scroll.y / 200.0).exp());
    } else if canvas_input && scroll != Vec2::ZERO {
        studio.view.offset.x += scroll.x;
        studio.view.offset.y += scroll.y;
    }
    if let Some(hp) = pointer {
        studio.cursor = Some(studio.view.to_world(from_egui(hp) - origin));
    }

    let brand_input = super::library::canvas_drop(ui, studio, &resp);
    let shift = ctx.input(|i| i.modifiers.shift);
    studio.snap_override = ctrl;

    let plugin_input = !brand_input
        && super::plugins::canvas_input(
            ui,
            studio,
            &resp,
            rect,
            space_pan || studio.tool == Tool::Hand,
        );
    let guide_input = !plugin_input
        && !brand_input
        && studio.deformation.is_none()
        && super::guides::handle_input(ui, studio, rect);
    let deform_input = if plugin_input || guide_input || brand_input {
        false
    } else {
        super::deform::input(studio, &resp, rect, space_pan || studio.tool == Tool::Hand)
    };
    if !plugin_input
        && !brand_input
        && !guide_input
        && !deform_input
        && studio.tool != Tool::Select
        && live_op_should_close(studio, &resp)
    {
        if studio.tool == Tool::Node && resp.drag_stopped_by(PointerButton::Primary) {
            let release = ctx.input(|input| {
                input.events.iter().rev().find_map(|event| match event {
                    eframe::egui::Event::PointerButton {
                        pos,
                        button: PointerButton::Primary,
                        pressed: false,
                        modifiers,
                    } => Some((*pos, *modifiers)),
                    _ => None,
                })
            });
            if let Some((at, modifiers)) = release {
                let point = studio.view.pointer_to_world(origin, from_egui(at));
                continue_drag(studio, point, modifiers.shift, modifiers.alt);
            }
        }
        end_drag(studio, studio.cursor.unwrap_or(Pt::ZERO), alt, ctrl, shift);
    }

    let panning = (space_pan && studio.type_edit.is_none()) || studio.tool == Tool::Hand;
    if plugin_input || guide_input || deform_input || brand_input {
        // Ruler and guide drags own this gesture.
    } else if panning && resp.dragged_by(PointerButton::Primary)
        || resp.dragged_by(PointerButton::Middle)
    {
        let d = resp.drag_delta();
        studio.view.offset.x += d.x;
        studio.view.offset.y += d.y;
    } else {
        handle_pointer(studio, &resp, space_pan);
    }
    if !plugin_input
        && !brand_input
        && !guide_input
        && !deform_input
        && studio.tool == Tool::Select
        && live_op_should_close(studio, &resp)
    {
        // Resolve a Select gesture once, after input so a press and release in
        // the same frame work too. Dragging preserves the whole selection;
        // only an actual click applies the press-time toggle/collapse.
        if resp.clicked()
            && !resp.drag_stopped()
            && let Some(Op::Move {
                selection_on_click, ..
            }) = &studio.op
        {
            studio.selection = selection_on_click.clone();
        }
        if resp.drag_stopped_by(PointerButton::Primary) {
            let release = ctx.input(|input| {
                input.events.iter().rev().find_map(|event| match event {
                    eframe::egui::Event::PointerButton {
                        pos,
                        button: PointerButton::Primary,
                        pressed: false,
                        modifiers,
                    } => Some((*pos, *modifiers)),
                    _ => None,
                })
            });
            if let Some((at, modifiers)) = release {
                let point = studio.view.pointer_to_world(origin, from_egui(at));
                continue_drag(studio, point, modifiers.shift, modifiers.alt);
                studio.cursor = Some(point);
            }
        }
        end_drag(studio, studio.cursor.unwrap_or(Pt::ZERO), alt, ctrl, shift);
    }

    // The rubber band uses the exact point a click would place. Keep the raw
    // cursor for picking/closing paths, and omit the next segment while shaping
    // the current anchor's handles.
    let pen_preview = if studio.tool == Tool::Pen
        && matches!(studio.op, Some(Op::Pen { .. }))
        && !ctx.input(|i| i.pointer.primary_down())
    {
        studio
            .cursor
            .map(|point| pen_snap_point(studio, point, ctx.input(|i| i.modifiers)))
    } else {
        None
    };

    let w = rect.width().max(1.0) as u32;
    let h = rect.height().max(1.0) as u32;
    let op_tag: u8 = match &studio.op {
        Some(Op::Brush { .. }) => 1,
        Some(Op::Smudge { .. } | Op::Clone { .. } | Op::Retouch { .. }) => 2,
        _ => 0,
    };
    let play_bits = if studio.is_motion() {
        studio.playhead.to_bits()
    } else {
        0
    };
    let key = (
        w,
        h,
        studio.view.scale.to_bits(),
        studio.view.offset.x.to_bits(),
        studio.view.offset.y.to_bits(),
        studio.canvas_gen,
        op_tag,
        play_bits,
    );
    let reuse = studio.canvas_key == Some(key)
        && studio
            .screen_tex
            .as_ref()
            .is_some_and(|t| t.size() == [w as usize, h as usize]);
    if !reuse {
        let draft = match &studio.op {
            Some(Op::Brush { layer, buf, .. }) => Draft {
                preview: None,
                brush: Some((*layer, buf, studio.brush.opacity)),
            },
            _ => Draft::none(),
        };
        let pm = if studio.is_motion() {
            compositor::render_view_posed(
                &studio.doc,
                studio.view,
                w,
                h,
                draft,
                Some(studio.playhead),
                Some(&studio.pose_drag),
            )
        } else {
            compositor::render_view(&studio.doc, studio.view, w, h, draft)
        };
        if let Some(pm) = pm {
            let image = eframe::egui::ColorImage::from_rgba_unmultiplied(
                [pm.width() as usize, pm.height() as usize],
                pm.data(),
            );
            let tex = if let Some(mut existing) = studio.screen_tex.take()
                && existing.size() == [pm.width() as usize, pm.height() as usize]
            {
                existing.set(image, eframe::egui::TextureOptions::LINEAR);
                existing
            } else {
                ui.ctx()
                    .load_texture("canvas", image, eframe::egui::TextureOptions::LINEAR)
            };
            studio.screen_tex = Some(tex);
            studio.canvas_key = Some(key);
        }
    }
    if let Some(tex) = &studio.screen_tex {
        ui.painter().image(
            tex.id(),
            rect,
            Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
            Color32::WHITE,
        );
    }

    let painter = ui.painter_at(rect);
    if studio.show_grid || studio.doc.grid.visible {
        draw_grid(&painter, rect, studio);
    }
    super::guides::draw(&painter, rect, studio);
    draw_snap_feedback(&painter, rect, studio);
    draw_artboard_frames(&painter, rect, studio);
    draw_layout_frames(&painter, rect, studio);
    draw_comment_pins(&painter, rect, studio);
    draw_bleed_safe(&painter, rect, studio);
    draw_overlays(&painter, rect, studio, pen_preview);
    super::deform::paint(&painter, rect, studio);
    super::plugins::paint(ui, studio, rect);
    if !plugin_input && !brand_input && !guide_input && !deform_input {
        set_cursor(ui, studio, &resp);
        context_menu(&resp, studio);
    }

    let files: Vec<_> = ui.ctx().input(|i| i.raw.dropped_files.clone());
    if !files.is_empty() {
        let at = studio.cursor;
        for f in files {
            studio.ingest_dropped(f.path(), at);
        }
    }
}

fn win(rect: Rect, view: crate::compositor::View, world: Pt) -> Pos2 {
    to_egui(view.world_to_window(Pt::new(rect.min.x, rect.min.y), world))
}

fn preview_shape(kind: CreateKind, start: Pt, cur: Pt, studio: &Studio) -> Geom {
    let min = Pt::new(start.x.min(cur.x), start.y.min(cur.y));
    let max = Pt::new(start.x.max(cur.x), start.y.max(cur.y));
    let size = max - min;
    match kind {
        CreateKind::Rect | CreateKind::Frame => Geom::Rect {
            origin: min,
            size,
            radius: studio.rect_radius,
        },
        CreateKind::Ellipse => Geom::Ellipse {
            center: (min + max) * 0.5,
            radii: size * 0.5,
        },
        CreateKind::Polygon => Geom::Polygon {
            center: (min + max) * 0.5,
            radii: size * 0.5,
            sides: studio.polygon_sides,
        },
        CreateKind::Star => Geom::Star {
            center: (min + max) * 0.5,
            outer: size * 0.5,
            inner: studio.star_inner,
            points: studio.star_points,
        },
        CreateKind::Line => Geom::Line { a: start, b: cur },
    }
}

fn live_op_should_close(studio: &Studio, resp: &eframe::egui::Response) -> bool {
    match &studio.op {
        None | Some(Op::Pen { .. }) => false,
        _ => {
            resp.drag_stopped()
                || resp
                    .ctx
                    .input(|i| !i.pointer.button_down(PointerButton::Primary))
        }
    }
}

fn pen_snap_point(studio: &mut Studio, point: Pt, modifiers: eframe::egui::Modifiers) -> Pt {
    let anchor = if modifiers.shift {
        match &studio.op {
            Some(Op::Pen { anchors, .. }) => anchors.last().map(|anchor| anchor.pt),
            _ => None,
        }
    } else {
        None
    };
    let previous_override = studio.snap_override;
    studio.snap_override = modifiers.ctrl || modifiers.command;
    let point = studio.snap_tool_point(point, anchor);
    studio.snap_override = previous_override;
    point
}

fn handle_pointer(studio: &mut Studio, resp: &eframe::egui::Response, space: bool) {
    if space {
        return;
    }
    let Some(screen) = resp.interact_pointer_pos().or(resp.hover_pos()) else {
        return;
    };
    let Some(crect) = studio.canvas_rect else {
        return;
    };
    let origin = Pt::new(crect.min.x, crect.min.y);
    let pick = studio.view.pointer_to_world(origin, from_egui(screen));
    let is_text = studio.tool == Tool::Text || studio.type_edit.is_some();
    let alt = resp.ctx.input(|i| i.modifiers.alt);
    let shift = resp.ctx.input(|i| i.modifiers.shift);
    let ctrl = resp.ctx.input(|i| i.modifiers.ctrl || i.modifiers.command);
    studio.snap_override = ctrl;
    if studio.persona == Persona::Pixel && ctrl {
        if resp.ctx.input(|i| i.pointer.primary_pressed()) && crect.contains(screen) {
            if let Some((li, id)) = hit_shape(studio, pick, 4. / studio.view.scale.max(0.01)) {
                studio.apply_item_outline(
                    li,
                    (id != RASTER_ID).then_some(id),
                    super::retouch::pixel_op(false, alt),
                );
            }
        }
        return;
    }
    if resp.ctx.input(|i| i.pointer.primary_pressed()) {
        studio.reset_snap_gesture();
        studio.selected_layer = None;
    }
    let snap = if is_text
        || (studio.pending_place.is_none()
            && studio.op.is_some()
            && !matches!(studio.op, Some(Op::Pen { .. })))
        || matches!(
            studio.tool,
            Tool::Brush | Tool::Eraser | Tool::Smudge | Tool::Clone | Tool::Heal
        ) {
        pick
    } else if studio.tool == Tool::Pen {
        pen_snap_point(studio, pick, resp.ctx.input(|i| i.modifiers))
    } else {
        studio.snap_tool_point(pick, None)
    };

    if studio.tool == Tool::Zoom && resp.clicked() && !resp.dragged() {
        studio.zoom_click(from_egui(screen) - origin, alt, ctrl, shift);
        return;
    }

    if studio.tool == Tool::Eyedropper {
        let pressed = resp.ctx.input(|i| i.pointer.primary_pressed());
        let released = resp.ctx.input(|i| i.pointer.primary_released());
        let armed = resp
            .ctx
            .data(|data| data.get_temp::<bool>(eyedrop_arm()).unwrap_or(false));
        if pressed && crect.contains(screen) {
            resp.ctx
                .data_mut(|data| data.insert_temp(eyedrop_arm(), true));
        }
        if released && armed {
            resp.ctx
                .data_mut(|data| data.insert_temp(eyedrop_arm(), false));
            if crect.contains(screen) {
                studio.eyedrop(pick);
            }
        }
        return;
    }

    if studio.pinning_comment && resp.clicked() {
        studio.drop_comment_pin(pick);
        return;
    }

    if studio.pending_place.is_some() {
        if resp.ctx.input(|i| i.pointer.primary_pressed()) {
            studio.op = Some(Op::Place {
                start: snap,
                cur: snap,
            });
        }
        if resp.dragged_by(PointerButton::Primary)
            && let Some(Op::Place { cur, .. }) = &mut studio.op
        {
            *cur = snap;
        }
        if resp.clicked() && !resp.dragged() && !resp.drag_stopped() {
            studio.commit_place_at(snap);
        }
        return;
    }

    if studio.tool == Tool::Trace && resp.clicked() {
        studio.trace_active_raster();
        return;
    }

    if studio.type_edit.is_some() && resp.clicked() {
        let slack = 8.0 / studio.view.scale.max(0.01);
        if let Some(hit) = hit_shape(studio, pick, slack) {
            if studio.editing_text(hit.0, hit.1) {
                studio.begin_type_edit(hit, pick);
                return;
            }
            if is_text_hit(studio, hit) {
                studio.commit_type_edit();
                studio.begin_type_edit(hit, pick);
                return;
            }
        }
        studio.commit_type_edit();
        if studio.tool == Tool::Text {
            studio.place_text(pick);
        }
        return;
    }

    if studio.tool == Tool::Text && resp.clicked() {
        let slack = 8.0 / studio.view.scale.max(0.01);
        if let Some(hit) = hit_shape(studio, pick, slack)
            && is_text_hit(studio, hit)
        {
            studio.begin_type_edit(hit, pick);
            return;
        }
        studio.place_text(pick);
        return;
    }

    if studio.tool == Tool::Fill && resp.clicked() {
        super::retouch::fill(studio, pick);
        return;
    }

    if studio.tool == Tool::Wand {
        let press = resp.ctx.input(|i| {
            i.events.iter().find_map(|event| match event {
                eframe::egui::Event::PointerButton {
                    pos,
                    button: PointerButton::Primary,
                    pressed: true,
                    modifiers,
                } => Some((*pos, *modifiers)),
                _ => None,
            })
        });
        if let Some((at, modifiers)) = press
            && crect.contains(at)
            && resp.ctx.layer_id_at(at) == Some(resp.layer_id)
        {
            let point = studio.view.pointer_to_world(origin, from_egui(at));
            super::retouch::apply_wand(
                studio,
                point,
                super::retouch::pixel_op(modifiers.shift, modifiers.alt),
            );
        }
        return;
    }

    if matches!(studio.tool, Tool::Clone | Tool::Heal) && alt && resp.clicked() {
        studio.clone_source = Some(pick);
        studio.status = if studio.tool == Tool::Heal {
            "Healing source set · paint over the blemish"
        } else {
            "Clone source set"
        }
        .into();
        return;
    }

    // Text edit drag: lock the caret on press. Waiting for drag_started
    // means the first ~6px already picked a different character.
    if studio.type_edit.is_some() {
        if resp.ctx.input(|i| i.pointer.primary_pressed()) {
            let slack = 8.0 / studio.view.scale.max(0.01);
            if let Some(hit) = hit_shape(studio, pick, slack)
                && studio.editing_text(hit.0, hit.1)
            {
                let caret = studio
                    .doc
                    .find_shape(hit.0, hit.1)
                    .and_then(|s| match &s.geom {
                        Geom::Text(run) => Some(crate::text::hit_char(run, pick)),
                        _ => None,
                    });
                if let (Some(c), Some(e)) = (caret, studio.type_edit.as_mut()) {
                    e.anchor = c;
                    e.caret = c;
                }
                return;
            }
        }
        if resp.dragged_by(PointerButton::Primary) {
            if let Some(edit) = studio.type_edit.as_ref() {
                let (li, id) = (edit.layer, edit.id);
                if let Some(shape) = studio.doc.find_shape(li, id)
                    && let Geom::Text(run) = &shape.geom
                {
                    let new_caret = crate::text::hit_char(run, pick);
                    if let Some(e) = studio.type_edit.as_mut() {
                        e.caret = new_caret;
                    }
                }
            }
            return;
        }
        if resp.drag_started_by(PointerButton::Primary) {
            studio.commit_type_edit();
        }
    }

    // Pen places on pointer *down*. The canvas uses Sense::click_and_drag, so
    // egui withholds drag_started until the pointer moves — a plain click never
    // reached start_drag, and close-on-first-point died with it.
    if studio.tool == Tool::Pen {
        if resp.double_clicked() {
            if let Some(Op::Pen {
                anchors, source, ..
            }) = studio.op.take()
            {
                studio.finish_pen(anchors, false, source);
            }
            return;
        }
        let press = resp.ctx.input(|i| {
            i.events.iter().find_map(|event| match event {
                eframe::egui::Event::PointerButton {
                    pos,
                    button: PointerButton::Primary,
                    pressed: true,
                    modifiers,
                } => Some((*pos, *modifiers)),
                _ => None,
            })
        });
        if let Some((at, modifiers)) = press
            && crect.contains(at)
            && resp.ctx.layer_id_at(at) == Some(resp.layer_id)
        {
            // A rapid Shift release can share this frame with the click. Use
            // the press event's position/modifiers, not the end-of-frame state.
            let pick = studio.view.pointer_to_world(origin, from_egui(at));
            let place = pen_snap_point(studio, pick, modifiers);
            studio.pen_click_at(pick, place);
        }
        if resp.drag_stopped_by(PointerButton::Primary) {
            let release = resp.ctx.input(|i| {
                i.events.iter().rev().find_map(|event| match event {
                    eframe::egui::Event::PointerButton {
                        pos,
                        button: PointerButton::Primary,
                        pressed: false,
                        modifiers,
                    } => Some((*pos, *modifiers)),
                    _ => None,
                })
            });
            if let Some((at, modifiers)) = release {
                let point = studio.view.pointer_to_world(origin, from_egui(at));
                continue_drag(studio, point, modifiers.shift, modifiers.alt);
            }
        } else if resp.dragged_by(PointerButton::Primary) {
            continue_drag(studio, pick, shift, alt);
        }
        return;
    }

    // Same click-vs-drag delay: handles are ~3px. Waiting for drag_started
    // means the pointer has already left the handle, and a segment steal wins.
    if studio.tool == Tool::Node {
        let press = resp.ctx.input(|input| {
            input.events.iter().find_map(|event| match event {
                eframe::egui::Event::PointerButton {
                    pos,
                    button: PointerButton::Primary,
                    pressed: true,
                    modifiers,
                } => Some((*pos, *modifiers)),
                _ => None,
            })
        });
        if let Some((at, modifiers)) = press
            && crect.contains(at)
            && resp.ctx.layer_id_at(at) == Some(resp.layer_id)
        {
            let point = studio.view.pointer_to_world(origin, from_egui(at));
            node_press(studio, point, point, modifiers.shift, modifiers.alt);
        }
        if resp.drag_started_by(PointerButton::Primary) && studio.op.is_none() {
            start_drag(studio, pick, snap, shift, alt);
        }
        if resp.dragged_by(PointerButton::Primary) {
            continue_drag(studio, pick, shift, alt);
        }
        if resp.clicked() && !resp.dragged() && !resp.drag_stopped() {
            node_click(studio, pick, alt);
        }
        return;
    }

    if studio.tool == Tool::Artboard {
        if resp.ctx.input(|i| i.pointer.primary_pressed()) {
            artboard_press(studio, pick, snap, shift, alt);
        }
        if resp.drag_started_by(PointerButton::Primary) && studio.op.is_none() {
            start_drag(studio, pick, snap, shift, alt);
        }
        if resp.dragged_by(PointerButton::Primary) {
            continue_drag(studio, pick, shift, alt);
        }
        return;
    }

    // Same click-vs-drag delay as Pen/Node/Artboard. Lock the hit on press
    // so scale/rotate/corner handles and thin strokes are grabbed where the
    // pointer went down, not 6px later. Do not end a live op on drag_started —
    // that would kill the press lock and re-hit at the moved point.
    let press = resp.ctx.input(|i| {
        i.events.iter().find_map(|event| match event {
            eframe::egui::Event::PointerButton {
                pos,
                button: PointerButton::Primary,
                pressed: true,
                modifiers,
            } => Some((*pos, *modifiers)),
            _ => None,
        })
    });
    if let Some((at, modifiers)) = press
        && studio.op.is_none()
        && studio.type_edit.is_none()
        && pointer_down_starts_op(studio.tool, modifiers.alt)
        && crect.contains(at)
        && resp.ctx.layer_id_at(at) == Some(resp.layer_id)
    {
        let pick = studio.view.pointer_to_world(origin, from_egui(at));
        let press_snap = studio.snap_tool_point(pick, None);
        start_drag(studio, pick, press_snap, modifiers.shift, false);
    }
    if resp.drag_started_by(PointerButton::Primary) {
        if alt && matches!(studio.op, Some(Op::Move { .. })) {
            studio.duplicate_selection_by(Pt::ZERO);
            let orig = snapshot_moving(studio);
            if let Some(Op::Move { orig: slot, .. }) = &mut studio.op {
                *slot = orig;
            }
        }
        if studio.op.is_none() {
            start_drag(studio, pick, snap, shift, alt);
        }
    }

    let primary_down = resp
        .ctx
        .input(|i| i.pointer.button_down(PointerButton::Primary));
    if studio.op.is_some()
        && primary_down
        && (resp.dragged_by(PointerButton::Primary) || pointer_down_starts_op(studio.tool, alt))
    {
        continue_drag(studio, pick, shift, alt);
    }

    if resp.double_clicked() {
        let slack = 8.0 / studio.view.scale.max(0.01);
        if let Some(hit) = hit_shape(studio, pick, slack) {
            if !studio.doc.layer_ancestors(hit.0).is_empty()
                && studio.individual_object != Some(hit)
            {
                studio.enter_group_item(hit);
            } else if is_text_hit(studio, hit) {
                studio.op = None;
                studio.begin_type_edit(hit, pick);
            } else if hit.1 != RASTER_ID && matches!(studio.tool, Tool::Select | Tool::Node) {
                studio.op = None;
                studio.selection = vec![hit];
                studio.node_sel.clear();
                studio.ensure_path(hit.0, hit.1);
                studio.tool = Tool::Node;
            }
        }
    }
}

pub(super) fn poll_screen_pick(ctx: &eframe::egui::Context, studio: &mut Studio) {
    let for_picker = super::color_picker::sample_request(ctx).is_some();
    let tick = crate::screen_pick::sync(studio.tool == Tool::Eyedropper || for_picker);
    if tick.live {
        ctx.request_repaint_after(std::time::Duration::from_millis(80));
    }
    if tick.just_started {
        studio.status = "Click any pixel on the screen".into();
    }
    if let Some(result) = tick.finished {
        match result {
            Ok(color) if for_picker => {
                super::color_picker::finish_sample(ctx, color);
                crate::screen_pick::stop();
                studio.push_recent(color);
                studio.status = format!("sampled screen {}", color.hex());
            }
            Ok(color) => studio.take_sampled(color, true),
            Err(error) => {
                if for_picker {
                    super::color_picker::clear_sample(ctx);
                }
                studio.status = error;
            }
        }
    }
}

fn eyedrop_arm() -> eframe::egui::Id {
    eframe::egui::Id::new("eyedropper-armed")
}

fn pointer_down_starts_op(tool: Tool, alt: bool) -> bool {
    match tool {
        Tool::Pen | Tool::Node | Tool::Artboard => false,
        Tool::Hand | Tool::Eyedropper | Tool::Fill | Tool::Wand | Tool::Text | Tool::Trace => false,
        Tool::Clone | Tool::Heal if alt => false,
        _ => true,
    }
}

fn is_text_hit(studio: &Studio, hit: (usize, u64)) -> bool {
    studio
        .doc
        .find_shape(hit.0, hit.1)
        .is_some_and(|s| matches!(s.geom, Geom::Text(_)))
}

fn start_drag(studio: &mut Studio, pick: Pt, snap: Pt, shift: bool, alt: bool) {
    let _ = alt;
    if studio.is_motion() {
        studio.playing = false;
    }
    // Handle hit takes priority from any design tool so a freshly drawn object
    // can be resized immediately without first pressing V. Text editing, Hand
    // and Zoom are excluded to keep their own pointer semantics.
    if studio.type_edit.is_none()
        && !matches!(
            studio.tool,
            Tool::Hand
                | Tool::Zoom
                | Tool::Node
                | Tool::Pen
                | Tool::Trace
                | Tool::Artboard
                | Tool::Brush
                | Tool::Eraser
                | Tool::Fill
                | Tool::Clone
                | Tool::Heal
                | Tool::Smudge
                | Tool::Marquee
                | Tool::EllipseMarquee
                | Tool::Lasso
                | Tool::Wand
                | Tool::Eyedropper
        )
        && !studio.selection.is_empty()
    {
        if let Some(sel) = hit_handle(studio, pick) {
            match sel {
                HandleKind::Rotate(center) => {
                    let orig = snapshot_moving(studio);
                    studio.op = Some(Op::Rotate {
                        orig,
                        center,
                        start_angle: (pick - center).y.atan2((pick - center).x),
                    });
                    return;
                }
                HandleKind::Scale(i, b) => {
                    studio.op = Some(Op::Resize {
                        orig: snapshot_with_children(studio),
                        handle: i,
                        start_box: b,
                    });
                    return;
                }
            }
        }
    }
    match studio.tool {
        Tool::Select => {
            if let Some(hit) = hit_shape(studio, pick, 6.0 / studio.view.scale.max(0.01)) {
                let hits = studio.selection_for_hit(hit);
                if studio.individual_object != Some(hit) {
                    studio.individual_object = None;
                }
                studio.selected_layer = None;
                let selection_on_click = if shift {
                    let mut selection = studio.selection.clone();
                    if hits.iter().all(|h| selection.contains(h)) {
                        selection.retain(|h| !hits.contains(h));
                    } else {
                        for h in &hits {
                            if !selection.contains(h) {
                                selection.push(*h);
                            }
                        }
                    }
                    selection
                } else {
                    hits.clone()
                };
                if !studio.selection.contains(&hit) {
                    if shift {
                        for h in hits {
                            if !studio.selection.contains(&h) {
                                studio.selection.push(h);
                            }
                        }
                    } else {
                        studio.selection = hits;
                    }
                }
                studio.active_layer = Some(hit.0);
                if alt {
                    studio.duplicate_selection_by(Pt::ZERO);
                }
                studio.op = Some(Op::Move {
                    orig: snapshot_moving(studio),
                    start: pick,
                    selection_on_click,
                });
            } else {
                studio.op = Some(Op::Marquee {
                    start: snap,
                    cur: snap,
                    ellipse: false,
                });
                if !shift {
                    studio.selection.clear();
                    studio.individual_object = None;
                    studio.selected_layer = None;
                }
            }
        }
        Tool::Node => {
            if let Some((li, id)) = studio.primary() {
                if id != RASTER_ID {
                    studio.ensure_path(li, id);
                }
                if let Some(shape) = studio.doc.find_shape(li, id)
                    && matches!(shape.geom, Geom::Path { .. } | Geom::Paths { .. })
                {
                    let slack = 8.0 / studio.view.scale.max(0.01);
                    if let Some(hit) = hit_edit_node(&shape.geom, shape.local_point(pick), slack) {
                        match hit {
                            NodeHit::Point(i) => {
                                if shift {
                                    if !studio.node_sel.remove(&i) {
                                        studio.node_sel.insert(i);
                                    }
                                } else if !studio.node_sel.contains(&i) {
                                    studio.node_sel.clear();
                                    studio.node_sel.insert(i);
                                }
                                let moving = if studio.node_sel.is_empty() {
                                    let mut s = BTreeSet::new();
                                    s.insert(i);
                                    s
                                } else {
                                    studio.node_sel.clone()
                                };
                                studio.op = Some(Op::Node {
                                    layer: li,
                                    id,
                                    which: hit,
                                    orig: shape.geom.clone(),
                                    moving,
                                });
                                return;
                            }
                            NodeHit::HandleIn(_) | NodeHit::HandleOut(_) => {
                                studio.op = Some(Op::Node {
                                    layer: li,
                                    id,
                                    which: hit,
                                    orig: shape.geom.clone(),
                                    moving: BTreeSet::new(),
                                });
                                return;
                            }
                            NodeHit::Segment(seg) => {
                                let mut moving = BTreeSet::new();
                                moving.insert(seg);
                                if let Some(end) = shape.geom.segment_end(seg) {
                                    moving.insert(end);
                                }
                                studio.node_sel = moving.clone();
                                studio.op = Some(Op::Node {
                                    layer: li,
                                    id,
                                    which: hit,
                                    orig: shape.geom.clone(),
                                    moving,
                                });
                                return;
                            }
                        }
                    }
                }
            }
            studio.op = Some(Op::NodeMarquee {
                start: snap,
                cur: snap,
            });
            if !shift {
                studio.node_sel.clear();
            }
        }
        Tool::Artboard => {
            if let Some(id) = studio
                .doc
                .artboard_hit(pick, 8.0 / studio.view.scale.max(0.01))
            {
                if !studio.artboard_sel.contains(&id) {
                    studio.artboard_sel = vec![id];
                }
                if alt {
                    studio.clone_artboard(id);
                }
                let ids = studio.artboard_sel.clone();
                let contents = snapshot_on_artboards(studio, &ids);
                studio.op = Some(Op::ArtboardMove {
                    orig: studio.doc.artboards.clone(),
                    ids,
                    start: pick,
                    contents,
                });
            } else {
                studio.op = Some(Op::ArtboardDraw {
                    start: snap,
                    cur: snap,
                });
                if !shift {
                    studio.artboard_sel.clear();
                }
            }
        }
        Tool::Pen => studio.pen_click_at(pick, snap),
        Tool::Pencil => {
            studio.op = Some(Op::Pencil { pts: vec![snap] });
        }
        Tool::Rect => {
            studio.op = Some(Op::Create {
                from_center: alt,
                kind: CreateKind::Rect,
                start: snap,
                cur: snap,
            })
        }
        Tool::Frame => {
            studio.op = Some(Op::Create {
                from_center: alt,
                kind: CreateKind::Frame,
                start: snap,
                cur: snap,
            })
        }
        Tool::Ellipse => {
            studio.op = Some(Op::Create {
                from_center: alt,
                kind: CreateKind::Ellipse,
                start: snap,
                cur: snap,
            })
        }
        Tool::Polygon => {
            studio.op = Some(Op::Create {
                from_center: alt,
                kind: CreateKind::Polygon,
                start: snap,
                cur: snap,
            })
        }
        Tool::Star => {
            studio.op = Some(Op::Create {
                from_center: alt,
                kind: CreateKind::Star,
                start: snap,
                cur: snap,
            })
        }
        Tool::Line => {
            studio.op = Some(Op::Create {
                from_center: alt,
                kind: CreateKind::Line,
                start: snap,
                cur: snap,
            })
        }
        Tool::Gradient => {
            studio.op = Some(Op::Gradient {
                start: snap,
                cur: snap,
            })
        }
        Tool::Eraser | Tool::Heal => super::retouch::start(studio, pick),
        Tool::Brush if studio.paint_mask => super::retouch::start(studio, pick),
        Tool::Brush => super::retouch::start_brush(studio, pick),
        Tool::Smudge => super::retouch::start_smudge(studio, pick),
        Tool::Clone => super::retouch::start_clone(studio, pick),
        Tool::Marquee => {
            studio.op = Some(Op::Marquee {
                start: snap,
                cur: snap,
                ellipse: false,
            })
        }
        Tool::EllipseMarquee => {
            studio.op = Some(Op::Marquee {
                start: snap,
                cur: snap,
                ellipse: true,
            })
        }
        Tool::Lasso => studio.op = Some(Op::Lasso { pts: vec![snap] }),
        Tool::Crop => {
            studio.op = Some(Op::CropPhoto {
                start: snap,
                cur: snap,
            })
        }
        Tool::Zoom => {
            studio.op = Some(Op::ZoomBox {
                start: snap,
                cur: snap,
            })
        }
        _ => {}
    }
}

fn snap_changed(studio: &Studio, snap: &ObjSnap) -> bool {
    if snap.id == RASTER_ID {
        studio
            .doc
            .layers
            .get(snap.layer)
            .and_then(|l| l.kind.raster_xform())
            .is_some_and(|(o, sz, rot)| {
                (o - snap.origin).length() > 0.2
                    || (sz - snap.size).length() > 0.2
                    || (rot - snap.rot).abs() > 1e-4
            })
    } else {
        studio.doc.find_shape(snap.layer, snap.id).is_some_and(|s| {
            snap.geom.as_ref().is_some_and(|g| *g != s.geom) || (s.rotation - snap.rot).abs() > 1e-4
        })
    }
}

fn object_commands(studio: &Studio, orig: Vec<ObjSnap>) -> Vec<crate::document::Cmd> {
    orig.into_iter()
        .filter(|snap| snap_changed(studio, snap))
        .filter_map(|snap| {
            if snap.id == RASTER_ID {
                let (origin, size, rotation) =
                    studio.doc.layers.get(snap.layer)?.kind.raster_xform()?;
                Some(crate::document::Cmd::SetRasterXform {
                    layer: snap.layer,
                    before: (snap.origin, snap.size, snap.rot),
                    after: (origin, size, rotation),
                })
            } else {
                let shape = studio.doc.find_shape(snap.layer, snap.id)?;
                Some(crate::document::Cmd::SetGeom {
                    layer: snap.layer,
                    id: snap.id,
                    before: snap.geom?,
                    after: shape.geom.clone(),
                    rot_before: snap.rot,
                    rot_after: shape.rotation,
                })
            }
        })
        .collect()
}

fn commit_canvas_commands(studio: &mut Studio, commands: Vec<crate::document::Cmd>) {
    if !commands.is_empty() {
        // The drag already updated the document. Store one reversible gesture.
        let mut commands = commands;
        commands.extend(studio.reconcile_layout());
        studio.history.push(crate::document::Cmd::Batch(commands));
        studio.dirty = true;
        studio.mark();
    }
}

fn commit_obj_snaps(studio: &mut Studio, orig: Vec<ObjSnap>) {
    let commands = object_commands(studio, orig);
    commit_canvas_commands(studio, commands);
}

/// Find a drop destination in painter order, ignoring the complete moving
/// subtrees so a dragged frame cannot intercept its own drop.
fn layout_drop_frame(studio: &Studio, world: Pt) -> Option<u64> {
    let mut excluded: std::collections::HashSet<_> = studio.selection.iter().copied().collect();
    for &(li, id) in &studio.selection {
        excluded.extend(
            crate::layout::descendants(&studio.doc, li, id)
                .into_iter()
                .map(|id| (li, id)),
        );
    }
    for (li, layer) in studio.doc.layers.iter().enumerate().rev() {
        if !studio.doc.layer_editable(li) {
            continue;
        }
        let Some(shapes) = layer.kind.shapes() else {
            continue;
        };
        let mut children: std::collections::HashMap<Option<u64>, Vec<usize>> =
            std::collections::HashMap::new();
        for (index, shape) in shapes.iter().enumerate() {
            children.entry(shape.layout.parent).or_default().push(index);
        }
        let mut pending: Vec<_> = children
            .get(&None)
            .into_iter()
            .flatten()
            .rev()
            .map(|&index| (index, world, 0usize))
            .collect();
        let mut seen = std::collections::HashSet::new();
        let mut best = None;
        let mut best_depth = 0;
        while let Some((index, point, depth)) = pending.pop() {
            if !seen.insert(index) {
                continue;
            }
            let shape = &shapes[index];
            if excluded.contains(&(li, shape.id))
                || !shape.visible
                || shape.locked
                || shape.guide
                || !shape.layout.frame
            {
                continue;
            }
            let contains = shape.contains_world(point);
            if contains && (best.is_none() || depth >= best_depth) {
                best = Some(shape.id);
                best_depth = depth;
            }
            if shape.layout.clip && !contains {
                continue;
            }
            let point = shape.local_point(point);
            if let Some(kids) = children.get(&Some(shape.id)) {
                pending.extend(kids.iter().rev().map(|&index| (index, point, depth + 1)));
            }
        }
        if best.is_some() {
            return best;
        }
    }
    None
}

fn commit_move(studio: &mut Studio, orig: Vec<ObjSnap>, world: Pt) {
    let commands = object_commands(studio, orig);
    if studio.persona == Persona::Layout {
        let parent = layout_drop_frame(studio, world);
        if studio.reparent_layout_selection_after(parent, commands.clone()) {
            return;
        }
    }
    commit_canvas_commands(studio, commands);
}

fn snapshot_moving(studio: &Studio) -> Vec<ObjSnap> {
    let mut ids = studio.selection.clone();
    for &(li, id) in &studio.selection {
        for child in crate::layout::descendants(&studio.doc, li, id) {
            let key = (li, child);
            if !ids.contains(&key) {
                ids.push(key);
            }
        }
    }
    snaps_for(studio, &ids)
}

fn snapshot_with_children(studio: &Studio) -> Vec<ObjSnap> {
    snapshot_moving(studio)
}

fn snaps_for(studio: &Studio, ids: &[(usize, u64)]) -> Vec<ObjSnap> {
    ids.iter()
        .filter_map(|(li, id)| {
            if *id == RASTER_ID {
                let layer = studio.doc.layers.get(*li)?;
                let (origin, size, rot) = layer.kind.raster_xform()?;
                Some(ObjSnap {
                    layer: *li,
                    id: RASTER_ID,
                    geom: None,
                    origin,
                    size,
                    rot,
                })
            } else {
                studio.doc.find_shape(*li, *id).map(|s| ObjSnap {
                    layer: *li,
                    id: *id,
                    geom: Some(s.geom.clone()),
                    origin: Pt::ZERO,
                    size: Pt::ZERO,
                    rot: s.rotation,
                })
            }
        })
        .collect()
}

enum HandleKind {
    Scale(usize, Bounds),
    Rotate(Pt),
}

fn posed_bounds(studio: &Studio) -> Option<Bounds> {
    let mut b: Option<Bounds> = None;
    for (li, id) in &studio.selection {
        let bb = if *id == RASTER_ID {
            studio
                .doc
                .layers
                .get(*li)
                .and_then(|l| l.kind.raster_bounds())
        } else {
            studio.doc.find_shape(*li, *id).map(|s| {
                if studio.is_motion() {
                    studio.live_pose(*id).map_bounds(s.world_bbox())
                } else {
                    s.world_bbox()
                }
            })
        };
        let Some(bb) = bb else {
            continue;
        };
        match &mut b {
            None => b = Some(bb),
            Some(acc) => {
                acc.union_pt(bb.min);
                acc.union_pt(bb.max);
            }
        }
    }
    b
}

fn hit_shape(studio: &Studio, world: Pt, slack: f32) -> Option<(usize, u64)> {
    if studio.is_motion() {
        crate::motion::hit_test(
            &studio.doc,
            studio.playhead,
            &studio.pose_drag,
            world,
            slack,
        )
    } else {
        studio.doc.hit_test(world, slack)
    }
}

fn hit_handle(studio: &Studio, world: Pt) -> Option<HandleKind> {
    let b = posed_bounds(studio)?;
    let slack = 8.0 / studio.view.scale.max(0.01);
    let rh = b.rotate_handle();
    if (rh - world).length() <= slack {
        return Some(HandleKind::Rotate(b.center()));
    }
    for i in 0..8 {
        if (b.handle(i) - world).length() <= slack {
            return Some(HandleKind::Scale(i, b));
        }
    }
    None
}

fn hit_node(anchors: &[Anchor], world: Pt, slack: f32, closed: bool) -> Option<NodeHit> {
    let handle_slack = slack * 1.75;
    for (i, a) in anchors.iter().enumerate() {
        // Ignore collapsed handles sitting on the node — those would steal Point.
        if a.h_in.length() > slack && (a.pt + a.h_in - world).length() <= handle_slack {
            return Some(NodeHit::HandleIn(i));
        }
        if a.h_out.length() > slack && (a.pt + a.h_out - world).length() <= handle_slack {
            return Some(NodeHit::HandleOut(i));
        }
        if (a.pt - world).length() <= slack {
            return Some(NodeHit::Point(i));
        }
    }
    if let Some((seg, _t, dist)) = crate::geom::closest_on_path(anchors, closed, world)
        && dist <= slack * 1.2
    {
        return Some(NodeHit::Segment(seg));
    }
    None
}

fn hit_edit_node(geom: &Geom, point: Pt, slack: f32) -> Option<NodeHit> {
    let mut offset = 0;
    let mut segment = None;
    for (anchors, closed) in geom.path_contours() {
        if let Some(hit) = hit_node(anchors, point, slack, closed) {
            match hit {
                NodeHit::Point(i) => return Some(NodeHit::Point(offset + i)),
                NodeHit::HandleIn(i) => return Some(NodeHit::HandleIn(offset + i)),
                NodeHit::HandleOut(i) => return Some(NodeHit::HandleOut(offset + i)),
                NodeHit::Segment(i) => {
                    segment = Some(NodeHit::Segment(offset + i));
                }
            }
        }
        offset += anchors.len();
    }
    segment
}

/// Grab a point or Bézier handle on pointer down, before egui decides it's a drag.
fn node_press(studio: &mut Studio, pick: Pt, snap: Pt, shift: bool, alt: bool) {
    let _ = (snap, alt);
    let Some((li, id)) = studio.primary() else {
        return;
    };
    if id == RASTER_ID {
        return;
    }
    studio.ensure_path(li, id);
    grab_corner(studio, pick, alt);
    if studio.op.is_some() {
        return;
    }
    let Some(shape) = studio.doc.find_shape(li, id) else {
        return;
    };
    if !matches!(shape.geom, Geom::Path { .. } | Geom::Paths { .. }) {
        return;
    }
    let slack = 8.0 / studio.view.scale.max(0.01);
    let Some(hit) = hit_edit_node(&shape.geom, shape.local_point(pick), slack) else {
        grab_corner(studio, pick, alt);
        return;
    };
    match hit {
        NodeHit::Point(i) => {
            if shift {
                if !studio.node_sel.remove(&i) {
                    studio.node_sel.insert(i);
                }
            } else if !studio.node_sel.contains(&i) {
                studio.node_sel.clear();
                studio.node_sel.insert(i);
            }
            let moving = if studio.node_sel.is_empty() {
                let mut s = BTreeSet::new();
                s.insert(i);
                s
            } else {
                studio.node_sel.clone()
            };
            studio.op = Some(Op::Node {
                layer: li,
                id,
                which: hit,
                orig: shape.geom.clone(),
                moving,
            });
        }
        NodeHit::HandleIn(_) | NodeHit::HandleOut(_) => {
            studio.op = Some(Op::Node {
                layer: li,
                id,
                which: hit,
                orig: shape.geom.clone(),
                moving: BTreeSet::new(),
            });
        }
        NodeHit::Segment(_) => grab_corner(studio, pick, alt),
    }
}

fn grab_corner(studio: &mut Studio, pick: Pt, alt: bool) {
    let Some((li, id, which)) = hit_corner(studio, pick) else {
        return;
    };
    let Some(s) = studio.doc.find_shape(li, id) else {
        return;
    };
    let radius = match &s.geom {
        Geom::Rect { radius, .. } => *radius,
        Geom::Path { .. } | Geom::Paths { .. } => which
            .and_then(|i| s.geom.anchors().nth(i))
            .map_or(0.0, |a| a.radius),
        _ => 0.0,
    };
    let moving = if alt {
        (0..s.geom.anchors().count()).collect()
    } else if which.is_some_and(|i| studio.node_sel.contains(&i)) {
        studio.node_sel.clone()
    } else {
        which.into_iter().collect()
    };
    studio.op = Some(Op::Corner {
        layer: li,
        id,
        which,
        moving,
        orig_corners: s.corners,
        orig_radius: radius,
        orig_geom: s.geom.clone(),
        start: pick,
    });
}

fn snapshot_on_artboards(studio: &Studio, ids: &[u64]) -> Vec<ObjSnap> {
    let mut out = Vec::new();
    let mut seen = BTreeSet::new();
    for id in ids {
        let Some(a) = studio.doc.artboards.iter().find(|x| x.id == *id) else {
            continue;
        };
        for snap in studio.snapshot_artboard_contents(a) {
            if seen.insert((snap.layer, snap.id)) {
                out.push(snap);
            }
        }
    }
    out
}

fn hit_artboard_handle(studio: &Studio, pick: Pt) -> Option<Op> {
    let slack = 12.0 / studio.view.scale.max(0.01);
    for id in &studio.artboard_sel {
        let Some(a) = studio.doc.artboards.iter().find(|x| x.id == *id) else {
            continue;
        };
        let c = a.center();
        let rh = a.rotate_handle_pt();
        if (rh - pick).length() <= slack {
            return Some(Op::ArtboardRotate {
                orig: a.clone(),
                center: c,
                start_angle: (pick - c).y.atan2((pick - c).x),
                contents: studio.snapshot_artboard_contents(a),
            });
        }
        let handles = a.handle_pts();
        for (i, handle) in handles.iter().enumerate() {
            if (*handle - pick).length() <= slack {
                return Some(Op::ArtboardResize {
                    orig: a.clone(),
                    handle: i,
                    start_box: a.local_bounds(),
                    contents: studio.snapshot_artboard_contents(a),
                });
            }
        }
    }
    None
}

fn artboard_press(studio: &mut Studio, pick: Pt, snap: Pt, shift: bool, alt: bool) {
    let _ = snap;
    if let Some(op) = hit_artboard_handle(studio, pick) {
        studio.op = Some(op);
        return;
    }
    let slack = 10.0 / studio.view.scale.max(0.01);
    if let Some(id) = studio.doc.artboard_hit(pick, slack) {
        if shift {
            if let Some(i) = studio.artboard_sel.iter().position(|x| *x == id) {
                studio.artboard_sel.remove(i);
            } else {
                studio.artboard_sel.push(id);
            }
        } else if !studio.artboard_sel.contains(&id) {
            studio.artboard_sel = vec![id];
        }
        if alt {
            studio.clone_artboard(id);
        }
        if let Some(op) = hit_artboard_handle(studio, pick) {
            studio.op = Some(op);
        }
        return;
    }
    if !shift {
        studio.artboard_sel.clear();
    }
}

fn hit_corner(studio: &Studio, world: Pt) -> Option<(usize, u64, Option<usize>)> {
    if studio.tool != Tool::Node || studio.selection.len() != 1 {
        return None;
    }
    let (li, id) = studio.selection[0];
    let s = studio.doc.find_shape(li, id)?;
    let slack = 8.0 / studio.view.scale.max(0.01);
    match &s.geom {
        Geom::Rect { origin, size, .. } => {
            let widgets =
                crate::geom::corner_widgets(*origin, *size).map(|point| s.world_point(point));
            if studio.tool == Tool::Select {
                for w in widgets {
                    if (w - world).length() <= slack {
                        return Some((li, id, None));
                    }
                }
            } else if studio.tool == Tool::Node {
                for (i, w) in widgets.iter().enumerate() {
                    if (*w - world).length() <= slack {
                        return Some((li, id, Some(i)));
                    }
                }
            }
            None
        }
        Geom::Path { .. } | Geom::Paths { .. } if studio.tool == Tool::Node => {
            for (i, a) in s.geom.anchors().enumerate() {
                {
                    let dir = Pt::new(14.0, 14.0) / studio.view.scale.max(0.01);
                    if (s.world_point(a.pt) + dir - world).length()
                        <= 5.0 / studio.view.scale.max(0.01)
                    {
                        return Some((li, id, Some(i)));
                    }
                }
            }
            None
        }
        _ => None,
    }
}

fn continue_drag(studio: &mut Studio, world: Pt, shift: bool, alt: bool) {
    let world = studio.precise_drag(world, shift);
    if matches!(
        studio.op,
        Some(
            Op::Move { .. }
                | Op::Resize { .. }
                | Op::Rotate { .. }
                | Op::Node { .. }
                | Op::Corner { .. }
                | Op::ArtboardMove { .. }
                | Op::ArtboardResize { .. }
                | Op::ArtboardRotate { .. }
                | Op::Brush { .. }
                | Op::Smudge { .. }
                | Op::Clone { .. }
                | Op::Retouch { .. }
        )
    ) {
        studio.mark();
    }
    match &mut studio.op {
        Some(Op::Create {
            start,
            cur,
            kind,
            from_center,
        }) => {
            *from_center = alt;
            let mut c = world;
            if shift && !matches!(kind, CreateKind::Line) {
                let dx = (c.x - start.x).abs();
                let dy = (c.y - start.y).abs();
                let s = dx.max(dy);
                c.x = start.x + s.copysign(c.x - start.x);
                c.y = start.y + s.copysign(c.y - start.y);
            }
            if shift && matches!(kind, CreateKind::Line) {
                let d = c - *start;
                let ang = d.y.atan2(d.x);
                let snapped =
                    (ang / std::f32::consts::FRAC_PI_4).round() * std::f32::consts::FRAC_PI_4;
                let len = d.length();
                c = *start + Pt::new(len * snapped.cos(), len * snapped.sin());
            }
            *cur = c;
        }
        Some(Op::Pen { anchors, press, .. }) => {
            if let Some(last) = anchors.last_mut() {
                crate::geom::apply_pen_smooth(last, world - *press, studio.view.scale, alt, shift);
            }
        }
        Some(Op::Pencil { pts }) => {
            if pts
                .last()
                .map(|p| (*p - world).length() > 1.5)
                .unwrap_or(true)
            {
                pts.push(world);
            }
        }
        Some(Op::Move { orig, start, .. }) => {
            let d = world - *start;
            if studio.persona == Persona::Motion {
                studio.playing = false;
                for snap in orig.clone() {
                    if snap.id == RASTER_ID {
                        continue;
                    }
                    let mut pose = studio.doc.motion.pose(snap.id, studio.playhead);
                    pose.dx += d.x;
                    pose.dy += d.y;
                    studio.pose_drag.insert(snap.id, pose);
                }
            } else {
                let moving: std::collections::HashSet<_> =
                    orig.iter().map(|s| (s.layer, s.id)).collect();
                for snap in orig.clone() {
                    if snap.id == RASTER_ID {
                        if let Some(l) = studio.doc.layers.get_mut(snap.layer) {
                            l.kind
                                .set_raster_xform(snap.origin + d, snap.size, snap.rot);
                        }
                    } else {
                        // All descendants of a moving frame share its external
                        // coordinate space. Rotations inside that moving subtree
                        // move with it and must not be applied to the delta twice.
                        let mut angle = 0.0;
                        let mut parent = studio
                            .doc
                            .find_shape(snap.layer, snap.id)
                            .and_then(|s| s.layout.parent);
                        let mut seen = std::collections::HashSet::new();
                        while let Some(id) = parent {
                            if !seen.insert(id) || seen.len() > 64 {
                                break;
                            }
                            let Some(frame) = studio.doc.find_shape(snap.layer, id) else {
                                break;
                            };
                            if !moving.contains(&(snap.layer, id)) {
                                angle += frame.rotation;
                            }
                            parent = frame.layout.parent;
                        }
                        if let (Some(geom), Some(s)) = (
                            snap.geom.clone(),
                            studio.doc.find_shape_mut(snap.layer, snap.id),
                        ) {
                            s.geom = geom;
                            s.geom.translate(d.rotate(-angle));
                            s.rotation = snap.rot;
                        }
                    }
                }
            }
        }
        Some(Op::Resize {
            orig,
            handle,
            start_box,
        }) => {
            let (hx, hy) = match *handle {
                0 => (0.0, 0.0),
                1 => (1.0, 0.0),
                2 => (1.0, 1.0),
                3 => (0.0, 1.0),
                4 => (0.5, 0.0),
                5 => (1.0, 0.5),
                6 => (0.5, 1.0),
                _ => (0.0, 0.5),
            };
            let mut min = start_box.min;
            let mut max = start_box.max;
            if alt {
                // Scale about centre.
                let c = start_box.center();
                let hw = if hx == 0.5 {
                    start_box.width() * 0.5
                } else {
                    (world.x - c.x).abs().max(0.5)
                };
                let hh = if hy == 0.5 {
                    start_box.height() * 0.5
                } else {
                    (world.y - c.y).abs().max(0.5)
                };
                let mut dst_w = hw * 2.0;
                let mut dst_h = hh * 2.0;
                if shift && hx != 0.5 && hy != 0.5 {
                    // Uniform about centre: keep aspect.
                    let asp = start_box.width().max(1.0) / start_box.height().max(1.0);
                    if dst_w / asp > dst_h {
                        dst_w = dst_h * asp;
                    } else {
                        dst_h = dst_w / asp;
                    }
                }
                min = Pt::new(c.x - dst_w * 0.5, c.y - dst_h * 0.5);
                max = Pt::new(c.x + dst_w * 0.5, c.y + dst_h * 0.5);
            } else {
                if hx == 0.0 {
                    min.x = world.x;
                } else if hx == 1.0 {
                    max.x = world.x;
                }
                if hy == 0.0 {
                    min.y = world.y;
                } else if hy == 1.0 {
                    max.y = world.y;
                }
                if shift && hx != 0.5 && hy != 0.5 {
                    // Uniform corner drag – preserve aspect ratio.
                    let asp = start_box.width().max(1.0) / start_box.height().max(1.0);
                    let w = (max.x - min.x).abs().max(1.0);
                    let h = (max.y - min.y).abs().max(1.0);
                    // Decide which axis drives scaling – the larger relative change.
                    let use_w = w / start_box.width() > h / start_box.height();
                    if use_w {
                        let nh = w / asp;
                        if hy == 0.0 {
                            min.y = max.y - nh;
                        } else {
                            max.y = min.y + nh;
                        }
                    } else {
                        let nw = h * asp;
                        if hx == 0.0 {
                            min.x = max.x - nw;
                        } else {
                            max.x = min.x + nw;
                        }
                    }
                }
                // Ensure valid size and handle inversion gracefully.
                if max.x - min.x < 1.0 {
                    if hx == 0.0 {
                        min.x = max.x - 1.0;
                    } else {
                        max.x = min.x + 1.0;
                    }
                }
                if max.y - min.y < 1.0 {
                    if hy == 0.0 {
                        min.y = max.y - 1.0;
                    } else {
                        max.y = min.y + 1.0;
                    }
                }
                // If the drag inverted (min > max), swap.
                if min.x > max.x {
                    std::mem::swap(&mut min.x, &mut max.x);
                }
                if min.y > max.y {
                    std::mem::swap(&mut min.y, &mut max.y);
                }
            }
            let dst = Bounds { min, max };
            if studio.persona == Persona::Motion {
                studio.playing = false;
                let sx = dst.width() / start_box.width().max(1.0);
                let sy = dst.height() / start_box.height().max(1.0);
                let f = if shift { sx.max(sy) } else { (sx + sy) * 0.5 };
                for snap in orig.clone() {
                    if snap.id == RASTER_ID {
                        continue;
                    }
                    let mut pose = studio.doc.motion.pose(snap.id, studio.playhead);
                    pose.scale = (pose.scale * f).clamp(0.05, 32.0);
                    studio.pose_drag.insert(snap.id, pose);
                }
            } else {
                for snap in orig.clone() {
                    if snap.id == RASTER_ID {
                        let nb = Bounds {
                            min: start_box.map_pt(snap.origin, dst),
                            max: start_box.map_pt(snap.origin + snap.size, dst),
                        };
                        if let Some(l) = studio.doc.layers.get_mut(snap.layer) {
                            l.kind.set_raster_xform(
                                nb.min,
                                Pt::new(nb.width().max(1.0), nb.height().max(1.0)),
                                snap.rot,
                            );
                        }
                    } else if let (Some(geom), Some(s)) = (
                        snap.geom.clone(),
                        studio.doc.find_shape_mut(snap.layer, snap.id),
                    ) {
                        s.geom = geom;
                        s.geom.map_into(*start_box, dst);
                        s.rotation = snap.rot;
                    }
                }
                let orig_geoms: Vec<_> = orig
                    .iter()
                    .filter_map(|s| s.geom.clone().map(|g| (s.layer, s.id, g)))
                    .collect();
                let changed: Vec<_> = orig.iter().map(|s| (s.layer, s.id)).collect();
                crate::layout::apply_resize(&mut studio.doc, &orig_geoms, &changed);
            }
        }
        Some(Op::Rotate {
            orig,
            center,
            start_angle,
        }) => {
            let mut ang = (world - *center).y.atan2((world - *center).x) - *start_angle;
            if shift {
                let step = std::f32::consts::PI / 12.0; // 15°
                ang = (ang / step).round() * step;
            }
            if studio.persona == Persona::Motion {
                studio.playing = false;
                for snap in orig.clone() {
                    if snap.id == RASTER_ID {
                        continue;
                    }
                    let mut pose = studio.doc.motion.pose(snap.id, studio.playhead);
                    pose.rotation += ang;
                    studio.pose_drag.insert(snap.id, pose);
                }
            } else {
                for snap in orig.clone() {
                    if snap.id == RASTER_ID {
                        let c0 = snap.origin + snap.size * 0.5;
                        let c1 = c0.rotate_about(*center, ang);
                        if let Some(l) = studio.doc.layers.get_mut(snap.layer) {
                            l.kind.set_raster_xform(
                                snap.origin + (c1 - c0),
                                snap.size,
                                snap.rot + ang,
                            );
                        }
                    } else if let (Some(geom), Some(s)) = (
                        snap.geom.clone(),
                        studio.doc.find_shape_mut(snap.layer, snap.id),
                    ) {
                        let original_center = geom.bbox().center();
                        s.geom = geom;
                        s.geom.translate(
                            original_center.rotate_about(*center, ang) - original_center,
                        );
                        s.rotation = snap.rot + ang;
                    }
                }
            }
        }
        Some(Op::Node {
            layer,
            id,
            which,
            orig,
            moving,
        }) => {
            let layer = *layer;
            let id = *id;
            let which = *which;
            let orig = orig.clone();
            let moving = moving.clone();
            if let Some(s) = studio.doc.find_shape_mut(layer, id) {
                let center = orig.bbox().center();
                let rotation = s.rotation;
                let local = world.rotate_about(center, -rotation);
                s.geom = orig;
                {
                    match which {
                        NodeHit::Point(i) => {
                            let Some(start) = s.geom.anchors().nth(i).map(|a| a.pt) else {
                                return;
                            };
                            let d = local - start;
                            for idx in moving {
                                if let Some(a) = s.geom.anchor_mut(idx) {
                                    a.pt += d;
                                }
                            }
                        }
                        NodeHit::Segment(seg) => {
                            let Some(start) = s.geom.anchors().nth(seg).map(|a| a.pt) else {
                                return;
                            };
                            let d = local - start;
                            for idx in moving {
                                if let Some(a) = s.geom.anchor_mut(idx) {
                                    a.pt += d;
                                }
                            }
                        }
                        NodeHit::HandleIn(i) => {
                            if let Some(a) = s.geom.anchor_mut(i) {
                                let mut h = world - a.pt.rotate_about(center, rotation);
                                if shift {
                                    h = crate::geom::constrain_45(h);
                                }
                                h = h.rotate(-rotation);
                                a.h_in = h;
                                if !alt {
                                    a.h_out = -a.h_in;
                                }
                            }
                        }
                        NodeHit::HandleOut(i) => {
                            if let Some(a) = s.geom.anchor_mut(i) {
                                let mut h = world - a.pt.rotate_about(center, rotation);
                                if shift {
                                    h = crate::geom::constrain_45(h);
                                }
                                h = h.rotate(-rotation);
                                a.h_out = h;
                                if !alt {
                                    a.h_in = -a.h_out;
                                }
                            }
                        }
                    }
                }
                s.geom.preserve_rotation_pivot(center, rotation);
            }
        }
        Some(Op::Brush { .. }) => super::retouch::brush_drag(studio, world),
        Some(Op::Retouch { .. }) => super::retouch::drag(studio, world),
        Some(Op::Smudge { .. }) => super::retouch::smudge_drag(studio, world),
        Some(Op::Clone { .. }) => super::retouch::clone_drag(studio, world),
        Some(Op::Marquee { cur, .. }) => *cur = world,
        Some(Op::Lasso { pts }) => {
            if pts
                .last()
                .map(|point| (*point - world).length() > 1.5)
                .unwrap_or(true)
            {
                pts.push(world);
            }
        }
        Some(Op::Gradient { cur, .. }) => *cur = world,
        Some(Op::CropPhoto { cur, .. }) => *cur = world,
        Some(Op::ZoomBox { cur, .. }) => *cur = world,
        Some(Op::Place { cur, .. }) => *cur = world,
        Some(Op::NodeMarquee { cur, .. }) => *cur = world,
        Some(Op::ArtboardDraw { cur, .. }) => *cur = world,
        Some(Op::Corner {
            layer,
            id,
            which,
            moving,
            orig_corners,
            orig_radius,
            orig_geom,
            start,
        }) => {
            let layer = *layer;
            let id = *id;
            let which = *which;
            let moving = moving.clone();
            let orig_corners = *orig_corners;
            let orig_radius = *orig_radius;
            let orig_geom = orig_geom.clone();
            let start = *start;
            if let Some(s) = studio.doc.find_shape_mut(layer, id) {
                s.geom = orig_geom;
                let dist = (world - start).length();
                let sign = if (world - start).dot(s.geom.bbox().center() - start) < 0.0 {
                    -1.0
                } else {
                    1.0
                };
                let r = (orig_radius + dist * 0.5 * sign).max(0.0);
                match which {
                    None => {
                        s.corners = [0.0; 4];
                        if let Geom::Rect { radius, .. } = &mut s.geom {
                            *radius = r;
                        }
                    }
                    Some(i) => {
                        if matches!(s.geom, Geom::Path { .. } | Geom::Paths { .. }) {
                            let targets = if alt {
                                (0..s.geom.anchors().count()).collect()
                            } else {
                                moving
                            };
                            for index in targets {
                                if let Some(a) = s.geom.anchor_mut(index) {
                                    a.radius = r;
                                }
                            }
                        } else {
                            let mut c = orig_corners;
                            if c.iter().all(|x| *x < 0.05) {
                                c = [orig_radius; 4];
                            }
                            if i < 4 {
                                c[i] = r;
                            }
                            s.corners = c;
                            if let Geom::Rect { radius, .. } = &mut s.geom {
                                *radius = c.iter().copied().fold(0.0, f32::max);
                            }
                        }
                    }
                }
            }
        }
        Some(Op::ArtboardMove {
            orig,
            ids,
            start,
            contents,
        }) => {
            let d = world - *start;
            let orig_boards = orig.clone();
            let ids = ids.clone();
            let contents = contents.clone();
            studio.doc.artboards = orig_boards;
            for a in &mut studio.doc.artboards {
                if ids.contains(&a.id) {
                    a.origin += d;
                }
            }
            for snap in &contents {
                if snap.id == RASTER_ID {
                    if let Some(l) = studio.doc.layers.get_mut(snap.layer) {
                        l.kind
                            .set_raster_xform(snap.origin + d, snap.size, snap.rot);
                    }
                } else if let (Some(geom), Some(s)) = (
                    snap.geom.clone(),
                    studio.doc.find_shape_mut(snap.layer, snap.id),
                ) {
                    s.geom = geom;
                    s.geom.translate(d);
                    s.rotation = snap.rot;
                }
            }
        }
        Some(Op::ArtboardResize {
            orig,
            handle,
            start_box,
            contents,
        }) => {
            let orig = orig.clone();
            let handle = *handle;
            let start_box = *start_box;
            let contents = contents.clone();
            let c0 = start_box.center();
            let local = if orig.rotation.abs() > 1e-5 {
                world.rotate_about(c0, -orig.rotation)
            } else {
                world
            };
            let (hx, hy) = match handle {
                0 => (0.0, 0.0),
                1 => (1.0, 0.0),
                2 => (1.0, 1.0),
                3 => (0.0, 1.0),
                4 => (0.5, 0.0),
                5 => (1.0, 0.5),
                6 => (0.5, 1.0),
                _ => (0.0, 0.5),
            };
            let mut min = start_box.min;
            let mut max = start_box.max;
            if hx == 0.0 {
                min.x = local.x;
            } else if hx == 1.0 {
                max.x = local.x;
            }
            if hy == 0.0 {
                min.y = local.y;
            } else if hy == 1.0 {
                max.y = local.y;
            }
            if min.x > max.x {
                std::mem::swap(&mut min.x, &mut max.x);
            }
            if min.y > max.y {
                std::mem::swap(&mut min.y, &mut max.y);
            }
            if let Some(a) = studio.doc.artboards.iter_mut().find(|a| a.id == orig.id) {
                a.origin = min;
                a.size = Pt::new((max.x - min.x).max(8.0), (max.y - min.y).max(8.0));
                a.rotation = orig.rotation;
            }
            if let Some(neu) = studio
                .doc
                .artboards
                .iter()
                .find(|a| a.id == orig.id)
                .cloned()
            {
                studio.apply_artboard_contents(&orig, &neu, &contents);
            }
        }
        Some(Op::ArtboardRotate {
            orig,
            center,
            start_angle,
            contents,
        }) => {
            let ang = (world - *center).y.atan2((world - *center).x) - *start_angle;
            let orig = orig.clone();
            let contents = contents.clone();
            if let Some(a) = studio.doc.artboards.iter_mut().find(|a| a.id == orig.id) {
                a.origin = orig.origin;
                a.size = orig.size;
                a.rotation = crate::document::Artboard::snap_rotation(orig.rotation + ang);
            }
            if let Some(neu) = studio
                .doc
                .artboards
                .iter()
                .find(|a| a.id == orig.id)
                .cloned()
            {
                studio.apply_artboard_contents(&orig, &neu, &contents);
            }
        }
        None => {}
    }
    studio.sync_pen_source();
}

fn end_drag(studio: &mut Studio, world: Pt, alt: bool, ctrl: bool, shift: bool) {
    studio.reset_snap_gesture();
    match studio.op.take() {
        Some(Op::Create {
            kind,
            start,
            cur,
            from_center,
        }) => studio.finish_create(
            kind,
            if from_center {
                start - (cur - start)
            } else {
                start
            },
            cur,
        ),
        Some(Op::Pencil { pts }) => studio.finish_pencil(pts),
        Some(op @ Op::Pen { .. }) => {
            studio.op = Some(op);
        }
        Some(Op::Move { orig, .. }) => {
            if studio.is_motion() {
                commit_pose_drag(studio);
            } else {
                if orig.iter().any(|item| snap_changed(studio, item)) {
                    commit_move(studio, orig, world);
                }
            }
        }
        Some(Op::Resize { orig, .. }) | Some(Op::Rotate { orig, .. }) => {
            if studio.is_motion() {
                commit_pose_drag(studio);
            } else if orig.iter().any(|s| snap_changed(studio, s)) {
                commit_obj_snaps(studio, orig);
            }
        }
        Some(Op::Node {
            layer, id, orig, ..
        }) => {
            if let Some(s) = studio.doc.find_shape(layer, id)
                && s.geom != orig
            {
                studio.history.push(crate::document::Cmd::SetGeom {
                    layer,
                    id,
                    before: orig,
                    after: s.geom.clone(),
                    rot_before: s.rotation,
                    rot_after: s.rotation,
                });
                studio.dirty = true;
            }
        }
        Some(Op::NodeMarquee { start, cur }) => {
            let r = Bounds {
                min: Pt::new(start.x.min(cur.x), start.y.min(cur.y)),
                max: Pt::new(start.x.max(cur.x), start.y.max(cur.y)),
            };
            if let Some((li, id)) = studio.primary()
                && let Some(s) = studio.doc.find_shape(li, id)
                && let Some(anchors) = s.world_anchors()
            {
                if !shift {
                    studio.node_sel.clear();
                }
                for (i, a) in anchors.iter().enumerate() {
                    if r.contains(a.pt) {
                        studio.node_sel.insert(i);
                    }
                }
            }
        }
        Some(Op::Corner {
            layer,
            id,
            orig_corners,
            orig_radius,
            orig_geom,
            ..
        }) => {
            if let Some(s) = studio.doc.find_shape(layer, id) {
                let radius_after = match s.geom {
                    Geom::Rect { radius, .. } => radius,
                    _ => orig_radius,
                };
                let corners_changed =
                    s.corners != orig_corners || (radius_after - orig_radius).abs() > 0.05;
                let geom_changed = s.geom != orig_geom;
                if corners_changed {
                    studio.history.push(crate::document::Cmd::SetCorners {
                        layer,
                        id,
                        before: orig_corners,
                        after: s.corners,
                        radius_before: orig_radius,
                        radius_after,
                    });
                    studio.dirty = true;
                }
                if geom_changed {
                    studio.history.push(crate::document::Cmd::SetGeom {
                        layer,
                        id,
                        before: orig_geom,
                        after: s.geom.clone(),
                        rot_before: s.rotation,
                        rot_after: s.rotation,
                    });
                    studio.dirty = true;
                }
            }
        }
        Some(Op::ArtboardDraw { start, cur }) => {
            let min = Pt::new(start.x.min(cur.x), start.y.min(cur.y));
            let max = Pt::new(start.x.max(cur.x), start.y.max(cur.y));
            let size = max - min;
            if size.x.abs() > 8.0 && size.y.abs() > 8.0 {
                let mut board = crate::document::Artboard::new(
                    studio.doc.artboards.len(),
                    min,
                    Pt::new(size.x.abs(), size.y.abs()),
                );
                board.name = studio.doc.unique_artboard_name(&board.name);
                let id = board.id;
                let mut after = studio.doc.artboards.clone();
                after.push(board);
                studio.commit_artboards(after);
                studio.artboard_sel = vec![id];
            }
        }
        Some(Op::ArtboardMove { orig, contents, .. }) => {
            let mut commands = object_commands(studio, contents);
            if orig != studio.doc.artboards {
                commands.insert(
                    0,
                    crate::document::Cmd::SetArtboards {
                        before: orig,
                        after: studio.doc.artboards.clone(),
                    },
                );
            }
            commit_canvas_commands(studio, commands);
        }
        Some(Op::ArtboardResize { orig, contents, .. })
        | Some(Op::ArtboardRotate { orig, contents, .. }) => {
            let mut commands = object_commands(studio, contents);
            let mut before = studio.doc.artboards.clone();
            if let Some(slot) = before.iter_mut().find(|board| board.id == orig.id) {
                *slot = orig;
            }
            if before != studio.doc.artboards {
                commands.insert(
                    0,
                    crate::document::Cmd::SetArtboards {
                        before,
                        after: studio.doc.artboards.clone(),
                    },
                );
            }
            commit_canvas_commands(studio, commands);
        }
        Some(Op::Brush {
            layer, buf, before, ..
        }) => {
            if let Some(px) = studio
                .doc
                .layers
                .get_mut(layer)
                .and_then(|l| l.kind.pixels_mut())
                && let Some(mut pm) = px.to_pixmap()
            {
                paint::blend_onto(&mut pm, &buf, studio.brush.opacity);
                *px = crate::document::Pixels::from_pixmap(&pm);
                let after = px.data.clone();
                studio.history.push(crate::document::Cmd::Pixels {
                    layer,
                    mask: false,
                    before,
                    after,
                });
                studio.dirty = true;
            }
        }
        Some(Op::Retouch {
            layer,
            mask,
            before,
            ..
        }) => super::retouch::finish(studio, layer, mask, before),
        Some(Op::Smudge { layer, before, .. }) | Some(Op::Clone { layer, before, .. }) => {
            if let Some(px) = studio.doc.layers.get(layer).and_then(|l| l.kind.pixels()) {
                studio.history.push(crate::document::Cmd::Pixels {
                    layer,
                    mask: false,
                    before,
                    after: px.data.clone(),
                });
                studio.dirty = true;
            }
        }
        Some(Op::Marquee {
            start,
            cur,
            ellipse,
        }) => {
            if studio.tool == Tool::Select {
                let b = Bounds {
                    min: Pt::new(start.x.min(cur.x), start.y.min(cur.y)),
                    max: Pt::new(start.x.max(cur.x), start.y.max(cur.y)),
                };
                if b.width() > 4.0 && b.height() > 4.0 {
                    let hits = if studio.is_motion() {
                        crate::motion::hits_in_rect(
                            &studio.doc,
                            studio.playhead,
                            &studio.pose_drag,
                            b,
                        )
                    } else {
                        studio.doc.hits_in_rect(b)
                    };
                    // Pointer-down already cleared the selection unless Shift
                    // was held. Keep that base even after a quick Shift release.
                    for hit in hits {
                        if !studio.selection.contains(&hit) {
                            studio.selection.push(hit);
                        }
                    }
                }
            } else {
                super::retouch::commit_marquee(
                    studio,
                    start,
                    cur,
                    ellipse,
                    super::retouch::pixel_op(shift, alt),
                );
            }
        }
        Some(Op::Lasso { pts }) => {
            super::retouch::commit_lasso(studio, &pts, super::retouch::pixel_op(shift, alt));
        }
        Some(Op::Gradient { start, cur }) => {
            if (cur - start).length() < 2.0 {
                // click without a drag is not a gradient
            } else if let Some((li, id)) = studio.primary()
                && let Some(s) = studio.doc.find_shape(li, id)
            {
                let b = s.geom.bbox();
                let start = s.local_point(start);
                let cur = s.local_point(cur);
                let from = [
                    (start.x - b.min.x) / b.width().max(1.),
                    (start.y - b.min.y) / b.height().max(1.),
                ];
                let to = [
                    (cur.x - b.min.x) / b.width().max(1.),
                    (cur.y - b.min.y) / b.height().max(1.),
                ];
                let mut after = s.style.clone();
                let mut gradient = if studio.fill_active {
                    after.fill.gradient()
                } else {
                    after.stroke.as_ref().and_then(|s| s.gradient.clone())
                }
                .unwrap_or_else(|| {
                    crate::gradient::Gradient::new(
                        crate::gradient::GradientKind::Linear,
                        studio.gradient.0,
                        studio.gradient.1,
                    )
                });
                gradient.from = from;
                gradient.to = to;
                if studio.fill_active {
                    after.fill = Fill::Gradient(gradient);
                } else {
                    after.stroke.get_or_insert_with(Default::default).gradient = Some(gradient);
                }
                studio.commit(crate::document::Cmd::SetStyle {
                    layer: li,
                    id,
                    before: s.style.clone(),
                    after,
                });
            }
        }
        Some(Op::CropPhoto { start, cur }) => studio.commit_photo_crop(start, cur),
        Some(Op::ZoomBox { start, cur }) => {
            studio.finish_zoom_box_mods(start, cur, alt, ctrl, shift)
        }
        Some(Op::Place { start, cur }) => studio.commit_place_rect(start, cur),
        None => {}
    }
}

fn node_click(studio: &mut Studio, world: Pt, alt: bool) {
    let slack = 8. / studio.view.scale.max(0.01);
    if let Some((li, id)) = studio.primary() {
        if id != RASTER_ID {
            studio.ensure_path(li, id);
        }
        if let Some(shape) = studio.doc.find_shape(li, id) {
            let orig = shape.geom.clone();
            let rot = shape.rotation;
            let point = shape.local_point(world);
            if let Some(hit) = hit_edit_node(&orig, point, slack) {
                let mut after = orig.clone();
                match hit {
                    NodeHit::Point(i) if alt => {
                        if let Some(a) = after.anchor_mut(i) {
                            if a.is_corner() {
                                a.make_smooth();
                            } else {
                                a.make_corner();
                            }
                        }
                    }
                    NodeHit::Segment(i) => {
                        if let Some((anchors, closed, offset)) = after.path_at_mut(i) {
                            if let Some(index) = insert_anchor(anchors, *closed, point, slack * 2.)
                            {
                                studio.node_sel = BTreeSet::from([offset + index]);
                            }
                        }
                    }
                    _ => return,
                }
                if after != orig {
                    after.preserve_rotation_pivot(orig.bbox().center(), rot);
                    studio.commit(crate::document::Cmd::SetGeom {
                        layer: li,
                        id,
                        before: orig,
                        after,
                        rot_before: rot,
                        rot_after: rot,
                    });
                }
                return;
            }
        }
    }
    if let Some(hit) = hit_shape(studio, world, slack) {
        studio.selection = vec![hit];
        studio.node_sel.clear();
        if hit.1 != RASTER_ID {
            studio.ensure_path(hit.0, hit.1);
        }
    }
}

fn draw_snap_feedback(painter: &eframe::egui::Painter, rect: Rect, studio: &Studio) {
    let screen = |point: Pt| {
        let point = studio.view.to_screen(point);
        rect.min + vec2(point.x, point.y)
    };
    let color = accent();
    for &(from, to) in &studio.snap_feedback.lines {
        painter.line_segment([screen(from), screen(to)], Stroke::new(1.0, color));
    }
    for &(from, to) in &studio.snap_feedback.gaps {
        let (a, b) = (screen(from), screen(to));
        painter.line_segment([a, b], Stroke::new(1.0, color));
        let normal = if (b.x - a.x).abs() > (b.y - a.y).abs() {
            vec2(0.0, 3.0)
        } else {
            vec2(3.0, 0.0)
        };
        for point in [a, b] {
            painter.line_segment([point - normal, point + normal], Stroke::new(1.0, color));
        }
        let middle = a + (b - a) * 0.5;
        painter.text(
            middle - vec2(0.0, 6.0),
            eframe::egui::Align2::CENTER_BOTTOM,
            format!(
                "{:.1} {}",
                (to - from).length() / studio.doc.ruler.unit.pixels_per_unit(studio.doc.dpi),
                studio.doc.ruler.unit.label()
            ),
            eframe::egui::FontId::proportional(10.0),
            color,
        );
    }
}

fn draw_grid(p: &eframe::egui::Painter, rect: Rect, studio: &Studio) {
    let g = studio.doc.grid.size.max(1.0);
    let col = crate::ui::theme::accent_soft();
    let origin = win(rect, studio.view, Pt::ZERO);
    let base_step = g * studio.view.scale;
    let step = base_step * (8.0 / base_step).ceil().max(1.0);
    let mut x = rect.min.x + (origin.x - rect.min.x).rem_euclid(step);
    while x < rect.max.x {
        p.line_segment(
            [pos2(x, rect.min.y), pos2(x, rect.max.y)],
            Stroke::new(1.0, col),
        );
        x += step;
    }
    let mut y = rect.min.y + (origin.y - rect.min.y).rem_euclid(step);
    while y < rect.max.y {
        p.line_segment(
            [pos2(rect.min.x, y), pos2(rect.max.x, y)],
            Stroke::new(1.0, col),
        );
        y += step;
    }
}

fn draw_layout_frames(p: &eframe::egui::Painter, rect: Rect, studio: &Studio) {
    if studio.persona != Persona::Layout && studio.tool != Tool::Frame {
        return;
    }
    for (li, layer) in studio.doc.layers.iter().enumerate() {
        let Some(shapes) = layer.kind.shapes() else {
            continue;
        };
        for shape in shapes {
            if !shape.layout.frame || !shape.visible {
                continue;
            }
            let on = studio.selection.contains(&(li, shape.id));
            if shape.layout.parent.is_some() && !on {
                continue;
            }
            let b = shape.world_bbox();
            let a = win(rect, studio.view, b.min);
            let c = win(rect, studio.view, b.max);
            p.rect_stroke(
                Rect::from_two_pos(a, c),
                0.0,
                Stroke::new(
                    if on { 1.6 } else { 1.0 },
                    if on {
                        accent()
                    } else {
                        Color32::from_rgb(140, 150, 165)
                    },
                ),
                eframe::egui::StrokeKind::Outside,
            );
            p.text(
                a + vec2(2.0, -5.0),
                eframe::egui::Align2::LEFT_BOTTOM,
                &shape.name,
                eframe::egui::FontId::monospace(10.0),
                fg_weak(),
            );
        }
    }
}

fn draw_comment_pins(p: &eframe::egui::Painter, rect: Rect, studio: &Studio) {
    for (index, pin) in studio.doc.comments.iter().enumerate() {
        let pos = win(rect, studio.view, pin.pos);
        let color = if pin.resolved {
            Color32::from_rgb(110, 180, 120)
        } else {
            Color32::from_rgb(232, 106, 60)
        };
        p.circle_filled(pos, 8.0, color);
        p.text(
            pos,
            eframe::egui::Align2::CENTER_CENTER,
            format!("{}", index + 1),
            eframe::egui::FontId::monospace(10.0),
            Color32::WHITE,
        );
    }
}

fn draw_artboard_frames(p: &eframe::egui::Painter, rect: Rect, studio: &Studio) {
    let doc = &studio.doc;
    if doc.artboards.is_empty() {
        return;
    }
    for a in &doc.artboards {
        let corners = a.corners();
        let scr: [Pos2; 4] = corners.map(|q| win(rect, studio.view, q));
        let on = studio.artboard_sel.contains(&a.id);
        let stroke = Stroke::new(
            if on { 2.0 } else { 1.0 },
            if on {
                accent()
            } else {
                Color32::from_rgb(120, 130, 150)
            },
        );
        for i in 0..4 {
            p.line_segment([scr[i], scr[(i + 1) % 4]], stroke);
        }
        p.text(
            scr[0] + vec2(4.0, 2.0),
            eframe::egui::Align2::LEFT_TOP,
            &a.name,
            eframe::egui::FontId::monospace(10.0),
            fg_weak(),
        );
        if on && studio.tool == Tool::Artboard {
            for h in a.handle_pts() {
                let sp = win(rect, studio.view, h);
                p.rect_filled(Rect::from_center_size(sp, vec2(9.0, 9.0)), 0.0, select());
            }
            let top = (corners[0] + corners[1]) * 0.5;
            let rh = a.rotate_handle_pt();
            p.line_segment(
                [win(rect, studio.view, top), win(rect, studio.view, rh)],
                Stroke::new(1.0, accent()),
            );
            p.circle_filled(win(rect, studio.view, rh), 5.0, accent());
        }
    }
}

fn draw_bleed_safe(p: &eframe::egui::Painter, rect: Rect, studio: &Studio) {
    let doc = &studio.doc;
    if !doc.show_bleed && !doc.show_safe {
        return;
    }
    let bleed = doc.bleed.max(1.0);
    let boards: Vec<(f32, f32, f32, f32)> = if doc.artboards.is_empty() {
        vec![(0.0, 0.0, doc.width, doc.height)]
    } else {
        doc.artboards
            .iter()
            .map(|a| (a.origin.x, a.origin.y, a.size.x, a.size.y))
            .collect()
    };
    for (x0, y0, single_w, single_h) in boards {
        if doc.show_bleed {
            let br = Rect::from_min_max(
                win(rect, studio.view, Pt::new(x0 - bleed, y0 - bleed)),
                win(
                    rect,
                    studio.view,
                    Pt::new(x0 + single_w + bleed, y0 + single_h + bleed),
                ),
            );
            p.rect_stroke(
                br,
                0.0,
                Stroke::new(1.0, Color32::from_rgba_unmultiplied(220, 38, 38, 120)),
                eframe::egui::StrokeKind::Middle,
            );
            // corner crop marks
            let inset = 8.0;
            let tl = win(rect, studio.view, Pt::new(x0, y0));
            let tr = win(rect, studio.view, Pt::new(x0 + single_w, y0));
            let bl = win(rect, studio.view, Pt::new(x0, y0 + single_h));
            let brp = win(rect, studio.view, Pt::new(x0 + single_w, y0 + single_h));
            let red = Color32::from_rgb(220, 38, 38);
            // TL
            p.line_segment(
                [pos2(tl.x - bleed, tl.y), pos2(tl.x - bleed + inset, tl.y)],
                Stroke::new(1.0, red),
            );
            p.line_segment(
                [pos2(tl.x, tl.y - bleed), pos2(tl.x, tl.y - bleed + inset)],
                Stroke::new(1.0, red),
            );
            // TR
            p.line_segment(
                [pos2(tr.x + bleed - inset, tr.y), pos2(tr.x + bleed, tr.y)],
                Stroke::new(1.0, red),
            );
            p.line_segment(
                [pos2(tr.x, tr.y - bleed), pos2(tr.x, tr.y - bleed + inset)],
                Stroke::new(1.0, red),
            );
            // BL
            p.line_segment(
                [pos2(bl.x - bleed, bl.y), pos2(bl.x - bleed + inset, bl.y)],
                Stroke::new(1.0, red),
            );
            p.line_segment(
                [pos2(bl.x, bl.y + bleed - inset), pos2(bl.x, bl.y + bleed)],
                Stroke::new(1.0, red),
            );
            // BR
            p.line_segment(
                [
                    pos2(brp.x + bleed - inset, brp.y),
                    pos2(brp.x + bleed, brp.y),
                ],
                Stroke::new(1.0, red),
            );
            p.line_segment(
                [
                    pos2(brp.x, brp.y + bleed - inset),
                    pos2(brp.x, brp.y + bleed),
                ],
                Stroke::new(1.0, red),
            );
        }
        if doc.show_safe {
            let inset = bleed + 18.0;
            let sr = Rect::from_min_max(
                win(rect, studio.view, Pt::new(x0 + inset, y0 + inset)),
                win(
                    rect,
                    studio.view,
                    Pt::new(x0 + single_w - inset, y0 + single_h - inset),
                ),
            );
            p.rect_stroke(
                sr,
                0.0,
                Stroke::new(1.0, Color32::from_rgba_unmultiplied(34, 197, 94, 140)),
                eframe::egui::StrokeKind::Middle,
            );
            p.rect_filled(sr, 0.0, Color32::from_rgba_unmultiplied(34, 197, 94, 16));
        }
    }
}

fn paint_brush_ring(p: &eframe::egui::Painter, rect: Rect, studio: &Studio) {
    if !matches!(
        studio.tool,
        Tool::Brush | Tool::Eraser | Tool::Clone | Tool::Heal | Tool::Smudge
    ) {
        return;
    }
    let Some(world) = studio.cursor else {
        return;
    };
    let center = win(rect, studio.view, world);
    if !rect.contains(center) {
        return;
    }
    let radius = (studio.brush.size * studio.view.scale * 0.5).max(1.0);
    p.circle_stroke(center, radius + 1.0, Stroke::new(1.0, Color32::BLACK));
    p.circle_stroke(center, radius, Stroke::new(1.0, Color32::WHITE));
}

fn draw_overlays(p: &eframe::egui::Painter, rect: Rect, studio: &Studio, pen_preview: Option<Pt>) {
    paint_brush_ring(p, rect, studio);
    let v = studio.view;
    if let Some(Op::Create {
        kind,
        start,
        cur,
        from_center,
    }) = &studio.op
    {
        let start = if *from_center {
            *start - (*cur - *start)
        } else {
            *start
        };
        let g = preview_shape(*kind, start, *cur, studio);
        stroke_geom(p, rect, &g, v);
    }
    if let Some(Op::Pen {
        anchors, source, ..
    }) = &studio.op
    {
        draw_pen(p, rect, anchors, studio, source.is_some(), pen_preview);
    }
    if let Some(Op::Pencil { pts }) = &studio.op {
        let scr: Vec<Pos2> = pts.iter().map(|q| win(rect, v, *q)).collect();
        if scr.len() >= 2 {
            p.add(eframe::egui::Shape::line(scr, Stroke::new(1.5, select())));
        }
    }
    if let Some(Op::Marquee {
        start,
        cur,
        ellipse,
    }) = &studio.op
    {
        if *ellipse {
            let min = Pt::new(start.x.min(cur.x), start.y.min(cur.y));
            let max = Pt::new(start.x.max(cur.x), start.y.max(cur.y));
            let g = Geom::Ellipse {
                center: (min + max) * 0.5,
                radii: (max - min) * 0.5,
            };
            stroke_geom(p, rect, &g, v);
        } else {
            let a = win(rect, v, *start);
            let b = win(rect, v, *cur);
            let r = Rect::from_two_pos(a, b);
            p.rect_filled(r, 0.0, select_fill());
            p.rect_stroke(
                r,
                0.0,
                Stroke::new(1.0, select()),
                eframe::egui::StrokeKind::Middle,
            );
        }
    }
    if studio.tool == Tool::Gradient {
        let endpoints = if let Some(Op::Gradient { start, cur }) = studio.op {
            Some((start, cur))
        } else {
            studio
                .primary()
                .and_then(|(li, id)| studio.doc.find_shape(li, id))
                .and_then(|s| {
                    let gradient = if studio.fill_active {
                        s.style.fill.gradient()
                    } else {
                        s.style.stroke.as_ref().and_then(|s| s.gradient.clone())
                    }?;
                    let (a, b) = gradient.endpoints(s.geom.bbox());
                    Some((s.world_point(a), s.world_point(b)))
                })
        };
        if let Some((a, b)) = endpoints {
            let a = win(rect, v, a);
            let b = win(rect, v, b);
            p.line_segment([a, b], Stroke::new(3., Color32::BLACK));
            p.line_segment([a, b], Stroke::new(1.5, Color32::WHITE));
            for endpoint in [a, b] {
                p.circle_filled(endpoint, 5., select());
                p.circle_stroke(endpoint, 5., Stroke::new(1.5, Color32::WHITE));
            }
        }
    }
    if let Some(Op::CropPhoto { start, cur }) | Some(Op::ZoomBox { start, cur }) = &studio.op {
        let a = win(rect, v, *start);
        let b = win(rect, v, *cur);
        let r = Rect::from_two_pos(a, b);
        p.rect_filled(r, 0.0, select_fill());
        p.rect_stroke(
            r,
            0.0,
            Stroke::new(1.0, select()),
            eframe::egui::StrokeKind::Middle,
        );
    }
    if let Some(Op::Lasso { pts }) = &studio.op {
        let scr: Vec<Pos2> = pts.iter().map(|q| win(rect, v, *q)).collect();
        if scr.len() >= 3 {
            p.add(eframe::egui::epaint::PathShape {
                points: scr,
                closed: true,
                fill: select_fill(),
                stroke: Stroke::new(1.2, select()).into(),
            });
        } else if scr.len() == 2 {
            p.add(eframe::egui::Shape::line(scr, Stroke::new(1.2, select())));
        }
    }
    draw_pixel_sel(p, rect, studio);

    // Draw selection outlines. For multi-selection we show a single group bbox;
    // otherwise per-shape bbox is the same as group, but we still use group
    // so handles are in the right place for rotated boxes.
    for (li, id) in &studio.selection {
        if *id == RASTER_ID {
            if let Some(b) = studio
                .doc
                .layers
                .get(*li)
                .and_then(|l| l.kind.raster_bounds())
            {
                let sb = Rect::from_min_max(win(rect, v, b.min), win(rect, v, b.max));
                p.rect_stroke(
                    sb,
                    0.0,
                    Stroke::new(1.0, select()),
                    eframe::egui::StrokeKind::Middle,
                );
            }
            continue;
        }
        if let Some(s) = studio.doc.find_shape(*li, *id) {
            stroke_world_posed(p, rect, s, v, studio.live_pose(*id));
            if studio.tool == Tool::Node
                && let Some(anchors) = s.world_anchors()
            {
                draw_nodes(p, rect, &anchors, v, &studio.node_sel);
            }
            if studio.deformation.is_none()
                && studio.tool == Tool::Node
                && let Geom::Rect { origin, size, .. } = &s.geom
            {
                for w in crate::geom::corner_widgets(*origin, *size) {
                    let sp = win(rect, v, s.world_point(w));
                    p.circle_filled(sp, 4.0, accent());
                }
            }
        }
    }
    if let Some(Op::NodeMarquee { start, cur }) | Some(Op::ArtboardDraw { start, cur }) = &studio.op
    {
        let a = win(rect, v, *start);
        let b = win(rect, v, *cur);
        let r = Rect::from_two_pos(a, b);
        p.rect_filled(r, 0.0, select_fill());
        p.rect_stroke(
            r,
            0.0,
            Stroke::new(1.0, select()),
            eframe::egui::StrokeKind::Middle,
        );
    }
    if studio.deformation.is_none()
        && !studio.selection.is_empty()
        && let Some(b) = posed_bounds(studio)
    {
        let sb = Rect::from_min_max(win(rect, v, b.min), win(rect, v, b.max));
        p.rect_stroke(
            sb,
            0.0,
            Stroke::new(1.0, select()),
            eframe::egui::StrokeKind::Middle,
        );
        let editing = studio
            .selection
            .iter()
            .any(|(li, id)| studio.editing_text(*li, *id));
        if !editing {
            for i in 0..8 {
                let h = win(rect, v, b.handle(i));
                p.rect_filled(Rect::from_center_size(h, vec2(7.0, 7.0)), 0.0, select());
            }
            let rh = win(rect, v, b.rotate_handle());
            p.line_segment([sb.center_top(), rh], Stroke::new(1.0, select()));
            p.circle_filled(rh, 4.0, accent());
        }
    }
    let placement_corners = match &studio.op {
        Some(Op::Place { start, cur }) => studio.pending_drag_preview_corners(*start, *cur),
        None => studio
            .cursor
            .and_then(|at| studio.pending_preview_corners(at)),
        _ => None,
    };
    if let Some(corners) = placement_corners {
        p.add(eframe::egui::Shape::convex_polygon(
            corners
                .into_iter()
                .map(|point| win(rect, v, point))
                .collect(),
            select_fill(),
            Stroke::new(1.0, select()),
        ));
    }
    if let Some(edit) = &studio.type_edit
        && let Some(s) = studio.doc.find_shape(edit.layer, edit.id)
        && let Geom::Text(run) = &s.geom
    {
        draw_type_caret(p, rect, studio, run, edit.caret, edit.anchor);
    }
}

fn draw_type_caret(
    p: &eframe::egui::Painter,
    rect: Rect,
    studio: &Studio,
    run: &crate::geom::TypeRun,
    caret: usize,
    anchor: usize,
) {
    let v = studio.view;
    for (a, b) in crate::text::selection_rects(run, caret, anchor) {
        let r = Rect::from_min_max(win(rect, v, a), win(rect, v, b));
        p.rect_filled(r, 0.0, select_fill());
    }
    let phase = (p.ctx().input(|i| i.time) * 2.0).fract();
    let on = phase < 0.5;
    let next = if on { 0.5 - phase } else { 1.0 - phase };
    p.ctx()
        .request_repaint_after(std::time::Duration::from_secs_f64(next * 0.5 + 0.001));
    if on {
        let c = crate::text::caret_pt(run, caret);
        let top = win(rect, v, Pt::new(c.x, c.y - run.px * 0.9));
        let bot = win(rect, v, Pt::new(c.x, c.y + run.px * 0.2));
        p.line_segment([top, bot], Stroke::new(1.5, select()));
    }
}

fn draw_pixel_sel(p: &eframe::egui::Painter, rect: Rect, studio: &Studio) {
    let Some(mask) = studio.pixel_sel.as_ref() else {
        return;
    };
    let space = studio.pixel_sel_space.or_else(|| {
        let layer = studio.doc.layers.get(studio.raster_target()?)?;
        let pixels = layer.kind.pixels()?;
        Some(crate::app::masking::SelectionSpace {
            w: pixels.w,
            h: pixels.h,
            transform: compositor::layer_pixel_transform(layer),
        })
    });
    let Some(space) = space else {
        return;
    };
    let (w, h) = (space.w, space.h);
    if mask.len() != w as usize * h as usize {
        return;
    }
    let Some((x0, y0, x1, y1)) = paint::selection_bounds(mask, w, h) else {
        return;
    };
    let id = eframe::egui::Id::new("pixel-sel-overlay");
    let tex = p.ctx().data(|data| {
        data.get_temp::<(u64, eframe::egui::TextureHandle)>(id)
            .filter(|(generation, _)| *generation == studio.pixel_sel_gen)
            .map(|(_, tex)| tex)
    });
    let tex = tex.unwrap_or_else(|| {
        let color = select();
        let rgba = paint::selection_overlay_rgba(mask, [color.r(), color.g(), color.b()], 72);
        let image = ColorImage::from_rgba_unmultiplied([w as usize, h as usize], &rgba);
        let tex = p
            .ctx()
            .load_texture("pixel-sel", image, TextureOptions::NEAREST);
        p.ctx().data_mut(|data| {
            data.insert_temp(id, (studio.pixel_sel_gen, tex.clone()));
        });
        tex
    });
    let transform = space.transform;
    let corner = |x: u32, y: u32| {
        let mut point = tiny_skia::Point::from_xy(x as f32, y as f32);
        transform.map_point(&mut point);
        win(rect, studio.view, Pt::new(point.x, point.y))
    };
    // Keep the tint's texels in native pixel space, just like the painted image
    // and selection outline. A screen-aligned image stretches rotated masks.
    let mut mesh = eframe::egui::Mesh::with_texture(tex.id());
    for (pos, uv) in [
        (corner(0, 0), pos2(0.0, 0.0)),
        (corner(w, 0), pos2(1.0, 0.0)),
        (corner(w, h), pos2(1.0, 1.0)),
        (corner(0, h), pos2(0.0, 1.0)),
    ] {
        mesh.vertices.push(eframe::egui::epaint::Vertex {
            pos,
            uv,
            color: Color32::WHITE,
        });
    }
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(0, 2, 3);
    p.add(Shape::mesh(mesh));
    let outline = [
        corner(x0, y0),
        corner(x1, y0),
        corner(x1, y1),
        corner(x0, y1),
        corner(x0, y0),
    ];
    let phase = (p.ctx().input(|i| i.time) * 28.0) as f32;
    p.extend(Shape::dashed_line_with_offset(
        &outline,
        Stroke::new(1.0, Color32::WHITE),
        &[5.0],
        &[5.0],
        phase,
    ));
    p.extend(Shape::dashed_line_with_offset(
        &outline,
        Stroke::new(1.0, Color32::BLACK),
        &[5.0],
        &[5.0],
        phase + 5.0,
    ));
    p.ctx()
        .request_repaint_after(std::time::Duration::from_millis(32));
}

fn stroke_geom(p: &eframe::egui::Painter, rect: Rect, g: &Geom, view: crate::compositor::View) {
    for c in g.contours(64) {
        let pts: Vec<Pos2> = c.iter().map(|q| win(rect, view, *q)).collect();
        if pts.len() >= 2 {
            if g.is_closed() {
                p.add(eframe::egui::epaint::PathShape {
                    points: pts,
                    closed: true,
                    fill: select_fill(),
                    stroke: Stroke::new(1.4, select()).into(),
                });
            } else {
                p.add(eframe::egui::Shape::line(pts, Stroke::new(1.4, select())));
            }
        }
    }
}

fn set_cursor(ui: &mut Ui, studio: &Studio, resp: &eframe::egui::Response) {
    if !resp.hovered() && !resp.dragged() {
        return;
    }
    use eframe::egui::CursorIcon;
    let icon = if studio.type_edit.is_some() {
        CursorIcon::Text
    } else if studio.pending_place.is_some() {
        CursorIcon::Copy
    } else if (ui.ctx().input(|i| i.key_down(eframe::egui::Key::Space)) && !studio.is_motion())
        || studio.tool == Tool::Hand
    {
        if resp.dragged() {
            CursorIcon::Grabbing
        } else {
            CursorIcon::Grab
        }
    } else {
        match studio.tool {
            Tool::Text => CursorIcon::Text,
            Tool::Zoom => {
                if ui.ctx().input(|i| i.modifiers.alt) {
                    CursorIcon::ZoomOut
                } else {
                    CursorIcon::ZoomIn
                }
            }
            Tool::Pen
            | Tool::Pencil
            | Tool::Brush
            | Tool::Eraser
            | Tool::Clone
            | Tool::Heal
            | Tool::Smudge => CursorIcon::Crosshair,
            Tool::Eyedropper | Tool::Trace => CursorIcon::Crosshair,
            Tool::Crop | Tool::Marquee | Tool::EllipseMarquee | Tool::Lasso => {
                CursorIcon::Crosshair
            }
            Tool::Select => CursorIcon::Default,
            Tool::Artboard => CursorIcon::Crosshair,
            _ => CursorIcon::Crosshair,
        }
    };
    ui.ctx().set_cursor_icon(icon);
}

fn context_menu(resp: &eframe::egui::Response, studio: &mut Studio) {
    if resp.secondary_clicked()
        && let Some(at) = resp.interact_pointer_pos()
    {
        let origin = Pt::new(resp.rect.min.x, resp.rect.min.y);
        let point = studio.view.pointer_to_world(origin, from_egui(at));
        if let Some(hit) = hit_shape(studio, point, 6.0 / studio.view.scale.max(0.01))
            && !studio.selection.contains(&hit)
        {
            studio.commit_type_edit();
            studio.end_deform(true);
            studio.reset_snap_gesture();
            studio.selected_layer = None;
            studio.selection = vec![hit];
            studio.node_sel.clear();
            studio.artboard_sel.clear();
            if studio.active_layer != Some(hit.0) {
                studio.paint_mask = false;
            }
            studio.active_layer = Some(hit.0);
        }
    }
    resp.context_menu(|ui| {
        if ui.button("Cut                    Ctrl+X").clicked() {
            studio.cut_selection(ui.ctx());
            ui.close();
        }
        if ui.button("Copy                   Ctrl+C").clicked() {
            studio.copy_selection(ui.ctx());
            ui.close();
        }
        if ui.button("Paste                  Ctrl+V").clicked() {
            studio.request_clipboard_paste(ui.ctx(), None);
            ui.close();
        }
        if ui.button("Duplicate              Super+D").clicked() {
            studio.duplicate_selection();
            ui.close();
        }
        if ui.button("Delete").clicked() {
            studio.delete_selection();
            ui.close();
        }
        ui.separator();
        if ui
            .add_enabled(
                studio.can_convert_to_guides(),
                eframe::egui::Button::new("Make guides"),
            )
            .clicked()
        {
            studio.convert_selection_to_guides();
            ui.close();
        }
        if ui
            .add_enabled(
                studio.can_release_guides(),
                eframe::egui::Button::new("Release guides"),
            )
            .clicked()
        {
            studio.release_selected_guides();
            ui.close();
        }
        ui.separator();
        if let Some(horizontal) = super::selection::flip_buttons(ui, studio.can_flip_selection()) {
            studio.flip_selection(horizontal);
        }
        ui.separator();
        if ui.button("Bring to front").clicked() {
            studio.bring_to_front();
            ui.close();
        }
        if ui.button("Send to back").clicked() {
            studio.send_to_back();
            ui.close();
        }
        ui.separator();
        if ui.button("Group     Ctrl+G").clicked() {
            studio.group_selected();
            ui.close();
        }
        if ui.button("Ungroup     Ctrl+Shift+G").clicked() {
            studio.ungroup_selected();
            ui.close();
        }
        if ui
            .add_enabled(
                studio.selection.len() >= 2,
                eframe::egui::Button::new("Compound shape     Ctrl+8"),
            )
            .clicked()
        {
            studio.combine_selected();
            ui.close();
        }
        ui.separator();
        if ui.button("Place…").clicked() {
            studio.begin_place();
            ui.close();
        }
        if ui.button("Trace to vector").clicked() {
            studio.trace_active_raster();
            ui.close();
        }
    });
}

fn commit_pose_drag(studio: &mut Studio) {
    if studio.pose_drag.is_empty() {
        return;
    }
    let t = studio.playhead;
    let drag = studio.pose_drag.clone();
    let mut after = studio.doc.motion.clone();
    for (id, pose) in &drag {
        after.set_key(
            *id,
            crate::motion::Prop::X,
            t,
            pose.dx,
            crate::motion::Ease::EaseInOut,
        );
        after.set_key(
            *id,
            crate::motion::Prop::Y,
            t,
            pose.dy,
            crate::motion::Ease::EaseInOut,
        );
        after.set_key(
            *id,
            crate::motion::Prop::Rotation,
            t,
            pose.rotation,
            crate::motion::Ease::EaseInOut,
        );
        after.set_key(
            *id,
            crate::motion::Prop::Scale,
            t,
            pose.scale,
            crate::motion::Ease::EaseInOut,
        );
        if let Some(op) = pose.opacity {
            after.set_key(
                *id,
                crate::motion::Prop::Opacity,
                t,
                op,
                crate::motion::Ease::EaseInOut,
            );
        }
    }
    studio.pose_drag.clear();
    studio.commit_motion(after);
}

fn stroke_world_posed(
    p: &eframe::egui::Painter,
    rect: Rect,
    s: &crate::document::Shape,
    view: crate::compositor::View,
    pose: crate::motion::Pose,
) {
    let c = s.world_bbox().center();
    for contour in s.world_contours(64) {
        let pts: Vec<Pos2> = contour
            .iter()
            .map(|q| win(rect, view, pose.map(c, *q)))
            .collect();
        if pts.len() >= 2 {
            p.add(eframe::egui::Shape::line(
                pts,
                Stroke::new(1.2, Color32::from_white_alpha(180)),
            ));
        }
    }
}

fn draw_pen(
    p: &eframe::egui::Painter,
    rect: Rect,
    anchors: &[Anchor],
    studio: &Studio,
    continuing: bool,
    preview: Option<Pt>,
) {
    let v = studio.view;
    let mut draft = anchors.to_vec();
    if let Some(c) = preview {
        draft.push(Anchor::corner(c));
    }
    if !continuing && draft.len() >= 2 {
        let g = Geom::Path {
            anchors: draft,
            closed: false,
        };
        for c in g.contours(16) {
            let pts: Vec<Pos2> = c.iter().map(|q| win(rect, v, *q)).collect();
            if pts.len() >= 2 {
                p.add(eframe::egui::Shape::line(pts, Stroke::new(1.6, select())));
            }
        }
    } else if let (Some(last), Some(c)) = (anchors.last(), preview) {
        p.line_segment(
            [win(rect, v, last.pt), win(rect, v, c)],
            Stroke::new(1.2, select()),
        );
    }
    let mut sel = BTreeSet::new();
    if !anchors.is_empty() {
        sel.insert(anchors.len() - 1);
    }
    draw_nodes(p, rect, anchors, v, &sel);
    if let Some(first) = anchors.first() {
        let close = studio
            .cursor
            .is_some_and(|c| (c - first.pt).length() * studio.view.scale < 12.0)
            && anchors.len() >= 3;
        p.circle_stroke(
            win(rect, v, first.pt),
            if close { 8.0 } else { 5.0 },
            Stroke::new(1.2, if close { accent() } else { select() }),
        );
    }
    if studio.tool == crate::tools::Tool::Pen {
        draw_open_ends(p, rect, studio);
    }
}

fn draw_open_ends(p: &eframe::egui::Painter, rect: Rect, studio: &Studio) {
    let skip = match &studio.op {
        Some(Op::Pen {
            source: Some((_, id, _)),
            ..
        }) => Some(*id),
        _ => None,
    };
    for layer in &studio.doc.layers {
        if !layer.visible {
            continue;
        }
        let Some(shapes) = layer.kind.shapes() else {
            continue;
        };
        for s in shapes {
            if skip == Some(s.id) || !s.visible || (s.guide && !studio.doc.ruler.guides_visible) {
                continue;
            }
            let Geom::Path {
                anchors,
                closed: false,
            } = &s.geom
            else {
                continue;
            };
            if anchors.len() < 2 {
                continue;
            }
            for pt in [anchors[0].pt, anchors.last().unwrap().pt] {
                p.circle_filled(win(rect, studio.view, pt), 4.0, accent());
            }
        }
    }
}

fn draw_nodes(
    p: &eframe::egui::Painter,
    rect: Rect,
    anchors: &[Anchor],
    view: crate::compositor::View,
    selected: &BTreeSet<usize>,
) {
    for (i, a) in anchors.iter().enumerate() {
        let sp = win(rect, view, a.pt);
        if !a.is_corner() {
            let hi = win(rect, view, a.pt + a.h_in);
            let ho = win(rect, view, a.pt + a.h_out);
            p.line_segment([sp, hi], Stroke::new(1.0, select()));
            p.line_segment([sp, ho], Stroke::new(1.0, select()));
            p.circle_filled(hi, 3.0, select());
            p.circle_filled(ho, 3.0, select());
        }
        let on = selected.contains(&i);
        let sz = if on { 8.0 } else { 6.0 };
        p.rect_filled(Rect::from_center_size(sp, vec2(sz, sz)), 0.0, select());
        if on {
            p.rect_stroke(
                Rect::from_center_size(sp, vec2(sz + 3.0, sz + 3.0)),
                0.0,
                Stroke::new(1.0, accent()),
                eframe::egui::StrokeKind::Outside,
            );
        }
        let radius_handle = sp + vec2(14.0, 14.0);
        p.line_segment([sp, radius_handle], Stroke::new(0.75, accent()));
        p.circle_filled(radius_handle, 3.5, accent());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rotated_raster_selection_tint_tracks_native_pixels_through_camera() {
        use crate::document::{Layer, Pixels};
        let ctx = eframe::egui::Context::default();
        let mut studio = Studio::new();
        studio.doc.layers = vec![Layer::placed_raster(
            "Rotated pixels",
            Pixels::from_rgba(4, 2, [255, 255, 255, 255].repeat(8)).unwrap(),
            Pt::new(100.0, 100.0),
            Pt::new(120.0, 60.0),
        )];
        studio.doc.layers[0].kind.set_raster_xform(
            Pt::new(100.0, 100.0),
            Pt::new(120.0, 60.0),
            std::f32::consts::FRAC_PI_2,
        );
        studio.active_layer = Some(0);
        studio.view = compositor::View {
            scale: 1.5,
            offset: Pt::new(7.0, -11.0),
        };
        studio.set_pixel_sel(Some(vec![255, 0, 0, 0, 0, 0, 0, 0]));
        let rect = Rect::from_min_size(pos2(30.0, 40.0), vec2(400.0, 400.0));
        let mut output = ctx.run_ui(
            eframe::egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(600.0, 600.0))),
                ..Default::default()
            },
            |ui| draw_pixel_sel(ui.painter(), rect, &studio),
        );
        let mesh = output
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                Shape::Mesh(mesh) => Some(mesh),
                _ => None,
            })
            .expect("selection tint must use a transformed texture quad");
        assert!(mesh.is_valid());
        assert_eq!(mesh.indices.len(), 6);
        assert_eq!(mesh.vertices.len(), 4);
        for (vertex, (expected, uv)) in mesh.vertices.iter().zip([
            (pos2(322.0, 134.0), pos2(0.0, 0.0)),
            (pos2(322.0, 314.0), pos2(1.0, 0.0)),
            (pos2(232.0, 314.0), pos2(1.0, 1.0)),
            (pos2(232.0, 134.0), pos2(0.0, 1.0)),
        ]) {
            assert!(vertex.pos.distance(expected) < 0.001);
            assert_eq!(vertex.uv, uv);
        }
        // The sole selected texel is the upper-left source pixel; its center
        // must follow rotation rather than remain in the screen's upper-left.
        let selected_center = mesh.vertices[0].pos
            + (mesh.vertices[1].pos - mesh.vertices[0].pos) * 0.125
            + (mesh.vertices[3].pos - mesh.vertices[0].pos) * 0.25;
        assert!(selected_center.distance(pos2(299.5, 156.5)) < 0.001);
        let delta = output
            .textures_delta
            .set
            .get(&mesh.texture_id)
            .and_then(|updates| updates.first())
            .expect("selection texture uploaded");
        let eframe::egui::ImageData::Color(image) = &delta.image;
        assert_eq!(image.size, [4, 2]);
        assert!(image.pixels[0].a() > 0);
        assert!(image.pixels[1..].iter().all(|pixel| pixel.a() == 0));
        output.textures_delta.clear();
    }

    pub(super) fn canvas_frame(
        ctx: &eframe::egui::Context,
        studio: &mut Studio,
        events: Vec<eframe::egui::Event>,
    ) -> Vec<eframe::egui::epaint::ClippedShape> {
        let mut output = ctx.run_ui(
            eframe::egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(400.0, 300.0))),
                events,
                ..Default::default()
            },
            |ui| show(ui, studio),
        );
        output.textures_delta.clear();
        output.shapes
    }

    pub(super) fn pen_input_studio() -> (eframe::egui::Context, Studio) {
        let ctx = eframe::egui::Context::default();
        crate::ui::theme::apply(&ctx);
        let mut studio = Studio::new();
        studio.doc = crate::document::Document::new("Pen input", 1.0, 1.0, 72.0);
        studio.doc.width = 400.0;
        studio.doc.height = 300.0;
        studio.doc.layers = vec![crate::document::Layer::vector("Drawing")];
        studio.show_welcome = false;
        studio.need_fit = false;
        studio.tool = Tool::Pen;
        studio.view.offset = Pt::new(13.0, 17.0);
        studio.snap = crate::snap::SnapSettings {
            enabled: false,
            grid: false,
            guides: true,
            objects: false,
            artboards: false,
            spacing: false,
            threshold: 6.0,
        };
        (ctx, studio)
    }

    pub(super) fn pen_pointer_button(
        at: Pos2,
        pressed: bool,
        modifiers: eframe::egui::Modifiers,
    ) -> eframe::egui::Event {
        eframe::egui::Event::PointerButton {
            pos: at,
            button: PointerButton::Primary,
            pressed,
            modifiers,
        }
    }

    fn selection_input_studio() -> (eframe::egui::Context, Studio, [(usize, u64); 2]) {
        let (ctx, mut studio) = pen_input_studio();
        studio.view.offset = Pt::ZERO;
        studio.view.scale = 1.0;
        studio.finish_create(CreateKind::Rect, Pt::new(30.0, 40.0), Pt::new(110.0, 120.0));
        let first = studio.primary().unwrap();
        studio.finish_create(
            CreateKind::Rect,
            Pt::new(190.0, 40.0),
            Pt::new(270.0, 120.0),
        );
        let second = studio.primary().unwrap();
        studio.tool = Tool::Select;
        studio.selection.clear();
        studio.dirty = false;
        canvas_frame(&ctx, &mut studio, vec![]);
        (ctx, studio, [first, second])
    }

    #[test]
    fn shift_click_adds_and_removes_once_with_separate_and_batched_pointer_frames() {
        use eframe::egui::{Event, Modifiers};
        for (batched, release_shift) in [(false, false), (false, true), (true, false), (true, true)]
        {
            let (ctx, mut studio, [first, second]) = selection_input_studio();
            let history = studio.history.len();
            let origin = studio.canvas_rect.unwrap().min;
            for (point, modifiers, expected) in [
                (vec2(70.0, 80.0), Modifiers::NONE, vec![first]),
                (vec2(230.0, 80.0), Modifiers::SHIFT, vec![first, second]),
                (vec2(30.0, 40.0), Modifiers::SHIFT, vec![first, second]),
                (vec2(70.0, 80.0), Modifiers::SHIFT, vec![second]),
                (vec2(340.0, 180.0), Modifiers::SHIFT, vec![second]),
                (vec2(340.0, 180.0), Modifiers::NONE, vec![]),
            ] {
                let at = origin + point;
                let mut down = vec![
                    Event::PointerMoved(at),
                    pen_pointer_button(at, true, modifiers),
                ];
                // Fast modifier release must not replace the click's modifiers.
                let mut up = vec![pen_pointer_button(at, false, modifiers)];
                if release_shift {
                    up.push(Event::ModifiersChanged(Modifiers::NONE));
                }
                if batched {
                    down.extend(up);
                    canvas_frame(&ctx, &mut studio, down);
                } else {
                    canvas_frame(&ctx, &mut studio, down);
                    canvas_frame(&ctx, &mut studio, up);
                }
                canvas_frame(&ctx, &mut studio, vec![]);
                assert_eq!(
                    studio.selection, expected,
                    "batched={batched}, point={point:?}"
                );
                assert!(studio.op.is_none());
                assert_eq!(
                    studio.history.len(),
                    history,
                    "selection is not a document edit"
                );
                assert!(!studio.dirty);
            }
        }
    }

    #[test]
    fn shift_drag_moves_the_existing_selection_and_single_click_collapses_it() {
        use eframe::egui::{Event, Modifiers};
        let (ctx, mut studio, ids) = selection_input_studio();
        studio.selection = ids.to_vec();
        let original =
            ids.map(|(layer, id)| studio.doc.find_shape(layer, id).unwrap().geom.clone());
        let at = studio.canvas_rect.unwrap().min + vec2(70.0, 80.0);
        canvas_frame(
            &ctx,
            &mut studio,
            vec![
                Event::PointerMoved(at),
                pen_pointer_button(at, true, Modifiers::SHIFT),
            ],
        );
        canvas_frame(
            &ctx,
            &mut studio,
            vec![
                Event::ModifiersChanged(Modifiers::SHIFT),
                Event::PointerMoved(at + vec2(40.0, 6.0)),
            ],
        );
        canvas_frame(
            &ctx,
            &mut studio,
            vec![pen_pointer_button(
                at + vec2(40.0, 6.0),
                false,
                Modifiers::SHIFT,
            )],
        );
        assert_eq!(studio.selection, ids);
        for ((layer, id), before) in ids.into_iter().zip(&original) {
            let delta =
                studio.doc.find_shape(layer, id).unwrap().geom.bbox().min - before.bbox().min;
            assert!(
                (delta.x - 40.0).abs() < 1.0 && delta.y.abs() < 0.01,
                "constrained group movement: {delta:?}"
            );
        }
        studio.undo();
        for ((layer, id), before) in ids.into_iter().zip(&original) {
            assert_eq!(&studio.doc.find_shape(layer, id).unwrap().geom, before);
        }
        studio.selection = ids.to_vec();
        canvas_frame(
            &ctx,
            &mut studio,
            vec![
                Event::PointerMoved(at),
                pen_pointer_button(at, true, Modifiers::NONE),
            ],
        );
        canvas_frame(
            &ctx,
            &mut studio,
            vec![pen_pointer_button(at, false, Modifiers::NONE)],
        );
        assert_eq!(studio.selection, vec![ids[0]]);
    }

    #[test]
    fn shift_marquee_keeps_the_original_selection_after_modifier_release() {
        use eframe::egui::{Event, Modifiers};
        let (ctx, mut studio, ids) = selection_input_studio();
        studio.selection = vec![ids[0]];
        let history = studio.history.len();
        let origin = studio.canvas_rect.unwrap().min;
        let start = origin + vec2(175.0, 25.0);
        let end = origin + vec2(285.0, 140.0);
        canvas_frame(
            &ctx,
            &mut studio,
            vec![
                Event::PointerMoved(start),
                pen_pointer_button(start, true, Modifiers::SHIFT),
            ],
        );
        canvas_frame(&ctx, &mut studio, vec![Event::PointerMoved(end)]);
        canvas_frame(
            &ctx,
            &mut studio,
            vec![
                pen_pointer_button(end, false, Modifiers::SHIFT),
                Event::ModifiersChanged(Modifiers::NONE),
            ],
        );
        assert_eq!(studio.selection, ids);
        assert_eq!(studio.history.len(), history);
        assert!(!studio.dirty);
    }

    #[test]
    fn marquee_uses_press_origin_when_motion_arrives_in_the_same_frame() {
        use eframe::egui::{Event, Modifiers};
        let (ctx, mut studio, ids) = selection_input_studio();
        studio.selection.clear();
        let origin = studio.canvas_rect.unwrap().min;
        let start = origin + vec2(175.0, 25.0);
        let end = origin + vec2(285.0, 140.0);
        canvas_frame(
            &ctx,
            &mut studio,
            vec![
                Event::PointerMoved(start),
                pen_pointer_button(start, true, Modifiers::NONE),
                Event::PointerMoved(end),
            ],
        );
        let expected = studio
            .view
            .pointer_to_world(from_egui(origin), from_egui(start));
        assert!(matches!(&studio.op, Some(Op::Marquee { start, .. }) if *start == expected));
        canvas_frame(&ctx, &mut studio, vec![Event::PointerMoved(end)]);
        canvas_frame(
            &ctx,
            &mut studio,
            vec![pen_pointer_button(end, false, Modifiers::NONE)],
        );
        assert_eq!(studio.selection, vec![ids[1]]);
    }

    fn assert_pen_preview(shapes: &[eframe::egui::epaint::ClippedShape], from: Pos2, to: Pos2) {
        fn matches(shape: &eframe::egui::Shape, from: Pos2, to: Pos2) -> bool {
            match shape {
                eframe::egui::Shape::Path(path) => {
                    path.points.first().is_some_and(|p| p.distance(from) < 0.01)
                        && path.points.last().is_some_and(|p| p.distance(to) < 0.01)
                }
                eframe::egui::Shape::LineSegment { points, .. } => {
                    points[0].distance(from) < 0.01 && points[1].distance(to) < 0.01
                }
                eframe::egui::Shape::Vec(shapes) => {
                    shapes.iter().any(|shape| matches(shape, from, to))
                }
                _ => false,
            }
        }
        assert!(
            shapes.iter().any(|shape| matches(&shape.shape, from, to)),
            "Pen preview did not reach {to:?} from {from:?}"
        );
    }

    #[test]
    fn pen_shift_hover_and_click_agree_at_horizontal_diagonal_and_vertical_angles() {
        use eframe::egui::{Event, Modifiers};
        for delta in [
            Pt::new(70.0, 18.0),
            Pt::new(70.0, 44.0),
            Pt::new(18.0, 70.0),
        ] {
            let (ctx, mut studio) = pen_input_studio();
            let start = Pt::new(60.0, 60.0);
            let first = to_egui(studio.view.to_screen(start));
            canvas_frame(&ctx, &mut studio, vec![Event::PointerMoved(first)]);
            canvas_frame(
                &ctx,
                &mut studio,
                vec![pen_pointer_button(first, true, Modifiers::NONE)],
            );
            canvas_frame(
                &ctx,
                &mut studio,
                vec![pen_pointer_button(first, false, Modifiers::NONE)],
            );
            let raw = to_egui(studio.view.to_screen(start + delta));
            let free = canvas_frame(&ctx, &mut studio, vec![Event::PointerMoved(raw)]);
            assert_pen_preview(&free, first, raw);
            let length = (delta.x * delta.x + delta.y * delta.y).sqrt();
            let offset = if delta.y < 20.0 {
                Pt::new(length, 0.0)
            } else if delta.x < 20.0 {
                Pt::new(0.0, length)
            } else {
                Pt::splat(length / 2.0_f32.sqrt())
            };
            let expected = start + offset;
            let preview = canvas_frame(
                &ctx,
                &mut studio,
                vec![Event::ModifiersChanged(Modifiers::SHIFT)],
            );
            assert_pen_preview(&preview, first, to_egui(studio.view.to_screen(expected)));
            let free = canvas_frame(
                &ctx,
                &mut studio,
                vec![Event::ModifiersChanged(Modifiers::NONE)],
            );
            assert_pen_preview(&free, first, raw);
            // A click and quick modifier release may arrive in the same native frame.
            canvas_frame(
                &ctx,
                &mut studio,
                vec![
                    Event::ModifiersChanged(Modifiers::SHIFT),
                    pen_pointer_button(raw, true, Modifiers::SHIFT),
                    pen_pointer_button(raw, false, Modifiers::SHIFT),
                    Event::ModifiersChanged(Modifiers::NONE),
                ],
            );
            let Some(Op::Pen { anchors, .. }) = &studio.op else {
                panic!("live Pen path")
            };
            assert_eq!(anchors.len(), 2);
            assert!(
                (anchors[1].pt - expected).length() < 0.001,
                "Shift click ignored its event modifiers: {:?} versus {expected:?}",
                anchors[1].pt
            );
            assert!(
                anchors[1].is_corner(),
                "snapping must not turn a click into a handle drag"
            );
        }
    }

    #[test]
    fn pen_shift_keeps_its_angle_when_ctrl_inverts_grid_guide_and_object_snapping() {
        use eframe::egui::{Event, Modifiers};
        for (target, snapped) in [("grid", 128.0), ("guide", 126.0), ("object", 126.0)] {
            for enabled in [false, true] {
                let (ctx, mut studio) = pen_input_studio();
                studio.snap.enabled = enabled;
                studio.snap.grid = true;
                studio.snap.objects = true;
                studio.doc.grid.snap = true;
                studio.doc.grid.size = 8.0;
                if target == "guide" {
                    studio.doc.guides.push(crate::document::Guide {
                        vertical: true,
                        pos: 126.0,
                    });
                } else if target == "object" {
                    studio.doc.layers[0].kind.shapes_mut().unwrap().push(
                        crate::document::Shape::new(
                            Geom::Rect {
                                origin: Pt::new(126.0, 200.0),
                                size: Pt::splat(20.0),
                                radius: 0.0,
                            },
                            crate::document::Style::default(),
                        ),
                    );
                }
                let start = Pt::splat(64.0);
                let first = to_egui(studio.view.to_screen(start));
                canvas_frame(&ctx, &mut studio, vec![Event::PointerMoved(first)]);
                canvas_frame(
                    &ctx,
                    &mut studio,
                    vec![pen_pointer_button(first, true, Modifiers::NONE)],
                );
                canvas_frame(
                    &ctx,
                    &mut studio,
                    vec![pen_pointer_button(first, false, Modifiers::NONE)],
                );
                let raw = to_egui(studio.view.to_screen(Pt::new(133.0, 120.0)));
                let free = 64.0 + ((69.0_f32 * 69.0 + 56.0 * 56.0) / 2.0).sqrt();
                let shift = Modifiers::SHIFT | Modifiers::ALT;
                let inverted = shift | Modifiers::CTRL | Modifiers::COMMAND;
                let preview = canvas_frame(
                    &ctx,
                    &mut studio,
                    vec![Event::ModifiersChanged(shift), Event::PointerMoved(raw)],
                );
                let expected = Pt::splat(if enabled { snapped } else { free });
                assert_pen_preview(&preview, first, to_egui(studio.view.to_screen(expected)));
                let preview =
                    canvas_frame(&ctx, &mut studio, vec![Event::ModifiersChanged(inverted)]);
                let expected = Pt::splat(if enabled { free } else { snapped });
                assert_pen_preview(&preview, first, to_egui(studio.view.to_screen(expected)));
                canvas_frame(
                    &ctx,
                    &mut studio,
                    vec![
                        pen_pointer_button(raw, true, inverted),
                        pen_pointer_button(raw, false, inverted),
                        Event::ModifiersChanged(Modifiers::NONE),
                    ],
                );
                let Some(Op::Pen { anchors, .. }) = &studio.op else {
                    panic!("live Pen path")
                };
                assert_eq!(anchors.len(), 2);
                assert!(
                    (anchors[1].pt - expected).length() < 0.001,
                    "{target}, enabled={enabled}: {:?} versus {expected:?}",
                    anchors[1].pt
                );
                assert!(anchors[1].is_corner());
            }
        }
    }

    #[test]
    fn pen_handles_follow_stationary_shift_alt_changes_and_the_release_sample() {
        use eframe::egui::{Event, Modifiers};
        for delta in [
            Pt::new(50.0, 12.0),
            Pt::new(50.0, 32.0),
            Pt::new(12.0, 50.0),
        ] {
            let (ctx, mut studio) = pen_input_studio();
            studio.view.scale = 2.0;
            let start = Pt::new(40.0, 40.0);
            let first = to_egui(studio.view.to_screen(start));
            canvas_frame(&ctx, &mut studio, vec![Event::PointerMoved(first)]);
            canvas_frame(
                &ctx,
                &mut studio,
                vec![pen_pointer_button(first, true, Modifiers::NONE)],
            );
            let raw = to_egui(studio.view.to_screen(start + delta));
            canvas_frame(&ctx, &mut studio, vec![Event::PointerMoved(raw)]);
            let handles = |studio: &Studio| {
                let Some(Op::Pen { anchors, .. }) = &studio.op else {
                    panic!("live Pen handles")
                };
                (anchors[0].h_in, anchors[0].h_out)
            };
            let (incoming, outgoing) = handles(&studio);
            assert!((outgoing - delta).length() < 0.001);
            assert!((incoming + delta).length() < 0.001);
            let length = (delta.x * delta.x + delta.y * delta.y).sqrt();
            let constrained = if delta.y < 20.0 {
                Pt::new(length, 0.0)
            } else if delta.x < 20.0 {
                Pt::new(0.0, length)
            } else {
                Pt::splat(length / 2.0_f32.sqrt())
            };
            canvas_frame(
                &ctx,
                &mut studio,
                vec![Event::ModifiersChanged(Modifiers::SHIFT)],
            );
            let (incoming, outgoing) = handles(&studio);
            assert!((outgoing - constrained).length() < 0.001);
            assert!((incoming + constrained).length() < 0.001);
            // Alt frees the outgoing handle without moving the existing incoming handle.
            canvas_frame(
                &ctx,
                &mut studio,
                vec![Event::ModifiersChanged(Modifiers::ALT)],
            );
            let (incoming, outgoing) = handles(&studio);
            assert!((outgoing - delta).length() < 0.001);
            assert!((incoming + constrained).length() < 0.001);
            canvas_frame(
                &ctx,
                &mut studio,
                vec![Event::ModifiersChanged(Modifiers::NONE)],
            );
            let (incoming, outgoing) = handles(&studio);
            assert!((outgoing - delta).length() < 0.001);
            assert!((incoming + delta).length() < 0.001);
            // The final sample can move and release Shift before the next redraw.
            let release = to_egui(studio.view.to_screen(start + delta * 1.1));
            canvas_frame(
                &ctx,
                &mut studio,
                vec![
                    Event::ModifiersChanged(Modifiers::SHIFT | Modifiers::ALT),
                    Event::PointerMoved(release),
                    pen_pointer_button(release, false, Modifiers::SHIFT | Modifiers::ALT),
                    Event::ModifiersChanged(Modifiers::NONE),
                ],
            );
            let (incoming, outgoing) = handles(&studio);
            assert!(
                (outgoing - constrained * 1.1).length() < 0.001,
                "final constrained handle sample was lost: {outgoing:?}"
            );
            assert!(
                (incoming + delta).length() < 0.001,
                "Alt release must retain the incoming handle"
            );
        }
    }

    #[test]
    fn scroll_gestures_keep_their_modifiers_and_canvas_anchor() {
        use eframe::egui::{Event, Modifiers, MouseWheelUnit, TouchPhase};
        for (modifiers, tool, should_zoom) in [
            (Modifiers::CTRL | Modifiers::COMMAND, Tool::Select, true),
            (Modifiers::ALT, Tool::Select, true),
            (Modifiers::NONE, Tool::Zoom, true),
            (Modifiers::NONE, Tool::Select, false),
        ] {
            let ctx = eframe::egui::Context::default();
            crate::ui::theme::apply(&ctx);
            let mut studio = Studio::new();
            studio.need_fit = false;
            studio.tool = tool;
            let pointer = pos2(160.0, 120.0);
            canvas_frame(&ctx, &mut studio, vec![Event::PointerMoved(pointer)]);
            let local = from_egui(pointer - studio.canvas_rect.unwrap().min.to_vec2());
            let anchor = studio.view.to_world(local);
            let old_scale = studio.view.scale;
            let old_offset = studio.view.offset;
            canvas_frame(
                &ctx,
                &mut studio,
                vec![
                    Event::ModifiersChanged(modifiers),
                    Event::MouseWheel {
                        unit: MouseWheelUnit::Point,
                        delta: vec2(0.0, 40.0),
                        phase: TouchPhase::Move,
                        modifiers,
                    },
                    Event::ModifiersChanged(Modifiers::NONE),
                ],
            );
            // Continue the smoothed wheel movement after the modifier was released.
            canvas_frame(&ctx, &mut studio, vec![]);
            if should_zoom {
                assert!(studio.view.scale > old_scale, "{modifiers:?}, {tool:?}");
                assert!((studio.view.to_world(local) - anchor).length() < 0.001);
            } else {
                assert_eq!(studio.view.scale, old_scale);
                assert!(studio.view.offset.y > old_offset.y);
            }
        }
    }

    #[test]
    fn space_drag_pans_without_creating_or_moving_artwork() {
        use eframe::egui::{Event, Key, Modifiers};
        let ctx = eframe::egui::Context::default();
        let mut studio = oval();
        studio.need_fit = false;
        studio.tool = Tool::Rect;
        let document = serde_json::to_value(&studio.doc).unwrap();
        let start = pos2(200.0, 150.0);
        let end = start + vec2(36.0, 24.0);
        canvas_frame(&ctx, &mut studio, vec![Event::PointerMoved(start)]);
        let old_offset = studio.view.offset;
        canvas_frame(
            &ctx,
            &mut studio,
            vec![
                Event::Key {
                    key: Key::Space,
                    physical_key: Some(Key::Space),
                    pressed: true,
                    repeat: false,
                    modifiers: Modifiers::NONE,
                },
                Event::PointerButton {
                    pos: start,
                    button: PointerButton::Primary,
                    pressed: true,
                    modifiers: Modifiers::NONE,
                },
            ],
        );
        canvas_frame(&ctx, &mut studio, vec![Event::PointerMoved(end)]);
        assert!((studio.view.offset - old_offset - Pt::new(36.0, 24.0)).length() < 0.001);
        assert_eq!(serde_json::to_value(&studio.doc).unwrap(), document);
        assert!(studio.op.is_none());
    }

    #[test]
    fn live_canvas_type_does_not_steal_inspector_focus_or_text() {
        use eframe::egui::{Event, Id, RawInput, TextEdit};
        let ctx = eframe::egui::Context::default();
        let mut studio = Studio::new();
        studio.need_fit = false;
        studio.place_text(Pt::new(50.0, 80.0));
        let (layer, id) = studio.primary().unwrap();
        let before = studio.doc.find_shape(layer, id).unwrap().geom.clone();
        let field = Id::new("inspector-focus-test");
        let mut label = String::from("Layer");
        for events in [vec![], vec![Event::Text("42".into())]] {
            let mut output = ctx.run_ui(
                RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(400.0, 300.0))),
                    events,
                    ..Default::default()
                },
                |ui| {
                    studio.handle_shortcuts(ui.ctx());
                    ui.add(TextEdit::singleline(&mut label).id(field))
                        .request_focus();
                    show(ui, &mut studio);
                },
            );
            output.textures_delta.clear();
            assert_eq!(ctx.memory(|memory| memory.focused()), Some(field));
        }
        assert!(label.contains("42"));
        assert_eq!(studio.doc.find_shape(layer, id).unwrap().geom, before);
        ctx.memory_mut(|memory| memory.surrender_focus(field));
        canvas_frame(&ctx, &mut studio, vec![]);
        assert_eq!(
            ctx.memory(|memory| memory.focused()),
            Some(Id::new("studio-canvas"))
        );
    }

    #[test]
    fn grid_and_rulers_stay_bounded_when_zoomed_out_and_panned() {
        let mut studio = Studio::new();
        studio.view.scale = 0.02;
        studio.view.offset = Pt::new(-10_000.0, -10_000.0);
        studio.doc.grid.size = 1.0;
        let rect = Rect::from_min_size(Pos2::ZERO, vec2(320.0, 240.0));
        let ctx = eframe::egui::Context::default();
        let mut output = ctx.run_ui(
            eframe::egui::RawInput {
                screen_rect: Some(rect),
                ..Default::default()
            },
            |ui| {
                draw_grid(ui.painter(), rect, &studio);
                super::super::guides::draw(ui.painter(), rect, &studio);
            },
        );
        output.textures_delta.clear();
        assert!(
            output.shapes.len() < 200,
            "only visible, readable grid lines should be emitted"
        );
    }
    use crate::app::Studio;
    use crate::tools::Tool;

    fn oval() -> Studio {
        let mut s = Studio::new();
        s.show_welcome = false;
        s.finish_create(
            CreateKind::Ellipse,
            Pt::new(40.0, 40.0),
            Pt::new(140.0, 100.0),
        );
        s.tool = Tool::Select;
        s
    }

    #[test]
    fn handle_hit_survives_grid_snap() {
        let mut s = oval();
        s.doc.grid.snap = true;
        s.doc.grid.size = 64.0;
        s.snap.enabled = true;
        s.snap.grid = true;
        s.snap.guides = false;
        s.snap.objects = false;
        s.snap.threshold = 40.0;
        let handle = posed_bounds(&s).unwrap().handle(1);
        let snapped = s.snap_pt(handle);
        assert!(
            (snapped - handle).length() > 8.0,
            "snap should pull off the handle so the pick/snap split is testable"
        );
        start_drag(&mut s, handle, snapped, false, false);
        assert!(
            matches!(s.op, Some(Op::Resize { handle: 1, .. })),
            "handle pick must use the raw pointer, not the snapped point"
        );
    }

    #[test]
    fn second_handle_drag_after_release() {
        let mut s = oval();
        let b = posed_bounds(&s).unwrap();
        start_drag(&mut s, b.handle(2), b.handle(2), false, false);
        continue_drag(&mut s, Pt::new(200.0, 160.0), false, false);
        end_drag(&mut s, Pt::new(200.0, 160.0), false, false, false);
        assert!(s.op.is_none(), "release must clear the live resize");
        let b2 = posed_bounds(&s).unwrap();
        start_drag(&mut s, b2.handle(0), b2.handle(0), false, false);
        assert!(matches!(s.op, Some(Op::Resize { handle: 0, .. })));
    }

    #[test]
    fn second_node_after_moving_the_first() {
        let mut s = Studio::new();
        s.show_welcome = false;
        s.finish_create(CreateKind::Rect, Pt::new(40.0, 40.0), Pt::new(140.0, 100.0));
        s.tool = Tool::Node;
        let (li, id) = s.primary().unwrap();
        s.ensure_path(li, id);
        let anchors = match &s.doc.find_shape(li, id).unwrap().geom {
            Geom::Path { anchors, .. } => anchors.clone(),
            _ => panic!("path"),
        };
        assert!(anchors.len() >= 2);
        start_drag(&mut s, anchors[0].pt, anchors[0].pt, false, false);
        continue_drag(&mut s, Pt::new(20.0, 20.0), false, false);
        end_drag(&mut s, Pt::new(20.0, 20.0), false, false, false);
        assert!(s.op.is_none());
        let anchors = match &s.doc.find_shape(li, id).unwrap().geom {
            Geom::Path { anchors, .. } => anchors.clone(),
            _ => panic!("path"),
        };
        start_drag(&mut s, anchors[1].pt, anchors[1].pt, false, false);
        match &s.op {
            Some(Op::Node {
                which: NodeHit::Point(1),
                ..
            }) => {}
            other => panic!("expected node 1, got op? {}", other.is_some()),
        }
    }

    #[test]
    fn pen_click_places_corner_without_drag() {
        let mut s = Studio::new();
        s.show_welcome = false;
        s.tool = Tool::Pen;
        start_drag(
            &mut s,
            Pt::new(20.0, 20.0),
            Pt::new(20.0, 20.0),
            false,
            false,
        );
        continue_drag(&mut s, Pt::new(21.0, 20.5), false, false);
        end_drag(&mut s, Pt::new(21.0, 20.5), false, false, false);
        start_drag(
            &mut s,
            Pt::new(80.0, 40.0),
            Pt::new(80.0, 40.0),
            false,
            false,
        );
        continue_drag(&mut s, Pt::new(80.5, 40.0), false, false);
        end_drag(&mut s, Pt::new(80.5, 40.0), false, false, false);
        match &s.op {
            Some(Op::Pen { anchors, .. }) => {
                assert_eq!(anchors.len(), 2);
                assert!(
                    anchors[0].is_corner(),
                    "jitter must not make a smooth point"
                );
                assert!(anchors[1].is_corner());
            }
            other => panic!("expected pen draft, got op? {}", other.is_some()),
        }
        if let Some(Op::Pen {
            anchors, source, ..
        }) = s.op.take()
        {
            s.finish_pen(anchors, false, source);
        }
        let n = s.doc.layers[1].kind.shapes().unwrap().len();
        assert_eq!(n, 1);
    }

    #[test]
    fn pen_drag_makes_smooth() {
        let mut s = Studio::new();
        s.show_welcome = false;
        s.tool = Tool::Pen;
        s.view.scale = 1.0;
        start_drag(
            &mut s,
            Pt::new(10.0, 10.0),
            Pt::new(10.0, 10.0),
            false,
            false,
        );
        continue_drag(&mut s, Pt::new(40.0, 10.0), false, false);
        match &s.op {
            Some(Op::Pen { anchors, .. }) => {
                assert!(!anchors[0].is_corner());
            }
            _ => panic!("pen"),
        }
    }

    #[test]
    fn node_shift_selects_two() {
        let mut s = Studio::new();
        s.show_welcome = false;
        s.finish_create(CreateKind::Rect, Pt::new(40.0, 40.0), Pt::new(140.0, 100.0));
        s.tool = Tool::Node;
        let (li, id) = s.primary().unwrap();
        s.ensure_path(li, id);
        let anchors = match &s.doc.find_shape(li, id).unwrap().geom {
            Geom::Path { anchors, .. } => anchors.clone(),
            _ => panic!("path"),
        };
        node_press(&mut s, anchors[0].pt, anchors[0].pt, false, false);
        end_drag(&mut s, anchors[0].pt, false, false, false);
        node_press(&mut s, anchors[1].pt, anchors[1].pt, true, false);
        assert_eq!(s.node_sel, BTreeSet::from([0, 1]));
    }

    #[test]
    fn node_press_grabs_bezier_handle() {
        let mut s = Studio::new();
        s.show_welcome = false;
        s.tool = Tool::Node;
        let a0 = Anchor::corner(Pt::new(0.0, 0.0));
        let a1 = Anchor::smooth(Pt::new(40.0, 0.0), Pt::new(20.0, 10.0));
        let shape = crate::document::Shape::new(
            Geom::Path {
                anchors: vec![a0, a1],
                closed: false,
            },
            crate::document::Style::default(),
        );
        let id = shape.id;
        s.commit(crate::document::Cmd::AddShape { layer: 1, shape });
        s.selection = vec![(1, id)];
        let handle = Pt::new(40.0, 0.0) + Pt::new(20.0, 10.0);
        node_press(&mut s, handle, handle, false, false);
        match &s.op {
            Some(Op::Node {
                which: NodeHit::HandleOut(1),
                ..
            }) => {}
            other => panic!(
                "expected handle-out grab, got {:?}",
                other.as_ref().map(|o| matches!(o, Op::Node { .. }))
            ),
        }
        continue_drag(&mut s, handle + Pt::new(10.0, 0.0), false, false);
        let Geom::Path { anchors, .. } = &s.doc.find_shape(1, id).unwrap().geom else {
            panic!("path");
        };
        assert!((anchors[1].h_out - Pt::new(30.0, 10.0)).length() < 0.5);
    }

    #[test]
    fn hit_node_handle_beats_segment() {
        let a0 = Anchor::corner(Pt::new(0.0, 0.0));
        let a1 = Anchor::smooth(Pt::new(40.0, 0.0), Pt::new(24.0, 0.0));
        let anchors = vec![a0, a1];
        let handle = Pt::new(64.0, 0.0);
        let hit = hit_node(&anchors, handle, 8.0, false);
        assert_eq!(hit, Some(NodeHit::HandleOut(1)));
    }

    #[test]
    fn select_press_locks_scale_handle_after_pointer_leaves() {
        let mut s = oval();
        let handle = posed_bounds(&s).unwrap().handle(1);
        start_drag(&mut s, handle, handle, false, false);
        assert!(matches!(s.op, Some(Op::Resize { handle: 1, .. })));
        continue_drag(&mut s, handle + Pt::new(24.0, 24.0), false, false);
        assert!(
            matches!(s.op, Some(Op::Resize { handle: 1, .. })),
            "leaving the handle must not steal the grab"
        );
    }

    #[test]
    fn select_press_locks_rotate_handle() {
        let mut s = oval();
        let rh = posed_bounds(&s).unwrap().rotate_handle();
        start_drag(&mut s, rh, rh, false, false);
        assert!(matches!(s.op, Some(Op::Rotate { .. })));
        continue_drag(&mut s, rh + Pt::new(20.0, 0.0), false, false);
        assert!(matches!(s.op, Some(Op::Rotate { .. })));
    }

    #[test]
    fn start_off_handle_does_not_grab_it() {
        let mut s = oval();
        let handle = posed_bounds(&s).unwrap().handle(1);
        let miss = handle + Pt::new(20.0, 20.0);
        start_drag(&mut s, miss, miss, false, false);
        assert!(
            !matches!(s.op, Some(Op::Resize { .. })),
            "hit-test 20px off the handle misses — that is the drag_started bug"
        );
    }

    #[test]
    fn radius_controls_belong_to_node_tool_and_edit_the_first_path_node() {
        let mut s = Studio::new();
        s.show_welcome = false;
        s.finish_create(CreateKind::Rect, Pt::new(40., 40.), Pt::new(140., 140.));
        let (li, id) = s.selection[0];
        s.tool = Tool::Select;
        assert!(hit_corner(&s, Pt::new(54., 54.)).is_none());
        s.tool = Tool::Node;
        s.ensure_path(li, id);
        let before = s.doc.find_shape(li, id).unwrap().geom.clone();
        let w = Pt::new(54., 54.);
        node_press(&mut s, w, w, false, false);
        assert!(matches!(s.op, Some(Op::Corner { which: Some(0), .. })));
        continue_drag(&mut s, w + Pt::splat(20.), false, false);
        let Geom::Path { anchors, .. } = &s.doc.find_shape(li, id).unwrap().geom else {
            panic!()
        };
        assert!(anchors[0].radius > 0.);
        assert_eq!(anchors[1].radius, 0.);
        end_drag(&mut s, w + Pt::splat(20.), false, false, false);
        s.undo();
        assert_eq!(s.doc.find_shape(li, id).unwrap().geom, before);
    }

    #[test]
    fn thin_line_press_starts_move_not_marquee() {
        let mut s = Studio::new();
        s.show_welcome = false;
        s.finish_create(CreateKind::Line, Pt::new(10.0, 40.0), Pt::new(120.0, 40.0));
        s.tool = Tool::Select;
        // Mid-edge bbox handles sit on a horizontal line. Pick a gap between them.
        start_drag(
            &mut s,
            Pt::new(30.0, 40.0),
            Pt::new(30.0, 40.0),
            false,
            false,
        );
        assert!(
            matches!(s.op, Some(Op::Move { .. })),
            "press on a thin stroke must lock Move, not marquee"
        );
    }

    #[test]
    fn alt_shift_shapes_stay_centered_and_ellipse_has_four_smooth_anchors() {
        for tool in [Tool::Rect, Tool::Ellipse, Tool::Polygon, Tool::Star] {
            let mut s = Studio::new();
            s.show_welcome = false;
            s.tool = tool;
            s.snap.enabled = false;
            let center = Pt::new(150., 130.);
            let end = Pt::new(190., 150.);
            start_drag(&mut s, center, center, true, true);
            continue_drag(&mut s, end, true, true);
            assert!(matches!(
                s.op,
                Some(Op::Create {
                    from_center: true,
                    ..
                })
            ));
            end_drag(&mut s, end, true, true, false);
            let (li, id) = s.primary().unwrap();
            let shape = s.doc.find_shape(li, id).unwrap();
            if matches!(tool, Tool::Rect | Tool::Ellipse) {
                assert_eq!(shape.geom.bbox().center(), center);
                assert_eq!(shape.geom.bbox().size(), Pt::new(80., 80.));
            }
            if tool == Tool::Ellipse {
                s.ensure_path(li, id);
                let Geom::Path { anchors, closed } = &s.doc.find_shape(li, id).unwrap().geom else {
                    panic!("ellipse path")
                };
                assert!(*closed);
                assert_eq!(anchors.len(), 4);
                for a in anchors {
                    assert!((a.h_in.length() - 40. * 0.5522848).abs() < 0.001);
                    assert!((a.h_in + a.h_out).length() < 0.001);
                }
            }
        }
    }

    #[test]
    fn canvas_hit_moves_a_group_until_an_item_is_explicitly_entered() {
        let mut s = Studio::new();
        s.show_welcome = false;
        s.snap.enabled = false;
        s.finish_create(CreateKind::Rect, Pt::new(40., 40.), Pt::new(80., 80.));
        let a = s.primary().unwrap();
        s.finish_create(CreateKind::Rect, Pt::new(120., 40.), Pt::new(160., 80.));
        let b = s.primary().unwrap();
        s.selection = vec![a, b];
        s.group_selected();
        s.deselect_all();
        s.tool = Tool::Select;
        let hit = Pt::new(55., 60.);
        start_drag(&mut s, hit, hit, false, false);
        assert_eq!(s.selection.len(), 2);
        let orig = s
            .selection
            .iter()
            .map(|&(li, id)| s.doc.find_shape(li, id).unwrap().geom.bbox().min)
            .collect::<Vec<_>>();
        continue_drag(&mut s, hit + Pt::new(15., 20.), false, false);
        end_drag(&mut s, hit + Pt::new(15., 20.), false, false, false);
        for (i, &(li, id)) in s.selection.iter().enumerate() {
            assert_eq!(
                s.doc.find_shape(li, id).unwrap().geom.bbox().min,
                orig[i] + Pt::new(15., 20.)
            );
        }
        let entered = s.selection[0];
        s.enter_group_item(entered);
        assert_eq!(s.selection_for_hit(entered), vec![entered]);
        s.deselect_all();
        assert_eq!(s.selection_for_hit(entered).len(), 2);
    }

    #[test]
    fn rect_create_records_press_point() {
        let mut s = Studio::new();
        s.show_welcome = false;
        s.tool = Tool::Rect;
        let press = Pt::new(12.0, 18.0);
        start_drag(&mut s, press, press, false, false);
        match &s.op {
            Some(Op::Create {
                start,
                cur,
                kind: CreateKind::Rect,
                ..
            }) => {
                assert_eq!(*start, press);
                assert_eq!(*cur, press);
            }
            other => panic!("expected create at press, got op? {}", other.is_some()),
        }
    }

    #[test]
    fn click_resize_does_not_dirty() {
        let mut s = oval();
        s.dirty = false;
        let n = s.history.len();
        let handle = posed_bounds(&s).unwrap().handle(1);
        start_drag(&mut s, handle, handle, false, false);
        end_drag(&mut s, handle, false, false, false);
        assert_eq!(
            s.history.len(),
            n,
            "clicking a handle without moving must not commit"
        );
        assert!(!s.dirty);
    }
    #[test]
    fn shift_move_and_alt_duplicate_keep_the_original_and_undo_the_move() {
        let mut studio = Studio::new();
        studio.show_welcome = false;
        studio.snap.enabled = false;
        studio.finish_create(CreateKind::Rect, Pt::new(40.0, 40.0), Pt::new(140.0, 100.0));
        studio.tool = Tool::Select;
        let original = studio.primary().unwrap();
        let before = studio
            .doc
            .find_shape(original.0, original.1)
            .unwrap()
            .geom
            .clone();
        let start = Pt::new(90.0, 70.0);
        start_drag(&mut studio, start, start, true, true);
        let copy = studio.primary().unwrap();
        assert_ne!(copy, original);
        continue_drag(&mut studio, start + Pt::new(36.0, 18.0), true, true);
        let copied_bounds = studio.doc.find_shape(copy.0, copy.1).unwrap().world_bbox();
        let delta = copied_bounds.min - Pt::new(40.0, 40.0);
        assert!((delta.x - delta.y).abs() < 0.001);
        assert!(delta.x > 20.0);
        assert_eq!(
            studio.doc.find_shape(original.0, original.1).unwrap().geom,
            before
        );
        end_drag(&mut studio, start + Pt::new(36.0, 18.0), true, false, true);
        studio.undo();
        assert_eq!(studio.doc.find_shape(copy.0, copy.1).unwrap().geom, before);
        studio.undo();
        assert!(studio.doc.find_shape(copy.0, copy.1).is_none());
        assert_eq!(
            studio.doc.find_shape(original.0, original.1).unwrap().geom,
            before
        );
    }

    #[test]
    fn shift_artboard_move_constrains_the_board_and_its_contents_together() {
        let mut studio = Studio::new();
        studio.snap.enabled = false;
        studio.doc.artboards = vec![crate::document::Artboard::new(
            0,
            Pt::new(40.0, 40.0),
            Pt::new(200.0, 160.0),
        )];
        let board_id = studio.doc.artboards[0].id;
        studio.finish_create(CreateKind::Rect, Pt::new(90.0, 90.0), Pt::new(110.0, 110.0));
        let object = studio.primary().unwrap();
        studio.tool = Tool::Artboard;
        studio.selection.clear();
        let start = Pt::new(45.0, 120.0);
        start_drag(&mut studio, start, start, true, false);
        assert!(matches!(studio.op, Some(Op::ArtboardMove { .. })));
        continue_drag(&mut studio, start + Pt::new(34.0, 17.0), true, false);
        let board = studio
            .doc
            .artboards
            .iter()
            .find(|b| b.id == board_id)
            .unwrap();
        let delta = board.origin - Pt::new(40.0, 40.0);
        assert!((delta.x - delta.y).abs() < 0.001);
        assert!(delta.x > 20.0);
        let moved = studio
            .doc
            .find_shape(object.0, object.1)
            .unwrap()
            .world_bbox()
            .min;
        assert!((moved - Pt::new(90.0, 90.0) - delta).length() < 0.001);
        end_drag(&mut studio, start + delta, false, false, true);
        studio.undo();
        assert_eq!(studio.doc.artboards[0].origin, Pt::new(40.0, 40.0));
        assert_eq!(
            studio
                .doc
                .find_shape(object.0, object.1)
                .unwrap()
                .world_bbox()
                .min,
            Pt::new(90.0, 90.0)
        );
    }

    #[test]
    fn shift_brush_and_pen_handles_follow_a_fixed_45_degree_anchor() {
        let mut studio = Studio::new();
        studio.snap.enabled = false;
        studio.add_layer(true);
        studio.tool = Tool::Brush;
        studio.selection.clear();
        let start = Pt::new(40.0, 40.0);
        start_drag(&mut studio, start, start, true, false);
        continue_drag(&mut studio, Pt::new(65.0, 54.0), true, false);
        continue_drag(&mut studio, Pt::new(80.0, 62.0), true, false);
        let Some(Op::Brush {
            last: Some(last),
            buf,
            ..
        }) = &studio.op
        else {
            panic!("brush stroke");
        };
        assert!(((last.x - start.x) - (last.y - start.y)).abs() < 0.001);
        let drawn = buf
            .pixels()
            .iter()
            .filter(|pixel| pixel.alpha() > 0)
            .count();
        assert!(drawn > 40);
        end_drag(&mut studio, Pt::new(80.0, 62.0), false, false, true);
        assert!(studio.stroke_constraint.is_none());
        studio.tool = Tool::Pen;
        studio.pen_click_at(start, start);
        continue_drag(&mut studio, Pt::new(70.0, 56.0), true, false);
        let Some(Op::Pen { anchors, .. }) = &studio.op else {
            panic!("pen handle");
        };
        assert!((anchors[0].h_out.x - anchors[0].h_out.y).abs() < 0.001);
        assert!((anchors[0].h_out + anchors[0].h_in).length() < 0.001);
    }

    #[test]
    fn ctrl_changes_snapping_during_the_same_pointer_drag() {
        use eframe::egui::{Event, Modifiers};
        for enabled in [false, true] {
            let ctx = eframe::egui::Context::default();
            let mut studio = Studio::new();
            studio.show_welcome = false;
            studio.need_fit = false;
            studio.snap = crate::snap::SnapSettings {
                enabled,
                grid: false,
                guides: true,
                objects: false,
                artboards: false,
                spacing: false,
                threshold: 6.0,
            };
            studio.doc.guides = vec![crate::document::Guide {
                vertical: true,
                pos: 180.0,
            }];
            studio.finish_create(CreateKind::Rect, Pt::new(40.0, 40.0), Pt::new(140.0, 100.0));
            studio.tool = Tool::Select;
            let object = studio.primary().unwrap();
            let start = pos2(90.0, 70.0);
            canvas_frame(&ctx, &mut studio, vec![Event::PointerMoved(start)]);
            canvas_frame(
                &ctx,
                &mut studio,
                vec![Event::PointerButton {
                    pos: start,
                    button: PointerButton::Primary,
                    pressed: true,
                    modifiers: Modifiers::NONE,
                }],
            );
            canvas_frame(
                &ctx,
                &mut studio,
                vec![Event::PointerMoved(pos2(126.0, 70.0))],
            );
            let x = |s: &Studio| {
                s.doc
                    .find_shape(object.0, object.1)
                    .unwrap()
                    .world_bbox()
                    .min
                    .x
            };
            assert!((x(&studio) - if enabled { 80.0 } else { 76.0 }).abs() < 0.001);
            canvas_frame(
                &ctx,
                &mut studio,
                vec![Event::ModifiersChanged(
                    Modifiers::CTRL | Modifiers::COMMAND,
                )],
            );
            assert!((x(&studio) - if enabled { 76.0 } else { 80.0 }).abs() < 0.001);
            canvas_frame(
                &ctx,
                &mut studio,
                vec![Event::ModifiersChanged(Modifiers::NONE)],
            );
            assert!((x(&studio) - if enabled { 80.0 } else { 76.0 }).abs() < 0.001);
            canvas_frame(
                &ctx,
                &mut studio,
                vec![Event::PointerButton {
                    pos: pos2(126.0, 70.0),
                    button: PointerButton::Primary,
                    pressed: false,
                    modifiers: Modifiers::NONE,
                }],
            );
            studio.undo();
            assert!((x(&studio) - 40.0).abs() < 0.001);
        }
    }
}

#[cfg(test)]
mod object_guide_interaction_tests {
    use super::*;
    use eframe::egui::{Context, Event, Modifiers, RawInput};

    #[test]
    fn object_guide_moves_with_the_selection_tool_and_undo_keeps_it_a_guide() {
        let ctx = Context::default();
        let mut studio = Studio::new();
        studio.doc.ruler.guides_locked = false;
        studio.show_welcome = false;
        studio.show_rulers = false;
        studio.need_fit = false;
        studio.view.scale = 1.0;
        studio.view.offset = Pt::ZERO;
        studio.snap.enabled = false;
        studio.tool = Tool::Select;
        studio.doc.layers = vec![crate::document::Layer::vector("Guides")];
        let mut guide = crate::document::Shape::new(
            Geom::Line {
                a: Pt::new(40.0, 100.0),
                b: Pt::new(140.0, 100.0),
            },
            crate::document::Style::default(),
        );
        guide.guide = true;
        let id = guide.id;
        let original = guide.geom.clone();
        studio.doc.layers[0].kind.shapes_mut().unwrap().push(guide);
        let frame = |studio: &mut Studio, events: Vec<Event>| {
            let mut output = ctx.run_ui(
                RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(400.0, 300.0))),
                    events,
                    ..Default::default()
                },
                |ui| show(ui, studio),
            );
            output.textures_delta.clear();
        };
        frame(&mut studio, vec![Event::PointerMoved(pos2(80.0, 100.0))]);
        frame(
            &mut studio,
            vec![Event::PointerButton {
                pos: pos2(80.0, 100.0),
                button: PointerButton::Primary,
                pressed: true,
                modifiers: Modifiers::NONE,
            }],
        );
        frame(&mut studio, vec![Event::PointerMoved(pos2(110.0, 130.0))]);
        frame(
            &mut studio,
            vec![Event::PointerButton {
                pos: pos2(110.0, 130.0),
                button: PointerButton::Primary,
                pressed: false,
                modifiers: Modifiers::NONE,
            }],
        );
        let moved = studio.doc.find_shape(0, id).unwrap();
        assert!(moved.guide);
        assert_eq!(
            moved.geom,
            Geom::Line {
                a: Pt::new(70.0, 130.0),
                b: Pt::new(170.0, 130.0)
            }
        );
        studio.undo();
        let restored = studio.doc.find_shape(0, id).unwrap();
        assert!(restored.guide);
        assert_eq!(restored.geom, original);
    }
}

#[cfg(test)]
#[path = "canvas_node_tests.rs"]
mod node_tests;

#[cfg(test)]
#[path = "canvas_layout_tests.rs"]
mod layout_tests;
