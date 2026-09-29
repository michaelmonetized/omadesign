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
        let head = self.doc.layers.iter().filter_map(|l| l.kind.shapes()).flatten()
            .find(|s| s.id == story).ok_or("Source story missing")?;
        let Geom::Text(head_run) = &head.geom else { return Err("Source story missing".into()); };
        // Fonts, sizes and paint belong to the whole run/shape. Range attributes
        // can preserve spacing and features, but cannot encode these differences.
        // Validate before ending an edit session or adding anything to history.
        if !b.content.is_empty() && (head_run.font != b.font || head_run.px != b.px
            || head.style != target.style || head.opacity != target.opacity
            || head.fill_opacity != target.fill_opacity || head.blend != target.blend
            || head.blend_interior != target.blend_interior || head.filters != target.filters)
        {
            return Err("Cannot merge populated frames with different fonts, sizes or appearance. Match those settings, or thread into an empty frame.".into());
        }
        let target_story = b.thread.as_ref().map_or(target.id, |t| t.story);
        self.commit_type_edit();
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
                        append_story(&mut after, b);
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

/// Preserve effective inherited attributes before moving text under another run.
/// Font/size/paint compatibility is checked before this helper is called.
fn append_story(head: &mut crate::geom::TypeRun, target: &crate::geom::TypeRun) {
    use std::collections::BTreeSet;
    if !head.content.is_empty() && !head.content.ends_with('\n') { head.content.push('\n'); }
    let offset = head.content.chars().count();
    let tags: BTreeSet<_> = [*b"kern", *b"liga", *b"clig", *b"tnum", *b"smcp", *b"c2sc", *b"calt"]
        .into_iter().chain(head.features.iter().map(|(tag,_)| *tag))
        .chain(target.features.iter().map(|(tag,_)| *tag))
        .chain(head.spans.iter().chain(&target.spans).flat_map(|span| span.features.iter().map(|(tag,_)| *tag)))
        .collect();
    let spans = target.character_style_runs(0, target.content.chars().count()).into_iter().map(|span| {
        let mut effective = crate::text::character_metrics(target, span.start);
        effective.features = tags.iter().map(|tag| {
            let value = if tag == b"kern" && !span.features.iter().any(|(t,_)| t == tag) && span.kerning.is_some() {
                u32::from(span.kerning == Some(crate::geom::KernMode::Metrics))
            } else { crate::text::feature_value(target, span.start, *tag) };
            (*tag, value)
        }).collect();
        effective.start = offset + span.start;
        effective.end = offset + span.end;
        effective
    });
    head.spans.extend(spans);
    head.manual_kern.extend(target.manual_kern.iter().map(|(&i,&value)| (offset+i,value)));
    head.paragraphs.retain(|p| p.start < offset);
    head.paragraphs.extend(std::iter::once(0).chain(target.content.chars().enumerate()
        .filter_map(|(i,c)| (c=='\n').then_some(i+1))).map(|start| {
            let mut style = crate::text::paragraph_style(target,start);
            style.start = offset + start;
            style
        }));
    head.content.push_str(&target.content);
}

#[cfg(test)]
mod readiness_tests {
    use super::*;
    use crate::geom::{CharSpan, KernMode, Leading, ParagraphStyle, TextAlign, TypeRun};
    use crate::text_geometry::TextFrame;

