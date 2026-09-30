//! CPU-side motion rendering on a supplied composition; does not measure displayed FPS.
use omadesign::{
    compositor::{self, Draft, View},
    geom::Pt,
};
use std::{path::Path, time::Instant};
fn main() {
    let args: Vec<_> = std::env::args().collect();
    let path = Path::new(&args[1]);
    let doc = omadesign::project::load_from(path).unwrap();
    let view = View {
        scale: (1200. / doc.width).min(750. / doc.height),
        offset: Pt::new(24., 24.),
    };
    let mut rows = vec![];
    for cycle in 0..2 {
        for frame in 0..12 {
            let time = frame as f32 / 11. * doc.motion.duration;
            let started = Instant::now();
            let pm = compositor::render_view_posed(
                &doc,
                view,
                1248,
                798,
                Draft::none(),
                Some(time),
                None,
            )
            .unwrap();
            let ms = started.elapsed().as_secs_f64() * 1000.;
            rows.push(serde_json::json!({"cycle":cycle,"frame":frame,"time":time,"render_ms":ms}));
            if cycle == 1
                && frame == 3
                && let Some(output) = args.get(2)
            {
                pm.save_png(output).unwrap();
            }
            std::hint::black_box(pm);
        }
    }
    let mut times: Vec<_> = rows
        .iter()
        .filter(|r| r["cycle"] == 1)
        .map(|r| r["render_ms"].as_f64().unwrap())
        .collect();
    times.sort_by(f64::total_cmp);
    println!("{}",serde_json::to_string_pretty(&serde_json::json!({"document":path,"viewport":[1248,798],"scale":view.scale,"warm_median_ms":(times[5]+times[6])/2.,"warm_max_ms":times[11],"frames":rows})).unwrap());
}
