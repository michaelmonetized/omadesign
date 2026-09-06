//! Rebuild the vector examples and import/develop the original generated images
//! with the studio's own engines. Run from the repository root after a release build.
//! rustc --edition 2024 -C lto=thin -C embed-bitcode=yes examples/site-showcase/render.rs \
//!   --extern omadesign=target/release/libomadesign.rlib -L dependency=target/release/deps \
//!   -o /tmp/omadesign-showcase && /tmp/omadesign-showcase
extern crate omadesign;
use omadesign::{
    compositor,
    document::{Document, Layer, Pixels},
    geom::{Geom, Pt},
    motion_presets::{self, Options, Preset, Target},
    photo::{self, DevelopParams, RgbaImage},
    project, svg, templates, text,
};
use std::{fs, path::Path};
fn save(name: &str, doc: &Document) {
    let directory = Path::new("examples/site-showcase/exports");
    fs::create_dir_all(directory).unwrap();
    fs::write(
        directory.join(format!("{name}.png")),
        compositor::export_png(doc, 1).unwrap(),
    )
    .unwrap();
    fs::write(
        directory.join(format!("{name}.svg")),
        svg::export(doc).unwrap(),
    )
    .unwrap();
    project::save_to(
        doc,
        &Path::new("examples/site-showcase").join(format!("{name}.oma")),
    )
    .unwrap();
}
fn main() {
    let mut design = templates::build("block-party", 1600., 1000., 72.).unwrap();
    for layer in &mut design.layers {
        if let Some(shapes) = layer.kind.shapes_mut() {
            for shape in shapes {
                if let Geom::Text(run) = &mut shape.geom {
                    run.content = run.content.replace("NEIGHBOURHOOD", "NEIGHBORHOOD");
                    text::fill_contours(&mut shape.geom);
                }
            }
        }
    }
    save("design", &design);
    let mut motion = templates::build("after-hours", 1600., 1000., 72.).unwrap();
    let targets: Vec<_> = motion.layers[1]
        .kind
        .shapes()
        .unwrap()
        .iter()
        .filter(|s| Preset::DrawStroke.supports(s))
        .map(|s| Target {
            id: s.id,
            bounds: s.world_bbox(),
            opacity: s.opacity,
        })
        .collect();
    motion.motion = motion_presets::apply(
        &motion.motion,
        Preset::DrawStroke,
        &targets,
        0.,
        Options {
            duration: 1.6,
            delay: 0.2,
            stagger: 0.07,
            intensity: 1.,
            start_at_playhead: false,
        },
    )
    .unwrap();
    motion.motion.duration = 3.;
    motion.motion.looped = true;
    save("motion", &motion);
    fs::write(
        "examples/site-showcase/exports/motion-animated.svg",
        svg::export_animated(&motion).unwrap(),
    )
    .unwrap();
    println!(
        "Design: {} shapes; Motion: {} editable channels",
        design
            .layers
            .iter()
            .filter_map(|l| l.kind.shapes())
            .map(|s| s.len())
            .sum::<usize>(),
        motion.motion.tracks.len()
    );
    let pixel = photo::load_file(Path::new("examples/site-showcase/pixel-original.png")).unwrap();
    save_raster("pixel", "Iris / original generated painting", &pixel);
    let photo = photo::load_file(Path::new("examples/site-showcase/photo-original.png")).unwrap();
    let developed = photo::develop(
        &photo,
        &DevelopParams {
            exposure: 0.06,
            highlights: -0.12,
            shadows: 0.08,
            vibrance: 1.035,
            ..DevelopParams::default()
        },
    );
    save_raster(
        "photo",
        "Coast / generated landscape, native Photo development",
        &developed,
    );
}

fn save_raster(name: &str, label: &str, image: &RgbaImage) {
    // Raster projects store complete pixel buffers. Keep those reproducible
    // intermediates out of the repository; only the original and WebP ship.
    let directory = std::env::temp_dir().join("omadesign-site-showcase");
    fs::create_dir_all(&directory).unwrap();
    let mut doc = Document::new(label, image.w as f32, image.h as f32, 72.);
    doc.layers = vec![Layer::placed_raster(
        label,
        Pixels::from_rgba(image.w, image.h, image.data.clone()).unwrap(),
        Pt::ZERO,
        Pt::new(image.w as f32, image.h as f32),
    )];
    project::save_to(&doc, &directory.join(format!("{name}.oma"))).unwrap();
    fs::write(
        directory.join(format!("{name}.png")),
        compositor::export_png(&doc, 1).unwrap(),
    )
    .unwrap();
    println!(
        "{name}: native raster import/export {}×{}",
        image.w, image.h
    );
}
