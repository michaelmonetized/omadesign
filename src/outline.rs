//! Expand the renderer's stroke geometry into editable filled contours.
use crate::document::Shape;
use crate::geom::{Geom, Pt, flatten_cubic};
use tiny_skia::{PathSegment, StrokeDash};

pub fn expand(shape: &Shape) -> Option<Geom> {
    let stroke = shape
        .style
        .stroke
        .as_ref()
        .filter(|s| s.width.is_finite() && s.width > 0.0)?;
    let path = shape.get_cached_path(96)?;
    let dashed = match stroke
        .dash
        .and_then(|(on, off)| StrokeDash::new(vec![on, off], 0.0))
    {
        Some(dash) => Some(path.dash(&dash, 1.0)?),
        None => None,
    };
    let expanded = dashed.as_ref().unwrap_or(&path).stroke(
        &tiny_skia::Stroke {
            width: stroke.width,
            line_cap: stroke.cap.to_skia(),
            line_join: stroke.join.to_skia(),
            ..Default::default()
        },
        1.0,
    )?;
    let point = |p: tiny_skia::Point| Pt::new(p.x, p.y);
    let mut contours = vec![];
    let mut contour = vec![];
    let mut last = Pt::ZERO;
    for segment in expanded.segments() {
        match segment {
            PathSegment::MoveTo(p) => {
                if contour.len() >= 3 {
                    contours.push(std::mem::take(&mut contour));
                }
                contour.clear();
                last = point(p);
                contour.push(last);
            }
            PathSegment::LineTo(p) => {
                last = point(p);
                contour.push(last);
            }
            PathSegment::QuadTo(c, p) => {
                let end = point(p);
                let control = point(c);
                flatten_cubic(
                    last,
                    last.lerp(control, 2.0 / 3.0),
                    end.lerp(control, 2.0 / 3.0),
                    end,
                    &mut contour,
                );
                last = end;
            }
            PathSegment::CubicTo(a, b, p) => {
                let end = point(p);
                flatten_cubic(last, point(a), point(b), end, &mut contour);
                last = end;
            }
            PathSegment::Close => {
                if contour.len() >= 3 {
                    contours.push(std::mem::take(&mut contour));
                }
            }
        }
    }
    if contour.len() >= 3 {
        contours.push(contour);
    }
    (!contours.is_empty()).then_some(Geom::Poly {
        contours,
        winding: true,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::{Cap, Fill, Join, Stroke, Style};
    #[test]
    fn stroke_outline_obeys_caps_dashes_and_closed_holes() {
        let mut shape = Shape::new(
            Geom::Line {
                a: Pt::new(10.0, 10.0),
                b: Pt::new(110.0, 10.0),
            },
            Style {
                fill: Fill::None,
                stroke: Some(Stroke {
                    width: 10.0,
                    cap: Cap::Butt,
                    ..Default::default()
                }),
            },
        );
        assert!((crate::boolean::area(&expand(&shape).unwrap()) - 1000.0).abs() < 0.1);
        shape.style.stroke.as_mut().unwrap().cap = Cap::Square;
        assert!((crate::boolean::area(&expand(&shape).unwrap()) - 1100.0).abs() < 0.1);
        shape.style.stroke.as_mut().unwrap().cap = Cap::Round;
        let round = crate::boolean::area(&expand(&shape).unwrap());
        assert!(round > 1070.0 && round < 1085.0);
        shape.style.stroke.as_mut().unwrap().cap = Cap::Butt;
        shape.style.stroke.as_mut().unwrap().dash = Some((10.0, 10.0));
        assert!((crate::boolean::area(&expand(&shape).unwrap()) - 500.0).abs() < 0.1);
        shape.geom = Geom::Rect {
            origin: Pt::new(20.0, 20.0),
            size: Pt::splat(100.0),
            radius: 0.0,
        };
        shape.style.stroke.as_mut().unwrap().dash = None;
        shape.style.stroke.as_mut().unwrap().join = Join::Miter;
        let outline = expand(&shape).unwrap();
        assert!((crate::boolean::area(&outline) - 4000.0).abs() < 0.1);
        shape.style.stroke.as_mut().unwrap().join = Join::Bevel;
        assert!((crate::boolean::area(&expand(&shape).unwrap()) - 3950.0).abs() < 0.1);
        assert!(!outline.contains(Pt::new(70.0, 70.0)));
        assert!(outline.contains(Pt::new(20.0, 70.0)));
    }
}
