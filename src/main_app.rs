use crate::brush::{self, BrushSettings, StrokeBuffer};
use crate::document::{
    history::{Cmd, History},
    next_shape_id, apply_cmd, Anchor, Document, Fill, Geometry, LayerBlend, Shape, Style,
};
use crate::render::{self, RenderDraft, View};
use crate::ui;
use eframe::egui::{
    pos2, vec2, Color32, Context, Key, PointerButton, Pos2, Rect, Sense, Vec2,
};

#[derive(Clone, Copy, PartialEq)]
pub enum Tool {
    Select,
    Rect,
    Ellipse,
    Pen,
    Brush,
    Text,
}

const HANDLES: [(f32, f32); 4] = [(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)];

pub enum ActiveOp {
    DragCreate {
        rect: bool,
        start: Pos2,
        cur: Pos2,
    },
    PenDraft {
        anchors: Vec<Anchor>,
        smoothing_last: bool,
    },
    Moving {
        layer: usize,
        id: u64,
        orig: Geometry,
        start_world: Pos2,
    },
    Resizing {
        layer: usize,
        id: u64,
        orig: Geometry,
        corner: usize,
    },
    Brushing {
        layer: usize,
        into_mask: bool,
        buffer: StrokeBuffer,
        last: Option<Pos2>,
        before: Vec<u8>,
    },
}

pub struct AtelierApp {
    pub doc: Document,
    pub view: View,
    pub tool: Tool,
    pub op: Option<ActiveOp>,
    pub selection: Option<(usize, u64)>,
    pub active_layer: Option<usize>,
    pub history: History,
    pub brush: BrushSettings,
    pub edit_mask: bool,
    pub default_style: Style,
    pub default_gradient: (Color32, Color32),
    pub status: String,
    pub cursor_world: Option<Pos2>,
    pub canvas_rect: Option<Rect>,
    pub need_fit: bool,
    pub export_scale: u32,
    pub text_buf: String,
    pub text_px: f32,
    pub text_edit_sid: Option<u64>,
    pub text_before: Option<(usize, u64, Geometry)>,
    pub bool_second: Option<(usize, u64)>,
    pub pending_bool: Option<crate::boolean::BoolOp>,
    pub screen_tex: Option<eframe::egui::TextureHandle>,
}

impl AtelierApp {
    pub fn new_dummy() -> Self {
        Self::new_dummy_inner()
    }

    fn new_dummy_inner() -> Self {
        let mut app = Self {
            doc: Document::new(1280.0, 800.0),
            view: View::default(),
            tool: Tool::Select,
            op: None,
            selection: None,
            active_layer: Some(1),
            history: History::default(),
            brush: BrushSettings::default(),
            edit_mask: false,
            default_style: Style::default(),
            default_gradient: (
                Color32::from_rgb(0xFF, 0xD1, 0x66),
                Color32::from_rgb(0xE5, 0x48, 0x4D),
            ),
            status: String::from("ready"),
            cursor_world: None,
            canvas_rect: None,
            need_fit: true,
            export_scale: 1,
            text_buf: String::new(),
            text_px: 48.0,
            text_edit_sid: None,
            text_before: None,
            bool_second: None,
            pending_bool: None,
            screen_tex: None,
        };
        if std::env::var("ATELIER_DEMO").is_ok() {
            app.seed_demo();
        }
        app
    }

