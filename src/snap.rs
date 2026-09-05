//! Screen-tolerant alignment and equal-spacing snaps for points and moving bounds.
use crate::document::{Document, RASTER_ID};
use crate::geom::{Bounds, Pt};
use std::collections::{HashMap, HashSet};

#[derive(Clone, Copy, Debug)]
pub struct SnapSettings {
    pub enabled: bool,
    pub grid: bool,
    pub guides: bool,
    pub objects: bool,
    pub artboards: bool,
    pub spacing: bool,
    pub threshold: f32,
}
impl Default for SnapSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            grid: true,
            guides: true,
            objects: true,
            artboards: true,
            spacing: true,
            threshold: 6.0,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct Feedback {
    pub lines: Vec<(Pt, Pt)>,
    pub gaps: Vec<(Pt, Pt)>,
}
#[derive(Clone)]
struct Candidate {
    correction: f32,
    line: Option<(Pt, Pt)>,
    gaps: Option<[(Pt, Pt); 2]>,
}

/// Frozen targets exclude the moving objects, preventing self-snaps and feedback drift.
#[derive(Clone)]
pub struct Scene {
    objects: Vec<Bounds>,
    boards: Vec<Bounds>,
    guides: Vec<(bool, f32)>,
    guide_paths: Vec<(Vec<Pt>, bool)>,
    grid: Option<f32>,
}
impl Scene {
    pub fn new(doc: &Document, excluded: &[(usize, u64)], boards: &[u64]) -> Self {
        Self::build(doc, excluded, boards, None)
    }

    /// Motion targets use the same evaluated pose and drag overrides as the
    /// canvas, rather than snapping animated objects to their invisible rest pose.
    pub fn new_posed(
        doc: &Document,
        excluded: &[(usize, u64)],
        boards: &[u64],
        time: f32,
        overrides: &HashMap<u64, crate::motion::Pose>,
    ) -> Self {
        Self::build(doc, excluded, boards, Some((time, overrides)))
    }

    fn build(
        doc: &Document,
        excluded: &[(usize, u64)],
        boards: &[u64],
        motion: Option<(f32, &HashMap<u64, crate::motion::Pose>)>,
    ) -> Self {
        let excluded: HashSet<_> = excluded.iter().copied().collect();
        let mut objects = Vec::new();
        let mut guide_paths = Vec::new();
        for (li, layer) in doc
            .layers
            .iter()
            .enumerate()
            .filter(|(_, layer)| layer.visible)
        {
            if let Some(shapes) = layer.kind.shapes() {
                for shape in shapes
                    .iter()
                    .filter(|shape| shape.visible && !excluded.contains(&(li, shape.id)))
                {
                    let bounds = shape.world_bbox();
                    let pose = motion.map(|(time, overrides)| {
                        overrides
                            .get(&shape.id)
                            .copied()
                            .unwrap_or_else(|| doc.motion.pose(shape.id, time))
                    });
                    if shape.guide {
                        for mut contour in shape.world_contours(96) {
                            if let Some(pose) = pose {
                                for point in &mut contour {
                                    *point = pose.map(bounds.center(), *point);
                                }
                            }
                            guide_paths.push((contour, shape.geom.is_closed()));
                        }
                    } else {
                        objects.push(pose.map_or(bounds, |pose| pose.map_bounds(bounds)));
                    }
                }
            } else if !excluded.contains(&(li, RASTER_ID))
                && layer.kind.is_placed_raster()
                && let Some(bounds) = layer.kind.raster_bounds()
            {
                objects.push(bounds);
            }
        }
        let mut artboards: Vec<_> = doc
            .artboards
            .iter()
            .filter(|board| !boards.contains(&board.id))
            .map(|board| board.bounds())
            .collect();
        if artboards.is_empty() && boards.is_empty() {
            artboards.push(Bounds {
                min: Pt::ZERO,
                max: doc.size(),
            });
        }
        Self {
            objects,
            boards: artboards,
            guides: doc
                .guides
                .iter()
                .map(|guide| (guide.vertical, guide.pos))
                .collect(),
            guide_paths,
            grid: doc.grid.snap.then_some(doc.grid.size.max(1.0)),
        }
    }

