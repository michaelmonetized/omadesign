//! Repeatable native object-drag workload, including inactive document pressure.
//! Usage: interaction_qa output-directory simple|complex|complex-middle|complex-same-layer|many-tabs
use eframe::egui::{self, Event, Key, Modifiers, PointerButton};
use omadesign::{
    app::Studio,
    color::Rgba,
    document::{Document, Fill, Layer, Pixels, Shape, Stroke, Style},
    geom::{Geom, Pt},
    tools::{Persona, Tool},
};
use std::{
    collections::{BTreeMap, VecDeque},
    path::PathBuf,
    time::{Duration, Instant},
};

const SIZE: Pt = Pt {
    x: 1600.0,
    y: 1000.0,
};
const TARGET: Pt = Pt { x: 740.0, y: 455.0 };
const TARGET_SIZE: Pt = Pt { x: 120.0, y: 90.0 };

fn fixture(name: &str, shapes: usize) -> Document {
    // Keep inactive tabs representative without allocating a canvas-size blank
    // raster per document. Raster placement still covers the entire artboard.
    let mut doc = Document::new(name, SIZE.x, SIZE.y, 72.0);
    doc.workspace = Some(Persona::Design);
    doc.grid.visible = false;
    let mut pixels = Pixels::new(320, 200);
    for y in 0..pixels.h {
        for x in 0..pixels.w {
            let i = ((y * pixels.w + x) * 4) as usize;
            pixels.data[i..i + 4].copy_from_slice(&[
                225 + ((x + y) % 25) as u8,
                230 + ((x / 8 + y / 8) % 20) as u8,
                240,
                255,
            ]);
        }
    }
    pixels.touch();
    doc.layers = vec![
        Layer::placed_raster("Photo", pixels, Pt::ZERO, SIZE),
        Layer::vector("Dense vector artwork"),
    ];
    let objects = doc.layers[1].kind.shapes_mut().unwrap();
    for i in 0..shapes {
        let x = (i % 40) as f32 * 39.0 + 6.0;
        let y = (i / 40) as f32 * 30.0 + 6.0;
        let geom = if i % 5 == 0 {
            Geom::Star {
                center: Pt::new(x + 14.0, y + 12.0),
                outer: Pt::new(17.0, 13.0),
                inner: 0.5,
                points: 7,
            }
        } else {
            Geom::Rect {
                origin: Pt::new(x, y),
                size: Pt::new(29.0, 21.0),
                radius: (i % 4) as f32 * 2.0,
            }
        };
        let mut shape = Shape::new(
            geom,
            Style {
                fill: Fill::Solid(Rgba::rgb(
                    (i * 19 % 210) as u8,
                    (i * 31 % 190) as u8,
                    (i * 43 % 220) as u8,
                )),
                stroke: Some(Stroke {
                    width: 1.5,
                    color: Rgba::rgb(42, 50, 70),
                    ..Default::default()
                }),
            },
        );
        shape.opacity = 0.82;
        if i % 40 == 0 {
            let mut mask = Pixels::new(128, 128);
            for (pixel, rgba) in mask.data.chunks_exact_mut(4).enumerate() {
                rgba.copy_from_slice(&[255, 255, 255, 96 + (pixel % 160) as u8]);
            }
            mask.touch();
            shape.mask = Some(mask);
        }
        objects.push(shape);
    }
    doc
}

