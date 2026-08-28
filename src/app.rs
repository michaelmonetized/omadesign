//! Studio: document + tool state. Mutations go through commands.

use crate::align::{self, Align, Distribute};
use crate::boolean::{self, BoolOp};
use crate::color::{Rgba, default_swatches};
use crate::compositor::{self, View};
use crate::document::{
    apply as apply_cmd, Cmd, Document, Fill, History, Layer, LayerKind, Shape, Stroke, Style,
};
use crate::geom::{Anchor, Bounds, Geom, Pt};
use crate::paint::{self, Brush};
use crate::photo::{self, Histogram, PhotoImage, RgbaImage};
use crate::presets::Preset;
use crate::snap::{self, SnapSettings};
use crate::tools::{Persona, Tool};
use eframe::egui::{self, Color32, Key, Pos2, Rect};
use std::path::PathBuf;
use tiny_skia::Pixmap;

pub struct PhotoSession {
    pub images: Vec<PhotoImage>,
    pub selected: Option<usize>,
    pub folder: String,
    pub folder_files: Vec<(String, String)>,
    pub view_scale: f32,
    pub show_original: bool,
    pub crop_drag: Option<(Pt, Pt)>,
    pub adjusted: Option<RgbaImage>,
    pub hists: [Histogram; 4],
    pub tex: Option<egui::TextureHandle>,
    pub orig_tex: Option<egui::TextureHandle>,
    pub thumbs: Vec<egui::TextureHandle>,
    pub sel_version: u64,
    pub built_version: u64,
    pub dirty: bool,
    pub samples_loaded: bool,
    pub status: String,
}

impl PhotoSession {
    pub fn new() -> Self {
        Self {
            images: vec![],
            selected: None,
            folder: String::new(),
            folder_files: vec![],
            view_scale: 1.0,
            show_original: false,
            crop_drag: None,
            adjusted: None,
            hists: Default::default(),
            tex: None,
            orig_tex: None,
            thumbs: vec![],
            sel_version: 0,
            built_version: 0,
            dirty: true,
            samples_loaded: false,
            status: "Open a folder, drop photos, or load samples".into(),
        }
    }

    pub fn selected(&self) -> Option<&PhotoImage> {
        self.selected.and_then(|i| self.images.get(i))
    }

    pub fn selected_mut(&mut self) -> Option<&mut PhotoImage> {
        self.selected.and_then(|i| self.images.get_mut(i))
    }

    pub fn import_file(&mut self, path: &std::path::Path) {
        let name = path
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| path.to_string_lossy().to_string());
        match photo::load_file(path) {
            Some(full) => {
                self.images.push(PhotoImage::from_full(name, full));
                self.selected = Some(self.images.len() - 1);
                self.sel_version += 1;
                self.dirty = true;
                self.status = "imported".into();
            }
            None => self.status = format!("could not open {}", path.display()),
        }
    }

    pub fn import_bytes(&mut self, name: String, bytes: &[u8]) {
        match photo::decode_bytes(bytes) {
            Some(full) => {
                let name = name
                    .split('/')
                    .next_back()
                    .unwrap_or(&name)
                    .to_string();
                self.images.push(PhotoImage::from_full(name, full));
                self.selected = Some(self.images.len() - 1);
                self.sel_version += 1;
                self.dirty = true;
                self.status = "imported".into();
            }
            None => self.status = "failed to decode image".into(),
        }
    }

    pub fn import_samples(&mut self) {
        for kind in 0..3 {
            let (name, full) = photo::sample_photo(kind);
            self.images.push(PhotoImage::from_full(name, full));
        }
        self.selected = Some(0);
        self.sel_version += 1;
        self.samples_loaded = true;
        self.dirty = true;
        self.status = "sample photos loaded".into();
    }

    pub fn set_folder(&mut self, path: &str) {
        let mut entries = vec![];
        if let Ok(read) = std::fs::read_dir(path) {
            for e in read.flatten() {
                let p = e.path();
                if p.is_file()
                    && let Some(ext) = p.extension().and_then(|s| s.to_str())
                    && matches!(
                        ext.to_ascii_lowercase().as_str(),
                        "jpg" | "jpeg" | "png" | "webp" | "gif" | "bmp" | "tif" | "tiff"
                    )
                {
                    entries.push((
                        p.file_name()
                            .map(|s| s.to_string_lossy().to_string())
                            .unwrap_or_default(),
                        p.to_string_lossy().to_string(),
                    ));
                }
            }
        }
        entries.sort();
        self.folder = path.to_string();
        self.folder_files = entries;
    }

    pub fn rebuild(&mut self) {
        let Some(img) = self.selected() else {
            return;
        };
        let src = if img.preview.w * img.preview.h > 0 {
            &img.preview
        } else {
            &img.full
        };
        let out = photo::develop(src, &img.develop);
        self.hists = photo::histograms(&out);
        self.adjusted = Some(out);
        self.built_version = self.sel_version;
        self.dirty = false;
    }
}

