use eframe::egui::{self, ViewportBuilder};
use omadesign::app::Studio;
use omadesign::shots;
use omadesign::ui::theme;
use std::path::PathBuf;

fn main() -> eframe::Result {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--export-demo") {
        return run_headless();
    }
    if args.iter().any(|a| a == "--list-shots") {
        for scene in shots::SCENES {
            println!("{}\t{}", scene.id, scene.caption);
        }
        return Ok(());
    }
    let shot = args.windows(2).find(|w| w[0] == "--shot").map(|w| w[1].clone());
    let out = args
        .windows(2)
        .find(|w| w[0] == "--out")
        .map(|w| PathBuf::from(&w[1]));

    let options = eframe::NativeOptions {
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
        let out = out.unwrap_or_else(|| PathBuf::from(format!("{name}.png")));
        return eframe::run_native(
            "omadesign",
            options,
            Box::new(move |cc| {
                theme::apply(&cc.egui_ctx);
                cc.egui_ctx.set_pixels_per_point(1.0);
                let mut studio = Studio::new();
                if let Err(e) = shots::apply(&mut studio, &name) {
                    eprintln!("{e}");
                    std::process::exit(2);
                }
                Ok(Box::new(ShotRunner {
                    studio,
                    out,
                    frame: 0,
                    requested: false,
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
}

impl eframe::App for ShotRunner {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        omadesign::ui::run(ui, &mut self.studio);
        let ctx = ui.ctx().clone();
        self.frame += 1;
        if self.frame == 2 {
            ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(1600.0, 1000.0)));
        }
        let size = ctx.viewport_rect().size();
        let landscape = size.x >= 1400.0 && size.y >= 800.0 && size.y <= 1200.0;
        if !self.requested && self.frame >= 28 && (landscape || self.frame >= 80) {
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
            eprintln!("screenshot timed out after {} frames ({}x{})", self.frame, size.x, size.y);
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
