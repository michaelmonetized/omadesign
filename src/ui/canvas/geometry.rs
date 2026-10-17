use super::*;
use tiny_skia::Transform;

/// Map a point through a painted placement.
/// Args: matrix and point. Returns mapped document coordinates.
pub(super) fn map_point(transform: Transform, point: Pt) -> Pt {
    let mut point = tiny_skia::Point::from_xy(point.x, point.y);
    transform.map_point(&mut point);
    Pt::new(point.x, point.y)
}

/// Resolve an object placement in the current editor.
/// Args: editor, layer and shape. Returns placement through ancestor and own poses and rotations.
pub(super) fn shape_transform(
    studio: &Studio,
    layer: usize,
    shape: &crate::document::Shape,
) -> Transform {
    shape_transform_with(&studio.doc, layer, shape, |id| studio.live_pose(id))
}

/// Resolve an object placement from a document snapshot.
/// Args: document, layer, shape and pose sampler. Returns the painter-compatible object transform.
pub(super) fn shape_transform_with(
    doc: &crate::document::Document,
    layer: usize,
    shape: &crate::document::Shape,
    pose: impl Fn(u64) -> crate::motion::Pose,
) -> Transform {
    let parent = compositor::shape_parent_transform_with(doc, layer, shape.id, &pose);
    compositor::frame_transform(shape, parent, pose(shape.id))
}

/// Place editable nodes and their relative handles.
/// Args: anchors and affine placement. Returns mapped anchors without changing their source geometry.
pub(super) fn transform_anchors(anchors: &[Anchor], transform: Transform) -> Vec<Anchor> {
    anchors
        .iter()
        .map(|anchor| {
            let mut mapped = *anchor;
            mapped.pt = map_point(transform, anchor.pt);
            mapped.h_in = map_point(transform, anchor.pt + anchor.h_in) - mapped.pt;
            mapped.h_out = map_point(transform, anchor.pt + anchor.h_out) - mapped.pt;
            mapped
        })
        .collect()
}

/// Locate editable nodes on the current painted object.
/// Args: editor, layer and shape. Returns mapped path anchors, or none for other geometry.
pub(super) fn posed_anchors(
    studio: &Studio,
    layer: usize,
    shape: &crate::document::Shape,
) -> Option<Vec<Anchor>> {
    if !matches!(shape.geom, Geom::Path { .. } | Geom::Paths { .. }) {
        return None;
    }
    let anchors: Vec<_> = shape.geom.anchors().copied().collect();
    Some(transform_anchors(
        &anchors,
        shape_transform(studio, layer, shape),
    ))
}

/// Recover editable coordinates from a document point.
/// Args: editor, layer, shape and document point. Returns local coordinates, or none for a singular placement.
pub(super) fn local_point(
    studio: &Studio,
    layer: usize,
    shape: &crate::document::Shape,
    point: Pt,
) -> Option<Pt> {
    Some(map_point(
        shape_transform(studio, layer, shape).invert()?,
        point,
    ))
}

/// Pick an actual painted node or handle.
/// Args: editor, layer, shape, document point and hit distance. Returns an unclipped editable path target.
pub(super) fn hit_shape_node(
    studio: &Studio,
    layer: usize,
    shape: &crate::document::Shape,
    point: Pt,
    slack: f32,
) -> Option<NodeHit> {
    studio
        .doc
        .layout_hit_point_with(layer, shape.id, point, |id| studio.live_pose(id))?;
    let transform = shape_transform(studio, layer, shape);
    let mut offset = 0;
    let mut segment = None;
    for (anchors, closed) in shape.geom.path_contours() {
        let mapped = transform_anchors(anchors, transform);
        if let Some(hit) = hit_node(&mapped, point, slack, closed) {
            match hit {
                NodeHit::Point(i) => return Some(NodeHit::Point(offset + i)),
                NodeHit::HandleIn(i) => return Some(NodeHit::HandleIn(offset + i)),
                NodeHit::HandleOut(i) => return Some(NodeHit::HandleOut(offset + i)),
                NodeHit::Segment(i) => segment = Some(NodeHit::Segment(offset + i)),
            }
        }
        offset += anchors.len();
    }
    segment
}

