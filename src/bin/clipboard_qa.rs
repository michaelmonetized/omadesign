//! Observe real native clipboard input without injecting egui events.
//! Usage: clipboard_qa OUTPUT_DIRECTORY [INPUT.oma] [--view SCALE X Y] [--layout]
//! Drive the titled window with the desktop's actual keyboard/clipboard tools.
use eframe::egui::{self, ViewportBuilder};
use omadesign::{
    app::Studio,
    document::LayerKind,
    geom::{Bounds, Geom, Pt},
};
use serde_json::{Value, json};
use std::{
    fs,
    path::PathBuf,
    time::{Duration, Instant},
};

struct ClipboardQa {
    studio: Studio,
    output: PathBuf,
    revision: u64,
    previous: String,
    checked: Instant,
    started: Instant,
    native_events: Vec<Value>,
}

fn bounds(bounds: Bounds) -> Value {
    json!({
        "min": [bounds.min.x, bounds.min.y],
        "max": [bounds.max.x, bounds.max.y],
        "center": [bounds.center().x, bounds.center().y],
    })
}

impl ClipboardQa {
    fn state(&self) -> Value {
        let history_undo = {
            let mut history = self.studio.history.clone();
            std::iter::from_fn(|| history.undo()).count()
        };
        let history_redo = {
            let mut history = self.studio.history.clone();
            std::iter::from_fn(|| history.redo()).count()
        };
        let canvas = self.studio.canvas_rect.map(|rect| {
            let center = self
                .studio
                .view
                .to_world(Pt::new(rect.width() / 2.0, rect.height() / 2.0));
            json!({
                "min": [rect.min.x, rect.min.y], "max": [rect.max.x, rect.max.y],
                "world_center": [center.x, center.y],
            })
        });
        let layers: Vec<_> = self.studio.doc.layers.iter().map(|layer| {
            let content = match &layer.kind {
                LayerKind::Vector { shapes } => json!({
                    "kind": "vector",
                    "shapes": shapes.iter().map(|shape| json!({
                        "id": shape.id, "name": shape.name,
                        "kind": shape.geom.kind_name(), "bounds": bounds(shape.world_bbox()),
                        "text": match &shape.geom { Geom::Text(text) => Some(&text.content), _ => None },
                    })).collect::<Vec<_>>(),
                }),
                LayerKind::Raster { pixels, .. } => json!({
                    "kind": "raster", "pixels": [pixels.w, pixels.h],
                    "pixel_version": pixels.version,
                    "bounds": layer.kind.raster_bounds().map(bounds),
                }),
            };
            json!({
                "id": layer.id, "name": layer.name, "parent": layer.parent,
                "group": layer.is_group, "visible": layer.visible, "content": content,
            })
        }).collect();
        json!({
            "version": env!("CARGO_PKG_VERSION"),
            "pid": std::process::id(), "status": self.studio.status,
            "canvas": canvas, "scale": self.studio.view.scale,
            "offset": [self.studio.view.offset.x, self.studio.view.offset.y],
            "artboards": self.studio.doc.artboards,
            "selected_artboards": self.studio.artboard_sel,
            "selected_shapes": self.studio.selection,
            "active_layer": self.studio.active_layer,
            "selected_layer": self.studio.selected_layer,
            "text_editing": self.studio.type_edit.is_some(),
            "can_undo": self.studio.history.can_undo(),
            "can_redo": self.studio.history.can_redo(),
            "history_undo": history_undo, "history_redo": history_redo,
            "layers": layers, "native_events": self.native_events,
        })
    }

    fn record(&mut self) -> Result<(), String> {
        let mut state = self.state();
        let signature = serde_json::to_string(&state).map_err(|error| error.to_string())?;
        if signature == self.previous {
            return Ok(());
        }
        self.revision += 1;
        state["revision"] = json!(self.revision);
        state["elapsed_ms"] = json!(self.started.elapsed().as_millis());
        let state = serde_json::to_vec_pretty(&state).map_err(|error| error.to_string())?;
        let document = omadesign::project::encode(&self.studio.doc)?;
        let name = format!("{:06}", self.revision);
        for (file, bytes) in [
            ("state.json", state.as_slice()),
            ("document.oma", document.as_bytes()),
        ] {
            fs::write(self.output.join(format!(".{file}.tmp")), bytes)
                .map_err(|error| error.to_string())?;
            fs::rename(
                self.output.join(format!(".{file}.tmp")),
                self.output.join(file),
            )
            .map_err(|error| error.to_string())?;
        }
        fs::write(
            self.output.join("revisions").join(format!("{name}.json")),
            state,
        )
        .map_err(|error| error.to_string())?;
        fs::write(
            self.output.join("revisions").join(format!("{name}.oma")),
            document,
        )
        .map_err(|error| error.to_string())?;
        self.previous = signature;
        Ok(())
    }
}

