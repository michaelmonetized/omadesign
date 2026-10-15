//! Editable scene controls beside the motion transport.
use crate::{
    app::Studio,
    scene3d::{
        Camera, Light, LightKind, Projection, Scene,
        model::{self, AlphaMode},
    },
};
use eframe::egui::{self, Id, Response, Ui};
use std::{cell::RefCell, ops::RangeInclusive};

#[derive(Clone, Default)]
struct RowOwners {
    source_generation: u64,
    light_generation: u64,
    revision: u64,
    widgets: Vec<Id>,
    light_widgets: Vec<Id>,
}

impl RowOwners {
    /// Retire native rows when their expected document revision is replaced.
    /// Args: UI context. Returns nothing; only focus owned by retired rows is released.
    fn retire_source(&mut self, ctx: &egui::Context) {
        if let Some(focused) = ctx.memory(|memory| memory.focused())
            && self.widgets.contains(&focused)
        {
            ctx.memory_mut(|memory| memory.surrender_focus(focused));
        }
        self.widgets.clear();
        self.light_widgets.clear();
        self.source_generation = self.source_generation.wrapping_add(1);
    }

    /// Retire only native light rows after an accepted light-list edit.
    /// Args: UI context. Returns nothing; concrete camera and object owners remain unchanged.
    fn retire_lights(&mut self, ctx: &egui::Context) {
        if let Some(focused) = ctx.memory(|memory| memory.focused())
            && self.light_widgets.contains(&focused)
        {
            ctx.memory_mut(|memory| memory.surrender_focus(focused));
        }
        self.widgets.retain(|id| !self.light_widgets.contains(id));
        self.light_widgets.clear();
        self.light_generation = self.light_generation.wrapping_add(1);
    }
}

#[derive(Clone)]
struct Rejection {
    fields: Vec<Id>,
    message: String,
}

#[derive(Default)]
struct Feedback {
    widgets: Vec<Response>,
    light_widgets: Vec<Id>,
    edited: Vec<(Domain, Response)>,
    domain: Domain,
}

#[derive(Clone, Copy, Default, PartialEq)]
enum Domain {
    #[default]
    Scene,
    Camera,
    Light,
    Object,
    Resource,
}

impl Feedback {
    /// Retain actual responses for native focus retirement and known candidate errors.
    /// Args: native response and accepted change. Returns the accepted change unchanged.
    fn observe(&mut self, response: &Response, changed: bool) -> bool {
        self.widgets.push(response.clone());
        if changed {
            self.edited.push((self.domain, response.clone()));
        }
        changed
    }
}

/// Name and focus an existing native widget in its semantic property scope.
/// Args: UI, stable owner, accessible name and original widget. Returns its actual response.
fn control(
    ui: &mut Ui,
    owner: Id,
    name: &str,
    widget: impl FnOnce(&mut Ui) -> Response,
) -> Response {
    let response = super::fields::native_scope(ui, owner, widget).inner;
    ui.ctx()
        .accesskit_node_builder(response.id, |node| node.set_label(name));
    if let Some(native) = ui.ctx().read_response(response.id) {
        super::welcome::focus_control(ui, &native);
    }
    response
}

/// Decorate a native numeric response without changing its precision or command ownership.
/// Args: UI, response, owner, observed parser attempt, before/current values and edit range.
/// Returns authoritative accepted change, commit and cancel signals for this actual widget.
fn numeric_response<N: egui::emath::Numeric>(
    ui: &mut Ui,
    response: Response,
    owner: Id,
    attempted: Option<String>,
    before: N,
    value: &mut N,
    range: RangeInclusive<f64>,
    events: &[egui::Event],
) -> super::fields::FieldResponse {
    let cancelled = (response.has_focus() || response.lost_focus() || response.dragged())
        && ui.input(|input| input.key_pressed(egui::Key::Escape));
    let edited = response.dragged()
        || response.is_pointer_button_down_on()
        || response.clicked()
        || events.iter().any(|event| match event {
            egui::Event::Text(_)
            | egui::Event::Paste(_)
            | egui::Event::Cut
            | egui::Event::Ime(_) => response.has_focus() || response.lost_focus(),
            egui::Event::Key {
                key, pressed: true, ..
            } => {
                response.has_focus()
                    && !matches!(key, egui::Key::Tab | egui::Key::Enter | egui::Key::Escape)
            }
            egui::Event::AccessKitActionRequest(request) => {
                request.target_node == response.id.accesskit_id()
                    && matches!(
                        request.action,
                        egui::accesskit::Action::SetValue
                            | egui::accesskit::Action::Increment
                            | egui::accesskit::Action::Decrement
                    )
            }
            _ => false,
        });
    let range = if edited && response.changed() {
        range
    } else {
        f64::NEG_INFINITY..=f64::INFINITY
    };
    let valid =
        super::welcome::native_number_valid(ui, &response, owner, attempted, value.to_f64(), range);
    if !valid || cancelled || !response.enabled() || !edited {
        *value = before;
    }
    if !valid && response.enabled() {
        super::welcome::control_error(
            ui,
            &[&response],
            "Enter a finite number within the shown range",
        );
    }
    super::keyboard_controls::number_focus(ui, &response);
    super::fields::FieldResponse {
        changed: response.enabled() && edited && valid && !cancelled && response.changed(),
        committed: response.enabled()
            && valid
            && !cancelled
            && (response.lost_focus() || response.drag_stopped()),
        cancelled,
        response,
    }
}

/// Draw the original native number with its original type, speed, bounds and suffix.
/// Args: UI, property owner/name, value and native options. Returns its accepted field response.
fn number<N: egui::emath::Numeric>(
    ui: &mut Ui,
    owner: Id,
    name: &str,
    value: &mut N,
    speed: Option<f64>,
    range: Option<RangeInclusive<N>>,
    suffix: &str,
) -> super::fields::FieldResponse {
    let before = *value;
    let events = ui.input(|input| input.events.clone());
    let attempted = RefCell::new(None);
    let limits = range
        .as_ref()
        .map_or(f64::NEG_INFINITY..=f64::INFINITY, |range| {
            range.start().to_f64()..=range.end().to_f64()
        });
    let response = control(ui, owner, name, |ui| {
        let mut widget = egui::DragValue::new(value)
            .clamp_existing_to_range(false)
            .suffix(suffix)
            .custom_parser(|text| super::welcome::parse_number(&attempted, text));
        if let Some(speed) = speed {
            widget = widget.speed(speed);
        }
        if let Some(range) = range {
            widget = widget.range(range);
        }
        ui.add(widget)
    });
    numeric_response(
        ui,
        response,
        owner,
        attempted.into_inner(),
        before,
        value,
        limits,
        &events,
    )
}