    pub fn point(
        &self,
        settings: SnapSettings,
        point: Pt,
        scale: f32,
        anchor: Option<Pt>,
    ) -> (Pt, Feedback) {
        let origin = anchor.unwrap_or(point);
        let (delta, feedback) = self.movement(
            settings,
            Bounds::from_pt(origin),
            point - origin,
            scale,
            anchor.is_some(),
        );
        (origin + delta, feedback)
    }

    pub fn movement(
        &self,
        settings: SnapSettings,
        bounds: Bounds,
        delta: Pt,
        scale: f32,
        constrained: bool,
    ) -> (Pt, Feedback) {
        let delta = if constrained {
            crate::geom::constrain_45(delta)
        } else {
            delta
        };
        if !settings.enabled {
            return (delta, Feedback::default());
        }
        let tolerance = settings.threshold / scale.max(0.01);
        let moved = Bounds {
            min: bounds.min + delta,
            max: bounds.max + delta,
        };
        let mut best: [Option<Candidate>; 2] = [None, None];
        for (axis, slot) in best.iter_mut().enumerate() {
            let (min, max, mid, cross_min, cross_max) = axes(moved, axis);
            let probes = [min, mid, max];
            let mut offer = |candidate: Candidate| {
                if candidate.correction.abs() <= tolerance
                    && slot
                        .as_ref()
                        .is_none_or(|old| candidate.correction.abs() < old.correction.abs())
                {
                    *slot = Some(candidate);
                }
            };
            for target in self
                .boards
                .iter()
                .filter(|_| settings.artboards)
                .chain(self.objects.iter().filter(|_| settings.objects))
            {
                let (lo, hi, center, c0, c1) = axes(*target, axis);
                for target in [lo, center, hi] {
                    for probe in probes {
                        offer(Candidate {
                            correction: target - probe,
                            line: Some((
                                unaxes(axis, target, c0.min(cross_min)),
                                unaxes(axis, target, c1.max(cross_max)),
                            )),
                            gaps: None,
                        });
                    }
                }
            }
            if settings.guides {
                for &(vertical, target) in &self.guides {
                    if vertical == (axis == 0) {
                        for probe in probes {
                            offer(Candidate {
                                correction: target - probe,
                                line: Some((
                                    unaxes(axis, target, cross_min - 24.0 / scale),
                                    unaxes(axis, target, cross_max + 24.0 / scale),
                                )),
                                gaps: None,
                            });
                        }
                    }
                }
            }
            if settings.spacing && settings.objects && max > min {
                // Only nearby rows/columns participate; sorting adjacent objects avoids O(n²) pairs.
                let mut neighbors: Vec<_> = self
                    .objects
                    .iter()
                    .copied()
                    .filter(|target| {
                        let (_, _, _, a, b) = axes(*target, axis);
                        a <= cross_max + tolerance && b >= cross_min - tolerance
                    })
                    .collect();
                neighbors.sort_by(|a, b| axes(*a, axis).0.total_cmp(&axes(*b, axis).0));
                let across = (cross_min + cross_max) * 0.5;
                for pair in neighbors.windows(2) {
                    let (a0, a1, _, _, _) = axes(pair[0], axis);
                    let (b0, b1, _, _, _) = axes(pair[1], axis);
                    let gap = b0 - a1;
                    if gap <= 0.0 {
                        continue;
                    }
                    let width = max - min;
                    let right = b1 + gap;
                    let left = a0 - gap - width;
                    let spans = |a: f32, b: f32| (unaxes(axis, a, across), unaxes(axis, b, across));
                    offer(Candidate {
                        correction: right - min,
                        line: None,
                        gaps: Some([spans(a1, b0), spans(b1, right)]),
                    });
                    offer(Candidate {
                        correction: left - min,
                        line: None,
                        gaps: Some([spans(left + width, a0), spans(a1, b0)]),
                    });
                    if gap >= width {
                        let between = (a1 + b0 - width) * 0.5;
                        offer(Candidate {
                            correction: between - min,
                            line: None,
                            gaps: Some([spans(a1, between), spans(between + width, b0)]),
                        });
                    }
                }
            }
            // Objects and guides win ties over the grid.
            if settings.grid
                && let Some(grid) = self.grid
            {
                for probe in probes {
                    let target = (probe / grid).round() * grid;
                    offer(Candidate {
                        correction: target - probe,
                        line: None,
                        gaps: None,
                    });
                }
            }
        }
        let mut output = delta;
        let mut feedback = Feedback::default();
        let mut accepted = false;
        let mut take = |candidate: &Candidate| {
            accepted = true;
            if let Some(line) = candidate.line {
                feedback.lines.push(line);
            }
            if let Some(gaps) = &candidate.gaps {
                feedback.gaps.extend_from_slice(gaps);
            }
        };
        if constrained {
            let length = delta.length();
            if length > 1e-5 {
                let axis = delta / length;
                let mut choice: Option<(f32, usize)> = None;
                for (i, component) in [axis.x, axis.y].iter().copied().enumerate() {
                    if component.abs() > 0.1
                        && let Some(candidate) = &best[i]
                    {
                        let distance = candidate.correction / component;
                        if distance.abs() <= tolerance
                            && choice.is_none_or(|old| distance.abs() < old.0.abs())
                        {
                            choice = Some((distance, i));
                        }
                    }
                }
                if let Some((distance, i)) = choice {
                    output += axis * distance;
                    take(best[i].as_ref().unwrap());
                }
            }
        } else {
            if let Some(candidate) = &best[0] {
                output.x += candidate.correction;
                take(candidate);
            }
            if let Some(candidate) = &best[1] {
                output.y += candidate.correction;
                take(candidate);
            }
        }
        if settings.guides {
            let probes = [
                moved.center(),
                moved.min,
                moved.max,
                Pt::new(moved.min.x, moved.max.y),
                Pt::new(moved.max.x, moved.min.y),
                Pt::new(moved.center().x, moved.min.y),
                Pt::new(moved.center().x, moved.max.y),
                Pt::new(moved.min.x, moved.center().y),
                Pt::new(moved.max.x, moved.center().y),
            ];
            let probes = if moved.width() < 1e-6 && moved.height() < 1e-6 {
                &probes[..1]
            } else {
                &probes[..]
            };
            let direction = (constrained && delta.length() > 1e-5).then(|| delta.normalized());
            let mut closest: Option<(Pt, (Pt, Pt))> = None;
            for (points, closed) in &self.guide_paths {
                let count =
                    points.len().saturating_sub(1) + usize::from(*closed && points.len() > 2);
                for index in 0..count {
                    let a = points[index];
                    let b = points[(index + 1) % points.len()];
                    if !(Bounds {
                        min: a.min(b),
                        max: a.max(b),
                    })
                    .inflate(tolerance)
                    .intersects(moved)
                    {
                        continue;
                    }
                    let segment = b - a;
                    for &probe in probes {
                        let correction = if let Some(direction) = direction {
                            let denominator = direction.cross(segment);
                            if denominator.abs() < 1e-8 {
                                continue;
                            }
                            let offset = a - probe;
                            let along_segment = offset.cross(direction) / denominator;
                            if !(0.0..=1.0).contains(&along_segment) {
                                continue;
                            }
                            direction * (offset.cross(segment) / denominator)
                        } else if constrained {
                            continue;
                        } else {
                            let t = if segment.length_sq() > 1e-10 {
                                (probe - a).dot(segment) / segment.length_sq()
                            } else {
                                0.0
                            };
                            a + segment * t.clamp(0.0, 1.0) - probe
                        };
                        if correction.length() <= tolerance
                            && closest
                                .as_ref()
                                .is_none_or(|(old, _)| correction.length_sq() < old.length_sq())
                        {
                            closest = Some((correction, (a, b)));
                        }
                    }
                }
            }
            if let Some((correction, line)) = closest
                && (!accepted || correction.length_sq() <= (output - delta).length_sq() + 1e-8)
            {
                output = delta + correction;
                feedback = Feedback {
                    lines: vec![line],
                    gaps: vec![],
                };
            }
        }
        (output, feedback)
    }
}
fn axes(bounds: Bounds, axis: usize) -> (f32, f32, f32, f32, f32) {
    if axis == 0 {
        (
            bounds.min.x,
            bounds.max.x,
            bounds.center().x,
            bounds.min.y,
            bounds.max.y,
        )
    } else {
        (
            bounds.min.y,
            bounds.max.y,
            bounds.center().y,
            bounds.min.x,
            bounds.max.x,
        )
    }
}
fn unaxes(axis: usize, along: f32, across: f32) -> Pt {
    if axis == 0 {
        Pt::new(along, across)
    } else {
        Pt::new(across, along)
    }
}
pub fn snap_point(doc: &Document, settings: SnapSettings, point: Pt, scale: f32) -> Pt {
    Scene::new(doc, &[], &[])
        .point(settings, point, scale, None)
        .0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::{Shape, Style};
    use crate::geom::Geom;
    fn bounds(x: f32, y: f32, width: f32) -> Bounds {
        Bounds {
            min: Pt::new(x, y),
            max: Pt::new(x + width, y + 20.0),
        }
    }
    fn object(doc: &mut Document, x: f32, visible: bool) -> u64 {
        let mut shape = Shape::new(
            Geom::Rect {
                origin: Pt::new(x, 20.0),
                size: Pt::splat(20.0),
                radius: 0.0,
            },
            Style::default(),
        );
        shape.visible = visible;
        let id = shape.id;
        doc.layers[1].kind.shapes_mut().unwrap().push(shape);
        id
    }
    fn settings() -> SnapSettings {
        SnapSettings {
            grid: false,
            guides: false,
            artboards: false,
            ..Default::default()
        }
    }
    #[test]
    fn moving_edges_and_centers_snap_without_snapping_to_self_or_hidden_shapes() {
        let mut doc = Document::new("snap", 800.0, 600.0, 96.0);
        let moving = object(&mut doc, 10.0, true);
        object(&mut doc, 100.0, true);
        object(&mut doc, 55.0, false);
        let scene = Scene::new(&doc, &[(1, moving)], &[]);
        let (delta, feedback) = scene.movement(
            settings(),
            bounds(10.0, 20.0, 20.0),
            Pt::new(68.0, 0.0),
            1.0,
            false,
        );
        assert_eq!(delta.x, 70.0);
        assert!(!feedback.lines.is_empty());
        assert_eq!(
            scene.point(settings(), Pt::new(55.0, 55.0), 1.0, None).0.x,
            55.0
        );
        assert_eq!(
            scene.point(settings(), Pt::new(11.0, 55.0), 1.0, None).0.x,
            11.0
        );
        assert_eq!(
            scene.point(settings(), Pt::new(107.0, 55.0), 1.0, None).0.x,
            110.0
        );
    }
    #[test]
    fn equal_gaps_repeat_and_balance_with_zoom_independent_tolerance() {
        let mut doc = Document::new("snap", 800.0, 600.0, 96.0);
        object(&mut doc, 20.0, true);
        object(&mut doc, 70.0, true);
        let scene = Scene::new(&doc, &[], &[]);
        let (delta, feedback) = scene.movement(
            settings(),
            bounds(0.0, 20.0, 20.0),
            Pt::new(118.0, 0.0),
            1.0,
            false,
        );
        assert_eq!(delta.x, 120.0);
        assert_eq!(feedback.gaps.len(), 2);
        let (delta, _) = scene.movement(
            settings(),
            bounds(0.0, 20.0, 20.0),
            Pt::new(47.0, 0.0),
            1.0,
            false,
        );
        assert_eq!(delta.x, 45.0);
        assert_eq!(
            scene.point(settings(), Pt::new(24.0, 100.0), 2.0, None).0.x,
            24.0
        );
    }
    #[test]
    fn posed_targets_follow_rotation_scale_and_live_overrides() {
        let mut doc = Document::new("posed snap", 800.0, 600.0, 96.0);
        let shape = Shape::new(
            Geom::Rect {
                origin: Pt::new(50.0, 20.0),
                size: Pt::new(20.0, 40.0),
                radius: 0.0,
            },
            Style::default(),
        );
        let id = shape.id;
        doc.layers[1].kind.shapes_mut().unwrap().push(shape);
        let mut overrides = HashMap::new();
        overrides.insert(
            id,
            crate::motion::Pose {
                dx: 100.0,
                rotation: std::f32::consts::FRAC_PI_2,
                scale: 2.0,
                ..crate::motion::Pose::identity()
            },
        );
        let scene = Scene::new_posed(&doc, &[], &[], 0.0, &overrides);
        // A 20×40 rectangle becomes 80×40 around (60,40), then moves 100px right.
        assert_eq!(
            scene
                .point(settings(), Pt::new(118.0, 100.0), 1.0, None)
                .0
                .x,
            120.0
        );
        assert_eq!(
            scene
                .point(settings(), Pt::new(158.0, 100.0), 1.0, None)
                .0
                .x,
            160.0
        );
        assert_eq!(
            scene
                .point(settings(), Pt::new(198.0, 100.0), 1.0, None)
                .0
                .x,
            200.0
        );
    }

    #[test]
    fn guide_grid_board_and_diagonal_constraints_cooperate() {
        let mut doc = Document::new("snap", 200.0, 100.0, 96.0);
        doc.guides.push(crate::document::Guide {
            vertical: true,
            pos: 83.0,
        });
        let scene = Scene::new(&doc, &[], &[]);
        let s = SnapSettings {
            grid: false,
            objects: false,
            ..Default::default()
        };
        assert_eq!(scene.point(s, Pt::new(81.0, 50.0), 1.0, None).0.x, 83.0);
        assert_eq!(
            scene.point(s, Pt::new(102.0, 52.0), 1.0, None).0,
            Pt::new(100.0, 50.0)
        );
        let (delta, _) = scene.movement(s, bounds(0.0, 0.0, 0.0), Pt::new(80.0, 78.0), 1.0, true);
        assert!((delta.x - delta.y).abs() < 0.001);
        let off = SnapSettings {
            enabled: false,
            ..s
        };
        assert_eq!(
            scene.point(off, Pt::new(81.0, 53.0), 1.0, None).0,
            Pt::new(81.0, 53.0)
        );
        let grid = SnapSettings {
            guides: false,
            objects: false,
            artboards: false,
            spacing: false,
            ..Default::default()
        };
        assert_eq!(
            scene.point(grid, Pt::new(9.0, 3.0), 1.0, None).0,
            Pt::new(8.0, 0.0)
        );
    }
}