#[derive(Clone, Copy)]
pub enum CreateKind {
    Rect,
    Ellipse,
    Polygon,
    Star,
    Line,
}

pub enum NodeHit {
    Point(usize),
    HandleIn(usize),
    HandleOut(usize),
}

pub enum Op {
    Create {
        kind: CreateKind,
        start: Pt,
        cur: Pt,
    },
    Pen {
        anchors: Vec<Anchor>,
    },
    Pencil {
        pts: Vec<Pt>,
    },
    Move {
        orig: Vec<(usize, u64, Geom, f32)>,
        start: Pt,
    },
    Resize {
        orig: Vec<(usize, u64, Geom, f32)>,
        handle: usize,
        start_box: Bounds,
    },
    Rotate {
        orig: Vec<(usize, u64, Geom, f32)>,
        center: Pt,
        start_angle: f32,
    },
    Node {
        layer: usize,
        id: u64,
        which: NodeHit,
        orig: Geom,
    },
    Brush {
        layer: usize,
        erase: bool,
        buf: Pixmap,
        last: Option<Pt>,
        before: Vec<u8>,
    },
    Smudge {
        layer: usize,
        last: Option<Pt>,
        before: Vec<u8>,
    },
    Clone {
        layer: usize,
        last: Option<Pt>,
        before: Vec<u8>,
    },
    Marquee {
        start: Pt,
        cur: Pt,
        ellipse: bool,
    },
    Lasso {
        pts: Vec<Pt>,
    },
    Gradient {
        start: Pt,
        cur: Pt,
    },
    CropPhoto {
        start: Pt,
        cur: Pt,
    },
}

pub struct Studio {
    pub doc: Document,
    pub path: Option<PathBuf>,
    pub dirty: bool,
    pub view: View,
    pub persona: Persona,
    pub tool: Tool,
    pub last_tool: Tool,
    pub op: Option<Op>,
    pub selection: Vec<(usize, u64)>,
    pub active_layer: Option<usize>,
    pub history: History,
    pub style: Style,
    pub gradient: (Rgba, Rgba),
    pub brush: Brush,
    pub fill_tolerance: f32,
    pub clone_source: Option<Pt>,
    pub pixel_sel: Option<Vec<u8>>,
    pub snap: SnapSettings,
    pub photo: PhotoSession,
    pub status: String,
    pub cursor: Option<Pt>,
    pub canvas_rect: Option<Rect>,
    pub need_fit: bool,
    pub export_scale: u32,
    pub show_welcome: bool,
    pub show_shortcuts: bool,
    pub show_rulers: bool,
    pub show_grid: bool,
    pub text_buf: String,
    pub text_px: f32,
    pub polygon_sides: u32,
    pub star_points: u32,
    pub star_inner: f32,
    pub rect_radius: f32,
    pub screen_tex: Option<egui::TextureHandle>,
    pub recent: Vec<Rgba>,
    pub hex_buf: String,
    pub swatches: Vec<Rgba>,
    pub bool_pick: Option<BoolOp>,
}

impl Studio {
    pub fn new() -> Self {
        let mut s = Self {
            doc: Document::new("Untitled", 1280.0, 800.0, 72.0),
            path: None,
            dirty: false,
            view: View::default(),
            persona: Persona::Design,
            tool: Tool::Select,
            last_tool: Tool::Select,
            op: None,
            selection: vec![],
            active_layer: Some(1),
            history: History::default(),
            style: Style::default(),
            gradient: (Rgba::from_hex(0xFFD166), Rgba::from_hex(0xE5484D)),
            brush: Brush::default(),
            fill_tolerance: 32.0,
            clone_source: None,
            pixel_sel: None,
            snap: SnapSettings::default(),
            photo: PhotoSession::new(),
            status: "Welcome home. V to move, R for a rectangle, B to paint, or open Photo."
                .into(),
            cursor: None,
            canvas_rect: None,
            need_fit: true,
            export_scale: 1,
            show_welcome: true,
            show_shortcuts: false,
            show_rulers: true,
            show_grid: false,
            text_buf: "Type".into(),
            text_px: 72.0,
            polygon_sides: 6,
            star_points: 5,
            star_inner: 0.4,
            rect_radius: 0.0,
            screen_tex: None,
            recent: vec![],
            hex_buf: "#4F8CFF".into(),
            swatches: default_swatches(),
            bool_pick: None,
        };
        s.doc.grid.visible = false;
        s
    }

