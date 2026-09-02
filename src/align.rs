//! Align and distribute selected shapes.

use crate::document::{Document, Shape};
use crate::geom::{Bounds, Pt};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Align {
    Left,
    CenterX,
    Right,
    Top,
    CenterY,
    Bottom,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Distribute {
    Horizontal,
    Vertical,
}

fn bbox_of(doc: &Document, ids: &[(usize, u64)]) -> Option<Bounds> {
    let mut b: Option<Bounds> = None;
    for (li, id) in ids {
        if let Some(s) = doc.find_shape(*li, *id) {
            let sb = s.world_bbox();
            b = Some(match b {
                None => sb,
                Some(bb) => bb.union(sb),
            });
        }
    }
    b
}

pub fn align_deltas(doc: &Document, ids: &[(usize, u64)], how: Align) -> Vec<(usize, u64, Pt)> {
    let Some(all) = bbox_of(doc, ids) else {
        return vec![];
    };
    let mut out = Vec::new();
    for (li, id) in ids {
        let Some(shape) = doc.find_shape(*li, *id) else {
            continue;
        };
        let b = shape.world_bbox();
        let delta = match how {
            Align::Left => Pt::new(all.min.x - b.min.x, 0.0),
            Align::CenterX => Pt::new(all.center().x - b.center().x, 0.0),
            Align::Right => Pt::new(all.max.x - b.max.x, 0.0),
            Align::Top => Pt::new(0.0, all.min.y - b.min.y),
            Align::CenterY => Pt::new(0.0, all.center().y - b.center().y),
            Align::Bottom => Pt::new(0.0, all.max.y - b.max.y),
        };
        if delta.length_sq() > 1e-8 {
            out.push((*li, *id, delta));
        }
    }
    out
}

pub fn align(doc: &mut Document, ids: &[(usize, u64)], how: Align) {
    for (li, id, d) in align_deltas(doc, ids, how) {
        if let Some(s) = doc.find_shape_mut(li, id) {
            s.geom.translate(d);
        }
    }
}

pub fn distribute_deltas(
    doc: &Document,
    ids: &[(usize, u64)],
    how: Distribute,
) -> Vec<(usize, u64, Pt)> {
    if ids.len() < 3 {
        return vec![];
    }
    let mut items: Vec<(usize, u64, Bounds)> = ids
        .iter()
        .filter_map(|(li, id)| {
            doc.find_shape(*li, *id)
                .map(|s| (*li, *id, s.world_bbox()))
        })
        .collect();
    match how {
        Distribute::Horizontal => {
            items.sort_by(|a, b| a.2.center().x.partial_cmp(&b.2.center().x).unwrap())
        }
        Distribute::Vertical => {
            items.sort_by(|a, b| a.2.center().y.partial_cmp(&b.2.center().y).unwrap())
        }
    }
    let first = items.first().unwrap().2;
    let last = items.last().unwrap().2;
    let n = items.len() as f32;
    let mut out = Vec::new();
    match how {
        Distribute::Horizontal => {
            let start = first.center().x;
            let end = last.center().x;
            let step = (end - start) / (n - 1.0);
            for (i, (li, id, b)) in items.iter().enumerate() {
                let target = start + step * i as f32;
                let d = Pt::new(target - b.center().x, 0.0);
                if d.length_sq() > 1e-8 {
                    out.push((*li, *id, d));
                }
            }
        }
        Distribute::Vertical => {
            let start = first.center().y;
            let end = last.center().y;
            let step = (end - start) / (n - 1.0);
            for (i, (li, id, b)) in items.iter().enumerate() {
                let target = start + step * i as f32;
                let d = Pt::new(0.0, target - b.center().y);
                if d.length_sq() > 1e-8 {
                    out.push((*li, *id, d));
                }
            }
        }
    }
    out
}

pub fn distribute(doc: &mut Document, ids: &[(usize, u64)], how: Distribute) {
    for (li, id, d) in distribute_deltas(doc, ids, how) {
        if let Some(s) = doc.find_shape_mut(li, id) {
            s.geom.translate(d);
        }
    }
}

pub fn selection_bounds(shapes: &[&Shape]) -> Option<Bounds> {
    let mut b: Option<Bounds> = None;
    for s in shapes {
        let sb = s.world_bbox();
        b = Some(match b {
            None => sb,
            Some(bb) => bb.union(sb),
        });
    }
    b
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::{Cmd, Document, Shape, Style, apply};
    use crate::geom::Geom;

    fn rect(x: f32, y: f32) -> Shape {
        Shape::new(
            Geom::Rect {
                origin: Pt::new(x, y),
                size: Pt::new(10.0, 10.0),
                radius: 0.0,
            },
            Style::default(),
        )
    }

    #[test]
    fn align_left() {
        let mut doc = Document::new("t", 200.0, 200.0, 72.0);
        let a = rect(40.0, 10.0);
        let b = rect(10.0, 40.0);
        let ia = a.id;
        let ib = b.id;
        apply(&mut doc, &Cmd::AddShape { layer: 1, shape: a });
        apply(&mut doc, &Cmd::AddShape { layer: 1, shape: b });
        align(&mut doc, &[(1, ia), (1, ib)], Align::Left);
        let ba = doc.find_shape(1, ia).unwrap().world_bbox();
        let bb = doc.find_shape(1, ib).unwrap().world_bbox();
        assert!((ba.min.x - bb.min.x).abs() < 0.1);
        assert!((ba.min.x - 10.0).abs() < 0.1);
    }
}
