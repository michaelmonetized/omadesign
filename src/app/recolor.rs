//! Selection-bound recoloring changes only existing editable paint colors.
use super::*;

#[derive(Clone, Debug)]
pub(crate) struct RecolorTarget {
    document: String,
    revision: u64,
    selection: Vec<(usize, u64)>,
    selected_layer: Option<u64>,
    objects: Vec<(usize, u64)>,
    pub colors: Vec<Rgba>,
    pub patterns: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> Studio {
        let mut studio = Studio::new();
        studio.doc = Document::new("Recolor", 240., 180., 72.);
        let mut layer = Layer::vector("Art");
        let mut first = Shape::new(
            Geom::Rect {
                origin: Pt::new(10., 10.),
                size: Pt::new(50., 40.),
                radius: 0.,
            },
            Style::default(),
        );
        let mut gradient = crate::gradient::Gradient::new(
            crate::gradient::GradientKind::Radial,
            Rgba::new(255, 0, 0, 77),
            Rgba::new(0, 0, 255, 192),
        );
        gradient.from = [0.1, 0.2];
        gradient.to = [0.8, 0.7];
        gradient.radial_axis = Some([-0.2, 0.9]);
        gradient.focal = [0.3, 0.4];
        gradient.spread = crate::gradient::Spread::Reflect;
        first.style.fill = Fill::Gradient(gradient);
        first.opacity = 0.61;
        first.fill_opacity = 0.73;
        let mut text = TypeRun {
            content: "AB".into(),
            px: 24.,
            ..TypeRun::default()
        };
        text.spans.push(crate::geom::CharSpan {
            start: 0,
            end: 1,
            fill: Some(Fill::Solid(Rgba::new(255, 0, 0, 77))),
            tracking: Some(13.),
            ..Default::default()
        });
        let second = Shape::new(Geom::Text(text), Style::default());
        studio.selection = vec![(0, first.id), (0, second.id)];
        *layer.kind.shapes_mut().unwrap() = vec![first, second];
        studio.doc.layers = vec![layer];
        studio.active_layer = Some(0);
        studio.selected_layer = None;
        studio
    }

    #[test]
    fn craft_color_recolor_keeps_gradient_geometry_spans_opacity_motion_one_undo_and_reopen() {
        let mut s = fixture();
        let (layer, id) = s.selection[0];
        s.doc.motion.set_key(
            id,
            crate::motion::Prop::Opacity,
            1.,
            0.3,
            crate::motion::Ease::EaseInOut,
        );
        let before = crate::project::encode(&s.doc).unwrap();
        let target = s.recolor_target().unwrap();
        let motion = s.doc.motion.clone();
        let replacements: Vec<_> = target
            .colors
            .iter()
            .map(|c| Rgba::new(c.b, c.r, c.g, 255))
            .collect();
        assert!(s.recolor_artwork(&target, &replacements).unwrap());
        assert_eq!(s.history.len(), 1);
        assert_eq!(s.doc.motion, motion);
        let first = s.doc.find_shape(layer, id).unwrap();
        assert_eq!(first.opacity, 0.61);
        assert_eq!(first.fill_opacity, 0.73);
        let Fill::Gradient(g) = &first.style.fill else {
            panic!()
        };
        assert_eq!(g.from, [0.1, 0.2]);
        assert_eq!(g.to, [0.8, 0.7]);
        assert_eq!(g.radial_axis, Some([-0.2, 0.9]));
        assert_eq!(g.focal, [0.3, 0.4]);
        assert_eq!(g.spread, crate::gradient::Spread::Reflect);
        assert_eq!(g.stops[0].color, Rgba::new(0, 255, 0, 77));
        assert_eq!(g.stops[1].color, Rgba::new(255, 0, 0, 192));
        let second = s
            .doc
            .find_shape(s.selection[1].0, s.selection[1].1)
            .unwrap();
        let Geom::Text(run) = &second.geom else {
            panic!()
        };
        assert_eq!(run.spans[0].tracking, Some(13.));
        assert_eq!(
            run.spans[0].fill,
            Some(Fill::Solid(Rgba::new(0, 255, 0, 77)))
        );
        let after = crate::project::encode(&s.doc).unwrap();
        assert_eq!(
            crate::project::encode(&crate::project::decode(&after).unwrap()).unwrap(),
            after
        );
        s.undo();
        assert_eq!(crate::project::encode(&s.doc).unwrap(), before);
        s.redo();
        assert_eq!(crate::project::encode(&s.doc).unwrap(), after);
    }

