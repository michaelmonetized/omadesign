//! Boolean path operations. Flattened contours in, a polygon out.

use crate::geom::{Geom, Pt};
use geo::{Area, BooleanOps};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BoolOp {
    Union,
    Subtract,
    Intersect,
    Xor,
}

impl BoolOp {
    pub fn name(self) -> &'static str {
        match self {
            BoolOp::Union => "Union",
            BoolOp::Subtract => "Subtract",
            BoolOp::Intersect => "Intersect",
            BoolOp::Xor => "XOR",
        }
    }

    pub fn all() -> [BoolOp; 4] {
        [
            BoolOp::Union,
            BoolOp::Subtract,
            BoolOp::Intersect,
            BoolOp::Xor,
        ]
    }
}

fn to_geo(geom: &Geom) -> geo::MultiPolygon<f64> {
    let polygons = geom
        .contours(96)
        .into_iter()
        .filter(|c| c.len() >= 3)
        .map(|c| {
            let ring = c
                .into_iter()
                .map(|p| (p.x as f64, p.y as f64))
                .collect::<Vec<_>>();
            geo::Polygon::new(geo::LineString::from(ring), vec![])
        })
        .collect();
    let raw = geo::MultiPolygon(polygons);
    // Normalize using the artwork's fill rule. Geo counts all ring crossings and
    // returns properly nested exterior/interior rings, including winding imports.
    let rule = if matches!(geom, Geom::Poly { winding: true, .. }) {
        geo::algorithm::bool_ops::FillRule::NonZero
    } else {
        geo::algorithm::bool_ops::FillRule::EvenOdd
    };
    raw.union_with_fill_rule(&geo::MultiPolygon(vec![]), rule)
}

fn from_geo(mp: &geo::MultiPolygon<f64>) -> Vec<Vec<Pt>> {
    mp.0.iter()
        .flat_map(|poly| {
            let mut out = vec![ring_pts(poly.exterior())];
            for hole in poly.interiors() {
                out.push(ring_pts(hole));
            }
            out
        })
        .filter(|c| c.len() >= 3)
        .collect()
}

fn ring_pts(ls: &geo::LineString<f64>) -> Vec<Pt> {
    let mut pts: Vec<Pt> = ls
        .points()
        .map(|p| Pt::new(p.x() as f32, p.y() as f32))
        .collect();
    if pts.len() > 1 {
        let first = pts[0];
        let last = *pts.last().unwrap();
        if (first - last).length() < 1e-3 {
            pts.pop();
        }
    }
    pts
}

pub fn apply(op: BoolOp, a: &Geom, b: &Geom) -> Option<Geom> {
    apply_many(op, &[a.clone(), b.clone()])
}

pub fn apply_many(op: BoolOp, geoms: &[Geom]) -> Option<Geom> {
    let mut inputs = geoms.iter().map(to_geo);
    let mut result = inputs.next()?;
    for next in inputs {
        result = match op {
            BoolOp::Union => result.union(&next),
            BoolOp::Subtract => result.difference(&next),
            BoolOp::Intersect => result.intersection(&next),
            BoolOp::Xor => result.xor(&next),
        };
    }
    let contours = from_geo(&result);
    (!contours.is_empty()).then_some(Geom::Poly {
        contours,
        winding: false,
    })
}

/// Split the covered area into independently selectable, non-overlapping faces.
/// Each face retains its holes instead of turning inner rings into filled pieces.
pub fn divide(geoms: &[Geom]) -> Vec<Geom> {
    let mut pieces: Vec<geo::Polygon<f64>> = vec![];
    for shape in geoms.iter().map(to_geo) {
        let mut uncovered = shape.clone();
        let mut next = vec![];
        for piece in pieces {
            next.extend(piece.difference(&shape).0);
            next.extend(piece.intersection(&shape).0);
            uncovered = uncovered.difference(&piece);
        }
        next.extend(uncovered.0);
        pieces = next;
    }
    pieces
        .into_iter()
        .map(|piece| Geom::Poly {
            contours: from_geo(&geo::MultiPolygon(vec![piece])),
            winding: false,
        })
        .collect()
}

