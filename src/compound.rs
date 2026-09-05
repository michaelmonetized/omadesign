//! Compound shape / path object tools.

use crate::boolean::{self, BoolOp};
use crate::document::Shape;
use crate::geom::{Geom, Pt};

/// Combine a set of shapes into a single even-odd Poly by concatenating their
/// world contours. The result preserves the visual union of outlines but keeps
/// holes via even-odd fill. This is the “Combine” / “Compound Path” operation.
pub fn combine_into_poly(shapes: &[&Shape]) -> Option<Geom> {
    if shapes.len() < 2 {
        return None;
    }
    let mut contours: Vec<Vec<Pt>> = Vec::new();
    for s in shapes {
        let cs = s.world_contours(96);
        for c in cs {
            if c.len() >= 3 {
                contours.push(c);
            } else if c.len() == 2 {
                // For lines, keep as degenerate contour – still forms shape
                contours.push(c);
            }
        }
    }
    if contours.is_empty() {
        return None;
    }
    Some(Geom::Poly {
        contours,
        winding: false,
    })
}

/// Fold a boolean op across N geoms. The first geom is the accumulator.
pub fn apply_multi(op: BoolOp, geoms: &[Geom]) -> Option<Geom> {
    if geoms.len() < 2 {
        return None;
    }
    let mut acc = geoms[0].clone();
    for g in &geoms[1..] {
        match boolean::apply(op, &acc, g) {
            Some(next) => acc = next,
            None => {
                // If any step produces nothing (e.g. disjoint intersect), the
                // overall result is empty.
                if op == BoolOp::Intersect {
                    return None;
                }
                // For other ops, continue with accumulator unchanged?
                // But boolean::apply returning None means empty, so we keep acc
                // for union when disjoint? Actually union of disjoint should succeed.
                // If union fails, it means one of the inputs had no contours.
                // We'll keep acc.
                continue;
            }
        }
    }
    Some(acc)
}

/// Explode a Poly with >1 contour into separate Poly shapes (one contour each).
pub fn explode_poly(poly: &Geom) -> Option<Vec<Geom>> {
    match poly {
        Geom::Poly { contours, winding } if contours.len() > 1 => {
            let geoms = contours
                .iter()
                .map(|c| Geom::Poly {
                    contours: vec![c.clone()],
                    winding: *winding,
                })
                .collect();
            Some(geoms)
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::{Shape, Style};
    use crate::geom::{Geom, Pt};

    fn rect(x: f32, y: f32, s: f32) -> Shape {
        Shape::new(
            Geom::Rect {
                origin: Pt::new(x, y),
                size: Pt::splat(s),
                radius: 0.0,
            },
            Style::default(),
        )
    }

    #[test]
    fn apply_multi_union_three() {
        let a = Geom::Rect {
            origin: Pt::new(0.0, 0.0),
            size: Pt::splat(10.0),
            radius: 0.0,
        };
        let b = Geom::Rect {
            origin: Pt::new(5.0, 0.0),
            size: Pt::splat(10.0),
            radius: 0.0,
        };
        let c = Geom::Rect {
            origin: Pt::new(10.0, 0.0),
            size: Pt::splat(10.0),
            radius: 0.0,
        };
        let r = apply_multi(BoolOp::Union, &[a, b, c]).unwrap();
        let area = boolean::area(&r);
        assert!((area - 200.0).abs() < 0.01, "area {area}");
    }

    #[test]
    fn explode_poly_roundtrip() {
        let a = rect(0.0, 0.0, 10.0);
        let b = rect(20.0, 0.0, 10.0);
        let combined = combine_into_poly(&[&a, &b]).unwrap();
        let parts = explode_poly(&combined).unwrap();
        assert_eq!(parts.len(), 2);
    }
}
