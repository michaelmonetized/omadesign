use super::*;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ArtworkResize {
    Keep,
    Move,
    #[default]
    Scale,
}
#[derive(Clone, Copy, Debug, Default)]
pub struct ArtboardOptions {
    pub resize: ArtworkResize,
    pub clone: bool,
}
impl ArtboardOptions {
    pub fn with_modifiers(self, ctrl: bool, alt: bool) -> Self {
        Self {
            resize: if ctrl {
                ArtworkResize::Keep
            } else {
                self.resize
            },
            clone: self.clone ^ alt,
        }
    }
}

impl Studio {
    pub(crate) fn apply_artboard_resize(
        &mut self,
        orig: &Artboard,
        next: &Artboard,
        snaps: &[ObjSnap],
        behavior: ArtworkResize,
    ) {
        match behavior {
            ArtworkResize::Keep => self.restore_snaps(snaps),
            ArtworkResize::Scale => self.apply_artboard_contents(orig, next, snaps),
            ArtworkResize::Move => {
                self.restore_snaps(snaps);
                let delta = next.origin - orig.origin;
                for snap in snaps {
                    if snap.id == RASTER_ID {
                        if let Some(layer) = self.doc.layers.get_mut(snap.layer) {
                            layer
                                .kind
                                .set_raster_xform(snap.origin + delta, snap.size, snap.rot);
                        }
                    } else if let Some(shape) = self.doc.find_shape_mut(snap.layer, snap.id) {
                        shape.geom.translate(delta);
                    }
                }
            }
        }
    }

    /// Copies are provisional until the resize gesture commits. IDs and native
    /// frame/text/motion references are remapped as one undoable transaction.
    pub(crate) fn copy_artboard_contents(&mut self, snaps: &[ObjSnap]) -> (Vec<ObjSnap>, Vec<Cmd>) {
        let ids: HashMap<_, _> = snaps
            .iter()
            .map(|s| {
                let old = if s.id == RASTER_ID {
                    self.doc.layers[s.layer].id
                } else {
                    s.id
                };
                (old, crate::document::next_id())
            })
            .collect();
        let mut detached = HashMap::new();
        for snap in snaps.iter().filter(|s| s.id != RASTER_ID) {
            let Some(root) = self.doc.find_shape(snap.layer, snap.id) else { continue; };
            if !root.layout.parent.is_some_and(|parent| !ids.contains_key(&parent)) { continue; }
            let center = root.geom.bbox().center();
            let mut world = center;
            let mut rotation = 0.0;
            let mut parent = root.layout.parent;
            let mut seen = HashSet::new();
            while let Some(id) = parent {
                if !seen.insert(id) { break; }
                let Some(frame) = self.doc.find_shape(snap.layer, id) else { break; };
                world = world.rotate_about(frame.geom.bbox().center(), frame.rotation);
                rotation += frame.rotation;
                parent = frame.layout.parent;
            }
            let delta = world - center;
            detached.insert(root.id, (delta, rotation));
            for id in crate::layout::descendants(&self.doc, snap.layer, root.id) {
                detached.insert(id, (delta, 0.0));
            }
        }
        let mut commands = Vec::new();
        let mut copies = Vec::new();
        let mut added_layers = 0;
        for snap in snaps {
            let mut copied = snap.clone();
            if snap.id == RASTER_ID {
                let mut layer = self.doc.layers[snap.layer].clone();
                layer.id = ids[&layer.id];
                layer.name = format!("{} copy", layer.name);
                copied.layer = self.doc.layers.len() + added_layers;
                added_layers += 1;
                commands.push(Cmd::AddLayer {
                    index: copied.layer,
                    layer,
                });
            } else if let Some(source) = self.doc.find_shape(snap.layer, snap.id) {
                let mut shape = source.clone();
                shape.id = ids[&shape.id];
                copied.id = shape.id;
                crate::layout_components::remap_duplicate(&mut shape, &ids);
                crate::text_geometry::remap_copy_in_document(&self.doc, &mut shape, &ids);
                shape.layout.parent = source.layout.parent.and_then(|parent| ids.get(&parent).copied());
                if let Some((delta, rotation)) = detached.get(&source.id) {
                    shape.geom.translate(*delta);
                    shape.rotation += rotation;
                    copied.geom = Some(shape.geom.clone());
                    copied.rot = shape.rotation;
                }
                commands.push(Cmd::AddShape {
                    layer: snap.layer,
                    shape,
                });
            }
            copies.push(copied);
        }
        let mut motion = self.doc.motion.clone();
        for track in &self.doc.motion.tracks {
            if let Some(&id) = ids.get(&track.shape) {
                let mut copy = track.clone();
                copy.shape = id;
                motion.tracks.push(copy);
            }
        }
        if motion != self.doc.motion {
            commands.push(Cmd::SetMotion {
                before: self.doc.motion.clone(),
                after: motion,
            });
        }
        for cmd in &commands {
            crate::document::apply(&mut self.doc, cmd);
        }
        (copies, commands)
    }