    #[test]
    fn craft_color_recolor_rejects_stale_locked_hidden_and_incomplete_drafts_atomically() {
        let mut s = fixture();
        let original = crate::project::encode(&s.doc).unwrap();
        let target = s.recolor_target().unwrap();
        assert!(s.recolor_artwork(&target, &[]).is_err());
        assert_eq!(crate::project::encode(&s.doc).unwrap(), original);
        s.selection.reverse();
        assert!(s.recolor_artwork(&target, &target.colors).is_err());
        s.selection.reverse();
        s.mark();
        assert!(s.recolor_artwork(&target, &target.colors).is_err());
        let target = s.recolor_target().unwrap();
        s.doc.layers[0].locked = true;
        assert!(s.recolor_artwork(&target, &target.colors).is_err());
        assert!(s.apply_palette_color(Rgba::WHITE).is_err());
        s.doc.layers[0].locked = false;
        s.doc.layers[0].visible = false;
        assert!(s.recolor_target().is_err());
        s.doc.layers[0].visible = true;
        s.swap_id.push('x');
        assert!(s.recolor_artwork(&target, &target.colors).is_err());
        assert_eq!(crate::project::encode(&s.doc).unwrap(), original);
        assert_eq!(s.history.len(), 0);
    }

    #[test]
    fn craft_color_palette_stroke_and_layer_targets_are_one_transaction() {
        let mut s = fixture();
        let before = crate::project::encode(&s.doc).unwrap();
        s.selection.clear();
        s.selected_layer = Some(s.doc.layers[0].id);
        s.fill_active = false;
        s.apply_palette_color(Rgba::new(18, 171, 52, 123)).unwrap();
        assert_eq!(s.history.len(), 1);
        assert!(
            s.doc.layers[0].kind.shapes().unwrap().iter().all(|s| s
                .style
                .stroke
                .as_ref()
                .unwrap()
                .color
                == Rgba::new(18, 171, 52, 123))
        );
        s.undo();
        assert_eq!(crate::project::encode(&s.doc).unwrap(), before);
    }
}

fn fill_colors(fill: &Fill, mut visit: impl FnMut(Rgba)) {
    match fill {
        Fill::Solid(c) => visit(*c),
        Fill::Linear { c0, c1, .. } | Fill::Radial { c0, c1 } => {
            visit(*c0);
            visit(*c1);
        }
        Fill::Gradient(g) => {
            for stop in &g.stops {
                visit(stop.color);
            }
        }
        _ => {}
    }
}
fn change_fill(fill: &mut Fill, map: &impl Fn(Rgba) -> Rgba) {
    match fill {
        Fill::Solid(c) => *c = map(*c),
        Fill::Linear { c0, c1, .. } | Fill::Radial { c0, c1 } => {
            *c0 = map(*c0);
            *c1 = map(*c1);
        }
        Fill::Gradient(g) => {
            for stop in &mut g.stops {
                stop.color = map(stop.color);
            }
        }
        _ => {}
    }
}
fn visit_colors(shape: &Shape, mut visit: impl FnMut(Rgba)) -> usize {
    fill_colors(&shape.style.fill, &mut visit);
    let mut patterns = usize::from(matches!(shape.style.fill, Fill::Pattern(_)));
    if let Some(stroke) = &shape.style.stroke {
        visit(stroke.color);
        if let Some(g) = &stroke.gradient {
            for stop in &g.stops {
                visit(stop.color);
            }
        }
    }
    if let Geom::Text(run) = &shape.geom {
        for span in &run.spans {
            if let Some(fill) = &span.fill {
                fill_colors(fill, &mut visit);
                patterns += usize::from(matches!(fill, Fill::Pattern(_)));
            }
        }
    }
    patterns
}
fn editable(studio: &Studio, layer: usize, id: u64) -> bool {
    if !studio.doc.layer_editable(layer) || !studio.doc.layer_visible(layer) {
        return false;
    }
    let mut current = Some(id);
    let mut seen = HashSet::new();
    while let Some(id) = current {
        if !seen.insert(id) {
            return false;
        }
        let Some(shape) = studio.doc.find_shape(layer, id) else {
            return false;
        };
        if shape.locked || !shape.visible || shape.guide {
            return false;
        }
        current = shape.layout.parent;
    }
    true
}

impl Studio {
    /// Apply a palette color to real editable paint owners as one Undo step.
    /// Args: straight RGBA color. Returns an error without touching locked or stale artwork.
    pub(crate) fn apply_palette_color(&mut self, color: Rgba) -> Result<(), String> {
        if self.pending_nav.is_some()
            || self.file_dialog_pending()
            || self.op.is_some()
            || self.updates.freezing
        {
            return Err("Finish the current operation before applying a color".into());
        }
        let mut commands = vec![];
        if !self.selection.is_empty() || self.selected_layer.is_some() {
            let target = self.recolor_target()?;
            for (layer, id) in target.objects {
                let shape = self.doc.find_shape(layer, id).unwrap();
                let mut style = shape.style.clone();
                if self.fill_active {
                    style.fill = Fill::Solid(color);
                } else {
                    let stroke = style.stroke.get_or_insert_with(Stroke::default);
                    stroke.color = color;
                    stroke.gradient = None;
                }
                if style != shape.style {
                    commands.push(Cmd::SetStyle {
                        layer,
                        id,
                        before: shape.style.clone(),
                        after: style,
                    });
                }
            }
        }
        if !commands.is_empty() {
            self.commit(Cmd::Batch(commands));
        }
        if self.fill_active {
            self.style.fill = Fill::Solid(color);
        } else {
            let stroke = self.style.stroke.get_or_insert_with(Stroke::default);
            stroke.color = color;
            stroke.gradient = None;
        }
        self.push_recent(color);
        Ok(())
    }

