//! Native path-type actions share the document transaction/history machinery.
use super::*;
use crate::text_geometry::{TextOnPath, guide_path};
impl Studio {
    pub fn text_path_target(&self, point: Pt, slack: f32) -> Option<(usize, u64)> {
        self.doc
            .layers
            .iter()
            .enumerate()
            .rev()
            .filter(|(_, l)| l.visible && !l.locked)
            .find_map(|(li, l)| {
                l.kind
                    .shapes()?
                    .iter()
                    .rev()
                    .filter(|s| s.visible && !s.locked)
                    .find_map(|s| {
                        let path = guide_path(s)?;
                        let distance = path.nearest_distance(point);
                        ((path.point_and_tangent_at(distance).0 - point).length() <= slack)
                            .then_some((li, s.id))
                    })
            })
    }
    pub fn place_text_on_path(&mut self, at: Pt, guide: (usize, u64)) {
        self.place_text(at);
        let Some(&(layer, id)) = self.selection.first() else {
            return;
        };
        let Some(shape) = self.doc.find_shape(guide.0, guide.1) else {
            return;
        };
        let Some(path) = guide_path(shape) else {
            return;
        };
        let start = path.nearest_distance(at) / path.length.max(0.001);
        if let Some(shape) = self.doc.find_shape_mut(layer, id)
            && let Geom::Text(run) = &mut shape.geom
        {
            run.on_path = Some(TextOnPath {
                path_id: guide.1,
                start,
                end: if path.closed { start + 1. } else { 1. },
                cache: Some(path),
                ..Default::default()
            });
            run.wrap_width = None;
            run.layout = None;
        }
        self.mark();
        self.status = "type on path — edit the text or drag its brackets".into();
    }
    pub fn can_attach_text_path(&self) -> bool {
        self.selection.len() == 2
            && self
                .selection
                .iter()
                .filter_map(|&(l, id)| self.doc.find_shape(l, id))
                .filter(|s| matches!(s.geom, Geom::Text(_)))
                .count()
                == 1
            && self
                .selection
                .iter()
                .filter_map(|&(l, id)| self.doc.find_shape(l, id))
                .any(|s| guide_path(s).is_some())
    }
    pub fn attach_text_path(&mut self) {
        self.commit_type_edit();
        if !self.can_attach_text_path() {
            return;
        }
        let selected = self.selection.clone();
        if let Some(hit) = selected.iter().copied().find(|&(layer, id)| {
            self.doc
                .find_shape(layer, id)
                .is_some_and(|s| matches!(&s.geom,Geom::Text(t) if t.frame.is_some()))
        }) {
            self.selection = vec![hit];
            self.convert_text_frame(false);
            self.selection = selected;
        }
        let Some((layer, id, before, rotation)) = self.selection.iter().find_map(|&(l, id)| {
            let s = self.doc.find_shape(l, id)?;
            matches!(s.geom, Geom::Text(_)).then_some((l, id, s.geom.clone(), s.rotation))
        }) else {
            return;
        };
        let Some((guide_id, path)) = self
            .selection
            .iter()
            .filter_map(|&(l, id)| self.doc.find_shape(l, id))
            .find_map(|s| guide_path(s).map(|path| (s.id, path)))
        else {
            return;
        };
        let mut after = before.clone();
        if let Geom::Text(run) = &mut after {
            run.on_path = Some(TextOnPath {
                path_id: guide_id,
                cache: Some(path),
                ..Default::default()
            });
            run.wrap_width = None;
            run.layout = None;
        }
        self.commit(Cmd::SetGeom {
            layer,
            id,
            before,
            after,
            rot_before: rotation,
            rot_after: 0.,
        });
        self.selection = vec![(layer, id)];
        self.status = "text attached to path".into();
    }
    pub fn release_text_path(&mut self) {
        self.commit_type_edit();
        self.patch_type(|run| crate::text_geometry::release(run));
        self.status = "released as editable point text".into();
    }
}

