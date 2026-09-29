//! Native pen/brush/type workload on a supplied, editable dense document.
//! Usage: authoring_qa seed.oma type-tasks.json output-directory
use eframe::egui::{self, Event, Key, Modifiers, PointerButton};
use omadesign::{
    app::Studio,
    color::Rgba,
    document::{Fill, Stroke},
    geom::{Geom, Pt},
    tools::Persona,
};
use std::{
    collections::{BTreeMap, VecDeque},
    path::PathBuf,
    time::{Duration, Instant},
};

#[derive(Clone, serde::Deserialize)]
struct TextTask {
    x: f32,
    y: f32,
    text: String,
    size: f32,
    font: String,
    color: String,
}
enum Action {
    Frame(&'static str, Vec<Event>),
    Point(&'static str, Pt, Option<(PointerButton, bool)>),
    Text(TextTask),
    Pen,
    Brush,
    Fit,
    CheckText,
    RememberBrush,
    CheckUndo,
    CheckRedo,
    Save,
}
#[derive(serde::Serialize)]
struct Sample {
    phase: String,
    ui_ms: f64,
    input_to_ui_ms: f64,
    frame_interval_ms: f64,
    canvas_changed: bool,
}
struct Qa {
    studio: Studio,
    output: PathBuf,
    tasks: Vec<TextTask>,
    actions: VecDeque<Action>,
    events: Vec<Event>,
    phase: &'static str,
    samples: Vec<Sample>,
    start: Instant,
    input_start: Instant,
    previous_ui: Instant,
    frames: u64,
    brush: Vec<u8>,
    saving: bool,
    done: bool,
    screenshot: bool,
}
fn key(key: Key, ctrl: bool, shift: bool) -> Vec<Event> {
    let modifiers = Modifiers {
        ctrl,
        command: ctrl,
        shift,
        ..Modifiers::NONE
    };
    vec![
        Event::ModifiersChanged(modifiers),
        Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers,
        },
        Event::Key {
            key,
            physical_key: None,
            pressed: false,
            repeat: false,
            modifiers,
        },
    ]
}
fn click(actions: &mut VecDeque<Action>, phase: &'static str, p: Pt) {
    actions.push_back(Action::Point(
        phase,
        p,
        Some((PointerButton::Primary, true)),
    ));
    actions.push_back(Action::Point(
        phase,
        p,
        Some((PointerButton::Primary, false)),
    ));
}
fn workload(tasks: &[TextTask]) -> VecDeque<Action> {
    let mut q = VecDeque::new();
    for _ in 0..60 {
        q.push_back(Action::Frame("idle", vec![]));
    }
    for task in tasks {
        q.push_back(Action::Text(task.clone()));
        q.push_back(Action::Frame("type-setup", key(Key::T, false, false)));
        click(&mut q, "type-place", Pt::new(task.x, task.y));
        for c in task.text.chars() {
            q.push_back(Action::Frame("typing", vec![Event::Text(c.to_string())]));
        }
        q.push_back(Action::Frame("type-commit", key(Key::Escape, false, false)));
    }
    q.push_back(Action::CheckText);
    q.push_back(Action::Pen);
    q.push_back(Action::Frame("pen-setup", key(Key::P, false, false)));
    for (x, y, dx, dy) in [
        (524., 246., 14., -10.),
        (523., 271., 20., 0.),
        (520., 296., 13., 8.),
    ] {
        click(&mut q, "pen-anchor", Pt::new(x, y));
        for step in 1..=16 {
            q.push_back(Action::Point(
                "pen-preview",
                Pt::new(x + dx * step as f32 / 16., y + dy * step as f32 / 16.),
                None,
            ));
        }
        click(&mut q, "pen-anchor", Pt::new(x + dx, y + dy));
        q.push_back(Action::Frame("pen-commit", key(Key::Enter, false, false)));
    }
    q.push_back(Action::Brush);
    q.push_back(Action::Frame("brush-setup", key(Key::B, false, false)));
    for line in 0..3 {
        let y = 359. + line as f32 * 7.;
        q.push_back(Action::Point(
            "brush-down",
            Pt::new(470., y),
            Some((PointerButton::Primary, true)),
        ));
        for step in 1..=30 {
            let x = 470. + step as f32 * 2.;
            q.push_back(Action::Point(
                "brush-drag",
                Pt::new(x, y - (step as f32 * 0.14).sin() * 3.),
                None,
            ));
        }
        q.push_back(Action::Point(
            "brush-up",
            Pt::new(530., y - (30_f32 * 0.14).sin() * 3.),
            Some((PointerButton::Primary, false)),
        ));
    }
    q.push_back(Action::RememberBrush);
    q.push_back(Action::Frame("undo", key(Key::Z, true, false)));
    q.push_back(Action::CheckUndo);
    q.push_back(Action::Frame("redo", key(Key::Z, true, true)));
    q.push_back(Action::CheckRedo);
    q.push_back(Action::Frame("select", key(Key::V, false, false)));
    q.push_back(Action::Point(
        "pan-start",
        Pt::new(600., 450.),
        Some((PointerButton::Middle, true)),
    ));
    // Window coordinates are derived from the original camera, not the moving camera.
    for _ in 0..40 {
        q.push_back(Action::Frame("pan", vec![]));
    }
    q.push_back(Action::Point(
        "pan-end",
        Pt::new(600., 450.),
        Some((PointerButton::Middle, false)),
    ));
    for factor in [1.018, 1. / 1.018] {
        for _ in 0..30 {
            q.push_back(Action::Frame("zoom", vec![Event::Zoom(factor)]));
        }
    }
    q.push_back(Action::Fit);
    for _ in 0..30 {
        q.push_back(Action::Frame("idle-complete", vec![]));
    }
    q.push_back(Action::Save);
    q
}
impl Qa {
    fn pixels(&self) -> &[u8] {
        &self.studio.doc.layers[self.studio.active_layer.unwrap()]
            .kind
            .pixels()
            .unwrap()
            .data
    }
    fn next(&mut self, ctx: &egui::Context) {
        self.phase = "setup";
        match self.actions.pop_front() {
            Some(Action::Frame(phase, mut events)) => {
                self.phase = phase;
                if phase == "pan" {
                    let pos = ctx.input(|i| i.pointer.latest_pos()).unwrap();
                    events = vec![Event::PointerMoved(pos + egui::vec2(2., 1.))];
                }
                self.events = events;
            }
            Some(Action::Point(phase, p, button)) => {
                self.phase = phase;
                let rect = self.studio.canvas_rect.unwrap();
                let p = self
                    .studio
                    .view
                    .world_to_window(Pt::new(rect.min.x, rect.min.y), p);
                let pos = egui::pos2(p.x, p.y);
                assert!(rect.contains(pos), "test point outside canvas");
                self.events = vec![Event::PointerMoved(pos)];
                if let Some((button, pressed)) = button {
                    self.events.push(Event::PointerButton {
                        pos,
                        button,
                        pressed,
                        modifiers: Modifiers::NONE,
                    });
                }
            }
            Some(Action::Text(t)) => {
                self.studio.persona = Persona::Design;
                self.studio.selection.clear();
                self.studio.text_px = t.size;
                self.studio.text_font = t.font;
                let n = u32::from_str_radix(t.color.trim_start_matches('#'), 16).unwrap();
                self.studio.style.fill =
                    Fill::Solid(Rgba::rgb((n >> 16) as u8, (n >> 8) as u8, n as u8));
                self.studio.style.stroke = None;
            }
            Some(Action::Pen) => {
                self.studio.selection.clear();
                self.studio.add_layer(false);
                self.studio.doc.layers[self.studio.active_layer.unwrap()].name =
                    "Pen — sunlight rays".into();
                self.studio.style.fill = Fill::None;
                self.studio.style.stroke = Some(Stroke {
                    color: Rgba::rgb(255, 212, 10),
                    width: 5.,
                    ..Default::default()
                });
            }
            Some(Action::Brush) => {
                assert_eq!(
                    self.studio.doc.layers[self.studio.active_layer.unwrap()]
                        .kind
                        .shapes()
                        .unwrap()
                        .len(),
                    3,
                    "real pen paths"
                );
                self.studio.selection.clear();
                self.studio.add_layer(true);
                self.studio.persona = Persona::Pixel;
                self.studio.doc.layers[self.studio.active_layer.unwrap()].name =
                    "Brush — sea glints".into();
                self.studio.brush.size = 3.;
                self.studio.brush.opacity = 0.8;
                self.studio.brush.flow = 0.8;
                self.studio.brush.color = Rgba::rgb(239, 255, 255);
            }
            Some(Action::Fit) => {
                self.studio.need_fit = true;
            }
            Some(Action::CheckText) => {
                for task in &self.tasks {
                    assert!(
                        self.studio
                            .doc
                            .layers
                            .iter()
                            .filter_map(|l| l.kind.shapes())
                            .flatten()
                            .any(|s| matches!(&s.geom,Geom::Text(t) if t.content==task.text)),
                        "missing typed text {}",
                        task.text
                    );
                }
            }
            Some(Action::RememberBrush) => {
                assert!(self.pixels().chunks_exact(4).any(|p| p[3] > 0));
                self.brush = self.pixels().to_vec();
            }
            Some(Action::CheckUndo) => {
                assert_ne!(
                    self.pixels(),
                    self.brush.as_slice(),
                    "undo removed last real brush stroke"
                );
            }
            Some(Action::CheckRedo) => {
                assert_eq!(
                    self.pixels(),
                    self.brush.as_slice(),
                    "redo restored exact brush pixels"
                );
            }
            Some(Action::Save) => {
                self.events = key(Key::S, true, false);
                self.phase = "save";
                self.saving = true;
            }
            None if self.saving
                && self.studio.path.as_ref().unwrap().exists()
                && omadesign::ui::scene_ready(ctx, &self.studio) =>
            {
                let saved =
                    omadesign::project::load_from(self.studio.path.as_ref().unwrap()).unwrap();
                assert_eq!(
                    omadesign::project::encode(&saved).unwrap(),
                    omadesign::project::encode(&self.studio.doc).unwrap()
                );
                std::fs::write(
                    self.output.join("infographic.png"),
                    omadesign::compositor::export_png(&saved, 1).unwrap(),
                )
                .unwrap();
                ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(Default::default()));
                self.screenshot = true;
                self.saving = false;
            }
            _ => {}
        }
    }
    fn finish(&mut self, ctx: &egui::Context) {
        for (phase, minimum) in [
            ("typing", 83),
            ("brush-drag", 85),
            ("pan", 35),
            ("zoom", 55),
        ] {
            assert!(
                self.samples
                    .iter()
                    .filter(|s| s.phase == phase && s.canvas_changed)
                    .count()
                    >= minimum,
                "{phase} must actually update the native canvas"
            );
        }
        let mut groups: BTreeMap<String, Vec<f64>> = BTreeMap::new();
        for s in &self.samples {
            groups.entry(s.phase.clone()).or_default().push(s.ui_ms);
        }
        let stats:BTreeMap<_,_>=groups.into_iter().map(|(phase,mut v)|{
            v.sort_by(f64::total_cmp);let n=v.len();
            (phase,serde_json::json!({"frames":n,"p50_ms":v[n/2],"p95_ms":v[(n*95/100).min(n-1)],"max_ms":v[n-1]}))
        }).collect();
        let canvas = self.studio.canvas_rect.unwrap();
        let report = serde_json::json!({"elapsed_seconds":self.start.elapsed().as_secs_f64(),"canvas_size":[canvas.width(),canvas.height()],"renderer":"native WGPU","timing":"full Studio::ui CPU duration; frame intervals also recorded; excludes GPU presentation and human decision time","input":"real egui pointer and keyboard events, one event step per frame","layers":self.studio.doc.layers.len(),"vector_shapes":self.studio.doc.layers.iter().filter_map(|l|l.kind.shapes()).map(|s|s.len()).sum::<usize>(),"typed_objects":self.tasks.len(),"pen_paths":3,"brush_strokes":3,"undo_redo":true,"save_reopen_exact":true,"phases":stats,"samples":self.samples});
        std::fs::write(
            self.output.join("authoring-result.json"),
            serde_json::to_vec_pretty(&report).unwrap(),
        )
        .unwrap();
        eprintln!("AUTHORING COMPLETE {}", report["phases"]);
        self.done = true;
        self.studio.allow_close = true;
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
    }
}
impl eframe::App for Qa {
    fn raw_input_hook(&mut self, _: &egui::Context, input: &mut egui::RawInput) {
        input
            .events
            .retain(|e| matches!(e, Event::Screenshot { .. }));
        input.focused = true;
        input.events.push(Event::ModifiersChanged(Modifiers::NONE));
        input.events.append(&mut self.events);
        self.input_start = Instant::now();
    }
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        let start = Instant::now();
        let key = self.studio.canvas_key;
        eframe::App::ui(&mut self.studio, ui, frame);
        let now = Instant::now();
        self.samples.push(Sample {
            phase: self.phase.into(),
            ui_ms: start.elapsed().as_secs_f64() * 1000.,
            input_to_ui_ms: self.input_start.elapsed().as_secs_f64() * 1000.,
            frame_interval_ms: now.duration_since(self.previous_ui).as_secs_f64() * 1000.,
            canvas_changed: key != self.studio.canvas_key,
        });
        self.previous_ui = now;
        self.frames += 1;
        if self.frames == 2 {
            ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(1440., 900.)));
        }
        if self.screenshot
            && let Some(image) = ctx.input(|i| {
                i.events.iter().find_map(|e| {
                    if let Event::Screenshot { image, .. } = e {
                        Some(image.clone())
                    } else {
                        None
                    }
                })
            })
        {
            let bytes: Vec<_> = image.pixels.iter().flat_map(|p| p.to_array()).collect();
            image::save_buffer(
                self.output.join("native-editor.png"),
                &bytes,
                image.size[0] as u32,
                image.size[1] as u32,
                image::ColorType::Rgba8,
            )
            .unwrap();
            self.finish(&ctx);
        }
        if !self.done && self.frames > 30 {
            self.next(&ctx);
        }
        assert!(
            self.start.elapsed() < Duration::from_secs(1200),
            "authoring workload timeout"
        );
        ctx.request_repaint_after(Duration::from_millis(16));
    }
}
fn main() -> eframe::Result {
    let args: Vec<_> = std::env::args().collect();
    assert_eq!(args.len(), 4, "seed.oma type-tasks.json output-dir");
    let input = PathBuf::from(&args[1]);
    let output = std::env::current_dir().unwrap().join(&args[3]);
    std::fs::create_dir_all(&output).unwrap();
    for (key, name) in [
        ("XDG_CONFIG_HOME", "config"),
        ("XDG_DATA_HOME", "data"),
        ("XDG_CACHE_HOME", "cache"),
        ("XDG_STATE_HOME", "state"),
    ] {
        let p = output.join("profile").join(name);
        std::fs::create_dir_all(&p).unwrap();
        unsafe {
            std::env::set_var(key, p);
        }
    }
    let tasks: Vec<TextTask> = serde_json::from_slice(&std::fs::read(&args[2]).unwrap()).unwrap();
    let mut studio = Studio::new();
    studio.doc = omadesign::project::load_from(&input).unwrap();
    studio.path = Some(output.join("infographic.oma"));
    studio.show_welcome = false;
    studio.need_fit = true;
    studio.startup_preferences.check_updates = false;
    studio.persona = Persona::Design;
    studio.add_layer(false);
    studio.doc.layers[studio.active_layer.unwrap()].name = "Type — weekly headline".into();
    let actions = workload(&tasks);
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1440., 900.])
            .with_min_inner_size([1440., 900.])
            .with_max_inner_size([1440., 900.])
            .with_resizable(false)
            .with_title("Omadesign · everyday authoring QA"),
        renderer: eframe::Renderer::Wgpu,
        ..Default::default()
    };
    eframe::run_native(
        "omadesign-authoring-qa",
        options,
        Box::new(move |cc| {
            omadesign::ui::theme::apply(&cc.egui_ctx);
            cc.egui_ctx.set_pixels_per_point(1.);
            Ok(Box::new(Qa {
                studio,
                output,
                tasks,
                actions,
                events: vec![],
                phase: "startup",
                samples: vec![],
                start: Instant::now(),
                input_start: Instant::now(),
                previous_ui: Instant::now(),
                frames: 0,
                brush: vec![],
                saving: false,
                done: false,
                screenshot: false,
            }))
        }),
    )
}
