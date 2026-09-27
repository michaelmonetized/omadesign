//! Alternating CPU compositor comparison with the retained historical filter path.
use omadesign::{
    color::{Blend, Rgba},
    document::{Document, Fill, Layer, Shape, Style},
    filter::Fx,
    geom::{Geom, Pt},
};
use std::{hint::black_box, time::Instant};
fn scene(legacy: bool) -> Document {
    let mut doc = Document::new("Normal effect performance", 512., 384., 72.);
    doc.artboards.clear();
    doc.layers = vec![Layer::vector("Artwork")];
    for row in 0..2 {
        for col in 0..3 {
            let mut shape = Shape::new(
                Geom::Rect {
                    origin: Pt::new(24. + col as f32 * 160., 24. + row as f32 * 176.),
                    size: Pt::new(120., 120.),
                    radius: 12.,
                },
                Style {
                    fill: Fill::Solid(Rgba::rgb(60, 130, 210)),
                    stroke: None,
                },
            );
            shape.filters.legacy_composite = legacy;
            shape.filters.items = vec![Fx::Shadow {
                dx: 6.,
                dy: 8.,
                blur: 5.,
                color: Rgba::new(0, 0, 0, 160),
                blend: Blend::Normal,
                opacity: 1.,
                knockout: false,
                spread: 0.,
            }];
            doc.layers[0].kind.shapes_mut().unwrap().push(shape);
        }
    }
    doc
}
fn measure(doc: &Document, iterations: usize) -> f64 {
    let start = Instant::now();
    for _ in 0..iterations {
        black_box(omadesign::compositor::render_export_at(black_box(doc), 1., 0., true).unwrap());
    }
    start.elapsed().as_secs_f64() * 1000. / iterations as f64
}
fn main() {
    let old = scene(true);
    let new = scene(false);
    let a = omadesign::compositor::render_export_at(&old, 1., 0., true).unwrap();
    let b = omadesign::compositor::render_export_at(&new, 1., 0., true).unwrap();
    assert_eq!(
        a.data(),
        b.data(),
        "Performance comparison must use pixel-identical inputs"
    );
    measure(&old, 3);
    measure(&new, 3);
    let mut old_times = vec![];
    let mut new_times = vec![];
    let iterations = 8;
    for round in 0..9 {
        if round % 2 == 0 {
            old_times.push(measure(&old, iterations));
            new_times.push(measure(&new, iterations));
        } else {
            new_times.push(measure(&new, iterations));
            old_times.push(measure(&old, iterations));
        }
    }
    let median = |data: &[f64]| {
        let mut v = data.to_vec();
        v.sort_by(f64::total_cmp);
        v[v.len() / 2]
    };
    let old_median = median(&old_times);
    let new_median = median(&new_times);
    println!("{}",serde_json::to_string_pretty(&serde_json::json!({"method":"Alternating retained pre-153 filter path vs Normal/100% independent path in the same binary", "fixture":"512x384, six rounded 120px rectangles, one Normal/100% shadow each, knockout=false on both", "pixel_identical":true,"build_profile":"dev, opt-level=0; CPU compositor only; not WGPU presentation timing","rounds":9,"iterations_per_round":iterations,"legacy_ms_per_frame":old_times,"independent_ms_per_frame":new_times,"legacy_median_ms":old_median,"independent_median_ms":new_median,"relative_change_percent":(new_median/old_median-1.)*100.})).unwrap());
}