/// Draw the existing unit slider and its native number editor without loading-time clamping.
/// Args: UI, owner, contextual name, visible caption and value. Returns accepted native edits.
fn slider(
    ui: &mut Ui,
    owner: Id,
    name: &str,
    label: &str,
    value: &mut f32,
) -> super::fields::FieldResponse {
    let before = *value;
    let events = ui.input(|input| input.events.clone());
    let attempted = RefCell::new(None);
    let response = control(ui, owner, name, |ui| {
        ui.add(
            egui::Slider::new(value, 0.0..=1.0)
                .clamping(egui::SliderClamping::Edits)
                .text(label)
                .custom_parser(|text| super::welcome::parse_number(&attempted, text)),
        )
    });
    numeric_response(
        ui,
        response,
        owner,
        attempted.into_inner(),
        before,
        value,
        0.0..=1.0,
        &events,
    )
}

/// Keep a native section's identity independent of its editable caption and earlier rows.
/// Args: UI, stable section owner, caption, accessible name and content. Returns no value.
fn section(
    ui: &mut Ui,
    owner: Id,
    label: &str,
    name: &str,
    feedback: &mut Feedback,
    body: impl FnOnce(&mut Ui, &mut Feedback),
) {
    let response = super::fields::native_scope(ui, owner, |ui| {
        egui::CollapsingHeader::new(label)
            .id_salt("header")
            .show(ui, |ui| body(ui, feedback))
    })
    .inner
    .header_response;
    ui.ctx()
        .accesskit_node_builder(response.id, |node| node.set_label(name));
    super::welcome::focus_control(ui, &response);
    feedback.observe(&response, false);
}

/// Name each native vector component while keeping the original unbounded f64 controls.
/// Args: UI, property identity, caption, contextual name, values and feedback. Returns accepted changes.
fn triple(
    ui: &mut Ui,
    owner: Id,
    label: &str,
    name: &str,
    axes: [&str; 3],
    values: &mut [f64; 3],
    feedback: &mut Feedback,
) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        let label = ui.label(label);
        for (axis, value) in axes.into_iter().zip(values) {
            let field = number(
                ui,
                owner.with(axis),
                &format!("{name} {axis}"),
                value,
                Some(0.05),
                None,
                "",
            );
            let _ = field.response.clone().labelled_by(label.id);
            changed |= feedback.observe(&field.response, field.changed);
        }
    });
    changed
}

