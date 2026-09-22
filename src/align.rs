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

type Item = (Vec<(usize, u64)>, Bounds);

fn items(doc: &Document, ids: &[(usize, u64)], individual: Option<(usize, u64)>) -> Vec<Item> {
    let mut items: Vec<(Option<usize>, Item)> = vec![];
    let mut seen = std::collections::HashSet::new();
    for &(li, id) in ids {
        if !seen.insert((li, id)) {
            continue;
        }
        // A selected layout frame carries its descendants; avoid aligning those twice.
        if ids.iter().any(|&(layer, parent)| {
            layer == li && parent != id && crate::layout::descendants(doc, li, parent).contains(&id)
        }) {
            continue;
        }
        let bounds = if id == crate::document::RASTER_ID {
            doc.layers.get(li).and_then(|l| l.kind.raster_bounds())
        } else {
            doc.find_shape(li, id).map(|s| s.world_bbox())
        };
        let Some(bounds) = bounds else {
            continue;
        };
        let group = (individual != Some((li, id)))
            .then(|| doc.layer_ancestors(li).last().copied())
            .flatten();
        if let Some((_, (members, all))) = items
            .iter_mut()
            .find(|(key, _)| group.is_some() && *key == group)
        {
            members.push((li, id));
            *all = all.union(bounds);
        } else {
            items.push((group, (vec![(li, id)], bounds)));
        }
    }
    items.into_iter().map(|(_, v)| v).collect()
}

pub fn align_deltas(doc: &Document, ids: &[(usize, u64)], how: Align) -> Vec<(usize, u64, Pt)> {
    align_items(doc, ids, how, None)
}

pub fn align_items(
    doc: &Document,
    ids: &[(usize, u64)],
    how: Align,
    individual: Option<(usize, u64)>,
) -> Vec<(usize, u64, Pt)> {
    let items = items(doc, ids, individual);
    let Some(mut all) = items.iter().map(|(_, b)| *b).reduce(|a, b| a.union(b)) else {
        return vec![];
    };
    if items.len() == 1 {
        all = doc
            .artboards
            .iter()
            .find(|a| a.bounds().contains(all.center()))
            .map(|a| a.bounds())
            .unwrap_or(Bounds::from_min_size(
                Pt::ZERO,
                Pt::new(doc.width, doc.height),
            ));
    }
    let mut result = vec![];
    for (members, b) in items {
        let delta = match how {
            Align::Left => Pt::new(all.min.x - b.min.x, 0.),
            Align::CenterX => Pt::new(all.center().x - b.center().x, 0.),
            Align::Right => Pt::new(all.max.x - b.max.x, 0.),
            Align::Top => Pt::new(0., all.min.y - b.min.y),
            Align::CenterY => Pt::new(0., all.center().y - b.center().y),
            Align::Bottom => Pt::new(0., all.max.y - b.max.y),
        };
        if delta.length_sq() > 1e-8 {
            result.extend(members.into_iter().map(|(li, id)| (li, id, delta)));
        }
    }
    result
}

pub fn distribute_deltas(
    doc: &Document,
    ids: &[(usize, u64)],
    how: Distribute,
) -> Vec<(usize, u64, Pt)> {
    distribute_items(doc, ids, how, None)
}
pub fn distribute_items(
    doc: &Document,
    ids: &[(usize, u64)],
    how: Distribute,
    individual: Option<(usize, u64)>,
) -> Vec<(usize, u64, Pt)> {
    let mut items = items(doc, ids, individual);
    if items.len() < 3 {
        return vec![];
    }
    let coordinate = |b: Bounds| match how {
        Distribute::Horizontal => b.center().x,
        Distribute::Vertical => b.center().y,
    };
    items.sort_by(|a, b| coordinate(a.1).total_cmp(&coordinate(b.1)));
    let start = coordinate(items[0].1);
    let end = coordinate(items[items.len() - 1].1);
    let step = (end - start) / (items.len() - 1) as f32;
    items
        .into_iter()
        .enumerate()
        .flat_map(|(i, (members, b))| {
            let offset = start + step * i as f32 - coordinate(b);
            let delta = match how {
                Distribute::Horizontal => Pt::new(offset, 0.),
                Distribute::Vertical => Pt::new(0., offset),
            };
            members
                .into_iter()
                .filter_map(move |(li, id)| (delta.length_sq() > 1e-8).then_some((li, id, delta)))
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
