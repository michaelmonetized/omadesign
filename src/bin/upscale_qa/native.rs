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
    release: Option<egui::Pos2>,
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
        self.events.extend([
            Event::PointerMoved(pos),
            Event::PointerButton {
                pos,
                button: PointerButton::Primary,
                pressed: true,
                modifiers: Modifiers::NONE,
            },
        ]);
        self.release = Some(pos);
    }
    fn click(&mut self, label: &str) {
        self.click_at(self.find(label));
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
            || self.release.is_some()
            || self.frames - self.stage_frame < 8
        {
            return;
        }
        let ready = omadesign::ui::scene_ready(ctx, &self.studio);
        match self.stage {
            0 if self.frames > 28 && ready => {
                self.click("Remove Background…");
                self.next(1);
            }
            1 => {
                self.click("Original resolution");
                self.next(2);
            }
            2 => {
                self.click("Upscale ×2 before removal");
                self.inference_started = Instant::now();
                self.inference_frames = self.frames;
                self.next(3);
            }
            3 if ready => {
                self.shot(ctx, "01-upscale-first-preview");
                self.next(4);
            }
            4 => {
                self.click("Apply mask");
                self.next(5);
            }
            5 if ready => {
                self.inference_frames = self.frames - self.inference_frames;
                self.inference_seconds = self.inference_started.elapsed().as_secs_f64();
                assert_eq!(self.studio.history.len(), 1);
                assert_eq!(
                    self.studio.doc.layers[0].kind.pixels().unwrap().data,
                    self.original
                );
                assert!(self.studio.doc.layers[0].mask.is_some());
                self.key(egui::Key::Z, false);
                self.next(6);
            }
            6 => {
                assert!(self.studio.doc.layers[0].mask.is_none());
                self.key(egui::Key::Z, true);
                self.next(7);
            }
            7 => {
                assert!(self.studio.doc.layers[0].mask.is_some());
                self.click("Remove Background…");
                self.next(8);
            }
            8 => {
                self.click("Upscale cutout");
                self.next(9);
            }
            9 => {
                self.click("Cancel");
                self.next(10);
            }
            10 => {
                assert_eq!(self.studio.history.len(), 1);
                assert_eq!(
                    self.studio.doc.layers[0].kind.pixels().unwrap().data,
                    self.original
                );
                self.click("Remove Background…");
                self.next(11);
            }
            11 => {
                self.click("Upscale cutout");
                self.next(12);
            }
            12 if ready => {
                self.shot(ctx, "02-upscale-cutout-preview");
                self.next(13);
            }
            13 => {
                self.click("Apply upscaled cutout");
                self.next(14);
            }
            14 if ready => {
                assert_eq!(self.studio.history.len(), 2);
                let p = self.studio.doc.layers[0].kind.pixels().unwrap();
                assert_eq!(p.data.len(), self.original.len() * 4);
                let (_, size, _) = self.studio.doc.layers[0].kind.raster_xform().unwrap();
                assert_eq!(
                    (size.x, size.y),
                    (self.studio.doc.width, self.studio.doc.height)
                );
                let m = self.studio.doc.layers[0].mask.as_ref().unwrap();
                assert_eq!((p.w, p.h), (m.w, m.h));
                self.key(egui::Key::Z, false);
                self.next(15);
            }
            15 => {
                assert_eq!(
                    self.studio.doc.layers[0].kind.pixels().unwrap().data,
                    self.original
                );
                self.key(egui::Key::Z, true);
                self.next(16);
            }
            16 if ready => {
                let path = self.output.join("upscaled-cutout.oma");
                omadesign::project::save_to(&self.studio.doc, &path).unwrap();
                let reopened =
                    omadesign::project::decode(&std::fs::read_to_string(path).unwrap()).unwrap();
                assert_eq!(
                    reopened.layers[0].kind.pixels().unwrap().data,
                    self.studio.doc.layers[0].kind.pixels().unwrap().data
                );
                assert_eq!(
                    reopened.layers[0].mask.as_ref().unwrap().data,
                    self.studio.doc.layers[0].mask.as_ref().unwrap().data
                );
                self.key(egui::Key::E, false);
                self.next(17);
            }
            17 => {
                self.click("AI upscale");
                self.next(18);
            }
            18 => {
                self.click("×4");
                self.next(19);
            }
            19 => {
                self.shot(ctx, "03-export-dialog");
                self.next(20);
            }
            20 => {
                omadesign::ui::save_export_preview(
                    ctx,
                    &self.studio,
                    self.output.join("cancelled-native.png"),
                )
                .unwrap();
                self.next(80);
            }
            80 => {
                self.click("Cancel");
                self.next(81);
            }
            81 => {
                assert!(!self.output.join("cancelled-native.png").exists());
                self.key(egui::Key::E, false);
                self.next(82);
            }
            82 => {
                self.click("AI upscale");
                self.next(83);
            }
            83 => {
                self.click("×4");
                self.next(84);
            }
            84 => {
                omadesign::ui::save_export_preview(
                    ctx,
                    &self.studio,
                    self.output.join("document-4x.png"),
                )
                .unwrap();
                self.next(21);
            }
            21 if self.output.join("document-4x.png").exists() && ready => {
                let img = image::open(self.output.join("document-4x.png")).unwrap();
                assert_eq!(img.width(), (self.studio.doc.width * 4.) as u32);
                assert_eq!(img.height(), (self.studio.doc.height * 4.) as u32);
                assert!(img.to_rgba8().pixels().any(|p| p[3] < 255));
                self.studio.persona = Persona::Photo;
                let (w, h) = (self.studio.doc.width as u32, self.studio.doc.height as u32);
                self.studio.photo.import_image(
                    "Original photo".into(),
                    omadesign::photo::RgbaImage {
                        w,
                        h,
                        data: self.original.clone(),
                    },
                );
                self.next(22);
            }
            22 if ready => {
                self.click("AI upscale photo…");
                self.next(23);
            }
            23 => {
                self.shot(ctx, "04-photo-upscale-dialog");
                self.next(24);
            }
            24 => {
                omadesign::ui::save_export_preview(
                    ctx,
                    &self.studio,
                    self.output.join("photo-copy.png"),
                )
                .unwrap();
                self.next(25);
            }
            25 if self.studio.photo.images.len() == 2 && ready => {
                assert_eq!(self.studio.photo.images[0].full.data, self.original);
                assert_eq!(
                    self.studio.photo.images[1].full.data.len(),
                    self.original.len() * 4
                );
                assert_eq!(
                    self.studio.photo.images[1]
                        .source
                        .as_ref()
                        .unwrap()
                        .file_name()
                        .unwrap(),
                    "photo-copy.png"
                );
                self.shot(ctx, "05-photo-copy-opened");
                self.next(26);
            }
            26 => {
                self.studio.photo.select_image(0);
                self.next(90);
            }
            90 if ready => {
                self.click("AI upscale photo…");
                self.next(91);
            }
            91 => {
                self.click("2.0");
                self.next(92);
            }
            92 => {
                self.key(egui::Key::A, false);
                self.events.push(Event::Text("2.5".into()));
                self.events.push(Event::Key {
                    key: egui::Key::Enter,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: Modifiers::NONE,
                });
                self.next(93);
            }
            93 => {
                assert!(self.labels.iter().any(|(s, _)| s == "Output: 800 × 530 px"));
                self.shot(ctx, "06-photo-custom-scale");
                self.next(94);
            }
            94 => {
                omadesign::ui::save_export_preview(
                    ctx,
                    &self.studio,
                    self.output.join("photo-custom.png"),
                )
                .unwrap();
                self.next(95);
            }
            95 if self.studio.photo.images.len() == 3 && ready => {
                assert_eq!(self.studio.photo.images[2].dimensions(), (800, 530));
                assert_eq!(self.studio.photo.images[0].full.data, self.original);
                self.studio.show_preferences = true;
                self.next(27);
            }
            27 => {
                self.click("Credits");
                self.next(28);
            }
            28 => {
                self.shot(ctx, "07-credits");
                self.next(29);
            }
            29 => {
                self.ui_ms.sort_by(f64::total_cmp);
                assert!(!self.output.join("cancelled-native.png").exists());
                let report = serde_json::json!({"renderer":"native WGPU", "interaction":"real egui pointer and keyboard events; save destinations supplied through the native host callback", "frames":self.frames,"inference_frames":self.inference_frames,"inference_seconds":self.inference_seconds,"ui_frame_p95_ms":self.ui_ms[self.ui_ms.len()*95/100],"ui_frame_max_ms":self.ui_ms.last(),"cancel_preserves_document":true, "cancel_export_no_partial":true,"upscale_first":true,"upscale_cutout":true,"one_undo_step_each":true,"undo_redo":true,"save_reopen":true,"document_export_4x":true,"photo_copy_2x_persisted_and_opened":true,"photo_custom_2_5x":true,"screenshots":self.shots,"elapsed_seconds":self.started.elapsed().as_secs_f64()});
                std::fs::write(
                    self.output.join("native-result.json"),
                    serde_json::to_vec_pretty(&report).unwrap(),
                )
                .unwrap();
                eprintln!("{report}");
                self.studio.allow_close = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                self.next(30);
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
            && let Some(pos) = self.release.take()
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
    studio.doc = Document::new("AI upscaling QA", w as f32, h as f32, 96.);
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
            .with_title("Omadesign · AI upscaling QA"),
        renderer: eframe::Renderer::Wgpu,
        ..Default::default()
    };
    eframe::run_native(
        "omadesign-upscale-qa",
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
                release: None,
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
