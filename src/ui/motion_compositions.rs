//! Native composition authoring in the motion timeline.
use crate::{app::Studio, motion::compositions::Composition};
use eframe::egui::{self, Ui};

pub(super) fn show(ui: &mut Ui, studio: &mut Studio) {
    let owner = egui::Id::new(("motion-compositions", &studio.swap_id));
    let key = owner.with("menu-open");
    let mut open = ui.data(|data| data.get_temp::<bool>(key)).unwrap_or(false);
    let menu = super::fields::native_scope(ui, owner, |ui| {
        let response = ui.button("Compositions");
        if response.clicked() {
            open = !open;
        }
        egui::Popup::menu(&response)
            .open_bool(&mut open)
            .show(|ui| {
                if studio.file_dialog_pending() || studio.updates.freezing {
                    ui.disable();
                }
                ui.set_min_width(300.);
                ui.set_max_width(420.);
                let clip = ui.clip_rect();
                let available = ui.available_rect_before_wrap().intersect(clip);
                let height = (available.bottom() - ui.next_widget_position().y).clamp(0., 600.);
                let scrolled = egui::ScrollArea::vertical()
                    .id_salt(owner.with("controls-scroll"))
                    .animated(false)
                    .min_scrolled_height(0.)
                    .max_height(height)
                    .show(ui, |ui| controls(ui, studio));
                #[cfg(test)]
                for composition in &studio.doc.motion.compositions {
                    let key = action_error_key(studio, composition.id);
                    if ui.data(|data| data.get_temp::<ActionError>(key)).is_some() {
                        eprintln!("Composition popup: available={available:?}, clip={clip:?}, max_height={height}, inner={:?}, content={:?}, offset={:?}", scrolled.inner_rect, scrolled.content_size, scrolled.state.offset);
                    }
                }
                #[cfg(not(test))]
                let _ = scrolled;
            });
        response
    })
    .inner;
    let popup = egui::Popup::default_response_id(&menu);
    if open {
        if !egui::Popup::is_any_open(ui.ctx()) || egui::Popup::is_id_open(ui.ctx(), popup) {
            egui::Popup::open_id(ui.ctx(), popup);
        }
    } else if egui::Popup::is_id_open(ui.ctx(), popup) {
        egui::Popup::close_id(ui.ctx(), popup);
    }
    ui.data_mut(|data| data.insert_temp(key, open));
    super::welcome::focus_control(ui, &menu);
}

