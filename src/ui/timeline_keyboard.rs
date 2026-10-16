//! Keyboard access to the existing motion keys and discrete motion commands.

use crate::{
    app::Studio,
    color::Rgba,
    motion::{self, Ease, Key, Prop},
    tools::Persona,
};
use eframe::egui::{self, Id, Response, Ui};

const ID: &str = "timeline-keyboard";
const TIME: &str = "Keyframe time (s)";
const VALUE: &str = "Keyframe value";
const COLOR: &str = "Keyframe color (RGBA hex)";
const EASE: &str = "Keyframe easing";
const CONTROLS: [&str; 4] = ["Bezier X1", "Bezier Y1", "Bezier X2", "Bezier Y2"];

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Curve {
    Linear,
    In,
    Out,
    InOut,
    Hold,
    Bezier,
}

impl Curve {
    const ALL: [Self; 6] = [
        Self::Linear,
        Self::In,
        Self::Out,
        Self::InOut,
        Self::Hold,
        Self::Bezier,
    ];

    /// Name one supported curve mode.
    /// Args: curve mode. Returns the visible picker name without changing its control points.
    fn name(self) -> &'static str {
        match self {
            Self::Linear => "Linear",
            Self::In => "Ease in",
            Self::Out => "Ease out",
            Self::InOut => "Ease in-out",
            Self::Hold => "Hold",
            Self::Bezier => "Cubic Bezier",
        }
    }
}

#[derive(Clone)]
struct Draft {
    document: String,
    generation: u64,
    selection: Vec<(usize, u64)>,
    target: u64,
    prop: Prop,
    source: Option<(usize, Key)>,
    time: String,
    value: String,
    curve: Curve,
    controls: [String; 4],
}

impl Draft {
    /// Capture a key or a new key's target.
    /// Args: editor, persistent target, channel and optional original key. Returns an isolated text draft.
    fn new(studio: &Studio, target: u64, prop: Prop, source: Option<(usize, Key)>) -> Self {
        let key = source.as_ref().map(|(_, key)| key);
        let ease = key.map_or(Ease::EaseInOut, |key| key.ease);
        let (curve, controls) = match ease {
            Ease::Linear => (Curve::Linear, [0.25, 0.1, 0.25, 1.]),
            Ease::EaseIn => (Curve::In, [0.25, 0.1, 0.25, 1.]),
            Ease::EaseOut => (Curve::Out, [0.25, 0.1, 0.25, 1.]),
            Ease::EaseInOut => (Curve::InOut, [0.25, 0.1, 0.25, 1.]),
            Ease::Hold => (Curve::Hold, [0.25, 0.1, 0.25, 1.]),
            Ease::CubicBezier { x1, y1, x2, y2 } => (Curve::Bezier, [x1, y1, x2, y2]),
        };
        let time = key.map_or(studio.playhead, |key| key.t);
        let value = if prop == Prop::Fill {
            key.and_then(|key| key.color)
                .or_else(|| studio.doc.motion.color_value(target, prop, time))
                .or_else(|| rest_color(studio, target))
                .map_or_else(String::new, Rgba::hex)
        } else {
            key.map(|key| key.value)
                .or_else(|| studio.doc.motion.value(target, prop, time))
                .unwrap_or_else(|| {
                    if prop == Prop::Opacity {
                        motion::selection(&studio.doc, target)
                            .and_then(|(layer, id)| motion::target(&studio.doc, layer, id))
                            .map_or(1., |target| target.opacity)
                    } else {
                        prop.identity()
                    }
                })
                .to_string()
        };
        Self {
            document: studio.swap_id.clone(),
            generation: studio.canvas_gen,
            selection: studio.selection.clone(),
            target,
            prop,
            time: time.to_string(),
            value,
            curve,
            controls: controls.map(|value| value.to_string()),
            source,
        }
    }

    /// Capture the editor's selected key by its existing tuple identity.
    /// Args: editor. Returns a draft only while the exact target, channel and index exist.
    fn selected(studio: &Studio) -> Option<Self> {
        let (target, prop, index) = studio.selected_key?;
        let key = key_at(studio, target, prop, index)?.clone();
        Some(Self::new(studio, target, prop, Some((index, key))))
    }

    /// Check the captured document, selection and original key.
    /// Args: editor. Returns false after another edit, selection change, document switch or key reorder.
    fn current(&self, studio: &Studio) -> bool {
        self.document == studio.swap_id
            && self.generation == studio.canvas_gen
            && self.selection == studio.selection
            && studio.persona == Persona::Motion
            && match &self.source {
                Some((index, key)) => {
                    studio.selected_key == Some((self.target, self.prop, *index))
                        && key_at(studio, self.target, self.prop, *index) == Some(key)
                }
                None => studio.selected_key.is_none(),
            }
    }

    /// Validate text without changing the model or limiting curves to presets.
    /// Args: clip duration. Returns a native key or errors associated with their actual fields.
    fn validate(&self, duration: f32) -> Result<Key, Vec<(&'static str, String)>> {
        let mut errors = Vec::new();
        let time = finite(&self.time).filter(|time| {
            duration.is_finite() && duration > 0. && *time >= 0. && *time <= duration
        });
        if time.is_none() {
            errors.push((
                TIME,
                format!("Enter a finite time from 0 to {duration} seconds"),
            ));
        }
        let value = if self.prop == Prop::Fill {
            Some(0.)
        } else {
            finite(&self.value)
        };
        let color = if self.prop == Prop::Fill {
            Rgba::parse_hex(&self.value)
        } else {
            None
        };
        if self.prop == Prop::Fill && color.is_none() {
            errors.push((COLOR, "Enter a color such as #336699 or #33669980".into()));
        } else if value.is_none() {
            errors.push((VALUE, "Enter a finite keyframe value".into()));
        }
        let ease = match self.curve {
            Curve::Linear => Ease::Linear,
            Curve::In => Ease::EaseIn,
            Curve::Out => Ease::EaseOut,
            Curve::InOut => Ease::EaseInOut,
            Curve::Hold => Ease::Hold,
            Curve::Bezier => {
                let mut values = [0.; 4];
                for (index, text) in self.controls.iter().enumerate() {
                    match finite(text) {
                        Some(value) => values[index] = value,
                        None => {
                            errors.push((CONTROLS[index], "Enter a finite curve control".into()))
                        }
                    }
                }
                Ease::CubicBezier {
                    x1: values[0],
                    y1: values[1],
                    x2: values[2],
                    y2: values[3],
                }
            }
        };
        if !ease.valid() {
            errors.push((EASE, "Use X controls from 0 to 1 and Y controls from -10000 to 10000; overshoot is allowed".into()));
        }
        if !errors.is_empty() {
            return Err(errors);
        }
        let time = time.unwrap();
        Ok(if let Some(color) = color {
            Key::color_at(time, color, ease)
        } else {
            Key::at(time, value.unwrap(), ease)
        })
    }
}

