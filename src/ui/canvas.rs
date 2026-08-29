use crate::app::{from_egui, to_egui, CreateKind, NodeHit, Op, Studio};
use crate::compositor::{self, Draft};
use crate::document::Fill;
use crate::geom::{insert_anchor, Anchor, Bounds, Geom, Pt};
use crate::paint;
use crate::tools::Tool;
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
    let scroll = ctx.input(|i| i.smooth_scroll_delta);
    let zoom_delta = ctx.input(|i| i.zoom_delta());
    let pointer = resp.hover_pos();
    let origin = Pt::new(rect.min.x, rect.min.y);

    if let Some(hp) = pointer {
        let local = from_egui(hp) - origin;
        if zoom_delta != 1.0 {
            studio.view.zoom_at(local, zoom_delta);
        } else if ctx.input(|i| i.modifiers.ctrl) && scroll.y.abs() > 0.0 {
            let f = if scroll.y > 0.0 { 1.08 } else { 1.0 / 1.08 };
            studio.view.zoom_at(local, f);
        } else if scroll != Vec2::ZERO && !ctx.input(|i| i.modifiers.ctrl) {
            studio.view.offset.x += scroll.x;
            studio.view.offset.y += scroll.y;
        }
        studio.cursor = Some(studio.view.to_world(local));
    }

    let panning = (space && studio.type_edit.is_none()) || studio.tool == Tool::Hand;
    if panning && resp.dragged_by(PointerButton::Primary)
        || resp.dragged_by(PointerButton::Middle)
    {
        let d = resp.drag_delta();
        studio.view.offset.x += d.x;
        studio.view.offset.y += d.y;
    } else {
        handle_pointer(studio, &resp, space);
    }

    let w = rect.width().max(1.0) as u32;
    let h = rect.height().max(1.0) as u32;
    let draft = match &studio.op {
        Some(Op::Create { kind, start, cur }) => {
            let preview = preview_shape(*kind, *start, *cur, studio);
            // Can't hold ref; render without preview geom via overlay instead
            let _ = preview;
            Draft::none()
        }
        Some(Op::Brush { layer, buf, .. }) => Draft {
            preview: None,
            brush: Some((*layer, buf, studio.brush.opacity)),
        },
        _ => Draft::none(),
    };

    if let Some(pm) = compositor::render_view(&studio.doc, studio.view, w, h, draft) {
        let image = eframe::egui::ColorImage::from_rgba_unmultiplied(
            [pm.width() as usize, pm.height() as usize],
            pm.data(),
        );
        let tex = ui.ctx().load_texture("canvas", image, eframe::egui::TextureOptions::LINEAR);
        ui.painter().image(
            tex.id(),
            rect,
            Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
            Color32::WHITE,
        );
        studio.screen_tex = Some(tex);
    }

    let painter = ui.painter_at(rect);
    if studio.show_rulers {
        draw_rulers(&painter, rect, studio);
    }
    if studio.show_grid || studio.doc.grid.visible {
        draw_grid(&painter, rect, studio);
    }
    draw_guides(&painter, rect, studio);
    draw_overlays(&painter, rect, studio);

    let files: Vec<_> = ui.ctx().input(|i| i.raw.dropped_files.clone());
    for f in files {
        let path = f.path();
        let ext = path
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        if ext == "oma" {
            if let Ok(doc) = crate::project::load_from(path) {
                studio.doc = doc;
                studio.path = Some(path.to_path_buf());
                studio.history.clear();
                studio.need_fit = true;
                studio.status = "opened".into();
            }
        } else if let Some(img) = crate::photo::load_file(path) {
            let mut layer = crate::document::Layer::raster(
                path.file_name()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_else(|| "Image".into()),
                img.w,
                img.h,
            );
            if let crate::document::LayerKind::Raster { pixels } = &mut layer.kind {
                *pixels = crate::document::Pixels::from_rgba(img.w, img.h, img.data)
                    .unwrap_or_else(|| crate::document::Pixels::new(img.w, img.h));
            }
            let index = studio.doc.layers.len();
            studio.commit(crate::document::Cmd::AddLayer { index, layer });
            studio.active_layer = Some(index);
            studio.status = "image placed as pixel layer".into();
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
    let mut world = studio.view.pointer_to_world(origin, from_egui(screen));
    // Don't snap when placing or editing type — the caret should be exact.
    let is_text = studio.tool == Tool::Text || studio.type_edit.is_some();
    if !is_text {
        world = studio.snap_pt(world);
    }
    let alt = resp.ctx.input(|i| i.modifiers.alt);
    let shift = resp.ctx.input(|i| i.modifiers.shift);

    if studio.tool == Tool::Zoom && resp.clicked() && !resp.dragged() {
        let f = if alt { 1.0 / 1.25 } else { 1.25 };
        studio.view.zoom_at(from_egui(screen) - origin, f);
        return;
    }

    if studio.tool == Tool::Eyedropper && resp.clicked() {
        studio.eyedrop(world);
        return;
    }

    if studio.type_edit.is_some() && resp.clicked() {
        let slack = 8.0 / studio.view.scale.max(0.01);
        if let Some(hit) = studio.doc.hit_test(world, slack) {
            if studio.editing_text(hit.0, hit.1) {
                studio.begin_type_edit(hit, world);
                return;
            }
            if is_text_hit(studio, hit) {
                studio.commit_type_edit();
                studio.begin_type_edit(hit, world);
                return;
            }
        }
        studio.commit_type_edit();
        if studio.tool == Tool::Text {
            studio.place_text(world);
        }
        return;
    }

    if studio.tool == Tool::Text && resp.clicked() {
        let slack = 8.0 / studio.view.scale.max(0.01);
        if let Some(hit) = studio.doc.hit_test(world, slack)
            && is_text_hit(studio, hit)
        {
            studio.begin_type_edit(hit, world);
            return;
        }
        studio.place_text(world);
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
                    paint::flood_fill(&mut pm, world, c, studio.fill_tolerance);
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
                    studio.pixel_sel = Some(paint::wand_mask(&pm, world, studio.fill_tolerance));
                    studio.status = "wand selection".into();
                }
            }
        }
        return;
    }

    if studio.tool == Tool::Clone && alt && resp.clicked() {
        studio.clone_source = Some(world);
        studio.status = "clone source set".into();
        return;
    }

    // Text edit drag: selecting a range should not commit, but update the caret.
    if studio.type_edit.is_some() {
        if resp.drag_started_by(PointerButton::Primary) {
            let slack = 8.0 / studio.view.scale.max(0.01);
            if let Some(hit) = studio.doc.hit_test(world, slack) {
                if studio.editing_text(hit.0, hit.1) {
                    let caret = studio.doc.find_shape(hit.0, hit.1).and_then(|s| match &s.geom {
                        Geom::Text(run) => Some(crate::text::hit_char(run, world)),
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
                        let new_caret = crate::text::hit_char(run, world);
                        if let Some(e) = studio.type_edit.as_mut() {
                            e.caret = new_caret;
                        }
                    }
                }
            }
            return;
        }
    }

    // Drag start. The pen stays in a draft `Op`, so it must accept further
    // presses — otherwise you get one point and then nothing.
    let continue_pen = studio.tool == Tool::Pen && matches!(studio.op, Some(Op::Pen { .. }));
    if resp.drag_started_by(PointerButton::Primary) && (studio.op.is_none() || continue_pen) {
        start_drag(studio, world, shift, alt);
    }

    if resp.dragged_by(PointerButton::Primary) {
        continue_drag(studio, world, shift, alt);
    }

    if resp.drag_stopped() {
        end_drag(studio, world);
    }

    if resp.clicked() && !resp.dragged() {
        click(studio, world, shift);
    }

    if resp.double_clicked() {
        if let Some(Op::Pen { anchors }) = studio.op.take() {
            studio.finish_pen(anchors, false);
        } else {
            let slack = 8.0 / studio.view.scale.max(0.01);
            if let Some(hit) = studio.doc.hit_test(world, slack)
                && is_text_hit(studio, hit)
            {
                studio.begin_type_edit(hit, world);
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

fn start_drag(studio: &mut Studio, world: Pt, shift: bool, alt: bool) {
    let _ = alt;
    // Handle hit takes priority from any design tool so a freshly drawn object
    // can be resized immediately without first pressing V. Text editing, Hand
    // and Zoom are excluded to keep their own pointer semantics.
    if studio.type_edit.is_none()
        && !matches!(studio.tool, Tool::Hand | Tool::Zoom)
        && !studio.selection.is_empty()
    {
        if let Some(sel) = hit_handle(studio, world) {
            match sel {
                HandleKind::Rotate(center) => {
                    let orig = snapshot(studio);
                    studio.op = Some(Op::Rotate {
                        orig,
                        center,
                        start_angle: (world - center).y.atan2((world - center).x),
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
            if let Some(hit) = studio.doc.hit_test(world, 6.0 / studio.view.scale.max(0.01)) {
                if !studio.selection.contains(&hit) {
                    if shift {
                        studio.selection.push(hit);
                    } else {
                        studio.selection = vec![hit];
                    }
                }
                studio.active_layer = Some(hit.0);
                studio.op = Some(Op::Move {
                    orig: snapshot(studio),
                    start: world,
                });
            } else {
                studio.op = Some(Op::Marquee {
                    start: world,
                    cur: world,
                    ellipse: false,
                });
                if !shift {
                    studio.selection.clear();
                }
            }
        }
        Tool::Node => {
            if let Some((li, id)) = studio.primary() {
                if let Some(shape) = studio.doc.find_shape(li, id) {
                    if let Geom::Path { anchors, .. } = &shape.geom {
                        if let Some(hit) = hit_node(anchors, world, 8.0 / studio.view.scale.max(0.01)) {
                            studio.op = Some(Op::Node {
                                layer: li,
                                id,
                                which: hit,
                                orig: shape.geom.clone(),
                            });
                            return;
                        }
                    }
                }
            }
        }
        Tool::Pen => studio.pen_click(world),
        Tool::Pencil => {
            studio.op = Some(Op::Pencil { pts: vec![world] });
        }
        Tool::Rect => {
            studio.op = Some(Op::Create {
                kind: CreateKind::Rect,
                start: world,
                cur: world,
            })
        }
        Tool::Ellipse => {
            studio.op = Some(Op::Create {
                kind: CreateKind::Ellipse,
                start: world,
                cur: world,
            })
        }
        Tool::Polygon => {
            studio.op = Some(Op::Create {
                kind: CreateKind::Polygon,
                start: world,
                cur: world,
            })
        }
        Tool::Star => {
            studio.op = Some(Op::Create {
                kind: CreateKind::Star,
                start: world,
                cur: world,
            })
        }
        Tool::Line => {
            studio.op = Some(Op::Create {
                kind: CreateKind::Line,
                start: world,
                cur: world,
            })
        }
        Tool::Gradient => {
            studio.op = Some(Op::Gradient {
                start: world,
                cur: world,
            })
        }
        Tool::Brush | Tool::Eraser => {
            if let Some(li) = studio.raster_target() {
                if let Some(px) = studio.doc.layers[li].kind.pixels() {
                    if let Some(buf) = PixmapOr::new(px.w, px.h) {
                        let mut buf = buf;
                        paint::stamp(&mut buf, world, &studio.brush, studio.tool == Tool::Eraser);
                        studio.op = Some(Op::Brush {
                            layer: li,
                            erase: studio.tool == Tool::Eraser,
                            buf,
                            last: Some(world),
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
                        last: Some(world),
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
                        last: Some(world),
                        before: px.data.clone(),
                    });
                }
            }
        }
        Tool::Marquee => {
            studio.op = Some(Op::Marquee {
                start: world,
                cur: world,
                ellipse: false,
            })
        }
        Tool::EllipseMarquee => {
            studio.op = Some(Op::Marquee {
                start: world,
                cur: world,
                ellipse: true,
            })
        }
        Tool::Lasso => studio.op = Some(Op::Lasso { pts: vec![world] }),
        Tool::Crop => {
            studio.op = Some(Op::CropPhoto {
                start: world,
                cur: world,
            })
        }
        Tool::Zoom => {
            studio.op = Some(Op::ZoomBox {
                start: world,
                cur: world,
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

fn snapshot(studio: &Studio) -> Vec<(usize, u64, Geom, f32)> {
    studio
        .selection
        .iter()
        .filter_map(|(li, id)| {
            studio
                .doc
                .find_shape(*li, *id)
                .map(|s| (*li, *id, s.geom.clone(), s.rotation))
        })
        .collect()
}

enum HandleKind {
    Scale(usize, Bounds),
    Rotate(Pt),
}

fn hit_handle(studio: &Studio, world: Pt) -> Option<HandleKind> {
    let shapes = studio.selected_shapes();
    let b = crate::align::selection_bounds(&shapes)?;
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

fn hit_node(anchors: &[Anchor], world: Pt, slack: f32) -> Option<NodeHit> {
    for (i, a) in anchors.iter().enumerate() {
        if (a.pt + a.h_in - world).length() <= slack && !a.is_corner() {
            return Some(NodeHit::HandleIn(i));
        }
        if (a.pt + a.h_out - world).length() <= slack && !a.is_corner() {
            return Some(NodeHit::HandleOut(i));
        }
        if (a.pt - world).length() <= slack {
            return Some(NodeHit::Point(i));
        }
    }
    None
}

fn continue_drag(studio: &mut Studio, world: Pt, shift: bool, alt: bool) {
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
        Some(Op::Pen { anchors }) => {
            if let Some(last) = anchors.last_mut() {
                let drag = world - last.pt;
                last.h_out = drag;
                last.h_in = -drag;
            }
        }
        Some(Op::Pencil { pts }) => {
            if pts.last().map(|p| (*p - world).length() > 1.5).unwrap_or(true) {
                pts.push(world);
            }
        }
        Some(Op::Move { orig, start }) => {
            let d = world - *start;
            for (li, id, geom, rot) in orig.clone() {
                if let Some(s) = studio.doc.find_shape_mut(li, id) {
                    s.geom = geom;
                    s.geom.translate(d);
                    s.rotation = rot;
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
            for (li, id, geom, rot) in orig.clone() {
                if let Some(s) = studio.doc.find_shape_mut(li, id) {
                    s.geom = geom;
                    s.geom.map_into(*start_box, dst);
                    s.rotation = rot;
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
            for (li, id, geom, rot) in orig.clone() {
                if let Some(s) = studio.doc.find_shape_mut(li, id) {
                    s.geom = geom;
                    s.geom.rotate_about(*center, ang);
                    s.rotation = rot;
                }
            }
        }
        Some(Op::Node {
            layer,
            id,
            which,
            orig,
        }) => {
            if let Some(s) = studio.doc.find_shape_mut(*layer, *id) {
                s.geom = orig.clone();
                if let Geom::Path { anchors, .. } = &mut s.geom {
                    match *which {
                        NodeHit::Point(i) => {
                            if let Some(a) = anchors.get_mut(i) {
                                a.pt = world;
                            }
                        }
                        NodeHit::HandleIn(i) => {
                            if let Some(a) = anchors.get_mut(i) {
                                a.h_in = world - a.pt;
                                if !shift {
                                    a.h_out = -a.h_in;
                                }
                            }
                        }
                        NodeHit::HandleOut(i) => {
                            if let Some(a) = anchors.get_mut(i) {
                                a.h_out = world - a.pt;
                                if !shift {
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
        None => {}
    }
}

fn end_drag(studio: &mut Studio, world: Pt) {
    match studio.op.take() {
        Some(Op::Create { kind, start, cur }) => studio.finish_create(kind, start, cur),
        Some(Op::Pencil { pts }) => studio.finish_pencil(pts),
        Some(Op::Pen { anchors }) => {
            // keep drafting until Enter / double-click / close
            studio.op = Some(Op::Pen { anchors });
        }
        Some(Op::Move { orig, start }) => {
            let d = world - start;
            if d.length() > 0.2 {
                for (li, id, geom, rot) in orig {
                    if let Some(s) = studio.doc.find_shape(li, id) {
                        studio.history.push(crate::document::Cmd::SetGeom {
                            layer: li,
                            id,
                            before: geom,
                            after: s.geom.clone(),
                            rot_before: rot,
                            rot_after: s.rotation,
                        });
                    }
                }
                studio.dirty = true;
            }
        }
        Some(Op::Resize { orig, .. }) | Some(Op::Rotate { orig, .. }) => {
            for (li, sid, geom, rot) in orig {
                if let Some(s) = studio.doc.find_shape(li, sid) {
                    studio.history.push(crate::document::Cmd::SetGeom {
                        layer: li,
                        id: sid,
                        before: geom,
                        after: s.geom.clone(),
                        rot_before: rot,
                        rot_after: s.rotation,
                    });
                }
            }
            studio.dirty = true;
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
                    studio.selection = studio.doc.hits_in_rect(b);
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
        Some(Op::ZoomBox { start, cur }) => studio.finish_zoom_box(start, cur),
        None => {}
    }
}

fn click(studio: &mut Studio, world: Pt, shift: bool) {
    match studio.tool {
        Tool::Select => {
            if let Some(hit) = studio.doc.hit_test(world, 6.0 / studio.view.scale.max(0.01)) {
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
                if let Some(shape) = studio.doc.find_shape_mut(li, id) {
                    if let Geom::Path {
                        anchors, closed, ..
                    } = &mut shape.geom
                    {
                        let slack = 8.0 / studio.view.scale.max(0.01);
                        if let Some(i) = hit_node(anchors, world, slack) {
                            if matches!(i, NodeHit::Point(_)) && shift {
                                if let NodeHit::Point(i) = i {
                                    if anchors[i].is_corner() {
                                        anchors[i].make_smooth();
                                    } else {
                                        anchors[i].make_corner();
                                    }
                                }
                            }
                        } else {
                            let _ = insert_anchor(anchors, *closed, world, slack * 2.0);
                        }
                    }
                }
            } else if let Some(hit) = studio.doc.hit_test(world, 6.0 / studio.view.scale.max(0.01)) {
                studio.selection = vec![hit];
            }
        }
        Tool::Pen => studio.pen_click(world),
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

fn draw_overlays(p: &eframe::egui::Painter, rect: Rect, studio: &Studio) {
    let v = studio.view;
    if let Some(Op::Create { kind, start, cur }) = &studio.op {
        let g = preview_shape(*kind, *start, *cur, studio);
        stroke_geom(p, rect, &g, v);
    }
    if let Some(Op::Pen { anchors }) = &studio.op {
        draw_pen(p, rect, anchors, studio);
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
    | Some(Op::ZoomBox { start, cur }) = &studio.op
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
        if let Some(s) = studio.doc.find_shape(*li, *id) {
            stroke_world(p, rect, s, v);
            if studio.tool == Tool::Node {
                if let Geom::Path { anchors, .. } = &s.geom {
                    draw_nodes(p, rect, anchors, v);
                }
            }
        }
    }
    if !studio.selection.is_empty() {
        let shapes = studio.selected_shapes();
        if let Some(b) = crate::align::selection_bounds(&shapes) {
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
            p.add(eframe::egui::Shape::line(pts, Stroke::new(1.4, select())));
        }
    }
}

fn stroke_world(p: &eframe::egui::Painter, rect: Rect, s: &crate::document::Shape, view: crate::compositor::View) {
    for c in s.world_contours(64) {
        let pts: Vec<Pos2> = c.iter().map(|q| win(rect, view, *q)).collect();
        if pts.len() >= 2 {
            p.add(eframe::egui::Shape::line(
                pts,
                Stroke::new(1.2, Color32::from_white_alpha(180)),
            ));
        }
    }
}

fn draw_pen(p: &eframe::egui::Painter, rect: Rect, anchors: &[Anchor], studio: &Studio) {
    draw_nodes(p, rect, anchors, studio.view);
    if let Some(c) = studio.cursor {
        if let Some(last) = anchors.last() {
            p.line_segment(
                [win(rect, studio.view, last.pt), win(rect, studio.view, c)],
                Stroke::new(1.0, select()),
            );
        }
    }
}

fn draw_nodes(p: &eframe::egui::Painter, rect: Rect, anchors: &[Anchor], view: crate::compositor::View) {
    for a in anchors {
        let sp = win(rect, view, a.pt);
        if !a.is_corner() {
            let hi = win(rect, view, a.pt + a.h_in);
            let ho = win(rect, view, a.pt + a.h_out);
            p.line_segment([sp, hi], Stroke::new(1.0, select()));
            p.line_segment([sp, ho], Stroke::new(1.0, select()));
            p.circle_filled(hi, 3.0, select());
            p.circle_filled(ho, 3.0, select());
        }
        p.rect_filled(Rect::from_center_size(sp, vec2(6.0, 6.0)), 0.0, select());
    }
}