    pub fn new_from_preset(&mut self, p: Preset) {
        self.doc = Document::new(p.name, p.w, p.h, p.dpi);
        self.path = None;
        self.dirty = false;
        self.history.clear();
        self.selection.clear();
        self.active_layer = Some(1);
        self.need_fit = true;
        self.show_welcome = false;
        self.op = None;
        self.status = format!("{} · {:.0}×{:.0} @ {} dpi", p.name, p.w, p.h, p.dpi);
    }

    pub fn seed_demo(&mut self) {
        self.show_welcome = false;
        self.doc = Document::new("omadesign", 1440.0, 900.0, 72.0);
        self.active_layer = Some(1);
        let navy = Rgba::from_hex(0x073B4C);
        let orange = Rgba::from_hex(0xF47C2E);
        let cream = Rgba::from_hex(0xF4EDE4);
        let teal = Rgba::from_hex(0x2EC4B6);

        let bg = Shape::new(
            Geom::Rect {
                origin: Pt::new(0.0, 0.0),
                size: Pt::new(1440.0, 900.0),
                radius: 0.0,
            },
            Style {
                fill: Fill::Solid(navy),
                stroke: None,
            },
        );
        self.commit(Cmd::AddShape {
            layer: 1,
            shape: bg,
        });

        let mark = Shape::new(
            Geom::Star {
                center: Pt::new(360.0, 420.0),
                outer: Pt::splat(160.0),
                inner: 0.42,
                points: 5,
            },
            Style {
                fill: Fill::Linear {
                    from: [0.0, 0.0],
                    to: [1.0, 1.0],
                    c0: orange,
                    c1: Rgba::from_hex(0xE5484D),
                },
                stroke: None,
            },
        );
        self.commit(Cmd::AddShape {
            layer: 1,
            shape: mark,
        });

        let disc = Shape::new(
            Geom::Ellipse {
                center: Pt::new(360.0, 420.0),
                radii: Pt::splat(58.0),
            },
            Style {
                fill: Fill::Solid(cream),
                stroke: None,
            },
        );
        self.commit(Cmd::AddShape {
            layer: 1,
            shape: disc,
        });

        let mut word = Shape::new(
            Geom::Text {
                origin: Pt::new(560.0, 460.0),
                content: "omadesign".into(),
                px: 96.0,
                tracking: -1.0,
                contours: vec![],
            },
            Style {
                fill: Fill::Solid(cream),
                stroke: None,
            },
        );
        if let Geom::Text {
            origin,
            content,
            px,
            tracking,
            contours,
        } = &mut word.geom
        {
            *contours = crate::text::shape(content, *px, *origin, *tracking, None);
        }
        self.commit(Cmd::AddShape {
            layer: 1,
            shape: word,
        });

        let mut tag = Shape::new(
            Geom::Text {
                origin: Pt::new(564.0, 520.0),
                content: "design  ·  paint  ·  photograph".into(),
                px: 28.0,
                tracking: 1.5,
                contours: vec![],
            },
            Style {
                fill: Fill::Solid(teal),
                stroke: None,
            },
        );
        if let Geom::Text {
            origin,
            content,
            px,
            tracking,
            contours,
        } = &mut tag.geom
        {
            *contours = crate::text::shape(content, *px, *origin, *tracking, None);
        }
        self.commit(Cmd::AddShape {
            layer: 1,
            shape: tag,
        });

        if let Some(px) = self.doc.layers[0].kind.pixels_mut() {
            if let Some(mut pm) = px.to_pixmap() {
                let b = Brush {
                    size: 64.0,
                    hardness: 0.2,
                    opacity: 0.35,
                    flow: 0.8,
                    spacing: 0.2,
                    color: orange,
                };
                paint::stroke_to(&mut pm, Pt::new(900.0, 80.0), Pt::new(1320.0, 780.0), &b, false);
                *px = crate::document::Pixels::from_pixmap(&pm);
            }
        }

        self.selection.clear();
        self.need_fit = true;
        self.status = "demo".into();
    }

    pub fn commit(&mut self, cmd: Cmd) {
        apply_cmd(&mut self.doc, &cmd);
        self.history.push(cmd);
        self.dirty = true;
        self.sanitize();
    }

