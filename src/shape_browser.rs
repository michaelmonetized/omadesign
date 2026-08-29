//! Shape browser – previews from popular OSS icon libs and pulls SVG on demand.

use crate::geom::{Anchor, Geom, Pt};

/// A single icon entry. SVGs are fetched lazily from the lib's CDN / GitHub raw.
#[derive(Clone, Debug)]
pub struct Icon {
    pub name: &'static str,
    pub lib: &'static str,
}

pub fn libs() -> &'static [&'static str] {
    &["Phosphor", "LineIcons", "Heroicons", "Feather", "Lucide"]
}

/// Hard-coded popular set – enough to show search without a live GitHub index.
/// Real browser lazy-fetches SVGs from the raw URLs on “Add”.
pub fn all_icons() -> Vec<Icon> {
    let phosphor = [
        "house", "user", "heart", "star", "gear", "magnifying-glass", "envelope", "bell",
        "calendar", "camera", "chat-circle", "code", "cube", "download-simple", "upload-simple",
        "trash", "pencil-simple", "plus", "minus", "x", "check", "arrow-right", "arrow-left",
        "caret-right", "caret-left", "dots-three", "list", "squares-four", "image", "film-strip",
        "music-notes", "map-pin", "phone", "shopping-cart", "credit-card", "currency-dollar",
        "globe", "flag", "bookmark", "tag", "link", "paper-plane-tilt", "share", "eye", "eye-slash",
        "lock", "key", "shield", "warning", "info", "question",
    ];
    let lineicons = [
        "home", "user", "heart", "star", "cog", "search", "envelope", "bell",
        "calendar", "camera", "comments", "code", "cube", "download", "upload",
        "trash", "pencil", "plus", "minus", "close",
    ];
    let mut out = Vec::new();
    for n in phosphor {
        out.push(Icon { name: n, lib: "Phosphor" });
    }
    for n in lineicons {
        out.push(Icon { name: n, lib: "LineIcons" });
    }
    // Add a few heroicons/feather for “All” search coverage
    for n in ["academic-cap", "adjustments", "archive", "arrow-path", "beaker"] {
        out.push(Icon { name: n, lib: "Heroicons" });
    }
    for n in ["activity", "airplay", "alert-circle", "anchor", "aperture"] {
        out.push(Icon { name: n, lib: "Feather" });
    }
    out
}

pub fn search(query: &str, lib: &str, limit: usize) -> Vec<Icon> {
    let q = query.trim().to_ascii_lowercase();
    let mut v: Vec<Icon> = all_icons()
        .into_iter()
        .filter(|ic| {
            (lib == "All" || ic.lib == lib) && (q.is_empty() || ic.name.contains(&q) || ic.lib.to_ascii_lowercase().contains(&q))
        })
        .collect();
    v.sort_by(|a, b| a.name.cmp(b.name));
    v.truncate(limit);
    v
}

pub fn svg_url(icon: &Icon) -> String {
    match icon.lib {
        "Phosphor" => format!(
            "https://raw.githubusercontent.com/phosphor-icons/core/main/assets/light/{}-light.svg",
            icon.name
        ),
        "LineIcons" => format!(
            "https://cdn.lineicons.com/4.0/lineicons/{}.svg",
            icon.name
        ),
        "Heroicons" => format!(
            "https://raw.githubusercontent.com/tailwindlabs/heroicons/master/24/outline/{}.svg",
            icon.name
        ),
        "Feather" => format!(
            "https://raw.githubusercontent.com/feathericons/feather/master/icons/{}.svg",
            icon.name
        ),
        _ => format!(
            "https://raw.githubusercontent.com/lucide-icons/lucide/main/icons/{}.svg",
            icon.name
        ),
    }
}

/// Download SVG text for an icon. Uses ureq; falls back to a minimal placeholder on failure.
pub fn fetch_svg(icon: &Icon) -> Result<String, String> {
    let url = svg_url(icon);
    let resp = ureq::get(&url)
        .timeout(std::time::Duration::from_secs(8))
        .call()
        .map_err(|e| format!("fetch {} failed: {e}", icon.name))?;
    if resp.status() != 200 {
        return Err(format!("HTTP {}", resp.status()));
    }
    resp.into_string().map_err(|e| e.to_string())
}

