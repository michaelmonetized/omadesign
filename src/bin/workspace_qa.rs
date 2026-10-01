//! Native WGPU acceptance for the 0.6.3 workspace. Files and profile are disposable.
use eframe::egui::{self, Event, Modifiers, PointerButton, Pos2, Rect};
use omadesign::{
    app::{Studio, libraries::Sidebar},
    document::{Document, Layer, Shape, Style},
    geom::{Geom, Pt},
    tools::Persona,
};
use std::{
    fs,
    path::PathBuf,
    time::{Duration, Instant},
};

struct Qa {
    studio: Studio,
    root: PathBuf,
    output: PathBuf,
    phase: usize,
    warm: usize,
    events: Vec<Event>,
    release: Option<(Pos2, Modifiers, PointerButton)>,
    labels: Vec<(String, Rect)>,
    cards: Vec<Rect>,
    folder: Option<Rect>,
    held: Modifiers,
    started: Instant,
    phase_at: Instant,
    screenshot: Option<String>,
    checks: Vec<String>,
    cursor: Pos2,
}
impl Qa {
    fn new(output: PathBuf) -> Self {
        let root = output.join("fixtures");
        fs::create_dir_all(root.join("Project")).unwrap();
        omadesign::brand::create(&root.join("Project"), "QA Project").unwrap();
        for index in 0..4 {
            let mut doc = Document::new(&format!("Card {index}"), 320., 220., 72.);
            doc.workspace = Some(Persona::Design);
            let mut layer = Layer::vector("Motion artwork");
            let mut shape = Shape::new(
                Geom::Rect {
                    origin: Pt::new(30., 45.),
                    size: Pt::new(70., 70.),
                    radius: 12.,
                },
                Style::default(),
            );
            shape.style.fill = omadesign::document::Fill::Solid(omadesign::color::Rgba::from_hex(
                [0xf38ba8, 0xfab387, 0x89b4fa, 0xa6e3a1][index],
            ));
            let id = shape.id;
            layer.kind.shapes_mut().unwrap().push(shape);
            doc.layers.push(layer);
            doc.motion.set_key(
                id,
                omadesign::motion::Prop::X,
                0.,
                0.,
                omadesign::motion::Ease::Linear,
            );
            doc.motion.set_key(
                id,
                omadesign::motion::Prop::X,
                2.,
                160.,
                omadesign::motion::Ease::Linear,
            );
            omadesign::project::save_to(&doc, &root.join(format!("Card {index}.oma"))).unwrap();
        }
        let mut studio = Studio::new();
        studio.show_welcome = true;
        Self {
            studio,
            root,
            output,
            phase: 0,
            warm: 0,
            events: vec![],
            release: None,
            labels: vec![],
            cards: vec![],
            folder: None,
            held: Modifiers::NONE,
            started: Instant::now(),
            phase_at: Instant::now(),
            screenshot: None,
            checks: vec![],
            cursor: Pos2::ZERO,
        }
    }
    fn click(&mut self, pos: Pos2, modifiers: Modifiers, button: PointerButton) {
        self.cursor = pos;
        self.held = modifiers;
        self.events.extend([
            Event::ModifiersChanged(modifiers),
            Event::PointerMoved(pos),
            Event::PointerButton {
                pos,
                button,
                pressed: true,
                modifiers,
            },
        ]);
        self.release = Some((pos, modifiers, button));
    }
    fn label(&self, label: &str) -> Pos2 {
        self.labels
            .iter()
            .rev()
            .find(|(t, _)| t == label)
            .unwrap_or_else(|| panic!("missing {label}: {:?}", self.labels))
            .1
            .center()
    }
    fn press(&mut self, key: egui::Key) {
        self.held = Modifiers::NONE;
        for pressed in [true, false] {
            self.events.push(Event::Key {
                key,
                physical_key: None,
                pressed,
                repeat: false,
                modifiers: Modifiers::NONE,
            });
        }
    }
    fn next(&mut self) {
        self.phase += 1;
        self.phase_at = Instant::now();
    }
    fn pass(&mut self, name: &str, ctx: &egui::Context) {
        self.checks.push(name.into());
        self.screenshot = Some(format!("{:02}-{}.png", self.phase, name.replace(' ', "-")));
        ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(Default::default()));
        self.next();
    }
    fn files(&self, folder: &str) -> Vec<PathBuf> {
        let mut files: Vec<_> = fs::read_dir(self.root.join(folder))
            .unwrap()
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|x| x == "oma"))
            .collect();
        files.sort();
        files
    }
    fn document_card(&self, path: &std::path::Path) -> Rect {
        let mut files = self.files("");
        files.extend(self.files("Project"));
        files.sort_by(|a, b| {
            fs::metadata(b)
                .unwrap()
                .modified()
                .unwrap()
                .cmp(&fs::metadata(a).unwrap().modified().unwrap())
                .then(a.cmp(b))
        });
        self.cards[files
            .iter()
            .position(|candidate| candidate == path)
            .unwrap()]
    }
    fn inspect(&mut self, ctx: &egui::Context) {
        fn visit(
            shape: &egui::Shape,
            clip: Rect,
            labels: &mut Vec<(String, Rect)>,
            images: &mut Vec<Rect>,
        ) {
            match shape {
                egui::Shape::Text(t) => {
                    let r = t.galley.rect.translate(t.pos.to_vec2());
                    if clip.contains(r.center()) {
                        labels.push((t.galley.job.text.clone(), r));
                    }
                }
                egui::Shape::Mesh(m) => {
                    let r = m.calc_bounds();
                    if clip.contains(r.center())
                        && r.width() > 90.
                        && r.width() < 230.
                        && r.height() > 65.
                        && r.top() > 90.
                    {
                        images.push(r);
                    }
                }
                egui::Shape::Vec(v) => {
                    for s in v {
                        visit(s, clip, labels, images)
                    }
                }
                _ => {}
            }
        }
        self.labels.clear();
        let mut images = vec![];
        let layers = ctx.memory(|m| m.layer_ids().collect::<Vec<_>>());
        ctx.graphics(|g| {
            for layer in layers {
                if let Some(list) = g.get(layer) {
                    for s in list.all_entries() {
                        visit(&s.shape, s.clip_rect, &mut self.labels, &mut images);
                    }
                }
            }
        });
        self.cards = images
            .iter()
            .copied()
            .filter(|r| r.center().x < 600.)
            .collect();
        self.cards.sort_by(|a, b| {
            a.top()
                .total_cmp(&b.top())
                .then(a.left().total_cmp(&b.left()))
        });
        self.cards.dedup();
        self.folder = images.into_iter().find(|r| r.center().x > 1000.);
    }
    fn step(&mut self, ctx: &egui::Context) {
        use egui::Key;
        match self.phase {
            0 => {
                if self.cards.len() < 4 || self.folder.is_none() {
                    return;
                }
                self.pass("welcome browser", ctx);
            }
            1 => {
                self.click(
                    self.cards[0].center(),
                    Modifiers::CTRL,
                    PointerButton::Primary,
                );
                self.next();
            }
            2 => {
                self.click(
                    self.cards[2].center(),
                    Modifiers::SHIFT,
                    PointerButton::Primary,
                );
                self.next();
            }
            3 => {
                assert!(self.labels.iter().any(|(s, _)| s == "3 selected"));
                self.press(Key::Enter);
                self.next();
            }
            4 => {
                if self.studio.tab_count() < 4 {
                    return;
                }
                assert_eq!(self.studio.tab_count(), 4);
                assert!(self.studio.show_welcome);
                self.pass("range and return retain welcome", ctx);
            }
            5 => {
                self.click(
                    self.cards[3].center(),
                    Modifiers::ALT,
                    PointerButton::Primary,
                );
                self.next();
            }
            6 => {
                if self.studio.tab_count() < 5 {
                    return;
                }
                assert!(self.studio.show_welcome);
                self.pass("alt click opens background tab", ctx);
            }
            7 => {
                self.press(Key::Escape);
                self.events.push(Event::PointerMoved(
                    self.cards[0].left_center() + egui::vec2(4., 0.),
                ));
                self.next();
            }
            8 => {
                self.pass("motion scrub start", ctx);
            }
            9 => {
                self.events.push(Event::PointerMoved(
                    self.cards[0].right_center() - egui::vec2(4., 0.),
                ));
                self.next();
            }
            10 => {
                self.pass("motion scrub end", ctx);
            }
            11 => {
                let pos = self.cards[0].center();
                self.cursor = pos;
                self.held = Modifiers::ALT;
                self.events.extend([
                    Event::ModifiersChanged(self.held),
                    Event::PointerMoved(pos),
                    Event::PointerButton {
                        pos,
                        button: PointerButton::Primary,
                        pressed: true,
                        modifiers: self.held,
                    },
                ]);
                self.next();
            }
            12 => {
                let pos = self.cards[0].center() + egui::vec2(20., 10.);
                self.events.push(Event::PointerMoved(pos));
                self.next();
            }
            13 => {
                let pos = self.folder.unwrap().center();
                self.cursor = pos;
                self.events.push(Event::PointerMoved(pos));
                self.next();
            }
            14 => {
                self.events.push(Event::PointerButton {
                    pos: self.cursor,
                    button: PointerButton::Primary,
                    pressed: false,
                    modifiers: Modifiers::ALT,
                });
                self.next();
            }
            15 => {
                if self.files("Project").len() != 1 {
                    return;
                }
                assert_eq!(self.files("").len(), 4);
                self.held = Modifiers::NONE;
                self.pass("alt drag copies to project", ctx);
            }
            16 => {
                self.press(Key::Escape);
                self.click(
                    self.cards[1].center(),
                    Modifiers::NONE,
                    PointerButton::Secondary,
                );
                self.next();
            }
            17 => {
                self.click(self.label("Clone"), Modifiers::NONE, PointerButton::Primary);
                self.next();
            }
            18 => {
                if self.files("").len() != 5 {
                    return;
                }
                self.pass("context clone", ctx);
            }
            19 => {
                self.click(
                    self.cards[0].center(),
                    Modifiers::NONE,
                    PointerButton::Secondary,
                );
                self.next();
            }
            20 => {
                self.click(
                    self.label("Make template"),
                    Modifiers::NONE,
                    PointerButton::Primary,
                );
                self.next();
            }
            21 => {
                let root = PathBuf::from(std::env::var_os("XDG_DATA_HOME").unwrap())
                    .join("omadesign/templates");
                if !root.exists()
                    || fs::read_dir(root)
                        .unwrap()
                        .flatten()
                        .filter(|e| e.path().extension().is_some_and(|e| e == "oma"))
                        .count()
                        == 0
                {
                    return;
                }
                self.pass("context makes reusable template", ctx);
            }
            22 => {
                self.click(
                    self.folder.unwrap().center(),
                    Modifiers::NONE,
                    PointerButton::Secondary,
                );
                self.next();
            }
            23 => {
                self.events.push(Event::PointerMoved(self.label("New")));
                self.next();
            }
            24 => {
                self.click(
                    self.label("Raster"),
                    Modifiers::NONE,
                    PointerButton::Primary,
                );
                self.next();
            }
            25 => {
                if self.studio.show_welcome {
                    return;
                }
                assert_eq!(self.studio.persona, Persona::Pixel);
                assert!(
                    self.studio
                        .path
                        .as_ref()
                        .unwrap()
                        .starts_with(self.root.join("Project"))
                );
                self.pass("new raster in project", ctx);
            }
            26 => {
                self.click(
                    self.label("\u{E6EE}"),
                    Modifiers::NONE,
                    PointerButton::Primary,
                );
                self.next();
            }
            27 => {
                assert_eq!(self.studio.libraries.sidebar, Sidebar::Typography);
                assert!(self.labels.iter().any(|(s, _)| s == "Typography"));
                self.pass("dedicated typography tab", ctx);
            }
            28 => {
                self.click(
                    self.label("\u{E6C8}"),
                    Modifiers::NONE,
                    PointerButton::Primary,
                );
                self.next();
            }
            29 => {
                assert_eq!(self.studio.libraries.sidebar, Sidebar::Palettes);
                self.click(
                    self.label("\u{ECA0}"),
                    Modifiers::NONE,
                    PointerButton::Primary,
                );
                self.next();
            }
            30 => {
                assert_eq!(self.studio.libraries.sidebar, Sidebar::Brand);
                self.click(
                    self.label("\u{E2CE}"),
                    Modifiers::NONE,
                    PointerButton::Primary,
                );
                self.next();
            }
            31 => {
                assert_eq!(self.studio.libraries.sidebar, Sidebar::Inspector);
                self.pass("icon sidebar navigation", ctx);
            }
            32 => {
                let r = ctx
                    .read_response(egui::Id::new("studio-welcome-toggle"))
                    .unwrap()
                    .rect;
                self.click(r.center(), Modifiers::NONE, PointerButton::Primary);
                self.next();
            }
            33 => {
                assert!(self.studio.show_welcome);
                let r = ctx
                    .read_response(egui::Id::new("studio-agent-toggle"))
                    .unwrap()
                    .rect;
                self.click(r.center(), Modifiers::NONE, PointerButton::Primary);
                self.next();
            }
            34 => {
                assert!(self.studio.agent.visible);
                self.pass("welcome and centered agent", ctx);
            }
            35 => {
                let r = ctx
                    .read_response(egui::Id::new("studio-agent-toggle"))
                    .unwrap()
                    .rect;
                self.click(r.center(), Modifiers::NONE, PointerButton::Primary);
                self.next();
            }
            36 => {
                assert!(!self.studio.agent.visible);
                self.press(Key::Escape);
                let original = self
                    .root
                    .join(format!("{}.oma", self.studio.tab_title(1).0));
                let pos = self.document_card(&original).center();
                self.cursor = pos;
                self.held = Modifiers::NONE;
                self.events.extend([
                    Event::PointerMoved(pos),
                    Event::PointerButton {
                        pos,
                        button: PointerButton::Primary,
                        pressed: true,
                        modifiers: Modifiers::NONE,
                    },
                ]);
                self.next();
            }
            37 => {
                self.events
                    .push(Event::PointerMoved(self.cursor + egui::vec2(20., 10.)));
                self.next();
            }
            38 => {
                self.cursor = self.folder.unwrap().center();
                self.events.push(Event::PointerMoved(self.cursor));
                self.next();
            }
            39 => {
                self.events.push(Event::PointerButton {
                    pos: self.cursor,
                    button: PointerButton::Primary,
                    pressed: false,
                    modifiers: Modifiers::NONE,
                });
                self.next();
            }
            40 => {
                if self.files("").len() != 4 {
                    return;
                }
                assert_eq!(self.files("Project").len(), 3);
                let active = self.studio.active_tab;
                self.studio.switch_tab(1);
                assert!(
                    self.studio
                        .path
                        .as_ref()
                        .unwrap()
                        .starts_with(self.root.join("Project"))
                );
                self.studio.switch_tab(active);
                self.studio.show_welcome_screen();
                self.pass("drag moves and rebases open tab", ctx);
            }
            41 => {
                self.click(
                    self.document_card(&self.files("")[0]).center(),
                    Modifiers::NONE,
                    PointerButton::Secondary,
                );
                self.next();
            }
            42 => {
                self.events
                    .push(Event::PointerMoved(self.label("Move to project")));
                self.next();
            }
            43 => {
                self.click(
                    self.label("Project"),
                    Modifiers::NONE,
                    PointerButton::Primary,
                );
                self.next();
            }
            44 => {
                if self.files("").len() != 3 {
                    return;
                }
                assert_eq!(self.files("Project").len(), 4);
                self.pass("context moves to project", ctx);
            }
            45 => {
                self.click(
                    self.document_card(&self.files("")[0]).center(),
                    Modifiers::NONE,
                    PointerButton::Secondary,
                );
                self.next();
            }
            46 => {
                self.click(
                    self.label("Delete · Move to Trash"),
                    Modifiers::NONE,
                    PointerButton::Primary,
                );
                self.next();
            }
            47 => {
                if self.files("").len() != 2 {
                    return;
                }
                self.pass("context moves file to trash", ctx);
            }
            48 => {
                self.studio.show_templates = true;
                self.next();
            }
            49 => {
                if !self.labels.iter().any(|(s, _)| s == "Your templates") {
                    return;
                }
                self.click(
                    self.label(
                        &fs::read_dir(
                            PathBuf::from(std::env::var_os("XDG_DATA_HOME").unwrap())
                                .join("omadesign/templates"),
                        )
                        .unwrap()
                        .flatten()
                        .find(|e| e.path().extension().is_some_and(|x| x == "oma"))
                        .unwrap()
                        .path()
                        .file_stem()
                        .unwrap()
                        .to_string_lossy(),
                    ),
                    Modifiers::NONE,
                    PointerButton::Primary,
                );
                self.next();
            }
            50 => {
                if self.studio.show_templates || self.studio.show_welcome {
                    return;
                }
                assert!(self.studio.path.is_none() && self.studio.dirty);
                assert_eq!(self.studio.doc.width, 320.);
                self.pass("personal template opens editable unsaved copy", ctx);
            }
            _ => {
                fs::write(self.output.join("result.json"),serde_json::to_vec_pretty(&serde_json::json!({"version":env!("CARGO_PKG_VERSION"),"checks":self.checks,"renderer":"native WGPU","input":"egui pointer and keyboard events","elapsed_seconds":self.started.elapsed().as_secs_f64()})).unwrap()).unwrap();
                self.studio.allow_close = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
    }
}
impl eframe::App for Qa {
    fn raw_input_hook(&mut self, _: &egui::Context, input: &mut egui::RawInput) {
        input
            .events
            .retain(|e| matches!(e, Event::Screenshot { .. }));
        input.focused = true;
        input.events.push(Event::ModifiersChanged(self.held));
        if self.events.is_empty()
            && let Some((pos, modifiers, button)) = self.release.take()
        {
            input.events.push(Event::PointerButton {
                pos,
                modifiers,
                button,
                pressed: false,
            });
        }
        input.events.append(&mut self.events);
    }
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        omadesign::ui::set_capture_catalog_root(&ctx, &self.root);
        eframe::App::ui(&mut self.studio, ui, frame);
        self.inspect(&ctx);
        self.warm += 1;
        if let Some(pixels) = ctx.input(|i| {
            i.events.iter().find_map(|e| {
                if let Event::Screenshot { image, .. } = e {
                    Some(image.clone())
                } else {
                    None
                }
            })
        }) {
            if let Some(name) = self.screenshot.take() {
                let bytes: Vec<_> = pixels.pixels.iter().flat_map(|p| p.to_array()).collect();
                image::save_buffer(
                    self.output.join(name),
                    &bytes,
                    pixels.width() as u32,
                    pixels.height() as u32,
                    image::ColorType::Rgba8,
                )
                .unwrap();
            }
        }
        if self.warm > 25
            && self.release.is_none()
            && self.screenshot.is_none()
            && self.phase_at.elapsed() > Duration::from_millis(350)
            && omadesign::ui::scene_ready(&ctx, &self.studio)
        {
            self.step(&ctx);
        }
        assert!(
            self.phase_at.elapsed() < Duration::from_secs(40),
            "phase {} timed out, status {}, cards {}, labels {:?}",
            self.phase,
            self.studio.status,
            self.cards.len(),
            self.labels
        );
        ctx.request_repaint_after(Duration::from_millis(16));
    }
}
fn main() -> eframe::Result {
    let output = PathBuf::from(std::env::args().nth(1).expect("workspace_qa OUTPUT"));
    fs::create_dir_all(&output).unwrap();
    for (key, dir) in [
        ("XDG_CONFIG_HOME", "config"),
        ("XDG_DATA_HOME", "data"),
        ("XDG_CACHE_HOME", "cache"),
        ("XDG_STATE_HOME", "state"),
    ] {
        let p = output.join("profile").join(dir);
        fs::create_dir_all(&p).unwrap();
        unsafe {
            std::env::set_var(key, p);
        }
    }
    eframe::run_native(
        "Omadesign 0.6.3 acceptance",
        eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default()
                .with_inner_size([1600., 1000.])
                .with_resizable(false),
            renderer: eframe::Renderer::Wgpu,
            ..Default::default()
        },
        Box::new(move |cc| {
            omadesign::ui::theme::apply(&cc.egui_ctx);
            cc.egui_ctx.set_pixels_per_point(1.);
            Ok(Box::new(Qa::new(output)))
        }),
    )
}
