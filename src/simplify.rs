//! Fit a path down to the corners and curves that are actually there.

use crate::geom::{Anchor, Geom, PathContour, Pt};

const KAPPA: f32 = 0.55228475;
const CORNER_DOT: f32 = 0.86;

pub fn apply(geom: &mut Geom, epsilon: f32) {
    let epsilon = epsilon.max(0.05);
    match geom {
        Geom::Path { anchors, closed } => {
            *anchors = fit_contour(anchors, *closed, epsilon);
        }
        Geom::Paths { paths, .. } => {
            for path in paths {
                path.anchors = fit_contour(&path.anchors, path.closed, epsilon);
            }
        }
        Geom::Poly { contours, winding } => {
            let paths: Vec<PathContour> = contours
                .iter()
                .map(|contour| PathContour {
                    anchors: fit_contour_points(contour, true, epsilon),
                    closed: true,
                })
                .filter(|path| path.anchors.len() >= 2)
                .collect();
            if paths.len() == 1 {
                *geom = Geom::Path {
                    anchors: paths[0].anchors.clone(),
                    closed: true,
                };
            } else if !paths.is_empty() {
                *geom = Geom::Paths {
                    winding: *winding,
                    paths,
                };
            }
        }
        _ => {}
    }
}

fn fit_contour(anchors: &[Anchor], closed: bool, epsilon: f32) -> Vec<Anchor> {
    if anchors.len() < 3 {
        return anchors.to_vec();
    }
    let curved = anchors.iter().any(|anchor| {
        anchor.h_in.length() > 0.4 || anchor.h_out.length() > 0.4
    });
    if curved {
        let points = sample_anchors(anchors, closed);
        return fit_contour_points(&points, closed, epsilon);
    }
    let points: Vec<Pt> = anchors.iter().map(|anchor| anchor.pt).collect();
    fit_contour_points(&points, closed, epsilon)
}

fn fit_contour_points(points: &[Pt], closed: bool, epsilon: f32) -> Vec<Anchor> {
    let mut points = points.to_vec();
    if closed
        && points.len() > 1
        && (points[0] - *points.last().unwrap()).length() < 0.05
    {
        points.pop();
    }
    if points.len() < 2 {
        return points.into_iter().map(Anchor::corner).collect();
    }
    if let Some(circle) = full_circle(&points) {
        return circle;
    }
    let mut corners = corner_indices(&points, closed);
    if corners.is_empty() {
        corners.push(0);
    }
    if !closed {
        if corners.first() != Some(&0) {
            corners.insert(0, 0);
        }
        if corners.last() != Some(&(points.len() - 1)) {
            corners.push(points.len() - 1);
        }
    }
    let mut fitted = Vec::new();
    for pair in corners.windows(2) {
        append_span(
            &mut fitted,
            &span_points(&points, pair[0], pair[1], false),
            epsilon,
        );
    }
    if closed {
        let last = *corners.last().unwrap();
        let first = corners[0];
        append_span(
            &mut fitted,
            &span_points(&points, last, first, true),
            epsilon,
        );
        if fitted.len() >= 2 && (fitted[0].pt - fitted.last().unwrap().pt).length() < 0.05 {
            let end = fitted.pop().unwrap();
            fitted[0].h_in = end.h_in;
        }
    }
    if fitted.len() < 2 {
        points.into_iter().map(Anchor::corner).collect()
    } else {
        fitted
    }
}

fn append_span(out: &mut Vec<Anchor>, points: &[Pt], epsilon: f32) {
    if points.len() < 2 {
        return;
    }
    let mut span = fit_open(points, epsilon);
    if span.is_empty() {
        return;
    }
    if let Some(previous) = out.last_mut()
        && (previous.pt - span[0].pt).length() < 0.05
    {
        previous.h_out = span[0].h_out;
        span.remove(0);
    }
    out.extend(span);
}

fn span_points(points: &[Pt], start: usize, end: usize, wrap: bool) -> Vec<Pt> {
    if !wrap && end >= start {
        return points[start..=end].to_vec();
    }
    let mut span = points[start..].to_vec();
    span.extend_from_slice(&points[..=end]);
    span
}

fn fit_open(points: &[Pt], epsilon: f32) -> Vec<Anchor> {
    if points.len() < 2 {
        return points.iter().copied().map(Anchor::corner).collect();
    }
    if points.len() == 2 || chord_error(points) <= epsilon {
        return vec![
            Anchor::corner(points[0]),
            Anchor::corner(*points.last().unwrap()),
        ];
    }
    let params = chord_params(points);
    let start = tangent_out(points);
    let end = tangent_in(points);
    if let Some((alpha, beta)) = solve_handles(points, &params, start, end) {
        let p0 = points[0];
        let p3 = *points.last().unwrap();
        let c1 = p0 + start * alpha;
        let c2 = p3 + end * beta;
        let (error, split) = max_error(points, &params, p0, c1, c2, p3);
        if error <= epsilon {
            let mut first = Anchor::corner(p0);
            first.h_out = start * alpha;
            let mut last = Anchor::corner(p3);
            last.h_in = end * beta;
            return vec![first, last];
        }
        let split = split.clamp(1, points.len() - 2);
        let mut left = fit_open(&points[..=split], epsilon);
        let right = fit_open(&points[split..], epsilon);
        if let (Some(join), Some(next)) = (left.last_mut(), right.first()) {
            join.h_out = next.h_out;
        }
        if right.len() > 1 {
            left.extend_from_slice(&right[1..]);
        }
        return left;
    }
    vec![
        Anchor::corner(points[0]),
        Anchor::corner(*points.last().unwrap()),
    ]
}