pub(super) fn show(ui: &mut Ui, studio: &mut Studio) {
    let document = Id::new(("scene3d", &studio.swap_id));
    let registry = document.with("row-owners");
    let error_key = document.with("candidate-error");
    let mut rows = ui
        .data(|data| data.get_temp::<RowOwners>(registry))
        .unwrap_or_else(|| RowOwners {
            revision: studio.canvas_gen,
            ..Default::default()
        });
    if rows.revision != studio.canvas_gen {
        rows.retire_source(ui.ctx());
        ui.data_mut(|data| data.remove::<Rejection>(error_key));
    }
    let owner = document.with(("source", rows.source_generation));
    if let Some(rejection) = ui.data(|data| data.get_temp::<Rejection>(error_key)) {
        let replacing = ui.input(|input| {
            input.events.iter().any(|event| {
                matches!(
                    event,
                    egui::Event::Text(_)
                        | egui::Event::Paste(_)
                        | egui::Event::Cut
                        | egui::Event::Key {
                            key: egui::Key::Escape
                                | egui::Key::Backspace
                                | egui::Key::Delete
                                | egui::Key::ArrowUp
                                | egui::Key::ArrowDown,
                            pressed: true,
                            ..
                        }
                )
            })
        });
        if replacing
            && ui
                .memory(|memory| memory.focused())
                .is_some_and(|id| rejection.fields.contains(&id))
        {
            ui.data_mut(|data| data.remove::<Rejection>(error_key));
        }
    }
    let mut feedback = Feedback::default();
    let mut structural = false;
    let available = !studio.file_dialog_pending() && !studio.updates.freezing;
    let menu = super::fields::native_scope(ui, document.with("menu"), |ui| {
        ui.add_enabled_ui(available, |ui| ui.menu_button("3D Scene", |ui| {
            super::fields::native_scope(ui, owner, |ui| {
                if !available { ui.disable(); }
                egui::ScrollArea::vertical().max_height(480.).show(ui, |ui| {
                    let mut motion = studio.doc.motion.clone();
                    let scene = motion.scene3d.get_or_insert_with(Scene::default);
                    let light_count = scene.lights.len();
                    let mut light_structure = false;
                    let mut changed = false;
                    let import = control(ui, owner.with("import"), "Add 3D model", |ui| ui.button("Add model…"));
                    feedback.observe(&import, false);
                    if import.clicked()
                        && let Some(path) = rfd::FileDialog::new()
                            .add_filter("3D models", &["gltf", "glb", "obj", "stl", "fbx"])
                            .pick_file()
                    {
                        match crate::scene3d::import_model(&path).and_then(|m| {
                            scene.add(m, path.file_stem().unwrap_or_default().to_string_lossy().into())
                        }) {
                            Ok(_) => changed = true,
                            Err(e) => {
                                super::welcome::control_error(ui, &[&import], &e);
                                studio.status = e;
                            }
                        }
                    }
                    ui.horizontal_wrapped(|ui| {
                        for kind in model::prim::PrimitiveKind::ALL {
                            let button = control(ui, owner.with(("primitive", kind)), kind.label(), |ui| ui.small_button(kind.label()));
                            feedback.observe(&button, false);
                            if button.clicked() {
                                let params = model::prim::PrimParams {
                                    size: [2.; 3], radius: 1., tube: 0.25, height: 2., segments: 24, rings: 16,
                                };
                                match scene.add(
                                    model::prim::model(kind, &params, model::Material::plain([0.65, 0.3, 0.12, 1.], 0., 0.6)),
                                    kind.label().into(),
                                ) {
                                    Ok(_) => changed = true,
                                    Err(e) => {
                                        super::welcome::control_error(ui, &[&button], &e);
                                        studio.status = e;
                                    }
                                }
                            }
                        }
                    });
                    if scene.objects.is_empty() {
                        ui.label("Add a model or shape to start.");
                        return;
                    }
                    let camera_owner = owner.with("camera");
                    section(ui, camera_owner, "Camera", "Scene camera controls", &mut feedback, |ui, feedback| {
                        feedback.domain = Domain::Camera;
                        let selected = scene.imported_camera;
                        let mut chosen = false;
                        let picker = control(ui, camera_owner.with("source"), "Scene camera source", |ui| {
                            egui::ComboBox::from_id_salt("value")
                                .selected_text(if selected.is_some() { "Model camera" } else { "Scene camera" })
                                .show_ui(ui, |ui| {
                                    let response = control(ui, camera_owner.with("scene-camera"), "Scene camera", |ui| ui.selectable_value(&mut scene.imported_camera, None, "Scene camera"));
                                    chosen |= response.clicked();
                                    changed |= feedback.observe(&response, response.changed());
                                    for object in &scene.objects {
                                        for camera in object.model.placed(false, object.animation, object.local_time(studio.playhead as f64)) {
                                            let name = format!("{} · {}", object.name, object.model.cameras[camera.index].name);
                                            let response = control(ui, camera_owner.with(("imported", object.id, camera.node)), &name, |ui| ui.selectable_value(&mut scene.imported_camera, Some((object.id, camera.node)), &name));
                                            chosen |= response.clicked();
                                            changed |= feedback.observe(&response, response.changed());
                                        }
                                    }
                                    if chosen { ui.close(); }
                                }).response
                        });
                        if chosen { picker.request_focus(); }
                        feedback.observe(&picker, false);
                        if scene.imported_camera.is_none() {
                            changed |= triple(ui, camera_owner.with("position"), "Position", "Scene camera position", ["X", "Y", "Z"], &mut scene.camera.position, feedback);
                            changed |= triple(ui, camera_owner.with("target"), "Look at", "Scene camera look at", ["X", "Y", "Z"], &mut scene.camera.target, feedback);
                            let perspective = matches!(scene.camera.projection, Projection::Perspective { .. });
                            ui.horizontal(|ui| {
                                let response = control(ui, camera_owner.with("perspective"), "Scene camera perspective", |ui| ui.selectable_label(perspective, "Perspective"));
                                feedback.observe(&response, false);
                                if response.clicked() && !perspective {
                                    scene.camera.projection = Projection::Perspective { fov: 45. };
                                    changed = true;
                                }
                                let response = control(ui, camera_owner.with("orthographic"), "Scene camera orthographic", |ui| ui.selectable_label(!perspective, "Orthographic"));
                                feedback.observe(&response, false);
                                if response.clicked() && perspective {
                                    scene.camera.projection = Projection::Orthographic { height: 4. };
                                    changed = true;
                                }
                            });
                            match &mut scene.camera.projection {
                                Projection::Perspective { fov } => {
                                    ui.horizontal(|ui| {
                                        ui.label("View angle");
                                        let field = number(ui, camera_owner.with("fov"), "Scene camera view angle (degrees)", fov, None, Some(1.0..=178.0), "°");
                                        changed |= feedback.observe(&field.response, field.changed);
                                    });
                                }
                                Projection::Orthographic { height } => {
                                    ui.horizontal(|ui| {
                                        ui.label("View height");
                                        let field = number(ui, camera_owner.with("height"), "Scene camera view height", height, Some(0.05), Some(0.001..=1e9), "");
                                        changed |= feedback.observe(&field.response, field.changed);
                                    });
                                }
                            }
                            ui.horizontal(|ui| {
                                ui.label("Near / far");
                                for (property, name, value, speed) in [
                                    ("near", "Scene camera near clip", &mut scene.camera.near, 0.01),
                                    ("far", "Scene camera far clip", &mut scene.camera.far, 1.),
                                ] {
                                    let field = number(ui, camera_owner.with(property), name, value, Some(speed), None, "");
                                    changed |= feedback.observe(&field.response, field.changed);
                                }
                            });
                            let reset = control(ui, camera_owner.with("reset"), "Reset scene camera", |ui| ui.button("Reset camera"));
                            feedback.observe(&reset, false);
                            if reset.clicked() { scene.camera = Camera::default(); changed = true; }
                        }
                    });
                    let lighting_owner = owner.with("lighting");
                    section(ui, lighting_owner, "Lighting", "Scene lighting controls", &mut feedback, |ui, feedback| {
                        feedback.domain = Domain::Light;
                        changed |= triple(ui, lighting_owner.with("ambient"), "Ambient", "Scene ambient", ["R", "G", "B"], &mut scene.ambient, feedback);
                        let response = control(ui, lighting_owner.with("imported"), "Use model lights", |ui| ui.checkbox(&mut scene.imported_lights, "Use model lights"));
                        changed |= feedback.observe(&response, response.changed());
                        let add = control(ui, lighting_owner.with("add"), "Add scene light", |ui| ui.button("Add light"));
                        feedback.observe(&add, false);
                        if add.clicked() {
                            scene.lights.push(Light {
                                kind: LightKind::Directional, position: [0., 2., 3.], direction: [0., 0., -1.], color: [1.; 3], intensity: 2., range: None,
                            });
                            light_structure = true;
                            changed = true;
                        }
                        let mut remove = None;
                        for (index, light) in scene.lights.iter_mut().enumerate() {
                            let light_owner = lighting_owner.with(("light", rows.light_generation, index));
                            let name = format!("Light {}", index + 1);
                            let first_widget = feedback.widgets.len();
                            section(ui, light_owner, &name, &format!("{name} controls"), feedback, |ui, feedback| {
                                let mut chosen = false;
                                let picker = control(ui, light_owner.with("kind"), &format!("{name} type"), |ui| {
                                    egui::ComboBox::from_id_salt("value")
                                        .selected_text(match light.kind { LightKind::Directional => "Directional", LightKind::Point => "Point", LightKind::Spot { .. } => "Spot" })
                                        .show_ui(ui, |ui| {
                                            for (key, kind, label) in [("directional", LightKind::Directional, "Directional"), ("point", LightKind::Point, "Point")] {
                                                let response = control(ui, light_owner.with(("type", key)), &format!("{name} {label}"), |ui| ui.selectable_value(&mut light.kind, kind, label));
                                                chosen |= response.clicked();
                                                changed |= feedback.observe(&response, response.changed());
                                            }
                                            let spot = control(ui, light_owner.with(("type", "spot")), &format!("{name} Spot"), |ui| ui.selectable_label(matches!(light.kind, LightKind::Spot { .. }), "Spot"));
                                            chosen |= spot.clicked();
                                            feedback.observe(&spot, false);
                                            if spot.clicked() { light.kind = LightKind::Spot { inner: 0.3, outer: 0.6 }; changed = true; }
                                            if chosen { ui.close(); }
                                        }).response
                                });
                                if chosen { picker.request_focus(); }
                                feedback.observe(&picker, false);
                                for (property, label, value, axes) in [
                                    ("position", "Position", &mut light.position, ["X", "Y", "Z"]),
                                    ("direction", "Direction", &mut light.direction, ["X", "Y", "Z"]),
                                    ("color", "Color", &mut light.color, ["R", "G", "B"]),
                                ] {
                                    changed |= triple(ui, light_owner.with(property), label, &format!("{name} {}", property), axes, value, feedback);
                                }
                                ui.horizontal(|ui| {
                                    ui.label("Brightness");
                                    let field = number(ui, light_owner.with("intensity"), &format!("{name} brightness"), &mut light.intensity, Some(0.05), Some(0.0..=1e9), "");
                                    changed |= feedback.observe(&field.response, field.changed);
                                });
                                let response = control(ui, light_owner.with("remove"), &format!("Remove {name}"), |ui| ui.button("Remove light"));
                                feedback.observe(&response, false);
                                if response.clicked() { remove = Some(index); }
                            });
                            feedback.light_widgets.extend(feedback.widgets[first_widget..].iter().map(|response| response.id));
                        }
                        if let Some(i) = remove { scene.lights.remove(i); light_structure = true; changed = true; }
                    });
                    let mut remove = None;
                    for object in &mut scene.objects {
                        let object_owner = owner.with(("object", object.id));
                        let name = object.name.clone();
                        section(ui, object_owner, &name, &format!("{name} controls"), &mut feedback, |ui, feedback| {
                            feedback.domain = Domain::Object;
                            let response = control(ui, object_owner.with("visible"), &format!("{name} visible"), |ui| ui.checkbox(&mut object.visible, "Visible"));
                            changed |= feedback.observe(&response, response.changed());
                            let response = control(ui, object_owner.with("name"), &format!("{name} model name"), |ui| ui.text_edit_singleline(&mut object.name));
                            changed |= feedback.observe(&response, response.changed());
                            ui.label(format!("{} vertices · {} triangles", object.model.vertex_count(), object.model.triangle_count()));
                            for (property, label, value) in [
                                ("position", "Position", &mut object.translation),
                                ("rotation", "Rotate °", &mut object.rotation),
                                ("scale", "Scale", &mut object.scale),
                            ] {
                                let units = if property == "rotation" { " (degrees)" } else { "" };
                                changed |= triple(ui, object_owner.with(property), label, &format!("{name} {property}{units}"), ["X", "Y", "Z"], value, feedback);
                            }
                            let field = slider(ui, object_owner.with("opacity"), &format!("{name} opacity"), "Opacity", &mut object.opacity);
                            changed |= feedback.observe(&field.response, field.changed);
                            let mut chosen = false;
                            let picker = control(ui, object_owner.with("clip"), &format!("{name} animation clip"), |ui| {
                                egui::ComboBox::from_id_salt("value")
                                    .selected_text(object.animation.and_then(|i| object.model.animations.get(i)).map_or("Rest pose", |a| a.name.as_str()))
                                    .show_ui(ui, |ui| {
                                        let response = control(ui, object_owner.with("rest"), &format!("{name} rest pose"), |ui| ui.selectable_value(&mut object.animation, None, "Rest pose"));
                                        chosen |= response.clicked();
                                        changed |= feedback.observe(&response, response.changed());
                                        for (i, clip) in object.model.animations.iter().enumerate() {
                                            let response = control(ui, object_owner.with(("clip", i)), &format!("{name} clip {}", clip.name), |ui| ui.selectable_value(&mut object.animation, Some(i), &clip.name));
                                            chosen |= response.clicked();
                                            changed |= feedback.observe(&response, response.changed());
                                        }
                                        if chosen { ui.close(); }
                                    }).response
                            });
                            if chosen { picker.request_focus(); }
                            feedback.observe(&picker, false);
                            if object.animation.is_some() {
                                let response = control(ui, object_owner.with("loop"), &format!("{name} loop clip"), |ui| ui.checkbox(&mut object.looped, "Loop clip"));
                                changed |= feedback.observe(&response, response.changed());
                                ui.horizontal(|ui| {
                                    ui.label("Clip offset / speed");
                                    for (property, label, value) in [("offset", "clip offset (s)", &mut object.time_offset), ("speed", "clip speed", &mut object.time_scale)] {
                                        let field = number(ui, object_owner.with(property), &format!("{name} {label}"), value, Some(0.05), None, "");
                                        changed |= feedback.observe(&field.response, field.changed);
                                    }
                                });
                            }
                            section(ui, object_owner.with("node"), "Node", &format!("{name} node controls"), feedback, |ui, feedback| {
                                feedback.domain = Domain::Resource;
                                if object.model.nodes.is_empty() { return; }
                                let key = object_owner.with("selected-node");
                                let mut index = ui.data(|d| d.get_temp::<usize>(key)).unwrap_or(0).min(object.model.nodes.len() - 1);
                                let field = number(ui, object_owner.with("node-index"), &format!("{name} node index"), &mut index, None, Some(0..=object.model.nodes.len() - 1), "");
                                feedback.observe(&field.response, false);
                                ui.data_mut(|d| d.insert_temp(key, index));
                                let mut node = object.model.nodes[index].clone();
                                ui.label(&node.name);
                                if node.matrix.is_some() { ui.label("Use the model position, rotation and scale controls for this node."); return; }
                                let node_owner = object_owner.with(("node", index));
                                let mut node_changed = triple(ui, node_owner.with("position"), "Position", &format!("{name} node {index} position"), ["X", "Y", "Z"], &mut node.translation, feedback);
                                node_changed |= triple(ui, node_owner.with("scale"), "Scale", &format!("{name} node {index} scale"), ["X", "Y", "Z"], &mut node.scale, feedback);
                                ui.horizontal(|ui| {
                                    ui.label("Rotation xyzw");
                                    for (axis, value) in ["X", "Y", "Z", "W"].into_iter().zip(&mut node.rotation) {
                                        let field = number(ui, node_owner.with(("rotation", axis)), &format!("{name} node {index} quaternion {axis}"), value, Some(0.01), None, "");
                                        node_changed |= feedback.observe(&field.response, field.changed);
                                    }
                                });
                                if node_changed {
                                    node.matrix = None;
                                    std::sync::Arc::make_mut(&mut object.model).nodes[index] = node;
                                    changed = true;
                                }
                            });
                            section(ui, object_owner.with("material"), "Material", &format!("{name} material controls"), feedback, |ui, feedback| {
                                feedback.domain = Domain::Resource;
                                if object.model.materials.is_empty() { ui.label("Default material"); return; }
                                let key = object_owner.with("selected-material");
                                let mut index = ui.data(|d| d.get_temp::<usize>(key)).unwrap_or(0).min(object.model.materials.len() - 1);
                                let field = number(ui, object_owner.with("material-index"), &format!("{name} material index"), &mut index, None, Some(0..=object.model.materials.len() - 1), "");
                                feedback.observe(&field.response, false);
                                ui.data_mut(|d| d.insert_temp(key, index));
                                let mut m = object.model.materials[index].clone();
                                ui.label(&m.name);
                                let material_owner = object_owner.with(("material", index));
                                let mut material_changed = false;
                                ui.horizontal(|ui| {
                                    ui.label("Color / alpha");
                                    for (axis, value) in ["R", "G", "B", "A"].into_iter().zip(&mut m.base_color) {
                                        let field = number(ui, material_owner.with(("color", axis)), &format!("{name} material {index} color {axis}"), value, Some(0.01), Some(0.0..=1.0), "");
                                        material_changed |= feedback.observe(&field.response, field.changed);
                                    }
                                });
                                for (property, label, value) in [("metallic", "Metallic", &mut m.metallic), ("roughness", "Roughness", &mut m.roughness)] {
                                    let field = slider(ui, material_owner.with(property), &format!("{name} material {index} {property}"), label, value);
                                    material_changed |= feedback.observe(&field.response, field.changed);
                                }
                                for (property, label, value) in [("unlit", "Unlit", &mut m.unlit), ("double-sided", "Both sides", &mut m.double_sided)] {
                                    let response = control(ui, material_owner.with(property), &format!("{name} material {index} {label}"), |ui| ui.checkbox(value, label));
                                    material_changed |= feedback.observe(&response, response.changed());
                                }
                                let mut chosen = false;
                                let picker = control(ui, material_owner.with("alpha"), &format!("{name} material {index} alpha mode"), |ui| {
                                    egui::ComboBox::from_id_salt("value").selected_text(format!("{:?}", m.alpha_mode))
                                        .show_ui(ui, |ui| {
                                            for (key, mode) in [("opaque", AlphaMode::Opaque), ("mask", AlphaMode::Mask), ("blend", AlphaMode::Blend)] {
                                                let label = format!("{mode:?}");
                                                let response = control(ui, material_owner.with(("alpha", key)), &format!("{name} material {index} {label}"), |ui| ui.selectable_value(&mut m.alpha_mode, mode, label));
                                                chosen |= response.clicked();
                                                material_changed |= feedback.observe(&response, response.changed());
                                            }
                                            if chosen { ui.close(); }
                                        }).response
                                });
                                if chosen { picker.request_focus(); }
                                feedback.observe(&picker, false);
                                if m.alpha_mode == AlphaMode::Mask {
                                    let field = slider(ui, material_owner.with("cutoff"), &format!("{name} material {index} alpha cutoff"), "Cutoff", &mut m.alpha_cutoff);
                                    material_changed |= feedback.observe(&field.response, field.changed);
                                }
                                if material_changed {
                                    std::sync::Arc::make_mut(&mut object.model).materials[index] = m;
                                    changed = true;
                                }
                            });
                            feedback.domain = Domain::Object;
                            let response = control(ui, object_owner.with("remove"), &format!("Remove {name} model"), |ui| ui.button("Remove model"));
                            feedback.observe(&response, false);
                            if response.clicked() { remove = Some(object.id); }
                        });
                    }
                    if let Some(id) = remove {
                        scene.objects.retain(|o| o.id != id);
                        if scene.imported_camera.is_some_and(|(o, _)| o == id) { scene.imported_camera = None; }
                        changed = true;
                    }
                    if changed {
                        light_structure |= scene.lights.len() != light_count;
                        motion.duration = scene
                            .objects
                            .iter()
                            .flat_map(|o| &o.model.animations)
                            .map(|a| a.duration() as f32)
                            .fold(motion.duration, f32::max);
                        let mut candidate = studio.doc.clone();
                        candidate.motion = motion.clone();
                        match crate::motion::compositions::validate(&candidate) {
                            Ok(()) => {
                                studio.commit_motion_step(motion);
                                structural |= light_structure;
                                ui.data_mut(|data| data.remove::<Rejection>(error_key));
                            }
                            Err(e) => {
                                let domain = if e.starts_with("SceneCamera:") { Some(Domain::Camera) }
                                    else if e.starts_with("SceneLight:") { Some(Domain::Light) }
                                    else if e.starts_with("SceneTransform:") { Some(Domain::Object) }
                                    else { None };
                                let fields = feedback.edited.iter().filter(|(group, _)| Some(*group) == domain).map(|(_, response)| response.id).collect::<Vec<_>>();
                                if !fields.is_empty() { ui.data_mut(|data| data.insert_temp(error_key, Rejection { fields, message: e.clone() })); }
                                studio.status = e;
                            }
                        }
                    }
                    if let Some(rejection) = ui.data(|data| data.get_temp::<Rejection>(error_key)) {
                        let fields = feedback.widgets.iter().filter(|response| rejection.fields.contains(&response.id)).collect::<Vec<_>>();
                        if !fields.is_empty() { super::welcome::control_error(ui, &fields, &rejection.message); }
                    }
                    if let Some(error) = crate::compositor::motion_error() {
                        ui.colored_label(egui::Color32::LIGHT_RED, error);
                    }
                });
            });
        })).inner
    }).inner.response;
    super::welcome::focus_control(ui, &menu);
    if let Some(focused) = ui.memory(|memory| memory.focused())
        && rows.widgets.contains(&focused)
        && !feedback
            .widgets
            .iter()
            .any(|response| response.id == focused)
    {
        ui.memory_mut(|memory| memory.surrender_focus(focused));
    }
    rows.widgets = feedback
        .widgets
        .iter()
        .map(|response| response.id)
        .collect();
    rows.light_widgets = feedback.light_widgets;
    if structural {
        rows.retire_lights(ui.ctx());
        ui.data_mut(|data| data.remove::<Rejection>(error_key));
    }
    rows.revision = studio.canvas_gen;
    ui.data_mut(|data| data.insert_temp(registry, rows));
}

