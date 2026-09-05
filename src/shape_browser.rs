//! Shape browser – previews from popular OSS icon libs and pulls SVG on demand.

use crate::geom::{Geom, Pt};

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
    let lineicons = [
        "home", "user", "heart", "star", "cog", "search", "envelope", "bell",
        "calendar", "camera", "comments", "code", "cube", "download", "upload",
        "trash", "pencil", "plus", "minus", "close",
    ];
    let mut out = Vec::new();
    for (n, _) in crate::phosphor_map::ICONS {
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

/// Build geometry for an icon. Phosphor comes from the bundled Light TTF —
/// the same outlines as the tool well — not a reconstructed SVG.
pub fn icon_to_geom(icon: &Icon, target_px: f32) -> Result<Geom, String> {
    if icon.lib == "Phosphor" {
        phosphor_from_font(icon.name, target_px)
            .ok_or_else(|| format!("no Phosphor glyph for {}", icon.name))
    } else {
        svg_to_geom(&fetch_svg(icon)?, target_px)
    }
}

fn phosphor_from_font(name: &str, target_px: f32) -> Option<Geom> {
    use ab_glyph::{Font, FontRef, OutlineCurve};
    let cp = crate::phosphor_map::ICONS
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(_, cp)| *cp)?;
    let ch = char::from_u32(cp)?;
    let font = FontRef::try_from_slice(include_bytes!("../assets/phosphor/Phosphor-Light.ttf")).ok()?;
    let gid = font.glyph_id(ch);
    if gid.0 == 0 {
        return None;
    }
    let outline = font.outline(gid)?;
    let mut contours: Vec<Vec<Pt>> = Vec::new();
    let mut pts: Vec<Pt> = Vec::new();
    let mut last_end: Option<ab_glyph::Point> = None;
    let start_of = |c: &OutlineCurve| match c {
        OutlineCurve::Line(p0, _)
        | OutlineCurve::Quad(p0, _, _)
        | OutlineCurve::Cubic(p0, _, _, _) => *p0,
    };
    let end_of = |c: &OutlineCurve| match c {
        OutlineCurve::Line(_, p1)
        | OutlineCurve::Quad(_, _, p1)
        | OutlineCurve::Cubic(_, _, _, p1) => *p1,
    };
    // Font space is y-up. Our canvas is y-down. Flip Y so the glyph sits upright.
    let map = |p: ab_glyph::Point| Pt::new(p.x, -p.y);
    for c in &outline.curves {
        let s = start_of(c);
        let broke = last_end
            .map(|e| (e.x - s.x).abs() > 0.05 || (e.y - s.y).abs() > 0.05)
            .unwrap_or(true);
        if broke {
            if pts.len() >= 3 {
                contours.push(std::mem::take(&mut pts));
            } else {
                pts.clear();
            }
            pts.push(map(s));
        }
        match *c {
            OutlineCurve::Line(_, p1) => pts.push(map(p1)),
            OutlineCurve::Quad(p0, p1, p2) => {
                for i in 1..=16 {
                    let t = i as f32 / 16.0;
                    let u = 1.0 - t;
                    pts.push(map(ab_glyph::point(
                        u * u * p0.x + 2.0 * u * t * p1.x + t * t * p2.x,
                        u * u * p0.y + 2.0 * u * t * p1.y + t * t * p2.y,
                    )));
                }
            }
            OutlineCurve::Cubic(p0, p1, p2, p3) => {
                for i in 1..=20 {
                    let t = i as f32 / 20.0;
                    pts.push(map(ab_glyph::point(
                        eval_cubic(p0.x, p1.x, p2.x, p3.x, t),
                        eval_cubic(p0.y, p1.y, p2.y, p3.y, t),
                    )));
                }
            }
        }
        last_end = Some(end_of(c));
    }
    if pts.len() >= 3 {
        contours.push(pts);
    }
    if contours.is_empty() {
        return None;
    }
    let mut min = Pt::new(f32::MAX, f32::MAX);
    let mut max = Pt::new(f32::MIN, f32::MIN);
    for c in &contours {
        for p in c {
            min.x = min.x.min(p.x);
            min.y = min.y.min(p.y);
            max.x = max.x.max(p.x);
            max.y = max.y.max(p.y);
        }
    }
    let dim = (max.x - min.x).max(max.y - min.y).max(1.0);
    let scale = target_px / dim;
    for c in &mut contours {
        for p in c {
            p.x = (p.x - min.x) * scale;
            p.y = (p.y - min.y) * scale;
        }
    }
    Some(Geom::Poly {
        contours,
        winding: true,
    })
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

/// viewBox width and height, or 256×256 when the file does not say.
pub fn svg_size(svg: &str) -> (f32, f32) {
    parse_viewbox(svg)
        .map(|(_, _, w, h)| (w.max(1.0), h.max(1.0)))
        .unwrap_or((256.0, 256.0))
}

/// First hex fill that is not `none`. Icons without a fill fall back to the caller.
pub fn svg_fill(svg: &str) -> Option<crate::color::Rgba> {
    let mut pos = 0;
    while let Some(idx) = svg[pos..].find("fill=") {
        let start = pos + idx + 5;
        let rest = &svg[start..];
        let quote = rest.chars().next()?;
        if quote != '"' && quote != '\'' {
            pos = start;
            continue;
        }
        let end = rest[1..].find(quote)?;
        let val = rest[1..1 + end].trim();
        pos = start + 1 + end + 1;
        if val.eq_ignore_ascii_case("none") || val.eq_ignore_ascii_case("transparent") {
            continue;
        }
        if let Some(c) = parse_svg_color(val) {
            return Some(c);
        }
    }
    None
}

fn parse_svg_color(val: &str) -> Option<crate::color::Rgba> {
    let v = val.trim();
    if v.eq_ignore_ascii_case("none") || v.eq_ignore_ascii_case("transparent") {
        return None;
    }
    if v.eq_ignore_ascii_case("black") {
        return Some(crate::color::Rgba::rgb(0, 0, 0));
    }
    if v.eq_ignore_ascii_case("white") {
        return Some(crate::color::Rgba::rgb(255, 255, 255));
    }
    if let Some(hex) = v.strip_prefix('#') {
        return match hex.len() {
            3 => {
                let r = u8::from_str_radix(&hex[0..1].repeat(2), 16).ok()?;
                let g = u8::from_str_radix(&hex[1..2].repeat(2), 16).ok()?;
                let b = u8::from_str_radix(&hex[2..3].repeat(2), 16).ok()?;
                Some(crate::color::Rgba::rgb(r, g, b))
            }
            6 | 8 => {
                let n = u32::from_str_radix(&hex[..6], 16).ok()?;
                Some(crate::color::Rgba::from_hex(n))
            }
            _ => None,
        };
    }
    let lower = v.to_ascii_lowercase();
    if let Some(rest) = lower.strip_prefix("rgba(").or_else(|| lower.strip_prefix("rgb(")) {
        let rest = rest.trim_end_matches(')');
        let parts: Vec<&str> = rest.split(',').map(|s| s.trim()).collect();
        if parts.len() >= 3 {
            let r: f32 = parts[0].parse().ok()?;
            let g: f32 = parts[1].parse().ok()?;
            let b: f32 = parts[2].parse().ok()?;
            let a = if parts.len() >= 4 {
                let t = parts[3];
                if t.ends_with('%') {
                    t.trim_end_matches('%').parse::<f32>().ok()? / 100.0
                } else {
                    t.parse::<f32>().ok()?
                }
            } else {
                1.0
            };
            let to_u8 = |x: f32| x.clamp(0.0, 255.0).round() as u8;
            let mut c = crate::color::Rgba::rgb(to_u8(r), to_u8(g), to_u8(b));
            c.a = (a.clamp(0.0, 1.0) * 255.0).round() as u8;
            return Some(c);
        }
    }
    None
}

/// Paint taken from a single SVG element.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SvgPaint {
    Unspecified,
    None,
    Solid(crate::color::Rgba),
}