fn full_circle(points: &[Pt]) -> Option<Vec<Anchor>> {
    if points.len() < 12 {
        return None;
    }
    let center = points.iter().copied().fold(Pt::ZERO, |acc, point| acc + point)
        / points.len() as f32;
    let radii: Vec<f32> = points.iter().map(|point| (*point - center).length()).collect();
    let mean = radii.iter().sum::<f32>() / radii.len() as f32;
    if mean < 1.0 || radii.iter().any(|radius| (*radius - mean).abs() > mean * 0.035) {
        return None;
    }
    let mut angles: Vec<f32> = points
        .iter()
        .map(|point| (*point - center).y.atan2((*point - center).x))
        .collect();
    angles.sort_by(|a, b| a.total_cmp(b));
    let mut gap = 0.0f32;
    for pair in angles.windows(2) {
        gap = gap.max(pair[1] - pair[0]);
    }
    gap = gap.max((angles[0] + std::f32::consts::TAU) - *angles.last().unwrap());
    if gap > 0.55 {
        return None;
    }
    let area = signed_area(points);
    Some(circle_anchors(center, mean, area > 0.0))
}

fn circle_anchors(center: Pt, radius: f32, positive: bool) -> Vec<Anchor> {
    let handle = KAPPA * radius;
    let sign = if positive { 1.0 } else { -1.0 };
    (0..4)
        .map(|index| {
            let angle = index as f32 * std::f32::consts::FRAC_PI_2 * sign;
            let point = center + Pt::new(angle.cos(), angle.sin()) * radius;
            let tangent = Pt::new(-angle.sin(), angle.cos()) * sign * handle;
            Anchor {
                pt: point,
                h_in: -tangent,
                h_out: tangent,
                radius: 0.0,
            }
        })
        .collect()
}

fn corner_indices(points: &[Pt], closed: bool) -> Vec<usize> {
    let mut corners = Vec::new();
    let count = points.len();
    for index in 0..count {
        if !closed && (index == 0 || index + 1 == count) {
            continue;
        }
        let previous = points[(index + count - 1) % count];
        let next = points[(index + 1) % count];
        let incoming = (points[index] - previous).normalized();
        let outgoing = (next - points[index]).normalized();
        if incoming == Pt::ZERO || outgoing == Pt::ZERO {
            continue;
        }
        if incoming.dot(outgoing) < CORNER_DOT {
            corners.push(index);
        }
    }
    corners
}

fn chord_error(points: &[Pt]) -> f32 {
    let start = points[0];
    let end = *points.last().unwrap();
    points
        .iter()
        .map(|point| point_segment_distance(*point, start, end))
        .fold(0.0, f32::max)
}

fn chord_params(points: &[Pt]) -> Vec<f32> {
    let mut distances = vec![0.0];
    for pair in points.windows(2) {
        distances.push(distances.last().copied().unwrap_or(0.0) + (pair[1] - pair[0]).length());
    }
    let total = distances.last().copied().unwrap_or(0.0);
    if total < 1e-6 {
        return distances;
    }
    distances.into_iter().map(|distance| distance / total).collect()
}

fn tangent_out(points: &[Pt]) -> Pt {
    (points[1] - points[0]).normalized()
}

fn tangent_in(points: &[Pt]) -> Pt {
    let count = points.len();
    (points[count - 2] - points[count - 1]).normalized()
}

fn solve_handles(points: &[Pt], params: &[f32], start: Pt, end: Pt) -> Option<(f32, f32)> {
    if start == Pt::ZERO || end == Pt::ZERO {
        return None;
    }
    let p0 = points[0];
    let p3 = *points.last().unwrap();
    let mut c00 = 0.0;
    let mut c01 = 0.0;
    let mut c11 = 0.0;
    let mut x0 = 0.0;
    let mut x1 = 0.0;
    for (point, &t) in points.iter().zip(params) {
        let u = 1.0 - t;
        let b1 = 3.0 * u * u * t;
        let b2 = 3.0 * u * t * t;
        let a1 = start * b1;
        let a2 = end * b2;
        let tmp = *point - (p0 * (u * u * u + b1) + p3 * (b2 + t * t * t));
        c00 += a1.dot(a1);
        c01 += a1.dot(a2);
        c11 += a2.dot(a2);
        x0 += a1.dot(tmp);
        x1 += a2.dot(tmp);
    }
    let det = c00 * c11 - c01 * c01;
    let chord = (p3 - p0).length() / 3.0;
    let (alpha, beta) = if det.abs() < 1e-6 {
        (chord, chord)
    } else {
        ((x0 * c11 - x1 * c01) / det, (c00 * x1 - c01 * x0) / det)
    };
    if !alpha.is_finite() || !beta.is_finite() || alpha < 0.0 || beta < 0.0 {
        return Some((chord.max(0.0), chord.max(0.0)));
    }
    Some((alpha, beta))
}