/// Import the project returned by the existing native chooser.
/// Args: studio and chosen project path. Returns no value; validation and discrete commit remain unchanged.
fn import_project(studio: &mut Studio, path: &std::path::Path) {
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

/// Keep continuous composition properties in the accepted field scope.
/// Args: existing menu UI and studio. Returns after the original typed validation/commit path.
fn controls(ui: &mut Ui, studio: &mut Studio) {
    if super::keyboard_controls::button(ui, "add-project", "Add project…").clicked()
        && let Some(path) = rfd::FileDialog::new()
            .add_filter("Motion project", &["oma", "omamotion"])
            .pick_file()
    {
        import_project(studio, &path);
    }
    if super::keyboard_controls::button(ui, "export-project", "Export motion project…").clicked()
        && let Some(path) = rfd::FileDialog::new()
            .add_filter("Motion project", &["omamotion"])
            .set_file_name(format!("{}.omamotion", studio.doc.name))
            .save_file()
    {
        studio.status = match crate::motion::compositions::save_to(&studio.doc, &path) {
            Ok(()) => "Motion project saved".into(),
            Err(e) => e,
        };
    }
    let mut action = None;
    studio.with_field_history(ui, "composition-properties", |ui, studio| {
        let mut after = studio.doc.motion.clone();
        let mut changed = false;
        let mut fields = Vec::new();
        let mut remove = None;
        for comp in &mut after.compositions {
            let owner = egui::Id::new(("motion-composition", &studio.swap_id, comp.id));
            super::fields::native_scope(ui, owner, |ui| {
                let section = egui::CollapsingHeader::new(comp.name.clone())
                    .id_salt("composition")
                    .show(ui, |ui| {
                        let visible = ui.checkbox(&mut comp.visible, "Visible");
                        super::welcome::focus_control(ui, &visible);
                        changed |= visible.changed();
                        let name = ui.label("Composition name");
                        let response = ui
                            .add(egui::TextEdit::singleline(&mut comp.name).id(owner.with("name")));
                        super::welcome::name_control(ui, &response, name.id, "Composition name");
                        changed |= response.changed();
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
                            let purpose = format!("Composition {}", name.to_lowercase());
                            let (field, _) = number(ui, name, &purpose, value, 0.05, "");
                            changed |= field.changed;
                            fields.push(field);
                        }
                        let mut degrees = comp.rotation.to_degrees();
                        let (field, _) = number(
                            ui,
                            "rotation",
                            "Composition rotation (°)",
                            &mut degrees,
                            1.,
                            "°",
                        );
                        if field.changed {
                            comp.rotation = degrees.to_radians();
                            changed = true;
                        }
                        fields.push(field);
                        let looped = ui.checkbox(&mut comp.looped, "Loop source");
                        super::welcome::focus_control(ui, &looped);
                        changed |= looped.changed();
                        animation_controls(ui, studio, comp, &mut action);
                        if super::keyboard_controls::button(ui, "remove", "Remove composition")
                            .clicked()
                        {
                            remove = Some(comp.id);
                        }
                    });
                super::welcome::focus_control(ui, &section.header_response);
            });
        }
        if let Some(id) = remove {
            after.compositions.retain(|c| c.id != id);
            after.drop_shape(id);
            changed = true;
        }
        let mut accepted = true;
        if changed {
            after.duration = after
                .compositions
                .iter()
                .map(|c| c.end)
                .fold(after.duration, f32::max);
            let mut doc = studio.doc.clone();
            doc.motion = after.clone();
            match crate::motion::compositions::validate(&doc) {
                Ok(()) => {}
                Err(e) => {
                    let rejected = fields
                        .iter()
                        .filter(|field| field.changed)
                        .map(|field| &field.response)
                        .collect::<Vec<_>>();
                    super::welcome::control_error(ui, &rejected, &e);
                    studio.status = e;
                    accepted = false;
                }
            }
        }
        for field in fields {
            crate::app::input_history::record(field.response, accepted && field.changed);
        }
        if changed && accepted {
            studio.commit_motion(after);
        }
    });
    if let Some((action, fields)) = action {
        let id = match &action {
            Edit::Key { id, .. } | Edit::Expression { id, .. } | Edit::Remap { id, .. } => *id,
        };
        let key = action_error_key(studio, id);
        let accepted = match &action {
            Edit::Expression {
                id,
                property,
                source: Some(source),
            } => validate_expression(studio, *id, *property, source),
            _ => Ok(()),
        };
        match accepted.and_then(|()| apply_edit(studio, action)) {
            Ok(()) => {
                ui.data_mut(|data| data.remove::<ActionError>(key));
                let draft_key = animation_draft_key(studio, id);
                let remap_key = remap_draft_key(studio, id);
                ui.data_mut(|data| {
                    if let Some(mut draft) = data.get_temp::<AnimationDraft>(draft_key) {
                        draft.generation = studio.canvas_gen;
                        data.insert_temp(draft_key, draft);
                    }
                    if let Some(mut draft) = data.get_temp::<RemapDraft>(remap_key) {
                        draft.generation = studio.canvas_gen;
                        data.insert_temp(remap_key, draft);
                    }
                });
            }
            Err(error) => {
                studio.status = error.clone();
                ui.data_mut(|data| {
                    data.insert_temp(
                        key,
                        ActionError {
                            message: error,
                            fields,
                            revision: studio.canvas_gen,
                            reveal: true,
                        },
                    )
                });
                ui.ctx().request_repaint();
            }
        }
    }
}

