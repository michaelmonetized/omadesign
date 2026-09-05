use super::*;

pub(crate) struct PointCache {
    generation: u64,
    motion_time: Option<u32>,
    excluded: Vec<(usize, u64)>,
    scene: snap::Scene,
    frozen: bool,
}

impl Studio {
    pub fn toggle_snapping(&mut self) {
        self.snap.enabled = !self.snap.enabled;
        self.status = format!(
            "Snapping {} · hold Ctrl while dragging to invert",
            if self.snap.enabled { "on" } else { "off" }
        );
    }
    pub(crate) fn reset_snap_gesture(&mut self) {
        self.snap_points = None;
        self.snap_scene = None;
        self.snap_bounds = None;
        self.snap_feedback = snap::Feedback::default();
        self.stroke_constraint = None;
    }
    pub(crate) fn effective_snap(&self) -> SnapSettings {
        SnapSettings {
            enabled: self.snap.enabled ^ self.snap_override,
            ..self.snap
        }
    }
    fn make_snap_scene(&self, excluded: &[(usize, u64)], boards: &[u64]) -> snap::Scene {
        if self.is_motion() {
            snap::Scene::new_posed(&self.doc, excluded, boards, self.playhead, &self.pose_drag)
        } else {
            snap::Scene::new(&self.doc, excluded, boards)
        }
    }

