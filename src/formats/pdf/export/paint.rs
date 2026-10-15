//! PDF vector paint fields and reusable SVG pattern tiles.
use super::*;
use crate::gradient::Spread;

/// Keep all visible page coordinates when a paint is evaluated before object rotation.
/// Args: page bounds, rotation center and radians. Returns a conservative unrotated page rectangle.
pub(super) fn local_bounds(bounds: Bounds, center: Pt, rotation: f32) -> Bounds {
    let corners = [
        bounds.min,
        Pt::new(bounds.max.x, bounds.min.y),
        bounds.max,
        Pt::new(bounds.min.x, bounds.max.y),
    ];
    let mut result = Bounds::from_pt(corners[0].rotate_about(center, -rotation));
    for point in corners {
        result.union_pt(point.rotate_about(center, -rotation));
    }
    result
}

/// Encode every visible period as explicit axial or two-circle vector shading segments.
/// Args: object store, normalized gradient, object and page bounds, channel selection. Returns a PDF shading.
pub(super) fn field(
    pdf: &mut Pdf,
    g: &Gradient,
    bounds: Bounds,
    extent: Bounds,
    alpha: bool,
) -> Result<Vec<ObjectId>, String> {
    let corners = [
        extent.min,
        Pt::new(extent.max.x, extent.min.y),
        extent.max,
        Pt::new(extent.min.x, extent.max.y),
    ];
    let radial = g.kind == GradientKind::Radial;
    let (start, end, coords) = if radial {
        let inverse = g
            .radial_transform(bounds)
            .invert()
            .filter(|t| t.is_finite())
            .ok_or("PDF radial paint matrix is singular")?;
        let mut points = corners.map(|p| tiny_skia::Point::from_xy(p.x, p.y));
        inverse.map_points(&mut points);
        let f = Pt::from_array(g.focal);
        let r = g.inner_radius.max(0.);
        let dr = 1. - r;
        let distance = points
            .iter()
            .map(|p| (Pt::new(p.x, p.y) - f).length())
            .fold(0., f32::max);
        let (start, end) = if dr.abs() < 1e-6 {
            let length = f.length();
            if length < 1e-6 {
                return Err("PDF periodic radial circles are coincident".into());
            }
            let positions = points.map(|p| 1. - Pt::new(p.x, p.y).dot(f) / f.length_sq());
            (
                (positions.into_iter().fold(f32::INFINITY, f32::min) - r / length).floor(),
                (positions.into_iter().fold(f32::NEG_INFINITY, f32::max) + r / length).ceil(),
            )
        } else {
            let a = f.length_sq() - dr * dr;
            if a.abs() < 1e-6 {
                return Err("PDF periodic radial field has an unbounded tangent cone".into());
            }
            let b = 2. * (distance * f.length() + r * dr.abs());
            let c = distance * distance + r * r;
            let reach = ((b + (b * b + 4. * a.abs() * c).sqrt()) / (2. * a.abs())).ceil() + 1.;
            if dr > 0. {
                (-r / dr, reach)
            } else {
                (-reach, -r / dr)
            }
        };
        let first = f * (1. - start);
        let last = f * (1. - end);
        (
            start,
            end,
            vec![
                first.x,
                first.y,
                (r + start * dr).max(0.),
                last.x,
                last.y,
                (r + end * dr).max(0.),
            ],
        )
    } else {
        let (a, b) = g.endpoints(bounds);
        let d = b - a;
        if d.length_sq() < 1e-12 {
            return Err("PDF periodic linear paint has coincident endpoints".into());
        }
        let positions = corners.map(|p| (p - a).dot(d) / d.length_sq());
        let start = positions.into_iter().fold(f32::INFINITY, f32::min).floor();
        let end = positions
            .into_iter()
            .fold(f32::NEG_INFINITY, f32::max)
            .ceil()
            .max(start + 1.);
        let first = a + d * start;
        let last = a + d * end;
        (start, end, vec![first.x, first.y, last.x, last.y])
    };
    if !start.is_finite()
        || !end.is_finite()
        || end <= start
        || end - start > 4096.
        || coords.iter().any(|v| !v.is_finite())
    {
        return Err("PDF periodic paint exceeds 4096 finite vector periods".into());
    }
    let channels = |color: Rgba| {
        if alpha {
            vec![color.a as f32 / 255.]
        } else {
            vec![
                color.r as f32 / 255.,
                color.g as f32 / 255.,
                color.b as f32 / 255.,
            ]
        }
    };
    let sample = |position: f32, right: bool| {
        let first = &g.stops[0];
        if position < first.offset || (!right && position == first.offset) {
            return channels(first.color);
        }
        for pair in g.stops.windows(2) {
            if (position < pair[1].offset || (!right && position == pair[1].offset))
                && pair[1].offset > pair[0].offset
            {
                let t =
                    ((position - pair[0].offset) / (pair[1].offset - pair[0].offset)).clamp(0., 1.);
                return channels(pair[0].color)
                    .into_iter()
                    .zip(channels(pair[1].color))
                    .map(|(a, b)| a + (b - a) * t)
                    .collect();
            }
        }
        channels(g.stops.last().unwrap().color)
    };
    let mut shadings = vec![];
    for cycle in start.floor() as i64..end.ceil() as i64 {
        let cycle = cycle as f32;
        let reverse = g.spread == Spread::Reflect && (cycle as i64).rem_euclid(2) == 1;
        let lower = (start - cycle).max(0.);
        let upper = (end - cycle).min(1.);
        if upper <= lower {
            continue;
        }
        let mut functions = vec![];
        let mut cuts = vec![];
        let mut stops = vec![lower, upper];
        stops.extend(
            g.stops
                .iter()
                .map(|s| if reverse { 1. - s.offset } else { s.offset })
                .filter(|p| *p > lower && *p < upper),
        );
        stops.sort_by(f32::total_cmp);
        stops.dedup();
        for pair in stops.windows(2) {
            let from = sample(if reverse { 1. - pair[0] } else { pair[0] }, !reverse);
            let to = sample(if reverse { 1. - pair[1] } else { pair[1] }, reverse);
            if !functions.is_empty() {
                cuts.push(Object::Real((pair[0] - lower) / (upper - lower)));
            }
            functions.push(Object::Reference(pdf.add_object(dictionary!{"FunctionType"=>2,"Domain"=>vec![0.into(),1.into()],"C0"=>from.into_iter().map(Object::Real).collect::<Vec<_>>(),"C1"=>to.into_iter().map(Object::Real).collect::<Vec<_>>(),"N"=>1})));
        }
        let encode = functions
            .iter()
            .flat_map(|_| [0.into(), 1.into()])
            .collect::<Vec<Object>>();
        let function=pdf.add_object(dictionary!{"FunctionType"=>3,"Domain"=>vec![0.into(),1.into()],"Functions"=>functions,"Bounds"=>cuts,"Encode"=>encode});
        let first = cycle + lower;
        let last = cycle + upper;
        let coords = if radial {
            let f = Pt::from_array(g.focal);
            let r = g.inner_radius.max(0.);
            let dr = 1. - r;
            let a = f * (1. - first);
            let b = f * (1. - last);
            vec![
                a.x,
                a.y,
                (r + first * dr).max(0.),
                b.x,
                b.y,
                (r + last * dr).max(0.),
            ]
        } else {
            let (a, b) = g.endpoints(bounds);
            let d = b - a;
            let from = a + d * first;
            let to = a + d * last;
            vec![from.x, from.y, to.x, to.y]
        };
        shadings.push(pdf.add_object(dictionary!{"ShadingType"=>if radial{3}else{2},"ColorSpace"=>if alpha{"DeviceGray"}else{"DeviceRGB"},"Coords"=>coords.into_iter().map(Object::Real).collect::<Vec<_>>(),"Function"=>function,"Extend"=>vec![Object::Boolean(false),Object::Boolean(false)]}));
    }
    Ok(shadings)
}