    pub fn undo(&mut self) {
        if let Some(inv) = self.history.undo() {
            apply_cmd(&mut self.doc, &inv);
            self.sanitize();
            self.status = "undo".into();
        }
    }

    pub fn redo(&mut self) {
        if let Some(cmd) = self.history.redo() {
            apply_cmd(&mut self.doc, &cmd);
            self.sanitize();
            self.status = "redo".into();
        }
    }

    fn sanitize(&mut self) {
        self.selection.retain(|(li, id)| {
            self.doc
                .layers
                .get(*li)
                .and_then(|l| l.find(*id))
                .is_some()
        });
        if let Some(li) = self.active_layer
            && li >= self.doc.layers.len()
        {
            self.active_layer = self.doc.layers.len().checked_sub(1);
        }
    }

    pub fn selected_shapes(&self) -> Vec<&Shape> {
        self.selection
            .iter()
            .filter_map(|(li, id)| self.doc.find_shape(*li, *id))
            .collect()
    }

    pub fn primary(&self) -> Option<(usize, u64)> {
        self.selection.first().copied()
    }

    pub fn vector_target(&mut self) -> Option<usize> {
        if let Some(i) = self.active_layer
            && matches!(self.doc.layers.get(i).map(|l| &l.kind), Some(LayerKind::Vector { .. }))
            && !self.doc.layers[i].locked
        {
            return Some(i);
        }
        self.doc
            .layers
            .iter()
            .enumerate()
            .rev()
            .find(|(_, l)| matches!(l.kind, LayerKind::Vector { .. }) && !l.locked)
            .map(|(i, _)| i)
    }

    pub fn raster_target(&mut self) -> Option<usize> {
        if let Some(i) = self.active_layer
            && matches!(self.doc.layers.get(i).map(|l| &l.kind), Some(LayerKind::Raster { .. }))
            && !self.doc.layers[i].locked
        {
            return Some(i);
        }
        self.doc
            .layers
            .iter()
            .enumerate()
            .rev()
            .find(|(_, l)| matches!(l.kind, LayerKind::Raster { .. }) && !l.locked)
            .map(|(i, _)| i)
    }

    pub fn add_layer(&mut self, raster: bool) {
        let n = self.doc.layers.len() + 1;
        let layer = if raster {
            Layer::raster(
                format!("Pixel {n}"),
                self.doc.width as u32,
                self.doc.height as u32,
            )
        } else {
            Layer::vector(format!("Layer {n}"))
        };
        let index = self.doc.layers.len();
        self.commit(Cmd::AddLayer { index, layer });
        self.active_layer = Some(index);
    }

    pub fn delete_layer(&mut self) {
        if let Some(i) = self.active_layer
            && self.doc.layers.len() > 1
            && i < self.doc.layers.len()
        {
            let layer = self.doc.layers[i].clone();
            self.commit(Cmd::RemoveLayer { index: i, layer });
        }
    }

    pub fn delete_selection(&mut self) {
        let mut by_layer: std::collections::BTreeMap<usize, Vec<Shape>> =
            std::collections::BTreeMap::new();
        for (li, id) in self.selection.clone() {
            if let Some(s) = self.doc.find_shape(li, id).cloned() {
                by_layer.entry(li).or_default().push(s);
            }
        }
        for (layer, shapes) in by_layer {
            self.commit(Cmd::RemoveShapes { layer, shapes });
        }
        self.selection.clear();
    }

    pub fn duplicate_selection(&mut self) {
        let sel = self.selection.clone();
        let mut neu = vec![];
        for (li, id) in sel {
            if let Some(mut s) = self.doc.find_shape(li, id).cloned() {
                s.id = crate::document::next_id();
                s.geom.translate(Pt::new(16.0, 16.0));
                neu.push((li, s.id));
                self.commit(Cmd::AddShape { layer: li, shape: s });
            }
        }
        self.selection = neu;
    }

    pub fn nudge(&mut self, dx: f32, dy: f32) {
        let d = Pt::new(dx, dy);
        for (li, id) in self.selection.clone() {
            if let Some(s) = self.doc.find_shape(li, id) {
                let mut after = s.geom.clone();
                after.translate(d);
                self.commit(Cmd::SetGeom {
                    layer: li,
                    id,
                    before: s.geom.clone(),
                    after,
                    rot_before: s.rotation,
                    rot_after: s.rotation,
                });
            }
        }
    }

