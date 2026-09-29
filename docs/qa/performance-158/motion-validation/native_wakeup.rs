//! Native regression for future and worker-thread repaint wakeups.
//! Run in the isolated regression Sway with an external timeout. There is no
//! watchdog repaint/thread, synthetic input, egui::Context::run, or clock change.
use eframe::egui;
use serde_json::{Value, json};
use std::{
    path::PathBuf,
    sync::{Arc, Mutex, atomic::{AtomicBool, Ordering}},
    time::{Duration, Instant},
};

const WARMUP_SECONDS: f64 = 1.0;
const TIMER_SECONDS: f64 = 0.7;
const WORKER_SECONDS: f64 = 0.7;
const MIN_IDLE_MS: f64 = 500.0;

#[derive(Clone, Copy, PartialEq)]
enum Stage { Warmup, Timer, Worker, Done }
impl Stage {
    fn name(self) -> &'static str {
        match self {
            Self::Warmup => "warmup", Self::Timer => "future_repaint",
            Self::Worker => "worker_repaint", Self::Done => "done",
        }
    }
}

struct Wakeup {
    output: PathBuf,
    started: Instant,
    stage: Stage,
    timer_due: f64,
    timer_idle_ms: f64,
    worker_idle_ms: f64,
    worker_sent: Arc<Mutex<Option<f64>>>,
    previous_input: Option<Instant>,
    input_sequence: usize,
    raw_callbacks: Vec<Value>,
    ui_callbacks: Vec<Value>,
    events: Vec<Value>,
    checks: Vec<Value>,
    adapter: Value,
    passed: Arc<AtomicBool>,
}

impl Wakeup {
    fn finish(&mut self, ctx: &egui::Context, failure: Option<&str>) {
        let passed = failure.is_none();
        let result = json!({
            "passed": passed, "failure": failure,
            "renderer": "native eframe WGPU", "adapter": self.adapter,
            "requested_window": [1440, 900],
            "observed_inner_rect": ctx.input(|input| input.viewport().inner_rect.map(|rect|
                [rect.min.x, rect.min.y, rect.width(), rect.height()])),
            "elapsed_seconds": self.started.elapsed().as_secs_f64(),
            "protocol": {"warmup_seconds":WARMUP_SECONDS,"future_delay_seconds":TIMER_SECONDS,
                "worker_delay_seconds":WORKER_SECONDS,"minimum_callback_idle_ms":MIN_IDLE_MS,
                "watchdog_repaints":false,"synthetic_input":false,"mutated_input_timestamps":false},
            "checks": self.checks, "events": self.events,
            "raw_input_callbacks": self.raw_callbacks, "ui_callbacks": self.ui_callbacks,
        });
        std::fs::write(self.output.join("wakeup-result.json"), serde_json::to_vec_pretty(&result).unwrap()).unwrap();
        self.passed.store(passed, Ordering::Release);
        self.stage = Stage::Done;
        eprintln!("WAKEUP_RESULT {}", json!({"passed":passed,"failure":failure,"checks":self.checks}));
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
    }
}

impl eframe::App for Wakeup {
    fn raw_input_hook(&mut self, _: &egui::Context, input: &mut egui::RawInput) {
        // Observe exactly what winit/egui supplied. Do not insert/remove events,
        // force focus, change timestamps or ask for repaint from this callback.
        let now = Instant::now();
        let gap_ms = self.previous_input.map_or(0.0, |previous| now.duration_since(previous).as_secs_f64()*1000.0);
        self.previous_input = Some(now);
        self.input_sequence += 1;
        match self.stage {
            Stage::Timer => self.timer_idle_ms = self.timer_idle_ms.max(gap_ms),
            Stage::Worker => self.worker_idle_ms = self.worker_idle_ms.max(gap_ms),
            _ => {},
        }
        self.raw_callbacks.push(json!({"sequence":self.input_sequence,
            "elapsed_seconds":now.duration_since(self.started).as_secs_f64(),
            "interval_ms":gap_ms,"phase":self.stage.name(),"egui_time":input.time,
            "events":input.events.iter().map(|event|format!("{event:?}")).collect::<Vec<_>>()}));
    }

