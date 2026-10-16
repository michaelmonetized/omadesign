//! Appearance presets use the normal atomic vector command path.
use super::*;
use crate::graphic_styles::Appearance;

fn ready(studio: &Studio) -> Result<(), String> {
    if studio.op.is_some()
        || studio.type_edit.is_some()
        || studio.deformation.is_some()
        || studio.free_transform.is_some()
        || studio.pending_nav.is_some()
        || studio.file_dialog_pending()
        || studio.updates.freezing
    {
        return Err("Finish the current edit before applying a graphic style".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color::Blend;
    #[test]
    fn craft_graphic_style_applies_complete_appearance_atomically_and_preserves_other_object_state()
    {
        let mut s = crate::app::symbols::tests::fixture();
        let (layer, source) = s.selection[0];
        let target = s.selection[1].1;
        let shape = s.doc.find_shape_mut(layer, source).unwrap();
        let mut gradient = crate::gradient::Gradient::new(
            crate::gradient::GradientKind::Radial,
            Rgba::new(12, 171, 52, 203),
            Rgba::rgb(200, 20, 40),
        );
        gradient.radial_axis = Some([-0.3, 0.8]);
        gradient.focal = [0.2, 0.3];
        gradient.inner_radius = 0.19;
        gradient.spread = crate::gradient::Spread::Reflect;
        shape.style.fill = Fill::Gradient(gradient);
        shape.opacity = 0.61;
        shape.fill_opacity = 0.73;
        shape.blend_interior = true;
        shape.blend = Blend::Multiply;
        shape.filters.items.push(crate::filter::Fx::Shadow {
            blend: Blend::Normal,
            opacity: 0.42,
            knockout: true,
            spread: 0.,
            dx: 2.,
            dy: 3.,
            blur: 1.2,
            color: Rgba::rgb(20, 40, 70),
        });
        let mut image = tiny_skia::Pixmap::new(2, 2).unwrap();
        image.fill(tiny_skia::Color::from_rgba8(12, 171, 52, 203));
        shape.layout.image =
            Some(crate::layout_images::from_bytes(&image.encode_png().unwrap()).unwrap());
        s.selection = vec![(layer, source)];
        let appearance = s.capture_graphic_style().unwrap();
        let mask = crate::document::Pixels::from_rgba(1, 1, vec![128, 128, 128, 255]).unwrap();
        s.doc.find_shape_mut(layer, target).unwrap().mask = Some(mask);
        let geometry = s.doc.find_shape(layer, target).unwrap().geom.clone();
        let original = crate::project::encode(&s.doc).unwrap();
        s.selection = vec![(layer, target), (layer, source)];
        let history = s.history.len();
        assert!(s.apply_graphic_style(&appearance).unwrap());
        assert_eq!(s.history.len(), history + 1);
        let target = s.doc.find_shape(layer, target).unwrap();
        assert_eq!(Appearance::capture(target), appearance);
        assert_eq!(target.geom, geometry);
        assert_eq!(
            target.mask.as_ref().unwrap().data.as_slice(),
            &[128, 128, 128, 255]
        );
        let saved = crate::project::encode(&s.doc).unwrap();
        let reopened = crate::project::decode(&saved).unwrap();
        assert_eq!(crate::project::encode(&reopened).unwrap(), saved);
        s.undo();
        assert_eq!(crate::project::encode(&s.doc).unwrap(), original);
        s.redo();
        assert_eq!(crate::project::encode(&s.doc).unwrap(), saved);
        let history = s.history.len();
        assert!(!s.apply_graphic_style(&appearance).unwrap());
        assert_eq!(s.history.len(), history);
    }
    #[test]
    fn craft_graphic_style_locked_hidden_missing_and_busy_selections_refuse_without_partial_edits()
    {
        let mut s = crate::app::symbols::tests::fixture();
        let appearance = s.capture_graphic_style().unwrap();
        let mut broken = appearance.clone();
        broken.image = Some(crate::layout_images::ImageFill {
            data: "not PNG data".into(),
            fit: Default::default(),
            focal: Pt::splat(0.5),
        });
        let original = crate::project::encode(&s.doc).unwrap();
        assert!(s.apply_graphic_style(&broken).is_err());
        assert_eq!(crate::project::encode(&s.doc).unwrap(), original);
        let (layer, target) = s.selection[1];
        s.doc.find_shape_mut(layer, target).unwrap().locked = true;
        let before = crate::project::encode(&s.doc).unwrap();
        assert!(s.apply_graphic_style(&appearance).is_err());
        assert_eq!(crate::project::encode(&s.doc).unwrap(), before);
        assert_eq!(s.history.len(), 0);
        s.doc.find_shape_mut(layer, target).unwrap().locked = false;
        s.doc.layers[layer].visible = false;
        assert!(s.apply_graphic_style(&appearance).is_err());
        s.doc.layers[layer].visible = true;
        s.selection.push((layer, u64::MAX));
        assert!(s.apply_graphic_style(&appearance).is_err());
        s.selection.pop();
        s.free_transform();
        assert!(s.apply_graphic_style(&appearance).is_err());
        assert_eq!(s.history.len(), 0);
    }
}
fn editable(studio: &Studio, layer: usize, id: u64) -> bool {
    studio.doc.layer_visible(layer)
        && super::layout_components::component_object_editable(&studio.doc, layer, id)
        && studio
            .doc
            .find_shape(layer, id)
            .is_some_and(|s| s.visible && !s.guide)
}
impl Studio {
    /// Capture paint, effects, transparency and an embedded image fill from the concrete primary object.
    /// Args: none. Returns a self-contained appearance without changing the document.
    pub(crate) fn capture_graphic_style(&self) -> Result<Appearance, String> {
        ready(self)?;
        let (layer, id) = self
            .primary()
            .ok_or("Select a vector object to capture its graphic style")?;
        if !editable(self, layer, id) {
            return Err("Show and unlock the source artwork first".into());
        }
        let value = Appearance::capture(self.doc.find_shape(layer, id).unwrap());
        crate::graphic_styles::validate_appearance(&value)?;
        Ok(value)
    }

    /// Apply an appearance to all current concrete vector targets in one Undo step.
    /// Args: saved appearance. Returns whether any artwork changed; missing or locked targets refuse the whole edit.
    pub(crate) fn apply_graphic_style(&mut self, appearance: &Appearance) -> Result<bool, String> {
        ready(self)?;
        crate::graphic_styles::validate_appearance(appearance)?;
        if self.selection.is_empty() {
            return Err("Select vector artwork before applying a graphic style".into());
        }
        if self
            .selection
            .iter()
            .any(|&(layer, id)| !editable(self, layer, id))
        {
            return Err("The selection changed or contains hidden or locked artwork".into());
        }
        let mut layers = std::collections::BTreeMap::<usize, Vec<Shape>>::new();
        for &(layer, id) in &self.selection {
            let shapes = layers
                .entry(layer)
                .or_insert_with(|| self.doc.layers[layer].kind.shapes().unwrap().to_vec());
            let shape = shapes.iter_mut().find(|s| s.id == id).unwrap();
            appearance.apply(shape);
        }
        let commands: Vec<_> = layers
            .into_iter()
            .filter_map(|(layer, after)| {
                let before = self.doc.layers[layer].kind.shapes().unwrap();
                (before != after).then(|| Cmd::SetVectorShapes {
                    layer,
                    before: before.to_vec(),
                    after,
                })
            })
            .collect();
        if commands.is_empty() {
            return Ok(false);
        }
        self.commit(Cmd::Batch(commands));
        Ok(true)
    }
}
