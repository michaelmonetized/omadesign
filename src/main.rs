use eframe::egui::ViewportBuilder;
use omadesign::app::Studio;
use omadesign::ui::theme;

fn main() -> eframe::Result {
    if std::env::args().any(|a| a == "--export-demo") {
        return run_headless();
    }
    let options = eframe::NativeOptions {
        viewport: ViewportBuilder::default()
            .with_inner_size([1440.0, 900.0])
            .with_min_inner_size([960.0, 640.0])
            .with_title("omadesign"),
        ..Default::default()
    };
    eframe::run_native(
        "omadesign",
        options,
        Box::new(|cc| {
            theme::apply(&cc.egui_ctx);
            Ok(Box::new(Studio::new()))
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