    pub(crate) fn snap_tool_point(&mut self, point: Pt, anchor: Option<Pt>) -> Pt {
        let motion_time = self.is_motion().then(|| self.playhead.to_bits());
        if self.snap_points.as_ref().is_none_or(|cache| {
            cache.excluded != self.selection
                || cache.motion_time != motion_time
                || (!cache.frozen && cache.generation != self.canvas_gen)
        }) {
            self.snap_points = Some(PointCache {
                generation: self.canvas_gen,
                motion_time,
                excluded: self.selection.clone(),
                scene: self.make_snap_scene(&self.selection, &[]),
                frozen: false,
            });
        }
        let (point, feedback) = self.snap_points.as_ref().unwrap().scene.point(
            self.effective_snap(),
            point,
            self.view.scale,
            anchor,
        );
        self.snap_feedback = feedback;
        point
    }
    pub(crate) fn freeze_point_snapping(&mut self) {
        self.snap_points = Some(PointCache {
            generation: self.canvas_gen,
            motion_time: self.is_motion().then(|| self.playhead.to_bits()),
            excluded: self.selection.clone(),
            scene: self.make_snap_scene(&self.selection, &[]),
            frozen: true,
        });
    }
    pub(crate) fn precise_drag(&mut self, point: Pt, shift: bool) -> Pt {
        if matches!(
            self.op,
            Some(
                Op::Brush { .. }
                    | Op::Smudge { .. }
                    | Op::Clone { .. }
                    | Op::Retouch { .. }
                    | Op::Pencil { .. }
            )
        ) {
            if !shift {
                self.stroke_constraint = None;
                return point;
            }
            if self.stroke_constraint.is_none() {
                self.stroke_constraint = match &self.op {
                    Some(
                        Op::Brush { last, .. } | Op::Smudge { last, .. } | Op::Clone { last, .. },
                    ) => *last,
                    Some(Op::Retouch { last, .. }) => Some(*last),
                    Some(Op::Pencil { pts }) => pts.last().copied(),
                    _ => None,
                };
            }
            return self.stroke_constraint.map_or(point, |anchor| {
                anchor + crate::geom::constrain_45(point - anchor)
            });
        }
        if matches!(
            self.op,
            Some(
                Op::Node {
                    which: NodeHit::HandleIn(_) | NodeHit::HandleOut(_),
                    ..
                } | Op::Pen { .. }
                    | Op::Rotate { .. }
                    | Op::ArtboardRotate { .. }
                    | Op::Corner { .. }
            )
        ) {
            return point;
        }
        if self.snap_scene.is_none() {
            let (excluded, boards) = match &self.op {
                Some(Op::Move { orig, .. } | Op::Resize { orig, .. } | Op::Rotate { orig, .. }) => {
                    (
                        orig.iter().map(|s| (s.layer, s.id)).collect::<Vec<_>>(),
                        vec![],
                    )
                }
                Some(Op::ArtboardMove { contents, ids, .. }) => (
                    contents.iter().map(|s| (s.layer, s.id)).collect(),
                    ids.clone(),
                ),
                Some(Op::ArtboardResize { contents, orig, .. }) => (
                    contents.iter().map(|s| (s.layer, s.id)).collect(),
                    vec![orig.id],
                ),
                _ => (self.selection.clone(), vec![]),
            };
            self.snap_bounds = match &self.op {
                Some(Op::Move { .. }) => excluded
                    .iter()
                    .filter_map(|(li, id)| {
                        if *id == RASTER_ID {
                            self.doc.layers.get(*li)?.kind.raster_bounds()
                        } else {
                            self.doc.find_shape(*li, *id).map(|shape| {
                                let bounds = shape.world_bbox();
                                if self.is_motion() {
                                    self.live_pose(*id).map_bounds(bounds)
                                } else {
                                    bounds
                                }
                            })
                        }
                    })
                    .reduce(|a, b| Bounds {
                        min: Pt::new(a.min.x.min(b.min.x), a.min.y.min(b.min.y)),
                        max: Pt::new(a.max.x.max(b.max.x), a.max.y.max(b.max.y)),
                    }),
                Some(Op::ArtboardMove { orig, ids, .. }) => orig
                    .iter()
                    .filter(|a| ids.contains(&a.id))
                    .map(Artboard::bounds)
                    .reduce(|a, b| Bounds {
                        min: Pt::new(a.min.x.min(b.min.x), a.min.y.min(b.min.y)),
                        max: Pt::new(a.max.x.max(b.max.x), a.max.y.max(b.max.y)),
                    }),
                _ => None,
            };
            self.snap_scene = Some(self.make_snap_scene(&excluded, &boards));
        }
        let scene = self.snap_scene.as_ref().unwrap();
        let (point, feedback) = match &self.op {
            Some(Op::Move { start, .. } | Op::ArtboardMove { start, .. }) => {
                let (delta, feedback) = scene.movement(
                    self.effective_snap(),
                    self.snap_bounds.unwrap_or(Bounds::from_pt(*start)),
                    point - *start,
                    self.view.scale,
                    shift,
                );
                (*start + delta, feedback)
            }
            Some(Op::Node {
                orig: Geom::Path { anchors, .. },
                which: NodeHit::Point(index),
                ..
            }) if shift => scene.point(
                self.effective_snap(),
                point,
                self.view.scale,
                anchors.get(*index).map(|a| a.pt),
            ),
            _ => scene.point(self.effective_snap(), point, self.view.scale, None),
        };
        self.snap_feedback = feedback;
        point
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn motion_studio() -> (Studio, u64, u64) {
        let mut studio = Studio::new();
        studio.persona = Persona::Motion;
        studio.doc.layers = vec![Layer::vector("Animated shapes")];
        let mut ids = Vec::new();
        for x in [20.0, 150.0] {
            let shape = Shape::new(
                Geom::Rect {
                    origin: Pt::new(x, 20.0),
                    size: Pt::splat(20.0),
                    radius: 0.0,
                },
                Style::default(),
            );
            ids.push(shape.id);
            studio.doc.layers[0].kind.shapes_mut().unwrap().push(shape);
        }
        studio.doc.motion.duration = 2.0;
        studio
            .doc
            .motion
            .set_key(ids[0], Prop::X, 0.0, 40.0, Ease::Linear);
        studio
            .doc
            .motion
            .set_key(ids[1], Prop::X, 0.0, 110.0, Ease::Linear);
        studio
            .doc
            .motion
            .set_key(ids[1], Prop::X, 1.0, 210.0, Ease::Linear);
        studio.selection = vec![(0, ids[0])];
        studio.snap = SnapSettings {
            grid: false,
            guides: false,
            artboards: false,
            spacing: false,
            ..Default::default()
        };
        (studio, ids[0], ids[1])
    }

    #[test]
    fn motion_movement_snaps_the_visible_edge_to_the_evaluated_target() {
        let (mut studio, moving, _) = motion_studio();
        let shape = studio.doc.find_shape(0, moving).unwrap();
        let start = Pt::new(60.0, 20.0);
        studio.op = Some(Op::Move {
            orig: vec![ObjSnap {
                layer: 0,
                id: moving,
                geom: Some(shape.geom.clone()),
                origin: shape.geom.bbox().min,
                size: shape.geom.bbox().size(),
                rot: 0.0,
            }],
            start,
        });
        let snapped = studio.precise_drag(start + Pt::new(178.0, 0.0), false);
        assert_eq!(snapped, start + Pt::new(180.0, 0.0));
        let initial = studio.snap_bounds.unwrap();
        assert_eq!(initial.min, Pt::new(60.0, 20.0));
        assert_eq!(initial.max.x + (snapped.x - start.x), 260.0);
    }

    #[test]
    fn point_snap_cache_tracks_playhead_and_live_pose_changes() {
        let (mut studio, _, target) = motion_studio();
        assert_eq!(studio.snap_tool_point(Pt::new(258.0, 100.0), None).x, 260.0);
        studio.playhead = 1.0;
        assert_eq!(studio.snap_tool_point(Pt::new(358.0, 100.0), None).x, 360.0);
        studio.pose_drag.insert(
            target,
            Pose {
                dx: 250.0,
                ..Pose::identity()
            },
        );
        studio.mark();
        assert_eq!(studio.snap_tool_point(Pt::new(398.0, 100.0), None).x, 400.0);
    }
}
