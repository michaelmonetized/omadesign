//! Native composition authoring in the motion timeline.
use crate::{app::Studio, motion::compositions::Composition};
use eframe::egui::{self, Ui};

pub(super) fn show(ui: &mut Ui, studio: &mut Studio) {
    ui.menu_button("Compositions", |ui| {
        if ui.button("Add project…").clicked()
            && let Some(path) = rfd::FileDialog::new()
                .add_filter("Motion project", &["oma", "omamotion"])
                .pick_file()
        {
            let imported = if path.extension().is_some_and(|e| e == "omamotion") {
                crate::motion::compositions::load_from(&path)
            } else {
                crate::project::load_from(&path)
            };
            match imported {
                Ok(doc) => {
                    let mut after = studio.doc.motion.clone();
                    let comp = Composition::new(doc);
                    after.duration = after.duration.max(comp.end);
                    after.compositions.push(comp);
                    let mut candidate = studio.doc.clone();
                    candidate.motion = after.clone();
                    match crate::motion::compositions::validate(&candidate) {
                        Ok(()) => studio.commit_motion_step(after),
                        Err(e) => studio.status = e,
                    }
                }
                Err(e) => studio.status = e,
            }
        }
        if ui.button("Export motion project…").clicked()
            && let Some(path) = rfd::FileDialog::new()
                .add_filter("Motion project", &["omamotion"])
                .set_file_name(format!("{}.omamotion", studio.doc.name))
                .save_file()
        {
            studio.status = match crate::motion::compositions::save_to(&studio.doc, &path)
            {
                Ok(()) => "Motion project saved".into(),
                Err(e) => e,
            };
        }
        let mut after = studio.doc.motion.clone();
        let mut changed = false;
        let mut remove = None;
        let mut action = None;
        for comp in &mut after.compositions {
            ui.push_id(comp.id, |ui| {
                ui.collapsing(comp.name.clone(), |ui| {
                    changed |= ui.checkbox(&mut comp.visible, "Visible").changed();
                    changed |= ui.text_edit_singleline(&mut comp.name).changed();
                    for (name, value) in [
                        ("Start", &mut comp.start),
                        ("End", &mut comp.end),
                        ("Source time", &mut comp.source_start),
                        ("Speed", &mut comp.speed),
                        ("X", &mut comp.position.x),
                        ("Y", &mut comp.position.y),
                        ("Anchor X", &mut comp.anchor.x),
                        ("Anchor Y", &mut comp.anchor.y),
                        ("Scale X", &mut comp.scale.x),
                        ("Scale Y", &mut comp.scale.y),
                        ("Opacity", &mut comp.opacity),
                    ] {
                        ui.horizontal(|ui| {
                            ui.label(name);
                            changed |= ui.add(egui::DragValue::new(value).speed(0.05)).changed();
                        });
                    }
                    let mut degrees = comp.rotation.to_degrees();
                    ui.horizontal(|ui| {
                        ui.label("Rotate");
                        if ui
                            .add(egui::DragValue::new(&mut degrees).suffix("°"))
                            .changed()
                        {
                            comp.rotation = degrees.to_radians();
                            changed = true;
                        }
                    });
                    changed |= ui.checkbox(&mut comp.looped, "Loop source").changed();
                    animation_controls(ui, studio, comp, &mut action);
                    if ui.button("Remove composition").clicked() {
                        remove = Some(comp.id);
                    }
                });
            });
        }
        if let Some(id) = remove {
            after.compositions.retain(|c| c.id != id);
            after.drop_shape(id);
            changed = true;
        }
        if changed {
            after.duration = after
                .compositions
                .iter()
                .map(|c| c.end)
                .fold(after.duration, f32::max);
            let mut doc = studio.doc.clone();
            doc.motion = after.clone();
            match crate::motion::compositions::validate(&doc) {
                Ok(()) => studio.commit_motion(after),
                Err(e) => studio.status = e,
            }
        }
        if let Some(action) = action {
            if let Err(error) = apply_edit(studio, action) {
                studio.status = error;
            }
        }
    });
}

#[derive(Clone)]
struct AnimationDraft {
    property: crate::motion::Prop,
    value: f32,
    ease: crate::motion::Ease,
    source: String,
    time: f32,
}

impl AnimationDraft {
    fn new(motion: &crate::motion::Motion, id: u64, time: f32) -> Self {
        let mut draft = Self {
            property: crate::motion::Prop::X,
            value: 0.,
            ease: crate::motion::Ease::EaseInOut,
            source: String::new(),
            time,
        };
        draft.read(motion, id, true);
        draft
    }
    fn read(&mut self, motion: &crate::motion::Motion, id: u64, source: bool) {
        self.value = motion
            .value(id, self.property, self.time)
            .unwrap_or(self.property.identity());
        if self.property == crate::motion::Prop::Rotation {
            self.value = self.value.to_degrees();
        }
        if source {
            self.source = motion
                .expressions
                .iter()
                .find(|e| e.shape == id && e.prop == self.property)
                .map(|e| e.source.clone())
                .unwrap_or_default();
        }
    }
}

