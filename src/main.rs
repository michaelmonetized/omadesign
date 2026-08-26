mod boolean;
mod brush;
mod document;
mod main_app;
mod project;
mod render;
mod svg;
mod text;
mod ui;

use eframe::egui::ViewportBuilder;

fn main() -> eframe::Result {
    if std::env::args().any(|a| a == "--export-demo") {
        return run_headless_export();
    }
    let options = eframe::NativeOptions {
        viewport: ViewportBuilder::default()
            .with_inner_size([1440.0, 900.0])
            .with_title("Atelier - all-in-one design"),
        ..Default::default()
    };
    eframe::run_native(
        "atelier",
        options,
        Box::new(|cc| Ok(Box::new(main_app::AtelierApp::new(cc)))),
    )
}

fn run_headless_export() -> eframe::Result {
    let mut app = main_app::AtelierApp::new_dummy();
    app.seed_demo();
    match render::export::png_bytes(&app.doc) {
        Ok(bytes) => {
            let path = "atelier-demo-export.png";
            std::fs::write(path, &bytes).unwrap_or_else(|e| panic!("write failed: {e}"));
            println!("wrote {path} ({} bytes)", bytes.len());
            Ok(())
        }
        Err(e) => {
            eprintln!("export failed: {e}");
            std::process::exit(1);
        }
    }
}