    pub fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        Self::new_dummy_inner()
    }

    pub fn seed_demo(&mut self) {
        self.push_shape(Geometry::Rect {
            origin: pos2(80.0, 80.0),
            size: Vec2::new(260.0, 160.0),
        });
        self.selection = None;
        let mut ellipse = Shape {
            id: next_shape_id(),
            geom: Geometry::Ellipse {
                center: pos2(620.0, 260.0),
                radii: Vec2::new(170.0, 120.0),
            },
            style: Style {
                fill: Fill::Linear {
                    from: [0.0, 0.0],
                    to: [1.0, 1.0],
                    c0: self.default_gradient.0,
                    c1: self.default_gradient.1,
                },
                stroke: Some(eframe::egui::Stroke::new(2.0, Color32::from_rgb(0x1B, 0x24, 0x33))),
            },
        };
        let _ = &mut ellipse;
        let li = self.ensure_vector_target().unwrap();
        self.history.push(Cmd::AddShape {
            layer: li,
            shape: ellipse.clone(),
        });
        apply_cmd(&mut self.doc, &Cmd::AddShape { layer: li, shape: ellipse });
        self.push_shape(Geometry::Path {
            anchors: vec![
                Anchor::corner(pos2(120.0, 560.0)),
                Anchor::smooth(pos2(340.0, 420.0), vec2(90.0, 60.0)),
                Anchor::corner(pos2(560.0, 600.0)),
                Anchor::smooth(pos2(760.0, 460.0), vec2(-70.0, 50.0)),
            ],
            closed: false,
        });
        self.selection = None;
        let subpaths = crate::text::shape_text("Atelier", 96.0, pos2(820.0, 700.0));
        if !subpaths.is_empty() {
            self.push_shape(Geometry::Text {
                subpaths,
                content: "Atelier".into(),
                px: 96.0,
            });
            self.selection = None;
        }
        if let Some(r) = self.doc.layers[0].kind.raster_mut() {
            let settings = BrushSettings {
                size: 42.0,
                color: Color32::from_rgb(0xE5, 0x48, 0x4D),
                flow: 1.0,
            };
            brush::stroke_segment(&mut r.pixmap, pos2(880.0, 120.0), pos2(1120.0, 640.0), &settings);
            brush::stamp(&mut r.pixmap, pos2(980.0, 380.0), &settings);
            r.touch();
        }
        self.status = "demo content loaded".into();
    }

    pub fn new_document(&mut self) {
        self.doc = Document::new(1280.0, 800.0);
        self.history.clear();
        self.selection = None;
        self.op = None;
        self.active_layer = Some(1);
        self.need_fit = true;
        self.status = "new document".into();
    }

    pub fn save_project(&mut self) {
        match crate::project::save(&self.doc) {
            Ok(path) => self.status = format!("saved {path}"),
            Err(e) => self.status = format!("save failed: {e}"),
        }
    }

    pub fn open_project(&mut self) {
        match crate::project::load() {
            Ok(doc) => {
                self.doc = doc;
                self.history.clear();
                self.selection = None;
                self.op = None;
                self.active_layer = Some(0);
                self.need_fit = true;
                self.status = "project loaded".into();
            }
            Err(e) => self.status = format!("open failed: {e}"),
        }
    }

    pub fn do_undo(&mut self) {
        if let Some(inv) = self.history.undo() {
            apply_cmd(&mut self.doc, &inv);
            self.sanitize();
            self.status = "undo".into();
        }
    }

    pub fn do_redo(&mut self) {
        if let Some(inv) = self.history.redo() {
            apply_cmd(&mut self.doc, &inv);
            self.sanitize();
            self.status = "redo".into();
        }
    }

    fn sanitize(&mut self) {
        self.selection = self.selection.and_then(|(li, sid)| {
            let ok = self
                .doc
                .layers
                .get(li)
                .and_then(|l| l.find_shape_by_id(sid))
                .is_some();
            ok.then_some((li, sid))
        });
        if let Some(li) = self.active_layer
            && li >= self.doc.layers.len()
        {
            self.active_layer = None;
        }
        if let Some((li, sid)) = self.bool_second
            && self
                .doc
                .layers
                .get(li)
                .and_then(|l| l.find_shape_by_id(sid))
                .is_none()
        {
            self.bool_second = None;
        }
    }

    pub fn add_layer(&mut self, raster: bool) {
        let n = self.doc.layers.len() + 1;
        let layer = if raster {
            crate::document::Layer {
                name: format!("Pixel {n}"),
                visible: true,
                locked: false,
                opacity: 1.0,
                blend: LayerBlend::Normal,
                mask: None,
                kind: crate::document::LayerKind::Raster(
                    crate::document::RasterLayer::new(self.doc.width as u32, self.doc.height as u32)
                        .unwrap(),
                ),
            }
        } else {
            crate::document::Layer {
                name: format!("Vector {n}"),
                visible: true,
                locked: false,
                opacity: 1.0,
                blend: LayerBlend::Normal,
                mask: None,
                kind: crate::document::LayerKind::Vector(crate::document::VectorLayer {
                    shapes: vec![],
                }),
            }
        };
        self.doc.layers.push(layer);
        self.active_layer = Some(self.doc.layers.len() - 1);
        self.status = "layer added".into();
    }

    pub fn duplicate_active_layer(&mut self) {
        let Some(li) = self.active_layer else {
            return;
        };
        let mut layer = self.doc.layers[li].clone();
        layer.name = format!("{} copy", layer.name);
        let idx = li + 1;
        self.history.push(Cmd::AddLayer {
            index: idx,
            layer: layer.clone(),
        });
        self.doc.layers.insert(idx, layer);
        self.active_layer = Some(idx);
        self.status = "layer duplicated".into();
    }

    pub fn delete_active_layer(&mut self) {
        if let Some(li) = self.active_layer {
            if self.doc.layers.len() <= 1 {
                self.status = "cannot delete the last layer".into();
                return;
            }
            let layer = self.doc.layers.remove(li);
            self.history.push(Cmd::RemoveLayer { index: li, layer });
            self.active_layer = Some(li.min(self.doc.layers.len() - 1));
            self.selection = None;
            self.status = "layer deleted (ctrl+z to undo)".into();
        }
    }

    pub fn toggle_mask(&mut self) {
        let Some(li) = self.active_layer else {
            return;
        };
        if self.doc.layers[li].mask.is_some() {
            self.doc.layers[li].mask = None;
            self.edit_mask = false;
            self.status = "mask removed".into();
        } else {
            self.doc.layers[li].mask = crate::document::RasterLayer::new(
                self.doc.width as u32,
                self.doc.height as u32,
            );
            self.edit_mask = true;
            self.status = "mask added - paint with B to hide (black hides)".into();
        }
    }

    pub fn export_png(&mut self) {
        match render::export::png_bytes_scaled(&self.doc, self.export_scale as f32) {
            Ok(bytes) => {
                let path = format!(
                    "atelier-export-{}.png",
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_secs())
                        .unwrap_or(0)
                );
                match std::fs::write(&path, &bytes) {
                    Ok(_) => self.status = format!("saved {path}"),
                    Err(e) => self.status = format!("export failed: {e}"),
                }
            }
            Err(e) => self.status = format!("export failed: {e}"),
        }
    }

    pub fn export_svg(&mut self) {
        match crate::svg::export(&self.doc) {
            Ok(_) => self.status = "saved atelier-export.svg".into(),
            Err(e) => self.status = format!("svg failed: {e}"),
        }
    }

    pub fn apply_boolean(&mut self, op: crate::boolean::BoolOp) {
        let Some((li, a_id)) = self.selection else {
            self.status = "select the base shape first".into();
            return;
        };
        let Some((_, b_id)) = self.bool_second else {
            self.pending_bool = Some(op);
            self.status = "now click the second shape".into();
            return;
        };
        if a_id == b_id {
            self.status = "pick two different shapes".into();
            return;
        }
        let ga = self
            .doc
            .layers
            .get(li)
            .and_then(|l| l.find_shape_by_id(a_id))
            .map(|s| s.geom.clone());
        let gb = self
            .doc
            .layers
            .get(li)
            .and_then(|l| l.find_shape_by_id(b_id))
            .map(|s| s.geom.clone());
        let (Some(ga), Some(gb)) = (ga, gb) else {
            return;
        };
        match crate::boolean::apply(op, &ga, &gb) {
            Some(result) => {
                self.history.push(Cmd::SetGeometry {
                    layer: li,
                    id: a_id,
                    before: ga,
                    after: result.clone(),
                });
                if let Some(s) = self.doc.find_shape_mut(li, a_id) {
                    s.geom = result;
                }
                let b_shape = self.doc.layers.get_mut(li).and_then(|l| {
                    l.kind
                        .vector_shapes_mut()
                        .and_then(|vs| vs.iter().position(|s| s.id == b_id).map(|p| (vs, p)))
                });
                if let Some((vs, pos)) = b_shape {
                    let removed = vs.remove(pos);
                    self.history.push(Cmd::RemoveShapes {
                        layer: li,
                        shapes: vec![removed],
                    });
                }
                self.bool_second = None;
                self.status = format!("{} applied", op.name());
            }
            None => self.status = format!("{} produced empty result", op.name()),
        }
    }

    fn push_shape(&mut self, geom: Geometry) {
        let style = Style {
            fill: self.default_style.fill.clone(),
            stroke: self.default_style.stroke,
        };
        let shape = Shape {
            id: next_shape_id(),
            geom,
            style,
        };
        let li = match self.ensure_vector_target() {
            Some(i) => i,
            None => {
                self.status = "no unlocked visible vector layer".into();
                return;
            }
        };
        let id = shape.id;
        self.history.push(Cmd::AddShape {
            layer: li,
            shape: shape.clone(),
        });
        apply_cmd(&mut self.doc, &Cmd::AddShape { layer: li, shape });
        self.selection = Some((li, id));
        self.active_layer = Some(li);
    }

    pub fn push_styled_shape(&mut self, geom: Geometry, style: Style) {
        let li = match self.ensure_vector_target() {
            Some(i) => i,
            None => {
                self.status = "no unlocked visible vector layer".into();
                return;
            }
        };
        let shape = Shape {
            id: next_shape_id(),
            geom,
            style,
        };
        let id = shape.id;
        self.history.push(Cmd::AddShape {
            layer: li,
            shape: shape.clone(),
        });
        apply_cmd(&mut self.doc, &Cmd::AddShape { layer: li, shape });
        self.selection = Some((li, id));
        self.active_layer = Some(li);
    }

    fn ensure_vector_target(&mut self) -> Option<usize> {
        if let Some(li) = self.active_layer
            && let Some(l) = self.doc.layers.get(li)
            && l.visible
            && !l.locked
            && matches!(l.kind, crate::document::LayerKind::Vector(_))
        {
            return Some(li);
        }
        for (li, l) in self.doc.layers.iter().enumerate() {
            if l.visible
                && !l.locked
                && matches!(l.kind, crate::document::LayerKind::Vector(_))
            {
                self.active_layer = Some(li);
                return Some(li);
            }
        }
        None
    }

    fn duplicate_selection(&mut self) {
        if let Some((li, sid)) = self.selection {
            let clone = self
                .doc
                .layers
                .get(li)
                .and_then(|l| l.find_shape_by_id(sid))
                .cloned();
            if let Some(mut shape) = clone {
                shape.id = next_shape_id();
                shape.geom.translate(Vec2::new(12.0, 12.0));
                let new_id = shape.id;
                self.history.push(Cmd::AddShape {
                    layer: li,
                    shape: shape.clone(),
                });
                apply_cmd(&mut self.doc, &Cmd::AddShape { layer: li, shape });
                self.selection = Some((li, new_id));
                self.status = "duplicated".into();
            }
        }
    }

    fn nudge_selection(&mut self, d: Vec2) {
        if let Some((li, sid)) = self.selection
            && self.doc.find_shape_mut(li, sid).is_some()
        {
            self.history.push(Cmd::TranslateShape {
                layer: li,
                id: sid,
                delta: d,
            });
            if let Some(s) = self.doc.find_shape_mut(li, sid) {
                s.geom.translate(d);
            }
        }
    }
}