fn max_error(points: &[Pt], params: &[f32], p0: Pt, c1: Pt, c2: Pt, p3: Pt) -> (f32, usize) {
    let mut worst = 0.0;
    let mut index = points.len() / 2;
    for (i, (point, &t)) in points.iter().zip(params).enumerate() {
        let error = (*point - cubic(p0, c1, c2, p3, t)).length();
        if error > worst {
            worst = error;
            index = i;
        }
    }
    (worst, index)
}

fn cubic(p0: Pt, c1: Pt, c2: Pt, p3: Pt, t: f32) -> Pt {
    let u = 1.0 - t;
    p0 * (u * u * u) + c1 * (3.0 * u * u * t) + c2 * (3.0 * u * t * t) + p3 * (t * t * t)
}

fn sample_anchors(anchors: &[Anchor], closed: bool) -> Vec<Pt> {
    let mut points = Vec::new();
    let count = anchors.len();
    let segments = if closed { count } else { count.saturating_sub(1) };
    for index in 0..segments {
        let current = anchors[index];
        let next = anchors[(index + 1) % count];
        if points.is_empty() {
            points.push(current.pt);
        }
        for step in 1..=8 {
            let t = step as f32 / 8.0;
            points.push(cubic(
                current.pt,
                current.pt + current.h_out,
                next.pt + next.h_in,
                next.pt,
                t,
            ));
        }
    }
    points
}

fn point_segment_distance(point: Pt, start: Pt, end: Pt) -> f32 {
    let span = end - start;
    let len_sq = span.length_sq();
    if len_sq < 1e-8 {
        return (point - start).length();
    }
    let t = ((point - start).dot(span) / len_sq).clamp(0.0, 1.0);
    (point - (start + span * t)).length()
}

fn signed_area(points: &[Pt]) -> f32 {
    let mut area = 0.0;
    for pair in points.windows(2) {
        area += pair[0].cross(pair[1]);
    }
    if let (Some(first), Some(last)) = (points.first(), points.last()) {
        area += last.cross(*first);
    }
    area * 0.5
}

#[cfg(test)]
mod tests {
    use super::*;

    fn circle_points(radius: f32, count: usize) -> Vec<Pt> {
        (0..count)
            .map(|index| {
                let angle = index as f32 / count as f32 * std::f32::consts::TAU;
                Pt::new(radius * angle.cos(), radius * angle.sin())
            })
            .collect()
    }

    #[test]
    fn a_many_node_circle_becomes_four_kappa_curves() {
        let radius = 100.0;
        let mut geom = Geom::Path {
            anchors: circle_points(radius, 64)
                .into_iter()
                .map(Anchor::corner)
                .collect(),
            closed: true,
        };
        apply(&mut geom, 1.25);
        let Geom::Path { anchors, .. } = geom else {
            panic!("circle stays one path");
        };
        assert_eq!(anchors.len(), 4);
        for anchor in &anchors {
            let length = anchor.h_out.length().max(anchor.h_in.length());
            assert!(
                (length - KAPPA * radius).abs() < radius * 0.02,
                "handle {length} vs {}",
                KAPPA * radius
            );
        }
    }

    #[test]
    fn a_crescent_keeps_two_cusps() {
        let left = Geom::Ellipse {
            center: Pt::ZERO,
            radii: Pt::splat(100.0),
        };
        let right = Geom::Ellipse {
            center: Pt::new(70.0, 0.0),
            radii: Pt::splat(80.0),
        };
        let mut crescent = crate::boolean::apply(crate::boolean::BoolOp::Subtract, &left, &right)
            .expect("circles overlap");
        let before = match &crescent {
            Geom::Poly { contours, .. } => contours[0].len(),
            _ => panic!("boolean returns a polygon"),
        };
        assert!(before > 12);
        apply(&mut crescent, 1.5);
        let anchors = match crescent {
            Geom::Path { anchors, .. } => anchors,
            Geom::Paths { paths, .. } => paths.into_iter().flat_map(|path| path.anchors).collect(),
            other => panic!("fitted crescent is a path, got {other:?}"),
        };
        assert!(anchors.len() < before);
        assert!(anchors.len() <= 10, "too many nodes: {}", anchors.len());
        let cusps = anchors
            .iter()
            .filter(|anchor| {
                let incoming = anchor.h_in.length();
                let outgoing = anchor.h_out.length();
                if incoming < 0.8 || outgoing < 0.8 {
                    return true;
                }
                anchor.h_in.dot(anchor.h_out) / (incoming * outgoing) > -0.65
            })
            .count();
        assert_eq!(cusps, 2, "anchors {anchors:?}");
    }
}
