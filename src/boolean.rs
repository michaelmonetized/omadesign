//! Boolean path operations. Flattened contours in, a polygon out.

use crate::geom::{Geom, Pt, poly_area};

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

fn to_geo(contours: &[Vec<Pt>]) -> geo::MultiPolygon<f64> {
    let polygons = contours
        .iter()
        .filter(|c| c.len() >= 3)
        .map(|c| {
            let mut ring: Vec<geo::Coord<f64>> =
                c.iter().map(|p| (p.x as f64, p.y as f64).into()).collect();
            if let (Some(first), Some(last)) = (ring.first(), ring.last())
                && (first.x - last.x).abs() + (first.y - last.y).abs() > 1e-6
            {
                ring.push(*first);
            }
            geo::Polygon::new(geo::LineString::from(ring), vec![])
        })
        .collect();
    geo::MultiPolygon(polygons)
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
    let ga = to_geo(&a.contours(96));
    let gb = to_geo(&b.contours(96));
    if ga.0.is_empty() || gb.0.is_empty() {
        return None;
    }
    use geo::BooleanOps;
    let result = match op {
        BoolOp::Union => ga.union(&gb),
        BoolOp::Subtract => ga.difference(&gb),
        BoolOp::Intersect => ga.intersection(&gb),
        BoolOp::Xor => ga.xor(&gb),
    };
    let contours = from_geo(&result);
    if contours.is_empty() {
        return None;
    }
    Some(Geom::Poly { contours })
}

pub fn area(g: &Geom) -> f32 {
    g.contours(96).iter().map(|c| poly_area(c).abs()).sum()
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
        let u = apply(BoolOp::Union, &square(0.0, 0.0, 10.0), &square(5.0, 0.0, 10.0)).unwrap();
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
        let x = apply(BoolOp::Xor, &square(0.0, 0.0, 10.0), &square(5.0, 0.0, 10.0)).unwrap();
        assert!((area(&x) - 100.0).abs() < 1.0);
    }

    #[test]
    fn disjoint_intersect_is_none() {
        assert!(apply(
            BoolOp::Intersect,
            &square(0.0, 0.0, 10.0),
            &square(100.0, 100.0, 10.0)
        )
        .is_none());
    }
}
