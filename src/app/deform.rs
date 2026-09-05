//! Interactive vector deformation. Every preview comes from the original
//! contours; pointer release records one command for the entire selection.

use super::*;
use crate::deform::{Cage, Mapper, Mode};

pub struct DeformSession {
    pub cage: Cage,
    sources: Vec<Source>,
    committed: Vec<Shape>,
    drag: Option<Drag>,
}

struct Source {
    layer: usize,
    shape: Shape,
    contours: Vec<Vec<Pt>>,
}

struct Drag {
    handle: usize,
    pointer: Pt,
    cage: Cage,
    before: Vec<Shape>,
}

impl DeformSession {
    pub fn dragging(&self) -> bool {
        self.drag.is_some()
    }
    pub fn active_handle(&self) -> Option<usize> {
        self.drag.as_ref().map(|d| d.handle)
    }
}

impl Studio {
    pub fn can_deform(&self) -> bool {
        self.persona == Persona::Design
            && !self.selection.is_empty()
            && self.selection.iter().all(|(li, id)| {
                self.doc
                    .layers
                    .get(*li)
                    .is_some_and(|layer| layer.visible && !layer.locked)
                    && self
                        .doc
                        .find_shape(*li, *id)
                        .is_some_and(|s| s.visible && !s.locked)
            })
    }

    pub fn begin_deform(&mut self, mode: Mode) {
        if self.persona != Persona::Design {
            self.status = "Switch to Design to reshape vector objects".into();
            return;
        }
        self.end_deform(false);
        self.commit_type_edit();
        if !self.can_deform() {
            self.status = "Select unlocked vector objects to reshape".into();
            return;
        }
        let mut bounds: Option<Bounds> = None;
        let shapes: Vec<_> = self
            .selection
            .iter()
            .filter_map(|(li, id)| {
                self.doc.find_shape(*li, *id).map(|shape| {
                    bounds =
                        Some(bounds.map_or(shape.world_bbox(), |b| b.union(shape.world_bbox())));
                    (*li, shape.clone())
                })
            })
            .collect();
        let Some(cage) = bounds.and_then(|b| Cage::new(mode, b)) else {
            self.status = "The selection needs width and height to reshape".into();
            return;
        };
        let step = cage.bounds.width().max(cage.bounds.height()) / 48.0;
        let sources: Option<Vec<_>> = shapes
            .into_iter()
            .map(|(layer, shape)| {
                let contours =
                    subdivide_contours(shape.world_contours(128), shape.geom.is_closed(), step)?;
                Some(Source {
                    layer,
                    shape,
                    contours,
                })
            })
            .collect();
        let Some(sources) = sources else {
            self.status = "This selection has too many path points to reshape interactively".into();
            return;
        };
        if sources
            .iter()
            .flat_map(|source| &source.contours)
            .map(Vec::len)
            .sum::<usize>()
            > 100_000
        {
            self.status = "Select fewer objects to reshape interactively".into();
            return;
        }
        self.set_tool(Tool::Select);
        self.op = None;
        self.artboard_sel.clear();
        self.deformation = Some(DeformSession {
            cage,
            committed: sources.iter().map(|source| source.shape.clone()).collect(),
            sources,
            drag: None,
        });
        self.reset_snap_gesture();
        self.status = format!("{} · drag a handle · Esc to leave", mode.label());
    }

    pub fn deformation_drag_start(&mut self, handle: usize, pointer: Pt) {
        let Some(session) = self.deformation.as_mut() else {
            return;
        };
        if session.drag.is_some() || handle >= session.cage.handles().len() {
            return;
        }
        // A selection change or an external edit must never be overwritten by
        // stale source snapshots. Mode entry can rebuild the envelope afterward.
        let valid = self.selection.len() == session.sources.len()
            && session
                .sources
                .iter()
                .zip(&session.committed)
                .all(|(source, committed)| {
                    self.selection.contains(&(source.layer, source.shape.id))
                        && self.doc.find_shape(source.layer, source.shape.id) == Some(committed)
                });
        if !valid {
            self.deformation = None;
            self.status = "Selection changed · choose a reshape mode again".into();
            return;
        }
        session.drag = Some(Drag {
            handle,
            pointer,
            cage: session.cage.clone(),
            before: session.committed.clone(),
        });
        self.freeze_point_snapping();
    }