    fn ui(&mut self, ui: &mut egui::Ui, _: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        let now = self.started.elapsed().as_secs_f64();
        self.ui_callbacks.push(json!({"elapsed_seconds":now,"input_sequence":self.input_sequence,
            "phase":self.stage.name(),"egui_time":ctx.input(|input|input.time)}));
        ui.label("Native repaint wakeup regression — waiting for real native callbacks.");
        match self.stage {
            Stage::Warmup => {
                if now < WARMUP_SECONDS {
                    ctx.request_repaint_after(Duration::from_secs_f64(WARMUP_SECONDS-now));
                } else {
                    self.stage = Stage::Timer;
                    self.timer_due = now + TIMER_SECONDS;
                    self.events.push(json!({"event":"request_repaint_after","elapsed_seconds":now,
                        "due_seconds":self.timer_due,"delay_seconds":TIMER_SECONDS}));
                    ctx.request_repaint_after(Duration::from_secs_f64(TIMER_SECONDS));
                }
            },
            Stage::Timer => {
                if now < self.timer_due {
                    // Early platform callbacks may occur while startup settles.
                    // Keep the original deadline, never introduce periodic ticks.
                    ctx.request_repaint_after(Duration::from_secs_f64(self.timer_due-now));
                    return;
                }
                if self.timer_idle_ms < MIN_IDLE_MS {
                    self.finish(&ctx, Some("future repaint did not follow a >=500ms native callback idle gap"));
                    return;
                }
                let external_events = self.raw_callbacks.iter().filter(|sample|
                    sample["phase"] == "future_repaint").any(|sample|
                    !sample["events"].as_array().unwrap().is_empty());
                if external_events {
                    self.finish(&ctx, Some("external input contaminated future-repaint phase"));
                    return;
                }
                self.checks.push(json!({"test":"future_repaint_after_idle","passed":true,
                    "due_seconds":self.timer_due,"callback_seconds":now,
                    "deadline_lateness_ms":(now-self.timer_due)*1000.0,
                    "maximum_native_callback_idle_ms":self.timer_idle_ms,"external_input_events":0}));
                self.stage = Stage::Worker;
                self.events.push(json!({"event":"spawn_worker","elapsed_seconds":now,
                    "sleep_seconds":WORKER_SECONDS}));
                let ctx = ctx.clone();
                let sent = self.worker_sent.clone();
                let started = self.started;
                std::thread::spawn(move || {
                    std::thread::sleep(Duration::from_secs_f64(WORKER_SECONDS));
                    let time = started.elapsed().as_secs_f64();
                    *sent.lock().unwrap() = Some(time);
                    ctx.request_repaint();
                });
                // No repaint request here or while Worker is pending. Only the
                // worker's explicit request can wake this otherwise idle app.
            },
            Stage::Worker => {
                let sent = *self.worker_sent.lock().unwrap();
                let Some(requested) = sent else { return; };
                let external_events = self.raw_callbacks.iter().filter(|sample|
                    sample["phase"] == "worker_repaint").any(|sample|
                    !sample["events"].as_array().unwrap().is_empty());
                if external_events {
                    self.finish(&ctx, Some("external input contaminated worker-repaint phase"));
                    return;
                }
                if self.worker_idle_ms < MIN_IDLE_MS {
                    self.finish(&ctx, Some("worker repaint did not follow a >=500ms native callback idle gap"));
                    return;
                }
                self.events.push(json!({"event":"worker_request_repaint","elapsed_seconds":requested}));
                self.checks.push(json!({"test":"worker_repaint_after_idle","passed":true,
                    "requested_seconds":requested,"callback_seconds":now,
                    "request_to_ui_ms":(now-requested)*1000.0,
                    "maximum_native_callback_idle_ms":self.worker_idle_ms,"external_input_events":0}));
                self.finish(&ctx, None);
            },
            Stage::Done => {},
        }
    }
}

fn main() -> eframe::Result {
    let output = PathBuf::from(std::env::args().nth(1).expect("native_wakeup NEW-output-directory"));
    assert!(!output.exists(), "preserve existing evidence; output must be new");
    std::fs::create_dir_all(&output).unwrap();
    let passed = Arc::new(AtomicBool::new(false));
    let app_passed = passed.clone();
    let native = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([1440.0,900.0])
            .with_min_inner_size([1440.0,900.0]).with_max_inner_size([1440.0,900.0])
            .with_resizable(false).with_title("Omadesign native wakeup regression"),
        renderer: eframe::Renderer::Wgpu,
        ..Default::default()
    };
    eframe::run_native("omadesign-native-wakeup", native, Box::new(move |cc| {
        let render = cc.wgpu_render_state.as_ref().expect("real native WGPU is required");
        let adapter = render.adapter.get_info();
        let adapter = json!({"name":adapter.name,"vendor":adapter.vendor,"device":adapter.device,
            "device_type":format!("{:?}",adapter.device_type),"backend":format!("{:?}",adapter.backend),
            "driver":adapter.driver,"driver_info":adapter.driver_info});
        cc.egui_ctx.set_pixels_per_point(1.0);
        Ok(Box::new(Wakeup {output,started:Instant::now(),stage:Stage::Warmup,timer_due:0.0,
            timer_idle_ms:0.0,worker_idle_ms:0.0,worker_sent:Arc::new(Mutex::new(None)),
            previous_input:None,input_sequence:0,raw_callbacks:vec![],ui_callbacks:vec![],
            events:vec![],checks:vec![],adapter,passed:app_passed}))
    }))?;
    if !passed.load(Ordering::Acquire) { std::process::exit(1); }
    Ok(())
}