impl eframe::App for ClipboardQa {
    fn raw_input_hook(&mut self, _: &egui::Context, input: &mut egui::RawInput) {
        // Observe, never synthesize or remove native events. Avoid recording
        // arbitrary clipboard text: fixture contents remain in the saved document.
        for event in &input.events {
            let observed = match event {
                egui::Event::Key {
                    key,
                    pressed: true,
                    modifiers,
                    ..
                } => Some(json!({
                    "key": format!("{key:?}"), "ctrl": modifiers.ctrl,
                    "command": modifiers.command, "shift": modifiers.shift,
                })),
                egui::Event::Paste(text) => Some(json!({"paste_bytes": text.len()})),
                egui::Event::Copy => Some(json!({"copy": true})),
                egui::Event::Cut => Some(json!({"cut": true})),
                _ => None,
            };
            if let Some(observed) = observed {
                self.native_events.push(observed);
                if self.native_events.len() > 32 {
                    self.native_events.remove(0);
                }
            }
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        eframe::App::ui(&mut self.studio, ui, frame);
        if self.checked.elapsed() >= Duration::from_millis(200) {
            self.checked = Instant::now();
            if let Err(error) = self.record() {
                eprintln!("clipboard QA snapshot: {error}");
            }
        }
        if self.started.elapsed() > Duration::from_secs(900) {
            self.studio.allow_close = true;
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
        }
        ui.ctx().request_repaint_after(Duration::from_millis(100));
    }
}

fn main() -> eframe::Result {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let output =
        PathBuf::from(args.first().expect(
            "usage: clipboard_qa OUTPUT_DIRECTORY [INPUT.oma] [--view SCALE X Y] [--layout]",
        ));
    fs::create_dir_all(output.join("revisions")).expect("create QA output");
    let mut studio = Studio::new();
    if let Some(input) = args.get(1).filter(|arg| !arg.starts_with('-')) {
        studio.doc = omadesign::project::decode(&fs::read_to_string(input).expect("read QA input"))
            .expect("decode QA input");
        studio.active_layer = studio.doc.layers.len().checked_sub(1);
    }
    studio.show_welcome = false;
    studio.allow_close = true;
    studio.path = Some(output.join("manual-save.oma"));
    if args.iter().any(|arg| arg == "--layout") {
        studio.persona = omadesign::tools::Persona::Layout;
    }
    if let Some(index) = args.iter().position(|arg| arg == "--view") {
        let value = |offset: usize| {
            args.get(index + offset)
                .expect("--view SCALE X Y")
                .parse::<f32>()
                .expect("numeric view argument")
        };
        studio.view.scale = value(1);
        assert!(studio.view.scale.is_finite() && studio.view.scale > 0.0);
        studio.view.offset = Pt::new(value(2), value(3));
        studio.need_fit = false;
    }
    let options = eframe::NativeOptions {
        viewport: ViewportBuilder::default()
            .with_title("Omadesign Clipboard QA")
            .with_inner_size([1280.0, 850.0]),
        renderer: eframe::Renderer::Wgpu,
        ..Default::default()
    };
    eframe::run_native(
        "omadesign-clipboard-qa",
        options,
        Box::new(move |cc| {
            omadesign::ui::theme::apply(&cc.egui_ctx);
            cc.egui_ctx.set_pixels_per_point(1.0);
            Ok(Box::new(ClipboardQa {
                studio,
                output,
                revision: 0,
                previous: String::new(),
                checked: Instant::now(),
                started: Instant::now(),
                native_events: vec![],
            }))
        }),
    )
}