enum Action {
    Idle,
    WaitReady,
    Point(&'static str, Pt, Option<bool>),
    Key(&'static str, bool),
    Check(Pt),
    CheckTabs,
    Save,
}

#[derive(serde::Serialize)]
struct Sample {
    phase: &'static str,
    ui_ms: f64,
    input_to_ui_ms: f64,
    frame_interval_ms: f64,
    canvas_changed: bool,
    object_moved: bool,
}

struct Qa {
    studio: Studio,
    output: PathBuf,
    scenario: String,
    target: (usize, u64),
    initial: Geom,
    inactive: Vec<String>,
    actions: VecDeque<Action>,
    events: Vec<Event>,
    phase: &'static str,
    samples: Vec<Sample>,
    start: Instant,
    input_start: Instant,
    previous_ui: Instant,
    frames: usize,
    checked_moves: usize,
    saved: bool,
    screenshot_requested: bool,
    done: bool,
}

fn workload() -> VecDeque<Action> {
    let mut actions = VecDeque::new();
    for _ in 0..30 {
        actions.push_back(Action::Idle);
    }
    actions.push_back(Action::WaitReady);
    let mut offset = Pt::ZERO;
    for delta in [
        Pt::new(180.0, 0.0),
        Pt::new(0.0, 120.0),
        Pt::new(-180.0, 0.0),
        Pt::new(0.0, -120.0),
    ] {
        let center = TARGET + TARGET_SIZE * 0.5 + offset;
        actions.push_back(Action::Point("drag-hover", center, None));
        actions.push_back(Action::Point("drag-down", center, Some(true)));
        for step in 1..=90 {
            actions.push_back(Action::Point(
                "drag-move",
                center + delta * (step as f32 / 90.0),
                None,
            ));
        }
        actions.push_back(Action::Point("drag-up", center + delta, Some(false)));
        actions.push_back(Action::Check(TARGET + offset + delta));
        actions.push_back(Action::Key("undo", false));
        actions.push_back(Action::Check(TARGET + offset));
        actions.push_back(Action::Key("redo", true));
        actions.push_back(Action::Check(TARGET + offset + delta));
        offset += delta;
    }
    actions.push_back(Action::CheckTabs);
    for _ in 0..30 {
        actions.push_back(Action::Idle);
    }
    actions.push_back(Action::WaitReady);
    actions.push_back(Action::Save);
    actions
}

impl Qa {
    fn origin(&self) -> Pt {
        match &self
            .studio
            .doc
            .find_shape(self.target.0, self.target.1)
            .expect("drag target exists")
            .geom
        {
            Geom::Rect { origin, .. } => *origin,
            _ => panic!("target geometry changed"),
        }
    }

    fn next(&mut self, ctx: &egui::Context) {
        self.phase = "check";
        match self.actions.pop_front() {
            Some(Action::Idle) => self.phase = "idle",
            Some(Action::WaitReady) => {
                self.phase = "preview-wait";
                if !omadesign::ui::scene_ready(ctx, &self.studio) {
                    self.actions.push_front(Action::WaitReady);
                }
            }
            Some(Action::Point(phase, point, pressed)) => {
                self.phase = phase;
                let rect = self.studio.canvas_rect.unwrap();
                let point = self
                    .studio
                    .view
                    .world_to_window(Pt::new(rect.min.x, rect.min.y), point);
                let pos = egui::pos2(point.x, point.y);
                assert!(
                    rect.contains(pos),
                    "drag point outside visible canvas: {pos:?}"
                );
                self.events.push(Event::PointerMoved(pos));
                if let Some(pressed) = pressed {
                    self.events.push(Event::PointerButton {
                        pos,
                        button: PointerButton::Primary,
                        pressed,
                        modifiers: Modifiers::NONE,
                    });
                }
            }
            Some(Action::Key(phase, redo)) => {
                self.phase = phase;
                let modifiers = Modifiers {
                    ctrl: true,
                    command: true,
                    shift: redo,
                    ..Modifiers::NONE
                };
                self.events.push(Event::ModifiersChanged(modifiers));
                for pressed in [true, false] {
                    self.events.push(Event::Key {
                        key: Key::Z,
                        physical_key: None,
                        pressed,
                        repeat: false,
                        modifiers,
                    });
                }
            }
            Some(Action::Check(expected)) => {
                let actual = self.origin();
                assert!(
                    (actual.x - expected.x).abs() < 0.05 && (actual.y - expected.y).abs() < 0.05,
                    "real move/undo/redo failed: expected {expected:?}, actual {actual:?}"
                );
                assert_eq!(
                    self.studio.selection,
                    vec![self.target],
                    "drag must select only the intended target"
                );
                self.checked_moves += 1;
            }
            Some(Action::CheckTabs) => {
                let mut expected = self.initial.clone();
                if let Geom::Rect { origin, .. } = &mut expected {
                    *origin = self.origin();
                }
                assert_eq!(
                    self.studio
                        .doc
                        .find_shape(self.target.0, self.target.1)
                        .unwrap()
                        .geom,
                    expected
                );
                for (i, expected) in self.inactive.iter().enumerate() {
                    self.studio.switch_tab(i + 1);
                    assert_eq!(
                        omadesign::project::encode(&self.studio.doc).unwrap(),
                        *expected,
                        "inactive document changed during another document's drag"
                    );
                }
                self.studio.switch_tab(0);
                assert_eq!(
                    self.studio.selection,
                    vec![self.target],
                    "tab switch must retain selection"
                );
                assert!(
                    self.studio.history.can_undo(),
                    "tab switch must retain history"
                );
            }
            Some(Action::Save) => {
                let path = self.output.join("interaction.oma");
                self.studio.path = Some(path.clone());
                self.studio.save();
                assert!(!self.studio.dirty && path.exists(), "native save failed");
                let reopened = omadesign::project::load_from(&path).unwrap();
                assert_eq!(
                    omadesign::project::encode(&reopened).unwrap(),
                    omadesign::project::encode(&self.studio.doc).unwrap()
                );
                self.saved = true;
            }
            None if self.saved
                && !self.screenshot_requested
                && omadesign::ui::scene_ready(ctx, &self.studio) =>
            {
                ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(Default::default()));
                self.screenshot_requested = true;
            }
            None => {}
        }
    }

    fn finish(&mut self, ctx: &egui::Context, image: &egui::ColorImage) {
        assert_eq!(self.checked_moves, 12);
        let moving: Vec<_> = self
            .samples
            .iter()
            .filter(|sample| sample.phase == "drag-move")
            .collect();
        assert_eq!(moving.len(), 360);
        assert!(
            moving.iter().filter(|sample| sample.object_moved).count() >= 352,
            "input events must continuously move the actual selected object"
        );
        assert!(
            moving.iter().filter(|sample| sample.canvas_changed).count() >= 352,
            "native canvas must redraw while the target moves"
        );
        let bytes: Vec<_> = image
            .pixels
            .iter()
            .flat_map(|pixel| pixel.to_array())
            .collect();
        image::save_buffer(
            self.output.join("native-editor.png"),
            &bytes,
            image.size[0] as u32,
            image.size[1] as u32,
            image::ColorType::Rgba8,
        )
        .unwrap();
        let mut groups: BTreeMap<&str, Vec<&Sample>> = BTreeMap::new();
        for sample in &self.samples {
            groups.entry(sample.phase).or_default().push(sample);
        }
        let percentile = |mut values: Vec<f64>| {
            values.sort_by(f64::total_cmp);
            let n = values.len();
            let median = if n % 2 == 0 {
                (values[n / 2 - 1] + values[n / 2]) * 0.5
            } else {
                values[n / 2]
            };
            serde_json::json!({"p50_ms": median, "p95_ms": values[(n * 95).div_ceil(100) - 1], "max_ms": values[n - 1]})
        };
        let phases: BTreeMap<_, _> = groups.into_iter().map(|(phase, samples)| {
            (phase, serde_json::json!({
                "frames": samples.len(),
                "ui": percentile(samples.iter().map(|sample| sample.ui_ms).collect()),
                "input_to_ui": percentile(samples.iter().map(|sample| sample.input_to_ui_ms).collect()),
                "frame_interval": percentile(samples.iter().map(|sample| sample.frame_interval_ms).collect()),
            }))
        }).collect();
        let rect = self.studio.canvas_rect.unwrap();
        let report = serde_json::json!({
            "scenario": self.scenario,
            "elapsed_seconds": self.start.elapsed().as_secs_f64(),
            "renderer": "native WGPU",
            "window": [1440, 900],
            "canvas_size": [rect.width(), rect.height()],
            "open_documents": self.studio.tab_count(),
            "active_shapes": self.studio.doc.layers.iter().filter_map(|layer| layer.kind.shapes()).map(|shapes| shapes.len()).sum::<usize>(),
            "inactive_shapes_each": if self.inactive.is_empty() { 0 } else { 320 },
            "timing": "Full Studio::ui CPU duration and real egui input-to-UI duration; frame intervals include scheduling and presentation pacing, not a GPU latency measurement",
            "input": "Real egui pointer and keyboard events through the native window; one movement per frame",
            "checks": { "moves_and_undo_redo": self.checked_moves, "continuous_object_motion": true, "continuous_canvas_updates": true, "inactive_documents_unchanged": true, "tab_selection_and_history": true, "save_reopen_exact": true },
            "phases": phases,
            "samples": self.samples,
        });
        std::fs::write(
            self.output.join("interaction-result.json"),
            serde_json::to_vec_pretty(&report).unwrap(),
        )
        .unwrap();
        eprintln!(
            "INTERACTION COMPLETE {} {}",
            self.scenario, report["phases"]["drag-move"]
        );
        self.done = true;
        self.studio.allow_close = true;
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
    }
}

impl eframe::App for Qa {
    fn raw_input_hook(&mut self, _: &egui::Context, input: &mut egui::RawInput) {
        input
            .events
            .retain(|event| matches!(event, Event::Screenshot { .. }));
        input.focused = true;
        input.events.push(Event::ModifiersChanged(Modifiers::NONE));
        input.events.append(&mut self.events);
        self.input_start = Instant::now();
    }

    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        let origin = self.origin();
        let key = self.studio.canvas_key;
        let start = Instant::now();
        eframe::App::ui(&mut self.studio, ui, frame);
        let now = Instant::now();
        self.samples.push(Sample {
            phase: self.phase,
            ui_ms: now.duration_since(start).as_secs_f64() * 1000.0,
            input_to_ui_ms: now.duration_since(self.input_start).as_secs_f64() * 1000.0,
            frame_interval_ms: now.duration_since(self.previous_ui).as_secs_f64() * 1000.0,
            canvas_changed: key != self.studio.canvas_key,
            object_moved: self.origin() != origin,
        });
        self.previous_ui = now;
        self.frames += 1;
        if self.frames == 2 {
            ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(1440.0, 900.0)));
        }
        if self.screenshot_requested
            && let Some(image) = ctx.input(|input| {
                input.events.iter().find_map(|event| match event {
                    Event::Screenshot { image, .. } => Some(image.clone()),
                    _ => None,
                })
            })
        {
            self.finish(&ctx, &image);
        }
        if !self.done && self.frames >= 10 {
            self.next(&ctx);
        }
        assert!(
            self.start.elapsed() < Duration::from_secs(900),
            "native interaction workload timed out"
        );
        ctx.request_repaint_after(Duration::from_millis(1));
    }
}