impl eframe::App for AtelierApp {
    fn ui(&mut self, ui: &mut eframe::egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.handle_shortcuts(&ctx);

        ui::top_bar(ui, self);
        ui::left_toolbar(ui, self);
        ui::right_panel(ui, self);
        ui::status_bar(ui, self);

        self.canvas_ui(ui);
    }
}

impl AtelierApp {
    fn handle_shortcuts(&mut self, ctx: &Context) {
        if ctx.egui_wants_keyboard_input() {
            return;
        }
        ctx.input(|i| {
            if i.modifiers.command && i.key_pressed(Key::Z) {
                if i.modifiers.shift {
                    self.do_redo();
                } else {
                    self.do_undo();
                }
            }
            if i.modifiers.command && i.key_pressed(Key::Y) {
                self.do_redo();
            }
            if i.modifiers.command && i.key_pressed(Key::S) {
                self.save_project();
            }
            if i.modifiers.command && i.key_pressed(Key::O) {
                self.open_project();
            }
            if i.modifiers.command && i.key_pressed(Key::E) {
                self.export_png();
            }
            if i.modifiers.command && i.key_pressed(Key::D) {
                self.duplicate_selection();
            }
            if i.key_pressed(Key::Delete) {
                self.delete_selection_key();
            }
            if i.key_pressed(Key::Escape) {
                self.op = None;
                self.selection = None;
                self.pending_bool = None;
                self.bool_second = None;
            }
            let nudge = [
                (Key::ArrowUp, Vec2::new(0.0, -1.0)),
                (Key::ArrowDown, Vec2::new(0.0, 1.0)),
                (Key::ArrowLeft, Vec2::new(-1.0, 0.0)),
                (Key::ArrowRight, Vec2::new(1.0, 0.0)),
            ];
            for (key, dir) in nudge {
                if i.key_pressed(key) {
                    let step = if i.modifiers.shift { 10.0 } else { 1.0 };
                    self.nudge_selection(dir * step);
                }
            }
            if i.key_pressed(Key::Equals) || i.key_pressed(Key::Plus) {
                if let Some(c) = self.canvas_rect {
                    self.view.zoom_at(c.center(), 1.25);
                }
            }
            if i.key_pressed(Key::Minus) {
                if let Some(c) = self.canvas_rect {
                    self.view.zoom_at(c.center(), 0.8);
                }
            }
            if i.key_pressed(Key::Num0) {
                self.need_fit = true;
            }
            if i.key_pressed(Key::Num1) {
                if let Some(c) = self.canvas_rect {
                    self.view.zoom_at(c.center(), 1.0 / self.view.scale);
                }
            }
            let pen_active =
                matches!(self.tool, Tool::Pen) && matches!(&self.op, Some(ActiveOp::PenDraft { .. }));
            if !pen_active {
                for (key, tool) in [
                    (Key::V, Tool::Select),
                    (Key::R, Tool::Rect),
                    (Key::O, Tool::Ellipse),
                    (Key::P, Tool::Pen),
                    (Key::B, Tool::Brush),
                    (Key::T, Tool::Text),
                ] {
                    if i.key_pressed(key) {
                        self.tool = tool;
                    }
                }
            }
            if pen_active && i.key_pressed(Key::Enter) {
                self.finish_pen(false);
            }
        });
    }

