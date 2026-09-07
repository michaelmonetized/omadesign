use eframe::egui::{self, ViewportBuilder};
use omadesign::app::Studio;
use omadesign::shots;
use omadesign::ui::theme;
use std::path::PathBuf;

fn main() -> eframe::Result {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if let Some(result) = omadesign::formats::cli::run(&args) {
        if let Err(error) = result {
            eprintln!("{error}");
            std::process::exit(1);
        }
        return Ok(());
    }
    if args.iter().any(|arg| arg == "--version" || arg == "-V") {
        println!("omadesign {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }
    if args.iter().any(|a| a == "--export-demo") {
        return run_headless();
    }
    if args.iter().any(|a| a == "--list-shots") {
        for scene in shots::SCENES {
            println!("{}\t{}", scene.id, scene.caption);
        }
        return Ok(());
    }
    let shot_file = args
        .windows(2)
        .find(|w| w[0] == "--shot-file")
        .map(|w| PathBuf::from(&w[1]));
    let shot = args
        .windows(2)
        .find(|w| w[0] == "--shot")
        .map(|w| w[1].clone())
        .or_else(|| shot_file.as_ref().map(|_| "imported".to_string()));
    let out = args
        .windows(2)
        .find(|w| w[0] == "--out")
        .map(|w| PathBuf::from(&w[1]));

    let shot_size = args
        .iter()
        .position(|arg| arg == "--size")
        .map(|index| {
            let size = args.get(index + 1).and_then(|value| {
                let (width, height) = value.split_once('x')?;
                let width = width.parse::<u32>().ok()?;
                let height = height.parse::<u32>().ok()?;
                ((320..=8192).contains(&width) && (240..=8192).contains(&height))
                    .then_some(egui::vec2(width as f32, height as f32))
            });
            if shot.is_none() || size.is_none() {
                eprintln!("use --shot SCENE --size WIDTHxHEIGHT (320–8192 × 240–8192)");
                std::process::exit(2);
            }
            size.unwrap()
        })
        .unwrap_or_else(|| egui::vec2(1600.0, 1000.0));
    let shot_modifiers = if let Some(index) = args.iter().position(|arg| arg == "--shot-modifiers")
    {
        let mut modifiers = egui::Modifiers::NONE;
        let valid = args.get(index + 1).is_some_and(|value| {
            value.split('+').all(|part| match part {
                "ctrl" => {
                    modifiers.ctrl = true;
                    modifiers.command = true;
                    true
                }
                "shift" => {
                    modifiers.shift = true;
                    true
                }
                "alt" => {
                    modifiers.alt = true;
                    true
                }
                _ => false,
            })
        });
        if shot.is_none() || !valid {
            eprintln!("use --shot SCENE --shot-modifiers ctrl, shift, alt, or a + combination");
            std::process::exit(2);
        }
        modifiers
    } else {
        egui::Modifiers::NONE
    };

    let mut options = eframe::NativeOptions {
        viewport: ViewportBuilder::default()
            .with_inner_size([1600.0, 1000.0])
            .with_min_inner_size([960.0, 640.0])
            .with_title("omadesign"),
        renderer: eframe::Renderer::Wgpu,
        multisampling: 0,
        depth_buffer: 0,
        stencil_buffer: 0,
        ..Default::default()
    };

    if let Some(name) = shot {
        options.viewport = options
            .viewport
            .with_inner_size(shot_size)
            .with_min_inner_size(shot_size)
            .with_max_inner_size(shot_size)
            .with_resizable(false)
            .with_title(format!("omadesign — {name} preview"));
        let out = out.unwrap_or_else(|| PathBuf::from(format!("{name}.png")));
        return eframe::run_native(
            "omadesign",
            options,
            Box::new(move |cc| {
                theme::apply(&cc.egui_ctx);
                cc.egui_ctx.set_pixels_per_point(1.0);
                let mut studio = Studio::new();
                if let Some(path) = &shot_file {
                    if omadesign::import::classify(path) == "raw" {
                        match omadesign::photo::PhotoImage::load(path) {
                            Ok(photo) => studio.photo.import_photo(photo),
                            Err(error) => {
                                eprintln!("{error}");
                                std::process::exit(2);
                            }
                        }
                        studio.persona = omadesign::tools::Persona::Photo;
                        studio.tool = omadesign::tools::Tool::Hand;
                    } else {
                        studio.doc = match omadesign::formats::cli::document(path) {
                            Ok(doc) => doc,
                            Err(error) => {
                                eprintln!("{error}");
                                std::process::exit(2);
                            }
                        };
                        studio.active_layer = studio.doc.layers.len().checked_sub(1);
                        studio.layer_expanded.extend(
                            studio
                                .doc
                                .layers
                                .iter()
                                .filter(|l| l.is_group)
                                .map(|l| l.id),
                        );
                        studio.need_fit = true;
                    }
                    studio.show_welcome = false;
                } else if let Err(e) = shots::apply(&mut studio, &name) {
                    eprintln!("{e}");
                    std::process::exit(2);
                }
                Ok(Box::new(ShotRunner {
                    studio,
                    out,
                    frame: 0,
                    requested: false,
                    size: shot_size,
                    modifiers: shot_modifiers,
                    detail_zoom_pending: name == "raw-detail" && shot_file.is_some(),
                }))
            }),
        );
    }

    let open: Vec<PathBuf> = args
        .into_iter()
        .filter(|a| !a.starts_with('-'))
        .map(PathBuf::from)
        .filter(|p| p.exists())
        .collect();

    eframe::run_native(
        "omadesign",
        options,
        Box::new(move |cc| {
            theme::apply(&cc.egui_ctx);
            cc.egui_ctx.set_pixels_per_point(1.0);
            let mut studio = Studio::new();
            for p in &open {
                studio.open_path(p.clone());
            }
            Ok(Box::new(studio))
        }),
    )
}

fn run_headless() -> eframe::Result {
    let mut studio = Studio::new();
    studio.seed_demo();
    match studio.export_demo_png("omadesign-demo-export.png") {
        Ok(()) => {
            println!("wrote omadesign-demo-export.png");
            Ok(())
        }
        Err(e) => {
            eprintln!("export failed: {e}");
            std::process::exit(1);
        }
    }
}

struct ShotRunner {
    studio: Studio,
    out: PathBuf,
    frame: u32,
    requested: bool,
    size: egui::Vec2,
    modifiers: egui::Modifiers,
    detail_zoom_pending: bool,
}

impl eframe::App for ShotRunner {
    fn raw_input_hook(&mut self, _ctx: &egui::Context, input: &mut egui::RawInput) {
        // A preview must not edit its scene when the user's desktop sends input.
        input
            .events
            .retain(|event| matches!(event, egui::Event::Screenshot { .. }));
        input.hovered_files.clear();
        input.dropped_files.clear();
        input
            .events
            .push(egui::Event::ModifiersChanged(self.modifiers));
        input.focused = true;
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        omadesign::ui::run(ui, &mut self.studio);
        let ctx = ui.ctx().clone();
        self.frame += 1;
        if self.detail_zoom_pending
            && self.frame >= 3
            && self.studio.persona == omadesign::tools::Persona::Photo
            && self.studio.photo.fit_scale > 0.0
        {
            self.studio.photo.view_scale = 1.0 / self.studio.photo.fit_scale;
            self.studio.photo.view_offset = egui::Vec2::ZERO;
            self.detail_zoom_pending = false;
        }
        if self.frame == 2 {
            ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(self.size));
        }
        let size = ctx.viewport_rect().size();
        let sized = (size - self.size).abs().max_elem() <= 1.0;
        if !self.requested
            && self.frame >= 28
            && sized
            && omadesign::ui::scene_ready(&ctx, &self.studio)
        {
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default()));
            self.requested = true;
        }
        let mut image = None;
        ctx.input(|i| {
            for ev in &i.raw.events {
                if let egui::Event::Screenshot { image: img, .. } = ev {
                    image = Some(img.clone());
                }
            }
        });
        if let Some(img) = image {
            if let Err(e) = save_color_image(&img, &self.out) {
                eprintln!("write {}: {e}", self.out.display());
                std::process::exit(1);
            }
            println!("wrote {}", self.out.display());
            std::process::exit(0);
        }
        if self.frame > 240 {
            eprintln!(
                "screenshot timed out after {} frames ({}x{})",
                self.frame, size.x, size.y
            );
            std::process::exit(1);
        }
        ctx.request_repaint();
    }
}

fn save_color_image(img: &egui::ColorImage, path: &PathBuf) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let mut rgba = Vec::with_capacity(img.pixels.len() * 4);
    for p in &img.pixels {
        rgba.extend_from_slice(&[p.r(), p.g(), p.b(), p.a()]);
    }
    image::save_buffer(
        path,
        &rgba,
        img.size[0] as u32,
        img.size[1] as u32,
        image::ColorType::Rgba8,
    )
    .map_err(|e| e.to_string())
}
