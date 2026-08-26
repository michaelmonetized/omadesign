use crate::document::Geometry;
use eframe::egui::Pos2;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BoolOp {
    Union,
    Subtract,
    Intersect,
    Xor,
}

impl BoolOp {
    pub fn name(&self) -> &'static str {
        match self {
            BoolOp::Union => "Union",
            BoolOp::Subtract => "Subtract",
            BoolOp::Intersect => "Intersect",
            BoolOp::Xor => "XOR",
        }
    }
}

fn to_geo(contours: &[Vec<Pos2>]) -> geo::MultiPolygon<f64> {
    let polygons = contours
        .iter()
        .filter(|c| c.len() >= 3)
        .map(|c| {
            let ring: Vec<geo::Coord<f64>> = c.iter().map(|p| (p.x as f64, p.y as f64).into()).collect();
            geo::Polygon::new(geo::LineString::from(ring), vec![])
        })
        .collect();
    geo::MultiPolygon(polygons)
}

fn from_geo(mp: &geo::MultiPolygon<f64>) -> Vec<Vec<Pos2>> {
    mp.0
        .iter()
        .map(|poly| {
            let mut pts: Vec<Pos2> = poly
                .exterior()
                .points()
                .map(|p| eframe::egui::pos2(p.x() as f32, p.y() as f32))
                .collect();
            if pts.len() > 1 {
                let first = pts[0];
                let last = *pts.last().unwrap();
                if (first - last).length() < 1e-4 {
                    pts.pop();
                }
            }
            pts
        })
        .filter(|c| c.len() >= 3)
        .collect()
}

pub fn apply(op: BoolOp, a: &Geometry, b: &Geometry) -> Option<Geometry> {
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
    Some(Geometry::MultiPolygon { contours })
}

#[cfg(test)]
mod tests {
    use super::*;
    use eframe::egui::pos2;
    use eframe::egui::Vec2;

    fn square(x: f32, y: f32, s: f32) -> Geometry {
        Geometry::Rect {
            origin: pos2(x, y),
            size: Vec2::splat(s),
        }
    }

    fn area(g: &Geometry) -> f32 {
        let mut total = 0.0;
        for c in g.contours(96) {
            let n = c.len();
            for i in 0..n {
                let p = c[i];
                let q = c[(i + 1) % n];
                total += p.x * q.y - q.x * p.y;
            }
        }
        (total * 0.5).abs()
    }

    #[test]
    fn union_of_overlapping_squares_has_combined_area() {
        let a = square(0.0, 0.0, 10.0);
        let b = square(5.0, 0.0, 10.0);
        let u = apply(BoolOp::Union, &a, &b).unwrap();
        assert!((area(&u) - 150.0).abs() < 0.5, "area {}", area(&u));
    }

    #[test]
    fn subtract_leaves_l_shape() {
        let a = square(0.0, 0.0, 10.0);
        let b = square(5.0, 0.0, 5.0);
        let d = apply(BoolOp::Subtract, &a, &b).unwrap();
        assert!((area(&d) - 75.0).abs() < 0.5, "area {}", area(&d));
    }

    #[test]
    fn intersect_gives_overlap() {
        let a = square(0.0, 0.0, 10.0);
        let b = square(5.0, 0.0, 10.0);
        let i = apply(BoolOp::Intersect, &a, &b).unwrap();
        assert!((area(&i) - 50.0).abs() < 0.5, "area {}", area(&i));
    }

    #[test]
    fn xor_excludes_overlap() {
        let a = square(0.0, 0.0, 10.0);
        let b = square(5.0, 0.0, 10.0);
        let x = apply(BoolOp::Xor, &a, &b).unwrap();
        assert!((area(&x) - 100.0).abs() < 0.5, "area {}", area(&x));
    }

    #[test]
    fn disjoint_intersect_is_none() {
        let a = square(0.0, 0.0, 10.0);
        let b = square(100.0, 100.0, 10.0);
        assert!(apply(BoolOp::Intersect, &a, &b).is_none());
    }
}