/// Validate an explicit source draft with the existing bounded expression evaluator.
/// Args: current studio, concrete composition/property and proposed source.
/// Returns the existing parser/evaluation error without committing or changing the document.
fn validate_expression(
    studio: &Studio,
    id: u64,
    property: crate::motion::Prop,
    source: &str,
) -> Result<(), String> {
    let mut candidate = studio.doc.motion.clone();
    candidate.set_expression(id, property, Some(source.to_owned()))?;
    let mut keep = || true;
    crate::motion::expressions::Evaluation::new(&candidate, studio.playhead, &mut keep)
        .value(id, property)
        .map(|_| ())
        .map_err(|error| error.to_string())
}

#[derive(Clone)]
struct ActionError {
    message: String,
    fields: Vec<egui::Id>,
    revision: u64,
    reveal: bool,
}

/// Identify a rejected explicit edit by its actual document and composition.
/// Args: studio and persistent composition ID. Returns app-owned error identity without edited values.
fn action_error_key(studio: &Studio, id: u64) -> egui::Id {
    egui::Id::new(("composition-action-error", &studio.swap_id, id))
}

/// Decorate one existing f32 number while retaining its native step and unrestricted finite values.
/// Args: UI, stable property, visible/native purpose, draft, original speed and suffix.
/// Returns the authoritative response and owned validity; callers decide whether it is an artwork property.
fn number(
    ui: &mut Ui,
    property: &str,
    purpose: &str,
    value: &mut f32,
    speed: f64,
    suffix: &str,
) -> (super::fields::FieldResponse, bool) {
    super::keyboard_controls::motion_number(
        ui,
        ui.make_persistent_id(property),
        (purpose, purpose),
        value,
        -f32::MAX..=f32::MAX,
        speed,
        suffix,
        None,
    )
}

#[derive(Clone)]
struct AnimationDraft {
    generation: u64,
    instance: egui::Id,
    property: crate::motion::Prop,
    value: f32,
    ease: crate::motion::Ease,
    source: String,
    time: f32,
}

impl AnimationDraft {
    fn new(
        motion: &crate::motion::Motion,
        id: u64,
        time: f32,
        generation: u64,
        instance: egui::Id,
    ) -> Self {
        let mut draft = Self {
            generation,
            instance,
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

/// Decorate the existing easing choice and four unrestricted native controls.
/// Args: UI, draft curve and native purpose. Returns parser/model validity and actual control IDs.
fn easing(ui: &mut Ui, ease: &mut crate::motion::Ease, purpose: &str) -> (bool, Vec<egui::Id>) {
    use crate::motion::Ease;
    let label = ui.label(purpose);
    let mut chosen = false;
    let picker = egui::ComboBox::from_id_salt("easing")
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
                let response = ui.selectable_value(ease, option, option.name());
                super::welcome::focus_control(ui, &response);
                chosen |= response.clicked();
            }
        });
    if chosen {
        picker.response.request_focus();
    }
    super::welcome::name_control(ui, &picker.response, label.id, purpose);
    let mut ids = vec![picker.response.id];
    let mut valid = true;
    if let Ease::CubicBezier { x1, y1, x2, y2 } = ease {
        for (label, value) in [("X1", x1), ("Y1", y1), ("X2", x2), ("Y2", y2)] {
            let (field, accepted) =
                number(ui, label, &format!("{purpose} {label}"), value, 0.01, "");
            ids.push(field.response.id);
            valid &= accepted;
        }
    }
    if !ease.valid() {
        super::welcome::control_error(
            ui,
            &[&picker.response],
            "Use finite curve controls within ±10000 and X controls from 0 to 1; Y overshoot is allowed",
        );
        valid = false;
    }
    (valid, ids)
}