#[derive(Clone)]
enum Edit {
    Key {
        id: u64,
        property: crate::motion::Prop,
        value: f32,
        ease: crate::motion::Ease,
        remove: bool,
    },
    Expression {
        id: u64,
        property: crate::motion::Prop,
        source: Option<String>,
    },
    Remap {
        id: u64,
        value: f32,
        ease: crate::motion::Ease,
        remove: bool,
        clear: bool,
    },
}

fn properties() -> [crate::motion::Prop; 7] {
    use crate::motion::Prop;
    [
        Prop::X,
        Prop::Y,
        Prop::Rotation,
        Prop::Scale,
        Prop::Width,
        Prop::Height,
        Prop::Opacity,
    ]
}

fn easing(ui: &mut Ui, ease: &mut crate::motion::Ease) {
    use crate::motion::Ease;
    egui::ComboBox::from_id_salt("easing")
        .selected_text(ease.name())
        .show_ui(ui, |ui| {
            for option in [
                Ease::Linear,
                Ease::EaseIn,
                Ease::EaseOut,
                Ease::EaseInOut,
                Ease::Hold,
                Ease::CubicBezier {
                    x1: 0.25,
                    y1: 0.1,
                    x2: 0.25,
                    y2: 1.,
                },
            ] {
                ui.selectable_value(ease, option, option.name());
            }
        });
    if let Ease::CubicBezier { x1, y1, x2, y2 } = ease {
        for (label, value) in [("X1", x1), ("Y1", y1), ("X2", x2), ("Y2", y2)] {
            ui.horizontal(|ui| {
                ui.label(label);
                ui.add(egui::DragValue::new(value).speed(0.01));
            });
        }
    }
}

fn animation_controls(ui: &mut Ui, studio: &Studio, comp: &Composition, action: &mut Option<Edit>) {
    ui.collapsing("Animate", |ui| {
        let key = ui.id().with("animation-draft");
        let mut draft = ui
            .data(|d| d.get_temp::<AnimationDraft>(key))
            .unwrap_or_else(|| AnimationDraft::new(&studio.doc.motion, comp.id, studio.playhead));
        let previous = draft.property;
        egui::ComboBox::from_id_salt("property")
            .selected_text(draft.property.name())
            .show_ui(ui, |ui| {
                for prop in properties() {
                    ui.selectable_value(&mut draft.property, prop, prop.name());
                }
            });
        if previous != draft.property || draft.time != studio.playhead {
            draft.time = studio.playhead;
            draft.read(&studio.doc.motion, comp.id, previous != draft.property);
        }
        ui.label(format!("Key at {:.3} s", studio.playhead));
        ui.add(egui::DragValue::new(&mut draft.value).speed(0.1));
        ui.label(match draft.property {
            crate::motion::Prop::Rotation => "Rotation in degrees",
            crate::motion::Prop::Opacity => "Opacity multiplier · 0–1",
            crate::motion::Prop::Scale
            | crate::motion::Prop::Width
            | crate::motion::Prop::Height => "Scale multiplier",
            _ => "Offset from composition position",
        });
        easing(ui, &mut draft.ease);
        ui.horizontal(|ui| {
            if ui.button("Set key").clicked() {
                *action = Some(Edit::Key {
                    id: comp.id,
                    property: draft.property,
                    value: draft.value,
                    ease: draft.ease,
                    remove: false,
                });
            }
            if ui.button("Remove key").clicked() {
                *action = Some(Edit::Key {
                    id: comp.id,
                    property: draft.property,
                    value: draft.value,
                    ease: draft.ease,
                    remove: true,
                });
            }
        });
        ui.separator();
        ui.label("Expression");
        ui.add(
            egui::TextEdit::multiline(&mut draft.source)
                .code_editor()
                .desired_rows(3)
                .hint_text("value + sin(time * tau) * 40"),
        );
        ui.label("time in seconds · expression rotation in radians");
        ui.horizontal(|ui| {
            if ui.button("Apply expression").clicked() {
                *action = Some(Edit::Expression {
                    id: comp.id,
                    property: draft.property,
                    source: Some(draft.source.clone()),
                });
            }
            if ui.button("Remove expression").clicked() {
                *action = Some(Edit::Expression {
                    id: comp.id,
                    property: draft.property,
                    source: None,
                });
                draft.source.clear();
            }
        });
        if let Err(error) = studio
            .doc
            .motion
            .evaluate_frame(studio.playhead, &mut || true)
        {
            ui.colored_label(egui::Color32::LIGHT_RED, error.to_string());
        }
        ui.data_mut(|d| d.insert_temp(key, draft));
    });
    ui.collapsing("Time remapping", |ui| {
        let key = ui.id().with("source-time");
        let mut value = ui.data(|d| d.get_temp::<f32>(key)).unwrap_or_else(|| {
            comp.local_time(studio.playhead)
                .unwrap_or(comp.source_start)
        });
        ui.label(format!("Parent time {:.3} s", studio.playhead));
        ui.horizontal(|ui| {
            ui.label("Source seconds");
            ui.add(egui::DragValue::new(&mut value).speed(0.05));
        });
        ui.data_mut(|d| d.insert_temp(key, value));
        let ease_key = ui.id().with("remap-ease");
        let mut ease = ui
            .data(|d| d.get_temp::<crate::motion::Ease>(ease_key))
            .unwrap_or(crate::motion::Ease::Linear);
        easing(ui, &mut ease);
        ui.data_mut(|d| d.insert_temp(ease_key, ease));
        ui.horizontal(|ui| {
            for (label, remove, clear) in [
                ("Set source key", false, false),
                ("Remove source key", true, false),
                ("Clear remapping", false, true),
            ] {
                if ui.button(label).clicked() {
                    *action = Some(Edit::Remap {
                        id: comp.id,
                        value,
                        ease,
                        remove,
                        clear,
                    });
                }
            }
        });
        ui.label(format!("{} source-time keys", comp.time_remap.len()));
        egui::ScrollArea::vertical()
            .max_height(120.)
            .show(ui, |ui| {
                for k in &comp.time_remap {
                    ui.label(format!(
                        "{:.3} s → {:.3} s · {}",
                        k.t,
                        k.value,
                        k.ease.name()
                    ));
                }
            });
    });
}