    fn delete_selection_key(&mut self) {
        if let Some((li, sid)) = self.selection.take() {
            let found = self.doc.layers.get_mut(li).and_then(|l| {
                l.kind
                    .vector_shapes_mut()
                    .and_then(|vs| vs.iter().position(|s| s.id == sid).map(|p| (vs, p)))
            });
            if let Some((vs, pos)) = found {
                let shape = vs.remove(pos);
                self.history.push(Cmd::RemoveShapes {
                    layer: li,
                    shapes: vec![shape],
                });
                self.status = "shape deleted".into();
            }
        }
    }

    fn finish_pen(&mut self, closed: bool) {
        if let Some(ActiveOp::PenDraft { mut anchors, .. }) = self.op.take() {
            if anchors.len() >= 2 {
                if anchors.len() > 1 {
                    let last = anchors.last().unwrap().pt;
                    let prev = anchors[anchors.len() - 2].pt;
                    if (last - prev).length() < 1e-3 {
                        anchors.pop();
                    }
                }
                if anchors.len() >= 2 {
                    self.push_shape(Geometry::Path { anchors, closed });
                }
            }
        }
    }

    fn brush_target(&self) -> Option<(usize, bool)> {
        let li = self.active_layer?;
        let l = self.doc.layers.get(li)?;
        if !l.visible || l.locked {
            return None;
        }
        match &l.kind {
            crate::document::LayerKind::Raster(_) => Some((li, false)),
            crate::document::LayerKind::Vector(_) if l.mask.is_some() && self.edit_mask => {
                Some((li, true))
            }
            _ => None,
        }
    }