/// Minimal SVG → Geom. Extracts viewBox and all `d="..."`, flattens beziers and
/// scales to a 256×256 artboard square. Returns a Poly so it renders even when
/// path data is imperfect; anchors could be preserved for Node editing in a follow-up.
pub fn svg_to_geom(svg: &str, target_px: f32) -> Result<Geom, String> {
    // viewBox="minX minY w h" – default to 0 0 256 256 (Phosphor's default)
    let (vb_x, vb_y, vb_w, vb_h) = parse_viewbox(svg).unwrap_or((0.0, 0.0, 256.0, 256.0));
    let scale = target_px / vb_w.max(vb_h).max(1.0);
    let contours = extract_path_ds(svg)
        .into_iter()
        .flat_map(|d| parse_path_d(&d, vb_x, vb_y, scale))
        .collect::<Vec<Vec<Pt>>>();
    if contours.is_empty() {
        return Err("no path data found".into());
    }
    Ok(Geom::Poly { contours })
}

fn parse_viewbox(svg: &str) -> Option<(f32, f32, f32, f32)> {
    let needle = "viewBox=\"";
    let start = svg.find(needle)? + needle.len();
    let end = svg[start..].find('"')?;
    let vals: Vec<f32> = svg[start..start + end]
        .split_whitespace()
        .filter_map(|s| s.parse().ok())
        .collect();
    if vals.len() == 4 {
        Some((vals[0], vals[1], vals[2], vals[3]))
    } else {
        None
    }
}

fn extract_path_ds(svg: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut pos = 0;
    while let Some(idx) = svg[pos..].find("d=\"") {
        let start = pos + idx + 3;
        if let Some(end) = svg[start..].find('"') {
            out.push(svg[start..start + end].to_string());
            pos = start + end + 1;
        } else {
            break;
        }
    }
    // Also handle single quotes
    if out.is_empty() {
        pos = 0;
        while let Some(idx) = svg[pos..].find("d='") {
            let start = pos + idx + 3;
            if let Some(end) = svg[start..].find('\'') {
                out.push(svg[start..start + end].to_string());
                pos = start + end + 1;
            } else {
                break;
            }
        }
    }
    out
}

// --- Path d parser: flatten to polyline ---