    pub(crate) fn prepare_artboard_resize_copies(&mut self) {
        let Some(Op::ArtboardResize {
            contents,
            options,
            copies,
            ..
        }) = &self.op
        else {
            return;
        };
        if !options.clone || options.resize == ArtworkResize::Keep || copies.is_some() {
            return;
        }
        let snapshots = contents.clone();
        let (new_contents, commands) = self.copy_artboard_contents(&snapshots);
        if let Some(Op::ArtboardResize {
            contents, copies, ..
        }) = &mut self.op
        {
            *contents = new_contents;
            *copies = Some(commands);
        }
    }
    pub(crate) fn discard_artboard_copies(&mut self, commands: Vec<Cmd>) {
        for command in commands.into_iter().rev() {
            crate::document::apply(&mut self.doc, &crate::document::invert_cmd(command));
        }
    }
    pub(crate) fn cancel_artboard_gesture(&mut self) -> bool {
        if !matches!(
            self.op,
            Some(
                Op::ArtboardMove { .. }
                    | Op::ArtboardResize { .. }
                    | Op::ArtboardRotate { .. }
                    | Op::ArtboardDraw { .. }
            )
        ) {
            return false;
        }
        match self.op.take().unwrap() {
            Op::ArtboardMove { orig, contents, .. } => {
                self.doc.artboards = orig;
                self.restore_snaps(&contents);
            }
            Op::ArtboardResize {
                orig,
                contents,
                copies,
                ..
            } => {
                self.restore_snaps(&contents);
                if let Some(copies) = copies {
                    self.discard_artboard_copies(copies);
                }
                if let Some(board) = self.doc.artboards.iter_mut().find(|b| b.id == orig.id) {
                    *board = orig;
                }
            }
            Op::ArtboardRotate { orig, contents, .. } => {
                self.restore_snaps(&contents);
                if let Some(board) = self.doc.artboards.iter_mut().find(|b| b.id == orig.id) {
                    *board = orig;
                }
            }
            _ => {}
        }
        self.mark();
        true
    }

    pub(crate) fn preview_inspector_artboard(&mut self, next: Artboard) {
        if !matches!(&self.op, Some(Op::ArtboardResize { handle: usize::MAX, orig, .. }) if orig.id == next.id) {
            self.cancel_artboard_gesture();
            let Some(orig) = self.doc.artboards.iter().find(|a| a.id == next.id).cloned() else { return; };
            self.op = Some(Op::ArtboardResize {
                options: self.artboard_options, copies: None,
                contents: self.snapshot_artboard_contents(&orig),
                handle: usize::MAX, start_box: orig.local_bounds(), orig,
            });
        }
        self.prepare_artboard_resize_copies();
        let Some(Op::ArtboardResize { orig, contents, options, .. }) = &self.op else { return; };
        let (orig, contents, options) = (orig.clone(), contents.clone(), *options);
        self.apply_artboard_resize(&orig, &next, &contents, options.resize);
        if let Some(board) = self.doc.artboards.iter_mut().find(|a| a.id == next.id) { *board = next; }
        self.mark();
    }

    pub(crate) fn finish_inspector_artboard(&mut self) {
        if !matches!(self.op, Some(Op::ArtboardResize { handle: usize::MAX, .. })) { return; }
        let Some(Op::ArtboardResize { orig, contents, copies, .. }) = self.op.take() else { return; };
        let after = self.doc.artboards.clone();
        let changed = after.iter().find(|a| a.id == orig.id) != Some(&orig);
        let mut commands = copies.clone().unwrap_or_default();
        if changed { commands.extend(self.transform_commands(&contents)); }
        self.restore_snaps(&contents);
        self.discard_artboard_copies(copies.unwrap_or_default());
        if let Some(board) = self.doc.artboards.iter_mut().find(|a| a.id == orig.id) { *board = orig; }
        if changed {
            commands.push(Cmd::SetArtboards { before: self.doc.artboards.clone(), after });
            self.commit(Cmd::Batch(commands));
        }
    }

