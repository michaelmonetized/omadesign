//! Record the real native editor replay, plus a full-frame animated ending.
//! Usage: cargo run --release --bin capture_logo -- OUTPUT_DIRECTORY
use eframe::egui::{self, Event, Modifiers, ViewportBuilder};
use omadesign::{
    app::Studio,
    compositor::{self, Draft, View},
    shots::logo_wordmark::{self, Cue, LogoDemo},
};
use std::{
    io::Write,
    path::{Path, PathBuf},
    process::{Child, ChildStdin, Command, Stdio},
};

const FPS: u32 = 30;
const SIZE: [u32; 2] = [1600, 900];

struct Encoder {
    child: Child,
    pipe: ChildStdin,
}
impl Encoder {
    fn new(path: &Path, size: [u32; 2]) -> Self {
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
                &format!("{}x{}", size[0], size[1]),
                "-framerate",
                "30",
                "-i",
                "pipe:0",
                "-an",
                "-c:v",
                "libx264",
                "-preset",
                "fast",
                "-crf",
                "17",
                "-pix_fmt",
                "yuv420p",
                "-movflags",
                "+faststart",
            ])
            .arg(path)
            .stdin(Stdio::piped())
            .spawn()
            .expect("ffmpeg must be installed to record the demo");
        let pipe = child.stdin.take().expect("encoder input");
        Self { child, pipe }
    }
    fn finish(mut self) {
        drop(self.pipe);
        assert!(self.child.wait().expect("encoder completion").success());
    }
}

struct Capture {
    studio: Studio,
    replay: LogoDemo,
    cue: Cue,
    directory: PathBuf,
    frames: u32,
    warm: u32,
    pending: bool,
    stepped: bool,
    encoder: Option<Encoder>,
}
impl Capture {
    fn new(directory: PathBuf) -> Self {
        let mut studio = Studio::new();
        let mut replay = logo_wordmark::seed(&mut studio);
        let cue = replay.step(&mut studio, 0.0);
        Self {
            studio,
            replay,
            cue,
            directory,
            frames: 0,
            warm: 0,
            pending: false,
            stepped: false,
            encoder: None,
        }
    }
}
impl eframe::App for Capture {
    fn raw_input_hook(&mut self, _: &egui::Context, input: &mut egui::RawInput) {
        // Isolate this reproducible editor replay from the user's real desktop input.
        input
            .events
            .retain(|e| matches!(e, Event::Screenshot { .. }));
        input.hovered_files.clear();
        input.dropped_files.clear();
        input.focused = true;
        if self.warm >= 28 && !self.pending && !self.stepped {
            self.cue = self
                .replay
                .step(&mut self.studio, self.frames as f32 / FPS as f32);
            self.stepped = true;
        }
        input.events.push(Event::ModifiersChanged(Modifiers {
            shift: self.cue.shift,
            ..Modifiers::NONE
        }));
        input.events.push(Event::PointerGone);
    }

