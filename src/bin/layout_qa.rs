//! Generate reproducible native and browser Layout artifacts for visual review.
use omadesign::{
    compositor,
    geom::{Bounds, Pt},
    layout, layout_demo, layout_export, project, svg,
};
use std::path::PathBuf;

fn run() -> Result<(), String> {
    let out = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .ok_or("Usage: layout_qa OUTPUT_DIRECTORY")?;
    std::fs::create_dir_all(&out).map_err(|e| e.to_string())?;
    let doc = layout_demo::build();
    let root = doc.layers[0]
        .kind
        .shapes()
        .and_then(|s| s.first())
        .ok_or("Demo root missing")?
        .id;
    std::fs::write(out.join("fieldwork.oma"), project::encode(&doc)?).map_err(|e| e.to_string())?;
    std::fs::write(
        out.join("fieldwork.html"),
        layout_export::export_html(&doc, 0, root)?,
    )
    .map_err(|e| e.to_string())?;
    std::fs::write(out.join("fieldwork.svg"), svg::export_frame(&doc, 0, root)?)
        .map_err(|e| e.to_string())?;
    let mut measurements = Vec::new();
    for (name, width) in [("desktop", 1280.), ("tablet", 768.), ("phone", 375.)] {
        let mut variant = doc.layout_snapshot();
        let frame = variant.find_shape_mut(0, root).ok_or("Demo root missing")?;
        let b = frame.geom.bbox();
        layout::set_bounds(
            &mut frame.geom,
            Bounds::from_min_size(b.min, Pt::new(width, b.height())),
        );
        layout::reflow(&mut variant, 0, root);
        let start = std::time::Instant::now();
        let png = compositor::export_frame_png(&variant, 0, root, 1)?;
        let render_ms = start.elapsed().as_secs_f64() * 1000.;
        std::fs::write(out.join(format!("fieldwork-{name}.png")), png)
            .map_err(|e| e.to_string())?;
        let root_shape = variant.find_shape(0, root).ok_or("Demo root missing")?;
        let b = root_shape.geom.bbox();
        measurements.push(serde_json::json!({"viewport":name,"width":b.width(),"height":b.height(),"render_ms":render_ms}));
    }
    std::fs::write(
        out.join("measurements.json"),
        serde_json::to_vec_pretty(&measurements).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    println!("Generated {}", out.display());
    println!(
        "{}",
        serde_json::to_string_pretty(&measurements).map_err(|e| e.to_string())?
    );
    Ok(())
}
fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1)
    }
}
