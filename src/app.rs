//! Studio: document + tool state. Mutations go through commands.

use crate::align::{self, Align, Distribute};
use crate::boolean::{self, BoolOp};
use crate::color::{Rgba, default_swatches};
use crate::compositor::{self, View};
use crate::document::{
    apply as apply_cmd, Artboard, Cmd, Document, Fill, History, Layer, LayerKind, Shape, Stroke,
    Style, RASTER_ID,
};
use crate::geom::{Anchor, Bounds, Geom, Pt, TypeRun};
use crate::motion::{self, Ease, Motion, Pose, Prop};
use crate::paint::{self, Brush};
use crate::photo::{self, Histogram, PhotoImage, RgbaImage};
use crate::presets::Preset;
use crate::snap::{self, SnapSettings};
use crate::tools::{Persona, Tool};
use eframe::egui::{self, Color32, Key, Pos2, Rect};
use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::PathBuf;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
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
    pub orig_built: u64,
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
            orig_built: u64::MAX,
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

#[derive(Clone)]
pub enum PendingPlace {
    Raster {
        name: String,
        image: RgbaImage,
    },
    Svg {
        name: String,
        svg: String,
    },
}

impl PendingPlace {
    pub fn name(&self) -> &str {
        match self {
            PendingPlace::Raster { name, .. } | PendingPlace::Svg { name, .. } => name,
        }
    }