pub fn area(g: &Geom) -> f32 {
    to_geo(g).unsigned_area() as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square(x: f32, y: f32, s: f32) -> Geom {
        Geom::Rect {
            origin: Pt::new(x, y),
            size: Pt::splat(s),
            radius: 0.0,
        }
    }

    #[test]
    fn union_of_overlapping_squares() {
        let u = apply(
            BoolOp::Union,
            &square(0.0, 0.0, 10.0),
            &square(5.0, 0.0, 10.0),
        )
        .unwrap();
        assert!((area(&u) - 150.0).abs() < 1.0, "area {}", area(&u));
    }

    #[test]
    fn subtract_leaves_remainder() {
        let d = apply(
            BoolOp::Subtract,
            &square(0.0, 0.0, 10.0),
            &square(5.0, 0.0, 5.0),
        )
        .unwrap();
        assert!((area(&d) - 75.0).abs() < 1.0, "area {}", area(&d));
    }

    #[test]
    fn intersect_gives_overlap() {
        let i = apply(
            BoolOp::Intersect,
            &square(0.0, 0.0, 10.0),
            &square(5.0, 0.0, 10.0),
        )
        .unwrap();
        assert!((area(&i) - 50.0).abs() < 1.0);
    }

    #[test]
    fn xor_excludes_overlap() {
        let x = apply(
            BoolOp::Xor,
            &square(0.0, 0.0, 10.0),
            &square(5.0, 0.0, 10.0),
        )
        .unwrap();
        assert!((area(&x) - 100.0).abs() < 1.0);
    }

    #[test]
    fn disjoint_intersect_is_none() {
        assert!(
            apply(
                BoolOp::Intersect,
                &square(0.0, 0.0, 10.0),
                &square(100.0, 100.0, 10.0)
            )
            .is_none()
        );
    }
}

#[cfg(test)]
mod topology_tests {
    use super::*;
    fn rect(x: f32, y: f32, w: f32, h: f32) -> Geom {
        Geom::Rect {
            origin: Pt::new(x, y),
            size: Pt::new(w, h),
            radius: 0.0,
        }
    }
    #[test]
    fn donut_holes_survive_repeated_operations_and_winding_is_respected() {
        let outer = rect(0.0, 0.0, 20.0, 20.0);
        let inner = rect(5.0, 5.0, 10.0, 10.0);
        let donut = apply(BoolOp::Subtract, &outer, &inner).unwrap();
        assert!((area(&donut) - 300.0).abs() < 0.01);
        let clipped = apply(BoolOp::Intersect, &donut, &outer).unwrap();
        assert!((area(&clipped) - 300.0).abs() < 0.01);
        assert!(apply(BoolOp::Intersect, &donut, &inner).is_none());
        let mut contours = outer.contours(4);
        contours.extend(inner.contours(4));
        let same_direction = Geom::Poly {
            contours: contours.clone(),
            winding: true,
        };
        assert!((area(&same_direction) - 400.0).abs() < 0.01);
        contours[1].reverse();
        assert!(
            (area(&Geom::Poly {
                contours,
                winding: true
            }) - 300.0)
                .abs()
                < 0.01
        );
    }
    #[test]
    fn divide_partitions_three_overlaps_without_filling_holes_or_double_counting() {
        let shapes = vec![
            rect(0.0, 0.0, 20.0, 20.0),
            rect(10.0, 0.0, 20.0, 20.0),
            rect(15.0, 5.0, 10.0, 10.0),
        ];
        let pieces = divide(&shapes);
        assert!(pieces.len() >= 5);
        assert!((pieces.iter().map(area).sum::<f32>() - 600.0).abs() < 0.01);
        for (i, a) in pieces.iter().enumerate() {
            for b in &pieces[i + 1..] {
                assert!(apply(BoolOp::Intersect, a, b).is_none());
            }
        }
        let donut = apply(BoolOp::Subtract, &shapes[0], &rect(5.0, 5.0, 10.0, 10.0)).unwrap();
        let pieces = divide(&[donut, rect(30.0, 0.0, 10.0, 10.0)]);
        assert_eq!(pieces.len(), 2);
        assert!((pieces.iter().map(area).sum::<f32>() - 400.0).abs() < 0.01);
        assert!(
            pieces
                .iter()
                .all(|p| apply(BoolOp::Intersect, p, &rect(6.0, 6.0, 8.0, 8.0)).is_none())
        );
    }
    #[test]
    fn empty_intermediate_xor_and_subtraction_are_not_discarded() {
        let a = rect(0.0, 0.0, 10.0, 10.0);
        let b = rect(20.0, 0.0, 5.0, 5.0);
        let xor = apply_many(BoolOp::Xor, &[a.clone(), a.clone(), b.clone()]).unwrap();
        assert!((area(&xor) - 25.0).abs() < 0.01);
        assert!(apply_many(BoolOp::Subtract, &[a.clone(), a, b]).is_none());
    }
}