#[cfg(test)]
mod object_guide_tests {
    use super::*;
    use crate::document::{Layer, Shape, Style};
    use crate::geom::{Anchor, Geom};

    #[test]
    fn curved_guides_snap_to_the_rendered_curve_and_preserve_shift_direction() {
        let mut doc = Document::new("Curved guide", 200.0, 200.0, 72.0);
        doc.layers = vec![Layer::vector("Guides")];
        let mut start = Anchor::corner(Pt::ZERO);
        start.h_out = Pt::new(0.0, 100.0);
        let mut end = Anchor::corner(Pt::new(100.0, 0.0));
        end.h_in = Pt::new(0.0, 100.0);
        let mut guide = Shape::new(
            Geom::Path {
                anchors: vec![start, end],
                closed: false,
            },
            Style::default(),
        );
        guide.guide = true;
        doc.layers[0].kind.shapes_mut().unwrap().push(guide.clone());
        let settings = SnapSettings {
            grid: false,
            objects: false,
            artboards: false,
            spacing: false,
            ..Default::default()
        };
        let scene = Scene::new(&doc, &[], &[]);
        let (point, feedback) = scene.point(settings, Pt::new(50.0, 78.0), 1.0, None);
        assert!((point - Pt::new(50.0, 75.0)).length() < 0.01);
        assert!(!feedback.lines.is_empty());
        let (point, _) = scene.point(settings, Pt::new(50.0, 78.0), 1.0, Some(Pt::new(0.0, 25.0)));
        assert!((point - Pt::new(50.0, 75.0)).length() < 0.01);
        let (point, feedback) = scene.point(
            SnapSettings {
                objects: true,
                guides: false,
                ..settings
            },
            Pt::new(49.0, 1.0),
            1.0,
            None,
        );
        assert_eq!(point, Pt::new(49.0, 1.0));
        assert!(
            feedback.lines.is_empty(),
            "object guides must not masquerade as bbox targets"
        );
        let scene = Scene::new(&doc, &[(0, guide.id)], &[]);
        let (point, _) = scene.point(settings, Pt::new(50.0, 78.0), 1.0, None);
        assert_eq!(
            point,
            Pt::new(50.0, 78.0),
            "an edited guide must not snap to itself"
        );
    }
}
