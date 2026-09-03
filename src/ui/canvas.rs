use crate::app::{from_egui, to_egui, CreateKind, NodeHit, ObjSnap, Op, Studio};
use crate::compositor::{self, Draft};
use crate::document::{Fill, RASTER_ID};
use crate::geom::{insert_anchor, Anchor, Bounds, Geom, Pt};
use std::collections::BTreeSet;
use crate::paint;
use crate::tools::{Persona, Tool};
use crate::ui::theme::{accent, bg_panel, border, fg_weak, select, select_fill};
use eframe::egui::{
    Color32, PointerButton, Pos2, Rect, Sense, Stroke, Ui, Vec2, pos2, vec2,
};

pub fn show(ui: &mut Ui, studio: &mut Studio) {
    let (rect, resp) = ui.allocate_exact_size(ui.available_size(), Sense::click_and_drag());
    studio.canvas_rect = Some(rect);
    if studio.type_edit.is_some() {
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
    let pinch = (zoom_delta - 1.0).abs() > 1e-4;
    let scroll_zoom = scroll.y.abs() > 0.0 && (ctrl || alt || studio.tool == Tool::Zoom);
    if pinch {
        studio.view.zoom_at(local, zoom_delta);
        ctx.request_repaint();
    } else if scroll_zoom {
        studio.view.zoom_at(local, (scroll.y / 200.0).exp());
    } else if scroll != Vec2::ZERO && !ctrl {
        studio.view.offset.x += scroll.x;
        studio.view.offset.y += scroll.y;
    }
    if let Some(hp) = pointer {
        studio.cursor = Some(studio.view.to_world(from_egui(hp) - origin));
    }

    let shift = ctx.input(|i| i.modifiers.shift);
    if live_op_should_close(studio, &resp) {
        end_drag(
            studio,
            studio.cursor.unwrap_or(Pt::ZERO),
            alt,
            ctrl,
            shift,
        );
    }

    let panning = (space_pan && studio.type_edit.is_none()) || studio.tool == Tool::Hand;
    if panning && resp.dragged_by(PointerButton::Primary)
        || resp.dragged_by(PointerButton::Middle)
    {
        let d = resp.drag_delta();
        studio.view.offset.x += d.x;
        studio.view.offset.y += d.y;
    } else if studio.show_rulers
        && resp
            .interact_pointer_pos()
            .is_some_and(|p| p.x < rect.min.x + 18.0 || p.y < rect.min.y + 18.0)
    {
        // Clicks on the ruler add guides; don't start a tool drag.
    } else {
        handle_pointer(studio, &resp, space_pan);
    }

    let w = rect.width().max(1.0) as u32;
    let h = rect.height().max(1.0) as u32;
    let op_tag: u8 = match &studio.op {
        Some(Op::Brush { .. }) => 1,
        Some(Op::Smudge { .. } | Op::Clone { .. }) => 2,
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
    if studio.show_rulers {
        draw_rulers(&painter, rect, studio);
    }
    if studio.show_grid || studio.doc.grid.visible {
        draw_grid(&painter, rect, studio);
    }
    draw_guides(&painter, rect, studio);
    draw_artboard_frames(&painter, rect, studio);
    draw_bleed_safe(&painter, rect, studio);
    draw_overlays(&painter, rect, studio);
    set_cursor(ui, studio, &resp);
    context_menu(&resp, studio);
    ruler_guides(studio, &resp, rect);

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
        CreateKind::Rect => Geom::Rect {
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
    let snap = if is_text { pick } else { studio.snap_pt(pick) };
    let alt = resp.ctx.input(|i| i.modifiers.alt);
    let shift = resp.ctx.input(|i| i.modifiers.shift);
    let ctrl = resp.ctx.input(|i| i.modifiers.ctrl || i.modifiers.command);

    if studio.tool == Tool::Zoom && resp.clicked() && !resp.dragged() {
        studio.zoom_click(from_egui(screen) - origin, alt, ctrl, shift);
        return;
    }

    if studio.tool == Tool::Eyedropper && resp.clicked() {
        studio.eyedrop(pick);
        return;
    }

    if studio.pending_place.is_some() {
        if resp.drag_started_by(PointerButton::Primary) {
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
        if let Some(li) = studio.raster_target() {
            if let Some(px) = studio.doc.layers[li].kind.pixels_mut() {
                if let Some(mut pm) = px.to_pixmap() {
                    let before = px.data.clone();
                    let c = match studio.style.fill {
                        Fill::Solid(c) => c,
                        _ => studio.brush.color,
                    };
                    paint::flood_fill(&mut pm, pick, c, studio.fill_tolerance);
                    *px = crate::document::Pixels::from_pixmap(&pm);
                    let after = px.data.clone();
                    studio.commit(crate::document::Cmd::Pixels {
                        layer: li,
                        mask: false,
                        before,
                        after,
                    });
                }
            }
        }
        return;
    }

    if studio.tool == Tool::Wand && resp.clicked() {
        if let Some(li) = studio.raster_target() {
            if let Some(px) = studio.doc.layers[li].kind.pixels() {
                if let Some(pm) = px.to_pixmap() {
                    studio.pixel_sel = Some(paint::wand_mask(&pm, pick, studio.fill_tolerance));
                    studio.status = "wand selection".into();
                }
            }
        }
        return;
    }

    if studio.tool == Tool::Clone && alt && resp.clicked() {
        studio.clone_source = Some(pick);
        studio.status = "clone source set".into();
        return;
    }

    // Text edit drag: selecting a range should not commit, but update the caret.
    if studio.type_edit.is_some() {
        if resp.drag_started_by(PointerButton::Primary) {
            let slack = 8.0 / studio.view.scale.max(0.01);
            if let Some(hit) = hit_shape(studio, pick, slack) {
                if studio.editing_text(hit.0, hit.1) {
                    let caret = studio.doc.find_shape(hit.0, hit.1).and_then(|s| match &s.geom {
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
            studio.commit_type_edit();
        } else if resp.dragged_by(PointerButton::Primary) {
            if let Some(edit) = studio.type_edit.as_ref() {
                let (li, id) = (edit.layer, edit.id);
                if let Some(shape) = studio.doc.find_shape(li, id) {
                    if let Geom::Text(run) = &shape.geom {
                        let new_caret = crate::text::hit_char(run, pick);
                        if let Some(e) = studio.type_edit.as_mut() {
                            e.caret = new_caret;
                        }
                    }
                }
            }
            return;
        }
    }

    // Pen places on pointer *down*. The canvas uses Sense::click_and_drag, so
    // egui withholds drag_started until the pointer moves — a plain click never
    // reached start_drag, and close-on-first-point died with it.
    if studio.tool == Tool::Pen {
        if resp.double_clicked() {
            if let Some(Op::Pen { anchors, source, .. }) = studio.op.take() {
                studio.finish_pen(anchors, false, source);
            }
            return;
        }
        if resp.ctx.input(|i| i.pointer.primary_pressed()) {
            studio.pen_click_at(pick, snap);
        }
        if resp.dragged_by(PointerButton::Primary) {
            continue_drag(studio, pick, shift, alt);
        }
        return;
    }

    // Same click-vs-drag delay: handles are ~3px. Waiting for drag_started
    // means the pointer has already left the handle, and a segment steal wins.
    if studio.tool == Tool::Node {
        if resp.ctx.input(|i| i.pointer.primary_pressed()) {
            node_press(studio, pick, snap, shift, alt);
        }
        if resp.drag_started_by(PointerButton::Primary) && studio.op.is_none() {
            start_drag(studio, pick, snap, shift, alt);
        }
        if resp.dragged_by(PointerButton::Primary) {
            continue_drag(studio, pick, shift, alt);
        }
        if resp.clicked() && !resp.dragged() && !resp.drag_stopped() {
            click(studio, pick, shift, alt);
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

    if resp.drag_started_by(PointerButton::Primary) {
        if studio.op.is_some() {
            end_drag(studio, snap, alt, ctrl, shift);
        }
        if studio.op.is_none() {
            start_drag(studio, pick, snap, shift, alt);
        }
    }

    if resp.dragged_by(PointerButton::Primary) {
        let world = if matches!(studio.tool, Tool::Node | Tool::Artboard) {
            pick
        } else {
            snap
        };
        continue_drag(studio, world, shift, alt);
    }

    if resp.clicked() && !resp.dragged() && !resp.drag_stopped() {
        click(studio, pick, shift, alt);
    }

    if resp.double_clicked() {
        if let Some(Op::Pen { anchors, source, .. }) = studio.op.take() {
            studio.finish_pen(anchors, false, source);
        } else {
            let slack = 8.0 / studio.view.scale.max(0.01);
            if let Some(hit) = hit_shape(studio, pick, slack)
                && is_text_hit(studio, hit)
            {
                studio.begin_type_edit(hit, pick);
            }
        }
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
            Tool::Hand | Tool::Zoom | Tool::Node | Tool::Pen | Tool::Trace | Tool::Artboard
        )
        && !studio.selection.is_empty()
    {
        if let Some((li, id, which)) = hit_corner(studio, pick) {
            if let Some(s) = studio.doc.find_shape(li, id) {
                let radius = match s.geom {
                    Geom::Rect { radius, .. } => radius,
                    _ => 0.0,
                };
                studio.op = Some(Op::Corner {
                    layer: li,
                    id,
                    which,
                    orig_corners: s.corners,
                    orig_radius: radius,
                    orig_geom: s.geom.clone(),
                    start: pick,
                });
                return;
            }
        }
        if let Some(sel) = hit_handle(studio, pick) {
            match sel {
                HandleKind::Rotate(center) => {
                    let orig = snapshot(studio);
                    studio.op = Some(Op::Rotate {
                        orig,
                        center,
                        start_angle: (pick - center).y.atan2((pick - center).x),
                    });
                    return;
                }
                HandleKind::Scale(i, b) => {
                    studio.op = Some(Op::Resize {
                        orig: snapshot(studio),
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
                if !studio.selection.contains(&hit) {
                    if shift {
                        studio.selection.push(hit);
                    } else {
                        studio.selection = vec![hit];
                    }
                }
                studio.active_layer = Some(hit.0);
                if alt {
                    studio.duplicate_selection_by(Pt::ZERO);
                }
                studio.op = Some(Op::Move {
                    orig: snapshot(studio),
                    start: snap,
                });
            } else {
                studio.op = Some(Op::Marquee {
                    start: snap,
                    cur: snap,
                    ellipse: false,
                });
                if !shift {
                    studio.selection.clear();
                }
            }
        }
        Tool::Node => {
            if let Some((li, id)) = studio.primary() {
                if id != RASTER_ID {
                    studio.ensure_path(li, id);
                }
                if let Some(shape) = studio.doc.find_shape(li, id) {
                    if let Geom::Path { anchors, closed } = &shape.geom {
                        let slack = 8.0 / studio.view.scale.max(0.01);
                        if let Some(hit) = hit_node(anchors, pick, slack, *closed) {
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
                                    moving.insert((seg + 1) % anchors.len().max(1));
                                    if !closed && seg + 1 >= anchors.len() {
                                        moving.remove(&(seg + 1));
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
                        let _ = closed;
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
            if let Some(id) = studio.doc.artboard_hit(pick, 8.0 / studio.view.scale.max(0.01)) {
                if !studio.artboard_sel.contains(&id) {
                    studio.artboard_sel = vec![id];
                }
                if alt {
                    studio.clone_artboard(id);
                }
                studio.op = Some(Op::ArtboardMove {
                    orig: studio.doc.artboards.clone(),
                    ids: studio.artboard_sel.clone(),
                    start: snap,
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
                kind: CreateKind::Rect,
                start: snap,
                cur: snap,
            })
        }
        Tool::Ellipse => {
            studio.op = Some(Op::Create {
                kind: CreateKind::Ellipse,
                start: snap,
                cur: snap,
            })
        }
        Tool::Polygon => {
            studio.op = Some(Op::Create {
                kind: CreateKind::Polygon,
                start: snap,
                cur: snap,
            })
        }
        Tool::Star => {
            studio.op = Some(Op::Create {
                kind: CreateKind::Star,
                start: snap,
                cur: snap,
            })
        }
        Tool::Line => {
            studio.op = Some(Op::Create {
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
        Tool::Brush | Tool::Eraser => {
            if let Some(li) = studio.raster_target() {
                if let Some(px) = studio.doc.layers[li].kind.pixels() {
                    if let Some(buf) = PixmapOr::new(px.w, px.h) {
                        let mut buf = buf;
                        paint::stamp(&mut buf, pick, &studio.brush, studio.tool == Tool::Eraser);
                        studio.op = Some(Op::Brush {
                            layer: li,
                            erase: studio.tool == Tool::Eraser,
                            buf,
                            last: Some(pick),
                            before: px.data.clone(),
                        });
                    }
                }
            } else {
                studio.status = "add a pixel layer to paint".into();
            }
        }
        Tool::Smudge => {
            if let Some(li) = studio.raster_target() {
                if let Some(px) = studio.doc.layers[li].kind.pixels() {
                    studio.op = Some(Op::Smudge {
                        layer: li,
                        last: Some(pick),
                        before: px.data.clone(),
                    });
                }
            }
        }
        Tool::Clone => {
            if studio.clone_source.is_none() {
                studio.status = "Alt-click to set clone source".into();
                return;
            }
            if let Some(li) = studio.raster_target() {
                if let Some(px) = studio.doc.layers[li].kind.pixels() {
                    studio.op = Some(Op::Clone {
                        layer: li,
                        last: Some(pick),
                        before: px.data.clone(),
                    });
                }
            }
        }
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

struct PixmapOr;
impl PixmapOr {
    fn new(w: u32, h: u32) -> Option<tiny_skia::Pixmap> {
        tiny_skia::Pixmap::new(w, h)
    }
}

fn commit_obj_snaps(studio: &mut Studio, orig: Vec<ObjSnap>) {
    for snap in orig {
        if snap.id == RASTER_ID {
            if let Some(l) = studio.doc.layers.get(snap.layer) {
                if let Some((o, sz, rot)) = l.kind.raster_xform() {
                    studio.history.push(crate::document::Cmd::SetRasterXform {
                        layer: snap.layer,
                        before: (snap.origin, snap.size, snap.rot),
                        after: (o, sz, rot),
                    });
                    studio.dirty = true;
                }
            }
        } else if let (Some(geom), Some(s)) = (
            snap.geom.clone(),
            studio.doc.find_shape(snap.layer, snap.id),
        ) {
            studio.history.push(crate::document::Cmd::SetGeom {
                layer: snap.layer,
                id: snap.id,
                before: geom,
                after: s.geom.clone(),
                rot_before: snap.rot,
                rot_after: s.rotation,
            });
            studio.dirty = true;
        }
    }
}

fn snapshot(studio: &Studio) -> Vec<ObjSnap> {
    studio
        .selection
        .iter()
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
    let Some(shape) = studio.doc.find_shape(li, id) else {
        return;
    };
    let Geom::Path { anchors, closed } = &shape.geom else {
        return;
    };
    let slack = 8.0 / studio.view.scale.max(0.01);
    let Some(hit) = hit_node(anchors, pick, slack, *closed) else {
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
        NodeHit::Segment(_) => {}
    }
}

fn hit_artboard_handle(studio: &Studio, pick: Pt) -> Option<Op> {
    let slack = 12.0 / studio.view.scale.max(0.01);
    for id in &studio.artboard_sel {
        let Some(a) = studio.doc.artboards.iter().find(|x| x.id == *id) else {
            continue;
        };
        let b = a.bounds();
        let rh = b.rotate_handle();
        if (rh - pick).length() <= slack {
            return Some(Op::ArtboardRotate {
                orig: a.clone(),
                center: b.center(),
                start_angle: (pick - b.center()).y.atan2((pick - b.center()).x),
            });
        }
        for i in 0..8 {
            if (b.handle(i) - pick).length() <= slack {
                return Some(Op::ArtboardResize {
                    orig: a.clone(),
                    handle: i,
                    start_box: b,
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
    if studio.selection.len() != 1 {
        return None;
    }
    let (li, id) = studio.selection[0];
    let s = studio.doc.find_shape(li, id)?;
    let slack = 8.0 / studio.view.scale.max(0.01);
    match &s.geom {
        Geom::Rect { origin, size, .. } => {
            let widgets = crate::geom::corner_widgets(*origin, *size);
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
        Geom::Path { anchors, .. } if studio.tool == Tool::Node => {
            for &i in &studio.node_sel {
                if let Some(a) = anchors.get(i) {
                    let dir = Pt::new(8.0, 8.0) / studio.view.scale.max(0.01);
                    if (a.pt + dir - world).length() <= slack {
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
        )
    ) {
        studio.mark();
    }
    match &mut studio.op {
        Some(Op::Create { start, cur, kind }) => {
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
                let snapped = (ang / std::f32::consts::FRAC_PI_4).round() * std::f32::consts::FRAC_PI_4;
                let len = d.length();
                c = *start + Pt::new(len * snapped.cos(), len * snapped.sin());
            }
            *cur = c;
        }
        Some(Op::Pen { anchors, press, .. }) => {
            if let Some(last) = anchors.last_mut() {
                crate::geom::apply_pen_smooth(
                    last,
                    world - *press,
                    studio.view.scale,
                    alt,
                    shift,
                );
            }
        }
        Some(Op::Pencil { pts }) => {
            if pts.last().map(|p| (*p - world).length() > 1.5).unwrap_or(true) {
                pts.push(world);
            }
        }
        Some(Op::Move { orig, start }) => {
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
                for snap in orig.clone() {
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
                            l.kind
                                .set_raster_xform(snap.origin + (c1 - c0), snap.size, snap.rot + ang);
                        }
                    } else if let (Some(geom), Some(s)) = (
                        snap.geom.clone(),
                        studio.doc.find_shape_mut(snap.layer, snap.id),
                    ) {
                        s.geom = geom;
                        s.geom.rotate_about(*center, ang);
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
                s.geom = orig;
                if let Geom::Path { anchors, .. } = &mut s.geom {
                    match which {
                        NodeHit::Point(i) => {
                            let Some(start) = anchors.get(i).map(|a| a.pt) else {
                                return;
                            };
                            let d = world - start;
                            for idx in moving {
                                if let Some(a) = anchors.get_mut(idx) {
                                    a.pt += d;
                                }
                            }
                        }
                        NodeHit::Segment(seg) => {
                            let Some(start) = anchors.get(seg).map(|a| a.pt) else {
                                return;
                            };
                            let d = world - start;
                            for idx in moving {
                                if let Some(a) = anchors.get_mut(idx) {
                                    a.pt += d;
                                }
                            }
                        }
                        NodeHit::HandleIn(i) => {
                            if let Some(a) = anchors.get_mut(i) {
                                let mut h = world - a.pt;
                                if shift {
                                    h = crate::geom::constrain_45(h);
                                }
                                a.h_in = h;
                                if !alt {
                                    a.h_out = -a.h_in;
                                }
                            }
                        }
                        NodeHit::HandleOut(i) => {
                            if let Some(a) = anchors.get_mut(i) {
                                let mut h = world - a.pt;
                                if shift {
                                    h = crate::geom::constrain_45(h);
                                }
                                a.h_out = h;
                                if !alt {
                                    a.h_in = -a.h_out;
                                }
                            }
                        }
                    }
                }
            }
        }
        Some(Op::Brush {
            buf,
            last,
            erase,
            ..
        }) => {
            if let Some(prev) = *last {
                paint::stroke_to(buf, prev, world, &studio.brush, *erase);
            } else {
                paint::stamp(buf, world, &studio.brush, *erase);
            }
            *last = Some(world);
        }
        Some(Op::Smudge { layer, last, .. }) => {
            let li = *layer;
            let prev = *last;
            if let Some(px) = studio.doc.layers.get_mut(li).and_then(|l| l.kind.pixels_mut()) {
                if let Some(mut pm) = px.to_pixmap() {
                    if let Some(p0) = prev {
                        paint::smudge(&mut pm, p0, world, &studio.brush);
                    }
                    *px = crate::document::Pixels::from_pixmap(&pm);
                }
            }
            if let Some(Op::Smudge { last, .. }) = &mut studio.op {
                *last = Some(world);
            }
        }
        Some(Op::Clone { layer, last, .. }) => {
            let li = *layer;
            let src = studio.clone_source;
            if let (Some(source), Some(px)) = (
                src,
                studio.doc.layers.get_mut(li).and_then(|l| l.kind.pixels_mut()),
            ) {
                if let Some(mut pm) = px.to_pixmap() {
                    paint::clone_stamp(&mut pm, world, source, &studio.brush);
                    *px = crate::document::Pixels::from_pixmap(&pm);
                }
            }
            let _ = last;
        }
        Some(Op::Marquee { cur, .. }) => *cur = world,
        Some(Op::Lasso { pts }) => pts.push(world),
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
            orig_corners,
            orig_radius,
            orig_geom,
            start,
        }) => {
            let layer = *layer;
            let id = *id;
            let which = *which;
            let orig_corners = *orig_corners;
            let orig_radius = *orig_radius;
            let orig_geom = orig_geom.clone();
            let start = *start;
            if let Some(s) = studio.doc.find_shape_mut(layer, id) {
                s.geom = orig_geom;
                let dist = (world - start).length();
                let sign = if (world - start).dot(Pt::new(1.0, 1.0)) < 0.0 {
                    1.0
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
                        let mut c = orig_corners;
                        if c.iter().all(|x| *x < 0.05) {
                            c = [orig_radius; 4];
                        }
                        if i < 4 {
                            c[i] = r;
                        } else if let Geom::Path { anchors, .. } = &mut s.geom {
                            if let Some(a) = anchors.get_mut(i) {
                                a.radius = r;
                            }
                        }
                        s.corners = c;
                        if let Geom::Rect { radius, .. } = &mut s.geom {
                            *radius = c.iter().copied().fold(0.0, f32::max);
                        }
                    }
                }
            }
        }
        Some(Op::ArtboardMove { orig, ids, start }) => {
            let d = world - *start;
            let orig = orig.clone();
            let ids = ids.clone();
            studio.doc.artboards = orig;
            for a in &mut studio.doc.artboards {
                if ids.contains(&a.id) {
                    a.origin += d;
                }
            }
        }
        Some(Op::ArtboardResize {
            orig,
            handle,
            start_box,
        }) => {
            let orig = orig.clone();
            let handle = *handle;
            let start_box = *start_box;
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
                min.x = world.x;
            } else if hx == 1.0 {
                max.x = world.x;
            }
            if hy == 0.0 {
                min.y = world.y;
            } else if hy == 1.0 {
                max.y = world.y;
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
        }
        Some(Op::ArtboardRotate {
            orig,
            center,
            start_angle,
        }) => {
            let mut ang = (world - *center).y.atan2((world - *center).x) - *start_angle;
            if shift {
                let step = std::f32::consts::PI / 12.0;
                ang = (ang / step).round() * step;
            }
            let orig = orig.clone();
            if let Some(a) = studio.doc.artboards.iter_mut().find(|a| a.id == orig.id) {
                a.origin = orig.origin;
                a.size = orig.size;
                a.rotation = orig.rotation + ang;
            }
        }
        None => {}
    }
    studio.sync_pen_source();
}

fn end_drag(studio: &mut Studio, world: Pt, alt: bool, ctrl: bool, shift: bool) {
    match studio.op.take() {
        Some(Op::Create { kind, start, cur }) => studio.finish_create(kind, start, cur),
        Some(Op::Pencil { pts }) => studio.finish_pencil(pts),
        Some(op @ Op::Pen { .. }) => {
            studio.op = Some(op);
        }
        Some(Op::Move { orig, start }) => {
            if studio.is_motion() {
                commit_pose_drag(studio);
            } else {
                let d = world - start;
                if d.length() > 0.2 {
                    commit_obj_snaps(studio, orig);
                }
            }
        }
        Some(Op::Resize { orig, .. }) | Some(Op::Rotate { orig, .. }) => {
            if studio.is_motion() {
                commit_pose_drag(studio);
            } else {
                commit_obj_snaps(studio, orig);
            }
        }
        Some(Op::Node { layer, id, orig, .. }) => {
            if let Some(s) = studio.doc.find_shape(layer, id) {
                studio.history.push(crate::document::Cmd::SetGeom {
                    layer,
                    id,
                    before: orig,
                    after: s.geom.clone(),
                    rot_before: s.rotation,
                    rot_after: s.rotation,
                });
            }
            studio.dirty = true;
        }
        Some(Op::NodeMarquee { start, cur }) => {
            let r = Bounds {
                min: Pt::new(start.x.min(cur.x), start.y.min(cur.y)),
                max: Pt::new(start.x.max(cur.x), start.y.max(cur.y)),
            };
            if let Some((li, id)) = studio.primary() {
                if let Some(s) = studio.doc.find_shape(li, id) {
                    if let Geom::Path { anchors, .. } = &s.geom {
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
                studio.history.push(crate::document::Cmd::SetCorners {
                    layer,
                    id,
                    before: orig_corners,
                    after: s.corners,
                    radius_before: orig_radius,
                    radius_after: match s.geom {
                        Geom::Rect { radius, .. } => radius,
                        _ => orig_radius,
                    },
                });
                if s.geom != orig_geom {
                    studio.history.push(crate::document::Cmd::SetGeom {
                        layer,
                        id,
                        before: orig_geom,
                        after: s.geom.clone(),
                        rot_before: s.rotation,
                        rot_after: s.rotation,
                    });
                }
                studio.dirty = true;
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
        Some(Op::ArtboardMove { orig, .. }) => {
            let after = studio.doc.artboards.clone();
            studio.doc.artboards = orig;
            studio.commit_artboards(after);
        }
        Some(Op::ArtboardResize { orig, .. }) | Some(Op::ArtboardRotate { orig, .. }) => {
            let after = studio.doc.artboards.clone();
            if let Some(slot) = studio.doc.artboards.iter_mut().find(|a| a.id == orig.id) {
                *slot = orig;
            }
            studio.commit_artboards(after);
        }
        Some(Op::Brush {
            layer,
            buf,
            before,
            ..
        }) => {
            if let Some(px) = studio.doc.layers.get_mut(layer).and_then(|l| l.kind.pixels_mut()) {
                if let Some(mut pm) = px.to_pixmap() {
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
        }
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
                    studio.selection = if studio.is_motion() {
                        crate::motion::hits_in_rect(
                            &studio.doc,
                            studio.playhead,
                            &studio.pose_drag,
                            b,
                        )
                    } else {
                        studio.doc.hits_in_rect(b)
                    };
                }
            } else if let Some(li) = studio.raster_target() {
                if let Some(px) = studio.doc.layers[li].kind.pixels() {
                    studio.pixel_sel = Some(if ellipse {
                        paint::fill_ellipse_mask(px.w, px.h, start.x, start.y, cur.x, cur.y)
                    } else {
                        paint::fill_rect_mask(px.w, px.h, start.x, start.y, cur.x, cur.y)
                    });
                }
            }
        }
        Some(Op::Lasso { pts }) => {
            if let Some(li) = studio.raster_target() {
                if let Some(px) = studio.doc.layers[li].kind.pixels() {
                    studio.pixel_sel = Some(paint::fill_poly_mask(px.w, px.h, &pts));
                }
            }
        }
        Some(Op::Gradient { start, cur }) => {
            if let Some((li, id)) = studio.primary() {
                if let Some(s) = studio.doc.find_shape(li, id) {
                    let b = s.world_bbox();
                    let from = [
                        ((start.x - b.min.x) / b.width().max(1.0)).clamp(0.0, 1.0),
                        ((start.y - b.min.y) / b.height().max(1.0)).clamp(0.0, 1.0),
                    ];
                    let to = [
                        ((cur.x - b.min.x) / b.width().max(1.0)).clamp(0.0, 1.0),
                        ((cur.y - b.min.y) / b.height().max(1.0)).clamp(0.0, 1.0),
                    ];
                    let mut after = s.style.clone();
                    after.fill = Fill::Linear {
                        from,
                        to,
                        c0: studio.gradient.0,
                        c1: studio.gradient.1,
                    };
                    studio.commit(crate::document::Cmd::SetStyle {
                        layer: li,
                        id,
                        before: s.style.clone(),
                        after,
                    });
                }
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

fn click(studio: &mut Studio, world: Pt, shift: bool, alt: bool) {
    match studio.tool {
        Tool::Select => {
            if hit_handle(studio, world).is_some() {
                return;
            }
            if let Some(hit) = hit_shape(studio, world, 6.0 / studio.view.scale.max(0.01)) {
                if shift {
                    if let Some(i) = studio.selection.iter().position(|x| *x == hit) {
                        studio.selection.remove(i);
                    } else {
                        studio.selection.push(hit);
                    }
                } else {
                    studio.selection = vec![hit];
                }
                studio.active_layer = Some(hit.0);
            } else if !shift {
                studio.selection.clear();
            }
        }
        Tool::Node => {
            if let Some((li, id)) = studio.primary() {
                if id != RASTER_ID {
                    studio.ensure_path(li, id);
                }
                let slack = 8.0 / studio.view.scale.max(0.01);
                let Some(shape) = studio.doc.find_shape(li, id) else {
                    return;
                };
                let Geom::Path { anchors, closed } = &shape.geom else {
                    if let Some(hit) = hit_shape(studio, world, slack) {
                        studio.selection = vec![hit];
                        studio.node_sel.clear();
                    }
                    return;
                };
                let anchors = anchors.clone();
                let closed = *closed;
                let orig = shape.geom.clone();
                let rot = shape.rotation;
                if let Some(hit) = hit_node(&anchors, world, slack, closed) {
                    match hit {
                        NodeHit::Point(i) => {
                            if alt {
                                let mut after = orig.clone();
                                if let Geom::Path { anchors, .. } = &mut after {
                                    if let Some(a) = anchors.get_mut(i) {
                                        if a.is_corner() {
                                            a.make_smooth();
                                        } else {
                                            a.make_corner();
                                        }
                                    }
                                }
                                studio.commit(crate::document::Cmd::SetGeom {
                                    layer: li,
                                    id,
                                    before: orig,
                                    after,
                                    rot_before: rot,
                                    rot_after: rot,
                                });
                            }
                        }
                        NodeHit::HandleIn(_) | NodeHit::HandleOut(_) => {}
                        NodeHit::Segment(_) => {
                            let mut anchors = anchors;
                            if let Some(idx) =
                                insert_anchor(&mut anchors, closed, world, slack * 2.0)
                            {
                                studio.node_sel.clear();
                                studio.node_sel.insert(idx);
                                studio.commit(crate::document::Cmd::SetGeom {
                                    layer: li,
                                    id,
                                    before: orig,
                                    after: Geom::Path { anchors, closed },
                                    rot_before: rot,
                                    rot_after: rot,
                                });
                            }
                        }
                    }
                } else {
                    let mut anchors = anchors;
                    if let Some(idx) = insert_anchor(&mut anchors, closed, world, slack * 2.0) {
                        studio.node_sel.clear();
                        studio.node_sel.insert(idx);
                        studio.commit(crate::document::Cmd::SetGeom {
                            layer: li,
                            id,
                            before: orig,
                            after: Geom::Path { anchors, closed },
                            rot_before: rot,
                            rot_after: rot,
                        });
                    } else if let Some(hit) = hit_shape(studio, world, slack) {
                        studio.selection = vec![hit];
                        studio.node_sel.clear();
                    }
                }
            } else if let Some(hit) = hit_shape(studio, world, 6.0 / studio.view.scale.max(0.01)) {
                studio.selection = vec![hit];
                studio.node_sel.clear();
                if hit.1 != RASTER_ID {
                    studio.ensure_path(hit.0, hit.1);
                }
            }
        }
        _ => {}
    }
}

fn draw_rulers(p: &eframe::egui::Painter, rect: Rect, studio: &Studio) {
    let size = 18.0;
    let bg = bg_panel();
    p.rect_filled(Rect::from_min_size(rect.min, vec2(rect.width(), size)), 0.0, bg);
    p.rect_filled(Rect::from_min_size(rect.min, vec2(size, rect.height())), 0.0, bg);
    let step = nice_step(40.0 / studio.view.scale);
    let origin = win(rect, studio.view, Pt::ZERO);
    let mut x = origin.x;
    let mut wx = 0.0f32;
    while x < rect.max.x {
        if x > rect.min.x + size {
            p.line_segment(
                [pos2(x, rect.min.y), pos2(x, rect.min.y + size)],
                Stroke::new(1.0, border()),
            );
            p.text(
                pos2(x + 2.0, rect.min.y + 2.0),
                eframe::egui::Align2::LEFT_TOP,
                format!("{wx:.0}"),
                eframe::egui::FontId::monospace(9.0),
                fg_weak(),
            );
        }
        x += step * studio.view.scale;
        wx += step;
    }
    let mut y = origin.y;
    let mut wy = 0.0f32;
    while y < rect.max.y {
        if y > rect.min.y + size {
            p.line_segment(
                [pos2(rect.min.x, y), pos2(rect.min.x + size, y)],
                Stroke::new(1.0, border()),
            );
        }
        y += step * studio.view.scale;
        wy += step;
        let _ = wy;
    }
}

fn nice_step(raw: f32) -> f32 {
    let p = 10f32.powf(raw.abs().max(1.0).log10().floor());
    let n = raw / p;
    let m = if n < 2.0 {
        1.0
    } else if n < 5.0 {
        2.0
    } else {
        5.0
    };
    m * p
}

fn draw_grid(p: &eframe::egui::Painter, rect: Rect, studio: &Studio) {
    let g = studio.doc.grid.size.max(1.0);
    let col = crate::ui::theme::accent_soft();
    let origin = win(rect, studio.view, Pt::ZERO);
    let mut x = origin.x;
    while x < rect.max.x {
        p.line_segment([pos2(x, rect.min.y), pos2(x, rect.max.y)], Stroke::new(1.0, col));
        x += g * studio.view.scale;
    }
    let mut y = origin.y;
    while y < rect.max.y {
        p.line_segment([pos2(rect.min.x, y), pos2(rect.max.x, y)], Stroke::new(1.0, col));
        y += g * studio.view.scale;
    }
}

fn draw_guides(p: &eframe::egui::Painter, rect: Rect, studio: &Studio) {
    let s = Stroke::new(1.0, accent());
    for g in &studio.doc.guides {
        if g.vertical {
            let x = win(rect, studio.view, Pt::new(g.pos, 0.0)).x;
            p.line_segment([pos2(x, rect.min.y), pos2(x, rect.max.y)], s);
        } else {
            let y = win(rect, studio.view, Pt::new(0.0, g.pos)).y;
            p.line_segment([pos2(rect.min.x, y), pos2(rect.max.x, y)], s);
        }
    }
}

fn draw_artboard_frames(p: &eframe::egui::Painter, rect: Rect, studio: &Studio) {
    let doc = &studio.doc;
    if doc.artboards.is_empty() {
        return;
    }
    for a in &doc.artboards {
        let r = Rect::from_min_max(
            win(rect, studio.view, a.origin),
            win(rect, studio.view, a.origin + a.size),
        );
        let on = studio.artboard_sel.contains(&a.id);
        p.rect_stroke(
            r,
            0.0,
            Stroke::new(
                if on { 2.0 } else { 1.0 },
                if on {
                    accent()
                } else {
                    Color32::from_rgb(120, 130, 150)
                },
            ),
            eframe::egui::StrokeKind::Middle,
        );
        p.text(
            r.left_top() + vec2(4.0, 2.0),
            eframe::egui::Align2::LEFT_TOP,
            &a.name,
            eframe::egui::FontId::monospace(10.0),
            fg_weak(),
        );
        if on && studio.tool == Tool::Artboard {
            let b = a.bounds();
            for i in 0..8 {
                let h = win(rect, studio.view, b.handle(i));
                p.rect_filled(Rect::from_center_size(h, vec2(9.0, 9.0)), 0.0, select());
            }
            let rh = win(rect, studio.view, b.rotate_handle());
            p.circle_filled(rh, 5.0, accent());
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
            p.line_segment([pos2(tl.x - bleed, tl.y), pos2(tl.x - bleed + inset, tl.y)], Stroke::new(1.0, red));
            p.line_segment([pos2(tl.x, tl.y - bleed), pos2(tl.x, tl.y - bleed + inset)], Stroke::new(1.0, red));
            // TR
            p.line_segment([pos2(tr.x + bleed - inset, tr.y), pos2(tr.x + bleed, tr.y)], Stroke::new(1.0, red));
            p.line_segment([pos2(tr.x, tr.y - bleed), pos2(tr.x, tr.y - bleed + inset)], Stroke::new(1.0, red));
            // BL
            p.line_segment([pos2(bl.x - bleed, bl.y), pos2(bl.x - bleed + inset, bl.y)], Stroke::new(1.0, red));
            p.line_segment([pos2(bl.x, bl.y + bleed - inset), pos2(bl.x, bl.y + bleed)], Stroke::new(1.0, red));
            // BR
            p.line_segment([pos2(brp.x + bleed - inset, brp.y), pos2(brp.x + bleed, brp.y)], Stroke::new(1.0, red));
            p.line_segment([pos2(brp.x, brp.y + bleed - inset), pos2(brp.x, brp.y + bleed)], Stroke::new(1.0, red));
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

fn draw_overlays(p: &eframe::egui::Painter, rect: Rect, studio: &Studio) {
    let v = studio.view;
    if let Some(Op::Create { kind, start, cur }) = &studio.op {
        let g = preview_shape(*kind, *start, *cur, studio);
        stroke_geom(p, rect, &g, v);
    }
    if let Some(Op::Pen { anchors, source, .. }) = &studio.op {
        draw_pen(p, rect, anchors, studio, source.is_some());
    }
    if let Some(Op::Pencil { pts }) = &studio.op {
        let scr: Vec<Pos2> = pts.iter().map(|q| win(rect, v, *q)).collect();
        if scr.len() >= 2 {
            p.add(eframe::egui::Shape::line(scr, Stroke::new(1.5, select())));
        }
    }
    if let Some(Op::Marquee { start, cur, .. })
    | Some(Op::Gradient { start, cur })
    | Some(Op::CropPhoto { start, cur })
    | Some(Op::ZoomBox { start, cur })
    | Some(Op::Place { start, cur }) = &studio.op
    {
        let a = win(rect, v, *start);
        let b = win(rect, v, *cur);
        let r = Rect::from_two_pos(a, b);
        p.rect_filled(r, 0.0, select_fill());
        p.rect_stroke(r, 0.0, Stroke::new(1.0, select()), eframe::egui::StrokeKind::Middle);
    }
    if let Some(Op::Lasso { pts }) = &studio.op {
        let scr: Vec<Pos2> = pts.iter().map(|q| win(rect, v, *q)).collect();
        if scr.len() >= 2 {
            p.add(eframe::egui::Shape::line(scr, Stroke::new(1.2, select())));
        }
    }

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
                p.rect_stroke(sb, 0.0, Stroke::new(1.0, select()), eframe::egui::StrokeKind::Middle);
            }
            continue;
        }
        if let Some(s) = studio.doc.find_shape(*li, *id) {
            stroke_world_posed(p, rect, s, v, studio.live_pose(*id));
            if studio.tool == Tool::Node {
                if let Geom::Path { anchors, .. } = &s.geom {
                    draw_nodes(p, rect, anchors, v, &studio.node_sel);
                }
            }
            if matches!(studio.tool, Tool::Select | Tool::Node) {
                if let Geom::Rect { origin, size, .. } = &s.geom {
                    for w in crate::geom::corner_widgets(*origin, *size) {
                        let sp = win(rect, v, w);
                        p.circle_filled(sp, 4.0, accent());
                    }
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
        p.rect_stroke(r, 0.0, Stroke::new(1.0, select()), eframe::egui::StrokeKind::Middle);
    }
    if !studio.selection.is_empty() {
        if let Some(b) = posed_bounds(studio) {
            let sb = Rect::from_min_max(win(rect, v, b.min), win(rect, v, b.max));
            p.rect_stroke(sb, 0.0, Stroke::new(1.0, select()), eframe::egui::StrokeKind::Middle);
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
    }
    if studio.op.is_none()
        && let Some(at) = studio.cursor
        && let Some(b) = studio.pending_preview_rect(at)
    {
        let r = Rect::from_min_max(win(rect, v, b.min), win(rect, v, b.max));
        p.rect_filled(r, 0.0, select_fill());
        p.rect_stroke(r, 0.0, Stroke::new(1.0, select()), eframe::egui::StrokeKind::Middle);
    }
    if let Some(edit) = &studio.type_edit {
        if let Some(s) = studio.doc.find_shape(edit.layer, edit.id) {
            if let Geom::Text(run) = &s.geom {
                draw_type_caret(p, rect, studio, run, edit.caret, edit.anchor);
            }
        }
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
    let on = (studio
        .canvas_rect
        .map(|_| p.ctx().input(|i| i.time))
        .unwrap_or(0.0)
        * 1.7)
        .fract()
        < 0.55;
    if on {
        let c = crate::text::caret_pt(run, caret);
        let top = win(rect, v, Pt::new(c.x, c.y - run.px * 0.9));
        let bot = win(rect, v, Pt::new(c.x, c.y + run.px * 0.2));
        p.line_segment([top, bot], Stroke::new(1.5, select()));
    }
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
            Tool::Pen | Tool::Pencil | Tool::Brush | Tool::Eraser | Tool::Clone | Tool::Smudge => {
                CursorIcon::Crosshair
            }
            Tool::Eyedropper | Tool::Trace => CursorIcon::Crosshair,
            Tool::Crop | Tool::Marquee | Tool::EllipseMarquee | Tool::Lasso => CursorIcon::Crosshair,
            Tool::Select => {
                if studio.selection.is_empty() {
                    CursorIcon::Default
                } else {
                    CursorIcon::Default
                }
            }
            Tool::Artboard => CursorIcon::Crosshair,
            _ => CursorIcon::Crosshair,
        }
    };
    ui.ctx().set_cursor_icon(icon);
}

fn context_menu(resp: &eframe::egui::Response, studio: &mut Studio) {
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
        if ui.button("Bring to front").clicked() {
            studio.bring_to_front();
            ui.close();
        }
        if ui.button("Send to back").clicked() {
            studio.send_to_back();
            ui.close();
        }
        ui.separator();
        if ui
            .add_enabled(studio.selection.len() >= 2, eframe::egui::Button::new("Combine"))
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

fn ruler_guides(studio: &mut Studio, resp: &eframe::egui::Response, rect: Rect) {
    if !studio.show_rulers || !resp.clicked() {
        return;
    }
    let Some(p) = resp.interact_pointer_pos() else {
        return;
    };
    let size = 18.0;
    let top = Rect::from_min_size(rect.min, vec2(rect.width(), size));
    let left = Rect::from_min_size(rect.min, vec2(size, rect.height()));
    let origin = Pt::new(rect.min.x, rect.min.y);
    if top.contains(p) && p.x > rect.min.x + size {
        let world = studio.view.pointer_to_world(origin, from_egui(p));
        studio.add_guide(true, world.x);
    } else if left.contains(p) && p.y > rect.min.y + size {
        let world = studio.view.pointer_to_world(origin, from_egui(p));
        studio.add_guide(false, world.y);
    }
}

fn commit_pose_drag(studio: &mut Studio) {
    if studio.pose_drag.is_empty() {
        return;
    }
    let t = studio.playhead;
    let drag = studio.pose_drag.clone();
    let mut after = studio.doc.motion.clone();
    for (id, pose) in &drag {
        after.set_key(*id, crate::motion::Prop::X, t, pose.dx, crate::motion::Ease::EaseInOut);
        after.set_key(*id, crate::motion::Prop::Y, t, pose.dy, crate::motion::Ease::EaseInOut);
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
            after.set_key(*id, crate::motion::Prop::Opacity, t, op, crate::motion::Ease::EaseInOut);
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
) {
    let v = studio.view;
    let mut draft = anchors.to_vec();
    if let Some(c) = studio.cursor {
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
    } else if let (Some(last), Some(c)) = (anchors.last(), studio.cursor) {
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
            if skip == Some(s.id) {
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
        if a.radius > 0.5 {
            p.circle_stroke(sp, 10.0, Stroke::new(1.0, accent()));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
        s.finish_create(
            CreateKind::Rect,
            Pt::new(40.0, 40.0),
            Pt::new(140.0, 100.0),
        );
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
    fn click_on_oval_handle_keeps_selection() {
        let mut s = oval();
        let id = s.selection[0];
        let b = posed_bounds(&s).unwrap();
        click(&mut s, b.handle(1), false, false);
        assert_eq!(s.selection, vec![id]);
    }

    #[test]
    fn pen_click_places_corner_without_drag() {
        let mut s = Studio::new();
        s.show_welcome = false;
        s.tool = Tool::Pen;
        start_drag(&mut s, Pt::new(20.0, 20.0), Pt::new(20.0, 20.0), false, false);
        continue_drag(&mut s, Pt::new(21.0, 20.5), false, false);
        end_drag(&mut s, Pt::new(21.0, 20.5), false, false, false);
        start_drag(&mut s, Pt::new(80.0, 40.0), Pt::new(80.0, 40.0), false, false);
        continue_drag(&mut s, Pt::new(80.5, 40.0), false, false);
        end_drag(&mut s, Pt::new(80.5, 40.0), false, false, false);
        match &s.op {
            Some(Op::Pen { anchors, .. }) => {
                assert_eq!(anchors.len(), 2);
                assert!(anchors[0].is_corner(), "jitter must not make a smooth point");
                assert!(anchors[1].is_corner());
            }
            other => panic!("expected pen draft, got op? {}", other.is_some()),
        }
        if let Some(Op::Pen { anchors, source, .. }) = s.op.take() {
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
        start_drag(&mut s, Pt::new(10.0, 10.0), Pt::new(10.0, 10.0), false, false);
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
        s.finish_create(
            CreateKind::Rect,
            Pt::new(40.0, 40.0),
            Pt::new(140.0, 100.0),
        );
        s.tool = Tool::Node;
        let (li, id) = s.primary().unwrap();
        s.ensure_path(li, id);
        let anchors = match &s.doc.find_shape(li, id).unwrap().geom {
            Geom::Path { anchors, .. } => anchors.clone(),
            _ => panic!("path"),
        };
        start_drag(&mut s, anchors[0].pt, anchors[0].pt, false, false);
        end_drag(&mut s, anchors[0].pt, false, false, false);
        start_drag(&mut s, anchors[1].pt, anchors[1].pt, true, false);
        assert!(s.node_sel.contains(&0) || s.node_sel.contains(&1));
        assert!(s.node_sel.len() >= 1);
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
            other => panic!("expected handle-out grab, got {:?}", other.as_ref().map(|o| matches!(o, Op::Node { .. }))),
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
}
