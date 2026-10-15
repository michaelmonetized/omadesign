//! Shared-selection fields retain the document's typed batch command path.
use super::fields::{self, FieldState};
use crate::{
    app::Studio,
    document::{Cmd, RASTER_ID},
    geom::{Bounds, Pt},
};
use eframe::egui::Ui;

/// Edit a mixed vector selection without changing untouched properties.
/// Args: panel UI and studio. Returns true when the selection was handled.
pub(super) fn mixed(ui: &mut Ui, studio: &mut Studio) -> bool {
    studio.with_field_history(ui, "mixed-transform", |ui, studio| mixed_contents(ui, studio))
}

fn mixed_contents(ui: &mut Ui, studio: &mut Studio) -> bool {
    if studio.selection.len() < 2 {
        return false;
    }
    let shapes: Vec<_> = studio
        .selection
        .iter()
        .filter_map(|&(layer, id)| {
            if id == RASTER_ID {
                return None;
            }
            studio
                .doc
                .find_shape(layer, id)
                .map(|shape| (layer, shape.clone()))
        })
        .collect();
    if shapes.len() != studio.selection.len() {
        ui.label("Select only vector objects to edit shared geometry");
        return true;
    }
    let values: Vec<[f32; 5]> = shapes
        .iter()
        .map(|(_, shape)| {
            let bounds = shape.geom.bbox();
            let pose = studio.live_pose(shape.id);
            let bounds = if studio.is_motion() {
                pose.map_bounds(shape.world_bbox())
            } else {
                bounds
            };
            [
                bounds.min.x,
                bounds.min.y,
                bounds.width(),
                bounds.height(),
                (shape.rotation
                    + if studio.is_motion() {
                        pose.rotation
                    } else {
                        0.0
                    })
                .to_degrees(),
            ]
        })
        .collect();
    let mut after = values[0];
    let mut changed = [false; 5];
    for (index, (label, unit, range, reset)) in [
        ("X", " px", -100000.0..=100000.0, Some(0.0)),
        ("Y", " px", -100000.0..=100000.0, Some(0.0)),
        ("Width", " px", 1.0..=20000.0, None),
        ("Height", " px", 1.0..=20000.0, None),
        ("Rotation", "°", -180.0..=180.0, Some(0.0)),
    ]
    .into_iter()
    .enumerate()
    {
        let state = FieldState {
            mixed: values
                .iter()
                .any(|value| (value[index] - after[index]).abs() > 0.001),
            ..Default::default()
        };
        let field = fields::number(
            ui,
            ("mixed-transform", index),
            label,
            &mut after[index],
            range,
            unit,
            reset,
            state,
        );
        changed[index] = crate::app::input_history::record(field.response, field.changed);
    }
    apply(studio, after, changed);
    true
}