/// One drawable pulled out of an SVG, with its own fill/stroke.
#[derive(Clone, Debug)]
pub struct SvgElement {
    pub geom: Geom,
    pub fill: SvgPaint,
    pub stroke: SvgPaint,
    pub stroke_width: f32,
    pub stroke_cap: Option<String>,
    pub stroke_join: Option<String>,
}

fn parse_paint(v: Option<&str>) -> SvgPaint {
    match v.map(str::trim) {
        None | Some("") => SvgPaint::Unspecified,
        Some(s) if s.eq_ignore_ascii_case("none") || s.eq_ignore_ascii_case("transparent") => {
            SvgPaint::None
        }
        Some(s) => parse_svg_color(s)
            .map(SvgPaint::Solid)
            .unwrap_or(SvgPaint::Unspecified),
    }
}

fn tag_attr<'a>(tag: &'a str, name: &str) -> Option<&'a str> {
    for q in ['"', '\''] {
        let needle = format!("{name}={q}");
        if let Some(i) = tag.find(&needle) {
            let from = i + needle.len();
            if let Some(end) = tag[from..].find(q) {
                return Some(&tag[from..from + end]);
            }
        }
    }
    None
}

fn each_open_tag<'a>(svg: &'a str, name: &str) -> Vec<&'a str> {
    let mut out = Vec::new();
    let open = format!("<{name}");
    let mut pos = 0;
    while let Some(idx) = svg[pos..].find(&open) {
        let start = pos + idx;
        let rest = &svg[start..];
        let end = rest.find('>').unwrap_or(rest.len());
        out.push(&rest[..end]);
        pos = start + end.max(1);
    }
    out
}