/// Resolve the static outer-frame orientation.
/// Args: editor, layer and shape. Returns the inherited angle in radians.
pub(super) fn parent_angle(studio: &Studio, layer: usize, shape: &crate::document::Shape) -> f32 {
    let transform = compositor::shape_parent_transform(&studio.doc, layer, shape.id);
    transform.ky.atan2(transform.sx)
}

/// Resize selected roots in their inherited coordinates.
/// Args: document, snapshots, source and destination bounds and grip angle. Returns updates root geometry while the existing solver owns descendants.
pub(super) fn nested_resize(
    doc: &mut crate::document::Document,
    snaps: &[ObjSnap],
    source: Bounds,
    destination: Bounds,
    angle: f32,
) {
    restore_geometry(doc, snaps);
    let moving: std::collections::HashSet<_> =
        snaps.iter().map(|snap| (snap.layer, snap.id)).collect();
    let mut work: Vec<_> = snaps
        .iter()
        .filter(|snap| snap.id != RASTER_ID)
        .filter_map(|snap| {
            let shape = doc.find_shape(snap.layer, snap.id)?;
            let mut parent = shape.layout.parent;
            for _ in 0..64 {
                let Some(id) = parent else {
                    break;
                };
                if moving.contains(&(snap.layer, id)) {
                    return None;
                }
                parent = doc
                    .find_shape(snap.layer, id)
                    .and_then(|shape| shape.layout.parent);
            }
            let parent = compositor::shape_parent_transform(doc, snap.layer, snap.id);
            let depth = ancestor_depth(doc, snap.layer, snap.id);
            Some((depth, snap, parent, shape.layout.frame))
        })
        .collect();
    work.sort_by_key(|(depth, ..)| *depth);
    let sx = destination.width() / source.width().max(1.);
    let sy = destination.height() / source.height().max(1.);
    let edit = Transform::from_rotate(angle.to_degrees())
        .pre_concat(
            Transform::from_translate(destination.min.x, destination.min.y)
                .pre_scale(sx, sy)
                .pre_translate(-source.min.x, -source.min.y),
        )
        .pre_rotate(-angle.to_degrees());
    for (_, snap, previous, _frame) in work {
        let current = compositor::shape_parent_transform(doc, snap.layer, snap.id);
        let Some(inverse) = current.invert() else {
            continue;
        };
        let Some(mut geom) = snap.geom.clone() else {
            continue;
        };
        let bounds = geom.bbox();
        let center = bounds.center();
        let old_angle = previous.ky.atan2(previous.sx);
        let new_angle = current.ky.atan2(current.sx);
        let rotation = snap.rot + old_angle - new_angle;
        let aligned = (snap.rot + old_angle - angle).sin().abs() < 1e-4;
        let new_center = map_point(inverse.pre_concat(edit).pre_concat(previous), center);
        let rotation = if aligned || (sx - sy).abs() <= 1e-4 * sx.abs().max(sy.abs()).max(1.) {
            geom.map_into(
                bounds,
                Bounds::from_min_size(
                    bounds.min,
                    Pt::new(bounds.width() * sx, bounds.height() * sy),
                ),
            );
            geom.translate(new_center - geom.bbox().center());
            rotation
        } else {
            let matrix = inverse.pre_concat(edit).pre_concat(previous).pre_concat(
                Transform::from_rotate_at(snap.rot.to_degrees(), center.x, center.y),
            );
            if let Geom::Text(run) = &mut geom {
                run.apply_affine([
                    matrix.sx, matrix.ky, matrix.kx, matrix.sy, matrix.tx, matrix.ty,
                ]);
            } else {
                geom = geom.to_path();
                for index in 0..geom.anchors().count() {
                    let anchor = geom.anchor_mut(index).unwrap();
                    let mapped = transform_anchors(&[*anchor], matrix)[0];
                    *anchor = mapped;
                }
            }
            0.
        };
        if let Some(shape) = doc.find_shape_mut(snap.layer, snap.id) {
            shape.geom = geom;
            shape.rotation = rotation;
        }
    }
}