impl Studio {
    pub fn place_area_text(&mut self, bounds: Bounds) {
        self.place_text(Pt::new(bounds.min.x, bounds.min.y + self.text_px * 0.85));
        if let Some(run) = self.live_type_mut() {
            run.frame = Some(crate::text_geometry::TextFrame {
                size: Pt::new(bounds.width().max(1.), bounds.height().max(1.)),
                ..Default::default()
            });
            run.wrap_width = Some(bounds.width().max(1.));
            run.on_path = None;
            run.layout = None;
        }
        self.mark();
        self.status = "area text — resize the frame to reflow".into();
    }
    pub fn convert_text_frame(&mut self, area: bool) {
        self.commit_type_edit();
        if !area && let Some(hit) = self.primary() {
            let previous = self.doc.find_shape(hit.0, hit.1).and_then(|s| {
                if let Geom::Text(t) = &s.geom {
                    t.thread.as_ref().and_then(|t| t.prev)
                } else {
                    None
                }
            });
            if let Some(previous) = previous
                && let Some(layer) = self.doc.layers.iter().position(|l| {
                    l.kind
                        .shapes()
                        .unwrap_or(&[])
                        .iter()
                        .any(|s| s.id == previous)
                })
            {
                self.break_text_thread((layer, previous));
            }
            self.break_text_thread(hit);
            self.selection = vec![hit];
        }
        self.patch_frame_geometry(|run| {
            if area {
                let bounds = Geom::Text(run.clone()).bbox();
                crate::text_geometry::release(run);
                run.frame = Some(crate::text_geometry::TextFrame {
                    size: Pt::new(
                        bounds.width().max(80.),
                        bounds.height().max(run.line_height()),
                    ),
                    ..Default::default()
                });
            } else {
                run.frame = None;
                run.thread = None;
                run.wrap_width = None;
            }
            run.layout = None;
        });
        self.mark();
    }
    pub fn thread_text_frames(
        &mut self,
        from: (usize, u64),
        to: (usize, u64),
    ) -> Result<(), String> {
        self.commit_type_edit();
        if from == to {
            return Err("A frame cannot flow into itself".into());
        }
        let Some(source) = self.doc.find_shape(from.0, from.1).cloned() else {
            return Err("Source frame missing".into());
        };
        let Some(target) = self.doc.find_shape(to.0, to.1).cloned() else {
            return Err("Target frame missing".into());
        };
        let (Geom::Text(a), Geom::Text(b)) = (&source.geom, &target.geom) else {
            return Err("Select two area text frames".into());
        };
        if a.frame.is_none() || b.frame.is_none() {
            return Err("Threads need area text frames".into());
        }
        if a.thread.as_ref().and_then(|t| t.next).is_some() {
            return Err("Break the outgoing thread first".into());
        }
        if b.thread.as_ref().and_then(|t| t.prev).is_some() {
            return Err("The target already has an incoming thread".into());
        }
        let story = a.thread.as_ref().map_or(source.id, |t| t.story);
        if b.thread.as_ref().is_some_and(|t| t.story == story) {
            return Err("A story cannot loop back into itself".into());
        }
        let target_story = b.thread.as_ref().map_or(target.id, |t| t.story);
        let mut changes = vec![];
        for (layer, l) in self.doc.layers.iter().enumerate() {
            for shape in l.kind.shapes().unwrap_or(&[]) {
                let Geom::Text(run) = &shape.geom else {
                    continue;
                };
                let mut after = run.clone();
                let mut changed = false;
                if shape.id == story {
                    if !b.content.is_empty() {
                        if !after.content.ends_with('\n') {
                            after.content.push('\n');
                        }
                        let offset = after.content.chars().count();
                        after.content.push_str(&b.content);
                        after.manual_kern.extend(
                            b.manual_kern
                                .iter()
                                .map(|(&index, &value)| (index + offset, value)),
                        );
                        after
                            .paragraphs
                            .extend(b.paragraphs.iter().cloned().map(|mut p| {
                                p.start += offset;
                                p
                            }));
                        after.spans.extend(b.spans.iter().cloned().map(|mut s| {
                            s.start += offset;
                            s.end += offset;
                            s
                        }));
                    }
                    changed = true;
                }
                if shape.id == source.id {
                    let t = after
                        .thread
                        .get_or_insert(crate::text_geometry::TextThread {
                            story,
                            prev: None,
                            next: None,
                        });
                    t.next = Some(target.id);
                    let frame = after.frame.as_mut().unwrap();
                    if frame.overflow != crate::text_geometry::Overflow::Flow {
                        frame.terminal_overflow =
                            if frame.overflow == crate::text_geometry::Overflow::Ellipsis {
                                frame.overflow
                            } else {
                                crate::text_geometry::Overflow::Clip
                            };
                    }
                    frame.overflow = crate::text_geometry::Overflow::Flow;
                    changed = true;
                }
                if shape.id == target.id
                    || run.thread.as_ref().is_some_and(|t| t.story == target_story)
                {
                    let t = after
                        .thread
                        .get_or_insert(crate::text_geometry::TextThread {
                            story,
                            prev: None,
                            next: None,
                        });
                    t.story = story;
                    if shape.id == target.id {
                        t.prev = Some(source.id);
                    }
                    after.content.clear();
                    changed = true;
                }
                if changed {
                    after.layout = None;
                    changes.push(Cmd::SetGeom {
                        layer,
                        id: shape.id,
                        before: shape.geom.clone(),
                        after: Geom::Text(after),
                        rot_before: shape.rotation,
                        rot_after: shape.rotation,
                    });
                }
            }
        }
        self.commit(Cmd::Batch(changes));
        self.selection = vec![to];
        self.status = "text frames threaded into one story".into();
        Ok(())
    }
    pub fn break_text_thread(&mut self, at: (usize, u64)) {
        self.commit_type_edit();
        let Some(shape) = self.doc.find_shape(at.0, at.1) else {
            return;
        };
        let Geom::Text(run) = &shape.geom else { return };
        let Some(next) = run.thread.as_ref().and_then(|t| t.next) else {
            return;
        };
        let story = run.thread.as_ref().unwrap().story;
        let split = run.layout.as_ref().map_or(0, |l| l.visible_end);
        let Some((_, head)) = self.doc.layers.iter().enumerate().find_map(|(li, l)| {
            l.kind
                .shapes()
                .unwrap_or(&[])
                .iter()
                .find(|s| s.id == story)
                .map(|s| (li, s.clone()))
        }) else {
            return;
        };
        let Geom::Text(head_run) = head.geom else {
            return;
        };
        let tail = crate::text_geometry::story_slice(&head_run, split);
        let mut after_next = false;
        let mut tail_ids = HashSet::new();
        let mut cursor = Some(next);
        while let Some(id) = cursor {
            if !tail_ids.insert(id) {
                break;
            }
            cursor = self
                .doc
                .layers
                .iter()
                .filter_map(|l| l.kind.shapes())
                .flatten()
                .find(|s| s.id == id)
                .and_then(|s| {
                    if let Geom::Text(t) = &s.geom {
                        t.thread.as_ref().and_then(|t| t.next)
                    } else {
                        None
                    }
                });
        }
        let mut changes = vec![];
        for (layer, l) in self.doc.layers.iter().enumerate() {
            for shape in l.kind.shapes().unwrap_or(&[]) {
                let Geom::Text(run) = &shape.geom else {
                    continue;
                };
                if shape.id != story && shape.id != at.1 && !tail_ids.contains(&shape.id) {
                    continue;
                }
                let mut after = run.clone();
                if shape.id == story {
                    after.content = after.content.chars().take(split).collect();
                    after.spans.retain(|s| s.start < split);
                    for s in &mut after.spans {
                        s.end = s.end.min(split);
                    }
                    after.paragraphs.retain(|p| p.start < split);
                    after.manual_kern.retain(|&index, _| index < split);
                }
                if shape.id == at.1 {
                    if let Some(t) = &mut after.thread {
                        t.next = None;
                    }
                    if let Some(f) = &mut after.frame {
                        f.overflow = f.terminal_overflow;
                    }
                }
                if tail_ids.contains(&shape.id) {
                    if let Some(t) = &mut after.thread {
                        t.story = next;
                        if shape.id == next {
                            t.prev = None;
                        }
                    }
                    if shape.id == next {
                        let top = after.origin.y-after.px*0.85;
                        let origin = after.origin;
                        let frame = after.frame.clone();
                        let thread = after.thread.clone();
                        after = tail.clone();
                        after.origin = Pt::new(origin.x,top+after.px*0.85);
                        after.frame = frame;
                        after.thread = thread;
                    }
                }
                after.layout = None;
                changes.push(Cmd::SetGeom {
                    layer,
                    id: shape.id,
                    before: shape.geom.clone(),
                    after: Geom::Text(after),
                    rot_before: shape.rotation,
                    rot_after: shape.rotation,
                });
                after_next = true;
            }
        }
        if after_next {
            self.commit(Cmd::Batch(changes));
            self.status = "thread broken; each story retains its text".into();
        }
    }
    pub fn story_head(&self, hit: (usize, u64)) -> (usize, u64) {
        let Some(shape) = self.doc.find_shape(hit.0, hit.1) else {
            return hit;
        };
        let Geom::Text(run) = &shape.geom else {
            return hit;
        };
        let Some(story) = run.thread.as_ref().map(|t| t.story) else {
            return hit;
        };
        self.doc
            .layers
            .iter()
            .enumerate()
            .find_map(|(l, layer)| {
                layer
                    .kind
                    .shapes()
                    .unwrap_or(&[])
                    .iter()
                    .any(|s| s.id == story)
                    .then_some((l, story))
            })
            .unwrap_or(hit)
    }
}

