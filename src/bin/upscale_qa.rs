//! Real-model, offline-capable QA using the exact application pipelines.
use omadesign::{
    ml::Progress,
    upscale::{self, Settings, models::Model},
};
use std::{
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
    time::Instant,
};
#[path = "upscale_qa/native.rs"]
mod native;
#[path = "upscale_qa/pipelines.rs"]
mod pipelines;
struct Report {
    done: AtomicUsize,
    cancel_at: usize,
}
impl Progress for Report {
    fn report(&self, stage: &str, done: usize, total: usize) {
        if stage.contains("tiles") {
            self.done.store(done, Ordering::Relaxed);
        }
        if done == 0 || done == total || stage.contains("tiles") && done % 20 == 0 {
            eprintln!("{stage}: {done}/{total}");
        }
    }
    fn cancelled(&self) -> bool {
        self.cancel_at > 0 && self.done.load(Ordering::Relaxed) >= self.cancel_at
    }
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let input = args.first().ok_or("upscale_qa INPUT OUTPUT FACTOR [general|quality|native2|illustration] [--download] [--cancel] [--tile=N]")?;
    let output = PathBuf::from(args.get(1).ok_or("Missing output path")?);
    if args.iter().any(|a| a == "--pipelines") {
        return pipelines::run(std::path::Path::new(input), &output);
    }
    if args.iter().any(|a| a == "--native") {
        std::fs::create_dir_all(&output)?;
        return native::run(PathBuf::from(input), output).map_err(|e| e.to_string().into());
    }
    let factor = args.get(2).ok_or("Missing scale")?.parse::<f64>()?;
    let model = match args.get(3).map(String::as_str) {
        Some("quality") => Model::Quality,
        Some("native2") => Model::Native2,
        Some("illustration") => Model::Illustration,
        _ => Model::General,
    };
    let progress = Report {
        done: AtomicUsize::new(0),
        cancel_at: if args.iter().any(|a| a == "--cancel") {
            1
        } else {
            0
        },
    };
    if args.iter().any(|a| a == "--download") {
        model.download(&progress)?;
    }
    let source = image::open(input)?.to_rgba8();
    let tile = args
        .iter()
        .find_map(|s| s.strip_prefix("--tile="))
        .map(str::parse)
        .transpose()?
        .unwrap_or(192);
    let start = Instant::now();
    let result = upscale::upscale(
        &source,
        Settings {
            factor,
            model,
            tile,
        },
        &progress,
    );
    if progress.cancel_at > 0 {
        assert!(result.is_err());
        assert!(!output.exists());
        println!("Cancelled after one tile; no output was written");
        return Ok(());
    }
    let result = result?;
    let seconds = start.elapsed().as_secs_f64();
    let dimensions = result.dimensions();
    result.save(&output)?;
    let report = serde_json::json!({ "model": model.filename(), "sha256": model.sha256(), "factor": factor, "source": source.dimensions(), "output": dimensions, "tile": tile, "tiles": progress.done.load(Ordering::Relaxed), "seconds": seconds, "runtime": "ONNX Runtime 1.28 CPU" });
    std::fs::write(
        output.with_extension("json"),
        serde_json::to_vec_pretty(&report)?,
    )?;
    println!("{report}");
    Ok(())
}