    pub fn place_text(&mut self, at: Pt) {
        let Some(li) = self.vector_target() else {
            self.status = "add a vector layer first".into();
            return;
        };
        let content = if self.text_buf.trim().is_empty() {
            "Type".to_string()
        } else {
            self.text_buf.clone()
        };
        let contours = crate::text::shape(&content, self.text_px, at, 0.0, None);
        let shape = Shape::new(
            Geom::Text {
                origin: at,
                content,
                px: self.text_px,
                tracking: 0.0,
                contours,
            },
            self.style.clone(),
        );
        let id = shape.id;
        self.commit(Cmd::AddShape { layer: li, shape });
        self.selection = vec![(li, id)];
        self.tool = Tool::Select;
    }

    pub fn finish_create(&mut self, kind: CreateKind, start: Pt, cur: Pt) {
        let Some(li) = self.vector_target() else {
            return;
        };
        let shift = false; // applied by caller via squaring
        let _ = shift;
        let min = Pt::new(start.x.min(cur.x), start.y.min(cur.y));
        let max = Pt::new(start.x.max(cur.x), start.y.max(cur.y));
        let size = max - min;
        if size.x.abs() < 2.0 && size.y.abs() < 2.0 && !matches!(kind, CreateKind::Line) {
            return;
        }
        let geom = match kind {
            CreateKind::Rect => Geom::Rect {
                origin: min,
                size,
                radius: self.rect_radius,
            },
            CreateKind::Ellipse => Geom::Ellipse {
                center: (min + max) * 0.5,
                radii: size * 0.5,
            },
            CreateKind::Polygon => Geom::Polygon {
                center: (min + max) * 0.5,
                radii: size * 0.5,
                sides: self.polygon_sides,
            },
            CreateKind::Star => Geom::Star {
                center: (min + max) * 0.5,
                outer: size * 0.5,
                inner: self.star_inner,
                points: self.star_points,
            },
            CreateKind::Line => Geom::Line { a: start, b: cur },
        };
        let shape = Shape::new(geom, self.style.clone());
        let id = shape.id;
        self.commit(Cmd::AddShape { layer: li, shape });
        self.selection = vec![(li, id)];
        self.status = "created".into();
    }

    pub fn finish_pen(&mut self, anchors: Vec<Anchor>, closed: bool) {
        if anchors.len() < 2 {
            return;
        }
        let Some(li) = self.vector_target() else {
            return;
        };
        if closed && anchors.len() >= 3 {
            // already closed by clicking first point
        }
        let shape = Shape::new(
            Geom::Path { anchors, closed },
            self.style.clone(),
        );
        let id = shape.id;
        self.commit(Cmd::AddShape { layer: li, shape });
        self.selection = vec![(li, id)];
    }

    pub fn finish_pencil(&mut self, pts: Vec<Pt>) {
        if pts.len() < 2 {
            return;
        }
        let Some(li) = self.vector_target() else {
            return;
        };
        let anchors: Vec<Anchor> = pts.into_iter().map(Anchor::corner).collect();
        let shape = Shape::new(
            Geom::Path {
                anchors,
                closed: false,
            },
            Style {
                fill: Fill::None,
                stroke: self.style.stroke.clone().or(Some(Stroke::default())),
            },
        );
        let id = shape.id;
        self.commit(Cmd::AddShape { layer: li, shape });
        self.selection = vec![(li, id)];
    }

    pub fn apply_boolean(&mut self, op: BoolOp) {
        if self.selection.len() < 2 {
            self.status = "select two shapes, then boolean".into();
            return;
        }
        let (la, ia) = self.selection[0];
        let (lb, ib) = self.selection[1];
        if la != lb {
            self.status = "boolean needs two shapes on the same layer".into();
            return;
        }
        let Some(a) = self.doc.find_shape(la, ia).cloned() else {
            return;
        };
        let Some(b) = self.doc.find_shape(lb, ib).cloned() else {
            return;
        };
        match boolean::apply(op, &a.geom, &b.geom) {
            Some(geom) => {
                let mut shape = a.clone();
                shape.id = crate::document::next_id();
                shape.geom = geom;
                shape.name = op.name().into();
                self.commit(Cmd::RemoveShapes {
                    layer: la,
                    shapes: vec![a, b],
                });
                let id = shape.id;
                self.commit(Cmd::AddShape { layer: la, shape });
                self.selection = vec![(la, id)];
                self.status = format!("{} applied", op.name());
            }
            None => self.status = "boolean produced nothing".into(),
        }
    }

    pub fn align_sel(&mut self, how: Align) {
        let ids = self.selection.clone();
        align::align(&mut self.doc, &ids, how);
        self.dirty = true;
        self.status = "aligned".into();
    }

    pub fn distribute_sel(&mut self, how: Distribute) {
        let ids = self.selection.clone();
        align::distribute(&mut self.doc, &ids, how);
        self.dirty = true;
        self.status = "distributed".into();
    }

