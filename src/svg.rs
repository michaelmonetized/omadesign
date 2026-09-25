//! SVG export of the current document.

use crate::color::Rgba;
use crate::document::{Document, Fill, Layer, LayerKind, Shape};
use crate::geom::{Bounds, Geom, Pt};

fn path_data(shape: &Shape) -> String {
    match &shape.geom {
        Geom::Path { anchors, closed } => crate::geom::path_svg_d(anchors, *closed),
        Geom::Paths { paths, .. } => paths
            .iter()
            .map(|p| crate::geom::path_svg_d(&p.anchors, p.closed))
            .collect::<Vec<_>>()
            .join(" "),
        Geom::Ellipse { .. } => {
            let Geom::Path { anchors, closed } = shape.geom.to_path() else {
                unreachable!()
            };
            crate::geom::path_svg_d(&anchors, closed)
        }
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
    if c.a == 255 { hex_css(c) } else { rgba_css(c) }
}

fn pixel_image(
    pixels: &crate::document::Pixels,
    transform: tiny_skia::Transform,
) -> Result<String, String> {
    let png = pixels
        .to_pixmap()
        .ok_or("Invalid image pixels in SVG export")?
        .encode_png()
        .map_err(|error| format!("Could not encode SVG image: {error}"))?;
    let b64 = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, png);
    Ok(format!(
        "  <image href=\"data:image/png;base64,{b64}\" width=\"{}\" height=\"{}\" preserveAspectRatio=\"none\" transform=\"matrix({:.6} {:.6} {:.6} {:.6} {:.6} {:.6})\"/>\n",
        pixels.w,
        pixels.h,
        transform.sx,
        transform.ky,
        transform.kx,
        transform.sy,
        transform.tx,
        transform.ty,
    ))
}

fn write_layer_mask(defs: &mut String, layer: &Layer) -> Result<Option<String>, String> {
    let Some(mask) = &layer.mask else {
        return Ok(None);
    };
    let transform = crate::compositor::layer_pixel_transform(layer);
    let mut corners = [
        tiny_skia::Point::from_xy(0.0, 0.0),
        tiny_skia::Point::from_xy(mask.w as f32, 0.0),
        tiny_skia::Point::from_xy(mask.w as f32, mask.h as f32),
        tiny_skia::Point::from_xy(0.0, mask.h as f32),
    ];
    transform.map_points(&mut corners);
    let mut bounds = Bounds::from_pt(Pt::new(corners[0].x, corners[0].y));
    for point in &corners[1..] {
        bounds.union_pt(Pt::new(point.x, point.y));
    }
    let bounds = bounds.inflate(1.0);
    let id = format!("oma-mask-{}", layer.id);
    defs.push_str(&format!(
        "<mask id=\"{id}\" maskUnits=\"userSpaceOnUse\" maskContentUnits=\"userSpaceOnUse\" mask-type=\"luminance\" color-interpolation=\"sRGB\" x=\"{:.3}\" y=\"{:.3}\" width=\"{:.3}\" height=\"{:.3}\">\n",
        bounds.min.x,bounds.min.y,bounds.width(),bounds.height(),
    ));
    defs.push_str(&pixel_image(mask, transform)?);
    defs.push_str("</mask>\n");
    Ok(Some(id))
}

