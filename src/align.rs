//! Align and distribute selected shapes.

use crate::document::Document;
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
        .filter_map(|(li, id)| doc.find_shape(*li, *id).map(|s| (*li, *id, s.world_bbox())))
        .collect();
    if items.len() < 3 {
        return vec![];
    }
    let coordinate = |bounds: Bounds| match how {
        Distribute::Horizontal => bounds.center().x,
        Distribute::Vertical => bounds.center().y,
    };
    items.sort_by(|a, b| coordinate(a.2).total_cmp(&coordinate(b.2)));
    let start = coordinate(items[0].2);
    let end = coordinate(items[items.len() - 1].2);
    let step = (end - start) / (items.len() - 1) as f32;
    items
        .iter()
        .enumerate()
        .filter_map(|(i, (li, id, bounds))| {
            let offset = start + step * i as f32 - coordinate(*bounds);
            let delta = match how {
                Distribute::Horizontal => Pt::new(offset, 0.0),
                Distribute::Vertical => Pt::new(0.0, offset),
            };
            (delta.length_sq() > 1e-8).then_some((*li, *id, delta))
        })
        .collect()
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
        let deltas = align_deltas(&doc, &[(1, ia), (1, ib)], Align::Left);
        assert_eq!(deltas, vec![(1, ia, Pt::new(-30.0, 0.0))]);
    }

    #[test]
    fn distribute_ignores_missing_shapes_and_preserves_endpoints() {
        let mut doc = Document::new("t", 200.0, 200.0, 72.0);
        let shapes = [rect(0.0, 0.0), rect(20.0, 20.0), rect(100.0, 100.0)];
        let ids: Vec<_> = shapes.iter().map(|s| (1, s.id)).collect();
        for shape in shapes {
            apply(&mut doc, &Cmd::AddShape { layer: 1, shape });
        }
        assert_eq!(
            distribute_deltas(&doc, &ids, Distribute::Horizontal),
            vec![(1, ids[1].1, Pt::new(30.0, 0.0))]
        );
        assert_eq!(
            distribute_deltas(&doc, &ids, Distribute::Vertical),
            vec![(1, ids[1].1, Pt::new(0.0, 30.0))]
        );
        assert!(distribute_deltas(&doc, &[(1, u64::MAX); 3], Distribute::Horizontal).is_empty());
    }
}