/// Rotate captured geometry through its frame hierarchy.
/// Args: document, snapshots, pivot and angle. Returns updates each object once in its new parent coordinates.
pub(super) fn nested_rotate(
    doc: &mut crate::document::Document,
    snaps: &[ObjSnap],
    center: Pt,
    angle: f32,
) {
    restore_geometry(doc, snaps);
    let mut work: Vec<_> = snaps
        .iter()
        .filter(|snap| snap.id != RASTER_ID)
        .filter_map(|snap| {
            let parent = compositor::shape_parent_transform(doc, snap.layer, snap.id);
            Some((ancestor_depth(doc, snap.layer, snap.id), snap, parent))
        })
        .collect();
    work.sort_by_key(|(depth, ..)| *depth);
    for (_, snap, previous) in work {
        let current = compositor::shape_parent_transform(doc, snap.layer, snap.id);
        let Some(inverse) = current.invert() else {
            continue;
        };
        let Some(mut geom) = snap.geom.clone() else {
            continue;
        };
        let origin = geom.bbox().center();
        let mapped = map_point(previous, origin).rotate_about(center, angle);
        geom.translate(map_point(inverse, mapped) - origin);
        if let Some(shape) = doc.find_shape_mut(snap.layer, snap.id) {
            shape.geom = geom;
            shape.rotation =
                snap.rot + angle + previous.ky.atan2(previous.sx) - current.ky.atan2(current.sx);
        }
    }
}

fn ancestor_depth(doc: &crate::document::Document, layer: usize, id: u64) -> usize {
    let mut parent = doc.find_shape(layer, id).and_then(|s| s.layout.parent);
    let mut depth = 0;
    while let Some(id) = parent {
        if depth >= 64 {
            break;
        }
        depth += 1;
        parent = doc.find_shape(layer, id).and_then(|s| s.layout.parent);
    }
    depth
}
fn restore_geometry(doc: &mut crate::document::Document, snaps: &[ObjSnap]) {
    for snap in snaps {
        if let Some(shape) = doc.find_shape_mut(snap.layer, snap.id) {
            if let Some(geom) = &snap.geom {
                shape.geom = geom.clone();
                shape.rotation = snap.rot;
            }
        }
    }
}

/// Resolve outer placements for selected animation roots.
/// Args: editor and captured objects. Returns parent transforms with selected descendants excluded.
pub(super) fn motion_parents(
    studio: &Studio,
    snaps: &[ObjSnap],
) -> std::collections::HashMap<(usize, u64), Transform> {
    let moving: std::collections::HashSet<_> =
        snaps.iter().map(|snap| (snap.layer, snap.id)).collect();
    snaps
        .iter()
        .filter_map(|snap| {
            let mut parent = studio
                .doc
                .find_shape(snap.layer, snap.id)
                .and_then(|shape| shape.layout.parent);
            for _ in 0..64 {
                let Some(id) = parent else {
                    break;
                };
                if moving.contains(&(snap.layer, id)) {
                    return None;
                }
                parent = studio
                    .doc
                    .find_shape(snap.layer, id)
                    .and_then(|shape| shape.layout.parent);
            }
            let transform =
                compositor::shape_parent_transform_with(&studio.doc, snap.layer, snap.id, |id| {
                    studio.doc.motion.pose(id, studio.playhead)
                });
            Some(((snap.layer, snap.id), transform))
        })
        .collect()
}
