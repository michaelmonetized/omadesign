//! Editable scene controls beside the motion transport.
use crate::{
    app::Studio,
    scene3d::{
        Camera, Light, LightKind, Projection, Scene,
        model::{self, AlphaMode},
    },
};
use eframe::egui::{self, Ui};

fn triple(ui: &mut Ui, label: &str, values: &mut [f64; 3]) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.label(label);
        for value in values {
            changed |= ui.add(egui::DragValue::new(value).speed(0.05)).changed();
        }
    });
    changed
}

pub(super) fn show(ui: &mut Ui, studio: &mut Studio) {
    ui.menu_button("3D Scene", |ui| {
        egui::ScrollArea::vertical()
            .max_height(480.)
            .show(ui, |ui| {
                let mut motion = studio.doc.motion.clone();
                let scene = motion.scene3d.get_or_insert_with(Scene::default);
                let mut changed = false;
                if ui.button("Add model…").clicked()
                    && let Some(path) = rfd::FileDialog::new()
                        .add_filter("3D models", &["gltf", "glb", "obj", "stl", "fbx"])
                        .pick_file()
                {
                    match crate::scene3d::import_model(&path).and_then(|m| {
                        scene.add(
                            m,
                            path.file_stem()
                                .unwrap_or_default()
                                .to_string_lossy()
                                .into(),
                        )
                    }) {
                        Ok(_) => changed = true,
                        Err(e) => studio.status = e,
                    }
                }
                ui.horizontal(|ui| {
                    for kind in model::prim::PrimitiveKind::ALL {
                        if ui.small_button(kind.label()).clicked() {
                            let params = model::prim::PrimParams {
                                size: [2.; 3],
                                radius: 1.,
                                tube: 0.25,
                                height: 2.,
                                segments: 24,
                                rings: 16,
                            };
                            match scene.add(
                                model::prim::model(
                                    kind,
                                    &params,
                                    model::Material::plain([0.65, 0.3, 0.12, 1.], 0., 0.6),
                                ),
                                kind.label().into(),
                            ) {
                                Ok(_) => changed = true,
                                Err(e) => studio.status = e,
                            }
                        }
                    }
                });
                if scene.objects.is_empty() {
                    ui.label("Add a model or shape to start.");
                    return;
                }
                ui.collapsing("Camera", |ui| {
                    let selected = scene.imported_camera;
                    egui::ComboBox::from_id_salt("scene-camera")
                        .selected_text(if selected.is_some() {
                            "Model camera"
                        } else {
                            "Scene camera"
                        })
                        .show_ui(ui, |ui| {
                            changed |= ui
                                .selectable_value(&mut scene.imported_camera, None, "Scene camera")
                                .changed();
                            for object in &scene.objects {
                                for camera in object.model.placed(
                                    false,
                                    object.animation,
                                    object.local_time(studio.playhead as f64),
                                ) {
                                    changed |= ui
                                        .selectable_value(
                                            &mut scene.imported_camera,
                                            Some((object.id, camera.node)),
                                            format!(
                                                "{} · {}",
                                                object.name,
                                                object.model.cameras[camera.index].name
                                            ),
                                        )
                                        .changed();
                                }
                            }
                        });
                    if scene.imported_camera.is_none() {
                        changed |= triple(ui, "Position", &mut scene.camera.position);
                        changed |= triple(ui, "Look at", &mut scene.camera.target);
                        let perspective =
                            matches!(scene.camera.projection, Projection::Perspective { .. });
                        ui.horizontal(|ui| {
                            if ui.selectable_label(perspective, "Perspective").clicked()
                                && !perspective
                            {
                                scene.camera.projection = Projection::Perspective { fov: 45. };
                                changed = true;
                            }
                            if ui.selectable_label(!perspective, "Orthographic").clicked()
                                && perspective
                            {
                                scene.camera.projection = Projection::Orthographic { height: 4. };
                                changed = true;
                            }
                        });
                        match &mut scene.camera.projection {
                            Projection::Perspective { fov } => {
                                ui.horizontal(|ui| {
                                    ui.label("View angle");
                                    changed |= ui
                                        .add(
                                            egui::DragValue::new(fov)
                                                .range(1.0..=178.0)
                                                .suffix("°"),
                                        )
                                        .changed();
                                });
                            }
                            Projection::Orthographic { height } => {
                                ui.horizontal(|ui| {
                                    ui.label("View height");
                                    changed |= ui
                                        .add(
                                            egui::DragValue::new(height)
                                                .speed(0.05)
                                                .range(0.001..=1e9),
                                        )
                                        .changed();
                                });
                            }
                        }
                        ui.horizontal(|ui| {
                            ui.label("Near / far");
                            changed |= ui
                                .add(egui::DragValue::new(&mut scene.camera.near).speed(0.01))
                                .changed();
                            changed |= ui
                                .add(egui::DragValue::new(&mut scene.camera.far).speed(1.))
                                .changed();
                        });
                        if ui.button("Reset camera").clicked() {
                            scene.camera = Camera::default();
                            changed = true;
                        }
                    }
                });
                ui.collapsing("Lighting", |ui| {
                    changed |= triple(ui, "Ambient", &mut scene.ambient);
                    changed |= ui
                        .checkbox(&mut scene.imported_lights, "Use model lights")
                        .changed();
                    if ui.button("Add light").clicked() {
                        scene.lights.push(Light {
                            kind: LightKind::Directional,
                            position: [0., 2., 3.],
                            direction: [0., 0., -1.],
                            color: [1.; 3],
                            intensity: 2.,
                            range: None,
                        });
                        changed = true;
                    }
                    let mut remove = None;
                    for (index, light) in scene.lights.iter_mut().enumerate() {
                        ui.push_id(index, |ui| {
                            ui.collapsing(format!("Light {}", index + 1), |ui| {
                                egui::ComboBox::from_id_salt("kind")
                                    .selected_text(match light.kind {
                                        LightKind::Directional => "Directional",
                                        LightKind::Point => "Point",
                                        LightKind::Spot { .. } => "Spot",
                                    })
                                    .show_ui(ui, |ui| {
                                        changed |= ui
                                            .selectable_value(
                                                &mut light.kind,
                                                LightKind::Directional,
                                                "Directional",
                                            )
                                            .changed();
                                        changed |= ui
                                            .selectable_value(
                                                &mut light.kind,
                                                LightKind::Point,
                                                "Point",
                                            )
                                            .changed();
                                        if ui
                                            .selectable_label(
                                                matches!(light.kind, LightKind::Spot { .. }),
                                                "Spot",
                                            )
                                            .clicked()
                                        {
                                            light.kind = LightKind::Spot {
                                                inner: 0.3,
                                                outer: 0.6,
                                            };
                                            changed = true;
                                        }
                                    });
                                changed |= triple(ui, "Position", &mut light.position);
                                changed |= triple(ui, "Direction", &mut light.direction);
                                changed |= triple(ui, "Color", &mut light.color);
                                ui.horizontal(|ui| {
                                    ui.label("Brightness");
                                    changed |= ui
                                        .add(
                                            egui::DragValue::new(&mut light.intensity)
                                                .speed(0.05)
                                                .range(0.0..=1e9),
                                        )
                                        .changed();
                                });
                                if ui.button("Remove light").clicked() {
                                    remove = Some(index);
                                }
                            });
                        });
                    }
                    if let Some(i) = remove {
                        scene.lights.remove(i);
                        changed = true;
                    }
                });
                let mut remove = None;
                for object in &mut scene.objects {
                    ui.push_id(object.id, |ui| {
                        ui.collapsing(object.name.clone(), |ui| {
                            changed |= ui.checkbox(&mut object.visible, "Visible").changed();
                            changed |= ui.text_edit_singleline(&mut object.name).changed();
                            ui.label(format!(
                                "{} vertices · {} triangles",
                                object.model.vertex_count(),
                                object.model.triangle_count()
                            ));
                            changed |= triple(ui, "Position", &mut object.translation);
                            changed |= triple(ui, "Rotate °", &mut object.rotation);
                            changed |= triple(ui, "Scale", &mut object.scale);
                            changed |= ui
                                .add(
                                    egui::Slider::new(&mut object.opacity, 0.0..=1.0)
                                        .text("Opacity"),
                                )
                                .changed();
                            egui::ComboBox::from_id_salt("clip")
                                .selected_text(
                                    object
                                        .animation
                                        .and_then(|i| object.model.animations.get(i))
                                        .map_or("Rest pose", |a| a.name.as_str()),
                                )
                                .show_ui(ui, |ui| {
                                    changed |= ui
                                        .selectable_value(&mut object.animation, None, "Rest pose")
                                        .changed();
                                    for (i, clip) in object.model.animations.iter().enumerate() {
                                        changed |= ui
                                            .selectable_value(
                                                &mut object.animation,
                                                Some(i),
                                                &clip.name,
                                            )
                                            .changed();
                                    }
                                });
                            if object.animation.is_some() {
                                changed |= ui.checkbox(&mut object.looped, "Loop clip").changed();
                                ui.horizontal(|ui| {
                                    ui.label("Clip offset / speed");
                                    changed |= ui
                                        .add(
                                            egui::DragValue::new(&mut object.time_offset)
                                                .speed(0.05),
                                        )
                                        .changed();
                                    changed |= ui
                                        .add(
                                            egui::DragValue::new(&mut object.time_scale)
                                                .speed(0.05),
                                        )
                                        .changed();
                                });
                            }
                            ui.collapsing("Node", |ui| {
                                if object.model.nodes.is_empty() {
                                    return;
                                }
                                let key = ui.id().with("node");
                                let mut index = ui
                                    .data(|d| d.get_temp::<usize>(key))
                                    .unwrap_or(0)
                                    .min(object.model.nodes.len() - 1);
                                ui.add(
                                    egui::DragValue::new(&mut index)
                                        .range(0..=object.model.nodes.len() - 1),
                                );
                                ui.data_mut(|d| d.insert_temp(key, index));
                                let mut node = object.model.nodes[index].clone();
                                ui.label(&node.name);
                        if node.matrix.is_some() {ui.label("Use the model position, rotation and scale controls for this node.");return;}
                                let mut node_changed =
                                    triple(ui, "Position", &mut node.translation);
                                node_changed |= triple(ui, "Scale", &mut node.scale);
                                ui.horizontal(|ui| {
                                    ui.label("Rotation xyzw");
                                    for v in &mut node.rotation {
                                        node_changed |=
                                            ui.add(egui::DragValue::new(v).speed(0.01)).changed();
                                    }
                                });
                                if node_changed {
                                    node.matrix = None;
                                    std::sync::Arc::make_mut(&mut object.model).nodes[index] = node;
                                    changed = true;
                                }
                            });
                            ui.collapsing("Material", |ui| {
                                if object.model.materials.is_empty() {
                                    ui.label("Default material");
                                    return;
                                }
                                let key = ui.id().with("material");
                                let mut index = ui
                                    .data(|d| d.get_temp::<usize>(key))
                                    .unwrap_or(0)
                                    .min(object.model.materials.len() - 1);
                                ui.add(
                                    egui::DragValue::new(&mut index)
                                        .range(0..=object.model.materials.len() - 1),
                                );
                                ui.data_mut(|d| d.insert_temp(key, index));
                                let mut m = object.model.materials[index].clone();
                                ui.label(&m.name);
                                let mut material_changed = false;
                                ui.horizontal(|ui| {
                                    ui.label("Color / alpha");
                                    for v in &mut m.base_color {
                                        material_changed |= ui
                                            .add(
                                                egui::DragValue::new(v)
                                                    .speed(0.01)
                                                    .range(0.0..=1.0),
                                            )
                                            .changed();
                                    }
                                });
                                material_changed |= ui
                                    .add(
                                        egui::Slider::new(&mut m.metallic, 0.0..=1.0)
                                            .text("Metallic"),
                                    )
                                    .changed();
                                material_changed |= ui
                                    .add(
                                        egui::Slider::new(&mut m.roughness, 0.0..=1.0)
                                            .text("Roughness"),
                                    )
                                    .changed();
                                material_changed |= ui.checkbox(&mut m.unlit, "Unlit").changed();
                                material_changed |=
                                    ui.checkbox(&mut m.double_sided, "Both sides").changed();
                                egui::ComboBox::from_id_salt("alpha")
                                    .selected_text(format!("{:?}", m.alpha_mode))
                                    .show_ui(ui, |ui| {
                                        for mode in
                                            [AlphaMode::Opaque, AlphaMode::Mask, AlphaMode::Blend]
                                        {
                                            material_changed |= ui
                                                .selectable_value(
                                                    &mut m.alpha_mode,
                                                    mode,
                                                    format!("{mode:?}"),
                                                )
                                                .changed();
                                        }
                                    });
                                if m.alpha_mode == AlphaMode::Mask {
                                    material_changed |= ui
                                        .add(
                                            egui::Slider::new(&mut m.alpha_cutoff, 0.0..=1.0)
                                                .text("Cutoff"),
                                        )
                                        .changed();
                                }
                                if material_changed {
                                    std::sync::Arc::make_mut(&mut object.model).materials[index] =
                                        m;
                                    changed = true;
                                }
                            });
                            if ui.button("Remove model").clicked() {
                                remove = Some(object.id);
                            }
                        });
                    });
                }
                if let Some(id) = remove {
                    scene.objects.retain(|o| o.id != id);
                    if scene.imported_camera.is_some_and(|(o, _)| o == id) {
                        scene.imported_camera = None;
                    }
                    changed = true;
                }
                if changed {
                    motion.duration = scene
                        .objects
                        .iter()
                        .flat_map(|o| &o.model.animations)
                        .map(|a| a.duration() as f32)
                        .fold(motion.duration, f32::max);
                    let mut candidate = studio.doc.clone();
                    candidate.motion = motion.clone();
                    match crate::motion::compositions::validate(&candidate) {
                        Ok(()) => studio.commit_motion_step(motion),
                        Err(e) => studio.status = e,
                    }
                }
                if let Some(error) = crate::compositor::motion_error() {
                    ui.colored_label(egui::Color32::LIGHT_RED, error);
                }
            });
    });
}