    pub fn snap_pt(&self, p: Pt) -> Pt {
        snap::snap_point(&self.doc, self.snap, p, self.view.scale)
    }

    pub fn eyedrop(&mut self, p: Pt) {
        if let Some((_, id)) = self.doc.hit_test(p, 4.0 / self.view.scale.max(0.01)) {
            for layer in &self.doc.layers {
                if let Some(s) = layer.find(id) {
                    self.style.fill = s.style.fill.clone();
                    if let Fill::Solid(c) = s.style.fill {
                        self.push_recent(c);
                    }
                    self.status = "sampled fill".into();
                    return;
                }
            }
        }
        if let Some(li) = self.raster_target()
            && let Some(px) = self.doc.layers[li].kind.pixels()
        {
            let x = p.x.round().clamp(0.0, px.w as f32 - 1.0) as u32;
            let y = p.y.round().clamp(0.0, px.h as f32 - 1.0) as u32;
            let i = ((y * px.w + x) * 4) as usize;
            if i + 3 < px.data.len() {
                let c = Rgba::new(px.data[i], px.data[i + 1], px.data[i + 2], px.data[i + 3]);
                self.style.fill = Fill::Solid(c);
                self.brush.color = c;
                self.push_recent(c);
                self.status = format!("sampled {}", c.hex());
            }
        }
    }

    pub fn push_recent(&mut self, c: Rgba) {
        self.recent.retain(|x| *x != c);
        self.recent.insert(0, c);
        self.recent.truncate(16);
        self.hex_buf = c.hex();
    }

    pub fn save(&mut self) {
        let path = if let Some(p) = &self.path {
            Some(p.clone())
        } else {
            crate::project::dialog_save(&self.doc.name)
        };
        if let Some(path) = path {
            match crate::project::save_to(&self.doc, &path) {
                Ok(()) => {
                    self.path = Some(path.clone());
                    self.dirty = false;
                    self.status = format!("saved {}", path.display());
                }
                Err(e) => self.status = format!("save failed: {e}"),
            }
        }
    }

    pub fn open(&mut self) {
        if let Some(path) = crate::project::dialog_open() {
            let ext = path
                .extension()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_ascii_lowercase();
            if ext == "oma" {
                match crate::project::load_from(&path) {
                    Ok(doc) => {
                        self.doc = doc;
                        self.path = Some(path);
                        self.history.clear();
                        self.selection.clear();
                        self.active_layer = Some(0);
                        self.need_fit = true;
                        self.show_welcome = false;
                        self.status = "opened".into();
                    }
                    Err(e) => self.status = format!("open failed: {e}"),
                }
            } else {
                self.photo.import_file(&path);
                self.persona = Persona::Photo;
                self.show_welcome = false;
            }
        }
    }

    pub fn export_png(&mut self) {
        if let Some(path) = crate::project::dialog_export("PNG", "png") {
            match compositor::export_png(&self.doc, self.export_scale) {
                Ok(bytes) => {
                    if let Err(e) = std::fs::write(&path, bytes) {
                        self.status = format!("write failed: {e}");
                    } else {
                        self.status = format!("exported {}", path.display());
                    }
                }
                Err(e) => self.status = format!("export failed: {e}"),
            }
        }
    }

    pub fn export_jpeg(&mut self) {
        if let Some(path) = crate::project::dialog_export("JPEG", "jpg") {
            match compositor::export_jpeg(&self.doc, self.export_scale, 90) {
                Ok(bytes) => {
                    let _ = std::fs::write(&path, bytes);
                    self.status = format!("exported {}", path.display());
                }
                Err(e) => self.status = format!("export failed: {e}"),
            }
        }
    }

    pub fn export_svg(&mut self) {
        if let Some(path) = crate::project::dialog_export("SVG", "svg") {
            match crate::svg::export(&self.doc) {
                Ok(s) => {
                    let _ = std::fs::write(&path, s);
                    self.status = format!("exported {}", path.display());
                }
                Err(e) => self.status = format!("export failed: {e}"),
            }
        }
    }

    pub fn export_demo_png(&self, path: &str) -> Result<(), String> {
        let bytes = compositor::export_png(&self.doc, 1)?;
        std::fs::write(path, bytes).map_err(|e| e.to_string())
    }

