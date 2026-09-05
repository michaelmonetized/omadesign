//! SVG export of the current document.

use crate::color::Rgba;
use crate::document::{Document, Fill, Layer, LayerKind, Shape};
use crate::geom::{Bounds, Geom, Pt};

fn path_data(shape: &Shape) -> String {
    match &shape.geom {
        Geom::Path { anchors, closed } => crate::geom::path_svg_d(anchors, *closed),
        Geom::Line { a, b } => format!("M {:.3} {:.3} L {:.3} {:.3}", a.x, a.y, b.x, b.y),
        Geom::Rect { origin, size, .. } => {
            let pts = crate::geom::rounded_rect_corners(*origin, *size, shape.effective_corners());
            poly_d(&pts, true)
        }
        _ => {
            let mut d = String::new();
            for pts in shape.geom.contours(96) {
                if pts.len() < 2 {
                    continue;
                }
                d.push_str(&poly_d(&pts, shape.geom.is_closed()));
                d.push(' ');
            }
            d.trim().to_string()
        }
    }
}

fn poly_d(pts: &[Pt], closed: bool) -> String {
    if pts.len() < 2 {
        return String::new();
    }
    let mut d = format!("M {:.3} {:.3}", pts[0].x, pts[0].y);
    for p in pts.iter().skip(1) {
        d.push_str(&format!(" L {:.3} {:.3}", p.x, p.y));
    }
    if closed {
        d.push_str(" Z");
    }
    d
}

fn xf_attr(shape: &Shape) -> String {
    if shape.rotation.abs() < 1e-5 {
        return String::new();
    }
    let c = shape.geom.bbox().center();
    format!(
        " transform=\"rotate({:.4} {:.3} {:.3})\"",
        shape.rotation.to_degrees(),
        c.x,
        c.y
    )
}

fn rgba_css(c: Rgba) -> String {
    c.css()
}

fn hex_css(c: Rgba) -> String {
    format!("#{:02X}{:02X}{:02X}", c.r, c.g, c.b)
}

fn svg_color(c: Rgba) -> String {
    if c.a >= 250 {
        hex_css(c)
    } else {
        rgba_css(c)
    }
}

fn raster_worth_exporting(
    pixels: &crate::document::Pixels,
    origin: Pt,
    size: Pt,
    doc: &Document,
) -> bool {
    if pixels.is_invisible() {
        return false;
    }
    if let Some(c) = pixels.is_uniform() {
        let paper = c.r > 250 && c.g > 250 && c.b > 250;
        let (dw, dh) = if size.x.abs() > 0.5 && size.y.abs() > 0.5 {
            (size.x.abs(), size.y.abs())
        } else {
            (pixels.w as f32, pixels.h as f32)
        };
        let covers = origin.x.abs() < 1.0
            && origin.y.abs() < 1.0
            && (dw - doc.width).abs() < 2.0
            && (dh - doc.height).abs() < 2.0;
        if paper && covers {
            return false;
        }
    }
    true
}

fn layer_bounds(layer: &Layer) -> Option<Bounds> {
    match &layer.kind {
        LayerKind::Vector { shapes } => {
            let mut b: Option<Bounds> = None;
            for s in shapes {
                if !s.visible {
                    continue;
                }
                let sb = s.world_bbox();
                b = Some(match b {
                    None => sb,
                    Some(acc) => acc.union(sb),
                });
            }
            b
        }
        LayerKind::Raster { .. } => layer.kind.raster_bounds().or_else(|| {
            layer.kind.pixels().map(|pixels| {
                Bounds::from_min_size(Pt::ZERO, Pt::new(pixels.w as f32, pixels.h as f32))
            })
        }),
    }
}

fn stop_color(c: Rgba) -> String {
    if c.a >= 250 {
        hex_css(c)
    } else {
        format!(
            "{}\" stop-opacity=\"{:.3}",
            hex_css(c),
            c.a as f32 / 255.0
        )
    }
}

