use super::*;
use crate::motion_presets::{self, Preset, Target};

impl Studio {
    pub fn motion_preset_count(&self, preset: Preset) -> usize {
        self.selection
            .iter()
            .filter(|(layer, id)| {
                self.doc
                    .layers
                    .get(*layer)
                    .is_some_and(|layer| layer.visible && !layer.locked)
                    && self.doc.find_shape(*layer, *id).is_some_and(|shape| {
                        shape.visible && !shape.locked && !shape.guide && preset.supports(shape)
                    })
            })
            .count()
    }

    pub fn apply_motion_preset(&mut self, preset: Preset) {
        self.end_deform(false);
        self.end_pixel_stroke(false);
        self.commit_type_edit();
        let targets: Vec<_> = self
            .selection
            .iter()
            .filter_map(|(layer, id)| {
                let layer = self.doc.layers.get(*layer)?;
                if !layer.visible || layer.locked {
                    return None;
                }
                let shape = layer.kind.shapes()?.iter().find(|shape| shape.id == *id)?;
                (shape.visible && !shape.locked && !shape.guide && preset.supports(shape)).then(
                    || Target {
                        id: shape.id,
                        bounds: shape.world_bbox(),
                        opacity: shape.opacity,
                    },
                )
            })
            .collect();
        match motion_presets::apply(
            &self.doc.motion,
            preset,
            &targets,
            self.playhead,
            self.motion_preset_options,
        ) {
            Ok(after) => {
                let before = self.doc.motion.clone();
                if after != before {
                    self.commit(Cmd::Batch(vec![Cmd::SetMotion { before, after }]));
                }
                self.persona = Persona::Motion;
                self.playing = false;
                self.pose_drag.clear();
                self.reset_snap_gesture();
                let skipped = self.selection.len().saturating_sub(targets.len());
                self.status = format!(
                    "{} · {} objects · editable in the timeline{}",
                    preset.name(),
                    targets.len(),
                    if skipped > 0 {
                        format!(" · {skipped} incompatible skipped")
                    } else {
                        String::new()
                    }
                );
            }
            Err(error) => self.status = error,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn presets_commit_once_per_application_and_ignore_guides_or_locked_objects() {
        let mut studio = Studio::new();
        studio.doc.layers = vec![Layer::vector("Shapes")];
        for index in 0..4 {
            let mut shape = Shape::new(
                Geom::Rect {
                    origin: Pt::new(index as f32 * 30.0, 0.0),
                    size: Pt::new(20.0, 20.0),
                    radius: 0.0,
                },
                Style::default(),
            );
            shape.guide = index == 1;
            shape.locked = index == 2;
            shape.visible = index != 3;
            studio.selection.push((0, shape.id));
            studio.doc.layers[0].kind.shapes_mut().unwrap().push(shape);
        }
        studio.history.clear();
        let before = studio.doc.motion.clone();
        assert_eq!(studio.motion_preset_count(Preset::PopIn), 1);
        studio.apply_motion_preset(Preset::PopIn);
        let first = studio.doc.motion.clone();
        assert_eq!(studio.history.len(), 1);
        assert_eq!(first.shapes(), vec![studio.selection[0].1]);
        studio.apply_motion_preset(Preset::Shake);
        let second = studio.doc.motion.clone();
        assert_ne!(second, first);
        assert_eq!(studio.history.len(), 2);
        studio.undo();
        assert_eq!(studio.doc.motion, first);
        studio.undo();
        assert_eq!(studio.doc.motion, before);
        studio.redo();
        studio.redo();
        assert_eq!(studio.doc.motion, second);
    }
}