/// Read a finite model number from native text.
/// Args: field text. Returns a finite f32 with no rounding or artificial magnitude bound.
fn finite(text: &str) -> Option<f32> {
    text.trim()
        .replace('−', "-")
        .parse()
        .ok()
        .filter(|value: &f32| value.is_finite())
}

/// Read one exact key through the persisted target, property and index.
/// Args: editor and tuple identity. Returns the current key, or none for a stale tuple.
fn key_at(studio: &Studio, target: u64, prop: Prop, index: usize) -> Option<&Key> {
    studio
        .doc
        .motion
        .tracks
        .iter()
        .find(|track| track.shape == target && track.prop == prop)?
        .keys
        .get(index)
}

/// Read the designed color used by the existing fill-key command.
/// Args: editor and persistent target. Returns a solid rest color without changing gradients or images.
fn rest_color(studio: &Studio, target: u64) -> Option<Rgba> {
    let (layer, id) = motion::selection(&studio.doc, target)?;
    match &studio.doc.find_shape(layer, id)?.style.fill {
        crate::document::Fill::Solid(color) => Some(*color),
        _ => None,
    }
}

/// Describe an existing animation target.
/// Args: editor and persistent target. Returns a unique, readable object or raster-layer name.
fn target_name(studio: &Studio, target: u64) -> String {
    let name = motion::selection(&studio.doc, target).and_then(|(layer, id)| {
        if id == crate::document::RASTER_ID {
            studio
                .doc
                .layers
                .get(layer)
                .map(|layer| layer.name.as_str())
        } else {
            studio
                .doc
                .find_shape(layer, id)
                .map(|shape| shape.name.as_str())
        }
    });
    format!("{} #{target}", name.unwrap_or("Missing target"))
}

/// Explain why motion authoring is unavailable.
/// Args: editor. Returns the active busy guard, or none when a discrete edit can proceed.
fn busy(studio: &Studio) -> Option<&'static str> {
    if studio.persona != Persona::Motion || studio.show_welcome {
        Some("Open a document in Motion to edit keyframes")
    } else if studio.file_dialog_pending() || studio.updates.freezing {
        Some("Finish the open dialog before editing keyframes")
    } else if studio.playing {
        Some("Pause playback before editing keyframes")
    } else if studio.op.is_some()
        || studio.type_edit.is_some()
        || studio.free_transform.is_some()
        || studio.deformation.is_some()
        || studio.pending_place.is_some()
        || studio.key_drag.is_some()
        || !studio.pose_drag.is_empty()
    {
        Some("Finish the current edit before editing keyframes")
    } else {
        None
    }
}

/// Resolve an unlocked, visible target in the current selection.
/// Args: editor and persistent target. Returns its canvas selection or a rejection message.
fn editable(studio: &Studio, target: u64) -> Result<(usize, u64), &'static str> {
    let hit =
        motion::selection(&studio.doc, target).ok_or("The keyframe target no longer exists")?;
    if !studio.selection.contains(&hit) {
        return Err("Select the keyframe target again");
    }
    if motion::target(&studio.doc, hit.0, hit.1).is_none() {
        return Err("The keyframe target is hidden or locked");
    }
    Ok(hit)
}

/// Commit one validated Apply or Add action.
/// Args: editor and captured draft. Returns after preserving native duplicate-time and sorting semantics.
fn apply(studio: &mut Studio, draft: &Draft) -> Result<(), String> {
    if let Some(error) = busy(studio) {
        return Err(error.into());
    }
    if !draft.current(studio) {
        return Err("The document, selection or key changed. Reset the draft.".into());
    }
    let hit = editable(studio, draft.target).map_err(str::to_owned)?;
    let key = draft
        .validate(studio.doc.motion.duration)
        .map_err(|errors| errors[0].1.clone())?;
    let mut after = studio.doc.motion.clone();
    if let Some((index, source)) = &draft.source
        && source.t != key.t
    {
        after.remove_key(draft.target, draft.prop, *index);
    }
    if let Some(color) = key.color {
        let missing = !after.tracks.iter().any(|track| {
            track.shape == draft.target
                && track.prop == Prop::Fill
                && track.keys.iter().any(|key| key.color.is_some())
        });
        if missing && key.t > 1e-3 {
            let rest = rest_color(studio, draft.target)
                .ok_or("This target has no solid rest fill to animate")?;
            after.set_color_key(draft.target, 0., rest, Ease::EaseInOut);
        }
        after.set_color_key(draft.target, key.t, color, key.ease);
    } else {
        if draft.source.is_none()
            && draft.prop == Prop::Opacity
            && key.t > 0.001
            && after.value(draft.target, Prop::Opacity, 0.).is_none()
        {
            let rest = motion::target(&studio.doc, hit.0, hit.1).unwrap().opacity;
            after.set_key(draft.target, Prop::Opacity, 0., rest, Ease::EaseInOut);
        }
        after.set_key(draft.target, draft.prop, key.t, key.value, key.ease);
    }
    let (index, time) = after
        .tracks
        .iter()
        .find(|track| track.shape == draft.target && track.prop == draft.prop)
        .and_then(|track| {
            track
                .keys
                .iter()
                .enumerate()
                .find(|(_, candidate)| (candidate.t - key.t).abs() < 1. / 120.)
        })
        .map(|(index, key)| (index, key.t))
        .ok_or("The keyframe could not be located after editing")?;
    studio.commit_motion_step(after);
    studio.selected_key = Some((draft.target, draft.prop, index));
    studio.playhead = time;
    studio.status = "keyframe applied".into();
    Ok(())
}