impl Studio {
    pub fn patch_frame_geometry(&mut self, f: impl FnOnce(&mut TypeRun)) {
        self.commit_type_edit();
        let Some((layer, id)) = self.primary() else {
            return;
        };
        let Some(shape) = self.doc.find_shape(layer, id) else {
            return;
        };
        let Geom::Text(run) = &shape.geom else { return };
        let mut after = run.clone();
        f(&mut after);
        after.layout = None;
        self.commit(Cmd::SetGeom {
            layer,
            id,
            before: shape.geom.clone(),
            after: Geom::Text(after),
            rot_before: shape.rotation,
            rot_after: shape.rotation,
        });
    }
}

impl Studio {
    pub fn empty_thread_frame(&mut self, source: (usize, u64), point: Pt) -> Option<(usize, u64)> {
        self.commit_type_edit();
        let original = self.doc.find_shape(source.0, source.1)?.clone();
        let Geom::Text(mut run) = original.geom else {
            return None;
        };
        run.origin = Pt::new(point.x, point.y + run.px * 0.85);
        run.content.clear();
        run.thread = None;
        run.layout = None;
        run.contours.clear();
        if let Some(frame) = &mut run.frame {
            frame.overflow = frame.terminal_overflow;
        }
        let shape = Shape::new(Geom::Text(run), original.style);
        let id = shape.id;
        self.commit(Cmd::AddShape {
            layer: source.0,
            shape,
        });
        self.selection = vec![(source.0, id)];
        Some((source.0, id))
    }
}