fn contours_to_geom(contours: Vec<Vec<Pt>>, closed: bool) -> Option<Geom> {
    if contours.is_empty() {
        return None;
    }
    if contours.len() == 1 {
        let mut pts = contours.into_iter().next().unwrap();
        if pts.len() < 2 {
            return None;
        }
        if closed
            && pts.len() > 2
            && (pts[0] - *pts.last().unwrap()).length() < 0.5
        {
            pts.pop();
        }
        let anchors: Vec<crate::geom::Anchor> =
            pts.into_iter().map(crate::geom::Anchor::corner).collect();
        Some(Geom::Path { anchors, closed })
    } else {
        Some(Geom::Poly {
            contours,
            winding: true,
        })
    }
}

fn element_from_path_tag(tag: &str, vb_x: f32, vb_y: f32, scale: f32) -> Option<SvgElement> {
    let d = tag_attr(tag, "d")?;
    let closed = d.bytes().any(|b| b == b'Z' || b == b'z');
    let contours = parse_path_d(d, vb_x, vb_y, scale);
    let geom = contours_to_geom(contours, closed)?;
    let stroke_width = tag_attr(tag, "stroke-width")
        .and_then(|s| s.parse().ok())
        .unwrap_or(0.0)
        * scale;
    Some(SvgElement {
        geom,
        fill: parse_paint(tag_attr(tag, "fill")),
        stroke: parse_paint(tag_attr(tag, "stroke")),
        stroke_width,
        stroke_cap: tag_attr(tag, "stroke-linecap").map(|s| s.to_string()),
        stroke_join: tag_attr(tag, "stroke-linejoin").map(|s| s.to_string()),
    })
}

/// Per-element SVG import. Keeps fill none + stroke instead of smashing
/// every path into one filled poly.
pub fn svg_to_elements(svg: &str) -> Result<Vec<SvgElement>, String> {
    let (vb_x, vb_y, vb_w, vb_h) = parse_viewbox(svg).unwrap_or((0.0, 0.0, 256.0, 256.0));
    let scale = 1.0;
    let _ = (vb_w, vb_h);
    let mut out = Vec::new();
    for tag in each_open_tag(svg, "path") {
        if let Some(el) = element_from_path_tag(tag, vb_x, vb_y, scale) {
            out.push(el);
        }
    }
    if out.is_empty() {
        let g = svg_to_geom(svg, vb_w.max(vb_h).max(1.0))?;
        out.push(SvgElement {
            geom: g,
            fill: SvgPaint::Unspecified,
            stroke: SvgPaint::None,
            stroke_width: 0.0,
            stroke_cap: None,
            stroke_join: None,
        });
    }
    Ok(out)
}

/// Minimal SVG → Geom. Extracts viewBox and all `d="..."`, flattens beziers and
/// scales to a 256×256 artboard square. Returns a Poly so it renders even when
/// path data is imperfect; anchors could be preserved for Node editing in a follow-up.
pub fn svg_to_geom(svg: &str, target_px: f32) -> Result<Geom, String> {
    // viewBox="minX minY w h" – default to 0 0 256 256 (Phosphor's default)
    let (vb_x, vb_y, vb_w, vb_h) = parse_viewbox(svg).unwrap_or((0.0, 0.0, 256.0, 256.0));
    let scale = target_px / vb_w.max(vb_h).max(1.0);
    let mut contours = extract_path_ds(svg)
        .into_iter()
        .flat_map(|d| parse_path_d(&d, vb_x, vb_y, scale))
        .collect::<Vec<Vec<Pt>>>();
    contours.extend(extract_basic_shapes(svg, vb_x, vb_y, scale));
    if contours.is_empty() {
        return Err("no path data found".into());
    }
    Ok(Geom::Poly {
        contours,
        winding: true,
    })
}

fn parse_viewbox(svg: &str) -> Option<(f32, f32, f32, f32)> {
    for needle in ["viewBox=\"", "viewBox='", "viewbox=\"", "viewbox='"] {
        if let Some(start) = svg.find(needle) {
            let q = needle.chars().last()?;
            let from = start + needle.len();
            if let Some(end) = svg[from..].find(q) {
                let vals: Vec<f32> = svg[from..from + end]
                    .split(|c: char| c.is_whitespace() || c == ',')
                    .filter(|s| !s.is_empty())
                    .filter_map(|s| s.parse().ok())
                    .collect();
                if vals.len() == 4 {
                    return Some((vals[0], vals[1], vals[2], vals[3]));
                }
            }
        }
    }
    None
}