#[cfg(test)]
mod keyboard_tests {
    use super::*;
    use crate::{document::Document, tools::Persona, ui::welcome::keyboard_tests as keys};
    use egui::{Event, Key, Modifiers, accesskit};

    fn context() -> egui::Context {
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        crate::ui::theme::apply(&ctx);
        ctx.all_styles_mut(|style| style.animation_time = 0.0);
        ctx
    }

    fn empty() -> Studio {
        let mut studio = Studio::new();
        studio.doc = Document::new("Native scene fields", 640., 480., 72.);
        studio.persona = Persona::Motion;
        studio.show_welcome = false;
        studio
    }

    fn frame(ctx: &egui::Context, studio: &mut Studio, events: Vec<Event>) -> egui::FullOutput {
        keys::render(ctx, events, |ui| {
            studio.handle_shortcuts(ui.ctx());
            show(ui, studio);
        })
    }

    fn has(output: &egui::FullOutput, name: &str) -> bool {
        output
            .platform_output
            .accesskit_update
            .as_ref()
            .unwrap()
            .nodes
            .iter()
            .any(|(_, node)| node.label() == Some(name))
    }

    fn native_number(output: &egui::FullOutput, name: &str, authoritative: f64) {
        let node = keys::named(output, name).1;
        assert_eq!(
            node.numeric_value(),
            Some(authoritative),
            "Actual native numeric value: {name}"
        );
        assert_eq!(node.invalid(), None, "Settled native value: {name}");
    }