    pub fn send_photo_to_design(&mut self) {
        let Some(img) = self.photo.selected() else {
            self.status = "no photo selected".into();
            return;
        };
        let developed = photo::develop(&img.full, &img.develop);
        let w = developed.w;
        let h = developed.h;
        let mut layer = Layer::raster(img.name.clone(), w, h);
        if let LayerKind::Raster { pixels } = &mut layer.kind {
            *pixels = crate::document::Pixels::from_rgba(w, h, developed.data)
                .unwrap_or_else(|| crate::document::Pixels::new(w, h));
        }
        if self.doc.layers.is_empty() {
            self.doc = Document::new(&img.name, w as f32, h as f32, 72.0);
            self.doc.layers.clear();
        }
        let index = self.doc.layers.len();
        self.commit(Cmd::AddLayer { index, layer });
        self.active_layer = Some(index);
        self.persona = Persona::Design;
        self.tool = Tool::Select;
        self.need_fit = true;
        self.status = "photo placed on a pixel layer".into();
    }

    pub fn handle_shortcuts(&mut self, ctx: &egui::Context) {
        let wants_text = ctx.egui_wants_keyboard_input();
        ctx.input(|i| {
            let mods = i.modifiers;
            let ctrl = mods.command || mods.ctrl;
            if ctrl && i.key_pressed(Key::S) {
                self.save();
                return;
            }
            if ctrl && i.key_pressed(Key::O) {
                self.open();
                return;
            }
            if ctrl && i.key_pressed(Key::N) {
                self.show_welcome = true;
                return;
            }
            if ctrl && i.key_pressed(Key::E) {
                self.export_png();
                return;
            }
            if ctrl && i.key_pressed(Key::Z) {
                if mods.shift {
                    self.redo();
                } else {
                    self.undo();
                }
                return;
            }
            if ctrl && i.key_pressed(Key::Y) {
                self.redo();
                return;
            }
            if ctrl && i.key_pressed(Key::D) {
                self.duplicate_selection();
                return;
            }
            if ctrl && i.key_pressed(Key::A) {
                self.selection = self
                    .doc
                    .layers
                    .iter()
                    .enumerate()
                    .flat_map(|(li, l)| {
                        l.kind
                            .shapes()
                            .into_iter()
                            .flatten()
                            .map(move |s| (li, s.id))
                    })
                    .collect();
                return;
            }
            if ctrl && i.key_pressed(Key::Num0) {
                self.need_fit = true;
                return;
            }
            if ctrl && i.key_pressed(Key::Num1) {
                self.view.scale = 1.0;
                return;
            }
            if i.key_pressed(Key::F1) {
                self.show_shortcuts = !self.show_shortcuts;
                return;
            }
            if wants_text {
                return;
            }
            if i.key_pressed(Key::Delete) || i.key_pressed(Key::Backspace) {
                self.delete_selection();
                return;
            }
            if i.key_pressed(Key::Escape) {
                self.op = None;
                self.bool_pick = None;
                return;
            }
            if i.key_pressed(Key::Enter) {
                match self.op.take() {
                    Some(Op::Pen { anchors }) => self.finish_pen(anchors, false),
                    Some(Op::CropPhoto { start, cur }) => {
                        self.commit_photo_crop(start, cur);
                    }
                    other => self.op = other,
                }
                return;
            }
            let step = if mods.shift { 10.0 } else { 1.0 };
            if i.key_pressed(Key::ArrowLeft) {
                self.nudge(-step, 0.0);
            }
            if i.key_pressed(Key::ArrowRight) {
                self.nudge(step, 0.0);
            }
            if i.key_pressed(Key::ArrowUp) {
                self.nudge(0.0, -step);
            }
            if i.key_pressed(Key::ArrowDown) {
                self.nudge(0.0, step);
            }
            if i.key_pressed(Key::X) {
                if let Fill::Solid(f) = self.style.fill {
                    if let Some(st) = &self.style.stroke {
                        let sc = st.color;
                        self.style.fill = Fill::Solid(sc);
                        if let Some(st) = &mut self.style.stroke {
                            st.color = f;
                        }
                    }
                }
            }
            if i.key_pressed(Key::D) {
                self.style = Style::default();
            }
            if i.key_pressed(Key::OpenBracket) {
                if mods.shift {
                    self.brush.hardness = (self.brush.hardness - 0.08).max(0.0);
                } else {
                    self.brush.size = (self.brush.size - 2.0).max(1.0);
                }
            }
            if i.key_pressed(Key::CloseBracket) {
                if mods.shift {
                    self.brush.hardness = (self.brush.hardness + 0.08).min(1.0);
                } else {
                    self.brush.size = (self.brush.size + 2.0).min(256.0);
                }
            }
            if !ctrl {
                self.tool_from_key(i);
            }
        });
    }