fn extract_basic_shapes(svg: &str, vb_x: f32, vb_y: f32, scale: f32) -> Vec<Vec<Pt>> {
    let mut out = Vec::new();
    for tag in ["rect", "circle", "ellipse", "polygon", "polyline", "line"] {
        let mut pos = 0;
        let open = format!("<{tag}");
        while let Some(idx) = svg[pos..].find(&open) {
            let start = pos + idx;
            let rest = &svg[start..];
            let end = rest.find('>').unwrap_or(rest.len());
            let attrs = &rest[..end];
            pos = start + end.max(1);
            if let Some(c) = shape_contour(tag, attrs, vb_x, vb_y, scale) {
                out.push(c);
            }
        }
    }
    out
}

fn attr(tag: &str, name: &str) -> Option<f32> {
    for q in ['"', '\''] {
        let needle = format!("{name}={q}");
        if let Some(i) = tag.find(&needle) {
            let from = i + needle.len();
            if let Some(end) = tag[from..].find(q) {
                return tag[from..from + end].trim().parse().ok();
            }
        }
    }
    None
}

fn attr_str<'a>(tag: &'a str, name: &str) -> Option<&'a str> {
    for q in ['"', '\''] {
        let needle = format!("{name}={q}");
        if let Some(i) = tag.find(&needle) {
            let from = i + needle.len();
            if let Some(end) = tag[from..].find(q) {
                return Some(&tag[from..from + end]);
            }
        }
    }
    None
}

fn shape_contour(kind: &str, tag: &str, vb_x: f32, vb_y: f32, scale: f32) -> Option<Vec<Pt>> {
    let t = |x: f32, y: f32| transform_pt(x, y, vb_x, vb_y, scale);
    match kind {
        "rect" => {
            let x = attr(tag, "x").unwrap_or(0.0);
            let y = attr(tag, "y").unwrap_or(0.0);
            let w = attr(tag, "width")?;
            let h = attr(tag, "height")?;
            Some(vec![t(x, y), t(x + w, y), t(x + w, y + h), t(x, y + h)])
        }
        "circle" => {
            let cx = attr(tag, "cx").unwrap_or(0.0);
            let cy = attr(tag, "cy").unwrap_or(0.0);
            let r = attr(tag, "r")?;
            Some(ellipse_contour(cx, cy, r, r, vb_x, vb_y, scale))
        }
        "ellipse" => {
            let cx = attr(tag, "cx").unwrap_or(0.0);
            let cy = attr(tag, "cy").unwrap_or(0.0);
            let rx = attr(tag, "rx")?;
            let ry = attr(tag, "ry")?;
            Some(ellipse_contour(cx, cy, rx, ry, vb_x, vb_y, scale))
        }
        "polygon" | "polyline" => {
            let pts = attr_str(tag, "points")?;
            let nums: Vec<f32> = pts
                .split(|c: char| c.is_whitespace() || c == ',')
                .filter(|s| !s.is_empty())
                .filter_map(|s| s.parse().ok())
                .collect();
            if nums.len() < 4 {
                return None;
            }
            let mut c = Vec::new();
            for pair in nums.chunks(2) {
                if pair.len() == 2 {
                    c.push(t(pair[0], pair[1]));
                }
            }
            (c.len() >= 2).then_some(c)
        }
        "line" => {
            let x1 = attr(tag, "x1")?;
            let y1 = attr(tag, "y1")?;
            let x2 = attr(tag, "x2")?;
            let y2 = attr(tag, "y2")?;
            Some(vec![t(x1, y1), t(x2, y2)])
        }
        _ => None,
    }
}