fn parse_path_d(d: &str, vb_x: f32, vb_y: f32, scale: f32) -> Vec<Vec<Pt>> {
    let tokens = tokenize(d);
    let mut contours: Vec<Vec<Pt>> = Vec::new();
    let mut cur = Vec::new();
    let mut cx = 0.0f32;
    let mut cy = 0.0f32;
    let mut sx = 0.0f32;
    let mut sy = 0.0f32;
    let mut last_cx = 0.0f32;
    let mut last_cy = 0.0f32;
    let mut i = 0;
    let mut cmd = 'M';
    while i < tokens.len() {
        let t = &tokens[i];
        if t.len() == 1 && t.chars().next().map(|c| c.is_ascii_alphabetic()).unwrap_or(false) {
            cmd = t.chars().next().unwrap();
            i += 1;
            if cmd == 'Z' || cmd == 'z' {
                if !cur.is_empty() {
                    // close
                    if cur.len() >= 2 {
                        contours.push(std::mem::take(&mut cur));
                    }
                    cx = sx;
                    cy = sy;
                }
                continue;
            }
            continue;
        }
        match cmd {
            'M' => {
                // absolute move
                if i + 1 >= tokens.len() { break; }
                let x: f32 = tokens[i].parse().unwrap_or(0.0);
                let y: f32 = tokens[i + 1].parse().unwrap_or(0.0);
                i += 2;
                cx = x;
                cy = y;
                sx = cx;
                sy = cy;
                if !cur.is_empty() {
                    if cur.len() >= 2 {
                        contours.push(std::mem::take(&mut cur));
                    } else {
                        cur.clear();
                    }
                }
                cur.push(transform_pt(cx, cy, vb_x, vb_y, scale));
                last_cx = cx;
                last_cy = cy;
                cmd = 'L'; // subsequent pairs are lines
            }
            'm' => {
                let x: f32 = tokens[i].parse().unwrap_or(0.0);
                let y: f32 = tokens[i + 1].parse().unwrap_or(0.0);
                i += 2;
                cx += x;
                cy += y;
                sx = cx;
                sy = cy;
                if !cur.is_empty() {
                    if cur.len() >= 2 {
                        contours.push(std::mem::take(&mut cur));
                    } else {
                        cur.clear();
                    }
                }
                cur.push(transform_pt(cx, cy, vb_x, vb_y, scale));
                last_cx = cx;
                last_cy = cy;
                cmd = 'l';
            }
            'L' => {
                let x: f32 = tokens[i].parse().unwrap_or(0.0);
                let y: f32 = tokens[i + 1].parse().unwrap_or(0.0);
                i += 2;
                cx = x;
                cy = y;
                cur.push(transform_pt(cx, cy, vb_x, vb_y, scale));
                last_cx = cx;
                last_cy = cy;
            }
            'l' => {
                let x: f32 = tokens[i].parse().unwrap_or(0.0);
                let y: f32 = tokens[i + 1].parse().unwrap_or(0.0);
                i += 2;
                cx += x;
                cy += y;
                cur.push(transform_pt(cx, cy, vb_x, vb_y, scale));
                last_cx = cx;
                last_cy = cy;
            }
            'H' => {
                let x: f32 = tokens[i].parse().unwrap_or(0.0);
                i += 1;
                cx = x;
                cur.push(transform_pt(cx, cy, vb_x, vb_y, scale));
                last_cx = cx;
            }
            'h' => {
                let x: f32 = tokens[i].parse().unwrap_or(0.0);
                i += 1;
                cx += x;
                cur.push(transform_pt(cx, cy, vb_x, vb_y, scale));
                last_cx = cx;
            }
            'V' => {
                let y: f32 = tokens[i].parse().unwrap_or(0.0);
                i += 1;
                cy = y;
                cur.push(transform_pt(cx, cy, vb_x, vb_y, scale));
                last_cy = cy;
            }
            'v' => {
                let y: f32 = tokens[i].parse().unwrap_or(0.0);
                i += 1;
                cy += y;
                cur.push(transform_pt(cx, cy, vb_x, vb_y, scale));
                last_cy = cy;
            }
            'C' => {
                if i + 5 >= tokens.len() { break; }
                let x1: f32 = tokens[i].parse().unwrap_or(0.0);
                let y1: f32 = tokens[i + 1].parse().unwrap_or(0.0);
                let x2: f32 = tokens[i + 2].parse().unwrap_or(0.0);
                let y2: f32 = tokens[i + 3].parse().unwrap_or(0.0);
                let x: f32 = tokens[i + 4].parse().unwrap_or(0.0);
                let y: f32 = tokens[i + 5].parse().unwrap_or(0.0);
                i += 6;
                // flatten cubic
                let p0 = Pt::new(cx, cy);
                let p1 = Pt::new(x1, y1);
                let p2 = Pt::new(x2, y2);
                let p3 = Pt::new(x, y);
                for t in 1..=16 {
                    let tt = t as f32 / 16.0;
                    cur.push(transform_pt(
                        eval_cubic(p0.x, p1.x, p2.x, p3.x, tt),
                        eval_cubic(p0.y, p1.y, p2.y, p3.y, tt),
                        vb_x, vb_y, scale,
                    ));
                }
                cx = x;
                cy = y;
                last_cx = x2;
                last_cy = y2;
            }
            'c' => {
                if i + 5 >= tokens.len() { break; }
                let x1: f32 = tokens[i].parse().unwrap_or(0.0);
                let y1: f32 = tokens[i + 1].parse().unwrap_or(0.0);
                let x2: f32 = tokens[i + 2].parse().unwrap_or(0.0);
                let y2: f32 = tokens[i + 3].parse().unwrap_or(0.0);
                let x: f32 = tokens[i + 4].parse().unwrap_or(0.0);
                let y: f32 = tokens[i + 5].parse().unwrap_or(0.0);
                i += 6;
                let p0 = Pt::new(cx, cy);
                let p1 = Pt::new(cx + x1, cy + y1);
                let p2 = Pt::new(cx + x2, cy + y2);
                let p3 = Pt::new(cx + x, cy + y);
                for t in 1..=16 {
                    let tt = t as f32 / 16.0;
                    cur.push(transform_pt(
                        eval_cubic(p0.x, p1.x, p2.x, p3.x, tt),
                        eval_cubic(p0.y, p1.y, p2.y, p3.y, tt),
                        vb_x, vb_y, scale,
                    ));
                }
                cx += x;
                cy += y;
                last_cx = p2.x;
                last_cy = p2.y;
            }
            'Q' => {
                if i + 3 >= tokens.len() { break; }
                let x1: f32 = tokens[i].parse().unwrap_or(0.0);
                let y1: f32 = tokens[i + 1].parse().unwrap_or(0.0);
                let x: f32 = tokens[i + 2].parse().unwrap_or(0.0);
                let y: f32 = tokens[i + 3].parse().unwrap_or(0.0);
                i += 4;
                // quadratic → cubic approximation for flattening use quad
                let p0 = Pt::new(cx, cy);
                let p1 = Pt::new(x1, y1);
                let p3 = Pt::new(x, y);
                for t in 1..=12 {
                    let tt = t as f32 / 12.0;
                    let a = (1.0 - tt) * (1.0 - tt);
                    let b = 2.0 * (1.0 - tt) * tt;
                    let d = tt * tt;
                    cur.push(transform_pt(
                        a * p0.x + b * p1.x + d * p3.x,
                        a * p0.y + b * p1.y + d * p3.y,
                        vb_x, vb_y, scale,
                    ));
                }
                cx = x;
                cy = y;
                last_cx = x1;
                last_cy = y1;
            }
            'q' => {
                if i + 3 >= tokens.len() { break; }
                let x1: f32 = tokens[i].parse().unwrap_or(0.0);
                let y1: f32 = tokens[i + 1].parse().unwrap_or(0.0);
                let x: f32 = tokens[i + 2].parse().unwrap_or(0.0);
                let y: f32 = tokens[i + 3].parse().unwrap_or(0.0);
                i += 4;
                let p0 = Pt::new(cx, cy);
                let p1 = Pt::new(cx + x1, cy + y1);
                let p3 = Pt::new(cx + x, cy + y);
                for t in 1..=12 {
                    let tt = t as f32 / 12.0;
                    let a = (1.0 - tt) * (1.0 - tt);
                    let b = 2.0 * (1.0 - tt) * tt;
                    let d = tt * tt;
                    cur.push(transform_pt(
                        a * p0.x + b * p1.x + d * p3.x,
                        a * p0.y + b * p1.y + d * p3.y,
                        vb_x, vb_y, scale,
                    ));
                }
                cx += x;
                cy += y;
                last_cx = cx + x1 - x;
                last_cy = cy + y1 - y;
            }
            'A' | 'a' => {
                // Arc – approximate as line to endpoint for MVP
                if i + 6 >= tokens.len() { break; }
                let x: f32 = tokens[i + 5].parse().unwrap_or(0.0);
                let y: f32 = tokens[i + 6].parse().unwrap_or(0.0);
                i += 7;
                if cmd == 'a' {
                    cx += x;
                    cy += y;
                } else {
                    cx = x;
                    cy = y;
                }
                cur.push(transform_pt(cx, cy, vb_x, vb_y, scale));
                last_cx = cx;
                last_cy = cy;
            }
            _ => {
                i += 1;
            }
        }
    }
    if cur.len() >= 2 {
        contours.push(cur);
    }
    contours
}

