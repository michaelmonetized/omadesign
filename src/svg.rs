//! SVG export of the current document.

use crate::color::Rgba;
use crate::document::{Document, Fill, Layer, LayerKind, Shape};
use crate::geom::{Bounds, Pt};

fn path_data(shape: &Shape) -> String {
    let mut d = String::new();
    for pts in shape.world_contours(96) {
        if pts.len() < 2 {
            continue;
        }
        d.push_str(&format!("M {:.2} {:.2} ", pts[0].x, pts[0].y));
        for p in pts.iter().skip(1) {
            d.push_str(&format!("L {:.2} {:.2} ", p.x, p.y));
        }
        if shape.geom.is_closed() {
            d.push('Z');
        }
        d.push(' ');
    }
    d.trim().to_string()
}

fn rgba_css(c: Rgba) -> String {
    c.css()
}

fn hex_css(c: Rgba) -> String {
    format!("#{:02X}{:02X}{:02X}", c.r, c.g, c.b)
}

fn layer_bounds(layer: &Layer) -> Option<Bounds> {
    match &layer.kind {
        LayerKind::Vector { shapes } => {
            let mut b: Option<Bounds> = None;
            for s in shapes {
                let sb = s.world_bbox();
                b = Some(match b {
                    None => sb,
                    Some(acc) => acc.union(sb),
                });
            }
            b
        }
        LayerKind::Raster { pixels } => Some(Bounds::from_min_size(
            Pt::ZERO,
            Pt::new(pixels.w as f32, pixels.h as f32),
        )),
    }
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
        body.push_str(&format!(
            "<g opacity=\"{:.3}\" style=\"mix-blend-mode:{}\"{fx_attr}>\n",
            layer.opacity,
            layer.blend.css()
        ));
        match &layer.kind {
            LayerKind::Vector { shapes } => {
                for shape in shapes {
                    let d = path_data(shape);
                    if d.is_empty() {
                        continue;
                    }
                    let fill_attr = match &shape.style.fill {
                        Fill::None => "fill=\"none\"".to_string(),
                        Fill::Solid(c) => format!("fill=\"{}\"", rgba_css(*c)),
                        Fill::Linear { from, to, c0, c1 } => {
                            grad_id += 1;
                            let id = format!("g{grad_id}");
                            let b = shape.world_bbox();
                            let (w, h) = (b.width().max(1e-3), b.height().max(1e-3));
                            let (x1, y1) = (b.min.x + from[0] * w, b.min.y + from[1] * h);
                            let (x2, y2) = (b.min.x + to[0] * w, b.min.y + to[1] * h);
                            defs.push_str(&format!(
                                "<linearGradient id=\"{id}\" gradientUnits=\"userSpaceOnUse\" x1=\"{x1:.2}\" y1=\"{y1:.2}\" x2=\"{x2:.2}\" y2=\"{y2:.2}\"><stop offset=\"0\" stop-color=\"{}\"/><stop offset=\"1\" stop-color=\"{}\"/></linearGradient>\n",
                                hex_css(*c0),
                                hex_css(*c1)
                            ));
                            format!("fill=\"url(#{id})\"")
                        }
                        Fill::Radial { c0, c1 } => {
                            grad_id += 1;
                            let id = format!("g{grad_id}");
                            let b = shape.world_bbox();
                            let c = b.center();
                            let r = b.width().max(b.height()) * 0.5;
                            defs.push_str(&format!(
                                "<radialGradient id=\"{id}\" cx=\"{:.2}\" cy=\"{:.2}\" r=\"{:.2}\" gradientUnits=\"userSpaceOnUse\"><stop offset=\"0\" stop-color=\"{}\"/><stop offset=\"1\" stop-color=\"{}\"/></radialGradient>\n",
                                c.x, c.y, r, hex_css(*c0), hex_css(*c1)
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
                                rgba_css(s.color),
                                s.width,
                                s.cap.name().to_ascii_lowercase(),
                                s.join.name().to_ascii_lowercase()
                            )
                        }
                        _ => String::new(),
                    };
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
                    body.push_str(&format!(
                        "  <path id=\"oma-{}\" d=\"{d}\" {fill_attr}{stroke_attr} opacity=\"{:.3}\"{extra}/>\n",
                        shape.id,
                        shape.opacity
                    ));
                }
            }
            LayerKind::Raster { pixels } => {
                if let Some(pm) = pixels.to_pixmap()
                    && let Ok(png) = pm.encode_png()
                {
                    let b64 = base64::Engine::encode(
                        &base64::engine::general_purpose::STANDARD,
                        png,
                    );
                    body.push_str(&format!(
                        "  <image href=\"data:image/png;base64,{b64}\" x=\"0\" y=\"0\" width=\"{}\" height=\"{}\"/>\n",
                        pixels.w, pixels.h
                    ));
                }
            }
        }
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
        assert!(s.contains("<path"));
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
}