fn main() -> eframe::Result {
    let args: Vec<_> = std::env::args().collect();
    assert_eq!(
        args.len(),
        3,
        "usage: interaction_qa output-directory simple|complex|complex-middle|complex-same-layer|many-tabs"
    );
    let output = std::env::current_dir().unwrap().join(&args[1]);
    let scenario = args[2].clone();
    assert!(
        [
            "simple",
            "complex",
            "complex-middle",
            "complex-same-layer",
            "many-tabs"
        ]
        .contains(&scenario.as_str())
    );
    std::fs::create_dir_all(&output).unwrap();
    for (key, directory) in [
        ("XDG_CONFIG_HOME", "config"),
        ("XDG_DATA_HOME", "data"),
        ("XDG_CACHE_HOME", "cache"),
        ("XDG_STATE_HOME", "state"),
    ] {
        let path = output.join("profile").join(directory);
        std::fs::create_dir_all(&path).unwrap();
        unsafe {
            std::env::set_var(key, path);
        }
    }
    let mut studio = Studio::new();
    studio.doc = fixture(
        &scenario,
        if scenario.starts_with("complex") {
            1200
        } else {
            0
        },
    );
    let foreground = if scenario == "complex-middle" {
        let mut layer = Layer::vector("Foreground above the drag target");
        let mut shapes = studio.doc.layers[1]
            .kind
            .shapes_mut()
            .unwrap()
            .split_off(600);
        // Keep the four pointer-down locations exposed, while foreground
        // artwork still overlaps the moving rectangle and its trajectory.
        let center = TARGET + TARGET_SIZE * 0.5;
        let presses = [
            center,
            center + Pt::new(180.0, 0.0),
            center + Pt::new(180.0, 120.0),
            center + Pt::new(0.0, 120.0),
        ];
        for shape in &mut shapes {
            let bounds = shape.world_bbox();
            if presses.iter().any(|point| {
                point.x >= bounds.min.x - 12.0
                    && point.x <= bounds.max.x + 12.0
                    && point.y >= bounds.min.y - 12.0
                    && point.y <= bounds.max.y + 12.0
            }) {
                shape.geom.translate(Pt::new(0.0, -400.0));
            }
        }
        layer.kind.shapes_mut().unwrap().extend(shapes);
        Some(layer)
    } else {
        None
    };
    studio.show_welcome = false;
    studio.persona = Persona::Design;
    studio.tool = Tool::Select;
    studio.snap.enabled = false;
    studio.startup_preferences.check_updates = false;
    let mut layer = Layer::vector("Drag target");
    let shape = Shape::new(
        Geom::Rect {
            origin: TARGET,
            size: TARGET_SIZE,
            radius: 12.0,
        },
        Style {
            fill: Fill::Solid(Rgba::rgb(247, 114, 44)),
            stroke: Some(Stroke {
                width: 4.0,
                color: Rgba::rgb(65, 37, 19),
                ..Default::default()
            }),
        },
    );
    let initial = shape.geom.clone();
    let target = if scenario == "complex-same-layer" {
        let target = (1, shape.id);
        studio.doc.layers[1].kind.shapes_mut().unwrap().push(shape);
        target
    } else {
        let target = (studio.doc.layers.len(), shape.id);
        layer.kind.shapes_mut().unwrap().push(shape);
        studio.doc.layers.push(layer);
        target
    };
    if let Some(layer) = foreground {
        studio.doc.layers.push(layer);
    }
    studio.active_layer = Some(target.0);
    let mut inactive = vec![];
    if scenario == "many-tabs" {
        for i in 1..24 {
            studio.new_tab();
            studio.doc = fixture(&format!("Inactive composition {i}"), 320);
            studio.active_layer = Some(1);
            inactive.push(omadesign::project::encode(&studio.doc).unwrap());
        }
        studio.switch_tab(0);
    }
    studio.need_fit = true;
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1440.0, 900.0])
            .with_min_inner_size([1440.0, 900.0])
            .with_max_inner_size([1440.0, 900.0])
            .with_resizable(false)
            .with_title(format!("Omadesign interaction QA · {scenario}")),
        renderer: eframe::Renderer::Wgpu,
        ..Default::default()
    };
    eframe::run_native(
        "omadesign-interaction-qa",
        options,
        Box::new(move |cc| {
            omadesign::ui::theme::apply(&cc.egui_ctx);
            cc.egui_ctx.set_pixels_per_point(1.0);
            Ok(Box::new(Qa {
                studio,
                output,
                scenario,
                target,
                initial,
                inactive,
                actions: workload(),
                events: vec![],
                phase: "startup",
                samples: vec![],
                start: Instant::now(),
                input_start: Instant::now(),
                previous_ui: Instant::now(),
                frames: 0,
                checked_moves: 0,
                saved: false,
                screenshot_requested: false,
                done: false,
            }))
        }),
    )
}