    pub fn deformation_drag_to(&mut self, pointer: Pt, shift: bool) {
        let Some(mut session) = self.deformation.take() else {
            return;
        };
        if let Some(drag) = &session.drag {
            let mut delta = pointer - drag.pointer;
            let handle = drag.cage.handles()[drag.handle];
            let skew = session.cage.mode == Mode::Skew;
            if skew {
                delta = if drag.handle.is_multiple_of(2) {
                    Pt::new(delta.x, 0.0)
                } else {
                    Pt::new(0.0, delta.y)
                };
            }
            // Snap the actual grip, preserving the initial pointer grab offset.
            // A click without movement must never snap or flatten an object.
            if delta.length_sq() > 1e-10 {
                delta = self.snap_tool_point(handle + delta, (shift || skew).then_some(handle))
                    - handle;
            } else {
                self.snap_feedback = snap::Feedback::default();
            }
            if let Some(cage) = drag.cage.dragged(drag.handle, delta) {
                if cage != session.cage {
                    // Returning exactly to pointer-down must restore original
                    // parameterized objects instead of leaving flattened paths.
                    let shapes = if cage == drag.cage {
                        Some(drag.before.clone())
                    } else {
                        cage.mapper().and_then(|mapper| {
                            session
                                .sources
                                .iter()
                                .map(|source| mapped_shape(source, &mapper))
                                .collect::<Option<Vec<_>>>()
                        })
                    };
                    if let Some(shapes) = shapes {
                        write_shapes(&mut self.doc, &session.sources, &shapes);
                        session.cage = cage;
                        self.mark();
                    }
                }
            } else {
                self.status =
                    "Keep the envelope open; crossed or collapsed corners are ignored".into();
            }
        }
        self.deformation = Some(session);
    }

    pub fn deformation_drag_finish(&mut self) {
        let Some(mut session) = self.deformation.take() else {
            return;
        };
        if let Some(drag) = session.drag.take() {
            let mut commands = Vec::new();
            for (source, before) in session.sources.iter().zip(&drag.before) {
                let Some(after) = self.doc.find_shape(source.layer, source.shape.id) else {
                    continue;
                };
                if before.geom != after.geom || before.rotation != after.rotation {
                    commands.push(Cmd::SetGeom {
                        layer: source.layer,
                        id: source.shape.id,
                        before: before.geom.clone(),
                        after: after.geom.clone(),
                        rot_before: before.rotation,
                        rot_after: after.rotation,
                    });
                }
                if before.style != after.style {
                    commands.push(Cmd::SetStyle {
                        layer: source.layer,
                        id: source.shape.id,
                        before: before.style.clone(),
                        after: after.style.clone(),
                    });
                }
            }
            if !commands.is_empty() {
                self.commit(Cmd::Batch(commands));
                session.committed = session
                    .sources
                    .iter()
                    .filter_map(|source| {
                        self.doc.find_shape(source.layer, source.shape.id).cloned()
                    })
                    .collect();
                self.status = format!(
                    "{} applied · drag another handle or finish",
                    session.cage.mode.label()
                );
            }
        }
        self.deformation = Some(session);
        self.reset_snap_gesture();
    }

    /// Cancel restores only the active pointer drag. Previously released drags
    /// remain normal undoable edits. Finish commits a drag before leaving mode.
    pub fn end_deform(&mut self, cancel_drag: bool) {
        if !cancel_drag {
            self.deformation_drag_finish();
        }
        if let Some(session) = self.deformation.take() {
            if let Some(drag) = session.drag {
                write_shapes(&mut self.doc, &session.sources, &drag.before);
                self.mark();
            }
            self.reset_snap_gesture();
        }
    }
}

fn write_shapes(doc: &mut Document, sources: &[Source], shapes: &[Shape]) {
    for (source, after) in sources.iter().zip(shapes) {
        if let Some(shape) = doc.find_shape_mut(source.layer, source.shape.id) {
            shape.geom = after.geom.clone();
            shape.rotation = after.rotation;
            shape.style = after.style.clone();
        }
    }
}

fn mapped_shape(source: &Source, mapper: &Mapper) -> Option<Shape> {
    let contours: Vec<Vec<Pt>> = source
        .contours
        .iter()
        .map(|contour| {
            contour
                .iter()
                .map(|p| mapper.map(*p))
                .collect::<Option<Vec<_>>>()
        })
        .collect::<Option<Vec<_>>>()?;
    let mut after = source.shape.clone();
    after.geom = if source.shape.geom.is_closed() {
        Geom::Poly {
            contours,
            winding: matches!(source.shape.geom, Geom::Poly { winding: true, .. }),
        }
    } else {
        Geom::Path {
            anchors: contours
                .into_iter()
                .next()
                .unwrap_or_default()
                .into_iter()
                .map(Anchor::corner)
                .collect(),
            closed: false,
        }
    };
    // The source contours already include the object's rotation.
    after.rotation = 0.0;
    if let Fill::Linear { from, to, .. } = &mut after.style.fill {
        let before_bounds = source.shape.geom.bbox();
        let after_bounds = after.geom.bbox();
        for endpoint in [from, to] {
            let mapped = mapper.map(
                before_bounds.min
                    + Pt::new(
                        endpoint[0] * before_bounds.width(),
                        endpoint[1] * before_bounds.height(),
                    ),
            )?;
            *endpoint = [
                (mapped.x - after_bounds.min.x) / after_bounds.width().max(1e-6),
                (mapped.y - after_bounds.min.y) / after_bounds.height().max(1e-6),
            ];
        }
    }
    Some(after)
}