    pub(crate) fn change_artboard(
        &mut self,
        original: Artboard,
        next: Artboard,
        options: ArtboardOptions,
    ) {
        let mut snaps = self.snapshot_artboard_contents(&original);
        let mut commands = Vec::new();
        if options.clone && options.resize != ArtworkResize::Keep {
            (snaps, commands) = self.copy_artboard_contents(&snaps);
        }
        self.apply_artboard_resize(&original, &next, &snaps, options.resize);
        commands.extend(self.transform_commands(&snaps));
        let before = self.doc.artboards.clone();
        let mut after = before.clone();
        if let Some(board) = after.iter_mut().find(|b| b.id == original.id) {
            *board = next;
        }
        commands.push(Cmd::SetArtboards { before, after });
        // Preview changes are already applied; restore before the native commit.
        self.restore_snaps(&snaps);
        let copies: Vec<_> = commands
            .iter()
            .filter(|c| {
                matches!(
                    c,
                    Cmd::AddShape { .. } | Cmd::AddLayer { .. } | Cmd::SetMotion { .. }
                )
            })
            .cloned()
            .collect();
        self.discard_artboard_copies(copies);
        self.commit(Cmd::Batch(commands));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> (Studio, Artboard, u64, usize) {
        let mut s = Studio::new();
        s.doc.layers = vec![Layer::vector("art")];
        let shape = Shape::new(
            Geom::Rect {
                origin: Pt::new(20., 30.),
                size: Pt::new(40., 20.),
                radius: 0.,
            },
            Default::default(),
        );
        let id = shape.id;
        s.doc.layers[0].kind.shapes_mut().unwrap().push(shape);
        let mut layer = Layer::vector("image");
        layer.kind = LayerKind::Raster {
            pixels: crate::document::Pixels::new(2, 2),
            origin: Pt::new(30., 40.),
            size: Pt::new(20., 10.),
            rotation: 0.,
            shear: 0.,
        };
        s.doc.layers.push(layer);
        let board = Artboard::new(0, Pt::ZERO, Pt::new(100., 100.));
        s.doc.artboards = vec![board.clone()];
        (s, board, id, 1)
    }
    #[test]
    fn artboard_resize_controls_preserve_move_or_scale_vectors_and_rasters_atomically() {
        for behavior in [
            ArtworkResize::Keep,
            ArtworkResize::Move,
            ArtworkResize::Scale,
        ] {
            let (mut s, original, id, li) = fixture();
            let mut next = original.clone();
            next.origin = Pt::new(10., 20.);
            next.size = Pt::new(200., 200.);
            let before = crate::project::encode(&s.doc).unwrap();
            s.change_artboard(
                original,
                next,
                ArtboardOptions {
                    resize: behavior,
                    clone: false,
                },
            );
            let b = s.doc.find_shape(0, id).unwrap().world_bbox();
            let r = s.doc.layers[li].kind.raster_bounds().unwrap();
            match behavior {
                ArtworkResize::Keep => {
                    assert_eq!(b.min, Pt::new(20., 30.));
                    assert_eq!(r.min, Pt::new(30., 40.));
                }
                ArtworkResize::Move => {
                    assert_eq!(b.min, Pt::new(30., 50.));
                    assert_eq!(b.width(), 40.);
                    assert_eq!(r.min, Pt::new(40., 60.));
                    assert_eq!(r.width(), 20.);
                }
                ArtworkResize::Scale => {
                    assert_eq!(b.min, Pt::new(50., 80.));
                    assert_eq!(b.width(), 80.);
                    assert_eq!(r.min, Pt::new(70., 100.));
                    assert_eq!(r.width(), 40.);
                }
            }
            assert_eq!(s.history.len(), 1);
            let after = crate::project::encode(&s.doc).unwrap();
            s.undo();
            assert_eq!(crate::project::encode(&s.doc).unwrap(), before);
            s.redo();
            assert_eq!(crate::project::encode(&s.doc).unwrap(), after);
        }
    }
    #[test]
    fn cloned_artboard_resize_keeps_originals_and_undo_removes_all_copies() {
        let (mut s, original, id, li) = fixture();
        let before = crate::project::encode(&s.doc).unwrap();
        let mut next = original.clone();
        next.size = Pt::new(200., 200.);
        s.change_artboard(
            original,
            next,
            ArtboardOptions {
                resize: ArtworkResize::Scale,
                clone: true,
            },
        );
        assert_eq!(s.doc.find_shape(0, id).unwrap().world_bbox().width(), 40.);
        assert_eq!(s.doc.layers[li].kind.raster_bounds().unwrap().width(), 20.);
        let shapes = s.doc.layers[0].kind.shapes().unwrap();
        assert_eq!(shapes.len(), 2);
        assert_ne!(shapes[0].id, shapes[1].id);
        assert_eq!(shapes[1].world_bbox().width(), 80.);
        assert_eq!(s.doc.layers.len(), 3);
        assert_eq!(s.doc.layers[2].kind.raster_bounds().unwrap().width(), 40.);
        assert_eq!(s.history.len(), 1);
        s.undo();
        assert_eq!(crate::project::encode(&s.doc).unwrap(), before);
        s.redo();
        assert_eq!(s.doc.layers.len(), 3);
    }
    #[test]
    fn artboard_modifier_overrides_are_explicit_and_temporary() {
        let original = ArtboardOptions::default();
        assert_eq!(
            original.with_modifiers(true, false).resize,
            ArtworkResize::Keep
        );
        assert!(original.with_modifiers(false, true).clone);
        assert_eq!(original.resize, ArtworkResize::Scale);
        assert!(
            !ArtboardOptions {
                clone: true,
                ..original
            }
            .with_modifiers(false, true)
            .clone
        );
    }
    #[test]
    fn inspector_drag_clones_once_and_commits_one_undo_step() {
        let (mut s, original, _, _) = fixture();
        let before = crate::project::encode(&s.doc).unwrap();
        let count = |s: &Studio| s.doc.layers.iter().map(|l| l.kind.shapes().map_or(1, |s| s.len())).sum::<usize>();
        let original_count = count(&s);
        s.artboard_options = ArtboardOptions { resize: ArtworkResize::Scale, clone: true };
        for width in 101..=132 {
            let mut next = original.clone(); next.size.x = width as f32;
            s.preview_inspector_artboard(next);
            assert_eq!(count(&s), original_count * 2);
            assert_eq!(s.history.len(), 0);
        }
        s.finish_inspector_artboard();
        assert_eq!(s.history.len(), 1);
        let after = crate::project::encode(&s.doc).unwrap();
        s.undo(); assert_eq!(crate::project::encode(&s.doc).unwrap(), before);
        s.redo(); assert_eq!(crate::project::encode(&s.doc).unwrap(), after);
        s.undo();
        let mut next = original.clone(); next.size.x = 140.;
        s.preview_inspector_artboard(next);
        s.cancel_artboard_gesture();
        assert_eq!(crate::project::encode(&s.doc).unwrap(), before);
    }
    #[test]
    fn motion_wrap_uses_displayed_vector_and_raster_bounds() {
        let (mut s, _, vector, raster_layer) = fixture();
        s.persona = Persona::Motion; s.playhead = 1.;
        for (layer, id) in [(0, vector), (raster_layer, RASTER_ID)] {
            let target = crate::motion::target(&s.doc, layer, id).unwrap();
            s.doc.motion.set_key(target.id, Prop::X, 1., 500., Ease::Linear);
            s.doc.motion.set_key(target.id, Prop::Scale, 1., 2., Ease::Linear);
            let rest = if id == RASTER_ID { s.doc.layers[layer].kind.raster_bounds().unwrap() } else { s.doc.find_shape(layer, id).unwrap().world_bbox() };
            let expected = s.live_pose(target.id).map_bounds(rest);
            s.selection = vec![(layer, id)];
            s.wrap_selection_artboard_with_padding(0.);
            assert_eq!(s.doc.artboards.last().unwrap().bounds(), expected);
        }
    }

    #[test]
    fn artboard_clone_detaches_external_frame_and_preserves_world_transform() {
        let (mut s, _, id, _) = fixture();
        let mut frame = crate::layout::make_frame(Pt::new(-200., -200.), Pt::splat(100.));
        frame.rotation = 0.4;
        let parent = frame.id;
        let center = frame.geom.bbox().center();
        s.doc.layers[0].kind.shapes_mut().unwrap().push(frame);
        s.doc.find_shape_mut(0, id).unwrap().layout.parent = Some(parent);
        let source = s.doc.find_shape(0, id).unwrap().clone();
        let expected = source.geom.bbox().center().rotate_about(center, 0.4);
        let snap = ObjSnap { layer: 0, id, geom: Some(source.geom.clone()), origin: Pt::ZERO, size: Pt::ZERO, rot: source.rotation, shear: 0. };
        let (copies, commands) = s.copy_artboard_contents(&[snap]);
        let copy = s.doc.find_shape(0, copies[0].id).unwrap();
        assert_eq!(copy.layout.parent, None);
        assert!((copy.geom.bbox().center() - expected).length() < 0.001);
        assert_eq!(copy.rotation, source.rotation + 0.4);
        assert_eq!(copies[0].geom.as_ref(), Some(&copy.geom));
        s.discard_artboard_copies(commands);
        assert_eq!(s.doc.find_shape(0, id).unwrap(), &source);
    }

}