impl Studio {
    pub fn can_text_inside_shape(&self) -> bool {
        self.primary()
            .and_then(|(layer, id)| self.doc.find_shape(layer, id))
            .is_some_and(|shape| {
                crate::text_geometry::guide_path(shape).is_some_and(|path| path.closed)
            })
    }
    pub fn text_inside_shape(&mut self) {
        self.commit_type_edit();
        let Some((layer, id)) = self.primary() else {
            return;
        };
        let Some(shape) = self.doc.find_shape(layer, id).cloned() else {
            return;
        };
        if crate::text_geometry::guide_path(&shape).is_none_or(|path| !path.closed) {
            return;
        }
        let bounds = shape.world_bbox();
        if bounds.width() < 1. || bounds.height() < 1. {
            return;
        }
        let contours = shape
            .world_contours(128)
            .into_iter()
            .map(|poly| {
                poly.into_iter()
                    .map(|p| {
                        Pt::new(
                            (p.x - bounds.min.x) / bounds.width(),
                            (p.y - bounds.min.y) / bounds.height(),
                        )
                    })
                    .collect()
            })
            .collect();
        self.active_layer = Some(layer);
        self.place_area_text(bounds);
        if let Some(run) = self.live_type_mut() {
            run.frame.as_mut().unwrap().contour = Some(contours);
            run.layout = None;
        }
        self.mark();
        self.status = "area text inside shape — the frame remains editable".into();
    }
}

impl Studio {
    /// Explicit Transform dimensions scale a threaded story's source typography once.
    pub fn transform_shape_with_text_scale(&mut self, layer: usize, id: u64, destination: Bounds) {
        let Some(shape) = self.doc.find_shape(layer, id).cloned() else { return; };
        let bounds = shape.geom.bbox();
        let mut after = shape.geom.clone();
        after.map_into_with_text_scale(bounds, destination);
        let mut commands = vec![Cmd::SetGeom { layer, id, before: shape.geom, after, rot_before: shape.rotation, rot_after: shape.rotation }];
        let head = self.story_head((layer,id));
        if head != (layer,id) && let Some(source) = self.doc.find_shape(head.0,head.1) {
            let mut after = source.geom.clone();
            if let Geom::Text(run) = &mut after {
                crate::text_geometry::scale_typography(run,destination.width()/bounds.width().max(0.001),destination.height()/bounds.height().max(0.001));
                commands.push(Cmd::SetGeom { layer:head.0,id:head.1,before:source.geom.clone(),after,rot_before:source.rotation,rot_after:source.rotation });
            }
        }
        self.commit(Cmd::Batch(commands));
    }
}