/// Remove one captured key through the existing motion commands.
/// Args: editor and current draft. Returns an error for new, stale, busy or unavailable targets.
fn remove(studio: &mut Studio, draft: &Draft) -> Result<(), String> {
    if let Some(error) = busy(studio) {
        return Err(error.into());
    }
    if !draft.current(studio) {
        return Err("The document, selection or key changed. Reset the draft.".into());
    }
    editable(studio, draft.target).map_err(str::to_owned)?;
    let (index, _) = draft
        .source
        .as_ref()
        .ok_or("Choose an existing keyframe to remove")?;
    let mut after = studio.doc.motion.clone();
    after.remove_key(draft.target, draft.prop, *index);
    let next = after.key_after_remove(draft.target, draft.prop, *index);
    studio.commit_motion_step(after);
    studio.selected_key = next;
    if let Some((target, prop, index)) = next {
        studio.playhead = key_at(studio, target, prop, index).unwrap().t;
    }
    studio.status = "keyframe removed".into();
    Ok(())
}

/// Paint native focus without replacing widget interaction.
/// Args: UI and actual widget response. Returns no value.
fn focus(ui: &Ui, response: &Response) {
    if response.gained_focus() {
        response.scroll_to_me(Some(egui::Align::Center));
    }
    if response.has_focus() {
        ui.painter().rect_stroke(
            response.rect.expand(2.),
            3.,
            egui::Stroke::new(2., ui.visuals().selection.stroke.color),
            egui::StrokeKind::Outside,
        );
    }
}

/// Give a native control its purpose and visible label relationship.
/// Args: UI, widget response, label, purpose, role and optional current choice. Returns no value.
fn name(
    ui: &Ui,
    response: &Response,
    label: Id,
    purpose: &str,
    kind: egui::WidgetType,
    value: Option<&str>,
) {
    response.clone().labelled_by(label);
    response.widget_info(|| {
        let mut info = egui::WidgetInfo::labeled(kind, response.enabled(), purpose);
        info.current_text_value = value.map(str::to_owned);
        info
    });
    focus(ui, response);
}

/// Draw an explicitly identified native text draft.
/// Args: UI, purpose and draft. Returns its actual response for validation associations.
fn input(ui: &mut Ui, purpose: &str, text: &mut String) -> Response {
    let label = ui.label(purpose);
    let response = ui.add(
        egui::TextEdit::singleline(text)
            .id(ui.make_persistent_id(purpose))
            .desired_width(220.),
    );
    name(
        ui,
        &response,
        label.id,
        purpose,
        egui::WidgetType::TextEdit,
        Some(text),
    );
    response
}

/// Draw a stable native action inside its current target and key scope.
/// Args: UI, purpose and availability. Returns the actual button response with visible focus.
fn action(ui: &mut Ui, purpose: &str, enabled: bool) -> Response {
    let id = ui.make_persistent_id(("action", purpose));
    let response = super::fields::native_scope(ui, id, |ui| {
        ui.add_enabled(enabled, egui::Button::new(purpose))
    })
    .inner;
    focus(ui, &response);
    response
}

/// Associate visible validation with the native field and a polite live message.
/// Args: UI, optional field response and error. Returns no value; no reader announcement is assumed.
fn error(ui: &mut Ui, field: Option<&Response>, message: &str) {
    let label = ui.colored_label(ui.visuals().error_fg_color, message);
    ui.ctx().accesskit_node_builder(label.id, |node| {
        node.set_live(egui::accesskit::Live::Polite);
        node.set_live_atomic();
    });
    if let Some(field) = field {
        ui.ctx().accesskit_node_builder(field.id, |node| {
            node.set_invalid(egui::accesskit::Invalid::True);
            node.set_error_message(label.id.accesskit_id());
            node.push_described_by(label.id.accesskit_id());
        });
    }
}

/// Offer keyboard editing alongside the existing timeline transport.
/// Args: transport UI and editor. Returns no value; model changes use discrete motion commands.
pub(super) fn show(ui: &mut Ui, studio: &mut Studio) {
    if studio.persona != Persona::Motion {
        return;
    }
    let id = ui.make_persistent_id((ID, &studio.swap_id));
    let key = id.with("menu-open");
    let mut open = ui.data(|data| data.get_temp::<bool>(key)).unwrap_or(false);
    let response = super::fields::native_scope(ui, id, |ui| {
        let response = ui.button("Keyframes");
        if response.clicked() {
            open = !open;
        }
        egui::Popup::menu(&response)
            .open_bool(&mut open)
            .show(|ui| {
                if !response.enabled() {
                    ui.disable();
                }
                ui.set_min_width(300.);
                egui::ScrollArea::vertical()
                    .max_height(430.)
                    .show(ui, |ui| editor(ui, studio));
            });
        response
    })
    .inner;
    let popup = egui::Popup::default_response_id(&response);
    if open {
        if !egui::Popup::is_any_open(ui.ctx()) || egui::Popup::is_id_open(ui.ctx(), popup) {
            egui::Popup::open_id(ui.ctx(), popup);
        }
    } else if egui::Popup::is_id_open(ui.ctx(), popup) {
        egui::Popup::close_id(ui.ctx(), popup);
    }
    ui.data_mut(|data| data.insert_temp(key, open));
    focus(ui, &response);
}