fn subdivide_contours(contours: Vec<Vec<Pt>>, closed: bool, step: f32) -> Option<Vec<Vec<Pt>>> {
    const MAX_POINTS: usize = 100_000;
    let mut count = 0;
    let mut out = Vec::with_capacity(contours.len());
    for contour in contours {
        if contour.len() < 2 {
            continue;
        }
        let edges = contour.len() - usize::from(!closed);
        let mut points = Vec::new();
        for i in 0..edges {
            let a = contour[i];
            let b = contour[(i + 1) % contour.len()];
            let segments = ((b - a).length() / step.max(0.01)).ceil().clamp(1.0, 512.0) as usize;
            count += segments;
            if count > MAX_POINTS {
                return None;
            }
            for j in 0..segments {
                points.push(a.lerp(b, j as f32 / segments as f32));
            }
        }
        if !closed {
            points.push(*contour.last()?);
        }
        out.push(points);
    }
    (!out.is_empty()).then_some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn studio() -> Studio {
        let mut studio = Studio::new();
        studio.doc.layers = vec![Layer::vector("Shapes")];
        for x in [10.0, 130.0] {
            let mut shape = Shape::new(
                Geom::Rect {
                    origin: Pt::new(x, 20.0),
                    size: Pt::new(80.0, 100.0),
                    radius: 8.0,
                },
                Style::default(),
            );
            shape.rotation = 0.3;
            shape.style.fill = Fill::Linear {
                from: [0.0, 0.0],
                to: [1.0, 1.0],
                c0: Rgba::rgb(10, 20, 30),
                c1: Rgba::rgb(100, 110, 120),
            };
            studio.selection.push((0, shape.id));
            studio.doc.layers[0].kind.shapes_mut().unwrap().push(shape);
        }
        studio.history.clear();
        studio
    }
    fn shapes(studio: &Studio) -> Vec<Shape> {
        studio.doc.layers[0].kind.shapes().unwrap().to_vec()
    }
    fn drag(studio: &mut Studio, mode: Mode) {
        studio.begin_deform(mode);
        let start = studio.deformation.as_ref().unwrap().cage.handles()[0];
        studio.deformation_drag_start(0, start);
        studio.deformation_drag_to(start + Pt::new(-30.0, -12.0), false);
    }

    #[test]
    fn multi_object_drag_is_one_undo_and_restores_rotation_style_and_parameters() {
        for mode in Mode::ALL {
            let mut studio = studio();
            let before = shapes(&studio);
            drag(&mut studio, mode);
            let after = shapes(&studio);
            assert_ne!(before, after);
            assert!(
                after
                    .iter()
                    .all(|shape| shape.rotation == 0.0 && matches!(shape.geom, Geom::Poly { .. }))
            );
            studio.deformation_drag_finish();
            assert_eq!(studio.history.len(), 1);
            studio.end_deform(false);
            studio.undo();
            assert_eq!(shapes(&studio), before);
            studio.redo();
            assert_eq!(shapes(&studio), after);
        }
    }

    #[test]
    fn cancellation_and_no_op_leave_original_objects_and_history_intact() {
        let mut studio = studio();
        let before = shapes(&studio);
        drag(&mut studio, Mode::Mesh);
        studio.end_deform(true);
        assert_eq!(shapes(&studio), before);
        assert_eq!(studio.history.len(), 0);
        studio.begin_deform(Mode::Distort);
        let start = studio.deformation.as_ref().unwrap().cage.handles()[0];
        studio.deformation_drag_start(0, start);
        studio.deformation_drag_to(start + Pt::new(-20.0, -10.0), false);
        studio.deformation_drag_to(start, false);
        studio.deformation_drag_finish();
        assert_eq!(shapes(&studio), before);
        assert_eq!(studio.history.len(), 0);
    }

    #[test]
    fn repeated_mesh_drags_use_original_samples_and_independent_undo_steps() {
        let mut studio = studio();
        drag(&mut studio, Mode::Mesh);
        studio.deformation_drag_finish();
        let first = shapes(&studio);
        let first_points: Vec<_> = first
            .iter()
            .map(|s| s.geom.contours(0).iter().map(Vec::len).sum::<usize>())
            .collect();
        let start = studio.deformation.as_ref().unwrap().cage.handles()[4];
        studio.deformation_drag_start(4, start);
        studio.deformation_drag_to(start + Pt::new(8.0, 16.0), false);
        studio.deformation_drag_finish();
        assert_eq!(studio.history.len(), 2);
        let second_points: Vec<_> = shapes(&studio)
            .iter()
            .map(|s| s.geom.contours(0).iter().map(Vec::len).sum::<usize>())
            .collect();
        assert_eq!(first_points, second_points);
        studio.end_deform(false);
        studio.undo();
        assert_eq!(shapes(&studio), first);
    }

    #[test]
    fn open_paths_stay_open_and_polygon_holes_keep_their_fill_rule() {
        let mut studio = studio();
        let shapes = studio.doc.layers[0].kind.shapes_mut().unwrap();
        shapes[0].geom = Geom::Line {
            a: Pt::new(10.0, 20.0),
            b: Pt::new(60.0, 80.0),
        };
        shapes[1].geom = Geom::Poly {
            contours: vec![
                vec![
                    Pt::new(100.0, 0.0),
                    Pt::new(200.0, 0.0),
                    Pt::new(200.0, 100.0),
                    Pt::new(100.0, 100.0),
                ],
                vec![
                    Pt::new(120.0, 20.0),
                    Pt::new(120.0, 80.0),
                    Pt::new(180.0, 80.0),
                    Pt::new(180.0, 20.0),
                ],
            ],
            winding: true,
        };
        drag(&mut studio, Mode::Mesh);
        let shapes = studio.doc.layers[0].kind.shapes().unwrap();
        assert!(!shapes[0].geom.is_closed());
        assert!(
            matches!(&shapes[1].geom, Geom::Poly { winding: true, contours } if contours.len() == 2)
        );
    }

    #[test]
    fn other_personas_cannot_enter_a_hidden_base_geometry_mode() {
        let mut studio = studio();
        let before = shapes(&studio);
        for persona in [Persona::Pixel, Persona::Photo, Persona::Motion] {
            studio.persona = persona;
            assert!(!studio.can_deform());
            studio.begin_deform(Mode::Mesh);
            assert!(studio.deformation.is_none());
            assert_eq!(shapes(&studio), before);
        }
    }

    #[test]
    fn tool_changes_undo_and_tab_switches_restore_an_unreleased_drag() {
        for leave in [0, 1, 2, 3] {
            let mut studio = studio();
            studio.show_welcome = false;
            let before = shapes(&studio);
            drag(&mut studio, Mode::Mesh);
            match leave {
                0 => studio.set_tool(Tool::Select),
                1 => studio.undo(),
                3 => studio.deselect_all(),
                _ => {
                    studio.new_tab();
                    studio.switch_tab(0);
                }
            }
            assert!(studio.deformation.is_none());
            assert_eq!(shapes(&studio), before);
            assert_eq!(studio.history.len(), 0);
        }
    }

    #[test]
    fn guide_snapping_and_its_in_flight_override_follow_the_handle_not_the_pointer() {
        let mut studio = studio();
        studio.snap = SnapSettings {
            enabled: true,
            grid: false,
            objects: false,
            artboards: false,
            guides: true,
            spacing: false,
            threshold: 6.0,
        };
        studio.begin_deform(Mode::Distort);
        let start = studio.deformation.as_ref().unwrap().cage.handles()[0];
        studio.doc.guides = vec![crate::document::Guide {
            vertical: true,
            pos: start.x - 20.0,
        }];
        let pointer = start + Pt::new(4.0, 4.0);
        studio.deformation_drag_start(0, pointer);
        studio.deformation_drag_to(pointer, false);
        assert_eq!(
            studio.deformation.as_ref().unwrap().cage.handles()[0],
            start
        );
        let moved = pointer - Pt::new(18.0, 10.0);
        studio.deformation_drag_to(moved, false);
        assert!(
            (studio.deformation.as_ref().unwrap().cage.handles()[0].x - (start.x - 20.0)).abs()
                < 0.001
        );
        studio.snap_override = true;
        studio.deformation_drag_to(moved, false);
        assert!(
            (studio.deformation.as_ref().unwrap().cage.handles()[0].x - (start.x - 18.0)).abs()
                < 0.001
        );
        studio.snap.enabled = false;
        studio.deformation_drag_to(moved, false);
        assert!(
            (studio.deformation.as_ref().unwrap().cage.handles()[0].x - (start.x - 20.0)).abs()
                < 0.001
        );
    }

    #[test]
    fn an_external_edit_is_never_replaced_by_stale_session_geometry() {
        let mut studio = studio();
        studio.begin_deform(Mode::Distort);
        studio.doc.layers[0].kind.shapes_mut().unwrap()[0].rotation = 1.0;
        let edited = shapes(&studio);
        studio.deformation_drag_start(0, Pt::ZERO);
        assert!(studio.deformation.is_none());
        assert_eq!(shapes(&studio), edited);
    }
}