impl Writer {
    /// Paint a retained SVG pattern as a reusable PDF vector tile.
    /// Args: current object, retained paint, page bounds and output. Returns a source or encoding error.
    pub(super) fn pattern(
        &mut self,
        shape: &Shape,
        pattern: &crate::svg_source::SvgPattern,
        bounds: Bounds,
        out: &mut Scene,
    ) -> Result<(), String> {
        if self.pattern_depth >= 32 {
            return Err("PDF pattern nesting exceeds32".into());
        }
        self.pattern_depth += 1;
        let result = self.pattern_inner(shape, pattern, bounds, out);
        self.pattern_depth -= 1;
        result
    }
    /// Resolve a tile using the retained SVG definitions and current object bounds.
    /// Args: object, source pattern, page bounds and destination. Returns any normalization or paint error.
    fn pattern_inner(
        &mut self,
        shape: &Shape,
        pattern: &crate::svg_source::SvgPattern,
        bounds: Bounds,
        out: &mut Scene,
    ) -> Result<(), String> {
        let b = shape.geom.bbox();
        if pattern.transform.iter().any(|v| !v.is_finite()) || !pattern.opacity.is_finite() {
            return Err("PDF pattern placement and opacity must be finite".into());
        }
        let (defs, paint) = pattern.definition("pdf-pattern-", b);
        let source = format!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{}\" height=\"{}\"><defs>{defs}</defs><rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"{paint}\"/></svg>",
            b.max.x.max(1.),
            b.max.y.max(1.),
            b.min.x,
            b.min.y,
            b.width(),
            b.height()
        );
        let tree = crate::svg_source::scene(&source)?;
        let p = tree
            .patterns()
            .iter()
            .find(|p| p.id() == "pdf-pattern-paint")
            .ok_or("PDF pattern source could not be resolved")?;
        let rect = p.rect();
        let normalized = tree.to_string(&usvg::WriteOptions::default());
        let xml = roxmltree::Document::parse(&normalized).map_err(|e| e.to_string())?;
        let definition = xml
            .descendants()
            .find(|n| n.has_tag_name("pattern") && n.attribute("id") == Some(p.id()))
            .ok_or("Missing normalized PDF pattern tile")?;
        let content = definition
            .children()
            .filter(|n| n.is_element())
            .map(|n| &normalized[n.range()])
            .collect::<String>();
        let definitions = xml
            .descendants()
            .find(|n| n.has_tag_name("defs"))
            .map(|n| &normalized[n.range()])
            .unwrap_or("");
        let tile = format!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" xmlns:xlink=\"http://www.w3.org/1999/xlink\" width=\"{}\" height=\"{}\">{definitions}{content}</svg>",
            rect.width(),
            rect.height()
        );
        let (doc, notes) = crate::formats::svg::read(&tile, "PDF pattern tile")?;
        for note in notes {
            self.warnings
                .insert(format!("PDF pattern on object {}: {note}", shape.id));
        }
        let (prepared, notes) = fallback::prepare(&doc)?;
        for note in notes {
            self.warnings
                .insert(format!("PDF pattern on object {}: {note}", shape.id));
        }
        let mut scene = Scene::default();
        self.children(
            &prepared,
            None,
            Bounds::from_min_size(Pt::ZERO, Pt::new(rect.width(), rect.height())),
            &mut scene,
            0,
            true,
        )?;
        let rotation = tiny_skia::Transform::from_rotate_at(
            shape.rotation.to_degrees(),
            b.center().x,
            b.center().y,
        );
        let t = rotation
            .pre_concat(p.transform())
            .pre_translate(rect.x(), rect.y());
        let encoded = scene.encode()?;
        let id=self.pdf.add_object(Stream::new(dictionary!{"Type"=>"Pattern","PatternType"=>1,"PaintType"=>1,"TilingType"=>1,"BBox"=>vec![0.into(),0.into(),Object::Real(rect.width()),Object::Real(rect.height())],"XStep"=>rect.width(),"YStep"=>rect.height(),"Matrix"=>vec![t.sx,t.ky,t.kx,t.sy,t.tx,t.ty].into_iter().map(Object::Real).collect::<Vec<_>>(),"Resources"=>scene.resources},encoded));
        let mut painted = Scene::default();
        painted.ops.push(Operation::new(
            "cs",
            vec![Object::Name(b"Pattern".to_vec())],
        ));
        painted.invoke("scn", "Pattern", id);
        path(&mut painted.ops, shape);
        op(
            &mut painted.ops,
            if shape.geom.nonzero_winding() {
                "f"
            } else {
                "f*"
            },
            &[],
        );
        let form = self.form(painted, bounds, true)?;
        let state = self.state(byte(pattern.opacity), byte(pattern.opacity), Blend::Normal);
        op(&mut out.ops, "q", &[]);
        out.invoke("gs", "ExtGState", state);
        out.invoke("Do", "XObject", form);
        op(&mut out.ops, "Q", &[]);
        Ok(())
    }
}
