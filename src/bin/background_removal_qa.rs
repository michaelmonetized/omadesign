//! Reproducible real-model QA. Writes full-size mattes and cutouts for inspection.
use omadesign::{
    background_removal::{self as removal, RawMask, Settings, Source},
    ml::{Progress, models::Model},
};
use std::{path::PathBuf, sync::Arc, time::Instant};
#[path = "background_removal_qa/native.rs"]
mod native;

struct Report;
impl Progress for Report {
    fn report(&self, stage: &str, done: usize, total: usize) {
        eprintln!("{stage}: {done}/{total}");
    }
    fn cancelled(&self) -> bool {
        false
    }
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let input = args
        .first()
        .ok_or("background_removal_qa INPUT OUTPUT_DIR [u2net|isnet] [--download]")?;
    let directory = PathBuf::from(args.get(1).ok_or("Missing output directory")?);
    std::fs::create_dir_all(&directory)?;
    if args.iter().any(|a| a == "--native") {
        return native::run(PathBuf::from(input), directory).map_err(|e| e.to_string().into());
    }
    let model = match args.get(2).map(String::as_str) {
        Some("u2net") => Model::U2Net,
        Some("isnet") => Model::IsNet,
        _ => Model::U2NetP,
    };
    if args.iter().any(|a| a == "--download") {
        model.download(&Report)?;
    }
    let img = image::open(input)?.to_rgba8();
    let source = Source {
        w: img.width(),
        h: img.height(),
        rgba: Arc::new(img.into_raw()),
        selection: None,
        existing_mask: None,
    };
    let started = Instant::now();
    let raw = removal::infer(&source, model, &Report)?;
    let inference_seconds = started.elapsed().as_secs_f64();
    let started = Instant::now();
    let refined = removal::refine(&source, &raw, Settings::default(), &Report)?;
    let refinement_seconds = started.elapsed().as_secs_f64();
    let baseline = RawMask {
        region: raw.region,
        rough: vec![],
        values: raw.rough.clone(),
        stats: Default::default(),
    };
    let coarse = removal::refine(
        &source,
        &baseline,
        Settings {
            radius: 0,
            ..Default::default()
        },
        &Report,
    )?;
    let coarse_refined = removal::refine(&source, &baseline, Settings::default(), &Report)?;
    for (name, mask) in [
        ("global", coarse),
        ("global-guided", coarse_refined),
        ("tiled-guided", refined),
    ] {
        image::save_buffer(
            directory.join(format!("{name}-mask.png")),
            &mask,
            source.w,
            source.h,
            image::ColorType::Rgba8,
        )?;
        let mut cutout = source.rgba.as_ref().clone();
        for (p, m) in cutout.chunks_exact_mut(4).zip(mask.chunks_exact(4)) {
            p[3] = ((p[3] as u32 * m[0] as u32 + 127) / 255) as u8;
        }
        image::save_buffer(
            directory.join(format!("{name}-cutout.png")),
            &cutout,
            source.w,
            source.h,
            image::ColorType::Rgba8,
        )?;
    }
    let result = serde_json::json!({"model":model.filename(),"sha256":model.sha256(),"width":source.w,"height":source.h,"tiles":raw.stats.tiles,"edge_tiles":raw.stats.edge_tiles,"skipped_tiles":raw.stats.tiles-raw.stats.edge_tiles,"inference_calls":raw.stats.inference_calls,"inference_seconds":inference_seconds,"refinement_seconds":refinement_seconds,"runtime":"ONNX Runtime 1.28 CPU","settings":{"radius":12,"epsilon":0.0001,"shift":0,"contrast":1}});
    std::fs::write(
        directory.join("result.json"),
        serde_json::to_vec_pretty(&result)?,
    )?;
    println!("{result}");
    Ok(())
}
