//! System-font text shaping. Glyph outlines become closed contours.

use crate::geom::Pt;
use ab_glyph::{Font as _, ScaleFont as _};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

const PREFERRED: &[&str] = &[
    "/usr/share/fonts/liberation/LiberationSans-Regular.ttf",
    "/usr/share/fonts/liberation/LiberationSans-Bold.ttf",
    "/usr/share/fonts/noto/NotoSans-Regular.ttf",
    "/usr/share/fonts/TTF/DejaVuSans.ttf",
    "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
    "/usr/share/fonts/dejavu/DejaVuSans.ttf",
];

static FONTS: OnceLock<Vec<FontFace>> = OnceLock::new();

#[derive(Clone)]
pub struct FontFace {
    pub name: String,
    pub path: PathBuf,
}

fn scan_fonts() -> Vec<FontFace> {
    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for p in PREFERRED {
        let path = PathBuf::from(p);
        if path.exists() {
            let name = path
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| "Sans".into());
            seen.insert(path.clone());
            out.push(FontFace { name, path });
        }
    }
    for root in [
        "/usr/share/fonts/liberation",
        "/usr/share/fonts/noto",
        "/usr/share/fonts/TTF",
        "/usr/share/fonts/truetype",
        "/usr/local/share/fonts",
        &format!(
            "{}/.local/share/fonts",
            std::env::var("HOME").unwrap_or_default()
        ),
    ] {
        walk_ttfs(Path::new(root), &mut out, &mut seen, 0);
        if out.len() > 48 {
            break;
        }
    }
    out
}

fn walk_ttfs(dir: &Path, out: &mut Vec<FontFace>, seen: &mut std::collections::HashSet<PathBuf>, depth: u8) {
    if depth > 3 || out.len() > 48 {
        return;
    }
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            walk_ttfs(&p, out, seen, depth + 1);
            continue;
        }
        let ext = p
            .extension()
            .and_then(|s| s.to_str())
            .map(|s| s.to_ascii_lowercase());
        if matches!(ext.as_deref(), Some("ttf") | Some("otf")) && seen.insert(p.clone()) {
            let name = p
                .file_stem()
                .map(|s| s.to_string_lossy().replace('-', " "))
                .unwrap_or_else(|| "Font".into());
            out.push(FontFace { name, path: p });
        }
    }
}

pub fn fonts() -> &'static [FontFace] {
    FONTS.get_or_init(scan_fonts)
}

pub fn default_path() -> Option<PathBuf> {
    fonts().first().map(|f| f.path.clone())
}

fn load_font(path: &Path) -> Option<ab_glyph::FontVec> {
    let bytes = std::fs::read(path).ok()?;
    ab_glyph::FontVec::try_from_vec(bytes).ok()
}

fn flatten_contour(curves: &[ab_glyph::OutlineCurve]) -> Vec<Pt> {
    use ab_glyph::OutlineCurve;
    let mut pts: Vec<Pt> = vec![];
    let start_of = |c: &OutlineCurve| match c {
        OutlineCurve::Line(p0, _)
        | OutlineCurve::Quad(p0, _, _)
        | OutlineCurve::Cubic(p0, _, _, _) => *p0,
    };
    if let Some(first) = curves.first() {
        let s = start_of(first);
        pts.push(Pt::new(s.x, s.y));
    }
    for c in curves {
        let s = start_of(c);
        let broke = pts
            .last()
            .map(|last| (last.x - s.x).abs() > 1e-3 || (last.y - s.y).abs() > 1e-3)
            .unwrap_or(true);
        if broke {
            pts.push(Pt::new(s.x, s.y));
        }
        match c {
            OutlineCurve::Line(_, p1) => pts.push(Pt::new(p1.x, p1.y)),
            OutlineCurve::Quad(p0, p1, p2) => {
                for i in 1..=8 {
                    let t = i as f32 / 8.0;
                    let a = (1.0 - t) * (1.0 - t);
                    let b = 2.0 * (1.0 - t) * t;
                    let d = t * t;
                    pts.push(Pt::new(
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
                    pts.push(Pt::new(
                        a * p0.x + b * p1.x + d * p2.x + e * p3.x,
                        a * p0.y + b * p1.y + d * p2.y + e * p3.y,
                    ));
                }
            }
        }
    }
    pts
}

pub fn shape(content: &str, px: f32, origin: Pt, tracking: f32, font_path: Option<&Path>) -> Vec<Vec<Pt>> {
    let path = font_path
        .map(|p| p.to_path_buf())
        .or_else(default_path);
    let Some(path) = path else {
        return fallback_block(content, px, origin);
    };
    let Some(f) = load_font(&path) else {
        return fallback_block(content, px, origin);
    };
    let scale = ab_glyph::PxScale::from(px.max(1.0));
    let scaled = f.as_scaled(scale);
    let factor = scaled.scale_factor();
    let (fh, fv) = (factor.horizontal, -factor.vertical);

    let mut subpaths = vec![];
    let mut pen_x = origin.x;
    let baseline = origin.y;
    for ch in content.chars() {
        if ch == '\n' {
            pen_x = origin.x;
            continue;
        }
        if ch == ' ' {
            pen_x += scaled.h_advance(scaled.glyph_id(' ')) + tracking;
            continue;
        }
        let gid = scaled.glyph_id(ch);
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
        pen_x += scaled.h_advance(gid) + tracking;
    }
    if subpaths.is_empty() {
        fallback_block(content, px, origin)
    } else {
        subpaths
    }
}

fn fallback_block(content: &str, px: f32, origin: Pt) -> Vec<Vec<Pt>> {
    let w = px * 0.55 * content.len().max(1) as f32;
    let h = px;
    vec![vec![
        Pt::new(origin.x, origin.y - h),
        Pt::new(origin.x + w, origin.y - h),
        Pt::new(origin.x + w, origin.y),
        Pt::new(origin.x, origin.y),
    ]]
}

pub fn measure(content: &str, px: f32, tracking: f32, font_path: Option<&Path>) -> (f32, f32) {
    let path = font_path
        .map(|p| p.to_path_buf())
        .or_else(default_path);
    let Some(path) = path else {
        return (px * content.len() as f32 * 0.55, px);
    };
    let Some(f) = load_font(&path) else {
        return (px * content.len() as f32 * 0.55, px);
    };
    let scaled = f.as_scaled(ab_glyph::PxScale::from(px.max(1.0)));
    let mut w = 0.0;
    for ch in content.chars() {
        w += scaled.h_advance(scaled.glyph_id(ch)) + tracking;
    }
    (w, scaled.ascent() - scaled.descent())
}

pub fn available() -> bool {
    default_path().is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shapes_when_font_exists() {
        let sub = shape("Ag", 48.0, Pt::new(0.0, 0.0), 0.0, None);
        assert!(!sub.is_empty());
        for sp in &sub {
            assert!(sp.len() >= 3);
        }
        let (w, h) = measure("Ag", 48.0, 0.0, None);
        assert!(w > 10.0 && h > 10.0);
    }
}