/// Draw the key picker and one captured edit draft.
/// Args: menu UI and editor. Returns no value; changing targets creates a fresh draft.
fn editor(ui: &mut Ui, studio: &mut Studio) {
    let state_id = Id::new((ID, "draft", &studio.swap_id, &studio.selection));
    let mut draft = ui
        .data(|data| data.get_temp::<Draft>(state_id))
        .or_else(|| Draft::selected(studio));
    let unavailable = busy(studio);
    let selected = studio.selected_key;
    let mut entries = Vec::new();
    for track in &studio.doc.motion.tracks {
        for (index, key) in track.keys.iter().enumerate() {
            entries.push((track.shape, track.prop, index, key.t));
        }
    }
    entries.sort_by(|a, b| {
        a.0.cmp(&b.0)
            .then_with(|| {
                Prop::all()
                    .iter()
                    .position(|prop| *prop == a.1)
                    .cmp(&Prop::all().iter().position(|prop| *prop == b.1))
            })
            .then_with(|| a.3.total_cmp(&b.3))
            .then_with(|| a.2.cmp(&b.2))
    });
    let key_label = |studio: &Studio, target, prop: Prop, time| {
        format!(
            "{} · {} · {time}s",
            target_name(studio, target),
            prop.name()
        )
    };
    let chosen = selected
        .and_then(|(target, prop, index)| {
            key_at(studio, target, prop, index).map(|key| key_label(studio, target, prop, key.t))
        })
        .unwrap_or_else(|| "Choose a keyframe".into());
    let label = ui.label("Keyframe");
    let selection = studio.selection.clone();
    let picker_salt = ("keyframe-picker", &selection);
    let focus_picker = Id::new((ID, "focus-picker", &studio.swap_id));
    let request_picker_focus = ui
        .data_mut(|data| data.remove_temp::<bool>(focus_picker))
        .unwrap_or(false);
    let mut key_chosen = false;
    let picker = ui
        .add_enabled_ui(unavailable.is_none(), |ui| {
            egui::ComboBox::from_id_salt(picker_salt)
                .selected_text(&chosen)
                .show_ui(ui, |ui| {
                    for (target, prop, index, time) in &entries {
                        let hit = motion::selection(&studio.doc, *target);
                        let enabled = hit.is_some_and(|(layer, id)| {
                            motion::target(&studio.doc, layer, id).is_some()
                        });
                        let id = ui.make_persistent_id((
                            "key",
                            &selection,
                            target,
                            prop,
                            time.to_bits(),
                            index,
                        ));
                        let response = super::fields::native_scope(ui, id, |ui| {
                            ui.add_enabled(
                                enabled,
                                egui::Button::selectable(
                                    selected == Some((*target, *prop, *index)),
                                    key_label(studio, *target, *prop, *time),
                                ),
                            )
                        })
                        .inner;
                        if unavailable.is_none() && enabled && response.clicked() {
                            let hit = hit.unwrap();
                            studio.selection = vec![hit];
                            studio.active_layer = Some(hit.0);
                            studio.selected_layer = None;
                            studio.selected_key = Some((*target, *prop, *index));
                            studio.playhead = *time;
                            draft = Draft::selected(studio);
                            ui.close();
                            key_chosen = true;
                            ui.data_mut(|data| data.insert_temp(focus_picker, true));
                        }
                    }
                })
        })
        .inner;
    if unavailable.is_none() && (request_picker_focus || key_chosen) {
        picker.response.request_focus();
    }
    name(
        ui,
        &picker.response,
        label.id,
        "Keyframe",
        egui::WidgetType::ComboBox,
        Some(&chosen),
    );
    let target = studio
        .selection
        .iter()
        .find_map(|&(layer, id)| motion::target(&studio.doc, layer, id).map(|target| target.id));
    let new_id = ui.make_persistent_id(("new-keyframe", &studio.selection, target));
    let new = super::fields::native_scope(ui, new_id, |ui| {
        action(
            ui,
            "New keyframe",
            unavailable.is_none() && target.is_some(),
        )
    })
    .inner;
    if unavailable.is_none()
        && new.clicked()
        && let Some(target) = target
    {
        studio.selected_key = None;
        draft = Some(Draft::new(studio, target, Prop::X, None));
    }
    if let Some(message) = unavailable {
        error(ui, None, message);
    }
    if let Some(mut current) = draft.take() {
        let active = current.current(studio);
        let target_error = editable(studio, current.target).err();
        let allowed = unavailable.is_none() && active && target_error.is_none();
        let field_id = ui.make_persistent_id((
            "fields",
            &current.document,
            &current.selection,
            current.target,
            current.prop,
            current
                .source
                .as_ref()
                .map(|(index, key)| (*index, key.t.to_bits())),
        ));
        super::fields::native_scope(ui, field_id, |ui| {
            ui.label(format!("Target: {}", target_name(studio, current.target)));
            let mut fields = Vec::new();
            ui.add_enabled_ui(allowed, |ui| {
                if current.source.is_none() {
                    let label = ui.label("Keyframe property");
                    let focus_property = Id::new((
                        ID,
                        "focus-property",
                        &current.document,
                        &current.selection,
                        current.target,
                    ));
                    let request_focus = ui
                        .data_mut(|data| data.remove_temp::<bool>(focus_property))
                        .unwrap_or(false);
                    let mut property_chosen = false;
                    let picker = egui::ComboBox::from_id_salt("property")
                        .selected_text(current.prop.name())
                        .show_ui(ui, |ui| {
                            for prop in Prop::all() {
                                let response = super::fields::native_scope(
                                    ui,
                                    ui.make_persistent_id(("property-option", prop)),
                                    |ui| {
                                        let response =
                                            ui.selectable_label(current.prop == prop, prop.name());
                                        focus(ui, &response);
                                        response
                                    },
                                )
                                .inner;
                                if response.clicked() {
                                    let next = Draft::new(studio, current.target, prop, None);
                                    current.prop = prop;
                                    current.value = next.value;
                                    ui.close();
                                    property_chosen = true;
                                    ui.data_mut(|data| data.insert_temp(focus_property, true));
                                }
                            }
                        });
                    if property_chosen || request_focus {
                        picker.response.request_focus();
                    }
                    name(
                        ui,
                        &picker.response,
                        label.id,
                        "Keyframe property",
                        egui::WidgetType::ComboBox,
                        Some(current.prop.name()),
                    );
                } else {
                    ui.label(format!("Property: {}", current.prop.name()));
                }
                fields.push((TIME, input(ui, TIME, &mut current.time)));
                let value_label = if current.prop == Prop::Fill {
                    COLOR
                } else {
                    VALUE
                };
                fields.push((value_label, input(ui, value_label, &mut current.value)));
                let label = ui.label(EASE);
                let mut curve_chosen = false;
                let picker = egui::ComboBox::from_id_salt("ease")
                    .selected_text(current.curve.name())
                    .show_ui(ui, |ui| {
                        for curve in Curve::ALL {
                            let response = super::fields::native_scope(
                                ui,
                                ui.make_persistent_id(("curve-option", curve)),
                                |ui| {
                                    let response =
                                        ui.selectable_label(current.curve == curve, curve.name());
                                    focus(ui, &response);
                                    response
                                },
                            )
                            .inner;
                            if response.clicked() {
                                current.curve = curve;
                                ui.close();
                                curve_chosen = true;
                            }
                        }
                    });
                if curve_chosen {
                    picker.response.request_focus();
                }
                name(
                    ui,
                    &picker.response,
                    label.id,
                    EASE,
                    egui::WidgetType::ComboBox,
                    Some(current.curve.name()),
                );
                fields.push((EASE, picker.response));
                if current.curve == Curve::Bezier {
                    for (index, label) in CONTROLS.into_iter().enumerate() {
                        fields.push((label, input(ui, label, &mut current.controls[index])));
                    }
                }
            });
            let validated = current.validate(studio.doc.motion.duration);
            if let Err(errors) = &validated {
                for (purpose, message) in errors {
                    error(
                        ui,
                        fields
                            .iter()
                            .find(|(name, _)| name == purpose)
                            .map(|(_, response)| response),
                        message,
                    );
                }
            }
            if !active {
                error(
                    ui,
                    None,
                    "The document, selection or key changed. Reset the draft before applying.",
                );
            }
            if let Some(message) = target_error {
                error(ui, None, message);
            }
            let apply_response = action(
                ui,
                if current.source.is_some() {
                    "Apply keyframe"
                } else {
                    "Add keyframe"
                },
                allowed && validated.is_ok(),
            );
            let removed = action(ui, "Remove keyframe", allowed && current.source.is_some());
            let reset = action(ui, "Reset keyframe draft", true);
            if allowed && apply_response.clicked() {
                if let Err(message) = apply(studio, &current) {
                    error(ui, Some(&apply_response), &message);
                } else if let Some(fresh) = Draft::selected(studio) {
                    current = fresh;
                    picker.response.request_focus();
                }
            } else if allowed && removed.clicked() {
                if let Err(message) = remove(studio, &current) {
                    error(ui, Some(&removed), &message);
                } else {
                    draft = Draft::selected(studio);
                    picker.response.request_focus();
                    return;
                }
            } else if reset.clicked() {
                current = Draft::selected(studio)
                    .unwrap_or_else(|| Draft::new(studio, current.target, current.prop, None));
                picker.response.request_focus();
            }
            draft = Some(current.clone());
        });
    }
    let next_state = Id::new((ID, "draft", &studio.swap_id, &studio.selection));
    ui.data_mut(|data| {
        if state_id != next_state {
            data.remove::<Draft>(state_id);
        }
        if let Some(draft) = draft {
            data.insert_temp(next_state, draft);
        } else {
            data.remove::<Draft>(next_state);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        document::{Document, Layer, Shape},
        geom::{Geom, Pt},
    };
    use egui::{Event, Modifiers, accesskit};

    /// Deliver a real key tap without a pointer or synthesized response.
    /// Args: native key and modifiers. Returns a press and release pair.
    fn key(key: egui::Key, modifiers: Modifiers) -> Vec<Event> {
        [true, false]
            .into_iter()
            .map(|pressed| Event::Key {
                key,
                physical_key: Some(key),
                pressed,
                repeat: false,
                modifiers,
            })
            .collect()
    }

    /// Render the existing shortcut and creation routes with the new native widgets.
    /// Args: context, editor and native events. Returns paint and AccessKit tree output.
    fn frame(ctx: &egui::Context, studio: &mut Studio, mut events: Vec<Event>) -> egui::FullOutput {
        let modifiers = events
            .iter()
            .find_map(|event| match event {
                Event::Key { modifiers, .. } => Some(*modifiers),
                _ => None,
            })
            .unwrap_or_default();
        events.insert(0, Event::ModifiersChanged(modifiers));
        let mut output = ctx.run_ui(
            egui::RawInput {
                events,
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(900., 900.),
                )),
                ..Default::default()
            },
            |ui| {
                studio.handle_shortcuts(ui.ctx());
                ui.interact(
                    ui.max_rect(),
                    Id::new("studio-canvas"),
                    egui::Sense::click(),
                );
                show(ui, studio);
                super::super::keyboard_authoring::show(ui.ctx(), studio);
            },
        );
        output.textures_delta.clear();
        output
    }

    /// Find an actual named widget in the current tree.
    /// Args: output and purpose. Returns its native identity and semantics.
    fn named<'a>(
        output: &'a egui::FullOutput,
        purpose: &str,
    ) -> (accesskit::NodeId, &'a accesskit::Node) {
        output
            .platform_output
            .accesskit_update
            .as_ref()
            .unwrap()
            .nodes
            .iter()
            .find_map(|(id, node)| (node.label() == Some(purpose)).then_some((*id, node)))
            .unwrap_or_else(|| panic!("missing native widget {purpose}"))
    }

    /// Check focus and paint for a real inspector field.
    /// Args: context, output and purpose. Returns no value; it does not change focus or tree contents.
    fn assert_focused(ctx: &egui::Context, output: &egui::FullOutput, purpose: &str) {
        let (id, node) = named(output, purpose);
        assert_eq!(
            ctx.memory(|memory| memory.focused())
                .map(|id| id.accesskit_id()),
            Some(id),
            "{purpose}"
        );
        assert_eq!(
            output
                .platform_output
                .accesskit_update
                .as_ref()
                .unwrap()
                .focus,
            id
        );
        let bounds = node.bounds().unwrap();
        let outline = egui::Rect::from_min_max(
            egui::pos2(bounds.x0 as f32, bounds.y0 as f32),
            egui::pos2(bounds.x1 as f32, bounds.y1 as f32),
        )
        .expand(2.);
        fn painted(shape: &egui::Shape, outline: egui::Rect) -> bool {
            match shape {
                egui::Shape::Rect(rect) => {
                    rect.rect == outline && rect.stroke.width == 2. && rect.stroke.color.a() > 0
                }
                egui::Shape::Vec(shapes) => shapes.iter().any(|shape| painted(shape, outline)),
                _ => false,
            }
        }
        assert!(
            output
                .shapes
                .iter()
                .any(|shape| painted(&shape.shape, outline)),
            "{purpose} needs visible focus"
        );
    }

    /// Reach an actual control using Tab and the native focus order.
    /// Args: context, editor and purpose. Returns its focused frame without requesting focus directly.
    fn tab_to(ctx: &egui::Context, studio: &mut Studio, purpose: &str) -> egui::FullOutput {
        let mut output = frame(ctx, studio, vec![]);
        for _ in 0..96 {
            let id = named(&output, purpose).0;
            if ctx
                .memory(|memory| memory.focused())
                .map(|id| id.accesskit_id())
                == Some(id)
            {
                return frame(ctx, studio, vec![]);
            }
            frame(ctx, studio, key(egui::Key::Tab, Modifiers::NONE));
            output = frame(ctx, studio, vec![]);
        }
        panic!("native Tab did not reach {purpose}");
    }

    /// Open the app-local menu through its native keyboard button.
    /// Args: context and editor. Returns after pressing Space on the focused opener.
    fn open(ctx: &egui::Context, studio: &mut Studio) {
        tab_to(ctx, studio, "Keyframes");
        frame(ctx, studio, key(egui::Key::Space, Modifiers::NONE));
        frame(ctx, studio, vec![]);
    }

    /// Choose a native menu option with Tab and Enter.
    /// Args: context, editor, picker purpose and option. Returns after native activation and popup closure.
    fn choose(ctx: &egui::Context, studio: &mut Studio, picker: &str, option: &str) {
        tab_to(ctx, studio, picker);
        frame(ctx, studio, key(egui::Key::Space, Modifiers::NONE));
        frame(ctx, studio, vec![]);
        tab_to(ctx, studio, option);
        frame(ctx, studio, key(egui::Key::Enter, Modifiers::NONE));
        frame(ctx, studio, vec![]);
    }

    /// Edit one real text field through Select All and native text input.
    /// Args: context, editor, field purpose and replacement. Returns the edited frame.
    fn replace(
        ctx: &egui::Context,
        studio: &mut Studio,
        purpose: &str,
        text: &str,
    ) -> egui::FullOutput {
        tab_to(ctx, studio, purpose);
        frame(
            ctx,
            studio,
            key(egui::Key::A, Modifiers::CTRL | Modifiers::COMMAND),
        );
        frame(ctx, studio, vec![Event::Text(text.into())])
    }

    /// Activate an actual button with the native Enter path.
    /// Args: context, editor and button purpose. Returns the settled frame.
    fn activate(ctx: &egui::Context, studio: &mut Studio, purpose: &str) -> egui::FullOutput {
        tab_to(ctx, studio, purpose);
        frame(ctx, studio, key(egui::Key::Enter, Modifiers::NONE));
        frame(ctx, studio, vec![])
    }

    /// Make a typed model fixture for rejection and sorting checks.
    /// Args: none. Returns an editor and target; this fixture is not the empty-selection acceptance route.
    fn fixture() -> (egui::Context, Studio, u64) {
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        let mut studio = Studio::new();
        studio.persona = Persona::Motion;
        studio.show_welcome = false;
        studio.doc = Document::new("Keyframe state checks", 128., 96., 72.);
        let mut layer = Layer::vector("Objects");
        let shape = Shape::new(
            Geom::Rect {
                origin: Pt::ZERO,
                size: Pt::splat(20.),
                radius: 0.,
            },
            Default::default(),
        );
        let target = shape.id;
        layer.kind.shapes_mut().unwrap().push(shape);
        studio.doc.layers = vec![layer];
        studio.selection = vec![(0, target)];
        studio.active_layer = Some(0);
        for (time, value) in [(0., 0.), (0.5, 10.), (1.5, 30.)] {
            studio
                .doc
                .motion
                .set_key(target, Prop::X, time, value, Ease::Linear);
        }
        studio.selected_key = Some((target, Prop::X, 1));
        (ctx, studio, target)
    }

    #[test]
    fn timeline_keyboard_empty_selection_creates_edits_and_reopens() {
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        let mut studio = Studio::new();
        studio.doc = Document::new("Keyboard motion", 128., 96., 72.);
        studio.persona = Persona::Motion;
        studio.show_welcome = false;
        assert!(studio.selection.is_empty());
        assert!(studio.doc.motion.is_empty());
        frame(&ctx, &mut studio, vec![]);
        frame(&ctx, &mut studio, key(egui::Key::F8, Modifiers::NONE));
        frame(
            &ctx,
            &mut studio,
            key(
                egui::Key::K,
                Modifiers::CTRL | Modifiers::COMMAND | Modifiers::SHIFT,
            ),
        );
        frame(&ctx, &mut studio, vec![]);
        assert!(super::super::keyboard_authoring::is_open(&ctx));
        activate(&ctx, &mut studio, "Create");
        assert!(!super::super::keyboard_authoring::is_open(&ctx));
        let hit = studio.primary().unwrap();
        let target = motion::target(&studio.doc, hit.0, hit.1).unwrap().id;
        let geometry = studio.doc.find_shape(hit.0, hit.1).unwrap().geom.clone();
        let rest = rest_color(&studio, target).unwrap();
        frame(&ctx, &mut studio, key(egui::Key::K, Modifiers::NONE));
        let mut end_key = key(egui::Key::End, Modifiers::NONE);
        end_key.extend(key(egui::Key::K, Modifiers::NONE));
        frame(&ctx, &mut studio, end_key);
        assert_eq!(
            studio
                .doc
                .motion
                .tracks
                .iter()
                .find(|track| track.shape == target && track.prop == Prop::X)
                .unwrap()
                .keys
                .len(),
            2
        );
        assert!(studio.selected_key.is_none());
        open(&ctx, &mut studio);
        let caption = format!("{} · X · 0s", target_name(&studio, target));
        choose(&ctx, &mut studio, "Keyframe", &caption);
        assert!(
            egui::Popup::is_any_open(&ctx),
            "The actual Keyframes parent survives a native child choice"
        );
        let output = frame(&ctx, &mut studio, vec![]);
        named(&output, "Apply keyframe");
        assert_eq!(studio.selected_key, Some((target, Prop::X, 0)));
        let before = studio.doc.motion.clone();
        replace(&ctx, &mut studio, TIME, "1.25");
        replace(&ctx, &mut studio, VALUE, "17.125");
        choose(&ctx, &mut studio, EASE, "Cubic Bezier");
        for (name, text) in CONTROLS.into_iter().zip(["0.125", "-0.5", "0.875", "1.5"]) {
            replace(&ctx, &mut studio, name, text);
        }
        assert_eq!(
            studio.doc.motion, before,
            "draft fields cannot edit the clip before Apply"
        );
        activate(&ctx, &mut studio, "Apply keyframe");
        let selected = studio.selected_key.unwrap();
        assert_eq!((selected.0, selected.1), (target, Prop::X));
        let edited = key_at(&studio, selected.0, selected.1, selected.2).unwrap();
        assert_eq!(edited.t, 1.25);
        assert_eq!(edited.value, 17.125);
        assert_eq!(
            edited.ease,
            Ease::CubicBezier {
                x1: 0.125,
                y1: -0.5,
                x2: 0.875,
                y2: 1.5
            }
        );
        activate(&ctx, &mut studio, "New keyframe");
        choose(&ctx, &mut studio, "Keyframe property", "Fill");
        replace(&ctx, &mut studio, TIME, "0.75");
        replace(&ctx, &mut studio, COLOR, "#33669980");
        choose(&ctx, &mut studio, EASE, "Hold");
        activate(&ctx, &mut studio, "Add keyframe");
        let fill = studio
            .doc
            .motion
            .tracks
            .iter()
            .find(|track| track.shape == target && track.prop == Prop::Fill)
            .unwrap();
        assert_eq!(fill.keys.len(), 2);
        assert_eq!(fill.keys[0].t, 0.);
        assert_eq!(fill.keys[0].color, Some(rest));
        assert_eq!(fill.keys[1].color, Rgba::parse_hex("#33669980"));
        assert_eq!(fill.keys[1].ease, Ease::Hold);
        assert_eq!(studio.selected_key, Some((target, Prop::Fill, 1)));
        assert_eq!(studio.doc.find_shape(hit.0, hit.1).unwrap().geom, geometry);
        let directory = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join(".artifacts")
            .join(format!(
                "timeline-keyboard-{}-{}",
                std::process::id(),
                crate::document::next_id()
            ));
        std::fs::create_dir_all(&directory).unwrap();
        let path = directory.join("motion.oma");
        crate::project::save_to(&studio.doc, &path).unwrap();
        let reopened = crate::project::load_from(&path).unwrap();
        assert_eq!(reopened.motion, studio.doc.motion);
        assert_eq!(
            crate::project::encode(&reopened).unwrap(),
            crate::project::encode(&studio.doc).unwrap()
        );
        activate(&ctx, &mut studio, "Remove keyframe");
        assert_eq!(studio.selected_key, Some((target, Prop::Fill, 0)));
        assert_eq!(
            studio
                .doc
                .motion
                .tracks
                .iter()
                .find(|track| track.shape == target && track.prop == Prop::Fill)
                .unwrap()
                .keys
                .len(),
            1
        );
        assert_eq!(studio.doc.find_shape(hit.0, hit.1).unwrap().geom, geometry);
        frame(&ctx, &mut studio, key(egui::Key::Escape, Modifiers::NONE));
        frame(&ctx, &mut studio, vec![]);
        frame(&ctx, &mut studio, key(egui::Key::F8, Modifiers::NONE));
        frame(
            &ctx,
            &mut studio,
            key(egui::Key::Z, Modifiers::CTRL | Modifiers::COMMAND),
        );
        assert_eq!(
            studio.doc.motion, reopened.motion,
            "Native Undo restores the removed key without touching artwork"
        );
        frame(
            &ctx,
            &mut studio,
            key(
                egui::Key::Z,
                Modifiers::CTRL | Modifiers::COMMAND | Modifiers::SHIFT,
            ),
        );
        assert_eq!(
            studio
                .doc
                .motion
                .tracks
                .iter()
                .find(|track| track.shape == target && track.prop == Prop::Fill)
                .unwrap()
                .keys
                .len(),
            1
        );
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn timeline_keyboard_retime_sorts_merges_and_keeps_discrete_undo() {
        let (_, mut studio, target) = fixture();
        let original = studio.doc.motion.clone();
        let mut draft = Draft::selected(&studio).unwrap();
        draft.time = "1.75".into();
        draft.value = "71.25".into();
        apply(&mut studio, &draft).unwrap();
        assert_eq!(studio.selected_key, Some((target, Prop::X, 2)));
        assert_eq!(key_at(&studio, target, Prop::X, 2).unwrap().t, 1.75);
        let moved = studio.doc.motion.clone();
        let mut draft = Draft::selected(&studio).unwrap();
        draft.time = "1.504".into();
        draft.value = "81".into();
        draft.curve = Curve::Hold;
        apply(&mut studio, &draft).unwrap();
        assert_eq!(studio.selected_key, Some((target, Prop::X, 1)));
        let track = studio
            .doc
            .motion
            .tracks
            .iter()
            .find(|track| track.shape == target && track.prop == Prop::X)
            .unwrap();
        assert_eq!(track.keys.len(), 2);
        assert_eq!(
            track.keys[1].t, 1.5,
            "native duplicate-time replacement keeps the survivor time"
        );
        assert_eq!(track.keys[1].value, 81.);
        assert_eq!(track.keys[1].ease, Ease::Hold);
        studio.undo();
        assert_eq!(studio.doc.motion, moved);
        studio.undo();
        assert_eq!(studio.doc.motion, original);
        studio.redo();
        assert_eq!(studio.doc.motion, moved);
    }

    #[test]
    fn timeline_keyboard_fill_and_opacity_preserve_rest_and_curve_data() {
        let (_, mut studio, target) = fixture();
        let rest = Rgba::new(210, 140, 70, 91);
        studio.doc.find_shape_mut(0, target).unwrap().style.fill =
            crate::document::Fill::Solid(rest);
        studio.doc.find_shape_mut(0, target).unwrap().opacity = 0.35;
        studio.selected_key = None;
        let mut draft = Draft::new(&studio, target, Prop::Fill, None);
        draft.time = "0.75".into();
        draft.value = "#11223344".into();
        draft.curve = Curve::Bezier;
        draft.controls = ["0.2", "-123.25", "0.8", "456.75"].map(str::to_owned);
        apply(&mut studio, &draft).unwrap();
        let curve = Ease::CubicBezier {
            x1: 0.2,
            y1: -123.25,
            x2: 0.8,
            y2: 456.75,
        };
        assert_eq!(
            key_at(&studio, target, Prop::Fill, 0).unwrap().color,
            Some(rest)
        );
        assert_eq!(
            key_at(&studio, target, Prop::Fill, 1).unwrap().color,
            Rgba::parse_hex("#11223344")
        );
        assert_eq!(key_at(&studio, target, Prop::Fill, 1).unwrap().ease, curve);
        let mut moved = Draft::selected(&studio).unwrap();
        moved.time = "1.25".into();
        moved.value = "#AABBCC55".into();
        apply(&mut studio, &moved).unwrap();
        assert_eq!(studio.selected_key, Some((target, Prop::Fill, 1)));
        assert_eq!(key_at(&studio, target, Prop::Fill, 1).unwrap().t, 1.25);
        assert_eq!(
            key_at(&studio, target, Prop::Fill, 1).unwrap().color,
            Rgba::parse_hex("#AABBCC55")
        );
        assert_eq!(key_at(&studio, target, Prop::Fill, 1).unwrap().ease, curve);
        studio.selected_key = None;
        let mut opacity = Draft::new(&studio, target, Prop::Opacity, None);
        opacity.time = "0.5".into();
        opacity.value = "0.8".into();
        apply(&mut studio, &opacity).unwrap();
        assert_eq!(
            key_at(&studio, target, Prop::Opacity, 0).unwrap().value,
            0.35
        );
        assert_eq!(
            key_at(&studio, target, Prop::Opacity, 1).unwrap().value,
            0.8
        );
        let before = studio.doc.motion.clone();
        let selected = Draft::selected(&studio).unwrap();
        remove(&mut studio, &selected).unwrap();
        assert_eq!(studio.selected_key, Some((target, Prop::Opacity, 0)));
        studio.undo();
        assert_eq!(studio.doc.motion, before);
    }

    #[test]
    fn timeline_keyboard_guards_stale_busy_locked_and_invalid_drafts() {
        let (_, mut studio, target) = fixture();
        let original = studio.doc.motion.clone();
        let draft = Draft::selected(&studio).unwrap();
        studio.playing = true;
        assert!(apply(&mut studio, &draft).is_err());
        assert!(remove(&mut studio, &draft).is_err());
        studio.playing = false;
        studio.doc.find_shape_mut(0, target).unwrap().locked = true;
        assert!(apply(&mut studio, &draft).is_err());
        assert!(remove(&mut studio, &draft).is_err());
        studio.doc.find_shape_mut(0, target).unwrap().locked = false;
        studio.doc.layers[0].locked = true;
        assert!(apply(&mut studio, &draft).is_err());
        studio.doc.layers[0].locked = false;
        studio.updates.freezing = true;
        assert!(apply(&mut studio, &draft).is_err());
        studio.updates.freezing = false;
        studio.selection.clear();
        assert!(apply(&mut studio, &draft).is_err());
        studio.selection = vec![(0, target)];
        studio.swap_id.push_str("-other-document");
        assert!(apply(&mut studio, &draft).is_err());
        studio.swap_id = draft.document.clone();
        studio.canvas_gen += 1;
        assert!(apply(&mut studio, &draft).is_err());
        studio.canvas_gen = draft.generation;
        studio.doc.motion.tracks[0].keys.swap(0, 1);
        assert!(apply(&mut studio, &draft).is_err());
        studio.doc.motion = original.clone();
        for text in ["NaN", "inf", "-0.1", "2.1"] {
            let mut invalid = draft.clone();
            invalid.time = text.into();
            assert!(apply(&mut studio, &invalid).is_err());
        }
        let mut invalid = draft.clone();
        invalid.value = "NaN".into();
        assert!(apply(&mut studio, &invalid).is_err());
        invalid.value = "22".into();
        invalid.curve = Curve::Bezier;
        invalid.controls[1] = "inf".into();
        assert!(apply(&mut studio, &invalid).is_err());
        invalid.controls = ["0.2", "-123.25", "0.8", "456.75"].map(str::to_owned);
        assert!(
            invalid.validate(studio.doc.motion.duration).is_ok(),
            "supported overshoot must remain editable"
        );
        assert_eq!(studio.doc.motion, original);
        assert_eq!(studio.selected_key, Some((target, Prop::X, 1)));
    }

    #[test]
    fn timeline_keyboard_native_fields_link_errors_and_keep_target_scopes() {
        let (ctx, mut studio, target) = fixture();
        open(&ctx, &mut studio);
        let initial = frame(&ctx, &mut studio, vec![]);
        let original_id = named(&initial, TIME).0;
        let original_apply = named(&initial, "Apply keyframe").0;
        let before = studio.doc.motion.clone();
        let output = replace(&ctx, &mut studio, TIME, "NaN");
        assert_focused(&ctx, &output, TIME);
        let (id, node) = named(&output, TIME);
        assert_eq!(id, original_id);
        assert_eq!(node.role(), accesskit::Role::TextInput);
        assert_eq!(node.invalid(), Some(accesskit::Invalid::True));
        assert!(!node.labelled_by().is_empty());
        let message = node.error_message().unwrap();
        assert!(node.described_by().contains(&message));
        let error = &output
            .platform_output
            .accesskit_update
            .as_ref()
            .unwrap()
            .nodes
            .iter()
            .find(|(id, _)| *id == message)
            .unwrap()
            .1;
        assert_eq!(error.live(), Some(accesskit::Live::Polite));
        let (apply_id, apply_node) = named(&output, "Apply keyframe");
        assert_eq!(
            apply_id, original_apply,
            "validation cannot change an action's native identity"
        );
        assert!(apply_node.is_disabled());
        frame(
            &ctx,
            &mut studio,
            vec![Event::AccessKitActionRequest(accesskit::ActionRequest {
                action: accesskit::Action::Click,
                target_tree: accesskit::TreeId::ROOT,
                target_node: apply_id,
                data: None,
            })],
        );
        assert_eq!(studio.doc.motion, before);
        replace(&ctx, &mut studio, TIME, "0.625");
        let output = frame(&ctx, &mut studio, vec![]);
        let (_, time) = named(&output, TIME);
        assert_eq!(time.invalid(), None);
        assert_eq!(time.error_message(), None);
        let (apply_id, apply_node) = named(&output, "Apply keyframe");
        assert!(!apply_node.is_disabled());
        frame(
            &ctx,
            &mut studio,
            vec![Event::AccessKitActionRequest(accesskit::ActionRequest {
                action: accesskit::Action::Click,
                target_tree: accesskit::TreeId::ROOT,
                target_node: apply_id,
                data: None,
            })],
        );
        let output = frame(&ctx, &mut studio, vec![]);
        assert_ne!(
            named(&output, TIME).0,
            original_id,
            "retiming rebinds fields to the exact new key"
        );
        assert_eq!(
            key_at(&studio, target, Prop::X, studio.selected_key.unwrap().2)
                .unwrap()
                .t,
            0.625
        );
        let same = frame(&ctx, &mut studio, vec![]);
        assert_eq!(named(&same, TIME).0, named(&output, TIME).0);
        let scoped = named(&output, TIME).0;
        studio.swap_id.push_str("-another-document");
        open(&ctx, &mut studio);
        let another = frame(&ctx, &mut studio, vec![]);
        assert_ne!(named(&another, TIME).0, scoped);
        assert_eq!(
            key_at(&studio, target, Prop::X, studio.selected_key.unwrap().2)
                .unwrap()
                .t,
            0.625
        );
    }
}