/// Reveal one rejected explicit edit only while its concrete controls and revision remain current.
/// Args: UI, studio and persistent composition. Returns no value and never changes document history.
fn action_error(ui: &mut Ui, studio: &Studio, id: u64, group: &[egui::Id]) {
    let key = action_error_key(studio, id);
    if let Some(mut error) = ui.data(|data| data.get_temp::<ActionError>(key)) {
        if error.revision != studio.canvas_gen {
            ui.data_mut(|data| data.remove::<ActionError>(key));
            return;
        }
        if !error.fields.iter().any(|id| group.contains(id)) {
            return;
        }
        let responses = error
            .fields
            .iter()
            .filter_map(|id| ui.ctx().read_response(*id))
            .collect::<Vec<_>>();
        if !responses.is_empty() {
            let shown = ui.scope(|ui| {
                ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
                let fields = responses.iter().collect::<Vec<_>>();
                if error.reveal {
                    super::welcome::reveal_control_error(ui, &fields, &error.message)
                } else {
                    super::welcome::control_error(ui, &fields, &error.message);
                    false
                }
            });
            if error.reveal && shown.inner
                && ui.clip_rect().contains_rect(shown.response.rect)
                && responses.iter().all(|field| ui.clip_rect().contains_rect(field.rect))
            {
                error.reveal = false;
                ui.data_mut(|data| data.insert_temp(key, error));
            }
        }
    }
}

#[derive(Clone)]
struct RemapDraft {
    generation: u64,
    instance: egui::Id,
    value: f32,
    ease: crate::motion::Ease,
}

/// Scope an animation draft to its concrete source owner.
/// Args: studio and composition identity. Returns an app-owned key without edited values.
fn animation_draft_key(studio: &Studio, id: u64) -> egui::Id {
    egui::Id::new(("composition-animation-draft", &studio.swap_id, id))
}

/// Scope a source-time draft to the current parent key target.
/// Args: studio and composition identity. Returns a key for this captured playhead target.
fn remap_draft_key(studio: &Studio, id: u64) -> egui::Id {
    egui::Id::new((
        "composition-remap-draft",
        &studio.swap_id,
        id,
        studio.playhead.to_bits(),
    ))
}