    fn loaded_number_frames(
        ctx: &egui::Context,
        studio: &mut Studio,
        name: &str,
        authoritative: f64,
    ) {
        let rendered = frame(ctx, studio, vec![]);
        native_number(&rendered, name, authoritative);
        let focused = keys::tab_to(ctx, name, |events| frame(ctx, studio, events));
        keys::focused(ctx, &focused, name);
        native_number(&focused, name, authoritative);
        let resting = frame(ctx, studio, vec![]);
        native_number(&resting, name, authoritative);
        let blur = frame(ctx, studio, keys::key(Key::Tab, Modifiers::NONE));
        native_number(&blur, name, authoritative);
        let settled_blur = frame(ctx, studio, vec![]);
        native_number(&settled_blur, name, authoritative);
    }

    fn press(ctx: &egui::Context, studio: &mut Studio, name: &str) -> egui::FullOutput {
        let output = keys::tab_to(ctx, name, |events| frame(ctx, studio, events));
        keys::focused(ctx, &output, name);
        frame(ctx, studio, keys::key(Key::Enter, Modifiers::NONE));
        frame(ctx, studio, vec![])
    }

    fn open(ctx: &egui::Context, studio: &mut Studio) {
        if !has(&frame(ctx, studio, vec![]), "Cube") {
            press(ctx, studio, "3D Scene");
        }
    }

