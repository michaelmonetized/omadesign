//! Exercise the real ACP adapter and MCP tools against an isolated live editor.
use eframe::egui;
use omadesign::{agent, app::Studio, document::Document};
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};

struct Capture {
    studio: Studio,
    directory: PathBuf,
    started: Instant,
    frames: u64,
    sent: bool,
    last_edits: usize,
    screenshots: usize,
    finished: bool,
    finished_shots: usize,
}
impl Capture {
    fn start(directory: PathBuf, resume: Option<PathBuf>) -> Self {
        let mut studio = Studio::new();
        studio.show_welcome = false;
        studio.doc = Document::new("Live ACP design", 960.0, 640.0, 96.0);
        studio.need_fit = true;
        studio.path = Some(directory.join("live-design.oma"));
        studio.agent.load();
        studio.agent.visible = true;
        studio.agent.settings.directory = directory.clone();
        studio.agent.sync_fields();
        studio.agent.request="Create a polished poster for a fictional botanical studio called FERN & FORM. Dark forest background, warm ivory editable title, sage and mint geometric botanical forms, one small warm orange accent, generous spacing. Use 8 to 12 native objects in this 960 by 640 canvas. Use only the supplied omadesign tools, one design tool at a time, and inspect a canvas snapshot. Do not use terminal, filesystem or subagent tools. Keep narration short.".into();
        if let Some(root) = resume {
            let thread: agent::workspace::Thread =
                serde_json::from_slice(&std::fs::read(root.join("conversation.json")).unwrap())
                    .unwrap();
            studio.doc = omadesign::project::decode(
                &std::fs::read_to_string(root.join("live-design.oma")).unwrap(),
            )
            .unwrap();
            studio.path = thread.document.clone();
            let mut harness = std::mem::take(&mut studio.agent);
            harness.restore(thread, &studio).unwrap();
            harness.request = "Change only the main title from FERN & FORM to FERN & FIELD using the native update tool. Preserve all other artwork. Inspect a snapshot. Do not use terminal, filesystem or subagent tools.".into();
            studio.agent = harness;
        }
        Self {
            studio,
            directory,
            started: Instant::now(),
            frames: 0,
            sent: false,
            last_edits: 0,
            screenshots: 0,
            finished: false,
            finished_shots: 0,
        }
    }
    fn report(&mut self) -> Result<(), String> {
        omadesign::project::save_to(&self.studio.doc, &self.directory.join("live-design.oma"))?;
        std::fs::write(
            self.directory.join("live-design.png"),
            omadesign::compositor::export_png(&self.studio.doc, 1)?,
        )
        .map_err(|e| e.to_string())?;
        if let Some(thread) = &self.studio.agent.thread {
            std::fs::write(
                self.directory.join("conversation.json"),
                serde_json::to_vec_pretty(thread).unwrap(),
            )
            .map_err(|e| e.to_string())?;
        }
        let result = serde_json::json!({"edits":self.studio.agent.edits,"objects":self.studio.doc.layers.iter().filter_map(|l|l.kind.shapes()).map(|s|s.len()).sum::<usize>(),"error":self.studio.agent.error,"status":self.studio.agent.status,"screenshots":self.screenshots,"elapsed_seconds":self.started.elapsed().as_secs()});
        std::fs::write(
            self.directory.join("result.json"),
            serde_json::to_vec_pretty(&result).unwrap(),
        )
        .map_err(|e| e.to_string())?;
        eprintln!("{result}");
        self.studio.agent.persist(true);
        Ok(())
    }
}
impl eframe::App for Capture {
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        if let Some(image) = ctx.input(|i| {
            i.events.iter().find_map(|e| {
                if let egui::Event::Screenshot { image, .. } = e {
                    Some(image.clone())
                } else {
                    None
                }
            })
        }) {
            let bytes: Vec<u8> = image.pixels.iter().flat_map(|p| p.to_array()).collect();
            let _ = image::save_buffer(
                self.directory
                    .join(format!("native-{:03}.png", self.screenshots)),
                &bytes,
                image.size[0] as u32,
                image.size[1] as u32,
                image::ColorType::Rgba8,
            );
            self.screenshots += 1;
        }
        eframe::App::ui(&mut self.studio, ui, frame);
        self.frames += 1;
        if self.frames == 20 {
            let mut agent = std::mem::take(&mut self.studio.agent);
            if let Err(e) =
                agent.send_prompt(&mut self.studio, &ctx, std::env::current_exe().unwrap())
            {
                agent.error = e;
            }
            self.studio.agent = agent;
            self.sent = true;
        }
        if self.studio.agent.edits != self.last_edits {
            self.last_edits = self.studio.agent.edits;
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(Default::default()));
        }
        if !self.studio.agent.permissions.is_empty() {
            let _ = std::fs::write(
                self.directory.join("permission.json"),
                serde_json::to_vec_pretty(&self.studio.agent.permissions).unwrap(),
            );
        }
        if !self.finished
            && self.sent
            && ((self.studio.agent.ready
                && !self.studio.agent.busy
                && !self.studio.agent.connecting
                && self.studio.agent.edits > 0)
                || self.started.elapsed() > Duration::from_secs(240)
                || (!self.studio.agent.error.is_empty()
                    && !self.studio.agent.busy
                    && !self.studio.agent.connecting))
        {
            self.finished = true;
            self.finished_shots = self.screenshots;
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(Default::default()));
        }
        if self.finished && self.screenshots > self.finished_shots {
            self.report().unwrap();
            self.studio.allow_close = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        ctx.request_repaint_after(Duration::from_millis(33));
    }
}
fn main() -> eframe::Result {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if let Some(result) = agent::cli(&args) {
        if let Err(e) = result {
            eprintln!("{e}");
            std::process::exit(2);
        }
        return Ok(());
    }
    let directory = PathBuf::from(args.first().expect("agent_qa OUTPUT_DIRECTORY"));
    std::fs::create_dir_all(&directory).unwrap();
    let directory = directory.canonicalize().unwrap();
    if args.iter().any(|a| a == "--discover") {
        discover(&directory);
        return Ok(());
    }
    let resume = args
        .windows(2)
        .find(|w| w[0] == "--resume-from")
        .map(|w| PathBuf::from(&w[1]));
    for (key, name) in [
        ("XDG_CONFIG_HOME", "config"),
        ("XDG_DATA_HOME", "data"),
        ("XDG_STATE_HOME", "state"),
        ("XDG_CACHE_HOME", "cache"),
    ] {
        let path = directory.join(name);
        std::fs::create_dir_all(&path).unwrap();
        unsafe {
            std::env::set_var(key, path);
        }
    }
    eframe::run_native(
        "Omadesign — live ACP verification",
        eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default().with_inner_size([1440.0, 960.0]),
            renderer: eframe::Renderer::Wgpu,
            ..Default::default()
        },
        Box::new(move |cc| {
            omadesign::ui::theme::apply(&cc.egui_ctx);
            Ok(Box::new(Capture::start(directory, resume)))
        }),
    )
}

