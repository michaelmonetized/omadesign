use ab_glyph::{Font as _, ScaleFont as _};
use eframe::egui::pos2;
use std::path::PathBuf;
use std::sync::OnceLock;

const FONT_CANDIDATES: &[&str] = &[
    "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
    "/usr/share/fonts/dejavu/DejaVuSans.ttf",
    "/usr/share/fonts/liberation/LiberationSans-Regular.ttf",
    "/usr/share/fonts/TTF/DejaVuSans.ttf",
    "/usr/share/fonts/ubuntu/Ubuntu-R.ttf",
];

static FONT: OnceLock<Option<ab_glyph::FontVec>> = OnceLock::new();

pub fn font_path() -> Option<PathBuf> {
    FONT_CANDIDATES
        .iter()
        .map(PathBuf::from)
        .find(|p| p.exists())
}

fn font() -> Option<&'static ab_glyph::FontVec> {
    FONT.get_or_init(|| {
        let path = font_path()?;
        let bytes = std::fs::read(path).ok()?;
        ab_glyph::FontVec::try_from_vec(bytes).ok()
    })
    .as_ref()
}

pub fn available() -> bool {
    font().is_some()
}

fn flatten_contour(curves: &[ab_glyph::OutlineCurve]) -> Vec<eframe::egui::Pos2> {
    use ab_glyph::OutlineCurve;
    let mut pts: Vec<eframe::egui::Pos2> = vec![];
    #[allow(dead_code)]
    let start_of = |c: &OutlineCurve| match c {
        OutlineCurve::Line(p0, _)
        | OutlineCurve::Quad(p0, _, _)
        | OutlineCurve::Cubic(p0, _, _, _) => *p0,
    };
    if let Some(first) = curves.first() {
        let s = start_of(first);
        pts.push(pos2(s.x, s.y));
    }
    for c in curves {
        let s = start_of(c);
        let broke = pts
            .last()
            .map(|last| (last.x - s.x).abs() > 1e-3 || (last.y - s.y).abs() > 1e-3)
            .unwrap_or(true);
        if broke {
            pts.push(pos2(s.x, s.y));
        }
        match c {
            OutlineCurve::Line(_, p1) => pts.push(pos2(p1.x, p1.y)),
            OutlineCurve::Quad(p0, p1, p2) => {
                for i in 1..=8 {
                    let t = i as f32 / 8.0;
                    let a = (1.0 - t) * (1.0 - t);
                    let b = 2.0 * (1.0 - t) * t;
                    let d = t * t;
                    pts.push(pos2(
                        a * p0.x + b * p1.x + d * p2.x,
                        a * p0.y + b * p1.y + d * p2.y,
                    ));
                }
            }
            OutlineCurve::Cubic(p0, p1, p2, p3) => {
                for i in 1..=12 {
                    let t = i as f32 / 12.0;
                    let a = (1.0 - t) * (1.0 - t) * (1.0 - t);
                    let b = 3.0 * (1.0 - t) * (1.0 - t) * t;
                    let d = 3.0 * (1.0 - t) * t * t;
                    let e = t * t * t;
                    pts.push(pos2(
                        a * p0.x + b * p1.x + d * p2.x + e * p3.x,
                        a * p0.y + b * p1.y + d * p2.y + e * p3.y,
                    ));
                }
            }
        }
    }
    pts
}

pub fn shape_text(
    content: &str,
    px: f32,
    origin: eframe::egui::Pos2,
) -> Vec<Vec<eframe::egui::Pos2>> {
    let Some(f) = font() else {
        return vec![];
    };
    let scale = ab_glyph::PxScale::from(px);
    let scaled = f.into_scaled(scale);
    let factor = scaled.scale_factor();
    let (fh, fv) = (factor.horizontal, -factor.vertical);

    let mut subpaths = vec![];
    let mut pen_x = origin.x;
    let baseline = origin.y;
    for ch in content.chars() {
        if ch == ' ' {
            pen_x += scaled.h_advance(scaled.glyph_id(' '));
            continue;
        }
        let gid = scaled.glyph_id(ch);
        if gid.0 == 0 && ch != '\0' {
            pen_x += scaled.h_advance(scaled.glyph_id(' '));
            continue;
        }
        if let Some(outline) = f.outline(gid) {
            let mapped: Vec<ab_glyph::OutlineCurve> = outline
                .curves
                .iter()
                .map(|c| match *c {
                    ab_glyph::OutlineCurve::Line(p0, p1) => ab_glyph::OutlineCurve::Line(
                        ab_glyph::point(pen_x + p0.x * fh, baseline + p0.y * fv),
                        ab_glyph::point(pen_x + p1.x * fh, baseline + p1.y * fv),
                    ),
                    ab_glyph::OutlineCurve::Quad(p0, p1, p2) => ab_glyph::OutlineCurve::Quad(
                        ab_glyph::point(pen_x + p0.x * fh, baseline + p0.y * fv),
                        ab_glyph::point(pen_x + p1.x * fh, baseline + p1.y * fv),
                        ab_glyph::point(pen_x + p2.x * fh, baseline + p2.y * fv),
                    ),
                    ab_glyph::OutlineCurve::Cubic(p0, p1, p2, p3) => ab_glyph::OutlineCurve::Cubic(
                        ab_glyph::point(pen_x + p0.x * fh, baseline + p0.y * fv),
                        ab_glyph::point(pen_x + p1.x * fh, baseline + p1.y * fv),
                        ab_glyph::point(pen_x + p2.x * fh, baseline + p2.y * fv),
                        ab_glyph::point(pen_x + p3.x * fh, baseline + p3.y * fv),
                    ),
                })
                .collect();
            let pts = flatten_contour(&mapped);
            if pts.len() >= 3 {
                subpaths.push(pts);
            }
        }
        pen_x += scaled.h_advance(gid);
    }
    subpaths
}

#[allow(dead_code)]
pub fn measure(content: &str, px: f32) -> (f32, f32) {
    let Some(f) = font() else {
        return (px * content.len() as f32 * 0.5, px);
    };
    let scaled = f.into_scaled(ab_glyph::PxScale::from(px));
    let mut w = 0.0;
    for ch in content.chars() {
        w += scaled.h_advance(scaled.glyph_id(ch));
    }
    let ascent = scaled.ascent();
    let descent = scaled.descent();
    (w, ascent - descent)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shapes_text_into_contours_when_font_available() {
        if !available() {
            return;
        }
        let subpaths = shape_text("Ag", 48.0, pos2(0.0, 0.0));
        assert!(subpaths.len() >= 2, "expected contours for A and g");
        for sp in &subpaths {
            assert!(sp.len() >= 3);
            let mut min_y = f32::INFINITY;
            for p in sp {
                min_y = min_y.min(p.y);
            }
            assert!(min_y < 0.0, "glyph should rise above baseline");
        }
        let (w, h) = measure("Ag", 48.0);
        assert!(w > 20.0 && h > 20.0);
    }
}