    fn create_cube(ctx: &egui::Context, studio: &mut Studio) {
        assert!(studio.doc.motion.scene3d.is_none());
        assert!(studio.selection.is_empty());
        assert_eq!(studio.history.len(), 0);
        open(ctx, studio);
        press(ctx, studio, "Cube");
        assert_eq!(studio.doc.motion.scene3d.as_ref().unwrap().objects.len(), 1);
        assert_eq!(studio.history.len(), 1);
        assert!(studio.selection.is_empty());
        open(ctx, studio);
    }

    fn edit(ctx: &egui::Context, studio: &mut Studio, name: &str, text: &str) -> egui::FullOutput {
        let output = keys::tab_to(ctx, name, |events| frame(ctx, studio, events));
        keys::focused(ctx, &output, name);
        keys::replace(text, |events| frame(ctx, studio, events));
        frame(ctx, studio, keys::key(Key::Enter, Modifiers::NONE));
        frame(ctx, studio, vec![])
    }

    fn close(ctx: &egui::Context, studio: &mut Studio) {
        for _ in 0..4 {
            if !egui::Popup::is_any_open(ctx) {
                frame(ctx, studio, keys::key(Key::F8, Modifiers::NONE));
                frame(ctx, studio, vec![]);
                return;
            }
            frame(ctx, studio, keys::key(Key::Escape, Modifiers::NONE));
            frame(ctx, studio, vec![]);
        }
        assert!(!egui::Popup::is_any_open(ctx));
        frame(ctx, studio, keys::key(Key::F8, Modifiers::NONE));
        frame(ctx, studio, vec![]);
    }

    #[test]
    fn scene3d_native_empty_scene_creation_target_edit_undo_and_json_reopen() {
        let ctx = context();
        let mut studio = empty();
        create_cube(&ctx, &mut studio);
        let created = studio.doc.motion.clone();
        press(&ctx, &mut studio, "Cube controls");
        let revision = studio.canvas_gen;
        keys::tab_to(&ctx, "Cube position X", |events| {
            frame(&ctx, &mut studio, events)
        });
        assert_eq!(studio.doc.motion, created);
        assert_eq!(
            studio.canvas_gen, revision,
            "Native focus cannot commit a Scene command"
        );
        edit(&ctx, &mut studio, "Cube position X", "12.125");
        assert_eq!(
            studio.doc.motion.scene3d.as_ref().unwrap().objects[0].translation[0],
            12.125
        );
        press(&ctx, &mut studio, "Cube node controls");
        edit(&ctx, &mut studio, "Cube node index", "0");
        edit(&ctx, &mut studio, "Cube node 0 position Y", "0.375");
        press(&ctx, &mut studio, "Cube material controls");
        edit(&ctx, &mut studio, "Cube material index", "0");
        edit(&ctx, &mut studio, "Cube material 0 color B", "0.75");
        let scene = studio.doc.motion.scene3d.as_ref().unwrap();
        assert_eq!(scene.objects[0].model.nodes[0].translation[1], 0.375);
        assert_eq!(scene.objects[0].model.materials[0].base_color[2], 0.75);
        assert!(
            studio.selection.is_empty(),
            "Scene headers and native indices are the existing target routes"
        );
        let authored = studio.doc.motion.clone();
        close(&ctx, &mut studio);
        frame(
            &ctx,
            &mut studio,
            keys::key(Key::Z, Modifiers::CTRL | Modifiers::COMMAND),
        );
        assert_ne!(
            studio.doc.motion, authored,
            "Native Scene edits must reach typed history"
        );
        frame(
            &ctx,
            &mut studio,
            keys::key(
                Key::Z,
                Modifiers::CTRL | Modifiers::COMMAND | Modifiers::SHIFT,
            ),
        );
        assert_eq!(studio.doc.motion, authored);
        let json = crate::project::encode(&studio.doc).unwrap();
        assert_eq!(crate::project::decode(&json).unwrap().motion, authored);
    }

    #[test]
    fn scene3d_native_invalid_parser_and_camera_candidate_errors_use_actual_responses() {
        let ctx = context();
        let mut studio = empty();
        create_cube(&ctx, &mut studio);
        press(&ctx, &mut studio, "Cube controls");
        let before = studio.doc.motion.clone();
        let history = studio.history.len();
        let revision = studio.canvas_gen;
        keys::tab_to(&ctx, "Cube position X", |events| {
            frame(&ctx, &mut studio, events)
        });
        let output = keys::replace("-", |events| frame(&ctx, &mut studio, events));
        keys::associated_error(&output, "Cube position X");
        assert_eq!(
            keys::named(&output, "Cube position X").1.numeric_value(),
            None
        );
        assert_eq!(studio.doc.motion, before);
        assert_eq!(studio.history.len(), history);
        assert_eq!(studio.canvas_gen, revision);
        let field = keys::named(&output, "Cube position X").0;
        let output = frame(
            &ctx,
            &mut studio,
            keys::action(
                field,
                accesskit::Action::SetValue,
                Some(accesskit::ActionData::NumericValue(f64::NAN)),
            ),
        );
        keys::associated_error(&output, "Cube position X");
        assert_eq!(
            keys::named(&output, "Cube position X").1.numeric_value(),
            None
        );
        assert_eq!(studio.doc.motion, before);
        assert_eq!(studio.history.len(), history);
        assert_eq!(studio.canvas_gen, revision);
        frame(&ctx, &mut studio, keys::key(Key::Escape, Modifiers::NONE));
        open(&ctx, &mut studio);
        let settled_cancel = frame(&ctx, &mut studio, vec![]);
        native_number(
            &settled_cancel,
            "Cube position X",
            studio.doc.motion.scene3d.as_ref().unwrap().objects[0].translation[0],
        );
        press(&ctx, &mut studio, "Scene camera controls");
        keys::tab_to(&ctx, "Scene camera near clip", |events| {
            frame(&ctx, &mut studio, events)
        });
        let output = keys::replace("-2", |events| frame(&ctx, &mut studio, events));
        keys::associated_error(&output, "Scene camera near clip");
        assert_eq!(
            keys::named(&output, "Scene camera near clip")
                .1
                .numeric_value(),
            None
        );
        assert!(studio.status.starts_with("SceneCamera:"));
        assert_eq!(studio.doc.motion, before);
        assert_eq!(studio.history.len(), history);
        assert_eq!(studio.canvas_gen, revision);
        let output = keys::replace("-", |events| frame(&ctx, &mut studio, events));
        keys::associated_error(&output, "Scene camera near clip");
        assert_eq!(
            keys::named(&output, "Scene camera near clip")
                .1
                .numeric_value(),
            None
        );
        assert_eq!(studio.doc.motion, before);
        frame(&ctx, &mut studio, keys::key(Key::Escape, Modifiers::NONE));
        open(&ctx, &mut studio);
        let settled_cancel = frame(&ctx, &mut studio, vec![]);
        native_number(
            &settled_cancel,
            "Scene camera near clip",
            studio.doc.motion.scene3d.as_ref().unwrap().camera.near,
        );
        assert_eq!(studio.doc.motion, before);
        assert_eq!(studio.history.len(), history);
        assert_eq!(studio.canvas_gen, revision);
    }

