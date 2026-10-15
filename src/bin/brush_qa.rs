//! Native WGPU qualification for pack import, painting, and library previews.
use eframe::egui::{self, Event, Modifiers, PointerButton};
use omadesign::{
    app::Studio,
    document::{Document, Layer},
    geom::Pt,
    tools::{Persona, Tool},
};
use std::{path::PathBuf, time::Instant};
struct Qa {
    studio: Studio,
    output: PathBuf,
    frame: usize,
    events: Vec<Event>,
    painted: Option<Vec<u8>>,
    started: Instant,
    screenshot: bool,
    canvas: bool,
    before: String,
}

fn canvas_point(step: usize) -> Pt {
    let corners = [
        Pt::new(-70., -60.),
        Pt::new(-70., 240.),
        Pt::new(430., 240.),
        Pt::new(430., -60.),
        Pt::new(-70., -60.),
    ];
    let segment = (step / 5).min(3);
    let t = (step - segment * 5) as f32 / 5.;
    corners[segment] + (corners[segment + 1] - corners[segment]) * t
}
impl eframe::App for Qa {
    fn raw_input_hook(&mut self, _: &egui::Context, input: &mut egui::RawInput) {
        input
            .events
            .retain(|e| matches!(e, Event::Screenshot { .. }));
        input.events.append(&mut self.events);
        input.focused = true;
    }
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        eframe::App::ui(&mut self.studio, ui, frame);
        assert!(
            self.started.elapsed().as_secs() < 60,
            "Native brush QA timed out"
        );
        if self.studio.brush_import_job.is_some() {
            ctx.request_repaint();
            return;
        }
        self.frame += 1;
        let position = |s: &Studio, point: Pt| {
            let rect = s.canvas_rect.unwrap();
            let p = s.view.to_screen(point);
            egui::pos2(rect.min.x + p.x, rect.min.y + p.y)
        };
        if self.frame == 10 {
            self.studio.select_brush(
                if self.canvas { 0 } else { 1 },
                if self.canvas { 6 } else { 0 },
            );
            self.studio.brush.size = 38.;
            let pos = position(
                &self.studio,
                if self.canvas {
                    canvas_point(0)
                } else {
                    Pt::new(70., 90.)
                },
            );
            self.events.push(Event::PointerMoved(pos));
            self.events.push(Event::PointerButton {
                pos,
                button: PointerButton::Primary,
                pressed: true,
                modifiers: Modifiers::NONE,
            });
        }
        if (11..=30).contains(&self.frame) {
            let pos = position(
                &self.studio,
                if self.canvas {
                    canvas_point(self.frame - 10)
                } else {
                    Pt::new(70. + (self.frame - 10) as f32 * 8., 90.)
                },
            );
            self.events.push(Event::PointerMoved(pos));
        }
        if self.frame == 31 {
            let pos = position(
                &self.studio,
                if self.canvas {
                    canvas_point(20)
                } else {
                    Pt::new(230., 90.)
                },
            );
            self.events.push(Event::PointerButton {
                pos,
                button: PointerButton::Primary,
                pressed: false,
                modifiers: Modifiers::NONE,
            });
        }
        if self.frame == 35 {
            let pixels = self.studio.doc.layers[0]
                .kind
                .pixels()
                .unwrap()
                .data
                .clone();
            assert!(
                pixels.chunks_exact(4).any(|p| p[3] > 0),
                "Native pointer painted pixels"
            );
            let after = omadesign::project::encode(&self.studio.doc).unwrap();
            if self.canvas {
                assert_eq!(self.studio.history.len(), 1);
                for point in [
                    Pt::new(-70., 90.),
                    Pt::new(180., 240.),
                    Pt::new(430., 90.),
                    Pt::new(180., -60.),
                ] {
                    let p = self.studio.mask_point(0, point);
                    let grid = self.studio.doc.layers[0].kind.pixels().unwrap();
                    assert!(p.x >= 0. && p.y >= 0. && p.x < grid.w as f32 && p.y < grid.h as f32);
                    let offset =
                        (p.y.floor() as usize * grid.w as usize + p.x.floor() as usize) * 4;
                    assert!(
                        grid.data[offset + 3] > 0,
                        "paint outside original bounds: {point:?}"
                    );
                }
            }
            self.studio.undo();
            assert_eq!(
                omadesign::project::encode(&self.studio.doc).unwrap(),
                self.before
            );
            assert!(
                self.studio.doc.layers[0]
                    .kind
                    .pixels()
                    .unwrap()
                    .data
                    .chunks_exact(4)
                    .all(|p| p[3] == 0)
            );
            self.studio.redo();
            assert_eq!(omadesign::project::encode(&self.studio.doc).unwrap(), after);
            assert_eq!(
                self.studio.doc.layers[0].kind.pixels().unwrap().data.as_slice(),
                pixels
            );
            let path = self.output.join("paint.oma");
            omadesign::project::save_to(&self.studio.doc, &path).unwrap();
            assert_eq!(
                omadesign::project::encode(&omadesign::project::load_from(&path).unwrap()).unwrap(),
                after
            );
            assert_eq!(
                omadesign::project::load_from(&path).unwrap().layers[0]
                    .kind
                    .pixels()
                    .unwrap()
                    .data.as_slice(),
                pixels
            );
            self.painted = Some(pixels);
            if !self.canvas {
                omadesign::ui::show_brush_library(&ctx);
            }
        }
        if self.frame == 42 {
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(Default::default()));
            self.screenshot = true;
        }
        if self.screenshot {
            if let Some(image) = ctx.input(|i| {
                i.events.iter().find_map(|e| {
                    if let Event::Screenshot { image, .. } = e {
                        Some(image.clone())
                    } else {
                        None
                    }
                })
            }) {
                let bytes: Vec<_> = image.pixels.iter().flat_map(|p| p.to_array()).collect();
                image::save_buffer(
                    self.output.join(if self.canvas {
                        "native-canvas.png"
                    } else {
                        "native-library.png"
                    }),
                    &bytes,
                    image.size[0] as u32,
                    image.size[1] as u32,
                    image::ColorType::Rgba8,
                )
                .unwrap();
                let library = omadesign::brushes::Library::load(
                    omadesign::project::data_dir().join("brushes"),
                );
                assert_eq!(
                    library.packs.len(),
                    if self.canvas { 2 } else { 3 },
                    "Brush packs persist"
                );
                let grid = self.studio.doc.layers[0].kind.pixels().unwrap();
                let report = serde_json::json!({"renderer":"native WGPU","input":"egui pointer down, 20 drag samples, pointer up","import":if self.canvas { "built-in Square" } else { "external JaxGrass ABR through editor importer" },"canvas_bounds":self.canvas,"original_grid":[320,180],"painted_grid":[grid.w,grid.h],"packs_after_reload":library.packs.len(),"undo_redo_exact":true,"save_reopen_exact":true,"frames":self.frame,"seconds":self.started.elapsed().as_secs_f32(),"painted_pixels":self.painted.as_ref().unwrap().chunks_exact(4).filter(|p|p[3]>0).count()});
                std::fs::write(
                    self.output.join("native-result.json"),
                    serde_json::to_vec_pretty(&report).unwrap(),
                )
                .unwrap();
                self.studio.allow_close = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                eprintln!("{report}");
            }
        }
        ctx.request_repaint();
    }
}
fn main() -> eframe::Result {
    let args: Vec<_> = std::env::args().collect();
    assert_eq!(args.len(), 3, "brush_qa INPUT.abr|--canvas OUTPUT");
    let canvas = args[1] == "--canvas";
    let output = PathBuf::from(&args[2]);
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
    let mut studio = Studio::new();
    studio.doc = Document::new("Brush packs", 320., 180., 72.);
    studio.doc.transparent = true;
    studio.doc.layers = vec![Layer::raster("Paint", 320, 180).unwrap()];
    studio.active_layer = Some(0);
    studio.show_welcome = false;
    studio.persona = Persona::Pixel;
    studio.tool = Tool::Brush;
    studio.need_fit = true;
    studio.startup_preferences.check_updates = false;
    if canvas {
        studio.need_fit = false;
        studio.show_rulers = false;
        studio.view = omadesign::compositor::View {
            scale: 1.5,
            offset: Pt::new(180., 180.),
        };
    } else {
        studio.import_brush_pack(std::path::Path::new(&args[1]));
    }
    let before = omadesign::project::encode(&studio.doc).unwrap();
    eframe::run_native(
        "Omadesign brush QA",
        eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default()
                .with_inner_size([1440., 900.])
                .with_title("Omadesign · brush pack QA"),
            renderer: eframe::Renderer::Wgpu,
            ..Default::default()
        },
        Box::new(move |cc| {
            omadesign::ui::theme::apply(&cc.egui_ctx);
            Ok(Box::new(Qa {
                studio,
                output,
                frame: 0,
                events: vec![],
                painted: None,
                started: Instant::now(),
                screenshot: false,
                canvas,
                before,
            }))
        }),
    )
}