/// Apply one explicit composition animation edit.
/// Arguments are the live studio and requested key, expression or source-time edit; returns validation status and commits one undo step on change.
fn apply_edit(studio: &mut Studio, edit: Edit) -> Result<(), String> {
    use crate::motion::{Key, Prop};
    let time = studio.playhead;
    if !time.is_finite() || !(0.0..=3600.0).contains(&time) {
        return Err("Key time must be 0–3600 seconds".into());
    }
    let mut after = studio.doc.motion.clone();
    let id = match &edit {
        Edit::Key { id, .. } | Edit::Expression { id, .. } | Edit::Remap { id, .. } => *id,
    };
    if !after.compositions.iter().any(|c| c.id == id) {
        return Err("Unknown composition".into());
    }
    match edit {
        Edit::Key {
            property,
            value,
            ease,
            remove,
            ..
        } => {
            if !properties().contains(&property) {
                return Err("Choose a composition transform property".into());
            }
            if remove {
                if let Some(track) = after
                    .tracks
                    .iter_mut()
                    .find(|t| t.shape == id && t.prop == property)
                {
                    track.keys.retain(|k| (k.t - time).abs() > 0.0001);
                }
                after.tracks.retain(|t| !t.keys.is_empty());
            } else {
                if !value.is_finite()
                    || value.abs() > 100_000.0
                    || property == Prop::Opacity && !(0.0..=1.0).contains(&value)
                    || !ease.valid()
                {
                    return Err("Key value or curve is invalid; opacity must be 0–1".into());
                }
                let value = if property == Prop::Rotation {
                    value.to_radians()
                } else {
                    value
                };
                after.duration = after.duration.max(time);
                after.set_key(id, property, time, value, ease);
            }
        }
        Edit::Expression {
            property, source, ..
        } => {
            if !properties().contains(&property) {
                return Err("Choose a composition transform property".into());
            }
            after.set_expression(id, property, source)?;
        }
        Edit::Remap {
            value,
            ease,
            remove,
            clear,
            ..
        } => {
            let comp = after
                .compositions
                .iter_mut()
                .find(|c| c.id == id)
                .ok_or("Unknown composition")?;
            if clear {
                comp.time_remap.clear();
            } else if remove {
                comp.time_remap.retain(|k| (k.t - time).abs() > 0.0001);
            } else {
                if !value.is_finite() || value.abs() > 3600.0 || !ease.valid() {
                    return Err(
                        "Source seconds must be finite within ±3600 and the curve valid".into(),
                    );
                }
                if let Some(key) = comp
                    .time_remap
                    .iter_mut()
                    .find(|k| (k.t - time).abs() < 0.0001)
                {
                    *key = Key::at(time, value, ease);
                } else {
                    if comp.time_remap.len() >= 10_000 {
                        return Err("At most 10000 source-time keys".into());
                    }
                    comp.time_remap.push(Key::at(time, value, ease));
                }
                comp.time_remap.sort_by(|a, b| a.t.total_cmp(&b.t));
            }
        }
    }
    let mut candidate = studio.doc.clone();
    candidate.motion = after.clone();
    crate::motion::compositions::validate(&candidate)?;
    studio.commit_motion_step(after);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        document::{Document, Layer},
        geom::Pt,
        motion::{Ease, Prop},
    };
    fn studio() -> (Studio, u64) {
        let mut studio = Studio::new();
        let mut child = Document::new("Artwork", 16., 16., 96.);
        child.layers.clear();
        child.artboards.clear();
        child.artboardless = true;
        child.transparent = true;
        let mut layer = Layer::vector("Block");
        layer
            .kind
            .shapes_mut()
            .unwrap()
            .push(crate::layout::make_frame(Pt::new(1., 1.), Pt::splat(4.)));
        child.layers.push(layer);
        let comp = Composition::new(child);
        let id = comp.id;
        studio.doc = Document::new("Parent", 48., 32., 96.);
        studio.doc.layers.clear();
        studio.doc.artboards.clear();
        studio.doc.artboardless = true;
        studio.doc.transparent = true;
        studio.doc.motion.compositions.push(comp);
        studio.history.clear();
        (studio, id)
    }
    #[test]
    fn explicit_composition_controls_animate_pixels_and_each_apply_has_one_undo() {
        let (mut studio, id) = studio();
        apply_edit(
            &mut studio,
            Edit::Key {
                id,
                property: Prop::X,
                value: 0.,
                ease: Ease::Linear,
                remove: false,
            },
        )
        .unwrap();
        studio.playhead = 1.;
        apply_edit(
            &mut studio,
            Edit::Key {
                id,
                property: Prop::X,
                value: 12.,
                ease: Ease::Hold,
                remove: false,
            },
        )
        .unwrap();
        assert_eq!(studio.history.len(), 2);
        assert_eq!(studio.doc.motion.pose(id, 0.5).dx, 6.);
        let a = crate::compositor::render_export_at(&studio.doc, 1., 0., true).unwrap();
        let b = crate::compositor::render_export_at(&studio.doc, 1., 1., true).unwrap();
        assert_ne!(a.data(), b.data());
        let keyed = studio.doc.motion.clone();
        apply_edit(
            &mut studio,
            Edit::Expression {
                id,
                property: Prop::X,
                source: Some("value + time*2".into()),
            },
        )
        .unwrap();
        assert_eq!(studio.history.len(), 3);
        assert_eq!(studio.doc.motion.pose(id, 0.5).dx, 7.);
        studio.undo();
        assert_eq!(studio.doc.motion, keyed);
        studio.redo();
        apply_edit(
            &mut studio,
            Edit::Remap {
                id,
                value: 0.25,
                ease: Ease::Hold,
                remove: false,
                clear: false,
            },
        )
        .unwrap();
        assert_eq!(studio.history.len(), 4);
        assert_eq!(studio.doc.motion.compositions[0].local_time(1.), Some(0.25));
        let reopened =
            crate::project::decode(&crate::project::encode(&studio.doc).unwrap()).unwrap();
        assert_eq!(reopened.motion, studio.doc.motion);
        apply_edit(
            &mut studio,
            Edit::Expression {
                id,
                property: Prop::X,
                source: None,
            },
        )
        .unwrap();
        assert!(studio.doc.motion.expressions.is_empty());
        apply_edit(
            &mut studio,
            Edit::Remap {
                id,
                value: 0.,
                ease: Ease::Linear,
                remove: false,
                clear: true,
            },
        )
        .unwrap();
        assert!(studio.doc.motion.compositions[0].time_remap.is_empty());
    }
    #[test]
    fn invalid_composition_controls_and_unknown_ids_preserve_document_and_history() {
        let (mut studio, id) = studio();
        let before = studio.doc.motion.clone();
        let revision = studio.canvas_gen;
        for edit in [
            Edit::Key {
                id,
                property: Prop::Fill,
                value: 1.,
                ease: Ease::Linear,
                remove: false,
            },
            Edit::Key {
                id,
                property: Prop::Opacity,
                value: -0.1,
                ease: Ease::Linear,
                remove: false,
            },
            Edit::Key {
                id,
                property: Prop::X,
                value: f32::INFINITY,
                ease: Ease::Linear,
                remove: false,
            },
            Edit::Key {
                id: 0,
                property: Prop::X,
                value: 1.,
                ease: Ease::Linear,
                remove: false,
            },
            Edit::Expression {
                id,
                property: Prop::Fill,
                source: Some("time".into()),
            },
            Edit::Remap {
                id,
                value: f32::NAN,
                ease: Ease::Linear,
                remove: false,
                clear: false,
            },
        ] {
            assert!(apply_edit(&mut studio, edit).is_err());
            assert_eq!(studio.doc.motion, before);
            assert_eq!(studio.history.len(), 0);
            assert_eq!(studio.canvas_gen, revision);
        }
    }
}