fn layer_bounds(layer: &Layer) -> Option<Bounds> {
    match &layer.kind {
        LayerKind::Vector { shapes } => {
            let mut b: Option<Bounds> = None;
            for s in shapes {
                if !s.visible || s.guide {
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
    if c.a == 255 {
        hex_css(c)
    } else {
        format!("{}\" stop-opacity=\"{:.3}", hex_css(c), c.a as f32 / 255.0)
    }
}

fn gradient_reference(
    defs: &mut String,
    grad_id: &mut usize,
    gradient: &crate::gradient::Gradient,
    shape: &Shape,
    padding: f32,
) -> String {
    use crate::gradient::GradientKind;
    *grad_id += 1;
    let id = format!("g{grad_id}");
    let mut gradient = gradient.clone();
    gradient.normalize();
    let (a, b) = gradient.endpoints(shape.geom.bbox());
    if matches!(gradient.kind, GradientKind::Shape | GradientKind::Conic) {
        if let Some((image, bounds)) = crate::gradient::texture(&gradient, shape, padding, 2.)
            && let Ok(png) = image.encode_png()
        {
            let data = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, png);
            defs.push_str(&format!("<pattern id=\"{id}\" patternUnits=\"userSpaceOnUse\" patternTransform=\"translate({} {})\" x=\"0\" y=\"0\" width=\"{}\" height=\"{}\"><image href=\"data:image/png;base64,{data}\" x=\"0\" y=\"0\" width=\"{}\" height=\"{}\" preserveAspectRatio=\"none\"/></pattern>\n",bounds.min.x,bounds.min.y,bounds.width(),bounds.height(),bounds.width(),bounds.height()));
        }
    } else {
        let tag = if gradient.kind == GradientKind::Linear {
            "linearGradient"
        } else {
            "radialGradient"
        };
        let geometry = if gradient.kind == GradientKind::Linear {
            format!(
                "x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\"",
                a.x, a.y, b.x, b.y
            )
        } else {
            format!(
                "cx=\"{}\" cy=\"{}\" r=\"{}\"",
                a.x,
                a.y,
                (b - a).length().max(0.001)
            )
        };
        defs.push_str(&format!(
            "<{tag} id=\"{id}\" gradientUnits=\"userSpaceOnUse\" {geometry}>"
        ));
        for stop in &gradient.stops {
            defs.push_str(&format!(
                "<stop offset=\"{:.6}\" stop-color=\"{}\"/>",
                stop.offset,
                stop_color(stop.color)
            ));
        }
        defs.push_str(&format!("</{tag}>\n"));
    }
    format!("url(#{id})")
}

fn object_mask_definition(defs: &mut String, shape: &Shape) -> Option<String> {
    let mask = shape.mask.as_ref()?;
    let mut local = shape.clone();
    local.rotation = 0.;
    let image = pixel_image(mask, crate::compositor::shape_mask_transform(&local)).ok()?;
    let id = format!("oma-object-mask-{}", shape.id);
    let b = shape.geom.bbox();
    defs.push_str(&format!("<mask id=\"{id}\" maskUnits=\"userSpaceOnUse\" maskContentUnits=\"userSpaceOnUse\" mask-type=\"luminance\" x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\">{image}</mask>",b.min.x,b.min.y,b.width(),b.height()));
    Some(id)
}

fn write_shape(
    body: &mut String,
    defs: &mut String,
    grad_id: &mut usize,
    shape: &Shape,
    extra: &str,
    text_as_paths: bool,
) {
    if shape.mask.is_some() {
        let mut plain = shape.clone();
        plain.mask = None;
        let mut nested = String::new();
        write_shape(&mut nested, defs, grad_id, &plain, "", text_as_paths);
        if let Some(id) = object_mask_definition(defs, shape) {
            body.push_str(&format!(
                "<g{extra}><g mask=\"url(#{id})\">{nested}</g></g>"
            ));
        }
        return;
    }
    if let Some(stroke) = &shape.style.stroke
        && shape.geom.is_closed()
        && stroke.alignment != crate::document::StrokeAlignment::Center
    {
        let id = format!("oma-stroke-position-{}", shape.id);
        let b = shape.geom.bbox().inflate(stroke.width * 6. + 2.);
        let (background, foreground) =
            if stroke.alignment == crate::document::StrokeAlignment::Inside {
                ("black", "white")
            } else {
                ("white", "black")
            };
        let rule = if matches!(
            shape.geom,
            Geom::Poly { winding: true, .. } | Geom::Paths { winding: true, .. }
        ) {
            "nonzero"
        } else {
            "evenodd"
        };
        defs.push_str(&format!("<mask id=\"{id}\" maskUnits=\"userSpaceOnUse\" maskContentUnits=\"userSpaceOnUse\" x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\"><rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"{background}\"/><path d=\"{}\" fill=\"{foreground}\" fill-rule=\"{rule}\"/></mask>", b.min.x,b.min.y,b.width(),b.height(), b.min.x,b.min.y,b.width(),b.height(),path_data(shape)));
        let mut plain = shape.clone();
        plain.style.stroke = None;
        plain.opacity = 1.;
        plain.blend = crate::color::Blend::Normal;
        let mut fill = String::new();
        write_shape(&mut fill, defs, grad_id, &plain, "", text_as_paths);
        plain.style.fill = Fill::None;
        plain.layout.image = None;
        let mut centered = stroke.clone();
        centered.alignment = crate::document::StrokeAlignment::Center;
        centered.width *= 2.;
        plain.style.stroke = Some(centered);
        let mut edge = String::new();
        write_shape(
            &mut edge,
            defs,
            grad_id,
            &plain,
            &format!(" mask=\"url(#{id})\""),
            text_as_paths,
        );
        edge = edge.replace(
            &format!("id=\"oma-{}\"", shape.id),
            &format!("id=\"oma-stroke-{}\"", shape.id),
        );
        body.push_str(&format!(
            "<g{extra} opacity=\"{}\" style=\"mix-blend-mode:{}\">{fill}{edge}</g>",
            shape.opacity,
            shape.blend.css()
        ));
        return;
    }
    if let Some(image) = &shape.layout.image {
        if let Ok(pixels) = crate::layout_images::pixmap(image) {
            let b = shape.geom.bbox();
            let placed = crate::layout_images::placement(
                b,
                Pt::new(pixels.width() as f32, pixels.height() as f32),
                image.fit,
                image.focal,
            );
            let mut background = shape.clone();
            background.layout.image = None;
            background.opacity = 1.;
            background.blend = crate::color::Blend::Normal;
            background.rotation = 0.;
            background.style.stroke = None;
            body.push_str(&format!(
                "<g{extra} opacity=\"{:.4}\" style=\"mix-blend-mode:{}\"{}>",
                shape.opacity,
                shape.blend.css(),
                if shape.visible {
                    ""
                } else {
                    " visibility=\"hidden\""
                }
            ));
            write_shape(body, defs, grad_id, &background, "", text_as_paths);
            let clip_id = format!("oma-image-clip-{}", shape.id);
            defs.push_str(&format!("<clipPath id=\"{clip_id}\" clipPathUnits=\"userSpaceOnUse\"><path d=\"{}\"/></clipPath>",path_data(&background)));
            body.push_str(&format!("<image href=\"data:image/png;base64,{}\" x=\"{:.4}\" y=\"{:.4}\" width=\"{:.4}\" height=\"{:.4}\" preserveAspectRatio=\"none\" clip-path=\"url(#{clip_id})\"/>",image.data,placed.min.x,placed.min.y,placed.width(),placed.height()));
            if shape.style.stroke.is_some() {
                background.style.fill = Fill::None;
                background.style.stroke = shape.style.stroke.clone();
                let mut stroke = String::new();
                write_shape(&mut stroke, defs, grad_id, &background, "", text_as_paths);
                body.push_str(&stroke.replace(
                    &format!("id=\"oma-{}\"", shape.id),
                    &format!("id=\"oma-image-stroke-{}\"", shape.id),
                ));
            }
            body.push_str("</g>");
            return;
        }
    }
    let extra = format!(
        " inkscape:label=\"{}\" style=\"mix-blend-mode:{}\"{}{}",
        xml_escape(&shape.name),
        shape.blend.css(),
        if shape.visible {
            ""
        } else {
            " visibility=\"hidden\""
        },
        extra
    );
    let fill_attr = match &shape.style.fill {
        Fill::Gradient(g) => format!(
            "fill=\"{}\"",
            gradient_reference(defs, grad_id, g, shape, 0.)
        ),
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
                .map(|(a, b)| {
                    let offset = if s.dash_offset.abs() > 0.01 {
                        format!(" stroke-dashoffset=\"{:.2}\"", s.dash_offset)
                    } else {
                        String::new()
                    };
                    format!(" stroke-dasharray=\"{a} {b}\"{offset}")
                })
                .unwrap_or_default();
            format!(
                " stroke=\"{}\" stroke-width=\"{:.2}\" stroke-linecap=\"{}\" stroke-linejoin=\"{}\"{dash}",
                s.gradient
                    .as_ref()
                    .map(|g| gradient_reference(defs, grad_id, g, shape, s.width * 0.5 + 1.))
                    .unwrap_or_else(|| svg_color(s.color)),
                s.width,
                s.cap.name().to_ascii_lowercase(),
                s.join.name().to_ascii_lowercase()
            )
        }
        _ => String::new(),
    };
    // Animation and portable project faces use glyph outlines: an SVG recipient
    // may not have the brand font. Text stays editable in the original document.
    if let Geom::Text(run) = &shape.geom
        && !text_as_paths
        && !run.font.starts_with("omatype:")
    {
        let family = crate::text::label_for(&run.font);
        let escaped = run
            .content
            .replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;");
        body.push_str(&format!(
            "  <text id=\"oma-{}\" x=\"{:.3}\" y=\"{:.3}\" font-family=\"{}\" font-size=\"{:.2}\" {fill_attr}{stroke_attr} opacity=\"{:.3}\"{extra}>{}</text>\n",
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
        Geom::Poly { winding: true, .. } | Geom::Paths { winding: true, .. } => "",
        _ => " fill-rule=\"evenodd\"",
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
    export_inner(doc, false, false)
}

pub fn export_frame(doc: &Document, layer: usize, frame_id: u64) -> Result<String, String> {
    export_inner(
        &crate::layout_export::frame_document(doc, layer, frame_id)?,
        false,
        true,
    )
}

pub fn export_animated(doc: &Document) -> Result<String, String> {
    Ok(animated_export(doc)?.svg)
}

/// Animated SVG plus every effect the file cannot carry.
pub struct SvgExport {
    pub svg: String,
    pub warnings: Vec<String>,
}

/// Build an animated SVG.
///
/// Mapped motion is CSS or SMIL. Move, rotate, scale, width, and height share
/// one transform. Opacity is its own track. Fill reveal and stroke reveal are
/// SMIL. A linear or radial gradient angle is `animateTransform` on
/// `gradientTransform`, added to the designed angle. Stroke width, fill color,
/// dash, gap, and dash offset are not mapped.
///
/// `warnings` names each unmapped effect and its layer or object. That includes
/// Apple glass, turbulence, displacement, color matrix, and a pixel layer that
/// is flattened into the file. A rotating linear or radial gradient is not a warning.
pub fn animated_export(doc: &Document) -> Result<SvgExport, String> {
    Ok(SvgExport {
        svg: export_inner(doc, true, true)?,
        warnings: export_warnings(doc),
    })
}

fn animate_tag(
    tag: &str,
    attribute: &str,
    extra: &str,
    motion: &crate::motion::Motion,
    times: &[f32],
    values: &str,
) -> String {
    let duration = motion.duration.max(0.05);
    let keys = times
        .iter()
        .map(|t| format!("{:.7}", (t / duration).clamp(0.0, 1.0)))
        .collect::<Vec<_>>()
        .join(";");
    let repeat = if motion.looped { "indefinite" } else { "1" };
    format!(
        "<{tag} attributeName=\"{attribute}\"{extra} dur=\"{duration:.4}s\" repeatCount=\"{repeat}\" fill=\"freeze\" calcMode=\"linear\" keyTimes=\"{keys}\" values=\"{values}\"/>\n"
    )
}

fn animate_attribute(
    attribute: &str,
    motion: &crate::motion::Motion,
    times: &[f32],
    values: impl Fn(f32) -> f32,
) -> String {
    let values = times
        .iter()
        .map(|t| format!("{:.6}", values(*t)))
        .collect::<Vec<_>>()
        .join(";");
    animate_tag("animate", attribute, "", motion, times, &values)
}

#[derive(Clone, Copy, PartialEq)]
enum SpinTarget {
    Fill,
    Stroke,
}

fn svg_gradient(kind: crate::gradient::GradientKind) -> bool {
    matches!(
        kind,
        crate::gradient::GradientKind::Linear | crate::gradient::GradientKind::Radial
    )
}

fn spin_placement(shape: &Shape) -> Option<(SpinTarget, Pt)> {
    let bounds = shape.geom.bbox();
    let center_of = |gradient: &crate::gradient::Gradient| {
        let (start, end) = gradient.endpoints(bounds);
        if gradient.kind == crate::gradient::GradientKind::Linear {
            (start + end) * 0.5
        } else {
            start
        }
    };
    match &shape.style.fill {
        Fill::Gradient(gradient) if svg_gradient(gradient.kind) => {
            Some((SpinTarget::Fill, center_of(gradient)))
        }
        Fill::Linear { .. } | Fill::Radial { .. } => {
            let gradient = shape.style.fill.gradient()?;
            Some((SpinTarget::Fill, center_of(&gradient)))
        }
        Fill::Gradient(_) => None,
        _ => {
            let gradient = shape.style.stroke.as_ref()?.gradient.as_ref()?;
            svg_gradient(gradient.kind).then(|| (SpinTarget::Stroke, center_of(gradient)))
        }
    }
}

fn gradient_turns(motion: &crate::motion::Motion, shape: u64, times: &[f32]) -> bool {
    motion.tracks.iter().any(|track| {
        track.shape == shape
            && track.prop == crate::motion::Prop::GradientAngle
            && !track.keys.is_empty()
    }) || times.iter().any(|time| {
        motion
            .pose(shape, *time)
            .gradient_angle
            .is_some_and(|angle| angle.abs() > 1e-3)
    })
}

fn gradient_motion(
    motion: &crate::motion::Motion,
    shape: &Shape,
    times: &[f32],
) -> (Option<SpinTarget>, String) {
    let Some((target, center)) = spin_placement(shape) else {
        return (None, String::new());
    };
    if !gradient_turns(motion, shape.id, times) {
        return (Some(target), String::new());
    }
    let values = times
        .iter()
        .map(|time| {
            let offset = motion
                .pose(shape.id, *time)
                .gradient_angle
                .unwrap_or(0.0);
            format!("{offset:.6} {:.4} {:.4}", center.x, center.y)
        })
        .collect::<Vec<_>>()
        .join(";");
    (
        Some(target),
        animate_tag(
            "animateTransform",
            "gradientTransform",
            " type=\"rotate\"",
            motion,
            times,
            &values,
        ),
    )
}

fn insert_gradient_motion(defs: &mut String, start: usize, motion: &str) {
    if motion.is_empty() || start > defs.len() {
        return;
    }
    let region = &defs[start..];
    let linear = region.find("</linearGradient>");
    let radial = region.find("</radialGradient>");
    let Some(at) = linear.into_iter().chain(radial).min() else {
        return;
    };
    defs.insert_str(start + at, motion);
}

fn svg_fx(fx: &crate::filter::Fx) -> bool {
    !matches!(
        fx,
        crate::filter::Fx::AppleGlass { .. }
            | crate::filter::Fx::Turbulence { .. }
            | crate::filter::Fx::Displacement { .. }
            | crate::filter::Fx::ColorMatrix { .. }
    )
}

fn motion_exported(shape: &Shape, prop: crate::motion::Prop) -> bool {
    use crate::motion::Prop;
    match prop {
        Prop::X
        | Prop::Y
        | Prop::Rotation
        | Prop::Scale
        | Prop::Width
        | Prop::Height
        | Prop::Opacity => true,
        Prop::FillReveal => {
            !shape.layout.frame && !shape.style.fill.is_none() && shape.geom.is_closed()
        }
        Prop::StrokeReveal => {
            !shape.layout.frame
                && shape
                    .style
                    .stroke
                    .as_ref()
                    .is_some_and(|stroke| stroke.width > 0.0)
        }
        Prop::GradientAngle => match spin_placement(shape) {
            Some((SpinTarget::Fill, _)) => {
                shape.layout.frame || (shape.geom.is_closed() && !shape.style.fill.is_none())
            }
            Some((SpinTarget::Stroke, _)) => {
                shape.layout.frame
                    || shape
                        .style
                        .stroke
                        .as_ref()
                        .is_some_and(|stroke| stroke.width > 0.0)
            }
            None => false,
        },
        Prop::StrokeWidth | Prop::Fill | Prop::Dash | Prop::Gap | Prop::DashLength => false,
    }
}

fn export_warnings(doc: &Document) -> Vec<String> {
    let mut warnings = Vec::new();
    for layer in &doc.layers {
        if layer.filters.active() {
            for fx in &layer.filters.items {
                if !svg_fx(fx) {
                    warnings.push(format!(
                        "{} on layer \"{}\" is not representable in SVG",
                        fx.name(),
                        layer.name
                    ));
                }
            }
        }
        match &layer.kind {
            LayerKind::Raster { pixels, .. } => {
                if !pixels.is_invisible() && !crate::compositor::is_paper_raster(layer) {
                    warnings.push(format!(
                        "Pixel layer \"{}\" is flattened into the SVG",
                        layer.name
                    ));
                }
            }
            LayerKind::Vector { shapes } => {
                for shape in shapes {
                    if shape.guide {
                        continue;
                    }
                    if shape.filters.active() {
                        for fx in &shape.filters.items {
                            if !svg_fx(fx) {
                                warnings.push(format!(
                                    "{} on \"{}\" in layer \"{}\" is not representable in SVG",
                                    fx.name(),
                                    shape.name,
                                    layer.name
                                ));
                            }
                        }
                    }
                    for track in doc
                        .motion
                        .tracks
                        .iter()
                        .filter(|track| track.shape == shape.id && !track.keys.is_empty())
                    {
                        if !motion_exported(shape, track.prop) {
                            warnings.push(format!(
                                "{} on \"{}\" in layer \"{}\" is not in the animated SVG",
                                track.prop.name(),
                                shape.name,
                                layer.name
                            ));
                        }
                    }
                }
            }
        }
    }
    warnings
}

fn write_animated_shape(
    body: &mut String,
    defs: &mut String,
    grad_id: &mut usize,
    shape: &Shape,
    motion: &crate::motion::Motion,
) {
    use crate::motion::Prop;
    let times = motion.sample_times(shape.id);
    let bounds = shape.world_bbox();
    let center = bounds.center();
    let (spin_target, spin) = gradient_motion(motion, shape, &times);
    body.push_str(&format!(
        "<g class=\"oma-a\" style=\"animation-name: oma-{}; transform-origin: {:.4}px {:.4}px\">\n",
        shape.id, center.x, center.y
    ));
    let mut component = shape.clone();
    component.opacity = 1.0;
    if !shape.style.fill.is_none() && shape.geom.is_closed() {
        component.style.stroke = None;
        body.push_str(&format!(
            "<g class=\"oma-a\" style=\"animation-name: oma-{}-opacity\">\n",
            shape.id
        ));
        let reveal = motion.value(shape.id, Prop::FillReveal, 0.0).is_some();
        if reveal {
            let id = format!("oma-fill-reveal-{}", shape.id);
            let initial = motion
                .pose(shape.id, 0.0)
                .fill_reveal
                .unwrap_or(1.0)
                .clamp(0.0, 1.0);
            defs.push_str(&format!("<clipPath id=\"{id}\" clipPathUnits=\"userSpaceOnUse\"><rect x=\"{:.4}\" y=\"{:.4}\" width=\"{:.4}\" height=\"{:.4}\">\n",bounds.min.x,bounds.max.y-bounds.height()*initial,bounds.width(),bounds.height()*initial));
            defs.push_str(&animate_attribute("y", motion, &times, |time| {
                bounds.max.y
                    - bounds.height()
                        * motion
                            .pose(shape.id, time)
                            .fill_reveal
                            .unwrap_or(1.0)
                            .clamp(0.0, 1.0)
            }));
            defs.push_str(&animate_attribute("height", motion, &times, |time| {
                bounds.height()
                    * motion
                        .pose(shape.id, time)
                        .fill_reveal
                        .unwrap_or(1.0)
                        .clamp(0.0, 1.0)
            }));
            defs.push_str("</rect></clipPath>\n");
            body.push_str(&format!("<g clip-path=\"url(#{id})\">\n"));
        }
        let defs_at = defs.len();
        let mut part = String::new();
        write_shape(
            &mut part,
            defs,
            grad_id,
            &component,
            &xf_attr(&component),
            true,
        );
        if spin_target == Some(SpinTarget::Fill) {
            insert_gradient_motion(defs, defs_at, &spin);
        }
        body.push_str(&part.replacen(
            &format!("id=\"oma-{}\"", shape.id),
            &format!("id=\"oma-{}-fill\"", shape.id),
            1,
        ));
        if reveal {
            body.push_str("</g>\n");
        }
        body.push_str("</g>\n");
    }
    if let Some(stroke) = shape
        .style
        .stroke
        .as_ref()
        .filter(|stroke| stroke.width > 0.0)
    {
        body.push_str(&format!(
            "<g class=\"oma-a\" style=\"animation-name: oma-{}-opacity\">\n",
            shape.id
        ));
        component.style.fill = Fill::None;
        component.style.stroke = Some(stroke.clone());
        let reveal = motion.value(shape.id, Prop::StrokeReveal, 0.0).is_some();
        if reveal {
            let id = format!("oma-stroke-reveal-{}", shape.id);
            let mask_bounds = bounds.inflate(stroke.width * 2.0 + 2.0);
            defs.push_str(&format!("<mask id=\"{id}\" maskUnits=\"userSpaceOnUse\" maskContentUnits=\"userSpaceOnUse\" mask-type=\"alpha\" x=\"{:.4}\" y=\"{:.4}\" width=\"{:.4}\" height=\"{:.4}\">\n",mask_bounds.min.x,mask_bounds.min.y,mask_bounds.width(),mask_bounds.height()));
            let closed = shape.geom.is_closed();
            let contours = shape.world_contours(128);
            let lengths: Vec<f32> = contours
                .iter()
                .map(|points| {
                    points
                        .windows(2)
                        .map(|pair| (pair[1] - pair[0]).length())
                        .sum::<f32>()
                        + if closed && points.len() > 1 {
                            (points[0] - points[points.len() - 1]).length()
                        } else {
                            0.0
                        }
                })
                .collect();
            let total: f32 = lengths.iter().sum();
            let mut passed = 0.0;
            // Add a near-zero sample so a round start cap is absent at zero,
            // but appears as soon as this contour begins drawing.
            for (points, length) in contours.iter().zip(lengths) {
                if length <= 1e-5 {
                    continue;
                }
                let offset = |time| {
                    length
                        - (total
                            * motion
                                .pose(shape.id, time)
                                .stroke_reveal
                                .unwrap_or(1.0)
                                .clamp(0.0, 1.0)
                            - passed)
                            .clamp(0.0, length)
                };
                defs.push_str(&format!("<path d=\"{}\" fill=\"none\" stroke=\"white\" stroke-width=\"{:.4}\" stroke-linecap=\"{}\" stroke-linejoin=\"{}\" stroke-dasharray=\"{length:.5} {length:.5}\" stroke-dashoffset=\"{:.5}\">\n",poly_d(points,closed),stroke.width+0.5,stroke.cap.name().to_ascii_lowercase(),stroke.join.name().to_ascii_lowercase(),offset(0.0)));
                defs.push_str(&animate_attribute(
                    "stroke-dashoffset",
                    motion,
                    &times,
                    offset,
                ));
                let mut cap_times = times.clone();
                for pair in times.windows(2) {
                    cap_times.push(pair[0] + (pair[1] - pair[0]) * 0.0001);
                }
                cap_times.sort_by(f32::total_cmp);
                defs.push_str(&animate_attribute("opacity", motion, &cap_times, |time| {
                    if total * motion.pose(shape.id, time).stroke_reveal.unwrap_or(1.0) > passed {
                        1.0
                    } else {
                        0.0
                    }
                }));
                defs.push_str("</path>\n");
                passed += length;
            }
            defs.push_str("</mask>\n");
            body.push_str(&format!("<g mask=\"url(#{id})\">\n"));
        }
        let defs_at = defs.len();
        let mut part = String::new();
        write_shape(
            &mut part,
            defs,
            grad_id,
            &component,
            &xf_attr(&component),
            true,
        );
        if spin_target == Some(SpinTarget::Stroke) {
            insert_gradient_motion(defs, defs_at, &spin);
        }
        body.push_str(&part.replacen(
            &format!("id=\"oma-{}\"", shape.id),
            &format!("id=\"oma-{}-stroke\"", shape.id),
            1,
        ));
        if reveal {
            body.push_str("</g>\n");
        }
        body.push_str("</g>\n");
    }
    body.push_str("</g>\n");
}

/// A self-contained vector node for faithful non-rectangular HTML artwork.
pub(crate) fn shape_fragment(shape: &Shape) -> String {
    let bounds = shape.world_bbox();
    let mut body = String::new();
    let mut defs = String::new();
    write_shape(
        &mut body,
        &mut defs,
        &mut (shape.id as usize),
        shape,
        &xf_attr(shape),
        true,
    );
    body = body.replace(
        &format!("id=\"oma-{}\"", shape.id),
        &format!("id=\"oma-vector-{}\"", shape.id),
    );
    format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" xmlns:inkscape=\"http://www.inkscape.org/namespaces/inkscape\" viewBox=\"{} {} {} {}\" width=\"100%\" height=\"100%\" preserveAspectRatio=\"none\" aria-hidden=\"true\"><defs>{defs}</defs>{body}</svg>",
        bounds.min.x,
        bounds.min.y,
        bounds.width().max(1.0),
        bounds.height().max(1.0)
    )
}

fn write_shape_tree(
    shapes: &[Shape],
    body: &mut String,
    defs: &mut String,
    grad_id: &mut usize,
    css: &mut String,
    motion: &crate::motion::Motion,
    animate: bool,
    text_as_paths: bool,
) {
    let frames: std::collections::HashSet<_> = shapes
        .iter()
        .filter(|s| s.layout.frame)
        .map(|s| s.id)
        .collect();
    let mut children: std::collections::HashMap<Option<u64>, Vec<&Shape>> =
        std::collections::HashMap::new();
    for shape in shapes {
        children
            .entry(shape.layout.parent.filter(|id| frames.contains(id)))
            .or_default()
            .push(shape);
    }
    fn visit(
        parent: Option<u64>,
        children: &std::collections::HashMap<Option<u64>, Vec<&Shape>>,
        body: &mut String,
        defs: &mut String,
        grad_id: &mut usize,
        css: &mut String,
        motion: &crate::motion::Motion,
        animate: bool,
        text_as_paths: bool,
        depth: usize,
    ) {
        if depth >= 64 {
            return;
        }
        for shape in children.get(&parent).into_iter().flatten() {
            if shape.guide {
                continue;
            }
            let mut extra = String::new();
            if shape.filters.active() {
                let fid = format!("oma-fx-s{}", shape.id);
                let r = shape
                    .world_bbox()
                    .inflate(crate::filter::svg_pad(&shape.filters));
                if let Some(filter) = crate::filter::svg_filter(
                    &fid,
                    &shape.filters,
                    [r.min.x, r.min.y, r.width().max(1.0), r.height().max(1.0)],
                ) {
                    defs.push_str(&filter);
                    extra.push_str(&format!(" filter=\"url(#{fid})\""));
                }
            }
            if shape.layout.frame {
                let animated = animate
                    .then(|| {
                        motion.css_keyframes(shape.id, &format!("oma-{}", shape.id), shape.opacity)
                    })
                    .flatten();
                if let Some(keyframes) = &animated {
                    css.push_str(keyframes);
                    let center = shape.world_bbox().center();
                    body.push_str(&format!("<g class=\"oma-a\" style=\"animation-name:oma-{},oma-{}-opacity;transform-origin:{:.4}px {:.4}px\">\n",shape.id,shape.id,center.x,center.y));
                }
                body.push_str(&format!(
                    "<g id=\"oma-frame-{}\" style=\"mix-blend-mode:{};isolation:isolate\" opacity=\"{:.3}\"{}{}{extra}>\n",
                    shape.id,
                    shape.blend.css(),
                    if animated.is_some() {
                        1.0
                    } else {
                        shape.opacity
                    },
                    if shape.visible {
                        ""
                    } else {
                        " visibility=\"hidden\""
                    },
                    xf_attr(shape)
                ));
                let mut background = (*shape).clone();
                background.rotation = 0.0;
                background.opacity = 1.0;
                background.blend = crate::color::Blend::Normal;
                background.mask = None;
                let object_mask = object_mask_definition(defs, shape);
                if let Some(id) = &object_mask {
                    body.push_str(&format!("<g mask=\"url(#{id})\">"));
                }
                let defs_at = defs.len();
                write_shape(body, defs, grad_id, &background, "", false);
                if animate {
                    let (target, spin) =
                        gradient_motion(motion, shape, &motion.sample_times(shape.id));
                    if target.is_some() {
                        insert_gradient_motion(defs, defs_at, &spin);
                    }
                }
                if shape.layout.clip {
                    let clip_id = format!("oma-frame-clip-{}", shape.id);
                    defs.push_str(&format!("<clipPath id=\"{clip_id}\" clipPathUnits=\"userSpaceOnUse\"><path d=\"{}\"/></clipPath>\n",path_data(&background)));
                    body.push_str(&format!("<g clip-path=\"url(#{clip_id})\">\n"));
                }
                visit(
                    Some(shape.id),
                    children,
                    body,
                    defs,
                    grad_id,
                    css,
                    motion,
                    animate,
                    text_as_paths,
                    depth + 1,
                );
                if shape.layout.clip {
                    body.push_str("</g>\n");
                }
                if object_mask.is_some() {
                    body.push_str("</g>\n");
                }
                body.push_str("</g>\n");
                if animated.is_some() {
                    body.push_str("</g>\n");
                }
            } else if let Some(keyframes) = animate
                .then(|| {
                    motion.css_keyframes(shape.id, &format!("oma-{}", shape.id), shape.opacity)
                })
                .flatten()
            {
                css.push_str(&keyframes);
                body.push_str(&format!("<g{extra}>\n"));
                write_animated_shape(body, defs, grad_id, shape, motion);
                body.push_str("</g>\n");
            } else {
                extra.push_str(&xf_attr(shape));
                write_shape(body, defs, grad_id, shape, &extra, text_as_paths);
            }
        }
    }
    visit(
        None,
        &children,
        body,
        defs,
        grad_id,
        css,
        motion,
        animate,
        text_as_paths,
        0,
    );
}

fn export_inner(doc: &Document, animate: bool, text_as_paths: bool) -> Result<String, String> {
    doc.validate_hierarchy()?;
    let mut body = String::new();
    let mut layer_outputs = std::collections::HashMap::<u64, String>::new();
    let mut defs = String::new();
    let mut grad_id = 0usize;
    let mut css = String::new();
    let motion = &doc.motion;
    let looping = if motion.looped { "infinite" } else { "1" };
    if animate && !motion.is_empty() {
        css.push_str(&format!(
            ".oma-a {{ animation-duration: {:.3}s; animation-iteration-count: {looping}; animation-fill-mode: both; animation-timing-function: linear; transform-box: view-box; }}\n",
            motion.duration.max(0.05)
        ));
    }

    // Serialize descendants before parents, independent of their flat storage order.
    let by_id: std::collections::HashMap<_, _> = doc.layers.iter().map(|l| (l.id, l)).collect();
    let mut children = std::collections::HashMap::<u64, Vec<&Layer>>::new();
    for layer in &doc.layers {
        if let Some(parent) = layer.parent {
            children.entry(parent).or_default().push(layer);
        }
    }
    let mut order: Vec<_> = doc.layers.iter().collect();
    order.sort_by_cached_key(|layer| {
        let mut depth = 0;
        let mut parent = layer.parent;
        while let Some(id) = parent {
            depth += 1;
            parent = by_id[&id].parent;
        }
        std::cmp::Reverse(depth)
    });
    for layer in order {
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
        if layer.is_group {
            for child in children.get(&layer.id).into_iter().flatten() {
                if let Some(output) = layer_outputs.get(&child.id) {
                    layer_body.push_str(output);
                }
            }
        }
        match &layer.kind {
            LayerKind::Vector { shapes } => {
                write_shape_tree(
                    shapes,
                    &mut layer_body,
                    &mut defs,
                    &mut grad_id,
                    &mut css,
                    motion,
                    animate,
                    text_as_paths,
                );
            }
            LayerKind::Raster { pixels, .. } => {
                if !pixels.is_invisible() && !crate::compositor::is_paper_raster(layer) {
                    layer_body.push_str(&pixel_image(
                        pixels,
                        crate::compositor::layer_pixel_transform(layer),
                    )?);
                }
            }
        }
        if layer_body.is_empty() && !layer.is_group {
            continue;
        }
        let mut output = String::new();
        output.push_str(&format!(
            "<g id=\"oma-layer-{}\" inkscape:label=\"{}\"{} opacity=\"{:.3}\" style=\"mix-blend-mode:{};isolation:{}\"{fx_attr}>\n",
            layer.id, xml_escape(&layer.name), if layer.visible { "" } else { " visibility=\"hidden\"" },
            layer.opacity,
            layer.blend.css(),
            if layer.is_group && layer.pass_through { "auto" } else { "isolate" }
        ));
        // Canvas masks the layer before applying its effects. Keep the mask on
        // an inner group so SVG's filter-before-mask order cannot reverse that.
        if let Some(mask_id) = write_layer_mask(&mut defs, layer)? {
            output.push_str(&format!("<g mask=\"url(#{mask_id})\">\n{layer_body}</g>\n"));
        } else {
            output.push_str(&layer_body);
        }
        output.push_str("</g>\n");
        layer_outputs.insert(layer.id, output);
    }

    for layer in doc.layers.iter().filter(|layer| layer.parent.is_none()) {
        if let Some(output) = layer_outputs.get(&layer.id) {
            body.push_str(output);
        }
    }

    let style = if css.is_empty() {
        String::new()
    } else {
        format!("<style>\n{css}</style>\n")
    };
    Ok(format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<svg xmlns=\"http://www.w3.org/2000/svg\" xmlns:inkscape=\"http://www.inkscape.org/namespaces/inkscape\" width=\"{}\" height=\"{}\" viewBox=\"0 0 {} {}\">\n{style}<defs>\n{defs}</defs>\n{body}</svg>\n",
        doc.width, doc.height, doc.width, doc.height
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::{Cmd, Shape, Style, apply};
    use crate::geom::{Geom, Pt};

    #[test]
    fn project_type_exports_its_glyphs_without_requiring_the_brand_font() {
        let mut run = crate::geom::TypeRun {
            content: "Brand Ω".into(),
            origin: Pt::new(20., 80.),
            px: 48.,
            ..Default::default()
        };
        run.contours = crate::text::shape(&run);
        assert!(!run.contours.is_empty());
        // A saved glyph outline remains exportable even without its font kit.
        run.font = "omatype:missing-on-this-machine".into();
        let mut shape = Shape::new(Geom::Text(run), Style::default());
        shape.rotation = 0.2;
        let expected = path_data(&shape);
        let mut doc = Document::new("Portable type", 4., 3., 72.);
        doc.layers = vec![Layer::vector("Type")];
        doc.layers[0].kind.shapes_mut().unwrap().push(shape.clone());

        let svg = export(&doc).unwrap();
        assert!(svg.contains(&format!("d=\"{expected}\"")), "{svg}");
        assert!(svg.contains("transform=\"rotate("));
        assert!(!svg.contains("<text") && !svg.contains("font-family"));
        assert_eq!(doc.layers[0].kind.shapes().unwrap()[0].geom, shape.geom);

        if let Geom::Text(run) = &mut doc.layers[0].kind.shapes_mut().unwrap()[0].geom {
            run.font.clear();
        }
        assert!(export(&doc).unwrap().contains("<text"));
    }

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
                            Anchor::smooth(
                                crate::geom::Pt::new(80.0, 40.0),
                                crate::geom::Pt::new(20.0, 10.0),
                            ),
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
        assert!(
            s.contains("stroke=\"#"),
            "opaque stroke must be hex, got {s}"
        );
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

    fn rotating_gradient(kind: crate::gradient::GradientKind) -> SvgExport {
        let mut doc = Document::new("Gradient", 200.0, 120.0, 72.0);
        let mut shape = Shape::new(
            Geom::Rect {
                origin: Pt::new(10.0, 10.0),
                size: Pt::new(80.0, 40.0),
                radius: 0.0,
            },
            Style {
                fill: Fill::Gradient(crate::gradient::Gradient::new(
                    kind,
                    crate::color::Rgba::rgb(20, 40, 200),
                    crate::color::Rgba::rgb(240, 180, 40),
                )),
                stroke: None,
            },
        );
        shape.name = "Swatch".into();
        let id = shape.id;
        apply(&mut doc, &Cmd::AddShape { layer: 1, shape });
        doc.layers[1].name = "Art".into();
        doc.motion.duration = 1.0;
        doc.motion
            .set_key(id, crate::motion::Prop::GradientAngle, 0.0, 0.0, crate::motion::Ease::Linear);
        doc.motion.set_key(
            id,
            crate::motion::Prop::GradientAngle,
            1.0,
            90.0,
            crate::motion::Ease::Linear,
        );
        animated_export(&doc).unwrap()
    }

    #[test]
    fn rotating_linear_gradient_is_an_animate_transform() {
        let report = rotating_gradient(crate::gradient::GradientKind::Linear);
        assert!(
            report.warnings.is_empty(),
            "a rotating gradient is expressible: {:?}",
            report.warnings
        );
        let parsed = roxmltree::Document::parse(&report.svg).expect("svg");
        let spin = parsed
            .descendants()
            .find(|node| {
                node.tag_name().name() == "animateTransform"
                    && node.attribute("attributeName") == Some("gradientTransform")
            })
            .expect("animateTransform");
        assert_eq!(spin.attribute("type"), Some("rotate"));
        assert_eq!(
            spin.parent().unwrap().tag_name().name(),
            "linearGradient",
            "{}",
            report.svg
        );
        let angles: Vec<f32> = spin
            .attribute("values")
            .unwrap()
            .split(';')
            .filter_map(|sample| sample.split_whitespace().next()?.parse().ok())
            .collect();
        assert!(
            angles.windows(2).any(|pair| (pair[0] - pair[1]).abs() > 1.0),
            "values must change: {angles:?}"
        );
        assert!(angles.iter().any(|angle| angle.abs() < 0.01), "{angles:?}");
        assert!(
            angles.iter().any(|angle| (angle - 90.0).abs() < 0.01),
            "{angles:?}"
        );
        let gradient = spin.parent().unwrap();
        let y1: f32 = gradient.attribute("y1").unwrap().parse().unwrap();
        let y2: f32 = gradient.attribute("y2").unwrap().parse().unwrap();
        assert!(
            (y1 - y2).abs() < 0.01,
            "the designed gradient stays put, got y1={y1} y2={y2}"
        );
        assert!(report.svg.contains("@keyframes"), "{}", report.svg);
    }

    #[test]
    fn rotating_radial_gradient_spins_around_its_center() {
        let report = rotating_gradient(crate::gradient::GradientKind::Radial);
        assert!(report.warnings.is_empty(), "{:?}", report.warnings);
        let parsed = roxmltree::Document::parse(&report.svg).unwrap();
        let spin = parsed
            .descendants()
            .find(|node| node.tag_name().name() == "animateTransform")
            .expect("animateTransform");
        assert_eq!(spin.parent().unwrap().tag_name().name(), "radialGradient");
        let values = spin.attribute("values").unwrap();
        let angles: Vec<f32> = values
            .split(';')
            .filter_map(|sample| sample.split_whitespace().next()?.parse().ok())
            .collect();
        assert!(angles.windows(2).any(|pair| (pair[0] - pair[1]).abs() > 1.0), "{values}");
        assert!(values.contains("50.0000") && values.contains("30.0000"), "{values}");
    }

    #[test]
    fn unsupported_effect_names_the_shape_and_layer() {
        let mut doc = Document::new("Effects", 120.0, 80.0, 72.0);
        let mut shape = Shape::new(
            Geom::Rect {
                origin: Pt::new(12.0, 12.0),
                size: Pt::new(40.0, 24.0),
                radius: 0.0,
            },
            Style::default(),
        );
        shape.name = "Badge".into();
        shape.filters.items.push(crate::filter::Fx::Turbulence {
            fractal: true,
            base: 0.04,
            octaves: 2,
            seed: 1,
        });
        apply(&mut doc, &Cmd::AddShape { layer: 1, shape });
        doc.layers[1].name = "Art".into();
        doc.layers[1].filters.items.push(crate::filter::Fx::Displacement {
            scale: 12.0,
            x_ch: 0,
            y_ch: 1,
        });
        let report = animated_export(&doc).unwrap();
        roxmltree::Document::parse(&report.svg).expect("svg");
        assert!(
            report.warnings.iter().any(|warning| {
                warning.contains("Turbulence") && warning.contains("Badge") && warning.contains("Art")
            }),
            "{:?}",
            report.warnings
        );
        assert!(
            report.warnings.iter().any(|warning| {
                warning.contains("Displacement") && warning.contains("Art")
            }),
            "{:?}",
            report.warnings
        );
    }

    #[test]
    fn flattened_pixels_and_unmapped_motion_are_named() {
        let mut doc = Document::new("Mixed", 80.0, 80.0, 72.0);
        let mut shape = Shape::new(
            Geom::Rect {
                origin: Pt::new(8.0, 8.0),
                size: Pt::new(30.0, 20.0),
                radius: 0.0,
            },
            Style::default(),
        );
        shape.name = "Badge".into();
        let id = shape.id;
        apply(&mut doc, &Cmd::AddShape { layer: 1, shape });
        doc.layers[1].name = "Art".into();
        doc.layers[1].filters.items.push(crate::filter::Fx::ColorMatrix {
            values: [0.0; 20],
        });
        doc.motion.set_key(
            id,
            crate::motion::Prop::StrokeWidth,
            1.0,
            6.0,
            crate::motion::Ease::Linear,
        );
        let mut photo = Layer::raster("Photo", 4, 4);
        if let LayerKind::Raster { pixels, .. } = &mut photo.kind {
            pixels.data.fill(255);
        }
        doc.layers.push(photo);
        let report = animated_export(&doc).unwrap();
        roxmltree::Document::parse(&report.svg).expect("svg");
        assert!(
            report.warnings.iter().any(|warning| {
                warning.contains("Stroke width")
                    && warning.contains("Badge")
                    && warning.contains("Art")
            }),
            "{:?}",
            report.warnings
        );
        assert!(
            report.warnings.iter().any(|warning| {
                warning.contains("Photo") && warning.contains("flattened")
            }),
            "{:?}",
            report.warnings
        );
        assert!(
            report
                .warnings
                .iter()
                .any(|warning| warning.contains("Color matrix") && warning.contains("Art")),
            "{:?}",
            report.warnings
        );
        assert!(
            report
                .warnings
                .iter()
                .all(|warning| !warning.contains("Gradient")),
            "{:?}",
            report.warnings
        );
    }
}

#[cfg(test)]
mod mask_tests {
    use super::*;
    use crate::document::Pixels;
    use base64::Engine as _;

    #[test]
    fn svg_masks_preserve_luminance_alpha_placement_and_filter_order() {
        let mut doc = Document::new("Mask export", 160.0, 120.0, 72.0);
        doc.transparent = true;
        let mut layer = Layer::raster("Masked image", 4, 2);
        if let LayerKind::Raster {
            pixels,
            origin,
            size,
            rotation,
        } = &mut layer.kind
        {
            pixels.data = [255, 0, 0, 255].repeat(8);
            *origin = Pt::new(40.0, 30.0);
            *size = Pt::new(80.0, 40.0);
            *rotation = std::f32::consts::FRAC_PI_2;
        }
        let mask_data = [
            0, 0, 0, 255, 255, 255, 255, 64, 128, 128, 128, 128, 255, 255, 255, 255,
        ]
        .repeat(2);
        layer.mask = Some(Pixels::from_rgba(4, 2, mask_data.clone()).unwrap());
        layer
            .filters
            .items
            .push(crate::filter::Fx::Blur { std: 2.0 });
        let id = layer.id;
        doc.layers = vec![layer];
        let svg = export(&doc).unwrap();
        let mask = svg
            .split("<mask ")
            .nth(1)
            .unwrap()
            .split("</mask>")
            .next()
            .unwrap();
        assert!(mask.contains("mask-type=\"luminance\""));
        assert!(mask.contains("color-interpolation=\"sRGB\""));
        let matrix: Vec<f32> = mask
            .split("matrix(")
            .nth(1)
            .unwrap()
            .split(')')
            .next()
            .unwrap()
            .split_whitespace()
            .map(|value| value.parse().unwrap())
            .collect();
        assert_eq!(matrix.len(), 6);
        for (actual, expected) in matrix.iter().zip([0.0, 20.0, -20.0, 0.0, 100.0, 10.0]) {
            assert!(
                (actual - expected).abs() < 0.001,
                "mask placement: {matrix:?}"
            );
        }
        let b64 = mask
            .split("data:image/png;base64,")
            .nth(1)
            .unwrap()
            .split('"')
            .next()
            .unwrap();
        let png = base64::engine::general_purpose::STANDARD
            .decode(b64)
            .unwrap();
        assert_eq!(
            image::load_from_memory(&png).unwrap().to_rgba8().into_raw(),
            mask_data
        );
        let filter = svg.find(&format!(" filter=\"url(#oma-fx-{id})\"")).unwrap();
        let masking = svg
            .find(&format!("<g mask=\"url(#oma-mask-{id})\""))
            .unwrap();
        assert!(
            filter < masking,
            "mask must be nested inside the outer filter group"
        );
        let decoded = crate::project::decode(&crate::project::encode(&doc).unwrap()).unwrap();
        assert_eq!(decoded.layers[0].mask.as_ref().unwrap().data, mask_data);
    }
}