    fn ui(&mut self, ui: &mut egui::Ui, _: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.studio.play_clock = ctx.input(|i| i.time);
        self.studio.last_input = std::time::Instant::now();
        self.studio.status = self.cue.caption.into();
        omadesign::ui::run(ui, &mut self.studio);
        if let (Some(point), Some(rect)) = (self.cue.cursor, self.studio.canvas_rect) {
            let local = self.studio.view.to_screen(point);
            let center = rect.min + egui::vec2(local.x, local.y);
            let painter = ctx
                .layer_painter(egui::LayerId::new(
                    egui::Order::Foreground,
                    egui::Id::new("replay-pointer"),
                ))
                .with_clip_rect(rect);
            painter.circle_stroke(center, 6.0, egui::Stroke::new(1.5, egui::Color32::WHITE));
            painter.circle_filled(center, 1.8, egui::Color32::WHITE);
        }
        self.warm += 1;
        if self.warm == 2 {
            ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(
                SIZE[0] as f32,
                SIZE[1] as f32,
            )));
        }
        let screenshot = ctx.input(|i| {
            i.raw.events.iter().find_map(|event| {
                if let Event::Screenshot { image, .. } = event {
                    Some(image.clone())
                } else {
                    None
                }
            })
        });
        if let Some(image) = screenshot {
            assert_eq!(image.size, [SIZE[0] as usize, SIZE[1] as usize]);
            let pixels: Vec<_> = image.pixels.iter().flat_map(|p| p.to_array()).collect();
            let encoder = self
                .encoder
                .get_or_insert_with(|| Encoder::new(&self.directory.join("03-logo.mp4"), SIZE));
            encoder.pipe.write_all(&pixels).expect("write frame");
            if self.frames % FPS == FPS / 2 {
                image::save_buffer(
                    self.directory.join(format!("logo-{:03}.png", self.frames)),
                    &pixels,
                    SIZE[0],
                    SIZE[1],
                    image::ColorType::Rgba8,
                )
                .expect("save review frame");
            }
            self.frames += 1;
            self.pending = false;
            self.stepped = false;
            if self.frames >= (logo_wordmark::DURATION * FPS as f32).round() as u32 {
                self.encoder.take().unwrap().finish();
                eprintln!("Captured {} native editor frames", self.frames);
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                return;
            }
        }
        if self.stepped && !self.pending && omadesign::ui::scene_ready(&ctx, &self.studio) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default()));
            self.pending = true;
        }
        ctx.request_repaint();
    }
}

fn ending(directory: &Path) {
    let mut studio = Studio::new();
    let mut replay = logo_wordmark::seed(&mut studio);
    replay.step(&mut studio, logo_wordmark::DURATION);
    let mut encoder = Encoder::new(&directory.join("logo-ending.mp4"), [1920, 1080]);
    let mut last = Vec::new();
    for frame in 0..FPS * 5 {
        if frame <= FPS * 3 {
            let time = frame as f32 / FPS as f32;
            let pixmap = compositor::render_view_posed(
                &studio.doc,
                View::default(),
                1920,
                1080,
                Draft::none(),
                Some(time),
                None,
            )
            .expect("render the finished logo");
            last = pixmap
                .pixels()
                .iter()
                .flat_map(|p| {
                    let c = p.demultiply();
                    [c.red(), c.green(), c.blue(), c.alpha()]
                })
                .collect();
            if frame == FPS * 3 {
                image::save_buffer(
                    directory.join("logo-final.png"),
                    &last,
                    1920,
                    1080,
                    image::ColorType::Rgba8,
                )
                .expect("save final logo");
            }
        }
        encoder.pipe.write_all(&last).expect("write ending frame");
    }
    encoder.finish();
    eprintln!("Rendered the five-second logo ending");
}

fn main() -> eframe::Result {
    let directory = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .expect("usage: capture_logo OUTPUT_DIRECTORY");
    std::fs::create_dir_all(&directory).expect("create output directory");
    let options = eframe::NativeOptions {
        viewport: ViewportBuilder::default()
            .with_inner_size([SIZE[0] as f32, SIZE[1] as f32])
            .with_min_inner_size([SIZE[0] as f32, SIZE[1] as f32])
            .with_max_inner_size([SIZE[0] as f32, SIZE[1] as f32])
            .with_resizable(false)
            .with_title("omadesign — logo construction replay"),
        renderer: eframe::Renderer::Wgpu,
        ..Default::default()
    };
    let output = directory.clone();
    eframe::run_native(
        "omadesign-logo-capture",
        options,
        Box::new(move |cc| {
            omadesign::ui::theme::apply(&cc.egui_ctx);
            cc.egui_ctx.set_pixels_per_point(1.0);
            Ok(Box::new(Capture::new(output)))
        }),
    )?;
    ending(&directory);
    Ok(())
}
