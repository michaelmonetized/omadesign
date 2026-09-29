use eframe::egui::{self, Event, Modifiers, PointerButton, Rect};
use omadesign::{
    app::Studio,
    document::Document,
    tools::{Persona, Tool},
};
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};

struct Capture {
    studio: Studio,
    output: PathBuf,
    original: Vec<u8>,
    stage: u8,
    stage_frame: u64,
    frames: u64,
    started: Instant,
    labels: Vec<(String, Rect)>,
    events: Vec<Event>,
    pending_click: Option<(Option<String>, egui::Pos2)>,
    held_release: Option<egui::Pos2>,
    refined_radius: u32,
    pending_shot: Option<String>,
    shots: Vec<String>,
    ui_ms: Vec<f64>,
    inference_frames: u64,
    inference_started: Instant,
    inference_seconds: f64,
}
impl Capture {
    fn next(&mut self, stage: u8) {
        self.stage = stage;
        self.stage_frame = self.frames;
    }
    fn find(&self, label: &str) -> egui::Pos2 {
        self.labels
            .iter()
            .rev()
            .find(|(s, _)| s == label)
            .unwrap_or_else(|| {
                panic!(
                    "Missing native control {label}; labels: {:?}",
                    self.labels.iter().map(|(s, _)| s).collect::<Vec<_>>()
                )
            })
            .1
            .center()
    }
    fn click_at(&mut self, pos: egui::Pos2) {
        assert!(
            self.pending_click.is_none() && self.held_release.is_none() && self.events.is_empty()
        );
        // Let egui resolve hover and settle layout before the semantic click.
        self.events.push(Event::PointerMoved(pos));
        self.pending_click = Some((None, pos));
    }
    fn click(&mut self, label: &str) {
        self.click_at(self.find(label));
        self.pending_click.as_mut().unwrap().0 = Some(label.into());
    }
    fn radius_value(&self) -> u32 {
        let radius = self
            .labels
            .iter()
            .find(|(label, _)| label == "Radius")
            .expect("Radius label")
            .1;
        self.labels
            .iter()
            .filter(|(_, rect)| {
                rect.right() < radius.left() && (rect.center().y - radius.center().y).abs() < 1.
            })
            .filter_map(|(label, rect)| {
                label
                    .trim_end_matches(" px")
                    .trim()
                    .parse::<u32>()
                    .ok()
                    .map(|value| (value, rect.right()))
            })
            .max_by(|a, b| a.1.total_cmp(&b.1))
            .expect("Radius numeric value")
            .0
    }
    fn assert_modal(&self, ctx: &egui::Context, id: &str, controls: &[&str]) {
        assert_eq!(
            ctx.memory(|m| m.top_modal_layer()),
            Some(egui::LayerId::new(
                egui::Order::Foreground,
                egui::Id::new(id)
            )),
            "Expected modal {id} at stage {}",
            self.stage,
        );
        for control in controls {
            assert!(
                self.labels.iter().any(|(label, _)| label == control),
                "Missing modal control {control} at stage {}",
                self.stage
            );
        }
    }
    fn assert_no_modal(&self, ctx: &egui::Context) {
        assert!(
            ctx.memory(|m| m.top_modal_layer()).is_none(),
            "Previous dialog did not close at stage {}",
            self.stage
        );
    }
    fn cancel(&mut self) {
        // The footer moves when inference finishes. Exercise the actual Escape
        // cancellation path, without asserting coverage of the Cancel button.
        assert!(
            self.pending_click.is_none() && self.held_release.is_none() && self.events.is_empty()
        );
        for pressed in [true, false] {
            self.events.push(Event::Key {
                key: egui::Key::Escape,
                physical_key: None,
                pressed,
                repeat: false,
                modifiers: Modifiers::NONE,
            });
        }
    }
    fn key(&mut self, key: egui::Key, shift: bool) {
        let modifiers = Modifiers {
            ctrl: true,
            command: true,
            shift,
            ..Modifiers::NONE
        };
        self.events.extend([
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
        ]);
    }
    fn shot(&mut self, ctx: &egui::Context, name: &str) {
        self.pending_shot = Some(name.into());
        ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(Default::default()));
    }
    fn read_labels(&mut self, ctx: &egui::Context) {
        fn visit(shape: &egui::Shape, clip: Rect, out: &mut Vec<(String, Rect)>) {
            match shape {
                egui::Shape::Text(t) => {
                    let r = t.galley.rect.translate(t.pos.to_vec2());
                    if clip.contains(r.center()) {
                        out.push((t.galley.job.text.clone(), r));
                    }
                }
                egui::Shape::Vec(shapes) => {
                    for s in shapes {
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
    fn step(&mut self, ctx: &egui::Context) {
        if self.pending_shot.is_some()
            || self.pending_click.is_some()
            || self.held_release.is_some()
            || self.frames - self.stage_frame < 8
        {
            return;
        }
        match self.stage {
            1 | 2 | 4..=10 => self.assert_modal(
                ctx,
                "remove-background-modal",
                &["Refine matte", "Radius", "Apply mask"],
            ),
            0 | 3 | 11..=16 => self.assert_no_modal(ctx),
            _ => {}
        }
        let ready = omadesign::ui::scene_ready(ctx, &self.studio);
        match self.stage {
            0 if self.frames > 28 && ready => {
                self.click("Remove Background…");
                self.next(1);
            }
            1 => {
                assert!(self.studio.doc.layers[0].mask.is_none());
                self.shot(ctx, "01-working");
                self.next(2);
            }
            2 => {
                self.cancel();
                self.next(3);
            }
            3 => {
                assert!(self.studio.doc.layers[0].mask.is_none());
                assert_eq!(self.studio.history.len(), 0);
                self.click("Remove Background…");
                self.inference_started = Instant::now();
                self.inference_frames = self.frames;
                self.next(4);
            }
            4 if ready => {
                self.inference_frames = self.frames - self.inference_frames;
                self.inference_seconds = self.inference_started.elapsed().as_secs_f64();
                self.shot(ctx, "02-preview");
                self.next(5);
            }
            5 => {
                self.click("Mask");
                self.next(6);
            }
            6 => {
                self.shot(ctx, "03-matte");
                self.next(7);
            }
            7 => {
                assert_eq!(self.radius_value(), 12, "initial matte radius");
                let rect = self
                    .labels
                    .iter()
                    .find(|(s, _)| s == "Radius")
                    .expect("Radius slider")
                    .1;
                self.click_at(egui::pos2(rect.left() - 85., rect.center().y));
                self.next(8);
            }
            8 if ready => {
                self.refined_radius = self.radius_value();
                assert!(
                    self.refined_radius > 12,
                    "Radius slider must change the matte setting"
                );
                self.click("Preview");
                self.next(9);
            }
            9 if ready => {
                self.shot(ctx, "04-refined");
                self.next(10);
            }
            10 if ready => {
                self.click("Apply mask");
                self.next(11);
            }
            11 if ready => {
                assert!(self.studio.doc.layers[0].mask.is_some());
                assert_eq!(self.studio.history.len(), 1);
                assert_eq!(
                    self.studio.doc.layers[0].kind.pixels().unwrap().data,
                    self.original
                );
                self.shot(ctx, "05-applied");
                self.next(12);
            }
            12 => {
                self.key(egui::Key::Z, false);
                self.next(13);
            }
            13 if ready => {
                assert!(self.studio.doc.layers[0].mask.is_none());
                self.shot(ctx, "06-undo");
                self.next(14);
            }
            14 => {
                self.key(egui::Key::Z, true);
                self.next(15);
            }
            15 if ready => {
                assert!(self.studio.doc.layers[0].mask.is_some());
                assert_eq!(
                    self.studio.doc.layers[0].kind.pixels().unwrap().data,
                    self.original
                );
                let path = self.output.join("native-cutout.oma");
                omadesign::project::save_to(&self.studio.doc, &path).unwrap();
                let reopened =
                    omadesign::project::decode(&std::fs::read_to_string(path).unwrap()).unwrap();
                assert_eq!(
                    reopened.layers[0].mask.as_ref().unwrap().data,
                    self.studio.doc.layers[0].mask.as_ref().unwrap().data
                );
                assert_eq!(
                    reopened.layers[0].kind.pixels().unwrap().data,
                    self.original
                );
                self.shot(ctx, "07-redo-saved");
                self.next(16);
            }
            16 => {
                self.studio.show_preferences = true;
                self.studio.settings_page = 0;
                self.next(17);
            }
            17 => {
                self.click("Credits");
                self.next(18);
            }
            18 => {
                self.shot(ctx, "08-credits");
                self.next(19);
            }
            19 => {
                self.ui_ms.sort_by(f64::total_cmp);
                let report = serde_json::json!({"renderer":"native WGPU","qa_input_protocol":"hover-atomic-buttons-held-sliders-escape-cancel-v2","cancel_input":"Escape key","cancel_button_covered":false,"matte_radius_changed":true,"matte_radius":self.refined_radius,"interaction":"real egui pointer and keyboard events","frames":self.frames,"inference_frames":self.inference_frames,"inference_seconds":self.inference_seconds,"ui_frame_p95_ms":self.ui_ms[self.ui_ms.len()*95/100],"ui_frame_max_ms":self.ui_ms.last(),"cancel_preserves_document":true,"pixels_preserved":true,"one_undo_step":true,"undo_redo":true,"save_reopen":true,"screenshots":self.shots,"elapsed_seconds":self.started.elapsed().as_secs_f64()});
                std::fs::write(
                    self.output.join("native-result.json"),
                    serde_json::to_vec_pretty(&report).unwrap(),
                )
                .unwrap();
                eprintln!("{report}");
                self.studio.allow_close = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                self.next(20);
            }
            _ => {}
        }
    }
}
impl eframe::App for Capture {
    fn raw_input_hook(&mut self, _: &egui::Context, input: &mut egui::RawInput) {
        input
            .events
            .retain(|e| matches!(e, Event::Screenshot { .. }));
        input.hovered_files.clear();
        input.dropped_files.clear();
        input.focused = true;
        input.events.push(Event::ModifiersChanged(Modifiers::NONE));
        if self.events.is_empty()
            && let Some((label, pos)) = self.pending_click.take()
        {
            // Refresh semantic targets after the hover pass. Keep press/release
            // in one batch: inference completion can move the dialog footer.
            let pos = label.as_deref().map_or(pos, |label| self.find(label));
            input.events.push(Event::PointerMoved(pos));
            let buttons: &[bool] = if label.is_some() {
                &[true, false]
            } else {
                // Slider interaction requires a held pointer across a UI pass.
                self.held_release = Some(pos);
                &[true]
            };
            for &pressed in buttons {
                input.events.push(Event::PointerButton {
                    pos,
                    button: PointerButton::Primary,
                    pressed,
                    modifiers: Modifiers::NONE,
                });
            }
        } else if self.events.is_empty()
            && let Some(pos) = self.held_release.take()
        {
            input.events.push(Event::PointerButton {
                pos,
                button: PointerButton::Primary,
                pressed: false,
                modifiers: Modifiers::NONE,
            });
        }
        input.events.append(&mut self.events);
    }
    fn ui(&mut self, ui: &mut egui::Ui, _: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        let started = Instant::now();
        omadesign::ui::run(ui, &mut self.studio);
        self.ui_ms.push(started.elapsed().as_secs_f64() * 1000.);
        self.read_labels(&ctx);
        self.frames += 1;
        if self.frames == 2 {
            ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(1440., 900.)));
        }
        if let Some(image) = ctx.input(|i| {
            i.events.iter().find_map(|e| {
                if let Event::Screenshot { image, .. } = e {
                    Some(image.clone())
                } else {
                    None
                }
            })
        }) {
            let name = self.pending_shot.take().expect("pending screenshot");
            let bytes: Vec<_> = image.pixels.iter().flat_map(|p| p.to_array()).collect();
            image::save_buffer(
                self.output.join(format!("{name}.png")),
                &bytes,
                image.size[0] as u32,
                image.size[1] as u32,
                image::ColorType::Rgba8,
            )
            .unwrap();
            self.shots.push(name);
        }
        self.step(&ctx);
        assert!(
            self.started.elapsed() < Duration::from_secs(600),
            "Native QA timed out at stage {}",
            self.stage
        );
        ctx.request_repaint_after(Duration::from_millis(16));
    }
}
pub fn run(input: PathBuf, output: PathBuf) -> eframe::Result {
    // Isolate capture state without replacing the user's desktop HOME/theme.
    let profile = output.join("profile");
    for (key, name) in [
        ("XDG_CONFIG_HOME", "config"),
        ("XDG_DATA_HOME", "data"),
        ("XDG_CACHE_HOME", "cache"),
        ("XDG_STATE_HOME", "state"),
    ] {
        let path = std::env::current_dir().unwrap().join(profile.join(name));
        std::fs::create_dir_all(&path).unwrap();
        unsafe {
            std::env::set_var(key, path);
        }
    }
    let img = image::open(input).unwrap().to_rgba8();
    let (w, h) = img.dimensions();
    let original = img.into_raw();
    let mut studio = Studio::new();
    studio.doc = Document::new("Offline cutout QA", w as f32, h as f32, 96.);
    studio.doc.transparent = true;
    let mut layer = omadesign::document::Layer::raster("Cat portrait", w, h);
    layer.kind.pixels_mut().unwrap().data = original.clone();
    studio.doc.layers = vec![layer];
    studio.active_layer = Some(0);
    studio.persona = Persona::Pixel;
    studio.tool = Tool::Select;
    studio.show_welcome = false;
    studio.need_fit = true;
    studio.startup_preferences.check_updates = false;
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1440., 900.])
            .with_min_inner_size([1440., 900.])
            .with_max_inner_size([1440., 900.])
            .with_resizable(false)
            .with_title("Omadesign · background removal QA"),
        renderer: eframe::Renderer::Wgpu,
        ..Default::default()
    };
    eframe::run_native(
        "omadesign-removal-qa",
        options,
        Box::new(move |cc| {
            omadesign::ui::theme::apply(&cc.egui_ctx);
            cc.egui_ctx.set_pixels_per_point(1.);
            Ok(Box::new(Capture {
                studio,
                output,
                original,
                stage: 0,
                stage_frame: 0,
                frames: 0,
                started: Instant::now(),
                labels: vec![],
                events: vec![],
                pending_click: None,
                held_release: None,
                refined_radius: 0,
                pending_shot: None,
                shots: vec![],
                ui_ms: vec![],
                inference_frames: 0,
                inference_started: Instant::now(),
                inference_seconds: 0.,
            }))
        }),
    )
}
