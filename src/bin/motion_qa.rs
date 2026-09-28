//! Native motion playback on copied real documents; never overrides egui time.
//! Usage: motion_qa document.oma output-directory [--width 1440 --height 900]
//!        [--background-tabs 23 --background-document other.oma]
//!        [--frames 240 --seconds 300 --warmup-frames 30 --warmup-seconds 2]
use eframe::egui::{self, Event, Modifiers};
use omadesign::{app::Studio, document::Document, tools::Persona};
use sha2::{Digest, Sha256};
use std::{
    io::Read,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

struct Options {
    input: PathBuf,
    output: PathBuf,
    width: u32,
    height: u32,
    background_tabs: usize,
    backgrounds: Vec<PathBuf>,
    frames: usize,
    seconds: f64,
    warmup_frames: usize,
    warmup_seconds: f64,
    timeout: f64,
}

impl Options {
    fn parse() -> Self {
        let mut args = std::env::args().skip(1);
        let usage = "motion_qa document.oma output-directory [--width 1440 --height 900] [--background-tabs 23 --background-document other.oma] [--frames 240 --seconds 300 --warmup-frames 30 --warmup-seconds 2 --timeout 360]";
        let input = args.next().expect(usage);
        if input == "--help" || input == "-h" {
            println!("{usage}");
            std::process::exit(0);
        }
        let mut options = Self {
            input: PathBuf::from(input).canonicalize().expect("input document"),
            output: std::env::current_dir()
                .unwrap()
                .join(args.next().expect(usage)),
            width: 1440,
            height: 900,
            background_tabs: 0,
            backgrounds: vec![],
            frames: 240,
            seconds: 300.0,
            warmup_frames: 30,
            warmup_seconds: 2.0,
            timeout: 360.0,
        };
        while let Some(flag) = args.next() {
            let value = args.next().expect("each option needs a value");
            match flag.as_str() {
                "--width" => options.width = value.parse().expect("integer width"),
                "--height" => options.height = value.parse().expect("integer height"),
                "--background-tabs" => options.background_tabs = value.parse().expect("tab count"),
                "--background-document" => options.backgrounds.push(
                    PathBuf::from(value)
                        .canonicalize()
                        .expect("background document"),
                ),
                "--frames" => options.frames = value.parse().expect("frame count"),
                "--seconds" => options.seconds = value.parse().expect("measurement seconds"),
                "--warmup-frames" => options.warmup_frames = value.parse().expect("warmup frames"),
                "--warmup-seconds" => {
                    options.warmup_seconds = value.parse().expect("warmup seconds")
                }
                "--timeout" => options.timeout = value.parse().expect("timeout seconds"),
                _ => panic!("unknown option {flag}: {usage}"),
            }
        }
        assert!(
            options.width >= 640 && options.height >= 480,
            "window too small"
        );
        assert!(
            options.frames >= 2,
            "at least two playback frames are required"
        );
        assert!(options.seconds.is_finite() && options.seconds > 0.0);
        assert!(options.warmup_seconds.is_finite() && options.warmup_seconds >= 0.0);
        assert!(options.timeout.is_finite() && options.timeout > options.warmup_seconds);
        assert!(
            options.background_tabs <= 128,
            "background tab limit is 128"
        );
        assert!(
            !options.output.exists() || options.output.read_dir().unwrap().next().is_none(),
            "use a new or empty output directory to preserve evidence"
        );
        options
    }
}

fn file_hash(path: &Path) -> String {
    let mut file = std::fs::File::open(path).unwrap();
    let mut digest = Sha256::new();
    let mut buffer = [0_u8; 65536];
    loop {
        let count = file.read(&mut buffer).unwrap();
        if count == 0 {
            break;
        }
        digest.update(&buffer[..count]);
    }
    format!("{:x}", digest.finalize())
}

fn document_hash(document: &Document) -> String {
    format!(
        "{:x}",
        Sha256::digest(omadesign::project::encode(document).unwrap().as_bytes())
    )
}

#[derive(serde::Serialize)]
struct FontInput {
    reference: String,
    loaded_from: PathBuf,
    snapshot: PathBuf,
    bytes: u64,
    sha256: String,
}

#[derive(serde::Serialize)]
struct Input {
    source: PathBuf,
    copy: PathBuf,
    bytes: u64,
    sha256: String,
    fonts: Vec<FontInput>,
}

fn font_references(value: &serde_json::Value, references: &mut std::collections::BTreeSet<String>) {
    match value {
        serde_json::Value::Object(fields) => {
            for (key, value) in fields {
                if key == "font"
                    && let Some(reference) = value.as_str()
                {
                    references.insert(reference.to_owned());
                } else {
                    font_references(value, references);
                }
            }
        }
        serde_json::Value::Array(values) => {
            for value in values {
                font_references(value, references);
            }
        }
        _ => {}
    }
}

fn copy_input(source: &Path, destination: &Path) -> Input {
    std::fs::copy(source, destination).unwrap();
    let sha256 = file_hash(destination);
    assert_eq!(file_hash(source), sha256, "source changed while copying");
    let directory = destination.parent().unwrap();
    let json = serde_json::from_slice(&std::fs::read(destination).unwrap()).unwrap();
    let mut references = std::collections::BTreeSet::new();
    font_references(&json, &mut references);
    if references
        .iter()
        .any(|reference| reference.starts_with("omatype:"))
    {
        omadesign::typography::copy_document_fonts(source, directory).unwrap();
    }
    let mut fonts = vec![];
    for reference in references {
        let path = if let Some(fingerprint) = reference.strip_prefix("omatype:") {
            let bank = directory.join(".omabrand/fonts");
            ["ttf", "otf"]
                .into_iter()
                .map(|extension| bank.join(format!("{fingerprint}.{extension}")))
                .find(|path| path.is_file())
                .expect("copied project font")
        } else if reference.is_empty() {
            omadesign::text::default_path().expect("default font")
        } else {
            PathBuf::from(&reference)
        };
        let loaded_from = path.canonicalize().unwrap_or_else(|error| {
            panic!(
                "font {reference:?} must be present; fallback would change the workload: {error}"
            )
        });
        let bytes = std::fs::read(&loaded_from).unwrap();
        assert!(
            rustybuzz::Face::from_slice(&bytes, 0).is_some()
                && ab_glyph::FontRef::try_from_slice(&bytes).is_ok(),
            "font {reference:?} must load without fallback"
        );
        let font_hash = format!("{:x}", Sha256::digest(&bytes));
        let snapshot_directory = directory.join("font-files");
        std::fs::create_dir_all(&snapshot_directory).unwrap();
        let extension = loaded_from
            .extension()
            .unwrap_or_default()
            .to_string_lossy();
        let snapshot = snapshot_directory.join(format!("{font_hash}.{extension}"));
        if !snapshot.exists() {
            std::fs::write(&snapshot, &bytes).unwrap();
        }
        assert_eq!(file_hash(&snapshot), font_hash);
        fonts.push(FontInput {
            reference,
            loaded_from,
            snapshot,
            bytes: bytes.len() as u64,
            sha256: font_hash,
        });
    }
    Input {
        source: source.to_owned(),
        copy: destination.to_owned(),
        bytes: destination.metadata().unwrap().len(),
        sha256,
        fonts,
    }
}

#[derive(serde::Serialize)]
struct Sample {
    phase: &'static str,
    input_sequence: usize,
    egui_time: f64,
    ui_ms: f64,
    input_to_ui_ms: f64,
    input_interval_ms: f64,
    frame_interval_ms: f64,
    playhead_before: f32,
    playhead_after: f32,
    playing_after: bool,
    canvas_changed: bool,
}

#[derive(serde::Serialize)]
struct NativeInputSample {
    input_sequence: usize,
    ui_calls: usize,
    ui_ms: f64,
    input_to_ui_ms: f64,
    input_interval_ms: f64,
    ui_completion_interval_ms: f64,
}

struct Qa {
    studio: Studio,
    options: Options,
    inputs: Vec<Input>,
    documents: Vec<String>,
    samples: Vec<Sample>,
    phase: &'static str,
    start: Instant,
    input_start: Instant,
    previous_ui: Instant,
    input_interval_ms: f64,
    input_sequence: usize,
    warmup_frames: usize,
    playback_start: Option<Instant>,
    playback_frames: usize,
    playback_seconds: f64,
    stop_reason: &'static str,
    screenshot_start: Option<Instant>,
    done: bool,
}

fn statistics(mut values: Vec<f64>) -> serde_json::Value {
    if values.is_empty() {
        return serde_json::Value::Null;
    }
    values.sort_by(f64::total_cmp);
    let count = values.len();
    let median = if count % 2 == 0 {
        (values[count / 2 - 1] + values[count / 2]) / 2.0
    } else {
        values[count / 2]
    };
    serde_json::json!({
        "mean_ms": values.iter().sum::<f64>() / count as f64,
        "median_ms": median,
        "p95_ms": values[(count * 95).div_ceil(100) - 1],
        "max_ms": values[count - 1],
    })
}

impl Qa {
    fn finish(&mut self, ctx: &egui::Context, image: &egui::ColorImage) {
        let playback: Vec<_> = self
            .samples
            .iter()
            .filter(|sample| sample.phase == "playback")
            .collect();
        assert!(
            playback.len() >= 2,
            "playback ended before two measured frames"
        );
        let advanced = playback
            .iter()
            .filter(|sample| sample.playhead_before != sample.playhead_after)
            .count();
        let changed = playback
            .iter()
            .filter(|sample| sample.canvas_changed)
            .count();
        assert!(advanced > 0, "the real motion playhead never advanced");
        assert!(changed > 0, "the native motion canvas never updated");
        let duration = self.studio.doc.motion.duration.max(0.05);
        for pair in self.samples.windows(2) {
            let sample = &pair[1];
            if sample.phase != "playback" {
                continue;
            }
            let delta = (sample.egui_time - pair[0].egui_time).max(0.0) as f32;
            let mut expected = sample.playhead_before + delta;
            if expected > duration {
                if self.studio.doc.motion.looped {
                    expected %= duration;
                } else {
                    expected = duration;
                }
            }
            assert_eq!(
                sample.playhead_after, expected,
                "playback did not follow the normal elapsed-time clock"
            );
        }
        let mut input_samples: Vec<NativeInputSample> = vec![];
        for sample in &playback {
            if let Some(input) = input_samples.last_mut()
                && input.input_sequence == sample.input_sequence
            {
                input.ui_calls += 1;
                input.ui_ms += sample.ui_ms;
                input.input_to_ui_ms = sample.input_to_ui_ms;
                input.ui_completion_interval_ms += sample.frame_interval_ms;
            } else {
                input_samples.push(NativeInputSample {
                    input_sequence: sample.input_sequence,
                    ui_calls: 1,
                    ui_ms: sample.ui_ms,
                    input_to_ui_ms: sample.input_to_ui_ms,
                    input_interval_ms: sample.input_interval_ms,
                    ui_completion_interval_ms: sample.frame_interval_ms,
                });
            }
        }
        let input_frames = input_samples.len();
        let summary = serde_json::json!({
            "ui_calls": playback.len(),
            "native_input_callbacks": input_frames,
            "ui": statistics(playback.iter().map(|sample| sample.ui_ms).collect()),
            "input_to_ui": statistics(playback.iter().map(|sample| sample.input_to_ui_ms).collect()),
            "ui_call_interval": statistics(playback.iter().map(|sample| sample.frame_interval_ms).collect()),
            "per_native_input": {
                "ui": statistics(input_samples.iter().map(|sample| sample.ui_ms).collect()),
                "input_to_ui": statistics(input_samples.iter().map(|sample| sample.input_to_ui_ms).collect()),
                "input_interval": statistics(input_samples.iter().map(|sample| sample.input_interval_ms).collect()),
                "ui_completion_interval": statistics(input_samples.iter().map(|sample| sample.ui_completion_interval_ms).collect()),
            },
        });
        let last_playhead = self.studio.playhead;
        let canvas = self.studio.canvas_rect.unwrap();
        let motion = self.studio.doc.motion.clone();
        let shape_count: usize = self
            .studio
            .doc
            .layers
            .iter()
            .filter_map(|layer| layer.kind.shapes())
            .map(|shapes| shapes.len())
            .sum();
        assert_eq!(self.studio.tab_count(), self.documents.len());
        // Encoding and tab checks happen after all timed playback and the screenshot.
        for (index, expected) in self.documents.iter().enumerate() {
            self.studio.switch_tab(index);
            assert_eq!(
                &document_hash(&self.studio.doc),
                expected,
                "document {index} changed during playback"
            );
        }
        for input in &self.inputs {
            assert_eq!(file_hash(&input.copy), input.sha256, "copied input changed");
            assert_eq!(
                file_hash(&input.source),
                input.sha256,
                "original input changed"
            );
            for font in &input.fonts {
                assert_eq!(
                    file_hash(&font.loaded_from),
                    font.sha256,
                    "loaded font changed"
                );
                assert_eq!(
                    file_hash(&font.snapshot),
                    font.sha256,
                    "font snapshot changed"
                );
            }
        }
        let bytes: Vec<_> = image
            .pixels
            .iter()
            .flat_map(|pixel| pixel.to_array())
            .collect();
        image::save_buffer(
            self.options.output.join("native-editor.png"),
            &bytes,
            image.size[0] as u32,
            image.size[1] as u32,
            image::ColorType::Rgba8,
        )
        .unwrap();
        let report = serde_json::json!({
            "renderer": "native WGPU",
            "input": "Unmodified native egui timestamps; playing=true once; Studio::ui calls the normal tick_motion path",
            "timing": "Each sample is one egui UI call; multiple calls can share a native input callback. UI and input-to-UI are CPU wall durations. frame_interval_ms measures successive UI-call completion times; input_interval_ms measures successive raw-input callbacks. Neither callback rate is presented FPS or isolated GPU latency. Static preview warmup, document verification and screenshot encoding are excluded from playback samples.",
            "inputs": self.inputs,
            "requested_window": [self.options.width, self.options.height],
            "screenshot_size": image.size,
            "canvas_size": [canvas.width(), canvas.height()],
            "open_documents": self.documents.len(),
            "active_shapes": shape_count,
            "motion": {"duration": motion.duration, "fps": motion.fps, "looped": motion.looped, "tracks": motion.tracks.len()},
            "requested_ui_calls": self.options.frames,
            "measurement_limit_seconds": self.options.seconds,
            "warmup_ui_calls": self.warmup_frames,
            "warmup_min_seconds": self.options.warmup_seconds,
            "playback_seconds": self.playback_seconds,
            "observed_ui_calls_per_second": self.playback_frames as f64 / self.playback_seconds,
            "observed_native_input_callbacks_per_second": input_frames as f64 / self.playback_seconds,
            "stop_reason": self.stop_reason,
            "last_playhead": last_playhead,
            "checks": {"playhead_advanced_ui_calls": advanced, "canvas_render_key_updated_ui_calls": changed, "normal_elapsed_time_playback": true, "documents_unchanged": true, "original_and_copied_files_unchanged": true, "font_files_and_snapshots_unchanged": true, "native_screenshot_received": true},
            "playback": summary,
            "samples": self.samples,
            "native_input_samples": input_samples,
        });
        std::fs::write(
            self.options.output.join("motion-result.json"),
            serde_json::to_vec_pretty(&report).unwrap(),
        )
        .unwrap();
        eprintln!("MOTION COMPLETE {}", report["playback"]);
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
        let now = Instant::now();
        self.input_interval_ms = now.duration_since(self.input_start).as_secs_f64() * 1000.0;
        self.input_start = now;
        self.input_sequence += 1;
    }

    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        let key = self.studio.canvas_key;
        let playhead_before = self.studio.playhead;
        let start = Instant::now();
        eframe::App::ui(&mut self.studio, ui, frame);
        let now = Instant::now();
        if self.done {
            return;
        }
        self.samples.push(Sample {
            phase: self.phase,
            input_sequence: self.input_sequence,
            egui_time: ctx.input(|input| input.time),
            ui_ms: now.duration_since(start).as_secs_f64() * 1000.0,
            input_to_ui_ms: now.duration_since(self.input_start).as_secs_f64() * 1000.0,
            input_interval_ms: self.input_interval_ms,
            frame_interval_ms: now.duration_since(self.previous_ui).as_secs_f64() * 1000.0,
            playhead_before,
            playhead_after: self.studio.playhead,
            playing_after: self.studio.playing,
            canvas_changed: key != self.studio.canvas_key,
        });
        self.previous_ui = now;
        if self.phase == "warmup" {
            self.warmup_frames += 1;
            if self.warmup_frames >= self.options.warmup_frames
                && self.start.elapsed().as_secs_f64() >= self.options.warmup_seconds
                && omadesign::ui::scene_ready(&ctx, &self.studio)
            {
                // The preceding real UI frame initialized play_clock. Do not
                // alter the clock, document FPS, looping or native input time.
                self.studio.playing = true;
                self.phase = "playback";
                self.playback_start = Some(Instant::now());
            }
            ctx.request_repaint_after(Duration::from_millis(16));
        } else if self.phase == "playback" {
            self.playback_frames += 1;
            self.playback_seconds = self.playback_start.unwrap().elapsed().as_secs_f64();
            let reason = if !self.studio.playing {
                Some("natural_playback_end")
            } else if self.playback_frames >= self.options.frames {
                Some("requested_frames")
            } else if self.playback_seconds >= self.options.seconds {
                Some("measurement_time_limit")
            } else {
                None
            };
            if let Some(reason) = reason {
                self.stop_reason = reason;
                self.studio.playing = false;
                self.phase = "screenshot";
                self.screenshot_start = Some(Instant::now());
                ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(Default::default()));
                ctx.request_repaint();
            }
        } else if self.phase == "screenshot" {
            if let Some(image) = ctx.input(|input| {
                input.events.iter().find_map(|event| match event {
                    Event::Screenshot { image, .. } => Some(image.clone()),
                    _ => None,
                })
            }) {
                self.finish(&ctx, &image);
            }
            assert!(
                self.screenshot_start.unwrap().elapsed() < Duration::from_secs(30),
                "native screenshot timed out"
            );
            ctx.request_repaint_after(Duration::from_millis(16));
        }
        assert!(
            self.start.elapsed().as_secs_f64() < self.options.timeout,
            "native motion workload exceeded --timeout"
        );
    }
}

