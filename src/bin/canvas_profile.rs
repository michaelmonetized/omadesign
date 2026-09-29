//! Focused CPU attribution for the supplied authoring fixture, without a window.
use omadesign::{
    compositor::{self, Draft, View},
    geom::Pt,
};
use std::time::Instant;
fn main() {
    let args: Vec<_> = std::env::args().collect();
    let doc = omadesign::project::load_from(std::path::Path::new(&args[1])).unwrap();
    let view = View {
        scale: 0.9,
        offset: Pt::new(200., 24.),
    };
    let mut rows = vec![];
    for index in 0..=doc.layers.len() {
        let mut d = doc.clone();
        let name = if index == doc.layers.len() {
            "ALL".to_owned()
        } else {
            d.layers = vec![doc.layers[index].clone()];
            d.layers[0].name.clone()
        };
        for round in 0..4 {
            let start = Instant::now();
            let pm = compositor::render_view(&d, view, 1456, 900, Draft::none()).unwrap();
            let render_ms = start.elapsed().as_secs_f64() * 1000.;
            let start = Instant::now();
            let image = eframe::egui::ColorImage::from_rgba_unmultiplied([1456, 900], pm.data());
            std::hint::black_box(image);
            rows.push(serde_json::json!({"layer":name,"round":round,"render_ms":render_ms,"convert_ms":start.elapsed().as_secs_f64()*1000.}));
        }
    }
    println!("{}", serde_json::to_string_pretty(&rows).unwrap());
}