    #[allow(clippy::manual_checked_ops)]
    fn canvas_ui(&mut self, ui: &mut eframe::egui::Ui) {
        let avail = ui.available_size();
        let (resp, painter) = ui.allocate_painter(avail, Sense::click_and_drag());
        self.canvas_rect = Some(resp.rect);
        let view = self.view;

        if let Some(p) = resp.hover_pos() {
            self.cursor_world = Some(view.to_world(p));
        }

        let scroll = ui.ctx().input(|i| i.smooth_scroll_delta);
        let ctrl = ui.ctx().input(|i| i.modifiers.ctrl || i.modifiers.command);
        if scroll != Vec2::ZERO {
            if ctrl {
                if let Some(p) = resp.hover_pos() {
                    self.view.zoom_at(p, (scroll.y * 0.01).exp());
                }
            } else {
                self.view.offset += Vec2::new(scroll.x, scroll.y);
            }
        }

        let space_held = ui.ctx().input(|i| i.keys_down.contains(&Key::Space));
        if resp.dragged_by(PointerButton::Middle)
            || (space_held && resp.dragged_by(PointerButton::Primary))
        {
            self.view.offset += resp.drag_delta();
        } else if resp.dragged_by(PointerButton::Primary)
            || resp.drag_started_by(PointerButton::Primary)
            || resp.clicked()
            || resp.double_clicked()
            || resp.secondary_clicked()
        {
            self.handle_canvas_pointer(&resp);
        }

        if self.need_fit
            && let Some(r) = self.canvas_rect
            && r.width() > 10.0
        {
            self.view.fit(Vec2::new(self.doc.width, self.doc.height), r);
            self.need_fit = false;
        }

        let (preview_shape, pen_anchors, brush_buf): (Option<Shape>, Option<Vec<Anchor>>, Option<(usize, tiny_skia::Pixmap, f32)>) =
            match (&self.tool, &self.op) {
                (Tool::Rect | Tool::Ellipse, Some(ActiveOp::DragCreate { rect, start, cur })) => {
                    let g = drag_geom(*rect, *start, *cur);
                    (
                        Some(Shape {
                            id: 0,
                            geom: g,
                            style: self.default_style.clone(),
                        }),
                        None,
                        None,
                    )
                }
                (Tool::Pen, Some(ActiveOp::PenDraft { anchors, .. })) => {
                    (None, Some(anchors.clone()), None)
                }
                (Tool::Brush, Some(ActiveOp::Brushing { layer, buffer, .. })) => {
                    (None, None, Some((*layer, buffer.pixmap.clone(), self.brush.flow)))
                }
                _ => (None, None, None),
            };

        let draft = RenderDraft {
            preview_shape: preview_shape.as_ref(),
            pen_preview: None,
            brush: brush_buf
                .as_ref()
                .map(|(l, pm, f)| (*l, pm, *f)),
        };

        if let Some(pm) = render::render_view(
            &self.doc,
            self.view,
            resp.rect.width().max(1.0) as u32,
            resp.rect.height().max(1.0) as u32,
            draft,
        ) {
            let image = eframe::egui::ColorImage::from_rgba_unmultiplied(
                [pm.width() as usize, pm.height() as usize],
                &render::straight_rgba(&pm),
            );
            let tex = ui.ctx().load_texture("canvas", image, eframe::egui::TextureOptions::LINEAR);
            painter.image(
                tex.id(),
                resp.rect,
                eframe::egui::Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                Color32::WHITE,
            );
            self.screen_tex = Some(tex);
        }

        if let Some(anchors) = pen_anchors
            && let Some(cursor) = self.cursor_world
        {
            draw_pen_overlay(&painter, &anchors, cursor, self.view);
        }

        if let Some((li, sid)) = self.selection
            && let Some(layer) = self.doc.layers.get(li)
            && let Some(shape) = layer.find_shape_by_id(sid)
        {
            draw_selection_overlay(&painter, &shape.geom, self.view);
        }
    }

    fn selection_handle_at(&self, world: Pos2, radius_world: f32) -> Option<(usize, u64, usize)> {
        let (li, sid) = self.selection?;
        let geom = self.doc.layers.get(li)?.find_shape_by_id(sid)?.geom.clone();
        let b = geom.bbox();
        for (i, (hx, hy)) in HANDLES.iter().enumerate() {
            let corner = pos2(
                b.min.x + (b.max.x - b.min.x) * hx,
                b.min.y + (b.max.y - b.min.y) * hy,
            );
            if (corner - world).length() <= radius_world {
                return Some((li, sid, i));
            }
        }
        None
    }

    pub fn create_text_at(&mut self, at: Pos2) {
        let content = "Text".to_string();
        let px = 48.0;
        let subpaths = crate::text::shape_text(&content, px, at);
        if subpaths.is_empty() {
            self.status = "no font found on this system".into();
            return;
        }
        let li = match self.ensure_vector_target() {
            Some(i) => i,
            None => {
                self.status = "no unlocked visible vector layer".into();
                return;
            }
        };
        let shape = Shape {
            id: next_shape_id(),
            geom: Geometry::Text {
                subpaths,
                content,
                px,
            },
            style: Style {
                fill: self.default_style.fill.clone(),
                stroke: None,
            },
        };
        let id = shape.id;
        self.history.push(Cmd::AddShape {
            layer: li,
            shape: shape.clone(),
        });
        apply_cmd(&mut self.doc, &Cmd::AddShape { layer: li, shape });
        self.selection = Some((li, id));
        self.active_layer = Some(li);
        self.status = "edit content in Properties".into();
    }

    pub fn text_live_edit(&mut self, content: &str, px: f32) {
        if let Some((li, sid)) = self.selection {
            let before = self
                .doc
                .layers
                .get(li)
                .and_then(|l| l.find_shape_by_id(sid))
                .map(|s| s.geom.clone());
            let Some(Geometry::Text { .. }) = &before else {
                return;
            };
            let at = before.as_ref().unwrap().bbox().min;
            if self.text_before.is_none() {
                self.text_before = Some((li, sid, before.clone().unwrap()));
            }
            let subpaths = crate::text::shape_text(content, px, at);
            if subpaths.is_empty() {
                return;
            }
            let after = Geometry::Text {
                subpaths,
                content: content.to_string(),
                px,
            };
            if let Some(s) = self.doc.find_shape_mut(li, sid) {
                s.geom = after;
            }
        }
    }