fn main() -> eframe::Result {
    let options = Options::parse();
    std::fs::create_dir_all(options.output.join("inputs")).unwrap();
    for (key, directory) in [
        ("XDG_CONFIG_HOME", "config"),
        ("XDG_DATA_HOME", "data"),
        ("XDG_CACHE_HOME", "cache"),
        ("XDG_STATE_HOME", "state"),
    ] {
        let path = options.output.join("profile").join(directory);
        std::fs::create_dir_all(&path).unwrap();
        unsafe {
            std::env::set_var(key, path);
        }
    }
    let active = copy_input(&options.input, &options.output.join("inputs/active.oma"));
    let mut studio = Studio::new();
    studio.doc = omadesign::project::load_from(&active.copy).unwrap();
    assert!(
        !studio.doc.motion.tracks.is_empty(),
        "input has no motion tracks"
    );
    studio.path = Some(active.copy.clone());
    studio.show_welcome = false;
    studio.startup_preferences.check_updates = false;
    studio.active_layer = studio.doc.layers.len().checked_sub(1);
    let mut documents = vec![document_hash(&studio.doc)];
    let mut inputs = vec![active];
    for index in 0..options.background_tabs {
        let source = if options.backgrounds.is_empty() {
            &options.input
        } else {
            &options.backgrounds[index % options.backgrounds.len()]
        };
        let input = copy_input(
            source,
            &options
                .output
                .join(format!("inputs/background-{index:02}.oma")),
        );
        studio.new_tab();
        studio.doc = omadesign::project::load_from(&input.copy).unwrap();
        studio.path = Some(input.copy.clone());
        studio.active_layer = studio.doc.layers.len().checked_sub(1);
        studio.show_welcome = false;
        documents.push(document_hash(&studio.doc));
        inputs.push(input);
    }
    studio.switch_tab(0);
    studio.persona = Persona::Motion;
    studio.playhead = 0.0;
    studio.playing = false;
    studio.need_fit = true;
    let size = [options.width as f32, options.height as f32];
    let native = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size(size)
            .with_min_inner_size(size)
            .with_max_inner_size(size)
            .with_resizable(false)
            .with_title("Omadesign · native motion QA"),
        renderer: eframe::Renderer::Wgpu,
        ..Default::default()
    };
    eframe::run_native(
        "omadesign-motion-qa",
        native,
        Box::new(move |cc| {
            assert!(
                cc.wgpu_render_state.is_some(),
                "native WGPU renderer is required"
            );
            omadesign::ui::theme::apply(&cc.egui_ctx);
            cc.egui_ctx.set_pixels_per_point(1.0);
            let now = Instant::now();
            Ok(Box::new(Qa {
                studio,
                options,
                inputs,
                documents,
                samples: vec![],
                phase: "warmup",
                start: now,
                input_start: now,
                previous_ui: now,
                input_interval_ms: 0.0,
                input_sequence: 0,
                warmup_frames: 0,
                playback_start: None,
                playback_frames: 0,
                playback_seconds: 0.0,
                stop_reason: "not_started",
                screenshot_start: None,
                done: false,
            }))
        }),
    )
}