    /// Capture concrete editable paint owners for a recolor draft.
    /// Args: none. Returns the document revision, selection and original straight colors.
    pub(crate) fn recolor_target(&self) -> Result<RecolorTarget, String> {
        let mut objects = self.selection.clone();
        if objects.is_empty()
            && let Some(id) = self.selected_layer
        {
            let layer = self
                .doc
                .layers
                .iter()
                .position(|l| l.id == id)
                .ok_or("The layer is no longer available")?;
            for index in self.layer_tree_indices(layer) {
                if let Some(shapes) = self.doc.layers[index].kind.shapes() {
                    objects.extend(shapes.iter().map(|s| (index, s.id)));
                }
            }
        }
        let selected: HashSet<_> = objects.iter().copied().collect();
        for (layer, item) in self.doc.layers.iter().enumerate() {
            if let Some(shapes) = item.kind.shapes() {
                for shape in shapes {
                    let mut parent = shape.layout.parent;
                    let mut seen = HashSet::new();
                    while let Some(id) = parent {
                        if !seen.insert(id) {
                            break;
                        }
                        if selected.contains(&(layer, id)) {
                            objects.push((layer, shape.id));
                            break;
                        }
                        parent = self.doc.find_shape(layer, id).and_then(|s| s.layout.parent);
                    }
                }
            }
        }
        let mut seen = HashSet::new();
        objects.retain(|item| seen.insert(*item));
        if objects.is_empty() {
            return Err("Select vector artwork to recolor".into());
        }
        if objects
            .iter()
            .any(|&(layer, id)| !editable(self, layer, id))
        {
            return Err("Recolor needs visible, unlocked vector artwork and ancestors".into());
        }
        let mut colors = vec![];
        let mut patterns = 0;
        for &(layer, id) in &objects {
            patterns += visit_colors(self.doc.find_shape(layer, id).unwrap(), |color| {
                if !colors.contains(&color) {
                    colors.push(color);
                }
            });
        }
        if colors.len() > 4096 {
            return Err("Recolor supports up to 4096 distinct paint colors".into());
        }
        Ok(RecolorTarget {
            document: self.swap_id.clone(),
            revision: self.canvas_gen,
            selection: self.selection.clone(),
            selected_layer: self.selected_layer,
            objects,
            colors,
            patterns,
        })
    }

    /// Apply one complete color mapping through the existing atomic command history.
    /// Args: captured owner and replacements in original-color order. Returns whether paint changed.
    pub(crate) fn recolor_artwork(
        &mut self,
        target: &RecolorTarget,
        replacements: &[Rgba],
    ) -> Result<bool, String> {
        if target.document != self.swap_id
            || target.revision != self.canvas_gen
            || target.selection != self.selection
            || target.selected_layer != self.selected_layer
            || self.pending_nav.is_some()
            || self.file_dialog_pending()
            || self.op.is_some()
            || self.updates.freezing
            || target
                .objects
                .iter()
                .any(|&(layer, id)| !editable(self, layer, id))
        {
            return Err("Artwork changed. Refresh the recolor selection before applying".into());
        }
        if replacements.len() != target.colors.len() {
            return Err("Every original color needs one replacement".into());
        }
        let mapping: HashMap<_, _> = target
            .colors
            .iter()
            .copied()
            .zip(replacements.iter().copied())
            .collect();
        let map = |color: Rgba| {
            mapping
                .get(&color)
                .map_or(color, |new| Rgba::new(new.r, new.g, new.b, color.a))
        };
        let mut commands = vec![];
        for &(layer, id) in &target.objects {
            let shape = self.doc.find_shape(layer, id).unwrap();
            let mut style = shape.style.clone();
            change_fill(&mut style.fill, &map);
            if let Some(stroke) = &mut style.stroke {
                stroke.color = map(stroke.color);
                if let Some(gradient) = &mut stroke.gradient {
                    for stop in &mut gradient.stops {
                        stop.color = map(stop.color);
                    }
                }
            }
            if style != shape.style {
                commands.push(Cmd::SetStyle {
                    layer,
                    id,
                    before: shape.style.clone(),
                    after: style,
                });
            }
            if let Geom::Text(run) = &shape.geom {
                let mut after = run.clone();
                for span in &mut after.spans {
                    if let Some(fill) = &mut span.fill {
                        change_fill(fill, &map);
                    }
                }
                if &after != run {
                    commands.push(Cmd::SetGeom {
                        layer,
                        id,
                        before: shape.geom.clone(),
                        after: Geom::Text(after),
                        rot_before: shape.rotation,
                        rot_after: shape.rotation,
                    });
                }
            }
        }
        if commands.is_empty() {
            return Ok(false);
        }
        self.commit(Cmd::Batch(commands));
        self.status = "Artwork recolored".into();
        Ok(true)
    }
}
