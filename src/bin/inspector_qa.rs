//! Recorded native UI acceptance tests. Only fixture setup bypasses user input.
//! inspector_qa ISSUE OUTPUT_DIRECTORY
use eframe::egui::{self, Event, Key, Modifiers, PointerButton, Pos2, Rect};
use omadesign::{
    app::Studio,
    document::{Document, Fill, Layer, Shape, Style},
    geom::{Anchor, Geom, Pt},
    tools::Persona,
};
use std::{
    collections::VecDeque,
    fs,
    io::Write,
    path::PathBuf,
    process::{Child, ChildStdin, Command, Stdio},
    time::{Duration, Instant},
};

const FPS: u32 = 10;
const SIZE: [usize; 2] = [1600, 1000];
enum Action {
    Click(&'static str),
    Right(&'static str),
    ScrollLayer,
    World(Pt, PointerButton),
    Press(Key, Modifiers),
    Type(&'static str),
    Check(&'static str),
}
struct Qa {
    studio: Studio,
    issue: String,
    output: PathBuf,
    actions: VecDeque<Action>,
    labels: Vec<(String, Rect)>,
    events: Vec<Event>,
    cursor: Pos2,
    frame: u32,
    warm: u32,
    pending: bool,
    step_ready: bool,
    encoder: Option<(Child, ChildStdin)>,
    original: String,
    changed: String,
    history: usize,
    checks: Vec<String>,
    started: Instant,
}
fn ctrl() -> Modifiers {
    Modifiers::CTRL | Modifiers::COMMAND
}
fn fixture() -> Studio {
    let mut s = Studio::new();
    s.show_welcome = false;
    s.show_key_hud = true;
    s.show_rulers = false;
    s.doc = Document::new("Inspector acceptance", 800., 520., 72.);
    s.doc.layers = vec![Layer::vector("Artwork")];
    s.libraries.sidebar = omadesign::app::libraries::Sidebar::Inspector;
    for (name, x, y, color) in [
        ("Amber triangle", 80., 130., 0xFAB387),
        ("Purple triangle", 410., 230., 0xCBA6F7),
    ] {
        let mut sh = Shape::new(
            Geom::Path {
                anchors: vec![
                    Anchor::corner(Pt::new(x, y)),
                    Anchor::corner(Pt::new(x + 170., y + 30.)),
                    Anchor::corner(Pt::new(x + 50., y + 130.)),
                ],
                closed: true,
            },
            Style {
                fill: Fill::Solid(omadesign::color::Rgba::from_hex(color)),
                ..Style::default()
            },
        );
        sh.name = name.into();
        s.selection.push((0, sh.id));
        s.doc.layers[0].kind.shapes_mut().unwrap().push(sh);
    }
    s.active_layer = Some(0);
    s.need_fit = true;
    s.snap.enabled = false;
    s.history.clear();
    s.dirty = false;
    s
}
fn actions(issue: &str) -> VecDeque<Action> {
    use Action::*;
    match issue {
        "154-motion" => vec![
            Press(Key::H, Modifiers::SHIFT), Check("posed horizontal reflection"),
            Press(Key::Z, ctrl()), Check("undo restores exact artwork"),
            Press(Key::V, Modifiers::SHIFT), Check("posed vertical reflection"),
            Press(Key::Z, ctrl()), Check("undo restores exact artwork"),
            Press(Key::H, Modifiers::SHIFT), Check("posed horizontal reflection"),
            Press(Key::S, ctrl()), Check("save reopen"),
        ].into(),
        "154" => vec![
            Check("one flip pair in inspector"),
            Click("\u{ED6A}"),
            Check("flip is one edit"),
            Press(Key::Z, ctrl()),
            Check("undo restores exact artwork"),
            Press(Key::Z, ctrl() | Modifiers::SHIFT),
            Check("redo restores flip"),
            Press(Key::H, Modifiers::SHIFT),
            Check("horizontal shortcut"),
            Click("Object"),
            Click("Transform"),
            Check("menu shortcuts"),
            Click("Flip Vertical"),
            World(Pt::new(150., 200.), PointerButton::Secondary),
            Click("Transform"),
            Click("Flip Horizontal"),
            ScrollLayer,
            Right("Amber triangle"),
            Click("Transform"),
            Click("Flip Vertical"),
            Click("\u{E6D6}"),
            Press(Key::H, Modifiers::SHIFT),
            Check("Layout shortcut"),
            Click("\u{E730}"),
            Press(Key::V, Modifiers::SHIFT),
            Check("Motion shortcut"),
            Click("\u{EB00}"),
            Press(Key::T, Modifiers::NONE),
            World(Pt::new(100., 420.), PointerButton::Primary),
            Press(Key::H, Modifiers::SHIFT),
            Type("H"),
            Press(Key::V, Modifiers::SHIFT),
            Type("V"),
            Check("typing preserves H and V"),
            Press(Key::Escape, Modifiers::NONE),
            Press(Key::S, ctrl()),
            Check("save reopen"),
        ]
        .into(),
        _ => panic!("unknown issue {issue}"),
    }
}
impl Qa {
    fn new(issue: String, output: PathBuf) -> Self {
        let mut studio = fixture();
        if issue == "154-motion" {
            use omadesign::motion::{Prop,Ease};
            studio.persona=Persona::Motion;studio.playhead=0.7;
            for (i,(_,id)) in studio.selection.clone().into_iter().enumerate() {
                for (prop,a,b) in [(Prop::X,0.,80.),(Prop::Y,-20.,40.),(Prop::Rotation,-0.2,0.3),(Prop::Width,0.8,1.2),(Prop::Height,1.1,0.9)] {
                    studio.doc.motion.set_key(id,prop,0.,a*(i as f32+1.),Ease::Linear);
                    studio.doc.motion.set_key(id,prop,1.4,b*(i as f32+1.),Ease::EaseInOut);
                }
            }
        }
        studio.path = Some(output.join("result.oma"));
        let original = omadesign::project::encode(&studio.doc).unwrap();
        let history = studio.history.len();
        Self {
            studio,
            actions: actions(&issue),
            issue,
            output,
            labels: vec![],
            events: vec![],
            cursor: egui::pos2(850., 80.),
            frame: 0,
            warm: 0,
            pending: false,
            step_ready: true,
            encoder: None,
            original,
            changed: String::new(),
            history,
            checks: vec![],
            started: Instant::now(),
        }
    }
    fn point(&self, label: &str) -> Pos2 {
        self.labels
            .iter()
            .find(|(s, _)| s == label)
            .map(|(_, r)| r.center())
            .unwrap_or_else(|| panic!("missing {label:?}; labels {:?}", self.labels))
    }
    fn pointer(&mut self, p: Pos2, button: PointerButton) {
        self.cursor = p;
        self.events.push(Event::PointerMoved(p));
        for pressed in [true, false] {
            self.events.push(Event::PointerButton {
                pos: p,
                button,
                pressed,
                modifiers: Modifiers::NONE,
            });
        }
    }
    fn step(&mut self) {
        if self.frame % FPS != 0 {
            return;
        }
        if let Some(action) = self.actions.pop_front() {
            match action {
                Action::Click(s) => self.pointer(self.point(s), PointerButton::Primary),
                Action::ScrollLayer => {
                    let p = self.point("Artwork");
                    self.cursor = p;
                    self.events.extend([
                        Event::PointerMoved(p),
                        Event::MouseWheel {
                            unit: egui::MouseWheelUnit::Point,
                            delta: egui::vec2(0., -160.),
                            modifiers: Modifiers::NONE,
                            phase: egui::TouchPhase::Move,
                        },
                    ]);
                }
                Action::Right(s) => self.pointer(self.point(s), PointerButton::Secondary),
                Action::World(p, button) => {
                    let p = self.studio.view.to_screen(p);
                    self.pointer(
                        self.studio.canvas_rect.unwrap().min + egui::vec2(p.x, p.y),
                        button,
                    );
                }
                Action::Press(key, modifiers) => {
                    self.events.push(Event::ModifiersChanged(modifiers));
                    for pressed in [true, false] {
                        self.events.push(Event::Key {
                            key,
                            physical_key: Some(key),
                            pressed,
                            repeat: false,
                            modifiers,
                        });
                    }
                }
                Action::Type(s) => self.events.push(Event::Text(s.into())),
                Action::Check(name) => self.check(name),
            }
        }
    }
    fn check(&mut self, name: &str) {
        match name {
            "posed horizontal reflection" | "posed vertical reflection" => {
                let horizontal=name=="posed horizontal reflection";
                let before=omadesign::project::decode(&self.original).unwrap();
                let axis=self.studio.selection.iter().map(|&(li,id)|before.motion.pose(id,self.studio.playhead).map_bounds(before.find_shape(li,id).unwrap().world_bbox())).reduce(|a,b|a.union(b)).unwrap().center();
                for &(li,id) in &self.studio.selection {
                    let original=before.find_shape(li,id).unwrap();let actual=self.studio.doc.find_shape(li,id).unwrap();
                    for t in [0.,self.studio.playhead,1.4] {
                        let points:Vec<_>=actual.world_contours(64).into_iter().flatten().map(|p|self.studio.doc.motion.pose(id,t).map(actual.world_bbox().center(),p)).collect();
                        for p in original.world_contours(64).into_iter().flatten() {
                            let p=before.motion.pose(id,t).map(original.world_bbox().center(),p);
                            let expected=if horizontal {Pt::new(2.*axis.x-p.x,p.y)}else{Pt::new(p.x,2.*axis.y-p.y)};
                            assert!(points.iter().any(|q|(*q-expected).length()<0.003));
                        }
                    }
                }
                assert_eq!(self.studio.history.len(),self.history+1);
            }
            "one flip pair in inspector" => {
                for glyph in ["\u{ED6A}", "\u{ED6C}"] {
                    assert_eq!(self.labels.iter().filter(|(s, _)| s == glyph).count(), 1);
                }
                assert!(!self.labels.iter().any(|(s, _)| matches!(
                    s.as_str(),
                    "Flip" | "Flip H" | "Flip V" | "Horizontal" | "Vertical"
                )));
            }
            "flip is one edit" => {
                assert_eq!(self.studio.history.len(), self.history + 1);
                self.changed = omadesign::project::encode(&self.studio.doc).unwrap();
                assert_ne!(self.changed, self.original);
            }
            "undo restores exact artwork" => assert_eq!(
                omadesign::project::encode(&self.studio.doc).unwrap(),
                self.original
            ),
            "redo restores flip" => assert_eq!(
                omadesign::project::encode(&self.studio.doc).unwrap(),
                self.changed
            ),
            "horizontal shortcut" => assert!(self.studio.status.starts_with("Flipped horizontal")),
            "menu shortcuts" => {
                for label in ["Flip Horizontal", "Flip Vertical", "Shift+H", "Shift+V"] {
                    assert!(
                        self.labels.iter().any(|(s, _)| s == label),
                        "missing menu {label}"
                    );
                }
            }
            "Layout shortcut" => {
                assert_eq!(self.studio.persona, Persona::Layout);
                assert!(self.studio.status.starts_with("Flipped horizontal"));
            }
            "Motion shortcut" => {
                assert_eq!(self.studio.persona, Persona::Motion);
                assert!(self.studio.status.starts_with("Flipped vertical"));
            }
            "typing preserves H and V" => assert!(
                self.studio
                    .doc
                    .layers
                    .iter()
                    .filter_map(|l| l.kind.shapes())
                    .flatten()
                    .any(|s| matches!(&s.geom,Geom::Text(t) if t.content=="HV"))
            ),
            "save reopen" => {
                let saved =
                    omadesign::project::load_from(self.studio.path.as_ref().unwrap()).unwrap();
                assert_eq!(
                    omadesign::project::encode(&saved).unwrap(),
                    omadesign::project::encode(&self.studio.doc).unwrap()
                );
            }
            _ => panic!("unknown check {name}"),
        }
        eprintln!("PASS: {name}");
        self.checks.push(name.into());
    }
    fn labels(&mut self, ctx: &egui::Context) {
        fn visit(s: &egui::Shape, clip: Rect, out: &mut Vec<(String, Rect)>) {
            match s {
                egui::Shape::Text(t) => {
                    let r = t.galley.rect.translate(t.pos.to_vec2());
                    if clip.contains(r.center()) {
                        out.push((t.galley.job.text.clone(), r));
                    }
                }
                egui::Shape::Vec(v) => {
                    for s in v {
                        visit(s, clip, out)
                    }
                }
                _ => {}
            }
        }
        self.labels.clear();
        let layers = ctx.memory(|m| m.layer_ids().collect::<Vec<_>>());
        ctx.graphics(|g| {
            for layer in layers {
                if let Some(list) = g.get(layer) {
                    for s in list.all_entries() {
                        visit(&s.shape, s.clip_rect, &mut self.labels);
                    }
                }
            }
        });
    }
}
impl eframe::App for Qa {
    fn raw_input_hook(&mut self, _: &egui::Context, input: &mut egui::RawInput) {
        input
            .events
            .retain(|e| matches!(e, Event::Screenshot { .. }));
        input.focused = true;
        input.time = Some(self.frame as f64 / FPS as f64);
        input.events.push(Event::ModifiersChanged(Modifiers::NONE));
        if self.warm >= 30 && !self.pending && self.step_ready {
            self.step();
            self.step_ready = false;
        }
        input.events.append(&mut self.events);
    }
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        eframe::App::ui(&mut self.studio, ui, frame);
        self.labels(&ctx);
        self.warm += 1;
        let painter = ctx.layer_painter(egui::LayerId::new(
            egui::Order::Tooltip,
            egui::Id::new("qa-pointer"),
        ));
        painter.circle_stroke(
            self.cursor,
            6.,
            egui::Stroke::new(1.5, egui::Color32::WHITE),
        );
        if let Some(image) = ctx.input(|i| {
            i.events.iter().find_map(|e| {
                if let Event::Screenshot { image, .. } = e {
                    Some(image.clone())
                } else {
                    None
                }
            })
        }) {
            assert_eq!(image.size, SIZE);
            let bytes: Vec<_> = image.pixels.iter().flat_map(|p| p.to_array()).collect();
            let path = self.output.join(format!("issue-{}.mp4", self.issue));
            let (_, pipe) = self.encoder.get_or_insert_with(|| {
                let mut child = Command::new("ffmpeg")
                    .args([
                        "-y",
                        "-loglevel",
                        "error",
                        "-f",
                        "rawvideo",
                        "-pixel_format",
                        "rgba",
                        "-video_size",
                        "1600x1000",
                        "-framerate",
                        "10",
                        "-i",
                        "pipe:0",
                        "-an",
                        "-c:v",
                        "libx264",
                        "-preset",
                        "fast",
                        "-crf",
                        "22",
                        "-pix_fmt",
                        "yuv420p",
                        "-movflags",
                        "+faststart",
                    ])
                    .arg(path)
                    .stdin(Stdio::piped())
                    .spawn()
                    .unwrap();
                let pipe = child.stdin.take().unwrap();
                (child, pipe)
            });
            pipe.write_all(&bytes).unwrap();
            if self.frame.is_multiple_of(FPS * 2) {
                image::save_buffer(
                    self.output.join(format!("frame-{:04}.png", self.frame)),
                    &bytes,
                    SIZE[0] as u32,
                    SIZE[1] as u32,
                    image::ColorType::Rgba8,
                )
                .unwrap();
                fs::write(self.output.join("labels.txt"), format!("{:?}", self.labels)).unwrap();
            }
            self.frame += 1;
            self.pending = false;
            self.step_ready = true;
            if self.actions.is_empty() && self.frame.is_multiple_of(FPS) {
                let (mut child, pipe) = self.encoder.take().unwrap();
                drop(pipe);
                assert!(child.wait().unwrap().success());
                fs::write(self.output.join("result.json"),serde_json::to_vec_pretty(&serde_json::json!({"issue":self.issue,"checks":self.checks,"frames":self.frame,"fps":FPS,"size":SIZE,"renderer":"native WGPU","input":"actual egui pointer and keyboard events","wall_seconds":self.started.elapsed().as_secs_f64()})).unwrap()).unwrap();
                self.studio.allow_close = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                return;
            }
        }
        if self.warm >= 30
            && !self.pending
            && !self.step_ready
            && omadesign::ui::scene_ready(&ctx, &self.studio)
        {
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(Default::default()));
            self.pending = true;
        }
        assert!(
            self.started.elapsed() < Duration::from_secs(600),
            "recording timed out"
        );
        ctx.request_repaint();
    }
}
fn main() -> eframe::Result {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let issue = args[0].clone();
    let output = PathBuf::from(&args[1]);
    fs::create_dir_all(&output).unwrap();
    let profile =
        std::env::temp_dir().join(format!("omadesign-inspector-qa-{}", std::process::id()));
    for (var, dir) in [
        ("XDG_CONFIG_HOME", "config"),
        ("XDG_DATA_HOME", "data"),
        ("XDG_CACHE_HOME", "cache"),
        ("XDG_STATE_HOME", "state"),
    ] {
        let p = profile.join(dir);
        fs::create_dir_all(&p).unwrap();
        unsafe {
            std::env::set_var(var, p);
        }
    }
    eframe::run_native(
        "omadesign-inspector-qa",
        eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default()
                .with_title(format!("Omadesign issue #{issue} acceptance"))
                .with_inner_size([1600., 1000.])
                .with_min_inner_size([1600., 1000.])
                .with_max_inner_size([1600., 1000.])
                .with_resizable(false),
            renderer: eframe::Renderer::Wgpu,
            ..Default::default()
        },
        Box::new(move |cc| {
            omadesign::ui::theme::apply(&cc.egui_ctx);
            cc.egui_ctx.set_pixels_per_point(1.);
            Ok(Box::new(Qa::new(issue, output)))
        }),
    )
}