    pub fn text_commit(&mut self) {
        if let Some((li, sid, before)) = self.text_before.take() {
            let after = self
                .doc
                .layers
                .get(li)
                .and_then(|l| l.find_shape_by_id(sid))
                .map(|s| s.geom.clone());
            if let Some(after) = after
                && after != before
            {
                self.history.push(Cmd::SetGeometry {
                    layer: li,
                    id: sid,
                    before,
                    after,
                });
            }
        }
    }

    fn handle_canvas_pointer(&mut self, resp: &eframe::egui::Response) {
        let view = self.view;
        let world_of =
            |r: &eframe::egui::Response| r.interact_pointer_pos().map(|p| view.to_world(p));

        if resp.secondary_clicked() {
            if let Some(ActiveOp::PenDraft { .. }) = self.op {
                self.op = None;
            }
            return;
        }

        if resp.double_clicked() {
            if let Some(ActiveOp::PenDraft { .. }) = self.op {
                self.finish_pen(false);
                return;
            }
        }

        if resp.drag_started_by(PointerButton::Primary) {
            let Some(world) = world_of(resp) else {
                return;
            };
            if matches!(self.tool, Tool::Pen) {
                let near_first = match &self.op {
                    Some(ActiveOp::PenDraft { anchors, .. }) => {
                        anchors.len() >= 3
                            && (anchors[0].pt - world).length() < 10.0 / self.view.scale.max(0.001)
                    }
                    _ => false,
                };
                if near_first {
                    self.finish_pen(true);
                } else {
                    match &mut self.op {
                        Some(ActiveOp::PenDraft { anchors, .. }) => {
                            anchors.push(Anchor::corner(world));
                            if let Some(ActiveOp::PenDraft { smoothing_last, .. }) = &mut self.op {
                                *smoothing_last = true;
                            }
                        }
                        None => {
                            self.op = Some(ActiveOp::PenDraft {
                                anchors: vec![Anchor::corner(world)],
                                smoothing_last: true,
                            });
                        }
                        _ => {}
                    }
                }
                return;
            }
            match self.tool {
                Tool::Select => {
                    if let Some((li, sid, corner)) = self.selection_handle_at(world, 8.0) {
                        if let Some(orig) = self.doc.find_shape_mut(li, sid).map(|s| s.geom.clone())
                        {
                            self.op = Some(ActiveOp::Resizing {
                                layer: li,
                                id: sid,
                                orig,
                                corner,
                            });
                            return;
                        }
                    }
                    if self.pending_bool.is_some() {
                        if let Some(hit) = self.doc.hit_test(world, 6.0 / self.view.scale.max(0.001))
                        {
                            self.bool_second = Some(hit);
                            self.status = "second shape picked - choose an operation".into();
                        }
                        self.pending_bool = None;
                        return;
                    }
                    let hit = self.doc.hit_test(world, 6.0 / self.view.scale.max(0.001));
                    if hit != self.selection {
                        self.selection = hit;
                    }
                    if let Some((li, sid)) = hit
                        && let Some(orig) = self.doc.find_shape_mut(li, sid).map(|s| s.geom.clone())
                    {
                        self.op = Some(ActiveOp::Moving {
                            layer: li,
                            id: sid,
                            orig,
                            start_world: world,
                        });
                    }
                }
                Tool::Rect | Tool::Ellipse => {
                    self.op = Some(ActiveOp::DragCreate {
                        rect: matches!(self.tool, Tool::Rect),
                        start: world,
                        cur: world,
                    });
                }
                Tool::Brush => match self.brush_target() {
                    Some((layer, into_mask)) => {
                        if let Some(buf) =
                            StrokeBuffer::new(self.doc.width as u32, self.doc.height as u32)
                        {
                            let before = if into_mask {
                                self.doc.layers[layer].mask.as_ref().unwrap().pixmap.data().to_vec()
                            } else {
                                self.doc.layers[layer]
                                    .kind
                                    .raster_mut()
                                    .unwrap()
                                    .pixmap
                                    .data()
                                    .to_vec()
                            };
                            self.op = Some(ActiveOp::Brushing {
                                layer,
                                into_mask,
                                buffer: buf,
                                last: None,
                                before,
                            });
                        }
                    }
                    None => {
                        self.status =
                            "select an unlocked pixel layer, or a masked layer with mask-edit on"
                                .into();
                    }
                },
                Tool::Text => {
                    self.create_text_at(world);
                }
                Tool::Pen => {}
            }
            return;
        }

        if resp.dragged_by(PointerButton::Primary) {
            let Some(world) = world_of(resp) else {
                return;
            };
            match &mut self.op {
                Some(ActiveOp::DragCreate { cur, .. }) => *cur = world,
                Some(ActiveOp::PenDraft {
                    anchors,
                    smoothing_last,
                }) => {
                    if *smoothing_last
                        && let Some(last) = anchors.last_mut()
                    {
                        let drag = world - last.pt;
                        last.h_out = drag;
                        last.h_in = -drag;
                    }
                }
                Some(ActiveOp::Moving {
                    id,
                    orig,
                    start_world,
                    ..
                }) => {
                    let delta = world - *start_world;
                    if let Some((li, _)) = self.selection
                        && let Some(s) = self.doc.find_shape_mut(li, *id)
                    {
                        s.geom = orig.clone();
                        s.geom.translate(delta);
                    }
                }
                Some(ActiveOp::Resizing {
                    id, orig, corner, ..
                }) => {
                    if let Some((li, _)) = self.selection {
                        let g = resize_geometry(orig, *corner, world);
                        if let Some(s) = self.doc.find_shape_mut(li, *id) {
                            s.geom = g;
                        }
                    }
                }
                Some(ActiveOp::Brushing {
                    layer,
                    into_mask,
                    buffer,
                    last,
                    ..
                }) => {
                    let clamped = pos2(
                        world.x.clamp(0.0, self.doc.width),
                        world.y.clamp(0.0, self.doc.height),
                    );
                    if let Some(prev) = *last {
                        brush::stroke_segment(&mut buffer.pixmap, prev, clamped, &self.brush);
                    } else {
                        brush::stamp(&mut buffer.pixmap, clamped, &self.brush);
                    }
                    buffer.touch();
                    *last = Some(clamped);
                    let _ = (*layer, *into_mask);
                }
                _ => {}
            }
            return;
        }

        if resp.drag_stopped_by(PointerButton::Primary) {
            // Set smoothing_last = false before take() so pen adds corners after drag
            if let Some(ActiveOp::PenDraft { smoothing_last, .. }) = &mut self.op {
                *smoothing_last = false;
            }
            match self.op.take() {
                Some(ActiveOp::DragCreate { rect, start, cur }) => {
                    let g = drag_geom(rect, start, cur);
                    let ok = match &g {
                        Geometry::Rect { size, .. } => size.x.abs() >= 2.0 && size.y.abs() >= 2.0,
                        Geometry::Ellipse { radii, .. } => radii.x >= 1.0 && radii.y >= 1.0,
                        _ => false,
                    };
                    if ok {
                        self.push_shape(g);
                    }
                }
                Some(ActiveOp::PenDraft { .. }) => {
                    // Pen draft drag ended
                }
                Some(ActiveOp::Resizing {
                    layer,
                    id,
                    orig,
                    corner,
                }) => {
                    if let Some(world) = world_of(resp) {
                        let after = resize_geometry(&orig, corner, world);
                        if after != orig {
                            self.history.push(Cmd::SetGeometry {
                                layer,
                                id,
                                before: orig.clone(),
                                after: after.clone(),
                            });
                            if let Some(s) = self.doc.find_shape_mut(layer, id) {
                                s.geom = after;
                            }
                        }
                    }
                }
                Some(ActiveOp::Moving {
                    layer,
                    id,
                    orig,
                    start_world,
                }) => {
                    if let Some(world) = world_of(resp) {
                        let delta = world - start_world;
                        if delta.length_sq() > 1e-6 {
                            if let Some(s) = self.doc.find_shape_mut(layer, id) {
                                s.geom = orig.clone();
                            }
                            self.history.push(Cmd::TranslateShape { layer, id, delta });
                            if let Some(s) = self.doc.find_shape_mut(layer, id) {
                                s.geom.translate(delta);
                            }
                        }
                    }
                }
                Some(ActiveOp::Brushing {
                    layer,
                    into_mask,
                    buffer,
                    before,
                    ..
                }) => {
                    let target = if into_mask {
                        self.doc.layers[layer]
                            .mask
                            .as_mut()
                            .map(|m| &mut m.pixmap)
                    } else {
                        self.doc.layers[layer]
                            .kind
                            .raster_mut()
                            .map(|r| &mut r.pixmap)
                    };
                    if let Some(pm) = target {
                        brush::blend_onto(pm, &buffer.pixmap, self.brush.flow);
                        if into_mask {
                            self.doc.layers[layer].mask.as_mut().unwrap().touch();
                        } else {
                            self.doc.layers[layer]
                                .kind
                                .raster_mut()
                                .unwrap()
                                .touch();
                        }
                    }
                    let after = if into_mask {
                        self.doc.layers[layer]
                            .mask
                            .as_ref()
                            .unwrap()
                            .pixmap
                            .data()
                            .to_vec()
                    } else {
                        self.doc.layers[layer]
                            .kind
                            .raster_mut()
                            .unwrap()
                            .pixmap
                            .data()
                            .to_vec()
                    };
                    self.history.push(Cmd::BrushStroke {
                        layer,
                        before,
                        after,
                    });
                    self.status = "stroke committed".into();
                }
                _ => {}
            }
            return;
        }

        if resp.clicked() {
            if let (Tool::Select, Some(world)) = (self.tool, world_of(resp)) {
                if self.pending_bool.is_some() {
                    if let Some(hit) = self.doc.hit_test(world, 6.0 / self.view.scale.max(0.001)) {
                        self.bool_second = Some(hit);
                        self.status = "second shape picked - choose an operation".into();
                    }
                    self.pending_bool = None;
                    return;
                }
                let hit = self.doc.hit_test(world, 6.0 / self.view.scale.max(0.001));
                self.selection = hit;
            }
        }
    }
}

