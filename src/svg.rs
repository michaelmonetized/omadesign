use crate::document::{Document, Fill, LayerKind, Shape};

fn path_data(shape: &Shape, precision: usize) -> String {
    let contours = shape.geom.contours(96);
    let closed = shape.geom.is_closed_outline();
    let fmt = |v: f32| {
        let s = format!("{v:.precision$}");
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    };
    let mut d = String::new();
    for pts in &contours {
        if pts.len() < 2 {
            continue;
        }
        d.push_str(&format!("M {} {} ", fmt(pts[0].x), fmt(pts[0].y)));
        for p in pts.iter().skip(1) {
            d.push_str(&format!("L {} {} ", fmt(p.x), fmt(p.y)));
        }
        if closed {
            d.push('Z');
        }
    }
    d.trim_end().to_string()
}

fn rgba_css(c: eframe::egui::Color32) -> String {
    format!("rgba({},{},{},{:.3})", c.r(), c.g(), c.b(), c.a() as f32 / 255.0)
}

pub fn export(doc: &Document) -> Result<String, String> {
    let mut svg = String::new();
    svg.push_str(&format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<svg xmlns=\"http://www.w3.org/2000/svg\" xmlns:xlink=\"http://www.w3.org/1999/xlink\" width=\"{}\" height=\"{}\" viewBox=\"0 0 {} {}\">\n",
        doc.width, doc.height, doc.width, doc.height
    ));

    let mut defs = String::new();
    let mut grad_id = 0usize;

    for layer in &doc.layers {
        if !layer.visible || layer.opacity <= 0.0 {
            continue;
        }
        svg.push_str(&format!(
            "<g opacity=\"{:.3}\" style=\"mix-blend-mode:{}\">\n",
            layer.opacity,
            blend_css(layer.blend)
        ));
        match &layer.kind {
            LayerKind::Vector(v) => {
                for shape in &v.shapes {
                    let d = path_data(shape, 2);
                    if d.is_empty() {
                        continue;
                    }
                    let fill_attr = match &shape.style.fill {
                        Fill::None if shape.geom.is_closed_outline() => "none".to_string(),
                        Fill::None => String::new(),
                        Fill::Solid(c) => format!("fill=\"{}\"", rgba_css(*c)),
                        Fill::Linear { from, to, c0, c1 } => {
                            grad_id += 1;
                            let id = format!("grad{grad_id}");
                            let b = shape.geom.bbox();
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
                    };
                    let stroke_attr = match &shape.style.stroke {
                        Some(s) if s.width > 0.0 => format!(
                            " stroke=\"{}\" stroke-width=\"{:.2}\" stroke-linecap=\"round\" stroke-linejoin=\"round\" fill=\"none\"",
                            rgba_css(s.color),
                            s.width
                        ),
                        _ => String::new(),
                    };
                    let fill_only = if stroke_attr.is_empty() {
                        fill_attr
                    } else {
                        String::new()
                    };
                    svg.push_str(&format!(
                        "<path d=\"{d}\" {fill_only}{stroke_attr}/>\n"
                    ));
                }
            }
            LayerKind::Raster(r) => {
                let png = r
                    .pixmap
                    .encode_png()
                    .map_err(|e| format!("svg raster encode: {e}"))?;
                let b64 = {
                    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
                    let mut out = String::with_capacity(png.len().div_ceil(3) * 4);
                    for chunk in png.chunks(3) {
                        let b = [
                            chunk[0] as u32,
                            chunk.get(1).map(|v| *v as u32).unwrap_or(0),
                            chunk.get(2).map(|v| *v as u32).unwrap_or(0),
                        ];
                        let n = (b[0] << 16) | (b[1] << 8) | b[2];
                        out.push(TABLE[(n >> 18 & 63) as usize] as char);
                        out.push(TABLE[(n >> 12 & 63) as usize] as char);
                        out.push(if chunk.len() > 1 { TABLE[(n >> 6 & 63) as usize] as char } else { '=' });
                        out.push(if chunk.len() > 2 { TABLE[(n & 63) as usize] as char } else { '=' });
                    }
                    out
                };
                svg.push_str(&format!(
                    "<image x=\"0\" y=\"0\" width=\"{}\" height=\"{}\" xlink:href=\"data:image/png;base64,{b64}\"/>\n",
                    doc.width, doc.height
                ));
            }
        }
        svg.push_str("</g>\n");
    }

    if !defs.is_empty() {
        svg.push_str(&format!("<defs>\n{defs}</defs>\n"));
    }
    svg.push_str("</svg>\n");

    let path = "atelier-export.svg";
    std::fs::write(path, &svg).map_err(|e| format!("write: {e}"))?;
    Ok(svg)
}

fn blend_css(b: crate::document::LayerBlend) -> &'static str {
    use crate::document::LayerBlend as B;
    match b {
        B::Normal => "normal",
        B::Multiply => "multiply",
        B::Screen => "screen",
        B::Overlay => "overlay",
        B::Darken => "darken",
        B::Lighten => "lighten",
        B::Difference => "difference",
        B::Exclusion => "exclusion",
        B::HardLight => "hard-light",
        B::SoftLight => "soft-light",
    }
}

fn hex_css(c: eframe::egui::Color32) -> String {
    format!("#{:02x}{:02x}{:02x}", c.r(), c.g(), c.b())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::{
        next_shape_id, Geometry, Layer, LayerBlend, LayerKind, Style, VectorLayer,
    };
    use eframe::egui::{pos2, Color32, Stroke, Vec2};

    #[test]
    fn svg_contains_shapes_and_raster() {
        let mut doc = Document::new(100.0, 100.0);
        doc.layers[1]
            .kind
            .vector_shapes_mut()
            .unwrap()
            .push(Shape {
                id: next_shape_id(),
                geom: Geometry::Ellipse {
                    center: pos2(50.0, 50.0),
                    radii: Vec2::splat(20.0),
                },
                style: Style {
                    fill: Fill::Solid(Color32::from_rgb(10, 20, 30)),
                    stroke: Some(Stroke::new(2.0, Color32::BLACK)),
                },
            });
        let out = export(&doc).unwrap();
        assert!(out.starts_with("<?xml"));
        assert!(out.contains("<path"));
        assert!(out.contains("data:image/png;base64"));
        assert!(out.contains("stroke-width=\"2.00\""));
    }
}