fn ellipse_contour(
    cx: f32,
    cy: f32,
    rx: f32,
    ry: f32,
    vb_x: f32,
    vb_y: f32,
    scale: f32,
) -> Vec<Pt> {
    (0..32)
        .map(|i| {
            let a = std::f32::consts::TAU * i as f32 / 32.0;
            transform_pt(cx + rx * a.cos(), cy + ry * a.sin(), vb_x, vb_y, scale)
        })
        .collect()
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

#[derive(Clone, Copy)]
enum LastCurve {
    None,
    Cubic(f32, f32),
    Quad(f32, f32),
}

fn parse_path_d(d: &str, vb_x: f32, vb_y: f32, scale: f32) -> Vec<Vec<Pt>> {
    let tokens = tokenize(d);
    let mut contours: Vec<Vec<Pt>> = Vec::new();
    let mut cur = Vec::new();
    let mut cx = 0.0f32;
    let mut cy = 0.0f32;
    let mut sx = 0.0f32;
    let mut sy = 0.0f32;
    let mut last = LastCurve::None;
    let mut i = 0;
    let mut cmd = 'M';

    let push = |cur: &mut Vec<Pt>, x: f32, y: f32| {
        cur.push(transform_pt(x, y, vb_x, vb_y, scale));
    };
    let flush = |contours: &mut Vec<Vec<Pt>>, cur: &mut Vec<Pt>| {
        if cur.len() >= 2 {
            contours.push(std::mem::take(cur));
        } else {
            cur.clear();
        }
    };

    while i < tokens.len() {
        let t = &tokens[i];
        if t.len() == 1 && t.chars().next().is_some_and(|c| c.is_ascii_alphabetic()) {
            cmd = t.chars().next().unwrap();
            i += 1;
            if cmd == 'Z' || cmd == 'z' {
                if cur.len() >= 2 {
                    contours.push(std::mem::take(&mut cur));
                } else {
                    cur.clear();
                }
                cx = sx;
                cy = sy;
                last = LastCurve::None;
            }
            continue;
        }
        match cmd {
            'M' | 'm' => {
                if i + 1 >= tokens.len() {
                    break;
                }
                let x: f32 = tokens[i].parse().unwrap_or(0.0);
                let y: f32 = tokens[i + 1].parse().unwrap_or(0.0);
                i += 2;
                if cmd == 'M' {
                    cx = x;
                    cy = y;
                } else {
                    cx += x;
                    cy += y;
                }
                sx = cx;
                sy = cy;
                flush(&mut contours, &mut cur);
                push(&mut cur, cx, cy);
                last = LastCurve::None;
                cmd = if cmd == 'M' { 'L' } else { 'l' };
            }
            'L' | 'l' => {
                if i + 1 >= tokens.len() {
                    break;
                }
                let x: f32 = tokens[i].parse().unwrap_or(0.0);
                let y: f32 = tokens[i + 1].parse().unwrap_or(0.0);
                i += 2;
                if cmd == 'L' {
                    cx = x;
                    cy = y;
                } else {
                    cx += x;
                    cy += y;
                }
                push(&mut cur, cx, cy);
                last = LastCurve::None;
            }
            'H' | 'h' => {
                let x: f32 = tokens[i].parse().unwrap_or(0.0);
                i += 1;
                if cmd == 'H' {
                    cx = x;
                } else {
                    cx += x;
                }
                push(&mut cur, cx, cy);
                last = LastCurve::None;
            }
            'V' | 'v' => {
                let y: f32 = tokens[i].parse().unwrap_or(0.0);
                i += 1;
                if cmd == 'V' {
                    cy = y;
                } else {
                    cy += y;
                }
                push(&mut cur, cx, cy);
                last = LastCurve::None;
            }
            'C' | 'c' => {
                if i + 5 >= tokens.len() {
                    break;
                }
                let mut x1: f32 = tokens[i].parse().unwrap_or(0.0);
                let mut y1: f32 = tokens[i + 1].parse().unwrap_or(0.0);
                let mut x2: f32 = tokens[i + 2].parse().unwrap_or(0.0);
                let mut y2: f32 = tokens[i + 3].parse().unwrap_or(0.0);
                let mut x: f32 = tokens[i + 4].parse().unwrap_or(0.0);
                let mut y: f32 = tokens[i + 5].parse().unwrap_or(0.0);
                i += 6;
                if cmd == 'c' {
                    x1 += cx;
                    y1 += cy;
                    x2 += cx;
                    y2 += cy;
                    x += cx;
                    y += cy;
                }
                flatten_cubic(&mut cur, cx, cy, x1, y1, x2, y2, x, y, vb_x, vb_y, scale);
                cx = x;
                cy = y;
                last = LastCurve::Cubic(x2, y2);
            }
            'S' | 's' => {
                if i + 3 >= tokens.len() {
                    break;
                }
                let mut x2: f32 = tokens[i].parse().unwrap_or(0.0);
                let mut y2: f32 = tokens[i + 1].parse().unwrap_or(0.0);
                let mut x: f32 = tokens[i + 2].parse().unwrap_or(0.0);
                let mut y: f32 = tokens[i + 3].parse().unwrap_or(0.0);
                i += 4;
                if cmd == 's' {
                    x2 += cx;
                    y2 += cy;
                    x += cx;
                    y += cy;
                }
                let (x1, y1) = match last {
                    LastCurve::Cubic(lx, ly) => (2.0 * cx - lx, 2.0 * cy - ly),
                    _ => (cx, cy),
                };
                flatten_cubic(&mut cur, cx, cy, x1, y1, x2, y2, x, y, vb_x, vb_y, scale);
                cx = x;
                cy = y;
                last = LastCurve::Cubic(x2, y2);
            }
            'Q' | 'q' => {
                if i + 3 >= tokens.len() {
                    break;
                }
                let mut x1: f32 = tokens[i].parse().unwrap_or(0.0);
                let mut y1: f32 = tokens[i + 1].parse().unwrap_or(0.0);
                let mut x: f32 = tokens[i + 2].parse().unwrap_or(0.0);
                let mut y: f32 = tokens[i + 3].parse().unwrap_or(0.0);
                i += 4;
                if cmd == 'q' {
                    x1 += cx;
                    y1 += cy;
                    x += cx;
                    y += cy;
                }
                flatten_quad(&mut cur, cx, cy, x1, y1, x, y, vb_x, vb_y, scale);
                cx = x;
                cy = y;
                last = LastCurve::Quad(x1, y1);
            }
            'T' | 't' => {
                if i + 1 >= tokens.len() {
                    break;
                }
                let mut x: f32 = tokens[i].parse().unwrap_or(0.0);
                let mut y: f32 = tokens[i + 1].parse().unwrap_or(0.0);
                i += 2;
                if cmd == 't' {
                    x += cx;
                    y += cy;
                }
                let (x1, y1) = match last {
                    LastCurve::Quad(lx, ly) => (2.0 * cx - lx, 2.0 * cy - ly),
                    _ => (cx, cy),
                };
                flatten_quad(&mut cur, cx, cy, x1, y1, x, y, vb_x, vb_y, scale);
                cx = x;
                cy = y;
                last = LastCurve::Quad(x1, y1);
            }
            'A' | 'a' => {
                if i + 6 >= tokens.len() {
                    break;
                }
                let rx: f32 = tokens[i].parse().unwrap_or(0.0);
                let ry: f32 = tokens[i + 1].parse().unwrap_or(0.0);
                let rot: f32 = tokens[i + 2].parse().unwrap_or(0.0);
                let large = tokens[i + 3].parse().unwrap_or(0.0) != 0.0;
                let sweep = tokens[i + 4].parse().unwrap_or(0.0) != 0.0;
                let mut x: f32 = tokens[i + 5].parse().unwrap_or(0.0);
                let mut y: f32 = tokens[i + 6].parse().unwrap_or(0.0);
                i += 7;
                if cmd == 'a' {
                    x += cx;
                    y += cy;
                }
                for (ax, ay) in svg_arc(cx, cy, rx, ry, rot, large, sweep, x, y) {
                    push(&mut cur, ax, ay);
                }
                cx = x;
                cy = y;
                last = LastCurve::None;
            }
            _ => {
                i += 1;
            }
        }
    }
    flush(&mut contours, &mut cur);
    contours
}

fn flatten_cubic(
    cur: &mut Vec<Pt>,
    x0: f32,
    y0: f32,
    x1: f32,
    y1: f32,
    x2: f32,
    y2: f32,
    x3: f32,
    y3: f32,
    vb_x: f32,
    vb_y: f32,
    scale: f32,
) {
    for t in 1..=16 {
        let tt = t as f32 / 16.0;
        cur.push(transform_pt(
            eval_cubic(x0, x1, x2, x3, tt),
            eval_cubic(y0, y1, y2, y3, tt),
            vb_x,
            vb_y,
            scale,
        ));
    }
}

fn flatten_quad(
    cur: &mut Vec<Pt>,
    x0: f32,
    y0: f32,
    x1: f32,
    y1: f32,
    x2: f32,
    y2: f32,
    vb_x: f32,
    vb_y: f32,
    scale: f32,
) {
    for t in 1..=12 {
        let tt = t as f32 / 12.0;
        let u = 1.0 - tt;
        cur.push(transform_pt(
            u * u * x0 + 2.0 * u * tt * x1 + tt * tt * x2,
            u * u * y0 + 2.0 * u * tt * y1 + tt * tt * y2,
            vb_x,
            vb_y,
            scale,
        ));
    }
}

/// SVG elliptical arc → sampled points, endpoint included. Spec: implnote.html#ArcImplementationNotes
fn svg_arc(
    x1: f32,
    y1: f32,
    mut rx: f32,
    mut ry: f32,
    phi_deg: f32,
    large: bool,
    sweep: bool,
    x2: f32,
    y2: f32,
) -> Vec<(f32, f32)> {
    rx = rx.abs();
    ry = ry.abs();
    if rx < 1e-6 || ry < 1e-6 || ((x1 - x2).abs() < 1e-6 && (y1 - y2).abs() < 1e-6) {
        return vec![(x2, y2)];
    }
    let phi = phi_deg.to_radians();
    let (sin_phi, cos_phi) = phi.sin_cos();
    let dx = (x1 - x2) * 0.5;
    let dy = (y1 - y2) * 0.5;
    let x1p = cos_phi * dx + sin_phi * dy;
    let y1p = -sin_phi * dx + cos_phi * dy;

    let mut rx2 = rx * rx;
    let mut ry2 = ry * ry;
    let x1p2 = x1p * x1p;
    let y1p2 = y1p * y1p;
    let lam = x1p2 / rx2 + y1p2 / ry2;
    if lam > 1.0 {
        let s = lam.sqrt();
        rx *= s;
        ry *= s;
        rx2 = rx * rx;
        ry2 = ry * ry;
    }
    let sign = if large == sweep { -1.0 } else { 1.0 };
    let num = (rx2 * ry2 - rx2 * y1p2 - ry2 * x1p2).max(0.0);
    let den = rx2 * y1p2 + ry2 * x1p2;
    let coef = if den < 1e-12 {
        0.0
    } else {
        sign * (num / den).sqrt()
    };
    let cxp = coef * (rx * y1p / ry);
    let cyp = coef * -(ry * x1p / rx);
    let cx = cos_phi * cxp - sin_phi * cyp + (x1 + x2) * 0.5;
    let cy = sin_phi * cxp + cos_phi * cyp + (y1 + y2) * 0.5;

    let ux = (x1p - cxp) / rx;
    let uy = (y1p - cyp) / ry;
    let vx = (-x1p - cxp) / rx;
    let vy = (-y1p - cyp) / ry;
    let theta1 = vec_angle(1.0, 0.0, ux, uy);
    let mut dtheta = vec_angle(ux, uy, vx, vy);
    if !sweep && dtheta > 0.0 {
        dtheta -= 2.0 * std::f32::consts::PI;
    }
    if sweep && dtheta < 0.0 {
        dtheta += 2.0 * std::f32::consts::PI;
    }

    let n = ((dtheta.abs() / (std::f32::consts::PI / 16.0)).ceil() as usize).clamp(6, 96);
    let mut pts = Vec::with_capacity(n);
    for i in 1..=n {
        let th = theta1 + dtheta * (i as f32 / n as f32);
        let (st, ct) = th.sin_cos();
        pts.push((
            cos_phi * rx * ct - sin_phi * ry * st + cx,
            sin_phi * rx * ct + cos_phi * ry * st + cy,
        ));
    }
    pts
}

fn vec_angle(ux: f32, uy: f32, vx: f32, vy: f32) -> f32 {
    let n = ((ux * ux + uy * uy) * (vx * vx + vy * vy)).sqrt();
    if n < 1e-12 {
        return 0.0;
    }
    let mut a = ((ux * vx + uy * vy) / n).clamp(-1.0, 1.0).acos();
    if ux * vy - uy * vx < 0.0 {
        a = -a;
    }
    a
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
        if ch.is_ascii_alphabetic() && ch != 'e' && ch != 'E' {
            if !cur.is_empty() {
                out.push(std::mem::take(&mut cur));
            }
            out.push(ch.to_string());
        } else if ch == '-' || ch == '+' || ch == '.' || ch.is_ascii_digit() || ch == 'e' || ch == 'E'
        {
            if ch == '-' && !cur.is_empty() && !cur.ends_with('e') && !cur.ends_with('E') {
                out.push(std::mem::take(&mut cur));
                cur.push(ch);
            } else if ch == '.' && cur.contains('.') && !cur.contains('e') && !cur.contains('E') {
                out.push(std::mem::take(&mut cur));
                cur.push(ch);
            } else {
                cur.push(ch);
            }
        } else if !cur.is_empty() {
            out.push(std::mem::take(&mut cur));
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
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
            Geom::Poly { contours, .. } => assert_eq!(contours.len(), 1),
            _ => panic!("poly"),
        }
    }

    #[test]
    fn parse_rgba_and_hex() {
        let a = parse_svg_color("rgba(0,0,0,1.000)").unwrap();
        assert_eq!((a.r, a.g, a.b, a.a), (0, 0, 0, 255));
        let b = parse_svg_color("#4F8CFF").unwrap();
        assert_eq!((b.r, b.g, b.b), (0x4F, 0x8C, 0xFF));
        assert!(parse_svg_color("none").is_none());
    }

    #[test]
    fn svg_elements_keep_stroke_and_fill_none() {
        let svg = r##"<svg viewBox="0 0 100 100">
            <path d="M10 10 L90 10 L90 90" fill="none" stroke="#000000" stroke-width="8"/>
            <path d="M20 20 L40 20 L40 40 L20 40 Z" fill="#112233"/>
        </svg>"##;
        let els = svg_to_elements(svg).unwrap();
        assert_eq!(els.len(), 2);
        assert_eq!(els[0].fill, SvgPaint::None);
        assert_eq!(els[0].stroke, SvgPaint::Solid(crate::color::Rgba::rgb(0, 0, 0)));
        assert!((els[0].stroke_width - 8.0).abs() < 0.01);
        assert_eq!(
            els[1].fill,
            SvgPaint::Solid(crate::color::Rgba::rgb(0x11, 0x22, 0x33))
        );
        assert_eq!(els[1].stroke, SvgPaint::Unspecified);
    }

    #[test]
    fn svg_rect_without_path_still_places() {
        let svg = r#"<svg viewBox="0 0 20 10"><rect x="1" y="2" width="8" height="6"/></svg>"#;
        let g = svg_to_geom(svg, 20.0).unwrap();
        let Geom::Poly { contours, .. } = g else {
            panic!("poly");
        };
        assert_eq!(contours.len(), 1);
        assert_eq!(contours[0].len(), 4);
    }

    #[test]
    fn tokenize_glued_minus_and_decimals() {
        let t = tokenize("A6,6,0,0,0-13.44,0");
        assert_eq!(t, vec!["A", "6", "6", "0", "0", "0", "-13.44", "0"]);
        let t2 = tokenize("M0.5.5");
        assert_eq!(t2, vec!["M", "0.5", ".5"]);
    }

    #[test]
    fn arc_is_not_a_straight_cut() {
        // Phosphor lens: large circular arc, not a line to the endpoint.
        let d = "M128 90 a42 42 0 1 0 42 42";
        let cs = parse_path_d(d, 0.0, 0.0, 1.0);
        assert_eq!(cs.len(), 1);
        assert!(
            cs[0].len() > 8,
            "arc must flatten to many points, got {}",
            cs[0].len()
        );
        let min_x = cs[0].iter().map(|p| p.x).fold(f32::MAX, f32::min);
        let max_x = cs[0].iter().map(|p| p.x).fold(f32::MIN, f32::max);
        let min_y = cs[0].iter().map(|p| p.y).fold(f32::MAX, f32::min);
        let max_y = cs[0].iter().map(|p| p.y).fold(f32::MIN, f32::max);
        assert!(
            max_x - min_x > 40.0 && max_y - min_y > 40.0,
            "arc bbox {}x{} looks like a chopped line",
            max_x - min_x,
            max_y - min_y
        );
    }

    #[test]
    fn phosphor_font_camera_is_a_real_glyph() {
        let g = phosphor_from_font("camera", 256.0).expect("camera in Phosphor-Light");
        match g {
            Geom::Poly { contours, winding } => {
                assert!(winding, "font glyphs use nonzero winding");
                assert!(
                    contours.len() >= 2,
                    "camera should have body + lens, got {}",
                    contours.len()
                );
            }
            _ => panic!("poly"),
        }
    }

    #[test]
    fn phosphor_camera_has_body_and_lens() {
        let svg = r#"<svg viewBox="0 0 256 256"><path d="M208,58H179.21L165,36.67A6,6,0,0,0,160,34H96a6,6,0,0,0-5,2.67L76.78,58H48A22,22,0,0,0,26,80V192a22,22,0,0,0,22,22H208a22,22,0,0,0,22-22V80A22,22,0,0,0,208,58Zm10,134a10,10,0,0,1-10,10H48a10,10,0,0,1-10-10V80A10,10,0,0,1,48,70H80a6,6,0,0,0,5-2.67L99.21,46h57.57L171,67.33A6,6,0,0,0,176,70h32a10,10,0,0,1,10,10ZM128,90a42,42,0,1,0,42,42A42,42,0,0,0,128,90Zm0,72a30,30,0,1,1,30-30A30,30,0,0,1,128,162Z"/></svg>"#;
        let g = svg_to_geom(svg, 256.0).unwrap();
        match g {
            Geom::Poly { contours, .. } => {
                assert_eq!(contours.len(), 4, "outer, inner, lens outer, lens inner");
                let b = contours[2].iter().fold(
                    (
                        f32::MAX,
                        f32::MAX,
                        f32::MIN,
                        f32::MIN,
                    ),
                    |(a, b, c, d), p| (a.min(p.x), b.min(p.y), c.max(p.x), d.max(p.y)),
                );
                let w = b.2 - b.0;
                let h = b.3 - b.1;
                assert!(
                    w > 70.0 && h > 70.0,
                    "lens should be a circle ~84px, got {w}x{h}"
                );
            }
            _ => panic!("poly"),
        }
    }
}