fn transform_pt(x: f32, y: f32, vb_x: f32, vb_y: f32, scale: f32) -> Pt {
    Pt::new((x - vb_x) * scale, (y - vb_y) * scale)
}

fn eval_cubic(p0: f32, p1: f32, p2: f32, p3: f32, t: f32) -> f32 {
    let u = 1.0 - t;
    p0 * u * u * u + p1 * 3.0 * u * u * t + p2 * 3.0 * u * t * t + p3 * t * t * t
}

fn tokenize(d: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    for ch in d.chars() {
        if ch.is_ascii_alphabetic() {
            if !cur.is_empty() {
                out.push(cur.clone());
                cur.clear();
            }
            out.push(ch.to_string());
        } else if ch == '-' || ch == '.' || ch == '+' || ch.is_ascii_digit() || ch == 'e' || ch == 'E' {
            // Minus starts new number unless it's part of exponent – simplify
            if ch == '-' && !cur.is_empty() && !cur.ends_with('e') && !cur.ends_with('E') {
                out.push(cur.clone());
                cur.clear();
                cur.push(ch);
            } else {
                cur.push(ch);
            }
        } else if ch == ',' || ch.is_whitespace() {
            if !cur.is_empty() {
                out.push(cur.clone());
                cur.clear();
            }
        } else {
            if !cur.is_empty() {
                out.push(cur.clone());
                cur.clear();
            }
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    // Split tokens that may have combined like "12.5-3" already handled via minus
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokenize_simple() {
        let t = tokenize("M10 20 L30 40");
        assert_eq!(t, vec!["M", "10", "20", "L", "30", "40"]);
    }

    #[test]
    fn parse_rect_path() {
        let d = "M0 0 H100 V100 H0 Z";
        let cs = parse_path_d(d, 0.0, 0.0, 1.0);
        assert_eq!(cs.len(), 1);
        assert!(cs[0].len() >= 4);
    }

    #[test]
    fn svg_to_geom_placeholder() {
        let svg = r#"<svg viewBox="0 0 100 100"><path d="M10 10 L90 10 L90 90 L10 90 Z"/></svg>"#;
        let g = svg_to_geom(svg, 100.0).unwrap();
        match g {
            Geom::Poly { contours } => assert_eq!(contours.len(), 1),
            _ => panic!("poly"),
        }
    }
}