pub fn resize_geometry(orig: &Geometry, corner: usize, cur: Pos2) -> Geometry {
    let b = orig.bbox();
    let (hx, hy) = HANDLES[corner];
    let anchor = pos2(
        if hx == 0.0 { b.max.x } else { b.min.x },
        if hy == 0.0 { b.max.y } else { b.min.y },
    );
    let min = pos2(anchor.x.min(cur.x), anchor.y.min(cur.y));
    let max = pos2(anchor.x.max(cur.x), anchor.y.max(cur.y));
    let nb = Rect::from_min_max(min, max);
    if nb.width() < 1.0 || nb.height() < 1.0 {
        return orig.clone();
    }
    let map = |p: &Pos2| {
        pos2(
            nb.min.x + (p.x - b.min.x) / b.width() * nb.width(),
            nb.min.y + (p.y - b.min.y) / b.height() * nb.height(),
        )
    };
    match orig {
        Geometry::Rect { .. } => Geometry::Rect {
            origin: nb.min,
            size: nb.max - nb.min,
        },
        Geometry::Ellipse { .. } => Geometry::Ellipse {
            center: (nb.min.to_vec2() * 0.5 + nb.max.to_vec2() * 0.5).to_pos2(),
            radii: Vec2::new(nb.width() * 0.5, nb.height() * 0.5),
        },
        Geometry::Polyline { points, closed } => Geometry::Polyline {
            points: points.iter().map(map).collect(),
            closed: *closed,
        },
        Geometry::Path { anchors, closed } => Geometry::Path {
            anchors: anchors
                .iter()
                .map(|a| {
                    let pt = map(&a.pt);
                    let k = Vec2::new(
                        nb.width() / b.width(),
                        nb.height() / b.height(),
                    );
                    Anchor {
                        pt,
                        h_in: Vec2::new(a.h_in.x * k.x, a.h_in.y * k.y),
                        h_out: Vec2::new(a.h_out.x * k.x, a.h_out.y * k.y),
                    }
                })
                .collect(),
            closed: *closed,
        },
        Geometry::Text {
            content,
            px,
            ..
        } => {
            let scale_y = nb.height() / b.height();
            let mut subpaths = vec![];
            if let Geometry::Text { subpaths: sp, .. } = orig {
                for c in sp {
                    subpaths.push(c.iter().map(map).collect());
                }
            }
            Geometry::Text {
                subpaths,
                content: content.clone(),
                px: px * scale_y,
            }
        }
        Geometry::MultiPolygon { contours } => Geometry::MultiPolygon {
            contours: contours.iter().map(|c| c.iter().map(map).collect()).collect(),
        },
    }
}