    #[test]
    fn scene3d_native_disabled_and_document_stale_actions_cannot_edit() {
        let ctx = context();
        let mut studio = empty();
        create_cube(&ctx, &mut studio);
        press(&ctx, &mut studio, "Cube controls");
        let output = keys::tab_to(&ctx, "Cube position X", |events| {
            frame(&ctx, &mut studio, events)
        });
        let stale = keys::named(&output, "Cube position X").0;
        let before = studio.doc.motion.clone();
        let history = studio.history.len();
        let revision = studio.canvas_gen;
        studio.updates.freezing = true;
        let output = frame(&ctx, &mut studio, vec![]);
        assert!(keys::named(&output, "Cube position X").1.is_disabled());
        frame(
            &ctx,
            &mut studio,
            keys::action(
                stale,
                accesskit::Action::SetValue,
                Some(accesskit::ActionData::NumericValue(77.)),
            ),
        );
        assert_eq!(studio.doc.motion, before);
        assert_eq!(studio.history.len(), history);
        assert_eq!(studio.canvas_gen, revision);
        studio.updates.freezing = false;
        frame(&ctx, &mut studio, vec![]);
        close(&ctx, &mut studio);
        let mut reopened = empty();
        reopened.doc =
            crate::project::decode(&crate::project::encode(&studio.doc).unwrap()).unwrap();
        open(&ctx, &mut reopened);
        press(&ctx, &mut reopened, "Cube controls");
        let output = keys::tab_to(&ctx, "Cube position X", |events| {
            frame(&ctx, &mut reopened, events)
        });
        let current = keys::named(&output, "Cube position X").0;
        assert_ne!(
            current, stale,
            "Reopened native documents retain object IDs but own new widget identities"
        );
        let revision = reopened.canvas_gen;
        frame(
            &ctx,
            &mut reopened,
            keys::action(
                stale,
                accesskit::Action::SetValue,
                Some(accesskit::ActionData::NumericValue(99.)),
            ),
        );
        assert_eq!(reopened.doc.motion, before);
        assert_eq!(reopened.history.len(), 0);
        assert_eq!(reopened.canvas_gen, revision);
    }

    #[test]
    fn scene3d_native_field_ids_survive_conditional_rows_rename_and_native_arrow_keys() {
        let ctx = context();
        let mut studio = empty();
        create_cube(&ctx, &mut studio);
        press(&ctx, &mut studio, "Cube controls");
        let output = keys::tab_to(&ctx, "Cube position X", |events| {
            frame(&ctx, &mut studio, events)
        });
        let position = keys::named(&output, "Cube position X").0;
        assert_ne!(
            keys::named(&output, "Cube position X")
                .1
                .state_description(),
            Some("Mixed")
        );
        frame(&ctx, &mut studio, keys::key(Key::ArrowUp, Modifiers::NONE));
        let output = frame(&ctx, &mut studio, vec![]);
        keys::focused(&ctx, &output, "Cube position X");
        assert_eq!(keys::named(&output, "Cube position X").0, position);
        assert_eq!(
            studio.doc.motion.scene3d.as_ref().unwrap().objects[0].translation[0],
            0.05
        );
        frame(&ctx, &mut studio, keys::key(Key::Enter, Modifiers::NONE));
        press(&ctx, &mut studio, "Scene camera controls");
        press(&ctx, &mut studio, "Scene camera orthographic");
        let output = frame(&ctx, &mut studio, vec![]);
        assert_eq!(keys::named(&output, "Cube position X").0, position);
        press(&ctx, &mut studio, "Cube material controls");
        let output = frame(&ctx, &mut studio, vec![]);
        let remove = keys::named(&output, "Remove Cube model").0;
        press(&ctx, &mut studio, "Cube material 0 alpha mode");
        press(&ctx, &mut studio, "Cube material 0 Mask");
        let output = frame(&ctx, &mut studio, vec![]);
        assert!(has(&output, "Cube material 0 alpha cutoff"));
        assert_eq!(keys::named(&output, "Remove Cube model").0, remove);
        edit(&ctx, &mut studio, "Cube model name", "Renamed");
        let output = frame(&ctx, &mut studio, vec![]);
        assert_eq!(keys::named(&output, "Renamed position X").0, position);
        assert_eq!(keys::named(&output, "Remove Renamed model").0, remove);
    }