/// Apply only edited shared geometry through one typed undo command.
/// Args: studio, desired X/Y/width/height/rotation and changed-property flags.
/// Returns whether an editable selection changed; locked or invalid edits leave artwork untouched.
fn apply(studio: &mut Studio, after: [f32; 5], changed: [bool; 5]) -> bool {
    if !changed.iter().any(|changed| *changed) || !after.iter().all(|value| value.is_finite()) {
        return false;
    }
    let shapes: Vec<_> = studio
        .selection
        .iter()
        .filter_map(|&(layer, id)| {
            studio
                .doc
                .find_shape(layer, id)
                .cloned()
                .map(|shape| (layer, shape))
        })
        .collect();
    if shapes.is_empty()
        || shapes.len() != studio.selection.len()
        || shapes
            .iter()
            .any(|(layer, shape)| !studio.layer_unlocked(*layer) || shape.locked)
    {
        return false;
    }
    if studio.is_motion() {
        let mut motion = studio.doc.motion.clone();
        for (_, shape) in shapes {
            let bounds = shape.world_bbox();
            let pose = studio.live_pose(shape.id);
            let shown = pose.map_bounds(bounds);
            for (index, prop, value) in [
                (0, crate::motion::Prop::X, pose.dx + after[0] - shown.min.x),
                (1, crate::motion::Prop::Y, pose.dy + after[1] - shown.min.y),
                (
                    2,
                    crate::motion::Prop::Width,
                    pose.width_scale * after[2] / shown.width().max(1.0),
                ),
                (
                    3,
                    crate::motion::Prop::Height,
                    pose.height_scale * after[3] / shown.height().max(1.0),
                ),
                (
                    4,
                    crate::motion::Prop::Rotation,
                    after[4].to_radians() - shape.rotation,
                ),
            ] {
                if changed[index] {
                    motion.set_key(
                        shape.id,
                        prop,
                        studio.playhead,
                        value,
                        crate::motion::Ease::EaseInOut,
                    );
                }
            }
        }
        studio.commit_motion(motion);
    } else {
        let resize_frames = changed[..4].iter().any(|changed| *changed)
            && shapes.iter().any(|(_, shape)| shape.layout.frame);
        let mut commands: Vec<Cmd> = shapes
            .into_iter()
            .map(|(layer, shape)| {
                let bounds = shape.geom.bbox();
                let mut value = [
                    bounds.min.x,
                    bounds.min.y,
                    bounds.width(),
                    bounds.height(),
                    shape.rotation.to_degrees(),
                ];
                for index in 0..5 {
                    if changed[index] {
                        value[index] = after[index];
                    }
                }
                let before = shape.geom.clone();
                let mut geometry = before.clone();
                geometry.map_into_with_text_scale(
                    before.bbox(),
                    Bounds::from_min_size(Pt::new(value[0], value[1]), Pt::new(value[2], value[3])),
                );
                Cmd::SetGeom {
                    layer,
                    id: shape.id,
                    before,
                    after: geometry,
                    rot_before: shape.rotation,
                    rot_after: value[4].to_radians(),
                }
            })
            .collect();
        if resize_frames {
            let mut resized = studio.doc.layout_snapshot();
            let original: Vec<_> = resized
                .layers
                .iter()
                .enumerate()
                .flat_map(|(layer, row)| {
                    row.kind
                        .shapes()
                        .unwrap_or(&[])
                        .iter()
                        .map(move |shape| (layer, shape.id, shape.geom.clone()))
                })
                .collect();
            for command in &commands {
                crate::document::apply(&mut resized, command);
            }
            crate::layout::apply_resize(&mut resized, &original, &studio.selection);
            commands = studio
                .doc
                .layers
                .iter()
                .zip(&resized.layers)
                .enumerate()
                .filter_map(|(layer, (before, after))| {
                    let (before, after) = (before.kind.shapes()?, after.kind.shapes()?);
                    (before != after).then(|| Cmd::SetVectorShapes {
                        layer,
                        before: before.to_vec(),
                        after: after.to_vec(),
                    })
                })
                .collect();
        }
        studio.commit(Cmd::Batch(commands));
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> Studio {
        let mut studio = Studio::new();
        studio.doc = crate::document::Document::new("Shared fields", 800.0, 600.0, 72.0);
        let mut layer = crate::document::Layer::vector("Artwork");
        for x in [20.0, 220.0] {
            let shape = crate::document::Shape::new(
                crate::geom::Geom::Rect {
                    origin: Pt::new(x, 30.0),
                    size: Pt::new(100.0, 80.0),
                    radius: 0.0,
                },
                Default::default(),
            );
            studio.selection.push((0, shape.id));
            layer.kind.shapes_mut().unwrap().push(shape);
        }
        studio.doc.layers = vec![layer];
        studio.active_layer = Some(0);
        studio.show_welcome = false;
        studio
    }
    #[test]
    fn shared_transform_preserves_mixed_values_undo_and_native_roundtrip() {
        let mut studio = fixture();
        let before = studio.doc.clone();
        assert!(apply(
            &mut studio,
            [0.0, 50.0, 100.0, 80.0, 0.0],
            [false, true, false, false, false]
        ));
        let positions: Vec<_> = studio.doc.layers[0]
            .kind
            .shapes()
            .unwrap()
            .iter()
            .map(|s| s.geom.bbox().min)
            .collect();
        assert_eq!(positions, vec![Pt::new(20.0, 50.0), Pt::new(220.0, 50.0)]);
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join(".artifacts")
            .join(format!("shared-ui-document-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("fields.oma");
        crate::project::save_to(&studio.doc, &path).unwrap();
        let reopened = crate::project::load_from(&path).unwrap();
        let raw: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert!(raw.get("doc").is_some());
        std::fs::remove_dir_all(root).unwrap();
        assert_eq!(
            reopened.layers[0].kind.shapes().unwrap()[1].geom.bbox().min,
            Pt::new(220.0, 50.0)
        );
        studio.undo();
        assert_eq!(
            studio.doc.layers[0].kind.shapes().unwrap()[0].geom,
            before.layers[0].kind.shapes().unwrap()[0].geom
        );
        studio.redo();
        assert_eq!(
            studio.doc.layers[0].kind.shapes().unwrap()[0]
                .geom
                .bbox()
                .min
                .y,
            50.0
        );
        studio.doc.layers[0].locked = true;
        let encoded = serde_json::to_vec(&studio.doc).unwrap();
        assert!(!apply(
            &mut studio,
            [0.0, 90.0, 100.0, 80.0, 0.0],
            [false, true, false, false, false]
        ));
        assert_eq!(encoded, serde_json::to_vec(&studio.doc).unwrap());
    }
    #[test]
    fn shared_frame_resize_keeps_child_constraints_and_one_batch_undo() {
        let mut studio = fixture();
        let frames = studio.selection.clone();
        let mut children = Vec::new();
        for (layer, id) in frames {
            let frame = studio.doc.find_shape_mut(layer, id).unwrap();
            frame.layout.frame = true;
            let min = frame.geom.bbox().min;
            let mut child = crate::document::Shape::new(
                crate::geom::Geom::Rect {
                    origin: min + Pt::new(10.0, 10.0),
                    size: Pt::new(20.0, 20.0),
                    radius: 0.0,
                },
                Default::default(),
            );
            child.layout.parent = Some(id);
            child.layout.constraint_x = crate::layout::Constraint::Stretch;
            children.push(child.id);
            studio.doc.layers[layer]
                .kind
                .shapes_mut()
                .unwrap()
                .push(child);
        }
        let before = crate::project::encode(&studio.doc).unwrap();
        assert!(apply(
            &mut studio,
            [0.0, 0.0, 200.0, 80.0, 0.0],
            [false, false, true, false, false]
        ));
        for id in children {
            assert_eq!(
                studio.doc.find_shape(0, id).unwrap().geom.bbox().width(),
                120.0
            );
        }
        studio.undo();
        assert_eq!(before, crate::project::encode(&studio.doc).unwrap());
    }
}

#[cfg(test)]
mod idle_render_tests {
    use super::*;
    #[test]
    fn shared_large_single_mixed_and_locked_selection_never_edits_on_render() {
        let svg = r#"<svg xmlns="http://www.w3.org/2000/svg"
            width="3000000" height="2000000" viewBox="0 0 3000000 2000000">
            <rect x="12000000" y="-6000000" width="2000000" height="800000"/>
            <rect x="12000100" y="-6000000" width="2000000" height="800000"/>
            </svg>"#;
        let document = crate::formats::svg::read(svg, "Large imported artwork")
            .unwrap()
            .0;
        assert_eq!(
            (document.width, document.height),
            (3_000_000.0, 2_000_000.0)
        );
        let imported: Vec<_> = document
            .layers
            .iter()
            .enumerate()
            .flat_map(|(layer, row)| {
                row.kind
                    .shapes()
                    .unwrap_or(&[])
                    .iter()
                    .map(move |shape| (layer, shape.id))
            })
            .collect();
        assert_eq!(imported.len(), 2);
        for count in [0, 1, 2] {
            for locked in [false, true] {
                let mut studio = Studio::new();
                studio.show_welcome = false;
                studio.doc = document.clone();
                for (index, &(layer, id)) in imported.iter().enumerate() {
                    let shape = studio.doc.find_shape_mut(layer, id).unwrap();
                    let bounds = shape.geom.bbox();
                    assert_eq!(
                        bounds.min,
                        Pt::new(12_000_000.0 + index as f32 * 100.0, -6_000_000.0)
                    );
                    assert_eq!((bounds.width(), bounds.height()), (2_000_000.0, 800_000.0));
                    shape.rotation = (720.0 + index as f32 * 30.0).to_radians();
                    if index < count {
                        studio.selection.push((layer, id));
                    }
                    studio.doc.layers[layer].locked = locked;
                }
                studio.active_layer = Some(imported[0].0);
                let before = crate::project::encode(&studio.doc).unwrap();
                let ctx = eframe::egui::Context::default();
                for _ in 0..3 {
                    let _ = crate::ui::shared_fields_tests::headless(
                        &ctx,
                        eframe::egui::RawInput {
                            screen_rect: Some(eframe::egui::Rect::from_min_size(
                                eframe::egui::Pos2::ZERO,
                                eframe::egui::vec2(1280.0, 900.0),
                            )),
                            ..Default::default()
                        },
                        |ui| super::super::studios::right_panel(ui, &mut studio),
                    );
                }
                assert_eq!(
                    before,
                    crate::project::encode(&studio.doc).unwrap(),
                    "selection={count} locked={locked}"
                );
                assert!(!studio.dirty);
            }
        }
    }
}