    fn tool_from_key(&mut self, i: &egui::InputState) {
        let set = |s: &mut Studio, t: Tool| {
            if t.in_persona(s.persona) {
                s.last_tool = s.tool;
                s.tool = t;
                s.op = None;
            }
        };
        if i.key_pressed(Key::V) {
            set(self, Tool::Select);
        }
        if i.key_pressed(Key::A) {
            set(self, Tool::Node);
        }
        if i.key_pressed(Key::P) {
            set(self, Tool::Pen);
        }
        if i.key_pressed(Key::N) {
            set(self, Tool::Pencil);
        }
        if i.key_pressed(Key::R) {
            set(self, Tool::Rect);
        }
        if i.key_pressed(Key::O) {
            set(self, Tool::Ellipse);
        }
        if i.key_pressed(Key::Y) {
            set(self, Tool::Polygon);
        }
        if i.key_pressed(Key::S) && self.persona == Persona::Design {
            set(self, Tool::Star);
        }
        if i.key_pressed(Key::L) {
            set(self, Tool::Line);
        }
        if i.key_pressed(Key::T) {
            set(self, Tool::Text);
        }
        if i.key_pressed(Key::G) {
            set(self, Tool::Gradient);
        }
        if i.key_pressed(Key::I) {
            set(self, Tool::Eyedropper);
        }
        if i.key_pressed(Key::B) {
            set(self, Tool::Brush);
        }
        if i.key_pressed(Key::E) {
            set(self, Tool::Eraser);
        }
        if i.key_pressed(Key::K) {
            set(self, Tool::Fill);
        }
        if i.key_pressed(Key::J) {
            set(self, Tool::Clone);
        }
        if i.key_pressed(Key::M) && self.persona == Persona::Pixel {
            set(self, Tool::Smudge);
        }
        if i.key_pressed(Key::C) {
            set(self, Tool::Crop);
        }
        if i.key_pressed(Key::W) {
            set(self, Tool::Wand);
        }
        if i.key_pressed(Key::Q) {
            set(self, Tool::Lasso);
        }
        if i.key_pressed(Key::H) {
            set(self, Tool::Hand);
        }
        if i.key_pressed(Key::Z) {
            set(self, Tool::Zoom);
        }
        if i.key_pressed(Key::Space) {
            // held separately
        }
    }

    pub fn commit_photo_crop(&mut self, start: Pt, cur: Pt) {
        let Some(img) = self.photo.selected_mut() else {
            return;
        };
        let (w, h) = (img.preview.w as f32, img.preview.h as f32);
        let x0 = (start.x.min(cur.x) / w).clamp(0.0, 1.0);
        let y0 = (start.y.min(cur.y) / h).clamp(0.0, 1.0);
        let x1 = (start.x.max(cur.x) / w).clamp(0.0, 1.0);
        let y1 = (start.y.max(cur.y) / h).clamp(0.0, 1.0);
        if x1 - x0 > 0.02 && y1 - y0 > 0.02 {
            img.develop.crop = Some([x0, y0, x1, y1]);
            self.photo.dirty = true;
            self.photo.sel_version += 1;
            self.status = "cropped".into();
        }
    }

    pub fn set_fill(&mut self, fill: Fill) {
        self.style.fill = fill.clone();
        if let Fill::Solid(c) = &fill {
            self.push_recent(*c);
        }
        for (li, id) in self.selection.clone() {
            if let Some(s) = self.doc.find_shape(li, id) {
                let mut after = s.style.clone();
                after.fill = fill.clone();
                self.commit(Cmd::SetStyle {
                    layer: li,
                    id,
                    before: s.style.clone(),
                    after,
                });
            }
        }
    }

    pub fn set_stroke_color(&mut self, c: Rgba) {
        let mut st = self.style.stroke.clone().unwrap_or_default();
        st.color = c;
        self.style.stroke = Some(st.clone());
        for (li, id) in self.selection.clone() {
            if let Some(s) = self.doc.find_shape(li, id) {
                let mut after = s.style.clone();
                after.stroke = Some(st.clone());
                self.commit(Cmd::SetStyle {
                    layer: li,
                    id,
                    before: s.style.clone(),
                    after,
                });
            }
        }
    }
}

impl eframe::App for Studio {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        crate::ui::run(ui, self);
    }
}

pub fn to_egui(p: Pt) -> Pos2 {
    Pos2::new(p.x, p.y)
}

pub fn from_egui(p: Pos2) -> Pt {
    Pt::new(p.x, p.y)
}

pub fn color32(c: Rgba) -> Color32 {
    c.to_egui()
}