    #[test]
    fn scene3d_native_light_removal_and_undo_retire_surviving_row_drafts_ids_and_focus() {
        let ctx = context();
        let mut studio = empty();
        create_cube(&ctx, &mut studio);
        press(&ctx, &mut studio, "Scene camera controls");
        press(&ctx, &mut studio, "Cube controls");
        press(&ctx, &mut studio, "Cube node controls");
        press(&ctx, &mut studio, "Cube material controls");
        press(&ctx, &mut studio, "Scene lighting controls");
        press(&ctx, &mut studio, "Light 1 controls");
        let output = frame(&ctx, &mut studio, vec![]);
        let stable_names = [
            "Scene camera near clip",
            "Cube position X",
            "Cube node 0 position Y",
            "Cube material 0 color B",
        ];
        let stable = stable_names.map(|name| (name, keys::named(&output, name).0));
        let before_insert = keys::named(&output, "Light 1 position X").0;
        let output = press(&ctx, &mut studio, "Add scene light");
        for (name, id) in stable {
            assert_eq!(
                keys::named(&output, name).0,
                id,
                "An inserted light cannot retire {name}"
            );
        }
        open(&ctx, &mut studio);
        press(&ctx, &mut studio, "Light 1 controls");
        press(&ctx, &mut studio, "Light 2 controls");
        let output = keys::tab_to(&ctx, "Light 2 position X", |events| {
            frame(&ctx, &mut studio, events)
        });
        let first = keys::named(&output, "Light 1 position X").0;
        assert_ne!(first, before_insert);
        let survivor = keys::named(&output, "Light 2 position X").0;
        let output = keys::replace("-", |events| frame(&ctx, &mut studio, events));
        keys::associated_error(&output, "Light 2 position X");
        press(&ctx, &mut studio, "Remove Light 1");
        assert_eq!(studio.doc.motion.scene3d.as_ref().unwrap().lights.len(), 1);
        assert_ne!(
            ctx.memory(|memory| memory.focused())
                .map(|id| id.accesskit_id()),
            Some(survivor)
        );
        open(&ctx, &mut studio);
        press(&ctx, &mut studio, "Light 1 controls");
        let output = frame(&ctx, &mut studio, vec![]);
        for (name, id) in stable {
            assert_eq!(
                keys::named(&output, name).0,
                id,
                "A removed light cannot retire {name}"
            );
        }
        let current = keys::named(&output, "Light 1 position X").0;
        assert_ne!(current, first);
        assert_ne!(current, survivor);
        assert_eq!(keys::named(&output, "Light 1 position X").1.invalid(), None);
        let before = studio.doc.motion.clone();
        let history = studio.history.len();
        let revision = studio.canvas_gen;
        let mut actions = keys::action(
            first,
            accesskit::Action::SetValue,
            Some(accesskit::ActionData::NumericValue(99.)),
        );
        actions.extend(keys::action(
            survivor,
            accesskit::Action::SetValue,
            Some(accesskit::ActionData::NumericValue(77.)),
        ));
        frame(&ctx, &mut studio, actions);
        assert_eq!(studio.doc.motion, before);
        assert_eq!(studio.history.len(), history);
        assert_eq!(studio.canvas_gen, revision);
        close(&ctx, &mut studio);
        frame(
            &ctx,
            &mut studio,
            keys::key(Key::Z, Modifiers::CTRL | Modifiers::COMMAND),
        );
        assert_eq!(studio.doc.motion.scene3d.as_ref().unwrap().lights.len(), 2);
        open(&ctx, &mut studio);
        press(&ctx, &mut studio, "Scene lighting controls");
        press(&ctx, &mut studio, "Light 2 controls");
        let output = frame(&ctx, &mut studio, vec![]);
        assert_ne!(keys::named(&output, "Light 2 position X").0, survivor);
        assert_eq!(keys::named(&output, "Light 2 position X").1.invalid(), None);
        let restored = keys::named(&output, "Light 2 position X").0;
        let mut replacement = studio.doc.motion.clone();
        replacement.scene3d.as_mut().unwrap().lights.swap(0, 1);
        studio.commit_motion_step(replacement);
        let expected = studio.doc.motion.clone();
        frame(
            &ctx,
            &mut studio,
            keys::action(
                restored,
                accesskit::Action::SetValue,
                Some(accesskit::ActionData::NumericValue(66.)),
            ),
        );
        press(&ctx, &mut studio, "Scene lighting controls");
        press(&ctx, &mut studio, "Light 2 controls");
        let output = frame(&ctx, &mut studio, vec![]);
        assert_ne!(keys::named(&output, "Light 2 position X").0, restored);
        assert_eq!(
            studio.doc.motion, expected,
            "An external same-length typed replacement retires the old native rows"
        );
    }

    #[test]
    fn scene3d_native_loaded_valid_values_survive_render_focus_and_blur_without_commands() {
        for projection in [
            Projection::Perspective { fov: 178.5 },
            Projection::Orthographic { height: 2e9 },
        ] {
            let ctx = context();
            let mut studio = empty();
            let mut scene = Scene::default();
            scene
                .add(
                    model::prim::model(
                        model::prim::PrimitiveKind::Cube,
                        &model::prim::PrimParams::default(),
                        model::Material::default(),
                    ),
                    "Loaded".into(),
                )
                .unwrap();
            scene.camera.projection = projection;
            scene.lights[0].intensity = 2e9;
            scene.objects[0].translation[0] = 1e11 + 0.125;
            scene.objects[0].opacity = 0.6123457;
            let material = &mut std::sync::Arc::make_mut(&mut scene.objects[0].model).materials[0];
            material.base_color[2] = 0.12345679;
            material.roughness = 0.7654321;
            studio.doc.motion.scene3d = Some(scene);
            crate::motion::compositions::validate(&studio.doc).unwrap();
            let before = studio.doc.motion.clone();
            let revision = studio.canvas_gen;
            open(&ctx, &mut studio);
            press(&ctx, &mut studio, "Scene camera controls");
            let name = match projection {
                Projection::Perspective { .. } => "Scene camera view angle (degrees)",
                Projection::Orthographic { .. } => "Scene camera view height",
            };
            let scene = studio.doc.motion.scene3d.as_ref().unwrap();
            let projection_value = match scene.camera.projection {
                Projection::Perspective { fov } => fov,
                Projection::Orthographic { height } => height,
            };
            for (name, expected) in [
                (name, projection_value),
                ("Scene camera near clip", scene.camera.near),
            ] {
                loaded_number_frames(&ctx, &mut studio, name, expected);
                assert_eq!(studio.doc.motion, before);
                assert_eq!(studio.history.len(), 0);
                assert_eq!(studio.canvas_gen, revision);
            }
            press(&ctx, &mut studio, "Scene lighting controls");
            press(&ctx, &mut studio, "Light 1 controls");
            let expected = studio.doc.motion.scene3d.as_ref().unwrap().lights[0].intensity;
            loaded_number_frames(&ctx, &mut studio, "Light 1 brightness", expected);
            press(&ctx, &mut studio, "Loaded controls");
            press(&ctx, &mut studio, "Loaded material controls");
            let scene = studio.doc.motion.scene3d.as_ref().unwrap();
            let values = [
                ("Loaded position X", scene.objects[0].translation[0]),
                ("Loaded opacity", f64::from(scene.objects[0].opacity)),
                (
                    "Loaded material 0 color B",
                    f64::from(scene.objects[0].model.materials[0].base_color[2]),
                ),
                (
                    "Loaded material 0 roughness",
                    f64::from(scene.objects[0].model.materials[0].roughness),
                ),
            ];
            for (name, expected) in values {
                loaded_number_frames(&ctx, &mut studio, name, expected);
                assert_eq!(studio.doc.motion, before);
                assert_eq!(studio.history.len(), 0);
                assert_eq!(studio.canvas_gen, revision);
            }
            keys::tab_to(&ctx, "Loaded material 0 color B", |events| {
                frame(&ctx, &mut studio, events)
            });
            let rejected = keys::replace("-", |events| frame(&ctx, &mut studio, events));
            keys::associated_error(&rejected, "Loaded material 0 color B");
            assert_eq!(
                keys::named(&rejected, "Loaded material 0 color B")
                    .1
                    .numeric_value(),
                None
            );
            assert_eq!(studio.doc.motion, before);
            assert_eq!(studio.history.len(), 0);
            assert_eq!(studio.canvas_gen, revision);
            frame(&ctx, &mut studio, keys::key(Key::Escape, Modifiers::NONE));
            open(&ctx, &mut studio);
            let settled_cancel = frame(&ctx, &mut studio, vec![]);
            let expected = f64::from(
                studio.doc.motion.scene3d.as_ref().unwrap().objects[0]
                    .model
                    .materials[0]
                    .base_color[2],
            );
            native_number(&settled_cancel, "Loaded material 0 color B", expected);
            assert_eq!(studio.doc.motion, before);
            assert_eq!(studio.history.len(), 0);
            assert_eq!(studio.canvas_gen, revision);
        }
    }
}