fn discover(directory: &std::path::Path) {
    use agent::{
        discovery::{Discovery, Status},
        runtime::{Command, Connection, Event},
    };
    let ctx = egui::Context::default();
    let mut discovery = Discovery::default();
    discovery.poll(&ctx, directory, true);
    let started = Instant::now();
    loop {
        discovery.poll(&ctx, directory, false);
        if !discovery
            .providers
            .iter()
            .any(|p| p.status == Status::Checking)
            || started.elapsed() > Duration::from_secs(30)
        {
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    let mut report = vec![];
    for provider in &discovery.providers {
        let mut changes = vec![];
        if provider.profile.name == "Codex" && provider.status == Status::Ready {
            let connection = Connection::start(
                provider.profile.clone(),
                directory.to_path_buf(),
                None,
                std::env::current_exe().unwrap(),
                ctx.clone(),
            )
            .unwrap();
            let mut metadata = serde_json::Value::Null;
            while let Ok(event) = connection.events.recv_timeout(Duration::from_secs(25)) {
                match event {
                    Event::Session { data, .. } => {
                        metadata = data;
                        break;
                    }
                    Event::Error(e) => panic!("{e}"),
                    _ => (),
                }
            }
            assert!(!metadata.is_null());
            for category in ["model", "thought_level"] {
                let Some(option) = agent::discovery::options(&metadata)
                    .into_iter()
                    .find(|o| o.category == category)
                else {
                    continue;
                };
                let choice = if category == "thought_level" {
                    option.choices.iter().find(|c| c.id != option.current)
                } else {
                    option.choices.iter().find(|c| c.id == option.current)
                }
                .unwrap_or(&option.choices[0]);
                let value = choice.id.clone();
                connection
                    .send(if option.legacy {
                        Command::Model(value.clone())
                    } else {
                        Command::Config {
                            id: option.id.clone(),
                            value: value.clone(),
                        }
                    })
                    .unwrap();
                let mut accepted = false;
                let started = Instant::now();
                while started.elapsed() < Duration::from_secs(15) {
                    match connection.events.recv_timeout(Duration::from_millis(100)) {
                        Ok(Event::Update(update)) => {
                            if update["configOptions"].is_array() {
                                metadata["configOptions"] = update["configOptions"].clone();
                            }
                            if update["currentModelId"].is_string() {
                                metadata["models"]["currentModelId"] =
                                    update["currentModelId"].clone();
                            }
                            accepted = agent::discovery::options(&metadata)
                                .iter()
                                .any(|o| o.id == option.id && o.current == value);
                            if accepted {
                                break;
                            }
                        }
                        Ok(Event::Error(e)) => panic!("Setting {}: {e}", option.id),
                        _ => (),
                    }
                }
                assert!(accepted, "{} was not accepted", option.id);
                changes.push(
                    serde_json::json!({"option":option.id,"value":value,"accepted":accepted}),
                );
            }
        }
        report.push(serde_json::json!({"provider":provider.profile.name,"status":provider.status.label(),"detail":provider.detail,"metadata":provider.metadata,"verified_changes":changes}));
    }
    std::fs::write(
        directory.join("discovery.json"),
        serde_json::to_vec_pretty(&report).unwrap(),
    )
    .unwrap();
    for provider in &discovery.providers {
        println!("{}: {}", provider.profile.name, provider.status.label());
    }
}