fn write_shape(
    body: &mut String,
    defs: &mut String,
    grad_id: &mut usize,
    shape: &Shape,
    extra: &str,
) {
    let fill_attr = match &shape.style.fill {
        Fill::None => "fill=\"none\"".to_string(),
        Fill::Solid(c) => format!("fill=\"{}\"", svg_color(*c)),
        Fill::Linear { from, to, c0, c1 } => {
            *grad_id += 1;
            let id = format!("g{grad_id}");
            let b = shape.geom.bbox();
            let (w, h) = (b.width().max(1e-3), b.height().max(1e-3));
            let (x1, y1) = (b.min.x + from[0] * w, b.min.y + from[1] * h);
            let (x2, y2) = (b.min.x + to[0] * w, b.min.y + to[1] * h);
            defs.push_str(&format!(
                "<linearGradient id=\"{id}\" gradientUnits=\"userSpaceOnUse\" x1=\"{x1:.2}\" y1=\"{y1:.2}\" x2=\"{x2:.2}\" y2=\"{y2:.2}\"><stop offset=\"0\" stop-color=\"{}\"/><stop offset=\"1\" stop-color=\"{}\"/></linearGradient>\n",
                stop_color(*c0),
                stop_color(*c1)
            ));
            format!("fill=\"url(#{id})\"")
        }
        Fill::Radial { c0, c1 } => {
            *grad_id += 1;
            let id = format!("g{grad_id}");
            let b = shape.geom.bbox();
            let c = b.center();
            let r = b.width().max(b.height()) * 0.5;
            defs.push_str(&format!(
                "<radialGradient id=\"{id}\" cx=\"{:.2}\" cy=\"{:.2}\" r=\"{:.2}\" gradientUnits=\"userSpaceOnUse\"><stop offset=\"0\" stop-color=\"{}\"/><stop offset=\"1\" stop-color=\"{}\"/></radialGradient>\n",
                c.x, c.y, r, stop_color(*c0), stop_color(*c1)
            ));
            format!("fill=\"url(#{id})\"")
        }
    };
    let stroke_attr = match &shape.style.stroke {
        Some(s) if s.width > 0.0 => {
            let dash = s
                .dash
                .map(|(a, b)| format!(" stroke-dasharray=\"{a} {b}\""))
                .unwrap_or_default();
            format!(
                " stroke=\"{}\" stroke-width=\"{:.2}\" stroke-linecap=\"{}\" stroke-linejoin=\"{}\"{dash}",
                svg_color(s.color),
                s.width,
                s.cap.name().to_ascii_lowercase(),
                s.join.name().to_ascii_lowercase()
            )
        }
        _ => String::new(),
    };
    if let Geom::Text(run) = &shape.geom {
        let family = crate::text::label_for(&run.font);
        let fill = match &shape.style.fill {
            Fill::Solid(c) => svg_color(*c),
            _ => "#111111".into(),
        };
        let escaped = run
            .content
            .replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;");
        body.push_str(&format!(
            "  <text id=\"oma-{}\" x=\"{:.3}\" y=\"{:.3}\" font-family=\"{}\" font-size=\"{:.2}\" fill=\"{fill}\" opacity=\"{:.3}\"{extra}>{}</text>\n",
            shape.id,
            run.origin.x,
            run.origin.y,
            xml_escape(&family),
            run.px,
            shape.opacity,
            escaped
        ));
        return;
    }
    if let Geom::Ellipse { center, radii } = &shape.geom {
        body.push_str(&format!(
            "  <ellipse id=\"oma-{}\" cx=\"{:.3}\" cy=\"{:.3}\" rx=\"{:.3}\" ry=\"{:.3}\" {fill_attr}{stroke_attr} opacity=\"{:.3}\"{extra}/>\n",
            shape.id, center.x, center.y, radii.x.abs(), radii.y.abs(), shape.opacity
        ));
        return;
    }
    if let Geom::Rect {
        origin,
        size,
        radius,
    } = &shape.geom
    {
        let corners = shape.effective_corners();
        if corners.iter().all(|c| *c < 0.5) && *radius < 0.5 {
            body.push_str(&format!(
                "  <rect id=\"oma-{}\" x=\"{:.3}\" y=\"{:.3}\" width=\"{:.3}\" height=\"{:.3}\" {fill_attr}{stroke_attr} opacity=\"{:.3}\"{extra}/>\n",
                shape.id,
                origin.x.min(origin.x + size.x),
                origin.y.min(origin.y + size.y),
                size.x.abs(),
                size.y.abs(),
                shape.opacity
            ));
            return;
        }
    }
    let d = path_data(shape);
    if d.is_empty() {
        return;
    }
    let rule = match &shape.geom {
        Geom::Poly { winding: false, .. } => " fill-rule=\"evenodd\"",
        _ => "",
    };
    body.push_str(&format!(
        "  <path id=\"oma-{}\" d=\"{d}\" {fill_attr}{stroke_attr}{rule} opacity=\"{:.3}\"{extra}/>\n",
        shape.id,
        shape.opacity
    ));
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

pub fn export(doc: &Document) -> Result<String, String> {
    export_inner(doc, false)
}

pub fn export_animated(doc: &Document) -> Result<String, String> {
    export_inner(doc, true)
}

fn export_inner(doc: &Document, animate: bool) -> Result<String, String> {
    let mut body = String::new();
    let mut defs = String::new();
    let mut grad_id = 0usize;
    let mut css = String::new();
    let motion = &doc.motion;
    let looping = if motion.looped { "infinite" } else { "1" };
    if animate && !motion.is_empty() {
        css.push_str(&format!(
            ".oma-a {{ animation-duration: {:.3}s; animation-iteration-count: {looping}; animation-fill-mode: both; transform-box: fill-box; transform-origin: center; }}\n",
            motion.duration.max(0.05)
        ));
    }

    for layer in &doc.layers {
        if !layer.visible || layer.opacity <= 0.0 {
            continue;
        }
        let fx_id = format!("oma-fx-{}", layer.id);
        let fx_attr = if layer.filters.active() {
            let b = layer_bounds(layer).unwrap_or(crate::geom::Bounds {
                min: crate::geom::Pt::ZERO,
                max: crate::geom::Pt::new(doc.width, doc.height),
            });
            let pad = crate::filter::svg_pad(&layer.filters);
            let r = b.inflate(pad);
            if let Some(f) = crate::filter::svg_filter(
                &fx_id,
                &layer.filters,
                [r.min.x, r.min.y, r.width().max(1.0), r.height().max(1.0)],
            ) {
                defs.push_str(&f);
                format!(" filter=\"url(#{fx_id})\"")
            } else {
                String::new()
            }
        } else {
            String::new()
        };
        let mut layer_body = String::new();
        match &layer.kind {
            LayerKind::Vector { shapes } => {
                for shape in shapes {
                    if !shape.visible {
                        continue;
                    }
                    let mut extra = String::new();
                    if animate
                        && let Some(kf) = motion.css_keyframes(shape.id, &format!("oma-{}", shape.id))
                    {
                        css.push_str(&kf);
                        extra = format!(
                            " class=\"oma-a\" style=\"animation-name: oma-{}\"",
                            shape.id
                        );
                    }
                    if shape.filters.active() {
                        let fid = format!("oma-fx-s{}", shape.id);
                        let b = shape.world_bbox();
                        let pad = crate::filter::svg_pad(&shape.filters);
                        let r = b.inflate(pad);
                        if let Some(f) = crate::filter::svg_filter(
                            &fid,
                            &shape.filters,
                            [r.min.x, r.min.y, r.width().max(1.0), r.height().max(1.0)],
                        ) {
                            defs.push_str(&f);
                            extra.push_str(&format!(" filter=\"url(#{fid})\""));
                        }
                    }
                    extra.push_str(&xf_attr(shape));
                    write_shape(
                        &mut layer_body,
                        &mut defs,
                        &mut grad_id,
                        shape,
                        &extra,
                    );
                }
            }
            LayerKind::Raster {
                pixels,
                origin,
                size,
                rotation,
            } => {
                if raster_worth_exporting(pixels, *origin, *size, doc)
                    && let Some(pm) = pixels.to_pixmap()
                    && let Ok(png) = pm.encode_png()
                {
                    let b64 = base64::Engine::encode(
                        &base64::engine::general_purpose::STANDARD,
                        png,
                    );
                    let (dw, dh) = if size.x.abs() > 0.5 && size.y.abs() > 0.5 {
                        (size.x, size.y)
                    } else {
                        (pixels.w as f32, pixels.h as f32)
                    };
                    let mut xf = String::new();
                    if rotation.abs() > 1e-5 {
                        let cx = origin.x + dw * 0.5;
                        let cy = origin.y + dh * 0.5;
                        xf = format!(
                            " transform=\"rotate({:.4} {cx:.3} {cy:.3})\"",
                            rotation.to_degrees()
                        );
                    }
                    layer_body.push_str(&format!(
                        "  <image href=\"data:image/png;base64,{b64}\" x=\"{:.3}\" y=\"{:.3}\" width=\"{:.3}\" height=\"{:.3}\"{xf}/>\n",
                        origin.x, origin.y, dw, dh
                    ));
                }
            }
        }
        if layer_body.is_empty() {
            continue;
        }
        body.push_str(&format!(
            "<g opacity=\"{:.3}\" style=\"mix-blend-mode:{}\"{fx_attr}>\n",
            layer.opacity,
            layer.blend.css()
        ));
        body.push_str(&layer_body);
        body.push_str("</g>\n");
    }

    let style = if css.is_empty() {
        String::new()
    } else {
        format!("<style>\n{css}</style>\n")
    };
    Ok(format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{}\" height=\"{}\" viewBox=\"0 0 {} {}\">\n{style}<defs>\n{defs}</defs>\n{body}</svg>\n",
        doc.width, doc.height, doc.width, doc.height
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::{Cmd, Shape, Style, apply};
    use crate::geom::{Geom, Pt};

    #[test]
    fn svg_contains_path() {
        let mut doc = Document::new("t", 100.0, 100.0, 72.0);
        apply(
            &mut doc,
            &Cmd::AddShape {
                layer: 1,
                shape: Shape::new(
                    Geom::Rect {
                        origin: Pt::new(10.0, 10.0),
                        size: Pt::new(40.0, 20.0),
                        radius: 0.0,
                    },
                    Style::default(),
                ),
            },
        );
        let s = export(&doc).unwrap();
        assert!(s.contains("<path") || s.contains("<rect"), "{s}");
        assert!(s.contains("viewBox"));
    }

    #[test]
    fn svg_writes_layer_fx() {
        let mut doc = Document::new("t", 400.0, 300.0, 72.0);
        apply(
            &mut doc,
            &Cmd::AddShape {
                layer: 1,
                shape: Shape::new(
                    Geom::Rect {
                        origin: Pt::new(40.0, 40.0),
                        size: Pt::new(120.0, 80.0),
                        radius: 0.0,
                    },
                    Style::default(),
                ),
            },
        );
        doc.layers[1].filters.enabled = true;
        doc.layers[1].filters.items = vec![
            crate::filter::Fx::Shadow {
                dx: 50.0,
                dy: 55.0,
                blur: 22.0,
                color: crate::color::Rgba::BLACK,
            },
            crate::filter::Fx::Blur { std: 29.0 },
        ];
        let s = export(&doc).unwrap();
        assert!(s.contains("<filter"), "defs must include a filter");
        assert!(s.contains("feGaussianBlur"), "{s}");
        assert!(s.contains("feOffset"), "{s}");
        assert!(s.contains("filter=\"url(#oma-fx-"), "{s}");
        assert!(s.contains("userSpaceOnUse"), "{s}");
        assert!(!s.contains("feDropShadow"));
    }

    #[test]
    fn svg_path_keeps_cubics() {
        use crate::geom::Anchor;
        let mut doc = Document::new("t", 200.0, 200.0, 72.0);
        apply(
            &mut doc,
            &Cmd::AddShape {
                layer: 1,
                shape: Shape::new(
                    crate::geom::Geom::Path {
                        anchors: vec![
                            Anchor::corner(crate::geom::Pt::new(10.0, 10.0)),
                            Anchor::smooth(crate::geom::Pt::new(80.0, 40.0), crate::geom::Pt::new(20.0, 10.0)),
                        ],
                        closed: false,
                    },
                    Style::default(),
                ),
            },
        );
        let s = export(&doc).unwrap();
        assert!(s.contains(" C ") || s.contains("C "), "{s}");
    }

    #[test]
    fn logo_oma_roundtrip_structure() {
        let path = std::path::Path::new("media/logo.oma");
        if !path.exists() {
            return;
        }
        let doc = crate::project::load_from(path).expect("logo.oma");
        let s = export(&doc).unwrap();
        assert!(s.contains("<svg"));
        assert!(s.contains("viewBox"));
        assert!(s.contains("<path") || s.contains("<rect") || s.contains("<ellipse"));
        assert!(
            !s.contains("<image"),
            "blank paper raster must not steal the SVG thumbnail"
        );
        assert!(s.contains("fill=\"none\""), "{s}");
        assert!(s.contains("stroke=\"#"), "opaque stroke must be hex, got {s}");
    }

    #[test]
    fn opaque_fill_is_hex_not_rgba() {
        let mut doc = Document::new("t", 100.0, 100.0, 72.0);
        apply(
            &mut doc,
            &Cmd::AddShape {
                layer: 1,
                shape: Shape::new(
                    Geom::Rect {
                        origin: Pt::new(10.0, 10.0),
                        size: Pt::new(40.0, 20.0),
                        radius: 0.0,
                    },
                    crate::document::Style {
                        fill: Fill::Solid(crate::color::Rgba::rgb(0, 0, 0)),
                        stroke: None,
                    },
                ),
            },
        );
        let s = export(&doc).unwrap();
        assert!(s.contains("fill=\"#000000\""), "{s}");
        assert!(!s.contains("rgba(0,0,0"), "{s}");
    }
}