    fn fixture(a: TypeRun, b: TypeRun) -> (Studio, (usize,u64), (usize,u64)) {
        let mut studio = Studio::new();
        let mut ids = Vec::new();
        for (index, mut run) in [a,b].into_iter().enumerate() {
            run.origin = Pt::new(40. + index as f32 * 330., 60.);
            run.frame = Some(TextFrame { size: Pt::new(280., 100.), ..Default::default() });
            let shape = Shape::new(Geom::Text(run), Style::default());
            ids.push((1,shape.id));
            studio.doc.layers[1].kind.shapes_mut().unwrap().push(shape);
        }
        studio.active_layer = Some(1);studio.mark();studio.history.clear();
        (studio, ids[0], ids[1])
    }
    fn run(content: &str) -> TypeRun {
        TypeRun {content: content.into(), px: 22., font: concat!(env!("CARGO_MANIFEST_DIR"),"/tests/assets/fonts/EBGaramond.ttf").into(), ..Default::default()}
    }
    fn text(studio: &Studio, hit: (usize,u64)) -> &TypeRun {
        let Geom::Text(run) = &studio.doc.find_shape(hit.0,hit.1).unwrap().geom else { panic!() };run
    }
    #[test]
    fn populated_merge_preserves_inherited_and_explicit_styles() {
        let mut a = run("Source paragraph");a.features=vec![(*b"dlig",1),(*b"ss01",1)];a.tracking=4.;a.leading=39.;a.align=TextAlign::Center;
        let mut b = run("office 123\nTarget defaults\nEnd");b.liga=false;b.tnum=true;b.kern=false;b.tracking=1.5;b.leading=31.;b.align=TextAlign::End;
        b.features=vec![(*b"smcp",1)];
        b.spans=vec![CharSpan{start:0,end:6,tracking:Some(75.),kerning:Some(KernMode::Optical),leading:Some(Leading::Auto(170.)),baseline_shift:Some(3.),features:vec![(*b"liga",1)],no_break:true,..Default::default()}];
        b.paragraphs=vec![ParagraphStyle{start:11,align:TextAlign::Start,word_spacing:[70.,125.,160.],..Default::default()}];b.manual_kern.insert(2,-40.);
        let original_a=a.clone();let original_b=b.clone();let offset=a.content.chars().count()+1;
        let (mut studio,head,target)=fixture(a,b);
        let before=crate::project::encode(&studio.doc).unwrap();
        studio.thread_text_frames(head,target).unwrap();
        let merged=text(&studio,head);
        assert_eq!(merged.content,format!("{}\n{}",original_a.content,original_b.content));
        assert!(text(&studio,target).content.is_empty());
        assert_eq!(merged.manual_kern.get(&(offset+2)),Some(&-40.));
        for index in 0..original_b.content.chars().count() {
            let old=crate::text::character_metrics(&original_b,index);let new=crate::text::character_metrics(merged,offset+index);
            assert_eq!((new.no_break,new.tracking,new.leading,new.baseline_shift,new.hscale,new.vscale,new.kerning),(old.no_break,old.tracking,old.leading,old.baseline_shift,old.hscale,old.vscale,old.kerning));
            for tag in [*b"liga",*b"clig",*b"tnum",*b"smcp",*b"dlig",*b"ss01"] {assert_eq!(crate::text::feature_value(merged,offset+index,tag),crate::text::feature_value(&original_b,index,tag));}
            let mut expected=crate::text::paragraph_style(&original_b,index);expected.start+=offset;assert_eq!(crate::text::paragraph_style(merged,offset+index),expected);
        }
        // Compare shaped glyph positions/features with the original target, not just serialized attributes.
        let mut original=original_b.clone();original.frame=None;original.wrap_width=Some(280.);
        let mut suffix=original.clone();
        suffix.content=original.content.clone();suffix.spans=merged.spans.iter().filter(|s|s.end>offset).cloned().map(|mut s|{s.start-=offset;s.end-=offset;s}).collect();
        suffix.features=merged.features.clone();suffix.kern=merged.kern;suffix.liga=merged.liga;suffix.tnum=merged.tnum;suffix.smcp=merged.smcp;suffix.tracking=merged.tracking;suffix.leading=merged.leading;suffix.align=merged.align;
        suffix.paragraphs=merged.paragraphs.iter().filter(|p|p.start>=offset).cloned().map(|mut p|{p.start-=offset;p}).collect();
        let old=crate::text::compose(&original);let new=crate::text::compose(&suffix);
        assert_eq!(old.len(),new.len());for (old,new) in old.iter().zip(new.iter()) {assert_eq!(old.glyphs.iter().map(|g|(g.id,g.cluster,g.x,g.y,g.advance)).collect::<Vec<_>>(),new.glyphs.iter().map(|g|(g.id,g.cluster,g.x,g.y,g.advance)).collect::<Vec<_>>());}
        let encoded=crate::project::encode(&studio.doc).unwrap();let reopened=crate::project::decode(&encoded).unwrap();assert_eq!(crate::project::encode(&reopened).unwrap(),encoded);
        assert_eq!(studio.history.len(),1);studio.undo();assert_eq!(crate::project::encode(&studio.doc).unwrap(),before);studio.redo();assert_eq!(crate::project::encode(&studio.doc).unwrap(),encoded);
    }
    #[test]
    fn populated_merge_rejects_unrepresentable_defaults_without_mutating_edit_or_history() {
        for mismatch in 0..3 {
            let (mut studio,head,target)=fixture(run("Source"),run("Target"));
            let shape=studio.doc.find_shape_mut(target.0,target.1).unwrap();
            match mismatch {0=>if let Geom::Text(t)=&mut shape.geom {t.font="different-font.ttf".into();},1=>if let Geom::Text(t)=&mut shape.geom {t.px=31.;},_=>shape.style.fill=crate::document::Fill::Solid(crate::color::Rgba::from_hex(0xff0033))}
            studio.begin_type_edit(head,Pt::new(50.,60.));studio.type_insert("Editing");
            let before=crate::project::encode(&studio.doc).unwrap();let history=studio.history.len();let caret=studio.type_edit.as_ref().unwrap().caret;
            let error=studio.thread_text_frames(head,target).unwrap_err();assert!(error.contains("empty frame"));
            assert_eq!(crate::project::encode(&studio.doc).unwrap(),before);assert_eq!(studio.history.len(),history);assert_eq!(studio.type_edit.as_ref().unwrap().caret,caret);
        }
    }
    #[test]
    fn empty_frames_thread_and_empty_head_does_not_invent_a_paragraph() {
        let (mut studio,head,target)=fixture(run(""),run("Target"));studio.thread_text_frames(head,target).unwrap();assert_eq!(text(&studio,head).content,"Target");
        let mut empty=run("");empty.px=90.;empty.font="another-font".into();let (mut studio,head,target)=fixture(run("Source"),empty);studio.thread_text_frames(head,target).unwrap();assert_eq!(text(&studio,head).content,"Source");
    }
    #[test]
    fn follower_source_validation_uses_the_actual_head_defaults() {
        let (mut studio,head,follower)=fixture(run("Source"),run(""));studio.thread_text_frames(head,follower).unwrap();
        let target=studio.empty_thread_frame(follower,Pt::new(40.,240.)).unwrap();
        if let Geom::Text(t)=&mut studio.doc.find_shape_mut(follower.0,follower.1).unwrap().geom {t.font="stale-follower-default".into();t.px=99.;}
        if let Geom::Text(t)=&mut studio.doc.find_shape_mut(target.0,target.1).unwrap().geom {t.content="Target".into();}
        studio.thread_text_frames(follower,target).unwrap();assert_eq!(text(&studio,head).content,"Source\nTarget");
    }
    #[test]
    fn follower_pointer_selection_stays_in_active_frame_and_undo_restores_story() {
        for (rotation,parent_rotation) in [(0.,0.),(0.63,0.),(0.37,0.48)] {
            let original="Every word remains in the story when editing a following frame. ".repeat(4);
            let (mut studio,head,follower)=fixture(run(&original),run(""));studio.thread_text_frames(head,follower).unwrap();
            studio.doc.find_shape_mut(follower.0,follower.1).unwrap().rotation=rotation;
            let parent = if parent_rotation != 0. {
                let mut outer = Shape::new(Geom::Rect{origin:Pt::new(320.,10.),size:Pt::new(390.,220.),radius:0.},Style::default());
                outer.layout.frame=true;outer.rotation=parent_rotation;let id=outer.id;
                studio.doc.find_shape_mut(follower.0,follower.1).unwrap().layout.parent=Some(id);
                studio.doc.layers[1].kind.shapes_mut().unwrap().push(outer);Some(id)
            } else {None};studio.mark();
            let shape=studio.doc.find_shape(follower.0,follower.1).unwrap();let Geom::Text(t)=&shape.geom else {panic!()};let layout=t.layout.as_ref().unwrap();let start=layout.visible_start+2;let end=start+5;
            let point=|index| {let point=shape.world_point(crate::text::caret_pt(t,index));parent.map_or(point,|id|studio.doc.find_shape(1,id).unwrap().world_point(point))};let (a,b)=(point(start),point(end));
            studio.begin_type_edit(follower,a);assert_eq!(studio.type_edit.as_ref().unwrap().caret,start);assert_eq!(studio.type_edit.as_ref().unwrap().frame,follower);
            assert!(studio.type_pointer_caret(follower,b,true));assert_eq!((studio.type_edit.as_ref().unwrap().anchor,studio.type_edit.as_ref().unwrap().caret),(start,end));
            studio.begin_type_edit(follower,b);assert_eq!((studio.type_edit.as_ref().unwrap().anchor,studio.type_edit.as_ref().unwrap().caret),(end,end));
            assert!(studio.type_pointer_caret(follower,a,true));assert_eq!((studio.type_edit.as_ref().unwrap().anchor,studio.type_edit.as_ref().unwrap().caret),(end,start));
            studio.type_insert("EDITED");studio.commit_type_edit();let expected=format!("{}EDITED{}",original.chars().take(start).collect::<String>(),original.chars().skip(end).collect::<String>());assert_eq!(text(&studio,head).content,expected);assert!(text(&studio,follower).content.is_empty());
            studio.undo();assert_eq!(text(&studio,head).content,original);
        }
    }
}