fn drag_geom(rect: bool, start: Pos2, cur: Pos2) -> Geometry {
    if rect {
        let min = pos2(start.x.min(cur.x), start.y.min(cur.y));
        let max = pos2(start.x.max(cur.x), start.y.max(cur.y));
        Geometry::Rect {
            origin: min,
            size: max - min,
        }
    } else {
        let center = pos2((start.x + cur.x) * 0.5, (start.y + cur.y) * 0.5);
        Geometry::Ellipse {
            center,
            radii: vec2((cur.x - start.x).abs() * 0.5, (cur.y - start.y).abs() * 0.5),
        }
    }
}

fn draw_pen_overlay(
    painter: &eframe::egui::Painter,
    anchors: &[Anchor],
    cursor: Pos2,
    view: View,
) {
    use eframe::egui::{Shape as EShape, Stroke};
    if !anchors.is_empty() {
        let mut pts: Vec<Pos2> = anchors.iter().map(|a| view.to_screen(a.pt)).collect();
        pts.push(view.to_screen(cursor));
        painter.add(EShape::line(pts, Stroke::new(1.5, Color32::from_rgb(0x2F, 0x81, 0xF7))));
        for a in anchors {
            let sp = view.to_screen(a.pt);
            if a.h_out != Vec2::ZERO || a.h_in != Vec2::ZERO {
                let out = view.to_screen(a.pt + a.h_out);
                let inn = view.to_screen(a.pt + a.h_in);
                painter.line_segment([sp, out], Stroke::new(1.0, Color32::from_rgb(0x8A, 0xC1, 0xFF)));
                painter.line_segment([sp, inn], Stroke::new(1.0, Color32::from_rgb(0x8A, 0xC1, 0xFF)));
                painter.circle_filled(out, 2.5, Color32::from_rgb(0x8A, 0xC1, 0xFF));
                painter.circle_filled(inn, 2.5, Color32::from_rgb(0x8A, 0xC1, 0xFF));
            }
        }
    }
    for a in anchors {
        let sp = view.to_screen(a.pt);
        painter.rect_filled(
            Rect::from_center_size(sp, Vec2::splat(6.0)),
            0.0,
            Color32::from_rgb(0x2F, 0x81, 0xF7),
        );
    }
}

fn draw_selection_overlay(
    painter: &eframe::egui::Painter,
    geom: &Geometry,
    view: View,
) {
    use eframe::egui::Stroke;
    let closed = geom.is_closed_outline();
    for world_pts in geom.contours(render::ELLIPSE_SEGMENTS) {
        if world_pts.len() < 2 {
            continue;
        }
        let pts: Vec<Pos2> = world_pts.iter().map(|p| view.to_screen(*p)).collect();
        painter.add(eframe::egui::Shape::line(
            pts.clone(),
            Stroke::new(4.0, Color32::from_black_alpha(160)),
        ));
        painter.add(eframe::egui::Shape::line(
            pts,
            Stroke::new(1.5, Color32::from_rgb(0xFF, 0xFF, 0xFF)),
        ));
        let _ = closed;
    }
    let b = geom.bbox();
    let sb = Rect::from_min_max(view.to_screen(b.min), view.to_screen(b.max));
    for corner in [sb.left_top(), sb.right_top(), sb.right_bottom(), sb.left_bottom()] {
        painter.rect_filled(
            Rect::from_center_size(corner, Vec2::splat(7.0)),
            0.0,
            Color32::from_rgb(0x2F, 0x81, 0xF7),
        );
    }
}