fn animation_controls(
    ui: &mut Ui,
    studio: &Studio,
    comp: &Composition,
    action: &mut Option<(Edit, Vec<egui::Id>)>,
) {
    let section = ui.collapsing("Animate", |ui| {
        let key = animation_draft_key(studio, comp.id);
        let mut draft = ui
            .data(|data| data.get_temp::<AnimationDraft>(key))
            .filter(|draft| draft.generation == studio.canvas_gen)
            .unwrap_or_else(|| {
                AnimationDraft::new(
                    &studio.doc.motion,
                    comp.id,
                    studio.playhead,
                    studio.canvas_gen,
                    super::keyboard_controls::draft_id(ui.ctx(), "composition-animation"),
                )
            });
        let previous = draft.property;
        let options = properties()
            .into_iter()
            .map(|prop| (prop, prop.name().to_owned()))
            .collect::<Vec<_>>();
        let property = super::keyboard_controls::choice(
            ui,
            "property",
            "Composition key property",
            &mut draft.property,
            &options,
        );
        if previous != draft.property || draft.time != studio.playhead {
            draft.time = studio.playhead;
            draft.read(&studio.doc.motion, comp.id, previous != draft.property);
        }
        let field_owner = ui.make_persistent_id((
            "key-fields",
            draft.instance,
            draft.property,
            studio.playhead.to_bits(),
        ));
        super::fields::native_scope(ui, field_owner, |ui| {
            ui.label(format!("Key at {:.3} s", studio.playhead));
            let (field, valid) = number(
                ui,
                "value",
                "Composition key value",
                &mut draft.value,
                0.1,
                "",
            );
            ui.label(match draft.property {
                crate::motion::Prop::Rotation => "Rotation in degrees",
                crate::motion::Prop::Opacity => "Opacity multiplier · 0–1",
                crate::motion::Prop::Scale
                | crate::motion::Prop::Width
                | crate::motion::Prop::Height => "Scale multiplier",
                _ => "Offset from composition position",
            });
            let (curve_valid, mut controls) = easing(ui, &mut draft.ease, "Composition key easing");
            controls.push(field.response.id);
            if field.response.changed() || property.changed() {
                ui.data_mut(|data| data.remove::<ActionError>(action_error_key(studio, comp.id)));
            }
            ui.horizontal(|ui| {
                if ui
                    .add_enabled_ui(valid && curve_valid, |ui| {
                        super::keyboard_controls::button(ui, "set-key", "Set key")
                    })
                    .inner
                    .clicked()
                {
                    *action = Some((
                        Edit::Key {
                            id: comp.id,
                            property: draft.property,
                            value: draft.value,
                            ease: draft.ease,
                            remove: false,
                        },
                        controls.clone(),
                    ));
                }
                let remove = super::keyboard_controls::button(ui, "remove-key", "Remove key");
                controls.push(remove.id);
                if remove.clicked() {
                    *action = Some((
                        Edit::Key {
                            id: comp.id,
                            property: draft.property,
                            value: draft.value,
                            ease: draft.ease,
                            remove: true,
                        },
                        vec![remove.id],
                    ));
                }
            });
            action_error(ui, studio, comp.id, &controls);
        });
        ui.separator();
        let label = ui.label("Expression");
        let expression_owner = egui::Id::new((
            "composition-expression",
            &studio.swap_id,
            comp.id,
            draft.property,
            draft.instance,
        ));
        let source = ui.add(
            egui::TextEdit::multiline(&mut draft.source)
                .id(expression_owner)
                .code_editor()
                .lock_focus(false)
                .desired_rows(3)
                .hint_text("value + sin(time * tau) * 40"),
        );
        super::welcome::name_control(ui, &source, label.id, "Composition expression");
        if source.changed() {
            let error = action_error_key(studio, comp.id);
            ui.data_mut(|data| data.remove::<ActionError>(error));
        }
        action_error(ui, studio, comp.id, &[source.id]);
        ui.label("time in seconds · expression rotation in radians");
        let mut expression_controls = Vec::new();
        ui.horizontal(|ui| {
            if super::keyboard_controls::button(ui, "apply-expression", "Apply expression")
                .clicked()
            {
                *action = Some((
                    Edit::Expression {
                        id: comp.id,
                        property: draft.property,
                        source: Some(draft.source.clone()),
                    },
                    vec![source.id],
                ));
            }
            let remove =
                super::keyboard_controls::button(ui, "remove-expression", "Remove expression");
            expression_controls.push(remove.id);
            if remove.clicked() {
                *action = Some((
                    Edit::Expression {
                        id: comp.id,
                        property: draft.property,
                        source: None,
                    },
                    vec![remove.id],
                ));
                draft.source.clear();
            }
        });
        action_error(ui, studio, comp.id, &expression_controls);
        if let Err(error) = studio
            .doc
            .motion
            .evaluate_frame(studio.playhead, &mut || true)
        {
            let message = ui.colored_label(egui::Color32::LIGHT_RED, error.to_string());
            ui.ctx().accesskit_node_builder(message.id, |node| {
                node.set_live(egui::accesskit::Live::Polite);
                node.set_live_atomic();
            });
        }
        ui.data_mut(|data| data.insert_temp(key, draft));
    });
    super::welcome::focus_control(ui, &section.header_response);
    let section = ui.collapsing("Time remapping", |ui| {
        let key = remap_draft_key(studio, comp.id);
        let mut draft = ui
            .data(|data| data.get_temp::<RemapDraft>(key))
            .filter(|draft| draft.generation == studio.canvas_gen)
            .unwrap_or_else(|| RemapDraft {
                generation: studio.canvas_gen,
                instance: super::keyboard_controls::draft_id(ui.ctx(), "composition-remap"),
                value: comp
                    .local_time(studio.playhead)
                    .unwrap_or(comp.source_start),
                ease: crate::motion::Ease::Linear,
            });
        super::fields::native_scope(ui, draft.instance.with("fields"), |ui| {
            ui.label(format!("Parent time {:.3} s", studio.playhead));
            let (field, valid) = number(
                ui,
                "source-seconds",
                "Composition source seconds",
                &mut draft.value,
                0.05,
                "",
            );
            let (curve_valid, mut fields) =
                easing(ui, &mut draft.ease, "Composition source easing");
            fields.push(field.response.id);
            if field.response.changed() {
                let error = action_error_key(studio, comp.id);
                ui.data_mut(|data| data.remove::<ActionError>(error));
            }
            ui.horizontal(|ui| {
                for (label, remove, clear) in [
                    ("Set source key", false, false),
                    ("Remove source key", true, false),
                    ("Clear remapping", false, true),
                ] {
                    let response = ui
                        .add_enabled_ui(remove || clear || (valid && curve_valid), |ui| {
                            super::keyboard_controls::button(ui, label, label)
                        })
                        .inner;
                    fields.push(response.id);
                    if response.clicked() {
                        *action = Some((
                            Edit::Remap {
                                id: comp.id,
                                value: draft.value,
                                ease: draft.ease,
                                remove,
                                clear,
                            },
                            if remove || clear {
                                vec![response.id]
                            } else {
                                fields.clone()
                            },
                        ));
                    }
                }
            });
            action_error(ui, studio, comp.id, &fields);
            ui.data_mut(|data| data.insert_temp(key, draft));
            ui.label(format!("{} source-time keys", comp.time_remap.len()));
            egui::ScrollArea::vertical()
                .id_salt("source-time-keys")
                .max_height(120.)
                .show(ui, |ui| {
                    for key in &comp.time_remap {
                        ui.label(format!(
                            "{:.3} s → {:.3} s · {}",
                            key.t,
                            key.value,
                            key.ease.name()
                        ));
                    }
                });
        });
    });
    super::welcome::focus_control(ui, &section.header_response);
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

#[cfg(test)]
mod keyboard_tests {
    use super::*;
    use crate::{
        document::{Document, Layer},
        geom::Pt,
        motion::{Ease, Prop},
        ui::welcome::keyboard_tests as keys,
    };
    use egui::{Event, Key, Modifiers};

    struct Native {
        root: std::path::PathBuf,
        ctx: egui::Context,
        studio: Studio,
        id: u64,
    }

    impl Native {
        fn new() -> Self {
            let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join(".artifacts")
                .join(format!(
                    "keyboard-composition-{}-{}",
                    std::process::id(),
                    crate::document::next_id()
                ));
            std::fs::create_dir_all(&root).unwrap();
            let mut source = Document::new("Imported artwork", 16., 16., 96.);
            source.layers.clear();
            source.artboards.clear();
            source.artboardless = true;
            source.transparent = true;
            let mut layer = Layer::vector("Native source artwork");
            layer
                .kind
                .shapes_mut()
                .unwrap()
                .push(crate::layout::make_frame(Pt::new(1., 1.), Pt::splat(4.)));
            source.layers.push(layer);
            let path = root.join("source.oma");
            crate::project::save_to(&source, &path).unwrap();
            let ctx = egui::Context::default();
            ctx.enable_accesskit();
            crate::ui::theme::apply(&ctx);
            let mut studio = Studio::new();
            studio.doc = Document::new("Native composition", 48., 32., 96.);
            studio.doc.layers.clear();
            studio.doc.artboards.clear();
            studio.doc.artboardless = true;
            studio.doc.transparent = true;
            studio.persona = crate::tools::Persona::Motion;
            studio.show_welcome = false;
            assert!(studio.selection.is_empty());
            assert!(studio.doc.motion.compositions.is_empty());
            let mut f = Self {
                root,
                ctx,
                studio,
                id: 0,
            };
            f.frame(false, vec![]);
            f.press("Compositions");
            let ctx = f.ctx.clone();
            let output = keys::tab_to(&ctx, "Add project…", |events| f.frame(false, events));
            keys::focused(&ctx, &output, "Add project…");
            import_project(&mut f.studio, &path);
            assert_eq!(f.studio.history.len(), 1);
            assert_eq!(f.studio.doc.motion.compositions.len(), 1);
            assert!(f.studio.selection.is_empty());
            f.id = f.studio.doc.motion.compositions[0].id;
            f.frame(false, vec![]);
            f.press("Imported artwork");
            f
        }
        fn frame(&mut self, preceding: bool, events: Vec<Event>) -> egui::FullOutput {
            let ctx = self.ctx.clone();
            keys::render(&ctx, events, |ui| {
                if preceding {
                    ui.label("Conditional native status");
                    let _ = ui.button("Conditional action");
                }
                self.studio.handle_shortcuts(ui.ctx());
                egui::Panel::bottom("native-composition-timeline")
                    .exact_size(350.)
                    .show(ui, |ui| crate::ui::timeline::content(ui, &mut self.studio));
                crate::ui::canvas::show(ui, &mut self.studio);
            })
        }
        fn press(&mut self, label: &str) -> egui::FullOutput {
            let ctx = self.ctx.clone();
            keys::tab_to(&ctx, label, |events| self.frame(false, events));
            self.frame(false, keys::key(Key::Enter, Modifiers::NONE));
            self.frame(false, vec![])
        }
        fn text(&mut self, label: &str, value: &str) -> egui::FullOutput {
            let ctx = self.ctx.clone();
            keys::tab_to(&ctx, label, |events| self.frame(false, events));
            keys::replace(value, |events| self.frame(false, events));
            self.frame(false, vec![])
        }
        fn close(&mut self) {
            for _ in 0..4 {
                if !egui::Popup::is_any_open(&self.ctx) {
                    break;
                }
                self.frame(false, keys::key(Key::Escape, Modifiers::NONE));
                self.frame(false, vec![]);
            }
            assert!(!egui::Popup::is_any_open(&self.ctx));
            self.frame(false, keys::key(Key::F8, Modifiers::NONE));
            self.frame(false, vec![]);
        }
    }
    impl Drop for Native {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }

    #[test]
    fn keyboard_composition_imported_native_fields_keys_expressions_remap_undo_and_reopen() {
        let mut f = Native::new();
        let output = f.frame(false, vec![]);
        let x = keys::named(&output, "Composition x").0;
        f.text("Composition name", "Renamed native source");
        let output = f.frame(true, vec![]);
        assert_eq!(keys::named(&output, "Composition x").0, x);
        f.text("Composition x", "1.25");
        assert_eq!(f.studio.doc.motion.compositions[0].position.x, 1.25);
        f.press("Animate");
        let before = f.studio.doc.motion.clone();
        f.text("Composition key value", "12.125");
        assert_eq!(
            f.studio.doc.motion, before,
            "An animation draft cannot commit before Set key"
        );
        f.press("Set key");
        assert_eq!(
            f.studio
                .doc
                .motion
                .tracks
                .iter()
                .find(|track| track.shape == f.id && track.prop == Prop::X)
                .unwrap()
                .keys[0]
                .value,
            12.125
        );
        f.text("Composition expression", "sin(");
        let before = f.studio.doc.motion.clone();
        f.press("Apply expression");
        let output = f.frame(false, vec![]);
        assert_eq!(f.studio.doc.motion, before);
        keys::associated_error(&output, "Composition expression");
        keys::focused(&f.ctx, &output, "Composition expression");
        f.text("Composition expression", "value + time * 2");
        f.press("Apply expression");
        assert_eq!(
            f.studio
                .doc
                .motion
                .expressions
                .iter()
                .find(|expression| expression.shape == f.id && expression.prop == Prop::X)
                .unwrap()
                .source,
            "value + time * 2"
        );
        f.press("Time remapping");
        f.text("Composition source seconds", "0.25");
        f.press("Composition source easing");
        f.press("Hold");
        let before = f.studio.doc.motion.clone();
        f.press("Set source key");
        let key = &f.studio.doc.motion.compositions[0].time_remap[0];
        assert_eq!(key.value, 0.25);
        assert_eq!(key.ease, Ease::Hold);
        let authored = f.studio.doc.motion.clone();
        let path = f.root.join("authored.omamotion");
        crate::motion::compositions::save_to(&f.studio.doc, &path).unwrap();
        assert_eq!(
            crate::motion::compositions::load_from(&path)
                .unwrap()
                .motion,
            authored
        );
        let path = f.root.join("authored.oma");
        crate::project::save_to(&f.studio.doc, &path).unwrap();
        assert_eq!(
            crate::project::encode(&crate::project::load_from(&path).unwrap()).unwrap(),
            crate::project::encode(&f.studio.doc).unwrap()
        );
        f.close();
        f.frame(
            false,
            keys::key(Key::Z, Modifiers::CTRL | Modifiers::COMMAND),
        );
        assert_eq!(f.studio.doc.motion, before);
        f.frame(
            false,
            keys::key(
                Key::Z,
                Modifiers::CTRL | Modifiers::COMMAND | Modifiers::SHIFT,
            ),
        );
        assert_eq!(f.studio.doc.motion, authored);
        assert!(f.studio.selection.is_empty());
    }

    #[test]
    fn keyboard_composition_invalid_native_drafts_and_external_revisions_do_not_cross_owners() {
        let mut f = Native::new();
        let before = f.studio.doc.motion.clone();
        let output = f.text("Composition start", "-");
        keys::associated_error(&output, "Composition start");
        assert_eq!(f.studio.doc.motion, before);
        f.text("Composition start", "0");
        f.press("Animate");
        assert_eq!(
            f.studio.doc.motion, before,
            "Opening native Animate must not set a key"
        );
        let output = f.text("Composition key value", "-");
        keys::associated_error(&output, "Composition key value");
        assert!(keys::named(&output, "Set key").1.is_disabled());
        assert_eq!(f.studio.doc.motion, before);
        let output = f.text("Composition key value", "25");
        let old = keys::named(&output, "Composition key value").0;
        assert!(!keys::named(&output, "Set key").1.is_disabled());
        assert_eq!(f.studio.doc.motion, before);
        let mut external = f.studio.doc.motion.clone();
        external.looped = !external.looped;
        f.studio.commit_motion_step(external.clone());
        let output = f.frame(false, vec![]);
        let current = keys::named(&output, "Composition key value");
        assert_ne!(current.0, old);
        assert_eq!(current.1.numeric_value(), Some(0.));
        assert_eq!(f.studio.doc.motion, external);
        let action = keys::named(&output, "Set key").0;
        f.studio.updates.freezing = true;
        f.frame(
            false,
            keys::action(action, egui::accesskit::Action::Click, None),
        );
        assert_eq!(f.studio.doc.motion, external);
        f.studio.updates.freezing = false;
        f.studio.swap_id.push_str("-reopened-document");
        f.frame(false, vec![]);
        f.press("Compositions");
        f.press("Imported artwork");
        f.press("Animate");
        let output = f.frame(false, vec![]);
        assert_ne!(keys::named(&output, "Composition key value").0, current.0);
        assert_eq!(f.studio.doc.motion, external);
    }
}