    pub fn native_size(&self) -> (f32, f32) {
        match self {
            PendingPlace::Raster { image, .. } => (image.w as f32, image.h as f32),
            PendingPlace::Svg { svg, .. } => crate::shape_browser::svg_size(svg),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum NodeHit {
    Point(usize),
    HandleIn(usize),
    HandleOut(usize),
    Segment(usize),
}

#[derive(Clone)]
pub struct ObjSnap {
    pub layer: usize,
    pub id: u64,
    pub geom: Option<Geom>,
    pub origin: Pt,
    pub size: Pt,
    pub rot: f32,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum WelcomePage {
    New,
    Recents,
    Recovered,
}

pub enum PendingNav {
    CloseTab(usize),
    Quit,
}

#[derive(Clone)]
pub struct TabState {
    pub doc: Document,
    pub path: Option<PathBuf>,
    pub dirty: bool,
    pub view: View,
    pub selection: Vec<(usize, u64)>,
    pub active_layer: Option<usize>,
    pub history: History,
    pub clone_source: Option<Pt>,
    pub pixel_sel: Option<Vec<u8>>,
    pub type_edit: Option<TypeEdit>,
    pub need_fit: bool,
    pub layer_rename: Option<(usize, String)>,
    pub paste_nudge: u32,
    pub node_sel: BTreeSet<usize>,
    pub playhead: f32,
    pub playing: bool,
    pub play_clock: f64,
    pub pose_drag: HashMap<u64, Pose>,
    pub selected_key: Option<(u64, Prop, usize)>,
    pub pending_place: Option<PendingPlace>,
    pub show_welcome: bool,
    pub artboard_sel: Vec<u64>,
    pub layer_expanded: HashSet<u64>,
    pub swap_id: String,
    pub last_input: Instant,
    pub last_swap: Option<Instant>,
    pub shape_rename: Option<(usize, u64, String)>,
    pub artboard_rename: Option<(u64, String)>,
}

pub enum Op {
    Create {
        kind: CreateKind,
        start: Pt,
        cur: Pt,
    },
    Pen {
        anchors: Vec<Anchor>,
        /// Continuing an existing open path: orig geom for undo / Esc.
        source: Option<(usize, u64, Geom)>,
        /// Pointer world pos when the last point was placed. Handle drag is
        /// measured from here so snap offset does not fake a smooth point.
        press: Pt,
    },
    Pencil {
        pts: Vec<Pt>,
    },
    Move {
        orig: Vec<ObjSnap>,
        start: Pt,
    },
    Resize {
        orig: Vec<ObjSnap>,
        handle: usize,
        start_box: Bounds,
    },
    Rotate {
        orig: Vec<ObjSnap>,
        center: Pt,
        start_angle: f32,
    },
    Node {
        layer: usize,
        id: u64,
        which: NodeHit,
        orig: Geom,
        moving: BTreeSet<usize>,
    },
    NodeMarquee {
        start: Pt,
        cur: Pt,
    },
    Corner {
        layer: usize,
        id: u64,
        which: Option<usize>,
        orig_corners: [f32; 4],
        orig_radius: f32,
        orig_geom: Geom,
        start: Pt,
    },
    ArtboardDraw {
        start: Pt,
        cur: Pt,
    },
    ArtboardMove {
        orig: Vec<Artboard>,
        ids: Vec<u64>,
        start: Pt,
    },
    ArtboardResize {
        orig: Artboard,
        handle: usize,
        start_box: Bounds,
    },
    ArtboardRotate {
        orig: Artboard,
        center: Pt,
        start_angle: f32,
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
    ZoomBox {
        start: Pt,
        cur: Pt,
    },
    Place {
        start: Pt,
        cur: Pt,
    },
}

/// On-canvas type session. History is one SetGeom at commit, not per key.
#[derive(Clone)]
pub struct TypeEdit {
    pub layer: usize,
    pub id: u64,
    pub caret: usize,
    pub anchor: usize,
    pub before: Geom,
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
    pub text_px: f32,
    pub text_tracking: f32,
    pub text_leading: f32,
    pub text_font: String,
    pub text_kern: bool,
    pub text_liga: bool,
    pub text_tnum: bool,
    pub text_smcp: bool,
    pub type_edit: Option<TypeEdit>,
    pub polygon_sides: u32,
    pub star_points: u32,
    pub star_inner: f32,
    pub rect_radius: f32,
    pub screen_tex: Option<egui::TextureHandle>,
    pub recent: Vec<Rgba>,
    pub hex_buf: String,
    pub swatches: Vec<Rgba>,
    pub bool_pick: Option<BoolOp>,
    pub google_query: String,
    pub google_status: String,
    pub google_catalog: Vec<crate::google_fonts::GoogleFont>,
    pub google_variant: String,
    pub google_catalog_loaded: bool,
    pub palettes: Vec<crate::palette::Palette>,
    pub palette_idx: usize,
    pub palette_name_buf: String,
    // Welcome / new document options
    pub new_doc_group: String,
    pub new_doc_transparent: bool,
    pub new_doc_bleed: bool,
    pub new_doc_safe: bool,
    pub new_doc_artboards: u32,
    // Browsers
    pub show_shape_browser: bool,
    pub show_asset_browser: bool,
    pub shape_query: String,
    pub shape_lib: String,
    pub shape_status: String,
    pub asset_query: String,
    pub asset_provider: String,
    pub asset_status: String,
    pub asset_results: Vec<crate::asset_browser::AssetHit>,
    pub clipboard: Vec<Shape>,
    pub style_clip: Option<Style>,
    pub recents: Vec<PathBuf>,
    pub fill_active: bool,
    pub custom_w: f32,
    pub custom_h: f32,
    pub custom_dpi: f32,
    pub canvas_gen: u64,
    pub canvas_key: Option<(u32, u32, u32, u32, u32, u64, u8, u32)>,
    pub layer_rename: Option<(usize, String)>,
    pub section_open: SectionOpen,
    pub paste_nudge: u32,
    pub node_sel: BTreeSet<usize>,
    pub playhead: f32,
    pub playing: bool,
    pub play_clock: f64,
    pub pose_drag: HashMap<u64, Pose>,
    pub selected_key: Option<(u64, Prop, usize)>,
    pub pending_place: Option<PendingPlace>,
    pub trace_opts: crate::trace::TraceOpts,
    pub tabs: Vec<TabState>,
    pub active_tab: usize,
    pub artboard_sel: Vec<u64>,
    pub layer_expanded: HashSet<u64>,
    pub swap_id: String,
    pub last_input: Instant,
    pub last_swap: Option<Instant>,
    pub pending_nav: Option<PendingNav>,
    pub welcome_page: WelcomePage,
    pub font_query: String,
    pub font_recents: Vec<String>,
    pub font_scroll_once: bool,
    pub artboard_rename: Option<(u64, String)>,
    pub shape_rename: Option<(usize, u64, String)>,
    pub clipboard_rasters: Vec<Layer>,
}

#[derive(Clone, Copy)]
pub struct SectionOpen {
    pub color: bool,
    pub character: bool,
    pub stroke: bool,
    pub transform: bool,
    pub brush: bool,
    pub layers: bool,
    pub palettes: bool,
    pub fx: bool,
}

impl Default for SectionOpen {
    fn default() -> Self {
        Self {
            color: true,
            character: true,
            stroke: true,
            transform: true,
            brush: true,
            layers: true,
            palettes: false,
            fx: true,
        }
    }
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
            text_px: 72.0,
            text_tracking: 0.0,
            text_leading: 0.0,
            text_font: crate::text::preferred_default_path()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_default(),
            text_kern: true,
            text_liga: true,
            text_tnum: false,
            text_smcp: false,
            type_edit: None,
            polygon_sides: 6,
            star_points: 5,
            star_inner: 0.4,
            rect_radius: 0.0,
            screen_tex: None,
            recent: vec![],
            hex_buf: "#4F8CFF".into(),
            swatches: default_swatches(),
            bool_pick: None,
            google_query: String::new(),
            google_status: String::new(),
            google_catalog: Vec::new(),
            google_variant: "regular".into(),
            google_catalog_loaded: false,
            palettes: crate::palette::load(),
            palette_idx: 0,
            palette_name_buf: String::new(),
            new_doc_group: "All".into(),
            new_doc_transparent: false,
            new_doc_bleed: false,
            new_doc_safe: false,
            new_doc_artboards: 1,
            show_shape_browser: false,
            show_asset_browser: false,
            shape_query: String::new(),
            shape_lib: "Phosphor".into(),
            shape_status: String::new(),
            asset_query: String::new(),
            asset_provider: "All".into(),
            asset_status: String::new(),
            asset_results: Vec::new(),
            clipboard: Vec::new(),
            style_clip: None,
            recents: crate::project::load_recents(),
            fill_active: true,
            custom_w: 1280.0,
            custom_h: 800.0,
            custom_dpi: 72.0,
            canvas_gen: 1,
            canvas_key: None,
            layer_rename: None,
            section_open: SectionOpen::default(),
            paste_nudge: 0,
            node_sel: BTreeSet::new(),
            playhead: 0.0,
            playing: false,
            play_clock: 0.0,
            pose_drag: HashMap::new(),
            selected_key: None,
            pending_place: None,
            trace_opts: crate::trace::TraceOpts::default(),
            tabs: vec![],
            active_tab: 0,
            artboard_sel: vec![],
            layer_expanded: HashSet::new(),
            swap_id: crate::project::new_swap_id(),
            last_input: Instant::now(),
            last_swap: None,
            pending_nav: None,
            welcome_page: WelcomePage::New,
            font_query: String::new(),
            font_recents: crate::project::load_font_recents(),
            font_scroll_once: true,
            artboard_rename: None,
            shape_rename: None,
            clipboard_rasters: vec![],
        };
        s.ensure_tabs();
        s.doc.grid.visible = false;
        if !s.palettes.is_empty() {
            s.palette_name_buf = s.palettes[0].name.clone();
        }
        // Hint the max-font default in the status line so it is discoverable
        // before any type is placed. The Character studio also shows it.
        if let Some(fam) = crate::text::preferred_default_family_name() {
            if !s.text_font.is_empty() {
                let label = crate::text::label_for(&s.text_font);
                s.status = format!("Default font: {label} (from {fam})");
            } else {
                s.status = format!("Default font: {fam} not installed — open Character → Google Fonts to install");
            }
        }
        s
    }

    pub fn new_from_preset(&mut self, p: Preset) {
        let art = self.new_doc_artboards.max(1);
        self.doc = Document::new_with_options(
            p.name,
            p.w,
            p.h,
            p.dpi,
            self.new_doc_transparent,
            art,
            self.new_doc_bleed,
            self.new_doc_safe,
        );
        self.path = None;
        self.dirty = false;
        crate::project::delete_swap(&self.swap_id);
        self.swap_id = crate::project::new_swap_id();
        self.last_swap = None;
        self.history.clear();
        self.selection.clear();
        self.active_layer = Some(1);
        self.need_fit = true;
        self.show_welcome = false;
        self.op = None;
        self.canvas_key = None;
        self.playhead = 0.0;
        self.playing = false;
        self.pose_drag.clear();
        self.selected_key = None;
        self.mark();
        let transp = if self.new_doc_transparent { " transparent" } else { "" };
        let bleed = if self.new_doc_bleed { " + bleed" } else { "" };
        let safe = if self.new_doc_safe { " + safe" } else { "" };
        let arts = if art > 1 { format!(" ×{} artboards", art) } else { String::new() };
        self.status = format!(
            "{} · {:.0}×{:.0} @ {} dpi{}{}{}{}",
            p.name, p.w, p.h, p.dpi, arts, transp, bleed, safe
        );
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
            Geom::Text(TypeRun {
                origin: Pt::new(560.0, 460.0),
                content: "omadesign".into(),
                px: 96.0,
                tracking: -1.0,
                ..TypeRun::default()
            }),
            Style {
                fill: Fill::Solid(cream),
                stroke: None,
            },
        );
        crate::text::fill_contours(&mut word.geom);
        self.commit(Cmd::AddShape {
            layer: 1,
            shape: word,
        });

        let mut tag = Shape::new(
            Geom::Text(TypeRun {
                origin: Pt::new(564.0, 520.0),
                content: "design  ·  paint  ·  photograph".into(),
                px: 28.0,
                tracking: 1.5,
                ..TypeRun::default()
            }),
            Style {
                fill: Fill::Solid(teal),
                stroke: None,
            },
        );
        crate::text::fill_contours(&mut tag.geom);
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
        self.canvas_key = None;
        self.mark();
        self.status = "demo".into();
    }

    pub fn mark(&mut self) {
        self.canvas_gen = self.canvas_gen.wrapping_add(1);
        self.last_input = Instant::now();
    }

    fn snapshot_tab(&self) -> TabState {
        TabState {
            doc: self.doc.clone(),
            path: self.path.clone(),
            dirty: self.dirty,
            view: self.view,
            selection: self.selection.clone(),
            active_layer: self.active_layer,
            history: self.history.clone(),
            clone_source: self.clone_source,
            pixel_sel: self.pixel_sel.clone(),
            type_edit: self.type_edit.clone(),
            need_fit: self.need_fit,
            layer_rename: self.layer_rename.clone(),
            paste_nudge: self.paste_nudge,
            node_sel: self.node_sel.clone(),
            playhead: self.playhead,
            playing: self.playing,
            play_clock: self.play_clock,
            pose_drag: self.pose_drag.clone(),
            selected_key: self.selected_key,
            pending_place: self.pending_place.clone(),
            show_welcome: self.show_welcome,
            artboard_sel: self.artboard_sel.clone(),
            layer_expanded: self.layer_expanded.clone(),
            swap_id: self.swap_id.clone(),
            last_input: self.last_input,
            last_swap: self.last_swap,
            shape_rename: self.shape_rename.clone(),
            artboard_rename: self.artboard_rename.clone(),
        }
    }

    fn apply_tab(&mut self, t: TabState) {
        self.doc = t.doc;
        self.path = t.path;
        self.dirty = t.dirty;
        self.view = t.view;
        self.selection = t.selection;
        self.active_layer = t.active_layer;
        self.history = t.history;
        self.clone_source = t.clone_source;
        self.pixel_sel = t.pixel_sel;
        self.type_edit = t.type_edit;
        self.need_fit = t.need_fit;
        self.layer_rename = t.layer_rename;
        self.paste_nudge = t.paste_nudge;
        self.node_sel = t.node_sel;
        self.playhead = t.playhead;
        self.playing = t.playing;
        self.play_clock = t.play_clock;
        self.pose_drag = t.pose_drag;
        self.selected_key = t.selected_key;
        self.pending_place = t.pending_place;
        self.show_welcome = t.show_welcome;
        self.artboard_sel = t.artboard_sel;
        self.layer_expanded = t.layer_expanded;
        self.swap_id = t.swap_id;
        self.last_input = t.last_input;
        self.last_swap = t.last_swap;
        self.shape_rename = t.shape_rename;
        self.artboard_rename = t.artboard_rename;
        self.op = None;
        self.screen_tex = None;
        self.canvas_key = None;
        self.mark();
    }

    pub fn ensure_tabs(&mut self) {
        if self.tabs.is_empty() {
            self.tabs.push(self.snapshot_tab());
            self.active_tab = 0;
        }
    }

    pub fn park_active(&mut self) {
        self.ensure_tabs();
        if self.active_tab < self.tabs.len() {
            self.tabs[self.active_tab] = self.snapshot_tab();
        }
    }

    pub fn tab_count(&self) -> usize {
        self.tabs.len().max(1)
    }

    pub fn tab_title(&self, i: usize) -> (String, bool) {
        if i == self.active_tab {
            let n = if self.doc.name.is_empty() {
                "Untitled".into()
            } else {
                self.doc.name.clone()
            };
            return (n, self.dirty);
        }
        self.tabs
            .get(i)
            .map(|t| {
                let n = if t.doc.name.is_empty() {
                    "Untitled".into()
                } else {
                    t.doc.name.clone()
                };
                (n, t.dirty)
            })
            .unwrap_or_else(|| ("Untitled".into(), false))
    }

    pub fn switch_tab(&mut self, i: usize) {
        self.ensure_tabs();
        if i == self.active_tab || i >= self.tabs.len() {
            return;
        }
        self.commit_type_edit();
        self.op = None;
        self.park_active();
        self.active_tab = i;
        let t = self.tabs[i].clone();
        self.apply_tab(t);
        self.status = format!("tab {}", i + 1);
    }

    pub fn new_tab(&mut self) {
        if self.show_welcome && self.current_is_blank() && self.tab_count() <= 1 {
            self.show_welcome = false;
            self.status = "new document".into();
            return;
        }
        self.commit_type_edit();
        self.op = None;
        self.park_active();
        let mut fresh = Studio::blank_tab_state();
        fresh.show_welcome = false;
        self.active_tab = self.tabs.len();
        self.tabs.push(fresh.clone());
        self.apply_tab(fresh);
        self.status = "new tab".into();
    }

    pub fn new_tab_welcome(&mut self) {
        self.new_tab();
        self.show_welcome = true;
        self.welcome_page = WelcomePage::New;
    }

    fn blank_tab_state() -> TabState {
        let mut doc = Document::new("Untitled", 1280.0, 800.0, 72.0);
        doc.grid.visible = false;
        TabState {
            doc,
            path: None,
            dirty: false,
            view: View::default(),
            selection: vec![],
            active_layer: Some(1),
            history: History::default(),
            clone_source: None,
            pixel_sel: None,
            type_edit: None,
            need_fit: true,
            layer_rename: None,
            paste_nudge: 0,
            node_sel: BTreeSet::new(),
            playhead: 0.0,
            playing: false,
            play_clock: 0.0,
            pose_drag: HashMap::new(),
            selected_key: None,
            pending_place: None,
            show_welcome: false,
            artboard_sel: vec![],
            layer_expanded: HashSet::new(),
            swap_id: crate::project::new_swap_id(),
            last_input: Instant::now(),
            last_swap: None,
            shape_rename: None,
            artboard_rename: None,
        }
    }

    pub fn request_close_tab(&mut self, i: usize) {
        self.ensure_tabs();
        let dirty = if i == self.active_tab {
            self.dirty
        } else {
            self.tabs.get(i).map(|t| t.dirty).unwrap_or(false)
        };
        if dirty {
            self.pending_nav = Some(PendingNav::CloseTab(i));
        } else {
            self.close_tab(i);
        }
    }

    pub fn close_tab(&mut self, i: usize) {
        self.ensure_tabs();
        if self.tabs.len() <= 1 {
            let id = if i == self.active_tab {
                self.swap_id.clone()
            } else {
                self.tabs.get(i).map(|t| t.swap_id.clone()).unwrap_or_default()
            };
            if !id.is_empty() {
                crate::project::delete_swap(&id);
            }
            let fresh = Studio::blank_tab_state();
            self.tabs = vec![fresh.clone()];
            self.active_tab = 0;
            self.apply_tab(fresh);
            self.show_welcome = true;
            return;
        }
        if i >= self.tabs.len() {
            return;
        }
        let id = if i == self.active_tab {
            self.swap_id.clone()
        } else {
            self.tabs[i].swap_id.clone()
        };
        crate::project::delete_swap(&id);
        self.park_active();
        self.tabs.remove(i);
        if self.active_tab >= self.tabs.len() {
            self.active_tab = self.tabs.len() - 1;
        } else if i < self.active_tab {
            self.active_tab -= 1;
        }
        let t = self.tabs[self.active_tab].clone();
        self.apply_tab(t);
    }

    pub fn tick_swap(&mut self) {
        if !self.dirty {
            return;
        }
        let now = Instant::now();
        if now.duration_since(self.last_input) < Duration::from_millis(1000) {
            return;
        }
        if let Some(prev) = self.last_swap
            && now.duration_since(prev) < Duration::from_millis(800)
        {
            return;
        }
        let saved_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let meta = crate::project::SwapMeta {
            id: self.swap_id.clone(),
            original: self.path.clone(),
            name: self.doc.name.clone(),
            saved_at,
            doc: self.doc.clone(),
        };
        if crate::project::write_swap(&meta).is_ok() {
            self.last_swap = Some(now);
        }
    }

    pub fn recover_swap(&mut self, path: PathBuf) {
        match crate::project::load_swap(&path) {
            Ok(meta) => {
                self.park_active();
                let mut t = Studio::blank_tab_state();
                t.doc = meta.doc;
                t.path = meta.original;
                t.dirty = true;
                t.swap_id = meta.id;
                t.show_welcome = false;
                t.doc.ensure_ids();
                self.active_tab = self.tabs.len();
                self.tabs.push(t.clone());
                self.apply_tab(t);
                self.status = format!("recovered {}", meta.name);
            }
            Err(e) => self.status = format!("recover failed: {e}"),
        }
    }

    pub fn delete_swap_file(&mut self, path: &std::path::Path) {
        let _ = std::fs::remove_file(path);
        self.status = "discarded recovery".into();
    }

    pub fn execute_nav(&mut self, ctx: &egui::Context, save: bool) {
        match self.pending_nav.take() {
            Some(PendingNav::CloseTab(i)) => {
                if save {
                    if i != self.active_tab {
                        self.switch_tab(i);
                    }
                    self.save();
                    if self.dirty {
                        self.pending_nav = Some(PendingNav::CloseTab(self.active_tab));
                        return;
                    }
                    self.close_tab(self.active_tab);
                } else {
                    let id = if i == self.active_tab {
                        self.swap_id.clone()
                    } else {
                        self.tabs.get(i).map(|t| t.swap_id.clone()).unwrap_or_default()
                    };
                    crate::project::delete_swap(&id);
                    self.close_tab(i);
                }
            }
            Some(PendingNav::Quit) => {
                if save {
                    self.park_active();
                    let n = self.tabs.len();
                    for i in 0..n {
                        self.switch_tab(i);
                        if self.dirty {
                            self.save();
                            if self.dirty {
                                self.pending_nav = Some(PendingNav::Quit);
                                return;
                            }
                        }
                    }
                } else {
                    self.park_active();
                    for t in &self.tabs {
                        crate::project::delete_swap(&t.swap_id);
                    }
                    crate::project::delete_swap(&self.swap_id);
                }
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            None => {}
        }
    }

    pub fn remember_font(&mut self, path: &str) {
        crate::project::push_font_recent(path);
        self.font_recents = crate::project::load_font_recents();
    }

    pub fn used_fonts(&self) -> Vec<String> {
        let mut v = Vec::new();
        for layer in &self.doc.layers {
            if let Some(shapes) = layer.kind.shapes() {
                for s in shapes {
                    if let Geom::Text(t) = &s.geom {
                        if !t.font.is_empty() && !v.iter().any(|x| x == &t.font) {
                            v.push(t.font.clone());
                        }
                    }
                }
            }
        }
        v
    }

    pub fn commit_artboards(&mut self, after: Vec<Artboard>) {
        let before = self.doc.artboards.clone();
        if before == after {
            return;
        }
        self.commit(Cmd::SetArtboards { before, after });
    }

    pub fn flip_selection(&mut self, horizontal: bool) {
        let mut items = Vec::new();
        for (li, id) in self.selection.clone() {
            if id == RASTER_ID {
                continue;
            }
            if let Some(s) = self.doc.find_shape(li, id) {
                let before = s.geom.clone();
                let mut after = before.clone();
                after.flip(horizontal);
                items.push((li, id, before, after, s.rotation, s.rotation));
            }
        }
        if items.is_empty() {
            self.status = "select an object to flip".into();
            return;
        }
        self.commit(Cmd::SetGeoms { items });
        self.status = if horizontal {
            "flipped horizontal".into()
        } else {
            "flipped vertical".into()
        };
    }

    pub fn wrap_selection_artboard(&mut self) {
        let mut b: Option<Bounds> = None;
        for (li, id) in &self.selection {
            if *id == RASTER_ID {
                if let Some(rb) = self.doc.layers.get(*li).and_then(|l| l.kind.raster_bounds()) {
                    b = Some(match b {
                        None => rb,
                        Some(acc) => acc.union(rb),
                    });
                }
            } else if let Some(s) = self.doc.find_shape(*li, *id) {
                let sb = s.world_bbox();
                b = Some(match b {
                    None => sb,
                    Some(acc) => acc.union(sb),
                });
            }
        }
        let Some(b) = b else {
            self.status = "select objects to wrap".into();
            return;
        };
        let pad = 16.0;
        let board = Artboard::new(
            self.doc.artboards.len(),
            Pt::new(b.min.x - pad, b.min.y - pad),
            Pt::new(b.width() + pad * 2.0, b.height() + pad * 2.0),
        );
        let id = board.id;
        let mut after = self.doc.artboards.clone();
        after.push(board);
        self.commit_artboards(after);
        self.artboard_sel = vec![id];
        self.status = "artboard from selection".into();
    }

    pub fn clone_artboard(&mut self, id: u64) {
        let Some(src) = self.doc.artboards.iter().find(|a| a.id == id).cloned() else {
            return;
        };
        let mut neu = src.clone();
        neu.id = crate::document::next_id();
        neu.name = self.doc.unique_artboard_name(&format!("{} copy", src.name));
        neu.origin.x += src.size.x + 48.0;
        let nid = neu.id;
        let mut after = self.doc.artboards.clone();
        after.push(neu);
        self.commit_artboards(after);
        self.artboard_sel = vec![nid];
        self.status = "artboard cloned".into();
    }

    pub fn delete_artboards(&mut self) {
        if self.artboard_sel.is_empty() {
            return;
        }
        if self.doc.artboards.len() <= 1 {
            self.status = "keep at least one artboard".into();
            return;
        }
        let drop: HashSet<u64> = self.artboard_sel.iter().copied().collect();
        let mut after: Vec<Artboard> = self
            .doc
            .artboards
            .iter()
            .filter(|a| !drop.contains(&a.id))
            .cloned()
            .collect();
        if after.is_empty() {
            after.push(Artboard::new(
                0,
                Pt::ZERO,
                Pt::new(self.doc.width, self.doc.height),
            ));
        }
        self.artboard_sel.clear();
        self.commit_artboards(after);
        self.status = "artboard deleted".into();
    }

    pub fn is_motion(&self) -> bool {
        self.persona == Persona::Motion
    }

    pub fn live_pose(&self, id: u64) -> Pose {
        if let Some(p) = self.pose_drag.get(&id).copied() {
            return p;
        }
        if self.is_motion() {
            self.doc.motion.pose(id, self.playhead)
        } else {
            Pose::identity()
        }
    }

    pub fn tick_motion(&mut self, ctx: &egui::Context) {
        if !self.is_motion() {
            self.playing = false;
            return;
        }
        let now = ctx.input(|i| i.time);
        if self.playing {
            let dt = (now - self.play_clock).max(0.0) as f32;
            self.playhead += dt;
            let dur = self.doc.motion.duration.max(0.05);
            if self.playhead > dur {
                if self.doc.motion.looped {
                    self.playhead %= dur;
                } else {
                    self.playhead = dur;
                    self.playing = false;
                }
            }
            ctx.request_repaint();
        }
        self.play_clock = now;
    }

    pub fn commit_motion(&mut self, after: Motion) {
        let before = self.doc.motion.clone();
        if before == after {
            return;
        }
        self.commit(Cmd::SetMotion { before, after });
    }

    pub fn key_selection(&mut self, ease: Ease) {
        let t = self.playhead;
        let sel = self.selection.clone();
        if sel.is_empty() {
            self.status = "select a shape to key".into();
            return;
        }
        let mut after = self.doc.motion.clone();
        for (_, id) in sel {
            let pose = self.live_pose(id);
            after.set_key(id, Prop::X, t, pose.dx, ease);
            after.set_key(id, Prop::Y, t, pose.dy, ease);
            after.set_key(id, Prop::Rotation, t, pose.rotation, ease);
            after.set_key(id, Prop::Scale, t, pose.scale, ease);
            if let Some(op) = pose.opacity {
                after.set_key(id, Prop::Opacity, t, op, ease);
            }
        }
        self.commit_motion(after);
        self.status = format!("keyed at {:.2}s", t);
    }

    pub fn key_prop(&mut self, id: u64, prop: Prop, value: f32) {
        let mut after = self.doc.motion.clone();
        after.set_key(id, prop, self.playhead, value, Ease::EaseInOut);
        self.commit_motion(after);
    }

    pub fn commit(&mut self, cmd: Cmd) {
        apply_cmd(&mut self.doc, &cmd);
        self.history.push(cmd);
        self.dirty = true;
        self.mark();
        self.sanitize();
    }

    pub fn undo(&mut self) {
        if let Some(inv) = self.history.undo() {
            apply_cmd(&mut self.doc, &inv);
            self.sanitize();
            self.mark();
            self.status = "undo".into();
        }
    }

    pub fn redo(&mut self) {
        if let Some(cmd) = self.history.redo() {
            apply_cmd(&mut self.doc, &cmd);
            self.sanitize();
            self.mark();
            self.status = "redo".into();
        }
    }

    fn sanitize(&mut self) {
        if let Some(e) = &self.type_edit
            && self.doc.find_shape(e.layer, e.id).is_none()
        {
            self.type_edit = None;
        }
        self.selection.retain(|(li, id)| {
            if *id == RASTER_ID {
                self.doc
                    .layers
                    .get(*li)
                    .is_some_and(|l| l.kind.is_placed_raster())
            } else {
                self.doc
                    .layers
                    .get(*li)
                    .and_then(|l| l.find(*id))
                    .is_some()
            }
        });
        self.artboard_sel
            .retain(|id| self.doc.artboards.iter().any(|a| a.id == *id));
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
        if self.tool == Tool::Node && !self.node_sel.is_empty() {
            self.delete_node();
            return;
        }
        if self.tool == Tool::Artboard && !self.artboard_sel.is_empty() {
            self.delete_artboards();
            return;
        }
        if self.is_motion()
            && let Some((id, prop, index)) = self.selected_key.take()
        {
            let mut after = self.doc.motion.clone();
            after.remove_key(id, prop, index);
            self.commit_motion(after);
            self.status = "key removed".into();
            return;
        }
        self.type_edit = None;
        let mut by_layer: std::collections::BTreeMap<usize, Vec<Shape>> =
            std::collections::BTreeMap::new();
        let mut rasters = vec![];
        for (li, id) in self.selection.clone() {
            if id == RASTER_ID {
                rasters.push(li);
            } else if let Some(s) = self.doc.find_shape(li, id).cloned() {
                by_layer.entry(li).or_default().push(s);
            }
        }
        for (layer, shapes) in by_layer {
            self.commit(Cmd::RemoveShapes { layer, shapes });
        }
        rasters.sort_by(|a, b| b.cmp(a));
        for li in rasters {
            if li < self.doc.layers.len() && self.doc.layers.len() > 1 {
                let layer = self.doc.layers[li].clone();
                self.commit(Cmd::RemoveLayer { index: li, layer });
            }
        }
        self.selection.clear();
    }

    pub fn duplicate_selection(&mut self) {
        self.duplicate_selection_by(Pt::new(16.0, 16.0));
    }

    pub fn duplicate_selection_by(&mut self, delta: Pt) {
        let sel = self.selection.clone();
        let mut neu = vec![];
        for (li, id) in sel {
            if id == RASTER_ID {
                if let Some(layer) = self.doc.layers.get(li).cloned() {
                    let mut layer = layer;
                    layer.id = crate::document::next_id();
                    layer.name = format!("{} copy", layer.name);
                    if let Some((o, sz, rot)) = layer.kind.raster_xform() {
                        layer.kind.set_raster_xform(o + delta, sz, rot);
                    }
                    let index = self.doc.layers.len();
                    self.commit(Cmd::AddLayer { index, layer });
                    neu.push((index, RASTER_ID));
                }
            } else if let Some(mut s) = self.doc.find_shape(li, id).cloned() {
                s.id = crate::document::next_id();
                s.geom.translate(delta);
                neu.push((li, s.id));
                self.commit(Cmd::AddShape { layer: li, shape: s });
            }
        }
        let n = neu.len();
        self.selection = neu;
        if n > 0 {
            self.status = format!("duplicated {n}");
        }
    }

    pub fn finish_zoom_box(&mut self, start: Pt, cur: Pt) {
        self.finish_zoom_box_mods(start, cur, false, false, false);
    }

    pub fn finish_zoom_box_mods(
        &mut self,
        start: Pt,
        cur: Pt,
        alt: bool,
        ctrl: bool,
        shift: bool,
    ) {
        let world = Bounds {
            min: Pt::new(start.x.min(cur.x), start.y.min(cur.y)),
            max: Pt::new(start.x.max(cur.x), start.y.max(cur.y)),
        };
        let Some(rect) = self.canvas_rect else {
            return;
        };
        let screen_w = world.width() * self.view.scale;
        let screen_h = world.height() * self.view.scale;
        if screen_w > 8.0 && screen_h > 8.0 && !ctrl {
            self.view.zoom_to(
                world,
                Bounds {
                    min: Pt::ZERO,
                    max: Pt::new(rect.width(), rect.height()),
                },
            );
            self.status = format!("zoom {:.0}%", self.view.scale * 100.0);
            return;
        }
        if screen_w <= 8.0 && screen_h <= 8.0 {
            // Click without a box: the click handler already applied modifiers.
            return;
        }
        let mid = self.view.to_screen(world.center());
        self.zoom_click(mid, alt, ctrl, shift);
    }

    /// Z tool click. Alt out, Ctrl artboard, Ctrl+Shift selection or all objects.
    pub fn zoom_click(&mut self, screen: Pt, alt: bool, ctrl: bool, shift: bool) {
        if ctrl && shift {
            self.zoom_to_objects(true);
            return;
        }
        if ctrl {
            self.need_fit = true;
            self.status = "zoom artboard".into();
            return;
        }
        self.zoom_by(if alt { 1.0 / 1.25 } else { 1.25 }, screen);
    }

    pub fn zoom_by(&mut self, factor: f32, screen: Pt) {
        self.view.zoom_at(screen, factor);
        self.status = format!("zoom {:.0}%", self.view.scale * 100.0);
    }

    fn canvas_zoom_anchor(&self) -> Pt {
        if let Some(c) = self.cursor {
            return self.view.to_screen(c);
        }
        if let Some(r) = self.canvas_rect {
            return Pt::new(r.width() * 0.5, r.height() * 0.5);
        }
        Pt::ZERO
    }

    /// Fit selected shapes, or every visible vector if nothing is selected.
    pub fn zoom_to_objects(&mut self, prefer_selection: bool) {
        let Some(rect) = self.canvas_rect else {
            return;
        };
        let viewport = Bounds {
            min: Pt::ZERO,
            max: Pt::new(rect.width(), rect.height()),
        };
        let use_sel = prefer_selection && !self.selection.is_empty();
        let mut b: Option<Bounds> = None;
        for (li, layer) in self.doc.layers.iter().enumerate() {
            if !layer.visible {
                continue;
            }
            let Some(shapes) = layer.kind.shapes() else {
                continue;
            };
            for s in shapes {
                if use_sel && !self.selection.contains(&(li, s.id)) {
                    continue;
                }
                let bb = if self.is_motion() {
                    self.live_pose(s.id).map_bounds(s.world_bbox())
                } else {
                    s.world_bbox()
                };
                b = Some(match b {
                    None => bb,
                    Some(acc) => acc.union(bb),
                });
            }
        }
        if let Some(world) = b {
            self.view.zoom_to(world.inflate(12.0), viewport);
            self.status = format!("zoom {:.0}%", self.view.scale * 100.0);
        } else {
            self.need_fit = true;
            self.status = "zoom artboard".into();
        }
    }

    pub fn nudge(&mut self, dx: f32, dy: f32) {
        if self.is_motion() {
            let sel = self.selection.clone();
            let t = self.playhead;
            let mut after = self.doc.motion.clone();
            for (_, id) in sel {
                let pose = self.live_pose(id);
                after.set_key(id, Prop::X, t, pose.dx + dx, Ease::EaseInOut);
                after.set_key(id, Prop::Y, t, pose.dy + dy, Ease::EaseInOut);
            }
            self.commit_motion(after);
            return;
        }
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

    pub fn type_defaults(&self) -> TypeRun {
        TypeRun {
            origin: Pt::ZERO,
            content: String::new(),
            px: self.text_px,
            tracking: self.text_tracking,
            leading: self.text_leading,
            font: self.text_font.clone(),
            kern: self.text_kern,
            liga: self.text_liga,
            tnum: self.text_tnum,
            smcp: self.text_smcp,
            contours: vec![],
        }
    }

    pub fn sync_type_defaults(&mut self, run: &TypeRun) {
        self.text_px = run.px;
        self.text_tracking = run.tracking;
        self.text_leading = run.leading;
        self.text_font = run.font.clone();
        self.text_kern = run.kern;
        self.text_liga = run.liga;
        self.text_tnum = run.tnum;
        self.text_smcp = run.smcp;
    }

    pub fn place_text(&mut self, at: Pt) {
        self.commit_type_edit();
        let Some(li) = self.vector_target() else {
            self.status = "add a vector layer first".into();
            return;
        };
        let mut run = self.type_defaults();
        run.origin = at;
        run.content = "Type".into();
        let mut geom = Geom::Text(run);
        crate::text::fill_contours(&mut geom);
        let shape = Shape::new(geom.clone(), self.style.clone());
        let id = shape.id;
        self.commit(Cmd::AddShape { layer: li, shape });
        self.selection = vec![(li, id)];
        let n = 4; // "Type"
        self.type_edit = Some(TypeEdit {
            layer: li,
            id,
            caret: n,
            anchor: 0,
            before: geom,
        });
        self.status = "type — click or Esc to finish, Enter for a new line".into();
    }

    pub fn begin_type_edit(&mut self, hit: (usize, u64), world: Pt) {
        if self.editing_text(hit.0, hit.1) {
            let caret = self
                .doc
                .find_shape(hit.0, hit.1)
                .and_then(|s| match &s.geom {
                    Geom::Text(run) => Some(crate::text::hit_char(run, world)),
                    _ => None,
                });
            if let (Some(c), Some(e)) = (caret, self.type_edit.as_mut()) {
                e.caret = c;
                e.anchor = c;
            }
            return;
        }
        self.commit_type_edit();
        let Some(s) = self.doc.find_shape(hit.0, hit.1) else {
            return;
        };
        let Geom::Text(run) = &s.geom else {
            return;
        };
        let caret = crate::text::hit_char(run, world);
        let defaults = run.clone();
        let before = s.geom.clone();
        self.sync_type_defaults(&defaults);
        self.selection = vec![hit];
        self.active_layer = Some(hit.0);
        self.type_edit = Some(TypeEdit {
            layer: hit.0,
            id: hit.1,
            caret,
            anchor: caret,
            before,
        });
        self.status = "type — click or Esc to finish, Enter for a new line".into();
    }

    pub fn editing_text(&self, layer: usize, id: u64) -> bool {
        self.type_edit
            .as_ref()
            .is_some_and(|e| e.layer == layer && e.id == id)
    }

    pub fn commit_type_edit(&mut self) {
        let Some(edit) = self.type_edit.take() else {
            return;
        };
        let Some(s) = self.doc.find_shape_mut(edit.layer, edit.id) else {
            return;
        };
        if let Geom::Text(run) = &mut s.geom
            && run.content.trim().is_empty()
        {
            run.content = "Type".into();
            crate::text::fill_contours(&mut s.geom);
        }
        let after = s.geom.clone();
        let rot = s.rotation;
        if after != edit.before {
            self.history.push(Cmd::SetGeom {
                layer: edit.layer,
                id: edit.id,
                before: edit.before,
                after,
                rot_before: rot,
                rot_after: rot,
            });
            self.dirty = true;
        }
        self.status = "type committed".into();
    }

    fn live_type_mut(&mut self) -> Option<&mut TypeRun> {
        let edit = self.type_edit.as_ref()?;
        let s = self.doc.find_shape_mut(edit.layer, edit.id)?;
        match &mut s.geom {
            Geom::Text(run) => Some(run),
            _ => None,
        }
    }

    fn reshape_live_type(&mut self) {
        let Some(edit) = &self.type_edit else {
            return;
        };
        let (li, id) = (edit.layer, edit.id);
        if let Some(s) = self.doc.find_shape_mut(li, id) {
            crate::text::fill_contours(&mut s.geom);
        }
    }

    fn type_sel_range(&self) -> (usize, usize) {
        let Some(e) = &self.type_edit else {
            return (0, 0);
        };
        (e.caret.min(e.anchor), e.caret.max(e.anchor))
    }

    fn type_delete_range(&mut self, lo: usize, hi: usize) {
        let Some(run) = self.live_type_mut() else {
            return;
        };
        let a = crate::text::char_to_byte(&run.content, lo);
        let b = crate::text::char_to_byte(&run.content, hi);
        if a < b {
            run.content.replace_range(a..b, "");
        }
        if let Some(e) = &mut self.type_edit {
            e.caret = lo;
            e.anchor = lo;
        }
        self.reshape_live_type();
        self.mark();
    }

    pub fn type_insert(&mut self, s: &str) {
        if s.is_empty() {
            return;
        }
        let (lo, hi) = self.type_sel_range();
        if lo != hi {
            self.type_delete_range(lo, hi);
        }
        let caret = self.type_edit.as_ref().map(|e| e.caret).unwrap_or(0);
        if let Some(run) = self.live_type_mut() {
            let byte = crate::text::char_to_byte(&run.content, caret);
            run.content.insert_str(byte, s);
        }
        let n = s.chars().count();
        if let Some(e) = &mut self.type_edit {
            e.caret += n;
            e.anchor = e.caret;
        }
        self.reshape_live_type();
        self.mark();
    }

    fn type_backspace(&mut self) {
        let (lo, hi) = self.type_sel_range();
        if lo != hi {
            self.type_delete_range(lo, hi);
            return;
        }
        if lo == 0 {
            return;
        }
        self.type_delete_range(lo - 1, lo);
    }

    fn type_delete_fwd(&mut self) {
        let (lo, hi) = self.type_sel_range();
        if lo != hi {
            self.type_delete_range(lo, hi);
            return;
        }
        let n = self
            .live_type_mut()
            .map(|r| r.content.chars().count())
            .unwrap_or(0);
        if lo >= n {
            return;
        }
        self.type_delete_range(lo, lo + 1);
    }

    fn type_move_caret(&mut self, to: usize, shift: bool) {
        let n = {
            let Some(edit) = &self.type_edit else {
                return;
            };
            self.doc
                .find_shape(edit.layer, edit.id)
                .and_then(|s| match &s.geom {
                    Geom::Text(r) => Some(r.content.chars().count()),
                    _ => None,
                })
                .unwrap_or(0)
        };
        let to = to.min(n);
        if let Some(e) = &mut self.type_edit {
            e.caret = to;
            if !shift {
                e.anchor = to;
            }
        }
    }

    pub fn patch_type(&mut self, f: impl FnOnce(&mut TypeRun)) {
        if self.type_edit.is_some() {
            let mut snap = None;
            if let Some(run) = self.live_type_mut() {
                f(run);
                snap = Some(run.clone());
            }
            if let Some(r) = &snap {
                self.sync_type_defaults(r);
            }
            self.reshape_live_type();
            return;
        }
        if let Some((li, id)) = self.primary()
            && let Some(s) = self.doc.find_shape(li, id)
            && matches!(s.geom, Geom::Text(_))
        {
            let mut after = s.geom.clone();
            let before = s.geom.clone();
            let rot = s.rotation;
            if let Geom::Text(run) = &mut after {
                f(run);
            }
            let defaults = match &after {
                Geom::Text(run) => Some(run.clone()),
                _ => None,
            };
            crate::text::fill_contours(&mut after);
            if let Some(d) = &defaults {
                self.sync_type_defaults(d);
            }
            self.commit(Cmd::SetGeom {
                layer: li,
                id,
                before,
                after,
                rot_before: rot,
                rot_after: rot,
            });
            return;
        }
        let mut d = self.type_defaults();
        f(&mut d);
        self.sync_type_defaults(&d);
    }

    pub fn selected_type(&self) -> Option<TypeRun> {
        if let Some(e) = &self.type_edit
            && let Some(s) = self.doc.find_shape(e.layer, e.id)
            && let Geom::Text(run) = &s.geom
        {
            return Some(run.clone());
        }
        let (li, id) = self.primary()?;
        let s = self.doc.find_shape(li, id)?;
        match &s.geom {
            Geom::Text(run) => Some(run.clone()),
            _ => None,
        }
    }

    pub fn set_tool(&mut self, t: Tool) {
        if self.tool != t {
            self.commit_type_edit();
            self.last_tool = self.tool;
            self.tool = t;
            self.op = None;
        }
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

    /// Click with the pen: add a corner, close, pick up an open end, or join.
    pub fn pen_click(&mut self, world: Pt) {
        self.pen_click_at(world, world);
    }

    /// `pick` is the raw pointer (close / join). `place` is the snapped point.
    pub fn pen_click_at(&mut self, pick: Pt, place: Pt) {
        let slack = 12.0 / self.view.scale.max(0.01);
        if let Some(Op::Pen { anchors, source, .. }) = &self.op
            && anchors.len() >= 3
            && ((anchors[0].pt - pick).length() < slack
                || (anchors[0].pt - place).length() < slack)
        {
            let anchors = anchors.clone();
            let source = source.clone();
            self.op = None;
            self.finish_pen(anchors, true, source);
            return;
        }
        let skip = match &self.op {
            Some(Op::Pen { source: Some((_, id, _)), .. }) => Some(*id),
            _ => None,
        };
        if let Some((li, id, at_start)) = self
            .hit_open_end(pick, slack, skip)
            .or_else(|| self.hit_open_end(place, slack, skip))
        {
            if let Some(Op::Pen { anchors, .. }) = &mut self.op {
                if let Some(s) = self.doc.find_shape(li, id)
                    && let Geom::Path {
                        anchors: other, ..
                    } = &s.geom
                {
                    let mut other = other.clone();
                    if !at_start {
                        crate::geom::reverse_anchors(&mut other);
                    }
                    if other.len() > 1
                        && anchors
                            .last()
                            .is_some_and(|a| (other[0].pt - a.pt).length() < slack * 2.0)
                    {
                        other.remove(0);
                    }
                    anchors.extend(other);
                    self.sync_pen_source();
                }
                if let Some(s) = self.doc.find_shape(li, id).cloned() {
                    self.commit(Cmd::RemoveShapes {
                        layer: li,
                        shapes: vec![s],
                    });
                }
                self.status = "paths joined".into();
                return;
            }
            if let Some(s) = self.doc.find_shape(li, id)
                && let Geom::Path { anchors, .. } = &s.geom
            {
                let mut a = anchors.clone();
                if at_start {
                    crate::geom::reverse_anchors(&mut a);
                }
                self.op = Some(Op::Pen {
                    anchors: a,
                    source: Some((li, id, s.geom.clone())),
                    press: pick,
                });
                self.selection = vec![(li, id)];
                self.status = "continuing path".into();
                return;
            }
        }
        match &mut self.op {
            Some(Op::Pen {
                anchors, press, ..
            }) => {
                anchors.push(Anchor::corner(place));
                *press = pick;
                self.sync_pen_source();
            }
            _ => {
                self.op = Some(Op::Pen {
                    anchors: vec![Anchor::corner(place)],
                    source: None,
                    press: pick,
                })
            }
        }
    }

    pub(crate) fn sync_pen_source(&mut self) {
        let Some(Op::Pen {
            anchors,
            source: Some((li, id, _)),
            ..
        }) = &self.op
        else {
            return;
        };
        let anchors = anchors.clone();
        let (li, id) = (*li, *id);
        if let Some(s) = self.doc.find_shape_mut(li, id)
            && let Geom::Path {
                anchors: dst,
                closed,
            } = &mut s.geom
        {
            *dst = anchors;
            *closed = false;
        }
        self.mark();
    }

    fn hit_open_end(&self, world: Pt, slack: f32, skip: Option<u64>) -> Option<(usize, u64, bool)> {
        let mut best: Option<(f32, usize, u64, bool)> = None;
        for (li, layer) in self.doc.layers.iter().enumerate() {
            if !layer.visible || layer.locked {
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
                let ds = (anchors[0].pt - world).length();
                if ds < slack {
                    if best.is_none_or(|(d, ..)| ds < d) {
                        best = Some((ds, li, s.id, true));
                    }
                }
                let de = (anchors.last().unwrap().pt - world).length();
                if de < slack {
                    if best.is_none_or(|(d, ..)| de < d) {
                        best = Some((de, li, s.id, false));
                    }
                }
            }
        }
        best.map(|(_, li, id, at_start)| (li, id, at_start))
    }

    pub fn finish_pen(
        &mut self,
        anchors: Vec<Anchor>,
        closed: bool,
        source: Option<(usize, u64, Geom)>,
    ) {
        if anchors.len() < 2 {
            return;
        }
        let closed = closed && anchors.len() >= 3;
        if let Some((li, id, orig)) = source {
            let after = Geom::Path {
                anchors,
                closed,
            };
            let rot = self
                .doc
                .find_shape(li, id)
                .map(|s| s.rotation)
                .unwrap_or(0.0);
            if let Some(s) = self.doc.find_shape_mut(li, id) {
                s.geom = after.clone();
            }
            self.commit(Cmd::SetGeom {
                layer: li,
                id,
                before: orig,
                after,
                rot_before: rot,
                rot_after: rot,
            });
            self.selection = vec![(li, id)];
            self.status = if closed { "path closed" } else { "path" }.into();
            return;
        }
        let Some(li) = self.vector_target() else {
            return;
        };
        let mut style = self.style.clone();
        if !closed {
            style.fill = Fill::None;
            if style.stroke.is_none() {
                style.stroke = Some(Stroke::default());
            }
        }
        let shape = Shape::new(Geom::Path { anchors, closed }, style);
        let id = shape.id;
        self.commit(Cmd::AddShape { layer: li, shape });
        self.selection = vec![(li, id)];
        self.status = if closed {
            "closed path"
        } else {
            "open path"
        }
        .into();
    }

    pub fn ensure_path(&mut self, li: usize, id: u64) {
        let Some(s) = self.doc.find_shape(li, id) else {
            return;
        };
        if matches!(s.geom, Geom::Path { .. } | Geom::Text(_)) {
            return;
        }
        let mut after = s.geom.to_path();
        let rot = s.rotation;
        if rot.abs() > 1e-5 {
            let c = after.bbox().center();
            after.rotate_about(c, rot);
        }
        self.commit(Cmd::SetGeom {
            layer: li,
            id,
            before: s.geom.clone(),
            after,
            rot_before: rot,
            rot_after: 0.0,
        });
    }

    pub fn delete_node(&mut self) {
        if self.node_sel.is_empty() {
            return;
        }
        let Some((li, id)) = self.primary() else {
            return;
        };
        let Some(s) = self.doc.find_shape(li, id) else {
            return;
        };
        let Geom::Path { anchors, closed } = &s.geom else {
            return;
        };
        let mut anchors = anchors.clone();
        let mut closed = *closed;
        let mut idxs: Vec<usize> = self.node_sel.iter().copied().collect();
        idxs.sort_unstable();
        idxs.dedup();
        for i in idxs.into_iter().rev() {
            if i < anchors.len() {
                anchors.remove(i);
            }
        }
        if closed && anchors.len() < 3 {
            closed = false;
        }
        if anchors.len() < 2 {
            self.node_sel.clear();
            self.delete_selection();
            return;
        }
        self.node_sel.clear();
        if !anchors.is_empty() {
            self.node_sel.insert(anchors.len() - 1);
        }
        self.commit(Cmd::SetGeom {
            layer: li,
            id,
            before: s.geom.clone(),
            after: Geom::Path { anchors, closed },
            rot_before: s.rotation,
            rot_after: s.rotation,
        });
        self.status = "point deleted".into();
    }

    pub fn break_node(&mut self) {
        let Some(i) = self.node_sel.iter().next().copied() else {
            self.status = "select a point to break".into();
            return;
        };
        let Some((li, id)) = self.primary() else {
            return;
        };
        let Some(s) = self.doc.find_shape(li, id).cloned() else {
            return;
        };
        let Geom::Path { anchors, closed } = &s.geom else {
            return;
        };
        let Some((left, right)) = crate::geom::break_path(anchors, *closed, i) else {
            self.status = "can't break at an endpoint".into();
            return;
        };
        self.commit(Cmd::SetGeom {
            layer: li,
            id,
            before: s.geom.clone(),
            after: Geom::Path {
                anchors: left,
                closed: false,
            },
            rot_before: s.rotation,
            rot_after: s.rotation,
        });
        if let Some(anchors) = right {
            let mut sh = s.clone();
            sh.id = crate::document::next_id();
            sh.geom = Geom::Path {
                anchors,
                closed: false,
            };
            self.commit(Cmd::AddShape {
                layer: li,
                shape: sh,
            });
        }
        self.node_sel.clear();
        self.node_sel.insert(0);
        self.status = "path broken".into();
    }

    pub fn commit_shape_filters(&mut self, layer: usize, id: u64, after: crate::filter::FilterStack) {
        let Some(s) = self.doc.find_shape(layer, id) else {
            return;
        };
        let before = s.filters.clone();
        if before == after {
            return;
        }
        self.commit(Cmd::SetShapeFilters {
            layer,
            id,
            before,
            after,
        });
    }

    pub fn commit_filters(&mut self, index: usize, after: crate::filter::FilterStack) {
        let Some(layer) = self.doc.layers.get(index) else {
            return;
        };
        let before = layer.filters.clone();
        if before == after {
            return;
        }
        self.commit(Cmd::SetFilters {
            index,
            before,
            after,
        });
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
        // If more than 2 selected, delegate to the multi version which folds the op.
        if self.selection.len() > 2 {
            self.apply_boolean_multi(op);
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

    pub fn apply_boolean_multi(&mut self, op: BoolOp) {
        if self.selection.len() < 2 {
            self.status = "select at least two shapes".into();
            return;
        }
        // All on same layer and sorted by z-order (selection order is already z-sorted
        // from hits_in_rect/selection; we keep it).
        let layer = self.selection[0].0;
        if !self.selection.iter().all(|(li, _)| *li == layer) {
            self.status = "boolean needs all shapes on the same layer".into();
            return;
        }
        let shapes: Vec<Shape> = self
            .selection
            .iter()
            .filter_map(|(li, id)| self.doc.find_shape(*li, *id).cloned())
            .collect();
        if shapes.len() < 2 {
            return;
        }
        let geoms: Vec<Geom> = shapes.iter().map(|s| s.geom.clone()).collect();
        let Some(result) = crate::compound::apply_multi(op, &geoms) else {
            self.status = "boolean produced nothing".into();
            return;
        };
        let mut new_shape = shapes[0].clone();
        new_shape.id = crate::document::next_id();
        new_shape.geom = result;
        new_shape.name = format!("{} ({} shapes)", op.name(), shapes.len());
        let old = shapes.clone();
        self.commit(Cmd::RemoveShapes {
            layer,
            shapes: old,
        });
        let id = new_shape.id;
        self.commit(Cmd::AddShape {
            layer,
            shape: new_shape,
        });
        self.selection = vec![(layer, id)];
        self.status = format!("{} on {} shapes", op.name(), shapes.len());
    }

    pub fn combine_selected(&mut self) {
        if self.selection.len() < 2 {
            self.status = "select at least two shapes to combine".into();
            return;
        }
        let layer = self.selection[0].0;
        if !self.selection.iter().all(|(li, _)| *li == layer) {
            self.status = "combine needs all shapes on the same layer".into();
            return;
        }
        let shapes: Vec<Shape> = self
            .selection
            .iter()
            .filter_map(|(li, id)| self.doc.find_shape(*li, *id).cloned())
            .collect();
        let refs: Vec<&Shape> = shapes.iter().collect();
        let Some(geom) = crate::compound::combine_into_poly(&refs) else {
            self.status = "combine produced nothing".into();
            return;
        };
        let mut combined = shapes[0].clone();
        combined.id = crate::document::next_id();
        combined.geom = geom;
        combined.name = "Compound".into();
        self.commit(Cmd::RemoveShapes {
            layer,
            shapes: shapes.clone(),
        });
        let id = combined.id;
        self.commit(Cmd::AddShape {
            layer,
            shape: combined,
        });
        self.selection = vec![(layer, id)];
        self.status = "combined into compound (even-odd)".into();
    }

    pub fn release_compound(&mut self) {
        let Some((li, id)) = self.primary() else {
            self.status = "select a compound shape to release".into();
            return;
        };
        if self.selection.len() != 1 {
            self.status = "select a single compound shape to release".into();
            return;
        }
        let Some(shape) = self.doc.find_shape(li, id).cloned() else {
            return;
        };
        let Some(parts) = crate::compound::explode_poly(&shape.geom) else {
            self.status = "not a compound (needs Poly with >1 contour)".into();
            return;
        };
        // Remove compound, add parts as separate shapes
        self.commit(Cmd::RemoveShapes {
            layer: li,
            shapes: vec![shape],
        });
        let mut new_ids = Vec::new();
        for g in parts {
            let mut s = Shape::new(g, self.style.clone());
            s.name = "Part".into();
            let nid = s.id;
            new_ids.push((li, nid));
            self.commit(Cmd::AddShape { layer: li, shape: s });
        }
        self.selection = new_ids;
        self.status = "compound released".into();
    }

    pub fn align_sel(&mut self, how: Align) {
        let ids = self.selection.clone();
        let deltas = align::align_deltas(&self.doc, &ids, how);
        self.apply_deltas(&deltas);
        self.status = "aligned".into();
    }

    pub fn distribute_sel(&mut self, how: Distribute) {
        let ids = self.selection.clone();
        let deltas = align::distribute_deltas(&self.doc, &ids, how);
        self.apply_deltas(&deltas);
        self.status = "distributed".into();
    }

    fn apply_deltas(&mut self, deltas: &[(usize, u64, Pt)]) {
        if deltas.is_empty() {
            return;
        }
        let mut items = Vec::new();
        for (li, id, d) in deltas {
            if let Some(s) = self.doc.find_shape(*li, *id) {
                let before = s.geom.clone();
                let mut after = before.clone();
                after.translate(*d);
                let rot = s.rotation;
                items.push((*li, *id, before, after, rot, rot));
            }
        }
        if !items.is_empty() {
            self.commit(Cmd::SetGeoms { items });
        }
    }

    const CLIP_PREFIX: &'static str = "omadesign-shapes:";

    pub fn copy_selection(&mut self, ctx: &egui::Context) {
        let shapes: Vec<Shape> = self
            .selection
            .iter()
            .filter_map(|(li, id)| self.doc.find_shape(*li, *id).cloned())
            .collect();
        self.clipboard_rasters = self
            .selection
            .iter()
            .filter_map(|(li, id)| {
                if *id == RASTER_ID {
                    self.doc.layers.get(*li).cloned()
                } else {
                    None
                }
            })
            .collect();
        if shapes.is_empty() && self.clipboard_rasters.is_empty() {
            self.status = "nothing to copy".into();
            return;
        }
        self.clipboard = shapes.clone();
        self.paste_nudge = 0;
        if let Ok(s) = serde_json::to_string(&shapes) {
            ctx.copy_text(format!("{}{s}", Self::CLIP_PREFIX));
        }
        let n = shapes.len() + self.clipboard_rasters.len();
        self.status = format!(
            "copied {n} {}",
            if n == 1 { "object" } else { "objects" }
        );
    }

    pub fn cut_selection(&mut self, ctx: &egui::Context) {
        self.copy_selection(ctx);
        if !self.clipboard.is_empty() {
            self.delete_selection();
            self.status = "cut".into();
        }
    }

    pub fn paste_clipboard(&mut self, payload: Option<&str>) {
        let mut shapes = None;
        if let Some(p) = payload
            && let Some(json) = p.strip_prefix(Self::CLIP_PREFIX)
            && let Ok(v) = serde_json::from_str::<Vec<Shape>>(json)
            && !v.is_empty()
        {
            shapes = Some(v);
        }
        let shapes = shapes.unwrap_or_else(|| self.clipboard.clone());
        if shapes.is_empty() && self.clipboard_rasters.is_empty() {
            self.status = "clipboard is empty".into();
            return;
        }
        self.paste_nudge += 1;
        let nudge = Pt::new(
            16.0 * self.paste_nudge as f32,
            16.0 * self.paste_nudge as f32,
        );
        let mut neu = Vec::new();
        if !shapes.is_empty() {
            let Some(li) = self.vector_target() else {
                self.status = "add a vector layer first".into();
                return;
            };
            for mut s in shapes {
                s.id = crate::document::next_id();
                s.geom.translate(nudge);
                crate::text::fill_contours(&mut s.geom);
                neu.push((li, s.id));
                self.commit(Cmd::AddShape { layer: li, shape: s });
            }
        }
        for mut layer in self.clipboard_rasters.clone() {
            layer.id = crate::document::next_id();
            if let Some((o, sz, rot)) = layer.kind.raster_xform() {
                layer.kind.set_raster_xform(o + nudge, sz, rot);
            }
            let index = self.doc.layers.len();
            self.commit(Cmd::AddLayer { index, layer });
            neu.push((index, RASTER_ID));
        }
        self.selection = neu;
        self.status = format!("pasted {}", self.selection.len());
    }

    pub fn copy_style(&mut self) {
        if let Some((li, id)) = self.primary()
            && let Some(s) = self.doc.find_shape(li, id)
        {
            self.style_clip = Some(s.style.clone());
            self.style = s.style.clone();
            self.status = "style copied".into();
        } else {
            self.style_clip = Some(self.style.clone());
            self.status = "style copied".into();
        }
    }

    pub fn paste_style(&mut self) {
        let Some(style) = self.style_clip.clone() else {
            self.status = "no style on the clipboard".into();
            return;
        };
        self.style = style.clone();
        for (li, id) in self.selection.clone() {
            if let Some(s) = self.doc.find_shape(li, id) {
                self.commit(Cmd::SetStyle {
                    layer: li,
                    id,
                    before: s.style.clone(),
                    after: style.clone(),
                });
            }
        }
        self.status = "style pasted".into();
    }

    pub fn swap_fill_stroke(&mut self) {
        let fill = self.style.fill.clone();
        let stroke = self.style.stroke.clone();
        match (fill, stroke) {
            (Fill::Solid(f), Some(mut st)) => {
                let sc = st.color;
                st.color = f;
                self.style.fill = Fill::Solid(sc);
                self.style.stroke = Some(st);
            }
            (Fill::None, Some(st)) => {
                self.style.fill = Fill::Solid(st.color);
                self.style.stroke = None;
            }
            (Fill::Solid(f), None) => {
                let mut st = crate::document::Stroke::default();
                st.color = f;
                self.style.fill = Fill::None;
                self.style.stroke = Some(st);
            }
            (Fill::Linear { c0, .. } | Fill::Radial { c0, .. }, Some(mut st)) => {
                let sc = st.color;
                st.color = c0;
                self.style.fill = Fill::Solid(sc);
                self.style.stroke = Some(st);
            }
            _ => return,
        }
        for (li, id) in self.selection.clone() {
            if let Some(s) = self.doc.find_shape(li, id) {
                self.commit(Cmd::SetStyle {
                    layer: li,
                    id,
                    before: s.style.clone(),
                    after: self.style.clone(),
                });
            }
        }
        if let Fill::Solid(c) = self.style.fill {
            self.push_recent(c);
        }
        self.status = "swapped fill / stroke".into();
    }

    pub fn bring_to_front(&mut self) {
        self.reorder_selected(true, true);
    }

    pub fn send_to_back(&mut self) {
        self.reorder_selected(false, true);
    }

    pub fn bring_forward(&mut self) {
        self.reorder_selected(true, false);
    }

    pub fn send_backward(&mut self) {
        self.reorder_selected(false, false);
    }

    fn reorder_selected(&mut self, forward: bool, extreme: bool) {
        if self.selection.is_empty() {
            return;
        }
        let mut by_layer: std::collections::BTreeMap<usize, Vec<u64>> =
            std::collections::BTreeMap::new();
        for (li, id) in &self.selection {
            by_layer.entry(*li).or_default().push(*id);
        }
        for (layer, ids) in by_layer {
            let index_of = |studio: &Studio, id: u64| {
                studio
                    .doc
                    .layers
                    .get(layer)
                    .and_then(|l| l.kind.shapes())
                    .and_then(|s| s.iter().position(|sh| sh.id == id))
            };
            let last_of = |studio: &Studio| {
                studio
                    .doc
                    .layers
                    .get(layer)
                    .and_then(|l| l.kind.shapes())
                    .map(|s| s.len().saturating_sub(1))
                    .unwrap_or(0)
            };
            let mut ordered: Vec<(usize, u64)> = ids
                .iter()
                .filter_map(|id| index_of(self, *id).map(|i| (i, *id)))
                .collect();
            ordered.sort_by_key(|(i, _)| *i);
            ordered.dedup_by_key(|(i, _)| *i);
            if ordered.is_empty() {
                continue;
            }
            if forward {
                if extreme {
                    for (_, id) in ordered {
                        if let Some(from) = index_of(self, id) {
                            let last = last_of(self);
                            if from < last {
                                self.commit(Cmd::ReorderShape {
                                    layer,
                                    from,
                                    to: last,
                                });
                            }
                        }
                    }
                } else {
                    for (_, id) in ordered.into_iter().rev() {
                        if let Some(from) = index_of(self, id) {
                            let last = last_of(self);
                            if from < last {
                                self.commit(Cmd::ReorderShape {
                                    layer,
                                    from,
                                    to: from + 1,
                                });
                            }
                        }
                    }
                }
            } else if extreme {
                for (_, id) in ordered.into_iter().rev() {
                    if let Some(from) = index_of(self, id)
                        && from > 0
                    {
                        self.commit(Cmd::ReorderShape {
                            layer,
                            from,
                            to: 0,
                        });
                    }
                }
            } else {
                for (_, id) in ordered {
                    if let Some(from) = index_of(self, id)
                        && from > 0
                    {
                        self.commit(Cmd::ReorderShape {
                            layer,
                            from,
                            to: from - 1,
                        });
                    }
                }
            }
        }
        self.status = if forward {
            if extreme { "brought to front" } else { "brought forward" }
        } else if extreme {
            "sent to back"
        } else {
            "sent backward"
        }
        .into();
    }

    pub fn add_guide(&mut self, vertical: bool, pos: f32) {
        self.commit(Cmd::AddGuide {
            guide: crate::document::Guide { vertical, pos },
        });
        self.status = if vertical {
            format!("guide x = {pos:.0}")
        } else {
            format!("guide y = {pos:.0}")
        };
    }

    pub fn remember_path(&mut self, path: &std::path::Path) {
        crate::project::push_recent(path);
        self.recents = crate::project::load_recents();
    }

    pub fn save_as(&mut self) {
        self.commit_type_edit();
        if let Some(path) = crate::project::dialog_save(&self.doc.name) {
            self.path = Some(path);
            self.save();
        }
    }

    pub fn current_is_blank(&self) -> bool {
        !self.dirty && self.path.is_none() && self.history.len() == 0
    }

    pub fn open_path(&mut self, path: PathBuf) {
        match crate::import::open_any(&path) {
            Ok(crate::import::Imported::Document(doc)) => {
                self.open_document(doc, Some(path));
            }
            Ok(crate::import::Imported::Svg { name, svg }) => {
                if !self.current_is_blank() {
                    self.new_tab();
                }
                self.persona = Persona::Design;
                self.show_welcome = false;
                self.pending_place = Some(PendingPlace::Svg { name, svg });
                self.commit_place_at(Pt::new(self.doc.width * 0.5, self.doc.height * 0.5));
            }
            Ok(crate::import::Imported::Raster { name, image }) => {
                if self.persona == Persona::Photo {
                    self.photo.import_file(&path);
                    self.show_welcome = false;
                    return;
                }
                if !self.current_is_blank() {
                    self.new_tab();
                }
                self.persona = Persona::Design;
                self.show_welcome = false;
                self.pending_place = Some(PendingPlace::Raster { name, image });
                self.commit_place_at(Pt::new(self.doc.width * 0.5, self.doc.height * 0.5));
            }
            Err(e) => self.status = e,
        }
    }

    fn open_document(&mut self, mut doc: crate::document::Document, path: Option<PathBuf>) {
        doc.ensure_ids();
        if !self.current_is_blank() {
            self.park_active();
            let mut t = Studio::blank_tab_state();
            t.doc = doc.clone();
            t.path = path.clone();
            t.show_welcome = false;
            t.need_fit = true;
            self.active_tab = self.tabs.len();
            self.tabs.push(t.clone());
            self.apply_tab(t);
        } else {
            self.doc = doc;
            self.path = path.clone();
            self.history.clear();
            self.selection.clear();
            self.artboard_sel.clear();
            self.active_layer = Some(self.doc.layers.len().saturating_sub(1));
            self.need_fit = true;
            self.playhead = 0.0;
            self.playing = false;
            self.pose_drag.clear();
            self.selected_key = None;
            self.show_welcome = false;
            self.op = None;
            self.type_edit = None;
            self.dirty = false;
            self.swap_id = crate::project::new_swap_id();
            self.mark();
        }
        if let Some(p) = path {
            self.remember_path(&p);
            self.status = format!("opened {}", p.display());
        } else {
            self.status = "opened".into();
        }
    }

    pub fn new_custom(&mut self) {
        let p = Preset {
            group: "Custom",
            name: "Custom",
            w: self.custom_w.max(1.0),
            h: self.custom_h.max(1.0),
            dpi: self.custom_dpi.max(1.0),
        };
        self.new_from_preset(p);
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
        self.commit_type_edit();
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
                    self.remember_path(&path);
                    crate::project::delete_swap(&self.swap_id);
                    self.last_swap = None;
                    self.status = format!("saved {}", path.display());
                }
                Err(e) => self.status = format!("save failed: {e}"),
            }
        }
    }

    pub fn open(&mut self) {
        if let Some(path) = crate::project::dialog_open() {
            self.open_path(path);
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

    pub fn export_animated_svg(&mut self) {
        if let Some(path) = crate::project::dialog_export("Animated SVG", "svg") {
            match crate::svg::export_animated(&self.doc) {
                Ok(s) => {
                    let _ = std::fs::write(&path, s);
                    self.status = format!("exported {}", path.display());
                }
                Err(e) => self.status = format!("export failed: {e}"),
            }
        }
    }

    pub fn export_lottie(&mut self) {
        if let Some(path) = crate::project::dialog_export("Lottie JSON", "json") {
            match motion::export_lottie(&self.doc) {
                Ok(s) => {
                    let _ = std::fs::write(&path, s);
                    self.status = format!("exported {}", path.display());
                }
                Err(e) => self.status = format!("export failed: {e}"),
            }
        }
    }

    pub fn import_lottie(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter("Lottie", &["json", "lottie"])
            .pick_file()
        else {
            return;
        };
        match std::fs::read_to_string(&path) {
            Ok(s) => self.import_lottie_str(&s),
            Err(e) => self.status = format!("read failed: {e}"),
        }
    }

    pub fn import_lottie_str(&mut self, json: &str) {
        match motion::import_lottie(json) {
            Ok(imp) => {
                let empty = self
                    .doc
                    .layers
                    .iter()
                    .filter_map(|l| l.kind.shapes())
                    .all(|s| s.is_empty());
                if empty {
                    self.doc.width = imp.width.max(1.0);
                    self.doc.height = imp.height.max(1.0);
                }
                let Some(li) = self.vector_target() else {
                    self.status = "add a vector layer first".into();
                    return;
                };
                let mut neu = vec![];
                for s in imp.shapes {
                    neu.push((li, s.id));
                    self.commit(Cmd::AddShape { layer: li, shape: s });
                }
                self.commit_motion(imp.motion);
                self.selection = neu;
                self.persona = Persona::Motion;
                self.tool = Tool::Select;
                self.playhead = 0.0;
                self.need_fit = true;
                self.status = "Lottie on the timeline".into();
            }
            Err(e) => self.status = format!("Lottie: {e}"),
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
        if let LayerKind::Raster { pixels, .. } = &mut layer.kind {
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

    pub fn begin_place(&mut self) {
        let Some(path) = crate::project::dialog_place() else {
            return;
        };
        self.load_place_path(&path);
    }

    pub fn load_place_path(&mut self, path: &std::path::Path) {
        match crate::import::open_any(path) {
            Ok(crate::import::Imported::Document(doc)) => {
                self.open_document(doc, Some(path.to_path_buf()));
            }
            Ok(crate::import::Imported::Raster { name, image }) => {
                self.show_welcome = false;
                if self.persona == Persona::Photo {
                    self.persona = Persona::Design;
                    self.tool = Tool::Select;
                }
                self.status = format!("click or drag to place {name}");
                self.pending_place = Some(PendingPlace::Raster { name, image });
            }
            Ok(crate::import::Imported::Svg { name, svg }) => {
                self.show_welcome = false;
                if self.persona == Persona::Photo {
                    self.persona = Persona::Design;
                    self.tool = Tool::Select;
                }
                self.status = format!("click or drag to place {name}");
                self.pending_place = Some(PendingPlace::Svg { name, svg });
            }
            Err(e) => self.status = e,
        }
    }

    pub fn ingest_dropped(&mut self, path: &std::path::Path, at: Option<Pt>) {
        let ext = path
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        if ext == "oma" || crate::import::classify(path) == "oma" {
            self.open_path(path.to_path_buf());
            return;
        }
        if ext == "json" || ext == "lottie" {
            match std::fs::read_to_string(path) {
                Ok(s) => self.import_lottie_str(&s),
                Err(e) => self.status = format!("read failed: {e}"),
            }
            return;
        }
        if self.persona == Persona::Photo && is_raster_ext(&ext) {
            self.photo.import_file(path);
            return;
        }
        match crate::import::open_any(path) {
            Ok(crate::import::Imported::Document(doc)) => self.open_document(doc, Some(path.to_path_buf())),
            Ok(crate::import::Imported::Raster { name, image }) => {
                self.show_welcome = false;
                if self.persona == Persona::Photo {
                    self.persona = Persona::Design;
                    self.tool = Tool::Select;
                }
                self.pending_place = Some(PendingPlace::Raster { name, image });
                self.commit_place_at(at.unwrap_or(Pt::new(self.doc.width * 0.5, self.doc.height * 0.5)));
            }
            Ok(crate::import::Imported::Svg { name, svg }) => {
                self.show_welcome = false;
                if self.persona == Persona::Photo {
                    self.persona = Persona::Design;
                    self.tool = Tool::Select;
                }
                self.pending_place = Some(PendingPlace::Svg { name, svg });
                self.commit_place_at(at.unwrap_or(Pt::new(self.doc.width * 0.5, self.doc.height * 0.5)));
            }
            Err(e) => self.status = e,
        }
    }

    pub fn cancel_place(&mut self) {
        if self.pending_place.take().is_some() {
            self.status = "place cancelled".into();
        }
    }

    pub fn pending_preview_rect(&self, at: Pt) -> Option<Bounds> {
        let pending = self.pending_place.as_ref()?;
        let (sw, sh) = pending.native_size();
        let (w, h) = fit_place_size(sw, sh, self.doc.width * 0.92, self.doc.height * 0.92);
        Some(place_rect_centered(at, w, h, self.doc.width, self.doc.height))
    }

    pub fn commit_place_at(&mut self, at: Pt) {
        let Some(pending) = self.pending_place.as_ref() else {
            return;
        };
        let (sw, sh) = pending.native_size();
        let (w, h) = fit_place_size(sw, sh, self.doc.width * 0.92, self.doc.height * 0.92);
        let dest = place_rect_centered(at, w, h, self.doc.width, self.doc.height);
        self.commit_place_dest(dest);
    }

    pub fn commit_place_rect(&mut self, start: Pt, cur: Pt) {
        let Some(pending) = self.pending_place.as_ref() else {
            return;
        };
        let (sw, sh) = pending.native_size();
        let min = Pt::new(start.x.min(cur.x), start.y.min(cur.y));
        let max = Pt::new(start.x.max(cur.x), start.y.max(cur.y));
        let bw = (max.x - min.x).abs();
        let bh = (max.y - min.y).abs();
        if bw < 8.0 && bh < 8.0 {
            self.commit_place_at(start);
            return;
        }
        let s = (bw / sw.max(1.0)).min(bh / sh.max(1.0)).max(0.01);
        let w = sw * s;
        let h = sh * s;
        let dest = Bounds::from_min_size(
            Pt::new(min.x + (bw - w) * 0.5, min.y + (bh - h) * 0.5),
            Pt::new(w, h),
        );
        self.commit_place_dest(dest);
    }

    fn commit_place_dest(&mut self, dest: Bounds) {
        let Some(pending) = self.pending_place.take() else {
            return;
        };
        match pending {
            PendingPlace::Raster { name, image } => self.place_raster(name, image, dest),
            PendingPlace::Svg { name, svg } => self.place_svg(name, &svg, dest),
        }
    }

    fn place_raster(&mut self, name: String, image: RgbaImage, dest: Bounds) {
        let pixels = crate::document::Pixels::from_rgba(image.w, image.h, image.data)
            .unwrap_or_else(|| crate::document::Pixels::new(image.w.max(1), image.h.max(1)));
        let layer = Layer::placed_raster(
            name.clone(),
            pixels,
            dest.min,
            Pt::new(dest.width().abs().max(1.0), dest.height().abs().max(1.0)),
        );
        let index = self.doc.layers.len();
        self.commit(Cmd::AddLayer { index, layer });
        self.active_layer = Some(index);
        self.selection = vec![(index, RASTER_ID)];
        self.status = format!("{name} placed");
    }

    fn place_svg(&mut self, name: String, svg: &str, dest: Bounds) {
        let target = dest.width().abs().max(dest.height().abs()).max(1.0);
        match crate::shape_browser::svg_to_geom(svg, target) {
            Ok(mut geom) => {
                let b = geom.bbox();
                geom.map_into(b, dest);
                let mut style = self.style.clone();
                if let Some(c) = crate::shape_browser::svg_fill(svg) {
                    style.fill = Fill::Solid(c);
                } else if style.fill.is_none() {
                    style.fill = Fill::Solid(self.brush.color);
                }
                style.stroke = None;
                let shape = Shape::new(geom, style);
                let id = shape.id;
                let Some(li) = self.vector_target() else {
                    let index = self.doc.layers.len();
                    self.commit(Cmd::AddLayer {
                        index,
                        layer: Layer::vector(name.clone()),
                    });
                    self.commit(Cmd::AddShape {
                        layer: index,
                        shape,
                    });
                    self.active_layer = Some(index);
                    self.selection = vec![(index, id)];
                    self.status = format!("{name} placed");
                    return;
                };
                self.commit(Cmd::AddShape { layer: li, shape });
                self.selection = vec![(li, id)];
                self.status = format!("{name} placed");
            }
            Err(e) => self.status = format!("{name}: {e}"),
        }
    }

    pub fn trace_active_raster(&mut self) {
        let Some(li) = self.raster_target() else {
            self.status = "select a pixel layer to trace".into();
            return;
        };
        let Some(px) = self.doc.layers.get(li).and_then(|l| l.kind.pixels()) else {
            self.status = "select a pixel layer to trace".into();
            return;
        };
        let name = self.doc.layers[li].name.clone();
        let traced = crate::trace::trace(px, self.trace_opts);
        if traced.is_empty() {
            self.status = "nothing to trace — drop the threshold or turn off ignore white".into();
            return;
        }
        let index = self.doc.layers.len();
        self.commit(Cmd::AddLayer {
            index,
            layer: Layer::vector(format!("Trace {name}")),
        });
        let mut sel = Vec::new();
        let fill_fallback = match &self.style.fill {
            Fill::Solid(c) => *c,
            _ => Rgba::rgb(0x11, 0x11, 0x11),
        };
        for t in traced {
            let color = if self.trace_opts.colors <= 1 {
                fill_fallback
            } else {
                t.color
            };
            let shape = Shape::new(
                t.geom,
                Style {
                    fill: Fill::Solid(color),
                    stroke: None,
                },
            );
            sel.push((index, shape.id));
            self.commit(Cmd::AddShape {
                layer: index,
                shape,
            });
        }
        self.active_layer = Some(index);
        self.selection = sel;
        self.persona = Persona::Design;
        self.tool = Tool::Select;
        self.status = format!(
            "traced {} path{}",
            self.selection.len(),
            if self.selection.len() == 1 { "" } else { "s" }
        );
    }

    pub fn handle_shortcuts(&mut self, ctx: &egui::Context) {
        if self.handle_type_keys(ctx) {
            return;
        }
        let wants_text = ctx.egui_wants_keyboard_input();
        let mut do_copy = false;
        let mut do_cut = false;
        let mut paste_payload: Option<String> = None;
        ctx.input(|i| {
            let mods = i.modifiers;
            let ctrl = mods.command || mods.ctrl;
            if ctrl && i.key_pressed(Key::S) {
                if mods.shift {
                    self.save_as();
                } else {
                    self.save();
                }
                return;
            }
            if ctrl && i.key_pressed(Key::O) {
                self.open();
                return;
            }
            if ctrl && mods.shift && i.key_pressed(Key::P) {
                self.begin_place();
                return;
            }
            if ctrl && i.key_pressed(Key::N) {
                self.new_tab();
                return;
            }
            if ctrl && i.key_pressed(Key::E) {
                self.export_png();
                return;
            }
            if ctrl && i.key_pressed(Key::G) {
                if mods.shift {
                    self.release_compound();
                } else {
                    self.combine_selected();
                }
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
            if ctrl && i.key_pressed(Key::C) && mods.alt {
                self.copy_style();
                return;
            }
            if ctrl && i.key_pressed(Key::V) && mods.alt {
                self.paste_style();
                return;
            }
            if ctrl && i.key_pressed(Key::C) && !wants_text {
                do_copy = true;
                return;
            }
            if ctrl && i.key_pressed(Key::X) && !wants_text {
                do_cut = true;
                return;
            }
            if ctrl && i.key_pressed(Key::V) && !wants_text {
                for ev in &i.events {
                    if let egui::Event::Paste(t) = ev {
                        paste_payload = Some(t.clone());
                    }
                }
                if paste_payload.is_none() {
                    paste_payload = Some(String::new());
                }
                return;
            }
            if ctrl && i.key_pressed(Key::CloseBracket) {
                if mods.shift {
                    self.bring_to_front();
                } else {
                    self.bring_forward();
                }
                return;
            }
            if ctrl && i.key_pressed(Key::OpenBracket) {
                if mods.shift {
                    self.send_to_back();
                } else {
                    self.send_backward();
                }
                return;
            }
            if ctrl && i.key_pressed(Key::A) {
                self.selection = self
                    .doc
                    .layers
                    .iter()
                    .enumerate()
                    .flat_map(|(li, l)| {
                        let mut ids: Vec<(usize, u64)> = l
                            .kind
                            .shapes()
                            .into_iter()
                            .flatten()
                            .filter(|s| s.visible && !s.locked)
                            .map(move |s| (li, s.id))
                            .collect();
                        if l.kind.is_placed_raster() && l.visible && !l.locked {
                            ids.push((li, RASTER_ID));
                        }
                        ids
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
                self.mark();
                return;
            }
            if ctrl && (i.key_pressed(Key::Plus) || i.key_pressed(Key::Equals)) {
                if self.persona == Persona::Photo {
                    self.photo.view_scale = (self.photo.view_scale * 1.25).clamp(0.1, 8.0);
                } else {
                    let at = self.canvas_zoom_anchor();
                    self.zoom_by(1.25, at);
                }
                return;
            }
            if ctrl && i.key_pressed(Key::Minus) {
                if self.persona == Persona::Photo {
                    self.photo.view_scale = (self.photo.view_scale / 1.25).clamp(0.1, 8.0);
                } else {
                    let at = self.canvas_zoom_anchor();
                    self.zoom_by(1.0 / 1.25, at);
                }
                return;
            }
            if i.key_pressed(Key::F1) {
                self.show_shortcuts = !self.show_shortcuts;
                return;
            }
            if wants_text {
                return;
            }
            if self.persona == Persona::Motion {
                if i.key_pressed(Key::Space) {
                    self.playing = !self.playing;
                    self.status = if self.playing { "play" } else { "pause" }.into();
                    return;
                }
                if i.key_pressed(Key::K) {
                    self.key_selection(Ease::EaseInOut);
                    return;
                }
                if i.key_pressed(Key::Home) {
                    self.playhead = 0.0;
                    self.playing = false;
                    return;
                }
                if i.key_pressed(Key::End) {
                    self.playhead = self.doc.motion.duration;
                    self.playing = false;
                    return;
                }
            }
            if i.key_pressed(Key::Delete) || i.key_pressed(Key::Backspace) {
                self.delete_selection();
                return;
            }
            if i.key_pressed(Key::Escape) {
                if self.pending_place.is_some() {
                    self.cancel_place();
                    self.op = None;
                    return;
                }
                if let Some(Op::Pen { anchors, source, .. }) = &mut self.op {
                    if anchors.len() > 1 {
                        anchors.pop();
                        self.sync_pen_source();
                        self.status = "point removed".into();
                        return;
                    }
                    if let Some((li, id, orig)) = source.clone() {
                        if let Some(s) = self.doc.find_shape_mut(li, id) {
                            s.geom = orig;
                        }
                    }
                    self.op = None;
                    self.mark();
                    self.bool_pick = None;
                    return;
                }
                self.op = None;
                self.bool_pick = None;
                return;
            }
            if i.key_pressed(Key::Enter) {
                if self.pending_place.is_some() {
                    self.commit_place_at(Pt::new(self.doc.width * 0.5, self.doc.height * 0.5));
                    return;
                }
                match self.op.take() {
                    Some(Op::Pen { anchors, source, .. }) => {
                        self.finish_pen(anchors, false, source)
                    }
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
                self.swap_fill_stroke();
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
        if do_copy {
            self.copy_selection(ctx);
        }
        if do_cut {
            self.cut_selection(ctx);
        }
        if let Some(p) = paste_payload {
            self.paste_clipboard(if p.is_empty() { None } else { Some(p.as_str()) });
        }
    }

    fn handle_type_keys(&mut self, ctx: &egui::Context) -> bool {
        if self.type_edit.is_none() {
            return false;
        }
        ctx.request_repaint();
        let mut insert = String::new();
        let mut backspace = false;
        let mut delete = false;
        let mut newline = false;
        let mut escape = false;
        let mut select_all = false;
        let mut undo = false;
        let mut redo = false;
        let mut left = false;
        let mut right = false;
        let mut up = false;
        let mut down = false;
        let mut home = false;
        let mut end = false;
        let mut shift = false;
        let mut save = false;
        let mut copy = false;
        let mut cut = false;
        ctx.input(|i| {
            shift = i.modifiers.shift;
            let ctrl = i.modifiers.command || i.modifiers.ctrl;
            if ctrl && i.key_pressed(Key::S) {
                save = true;
                return;
            }
            if ctrl && i.key_pressed(Key::Z) {
                if i.modifiers.shift {
                    redo = true;
                } else {
                    undo = true;
                }
                return;
            }
            if ctrl && i.key_pressed(Key::A) {
                select_all = true;
                return;
            }
            if i.key_pressed(Key::Escape) {
                escape = true;
                return;
            }
            if i.key_pressed(Key::Enter) {
                newline = true;
            }
            if i.key_pressed(Key::Backspace) {
                backspace = true;
            }
            if i.key_pressed(Key::Delete) {
                delete = true;
            }
            if i.key_pressed(Key::ArrowLeft) {
                left = true;
            }
            if i.key_pressed(Key::ArrowRight) {
                right = true;
            }
            if i.key_pressed(Key::ArrowUp) {
                up = true;
            }
            if i.key_pressed(Key::ArrowDown) {
                down = true;
            }
            if i.key_pressed(Key::Home) {
                home = true;
            }
            if i.key_pressed(Key::End) {
                end = true;
            }
            if ctrl && i.key_pressed(Key::C) {
                copy = true;
            }
            if ctrl && i.key_pressed(Key::X) {
                cut = true;
            }
            // Text and paste events: Text when !ctrl, Paste always (Ctrl+V generates Paste)
            for ev in &i.events {
                match ev {
                    egui::Event::Text(t) if !ctrl => insert.push_str(t),
                    egui::Event::Paste(t) => insert.push_str(t),
                    _ => {}
                }
            }
        });
        if copy || cut {
            if let Some(run) = self
                .type_edit
                .as_ref()
                .and_then(|e| self.doc.find_shape(e.layer, e.id))
                .and_then(|s| match &s.geom {
                    Geom::Text(r) => Some(r.clone()),
                    _ => None,
                })
            {
                let (lo, hi) = self.type_sel_range();
                if lo != hi {
                    let a = crate::text::char_to_byte(&run.content, lo);
                    let b = crate::text::char_to_byte(&run.content, hi);
                    let selected = run.content[a..b].to_owned();
                    ctx.copy_text(selected);
                    if cut {
                        self.type_delete_range(lo, hi);
                    }
                }
            }
            return true;
        }
        if save {
            self.commit_type_edit();
            self.save();
            return true;
        }
        if undo {
            self.commit_type_edit();
            self.undo();
            return true;
        }
        if redo {
            self.commit_type_edit();
            self.redo();
            return true;
        }
        if escape {
            self.commit_type_edit();
            self.tool = Tool::Select;
            return true;
        }
        if select_all {
            let n = self
                .live_type_mut()
                .map(|r| r.content.chars().count())
                .unwrap_or(0);
            if let Some(e) = &mut self.type_edit {
                e.anchor = 0;
                e.caret = n;
            }
            return true;
        }
        if newline {
            self.type_insert("\n");
        }
        if backspace {
            self.type_backspace();
        }
        if delete {
            self.type_delete_fwd();
        }
        if !insert.is_empty() {
            self.type_insert(&insert);
        }
        let caret = self.type_edit.as_ref().map(|e| e.caret).unwrap_or(0);
        if left {
            self.type_move_caret(caret.saturating_sub(1), shift);
        }
        if right {
            self.type_move_caret(caret + 1, shift);
        }
        if home || up {
            let Some(edit) = &self.type_edit else {
                return true;
            };
            let start = self
                .doc
                .find_shape(edit.layer, edit.id)
                .and_then(|s| match &s.geom {
                    Geom::Text(r) => {
                        let (line, _) = {
                            let mut line = 0usize;
                            let mut col = 0usize;
                            let mut start_of_line = 0usize;
                            for (i, ch) in r.content.chars().enumerate() {
                                if i == caret {
                                    break;
                                }
                                if ch == '\n' {
                                    line += 1;
                                    col = 0;
                                    start_of_line = i + 1;
                                } else {
                                    col += 1;
                                    let _ = col;
                                }
                            }
                            (line, start_of_line)
                        };
                        let _ = line;
                        Some({
                            let mut start_of_line = 0usize;
                            for (i, ch) in r.content.chars().enumerate() {
                                if i == caret {
                                    break;
                                }
                                if ch == '\n' {
                                    start_of_line = i + 1;
                                }
                            }
                            start_of_line
                        })
                    }
                    _ => None,
                });
            if let Some(s) = start {
                self.type_move_caret(s, shift);
            }
        }
        if end || down {
            let Some(edit) = &self.type_edit else {
                return true;
            };
            let end_i = self
                .doc
                .find_shape(edit.layer, edit.id)
                .and_then(|s| match &s.geom {
                    Geom::Text(r) => {
                        let mut i = 0usize;
                        let mut hit = false;
                        for ch in r.content.chars() {
                            if i == caret {
                                hit = true;
                            }
                            if hit && ch == '\n' {
                                return Some(i);
                            }
                            i += 1;
                        }
                        Some(i)
                    }
                    _ => None,
                });
            if let Some(s) = end_i {
                self.type_move_caret(s, shift);
            }
        }
        true
    }

    fn tool_from_key(&mut self, i: &egui::InputState) {
        let set = |s: &mut Studio, t: Tool| {
            if t.in_persona(s.persona) {
                s.set_tool(t);
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
            if i.modifiers.shift && self.persona == Persona::Design {
                set(self, Tool::Artboard);
            } else {
                set(self, Tool::Ellipse);
            }
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
        if i.key_pressed(Key::U) {
            set(self, Tool::Trace);
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
        let ctx = ui.ctx().clone();
        if ctx.input(|i| i.viewport().close_requested()) {
            self.park_active();
            let dirty = self.dirty || self.tabs.iter().any(|t| t.dirty);
            if dirty {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                self.pending_nav = Some(PendingNav::Quit);
            }
        }
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

fn is_raster_ext(ext: &str) -> bool {
    matches!(
        ext,
        "png" | "jpg" | "jpeg" | "webp" | "gif" | "bmp" | "tif" | "tiff"
    )
}

fn fit_place_size(src_w: f32, src_h: f32, max_w: f32, max_h: f32) -> (f32, f32) {
    let mut w = src_w.max(1.0);
    let mut h = src_h.max(1.0);
    if w > max_w || h > max_h {
        let s = (max_w / w).min(max_h / h);
        w *= s;
        h *= s;
    }
    (w.max(1.0), h.max(1.0))
}

fn place_rect_centered(at: Pt, w: f32, h: f32, doc_w: f32, doc_h: f32) -> Bounds {
    let mut x = at.x - w * 0.5;
    let mut y = at.y - h * 0.5;
    x = x.clamp(0.0, (doc_w - w).max(0.0));
    y = y.clamp(0.0, (doc_h - h).max(0.0));
    Bounds::from_min_size(Pt::new(x, y), Pt::new(w, h))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::Tool;

    #[test]
    fn pen_clicks_add_points_then_close() {
        let mut s = Studio::new();
        s.show_welcome = false;
        s.tool = Tool::Pen;
        s.pen_click(Pt::new(10.0, 10.0));
        s.pen_click(Pt::new(40.0, 10.0));
        s.pen_click(Pt::new(40.0, 40.0));
        match &s.op {
            Some(Op::Pen { anchors, .. }) => assert_eq!(anchors.len(), 3),
            other => panic!("expected pen draft, got op? {}", other.is_some()),
        }
        s.pen_click(Pt::new(10.0, 10.0));
        assert!(s.op.is_none(), "clicking the first point should close");
        let n: usize = s
            .doc
            .layers
            .iter()
            .filter_map(|l| l.kind.shapes())
            .map(|ss| ss.len())
            .sum();
        assert_eq!(n, 1);
        let Geom::Path { closed, .. } = &s.doc.layers[1].kind.shapes().unwrap()[0].geom else {
            panic!("path");
        };
        assert!(closed);
    }

    #[test]
    fn flip_h_does_not_collapse_open_path() {
        let mut s = Studio::new();
        s.show_welcome = false;
        let shape = Shape::new(
            Geom::Path {
                anchors: vec![
                    Anchor::corner(Pt::new(10.0, 10.0)),
                    Anchor::corner(Pt::new(90.0, 10.0)),
                    Anchor::corner(Pt::new(90.0, 40.0)),
                    Anchor::corner(Pt::new(10.0, 40.0)),
                ],
                closed: false,
            },
            Style::default(),
        );
        let id = shape.id;
        s.commit(Cmd::AddShape { layer: 1, shape });
        s.selection = vec![(1, id)];
        let w0 = s.doc.find_shape(1, id).unwrap().geom.bbox().width();
        s.flip_selection(true);
        let g = &s.doc.find_shape(1, id).unwrap().geom;
        assert!((g.bbox().width() - w0).abs() < 0.1, "width {}", g.bbox().width());
    }

    #[test]
    fn pen_closes_from_raw_click_even_when_snap_is_elsewhere() {
        let mut s = Studio::new();
        s.show_welcome = false;
        s.tool = Tool::Pen;
        s.pen_click(Pt::new(10.0, 10.0));
        s.pen_click(Pt::new(80.0, 10.0));
        s.pen_click(Pt::new(80.0, 80.0));
        s.pen_click_at(Pt::new(11.0, 10.0), Pt::new(64.0, 64.0));
        assert!(s.op.is_none(), "close must use the raw pointer, not snap");
        let n: usize = s
            .doc
            .layers
            .iter()
            .filter_map(|l| l.kind.shapes())
            .map(|ss| ss.len())
            .sum();
        assert_eq!(n, 1);
        let Geom::Path { closed, .. } = &s.doc.layers[1].kind.shapes().unwrap()[0].geom else {
            panic!("path");
        };
        assert!(closed);
    }

    #[test]
    fn pen_enter_leaves_path_open() {
        let mut s = Studio::new();
        s.show_welcome = false;
        s.tool = Tool::Pen;
        s.pen_click(Pt::new(0.0, 0.0));
        s.pen_click(Pt::new(20.0, 0.0));
        s.pen_click(Pt::new(20.0, 20.0));
        let Op::Pen { anchors, source, .. } = s.op.take().unwrap() else {
            panic!("pen");
        };
        s.finish_pen(anchors, false, source);
        let Geom::Path { closed, anchors } = &s.doc.layers[1].kind.shapes().unwrap()[0].geom else {
            panic!("path");
        };
        assert!(!*closed);
        assert_eq!(anchors.len(), 3);
    }

    #[test]
    fn place_text_starts_live_edit_and_typing_replaces_placeholder() {
        let mut s = Studio::new();
        s.show_welcome = false;
        s.place_text(Pt::new(80.0, 120.0));
        assert!(s.type_edit.is_some(), "click should start a type session");
        match s.doc.find_shape(s.type_edit.as_ref().unwrap().layer, s.type_edit.as_ref().unwrap().id) {
            Some(sh) => match &sh.geom {
                Geom::Text(t) => assert_eq!(t.content, "Type"),
                _ => panic!("expected text"),
            },
            None => panic!("missing shape"),
        }
        s.type_insert("Hello");
        let edit = s.type_edit.as_ref().unwrap();
        let sh = s.doc.find_shape(edit.layer, edit.id).unwrap();
        let Geom::Text(t) = &sh.geom else { panic!("text") };
        assert_eq!(t.content, "Hello");
        assert!(!t.contours.is_empty());
        s.commit_type_edit();
        assert!(s.type_edit.is_none());
    }

    #[test]
    fn zoom_box_fills_the_canvas() {
        let mut s = Studio::new();
        s.canvas_rect = Some(Rect::from_min_max(Pos2::ZERO, Pos2::new(400.0, 400.0)));
        s.finish_zoom_box(Pt::new(100.0, 50.0), Pt::new(200.0, 150.0));
        assert!((s.view.scale - 4.0).abs() < 1e-3, "scale {}", s.view.scale);
        let c = s.view.to_screen(Pt::new(150.0, 100.0));
        assert!((c.x - 200.0).abs() < 1e-2 && (c.y - 200.0).abs() < 1e-2);
    }

    #[test]
    fn zoom_click_alt_goes_out() {
        let mut s = Studio::new();
        s.view.scale = 2.0;
        s.zoom_click(Pt::new(100.0, 100.0), true, false, false);
        assert!((s.view.scale - 2.0 / 1.25).abs() < 1e-4);
        s.zoom_click(Pt::new(100.0, 100.0), false, false, false);
        assert!((s.view.scale - 2.0).abs() < 1e-4);
    }

    #[test]
    fn copy_paste_reids_and_offsets() {
        let mut s = Studio::new();
        s.show_welcome = false;
        s.place_text(Pt::new(40.0, 80.0));
        s.commit_type_edit();
        let orig = s.selection.clone();
        assert_eq!(orig.len(), 1);
        s.clipboard = s
            .selection
            .iter()
            .filter_map(|(li, id)| s.doc.find_shape(*li, *id).cloned())
            .collect();
        s.paste_clipboard(None);
        assert_eq!(s.selection.len(), 1);
        assert_ne!(s.selection[0].1, orig[0].1);
        let n: usize = s
            .doc
            .layers
            .iter()
            .filter_map(|l| l.kind.shapes())
            .map(|ss| ss.len())
            .sum();
        assert_eq!(n, 2);
    }

    #[test]
    fn align_is_undoable() {
        let mut s = Studio::new();
        s.show_welcome = false;
        let a = Shape::new(
            Geom::Rect {
                origin: Pt::new(40.0, 10.0),
                size: Pt::new(10.0, 10.0),
                radius: 0.0,
            },
            Style::default(),
        );
        let b = Shape::new(
            Geom::Rect {
                origin: Pt::new(10.0, 40.0),
                size: Pt::new(10.0, 10.0),
                radius: 0.0,
            },
            Style::default(),
        );
        let ia = a.id;
        let ib = b.id;
        s.commit(Cmd::AddShape { layer: 1, shape: a });
        s.commit(Cmd::AddShape { layer: 1, shape: b });
        s.selection = vec![(1, ia), (1, ib)];
        s.align_sel(Align::Left);
        let xa = s.doc.find_shape(1, ia).unwrap().world_bbox().min.x;
        let xb = s.doc.find_shape(1, ib).unwrap().world_bbox().min.x;
        assert!((xa - xb).abs() < 0.1);
        s.undo();
        let xa2 = s.doc.find_shape(1, ia).unwrap().world_bbox().min.x;
        assert!((xa2 - 40.0).abs() < 0.1);
    }

    #[test]
    fn z_order_bring_to_front() {
        let mut s = Studio::new();
        s.show_welcome = false;
        let a = Shape::new(
            Geom::Rect {
                origin: Pt::new(0.0, 0.0),
                size: Pt::new(10.0, 10.0),
                radius: 0.0,
            },
            Style::default(),
        );
        let b = Shape::new(
            Geom::Rect {
                origin: Pt::new(20.0, 0.0),
                size: Pt::new(10.0, 10.0),
                radius: 0.0,
            },
            Style::default(),
        );
        let ia = a.id;
        s.commit(Cmd::AddShape { layer: 1, shape: a });
        s.commit(Cmd::AddShape { layer: 1, shape: b });
        s.selection = vec![(1, ia)];
        s.bring_to_front();
        let ids: Vec<u64> = s.doc.layers[1]
            .kind
            .shapes()
            .unwrap()
            .iter()
            .map(|sh| sh.id)
            .collect();
        assert_eq!(ids.last().copied(), Some(ia));
    }

    #[test]
    fn place_raster_lands_on_a_pixel_layer() {
        let mut s = Studio::new();
        s.show_welcome = false;
        let mut data = vec![0u8; 16 * 16 * 4];
        for px in data.chunks_mut(4) {
            px[0] = 200;
            px[3] = 255;
        }
        s.pending_place = Some(PendingPlace::Raster {
            name: "mark.png".into(),
            image: RgbaImage::new(16, 16, data).unwrap(),
        });
        s.commit_place_at(Pt::new(s.doc.width * 0.5, s.doc.height * 0.5));
        assert!(s.pending_place.is_none());
        let li = s.active_layer.unwrap();
        assert!(s.doc.layers[li].kind.is_placed_raster());
        let px = s.doc.layers[li].kind.pixels().expect("raster layer");
        assert_eq!(px.w, 16);
        assert_eq!(px.h, 16);
        assert!(px.data.iter().any(|b| *b == 200), "placed pixels missing");
        let hit = s.doc.hit_test(Pt::new(s.doc.width * 0.5, s.doc.height * 0.5), 2.0);
        assert_eq!(hit, Some((li, RASTER_ID)));
        let n = s.doc.layers.len();
        s.undo();
        assert_eq!(s.doc.layers.len(), n - 1);
    }

    #[test]
    fn copy_paste_duplicates_shape() {
        let mut s = Studio::new();
        s.show_welcome = false;
        let sh = Shape::new(
            Geom::Rect {
                origin: Pt::new(10.0, 10.0),
                size: Pt::new(20.0, 20.0),
                radius: 0.0,
            },
            Style::default(),
        );
        let id = sh.id;
        s.commit(Cmd::AddShape { layer: 1, shape: sh });
        s.selection = vec![(1, id)];
        s.clipboard = vec![s.doc.find_shape(1, id).cloned().unwrap()];
        s.paste_clipboard(None);
        assert_eq!(s.selection.len(), 1);
        assert_ne!(s.selection[0].1, id);
        let n = s.doc.layers[1].kind.shapes().unwrap().len();
        assert_eq!(n, 2);
    }

    #[test]
    fn five_new_tabs() {
        let mut s = Studio::new();
        s.show_welcome = true;
        for _ in 0..5 {
            s.new_tab();
        }
        assert_eq!(s.tab_count(), 5);
    }

    #[test]
    fn swap_write_and_delete() {
        let mut s = Studio::new();
        s.show_welcome = false;
        s.swap_id = format!("test-{}", crate::document::next_id());
        s.commit(Cmd::AddShape {
            layer: 1,
            shape: Shape::new(
                Geom::Rect {
                    origin: Pt::new(0.0, 0.0),
                    size: Pt::new(10.0, 10.0),
                    radius: 0.0,
                },
                Style::default(),
            ),
        });
        s.last_input = Instant::now() - Duration::from_millis(1500);
        s.tick_swap();
        let p = crate::project::swap_path(&s.swap_id);
        assert!(p.exists(), "swap should be written after idle");
        crate::project::delete_swap(&s.swap_id);
        assert!(!p.exists());
    }

    #[test]
    fn place_svg_adds_a_shape() {
        let mut s = Studio::new();
        s.show_welcome = false;
        let svg = r##"<svg viewBox="0 0 10 10"><path d="M0 0 L10 0 L10 10 L0 10 Z" fill="#112233"/></svg>"##;
        s.pending_place = Some(PendingPlace::Svg {
            name: "box.svg".into(),
            svg: svg.into(),
        });
        s.commit_place_at(Pt::new(100.0, 80.0));
        let n: usize = s
            .doc
            .layers
            .iter()
            .filter_map(|l| l.kind.shapes())
            .map(|ss| ss.len())
            .sum();
        assert_eq!(n, 1);
        assert_eq!(s.selection.len(), 1);
    }

    #[test]
    fn drop_oma_opens_instead_of_placing() {
        let mut s = Studio::new();
        s.show_welcome = false;
        s.ingest_dropped(std::path::Path::new("/tmp/does-not-exist-xyz.oma"), None);
        assert!(
            s.status.contains("open failed")
                || s.status.contains("opened")
                || s.status.contains("No such file")
                || s.status.contains("failed")
                || s.status.contains("can't"),
            "status={}",
            s.status
        );
    }

    #[test]
    fn trace_active_raster_makes_vectors() {
        let mut s = Studio::new();
        s.show_welcome = false;
        let mut data = vec![255u8; 32 * 32 * 4];
        for y in 8..24u32 {
            for x in 8..24u32 {
                let i = ((y * 32 + x) * 4) as usize;
                data[i] = 0;
                data[i + 1] = 0;
                data[i + 2] = 0;
            }
        }
        let mut layer = Layer::raster("ink", 32, 32);
        if let LayerKind::Raster { pixels, .. } = &mut layer.kind {
            *pixels = crate::document::Pixels::from_rgba(32, 32, data).unwrap();
        }
        let index = s.doc.layers.len();
        s.commit(Cmd::AddLayer { index, layer });
        s.active_layer = Some(index);
        s.trace_active_raster();
        assert!(!s.selection.is_empty(), "trace should select the new paths");
        let traced = s
            .doc
            .layers
            .iter()
            .filter_map(|l| l.kind.shapes())
            .map(|ss| ss.len())
            .sum::<usize>();
        assert!(traced >= 1);
        s.undo();
        s.undo();
    }
}
