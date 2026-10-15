//! Compact, direct manipulation gradient editor for the Appearance studio.
use crate::gradient::{Gradient, GradientKind, GradientStop};
use eframe::egui::{self, Color32, Sense, Stroke, Ui, pos2, vec2};

#[derive(Clone, Default)]
struct Stops {
    selected: usize,
    identities: Vec<u64>,
    next: u64,
    expected: Vec<GradientStop>,
}

impl Stops {
    /// Follow native stop identities through sorting.
    /// Args: gradient. Returns after preserving even identical stops as separate controls.
    fn sort(&mut self, gradient: &mut Gradient) {
        let selected = self.identities[self.selected];
        let mut order: Vec<_> = (0..gradient.stops.len()).collect();
        order.sort_by(|&a, &b| {
            gradient.stops[a]
                .offset
                .total_cmp(&gradient.stops[b].offset)
        });
        gradient.stops = order.iter().map(|&i| gradient.stops[i].clone()).collect();
        self.identities = order.iter().map(|&i| self.identities[i]).collect();
        self.selected = self
            .identities
            .iter()
            .position(|&id| id == selected)
            .unwrap();
    }

    /// Add a stop through the existing sampled color semantics.
    /// Args: gradient and offset. Returns after selecting the inserted stop.
    fn add(&mut self, gradient: &mut Gradient, offset: f32) {
        let color = gradient.sample(offset);
        gradient.stops.push(GradientStop { offset, color });
        self.identities.push(self.next);
        self.next += 1;
        self.selected = gradient.stops.len() - 1;
        self.sort(gradient);
    }
}

/// Decorate the existing precise radial number and observe accepted edits.
/// Args: UI, stable property, visible name, native purpose and model draft.
/// Returns the authoritative change flag, retaining the native .01 step and finite grammar.
fn radial_number(ui: &mut Ui, property: &str, label: &str, purpose: &str, value: &mut f32) -> bool {
    let range = if property == "inner-radius" {
        0.0..=0.9999
    } else {
        -f32::MAX..=f32::MAX
    };
    let (field, _) = crate::ui::keyboard_controls::motion_number(
        ui,
        ui.make_persistent_id(("gradient-radial", property)),
        (label, purpose),
        value,
        range,
        0.01,
        "",
        None,
    );
    crate::app::input_history::record(field.response, field.changed)
}

#[derive(Default)]
pub(super) struct Edits {
    pub design: bool,
    pub angle: Option<f32>,
}

/// Edit saved gradient properties and report explicit angle intent separately.
/// Args: UI, saved rest gradient and displayed rest-plus-live angle.
/// Returns design changes and an accepted absolute angle without baking a posed preview into the design.
pub(super) fn editor(ui: &mut Ui, gradient: &mut Gradient, shown_angle: f32) -> Edits {
    gradient.normalize();
    let mut changed = false;
    let mut angle_changed = None;
    if gradient.kind == GradientKind::Radial {
        let radial = ui.collapsing("Radial geometry", |ui| {
            for (property, label, purpose, value) in [
                (
                    "focus-x",
                    "Focus X",
                    "Radial gradient focus X",
                    &mut gradient.focal[0],
                ),
                (
                    "inner-radius",
                    "Inner radius",
                    "Radial gradient inner radius",
                    &mut gradient.inner_radius,
                ),
            ] {
                changed |= radial_number(ui, property, label, purpose, value);
            }
            changed |= radial_number(
                ui,
                "focus-y",
                "Focus Y",
                "Radial gradient focus Y",
                &mut gradient.focal[1],
            );
            if let Some(axis) = &mut gradient.radial_axis {
                ui.label("Ellipse axis");
                for (index, value) in axis.iter_mut().enumerate() {
                    let (property, label, purpose) = if index == 0 {
                        ("axis-x", "X", "Radial gradient ellipse axis X")
                    } else {
                        ("axis-y", "Y", "Radial gradient ellipse axis Y")
                    };
                    changed |= radial_number(ui, property, label, purpose, value);
                }
            }
        });
        crate::ui::welcome::focus_control(ui, &radial.header_response);
    }
    ui.horizontal(|ui| {
        let label = ui.label("Spread");
        for mode in [
            crate::gradient::Spread::Pad,
            crate::gradient::Spread::Repeat,
            crate::gradient::Spread::Reflect,
        ] {
            let id = ui.make_persistent_id(("gradient-spread", mode.svg()));
            let response = crate::ui::fields::native_scope(ui, id, |ui| {
                ui.selectable_value(&mut gradient.spread, mode, mode.svg())
            })
            .inner;
            crate::ui::welcome::name_control(
                ui,
                &response,
                label.id,
                &format!("Gradient spread {}", mode.svg()),
            );
            changed |= response.changed();
        }
    });
    let id = ui.make_persistent_id("gradient-stop");
    let mut stops = ui
        .data(|data| data.get_temp::<Stops>(id))
        .unwrap_or_default();
    if stops.expected != gradient.stops || stops.identities.len() != gradient.stops.len() {
        stops.selected = stops.selected.min(gradient.stops.len() - 1);
        stops.identities = (stops.next..stops.next + gradient.stops.len() as u64).collect();
        stops.next += gradient.stops.len() as u64;
    }
    let label = ui.label("Gradient stop");
    let mut chosen = false;
    let picker = egui::ComboBox::from_id_salt("gradient-stop-picker")
        .selected_text(format!(
            "Stop {} of {}",
            stops.selected + 1,
            gradient.stops.len()
        ))
        .show_ui(ui, |ui| {
            for (index, stop) in gradient.stops.iter().enumerate() {
                ui.push_id(stops.identities[index], |ui| {
                    let response = ui.selectable_label(
                        stops.selected == index,
                        format!(
                            "Stop {} · {:.2}% · {}",
                            index + 1,
                            stop.offset * 100.,
                            stop.color.hex()
                        ),
                    );
                    crate::ui::welcome::focus_control(ui, &response);
                    if response.clicked() {
                        stops.selected = index;
                        chosen = true;
                        ui.close();
                    }
                });
            }
        });
    if chosen {
        picker.response.request_focus();
    }
    crate::ui::welcome::name_control(ui, &picker.response, label.id, "Gradient stop");
    let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), 32.), Sense::CLICK);
    let ramp = egui::Rect::from_min_max(rect.min, pos2(rect.max.x, rect.max.y - 9.));
    for y in 0..3 {
        for x in 0..((ramp.width() / 8.).ceil() as usize) {
            let tile = egui::Rect::from_min_size(
                ramp.min + vec2(x as f32 * 8., y as f32 * 8.),
                vec2(8., 8.),
            )
            .intersect(ramp);
            ui.painter().rect_filled(
                tile,
                0.,
                if (x + y) % 2 == 0 {
                    Color32::from_gray(175)
                } else {
                    Color32::from_gray(100)
                },
            );
        }
    }
    let steps = ramp.width().ceil() as usize;
    for i in 0..steps {
        let x = ramp.left() + i as f32;
        ui.painter().rect_filled(
            egui::Rect::from_min_max(pos2(x, ramp.top()), pos2(x + 1.1, ramp.bottom())),
            0.,
            gradient.sample(i as f32 / steps.max(1) as f32).to_egui(),
        );
    }
    ui.painter().rect_stroke(
        ramp,
        2.,
        Stroke::new(1., crate::ui::theme::border()),
        egui::StrokeKind::Inside,
    );
    let mut dragging = ui
        .data(|d| d.get_temp::<bool>(id.with("dragging")))
        .unwrap_or(false);
    for (index, stop) in gradient.stops.iter().enumerate() {
        let center = pos2(ramp.left() + stop.offset * ramp.width(), ramp.bottom() + 4.);
        let hit = ui.interact(
            egui::Rect::from_center_size(center, vec2(14., 17.)),
            id.with(stops.identities[index]),
            Sense::click_and_drag(),
        );
        hit.widget_info(|| {
            egui::WidgetInfo::selected(
                egui::WidgetType::Button,
                hit.enabled(),
                index == stops.selected,
                format!("Select gradient stop {}", index + 1),
            )
        });
        crate::ui::welcome::focus_control(ui, &hit);
        ui.painter().circle_filled(center, 5., stop.color.to_egui());
        ui.painter().circle_stroke(
            center,
            5.,
            Stroke::new(
                if index == stops.selected { 2. } else { 1. },
                if index == stops.selected {
                    crate::ui::theme::accent()
                } else {
                    Color32::WHITE
                },
            ),
        );
        if hit.clicked() || hit.drag_started() {
            stops.selected = index;
        }
        if hit.drag_started() {
            dragging = true;
        }
    }
    // Track the selected stop through re-sorting, including crossing another stop.
    let drag = if dragging {
        ui.input(|i| i.pointer.interact_pos()).map(|p| {
            (
                stops.selected,
                ((p.x - ramp.left()) / ramp.width()).clamp(0., 1.),
            )
        })
    } else {
        None
    };
    dragging &= ui.input(|i| i.pointer.primary_down());
    ui.data_mut(|d| d.insert_temp(id.with("dragging"), dragging));
    if let Some((index, offset)) = drag {
        gradient.stops[index].offset = offset;
        stops.sort(gradient);
        changed = true;
    }
    if response.clicked()
        && let Some(pointer) = response.interact_pointer_pos()
        && ramp.contains(pointer)
    {
        let offset = ((pointer.x - ramp.left()) / ramp.width()).clamp(0., 1.);
        stops.add(gradient, offset);
        changed = true;
    }
    response.on_hover_text("Click the gradient to add a stop. Drag a stop to move it.");
    ui.push_id(
        ("gradient-stop-properties", stops.identities[stops.selected]),
        |ui| {
            let stop = &mut gradient.stops[stops.selected];
            changed |= crate::ui::color_picker::color_edit(ui, "stop-color", &mut stop.color);
            let mut position = stop.offset * 100.;
            let response = crate::ui::fields::number(
                ui,
                "position",
                "Gradient stop position (%)",
                &mut position,
                0.0..=100.0,
                "%",
                None,
                Default::default(),
            );
            crate::ui::welcome::focus_control(ui, &response.response);
            if crate::app::input_history::record(response.response, response.changed) {
                stop.offset = position / 100.;
                changed = true;
            }
        },
    );
    stops.sort(gradient);
    ui.horizontal(|ui| {
        let remove = ui.add_enabled(
            gradient.stops.len() > 2,
            egui::Button::new("Remove gradient stop"),
        );
        crate::ui::welcome::focus_control(ui, &remove);
        if remove.clicked() {
            gradient.stops.remove(stops.selected);
            stops.identities.remove(stops.selected);
            stops.selected = stops.selected.min(gradient.stops.len() - 1);
            changed = true;
        }
        let add = ui
            .button("Add gradient stop")
            .on_hover_text("Add stop halfway to the next stop");
        crate::ui::welcome::focus_control(ui, &add);
        if add.clicked() {
            let offset = (gradient.stops[stops.selected].offset
                + gradient
                    .stops
                    .get(stops.selected + 1)
                    .map(|s| s.offset)
                    .unwrap_or(1.))
                * 0.5;
            stops.add(gradient, offset);
            changed = true;
        }
    });
    if gradient.kind != GradientKind::Shape {
        let mut angle = shown_angle;
        let response = crate::ui::fields::number(
            ui,
            "angle",
            "Gradient angle (°)",
            &mut angle,
            -f32::MAX..=f32::MAX,
            "°",
            None,
            Default::default(),
        );
        crate::ui::welcome::focus_control(ui, &response.response);
        if crate::app::input_history::record(response.response, response.changed) {
            angle_changed = Some(angle);
        }
    }
    ui.horizontal(|ui| {
        let reverse = ui.button("Reverse");
        crate::ui::welcome::focus_control(ui, &reverse);
        if reverse.clicked() {
            for stop in &mut gradient.stops {
                stop.offset = 1. - stop.offset;
            }
            stops.sort(gradient);
            changed = true;
        }
        let even = ui.button("Even").on_hover_text("Distribute stops evenly");
        crate::ui::welcome::focus_control(ui, &even);
        if even.clicked() {
            let n = gradient.stops.len() - 1;
            for (i, stop) in gradient.stops.iter_mut().enumerate() {
                stop.offset = i as f32 / n as f32;
            }
            changed = true;
        }
    });
    if gradient.kind == GradientKind::Shape {
        ui.small("Follows the object's silhouette, including holes.");
    } else {
        ui.small("Drag the Gradient tool on canvas to place the gradient.");
    }
    stops.expected = gradient.stops.clone();
    ui.data_mut(|data| data.insert_temp(id, stops));
    Edits {
        design: changed,
        angle: angle_changed,
    }
}

#[cfg(test)]
mod keyboard_tests {
    use super::*;
    use crate::{color::Rgba, geom::Bounds, ui::welcome::keyboard_tests as keys};
    use egui::{Event, Key, Modifiers};

    fn frame(
        ctx: &egui::Context,
        gradient: &mut Gradient,
        owner: &str,
        enabled: bool,
        events: Vec<Event>,
    ) -> egui::FullOutput {
        keys::render(ctx, events, |ui| {
            ui.set_max_width(320.);
            ui.push_id(owner, |ui| {
                ui.add_enabled_ui(enabled, |ui| {
                    let bounds =
                        Bounds::from_min_size(crate::geom::Pt::ZERO, crate::geom::Pt::splat(64.));
                    let angle = gradient.angle(bounds);
                    let edits = editor(ui, gradient, angle);
                    if let Some(angle) = edits.angle {
                        gradient.set_angle(angle, bounds);
                    }
                });
            });
        })
    }

    fn action(ctx: &egui::Context, gradient: &mut Gradient, label: &str) {
        keys::tab_to(ctx, label, |events| {
            frame(ctx, gradient, "document-A", true, events)
        });
        frame(
            ctx,
            gradient,
            "document-A",
            true,
            keys::key(Key::Enter, Modifiers::NONE),
        );
        frame(ctx, gradient, "document-A", true, vec![]);
    }

    fn studio_frame(
        ctx: &egui::Context,
        studio: &mut crate::app::Studio,
        events: Vec<Event>,
    ) -> egui::FullOutput {
        keys::render(ctx, events, |ui| {
            studio.handle_shortcuts(ui.ctx());
            egui::Panel::right("native-gradient-properties")
                .exact_size(400.)
                .show(ui, |ui| {
                    super::super::inspector_content(ui, studio);
                });
            crate::ui::canvas::show(ui, studio);
            crate::ui::keyboard_authoring::show(ui.ctx(), studio);
        })
    }

    fn studio_action(
        ctx: &egui::Context,
        studio: &mut crate::app::Studio,
        label: &str,
    ) -> egui::FullOutput {
        keys::tab_to(ctx, label, |events| studio_frame(ctx, studio, events));
        studio_frame(ctx, studio, keys::key(Key::Enter, Modifiers::NONE));
        studio_frame(ctx, studio, vec![])
    }

    fn studio_text(
        ctx: &egui::Context,
        studio: &mut crate::app::Studio,
        label: &str,
        text: &str,
    ) -> egui::FullOutput {
        keys::tab_to(ctx, label, |events| studio_frame(ctx, studio, events));
        let output = keys::replace(text, |events| studio_frame(ctx, studio, events));
        keys::focused(ctx, &output, label);
        output
    }

    #[test]
    fn keyboard_gradient_native_radial_geometry_keeps_precision_errors_and_current_owner() {
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        crate::ui::theme::apply(&ctx);
        let mut gradient = Gradient::new(GradientKind::Radial, Rgba::BLACK, Rgba::WHITE);
        gradient.radial_axis = Some([0.125, 0.375]);
        gradient.inner_radius = 1.5;
        let before = gradient.clone();
        action(&ctx, &mut gradient, "Radial geometry");
        assert_eq!(
            gradient, before,
            "Opening the native geometry group cannot clamp loaded values"
        );
        keys::tab_to(&ctx, "Radial gradient inner radius", |events| {
            frame(&ctx, &mut gradient, "document-A", true, events)
        });
        keys::replace("0.125", |events| {
            frame(&ctx, &mut gradient, "document-A", true, events)
        });
        frame(
            &ctx,
            &mut gradient,
            "document-A",
            true,
            keys::key(Key::Enter, Modifiers::NONE),
        );
        let before = gradient.clone();
        let purpose = "Radial gradient ellipse axis X";
        keys::tab_to(&ctx, purpose, |events| {
            frame(&ctx, &mut gradient, "document-A", true, events)
        });
        let output = keys::replace("-", |events| {
            frame(&ctx, &mut gradient, "document-A", true, events)
        });
        keys::associated_error(&output, purpose);
        assert_eq!(gradient, before);
        let id = keys::named(&output, purpose).0;
        keys::replace("0.25", |events| {
            frame(&ctx, &mut gradient, "document-A", true, events)
        });
        frame(
            &ctx,
            &mut gradient,
            "document-A",
            true,
            keys::key(Key::ArrowUp, Modifiers::NONE),
        );
        let output = frame(&ctx, &mut gradient, "document-A", true, vec![]);
        assert!((gradient.radial_axis.unwrap()[0] - 0.26).abs() < 1e-6);
        assert_eq!(keys::named(&output, purpose).0, id);
        keys::focused(&ctx, &output, purpose);
        assert_ne!(
            keys::named(&output, purpose).1.invalid(),
            Some(egui::accesskit::Invalid::True)
        );
        let saved = gradient.clone();
        frame(
            &ctx,
            &mut gradient,
            "document-A",
            false,
            keys::action(
                id,
                egui::accesskit::Action::SetValue,
                Some(egui::accesskit::ActionData::NumericValue(999.)),
            ),
        );
        assert_eq!(gradient, saved);
        let output = frame(&ctx, &mut gradient, "document-B", true, vec![]);
        assert_ne!(keys::named(&output, purpose).0, id);
    }

    #[test]
    fn keyboard_gradient_empty_motion_authoring_separates_rest_fields_from_native_angle_keys() {
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        crate::ui::theme::apply(&ctx);
        let mut studio = crate::app::Studio::new();
        studio.doc = crate::document::Document::new("Native radial authoring", 300., 220., 72.);
        studio.persona = crate::tools::Persona::Motion;
        studio.show_welcome = false;
        assert!(studio.selection.is_empty());
        assert_eq!(
            studio
                .doc
                .layers
                .iter()
                .filter_map(|layer| layer.kind.shapes())
                .flatten()
                .count(),
            0
        );
        studio_frame(&ctx, &mut studio, vec![]);
        studio_frame(&ctx, &mut studio, keys::key(Key::F8, Modifiers::NONE));
        studio_frame(
            &ctx,
            &mut studio,
            keys::key(
                Key::K,
                Modifiers::CTRL | Modifiers::COMMAND | Modifiers::SHIFT,
            ),
        );
        studio_frame(&ctx, &mut studio, vec![]);
        assert!(crate::ui::keyboard_authoring::is_open(&ctx));
        studio_action(&ctx, &mut studio, "Create");
        let target = studio.primary().unwrap();
        studio_action(&ctx, &mut studio, "Fill type");
        studio_action(&ctx, &mut studio, "Radial");
        let rest = studio
            .doc
            .find_shape(target.0, target.1)
            .unwrap()
            .style
            .fill
            .gradient()
            .unwrap();
        let geometry = studio
            .doc
            .find_shape(target.0, target.1)
            .unwrap()
            .geom
            .clone();
        let authored = crate::project::encode(&studio.doc).unwrap();
        studio_text(&ctx, &mut studio, "Gradient angle (°)", "47.25");
        studio_frame(&ctx, &mut studio, keys::key(Key::Enter, Modifiers::NONE));
        let motion = studio.doc.motion.clone();
        assert_eq!(
            studio
                .doc
                .find_shape(target.0, target.1)
                .unwrap()
                .style
                .fill
                .gradient()
                .unwrap(),
            rest
        );
        assert_eq!(
            motion
                .tracks
                .iter()
                .find(|track| track.shape == target.1
                    && track.prop == crate::motion::Prop::GradientAngle)
                .unwrap()
                .keys[0]
                .value,
            47.25
        );
        studio_action(&ctx, &mut studio, "Radial geometry");
        let before = crate::project::encode(&studio.doc).unwrap();
        let output = studio_text(&ctx, &mut studio, "Radial gradient focus X", "-");
        keys::associated_error(&output, "Radial gradient focus X");
        assert_eq!(crate::project::encode(&studio.doc).unwrap(), before);
        for (label, value) in [
            ("Radial gradient focus X", "0.25"),
            ("Radial gradient focus Y", "-0.375"),
            ("Radial gradient inner radius", "0.125"),
        ] {
            studio_text(&ctx, &mut studio, label, value);
            studio_frame(&ctx, &mut studio, keys::key(Key::Enter, Modifiers::NONE));
        }
        let before_spread = studio
            .doc
            .find_shape(target.0, target.1)
            .unwrap()
            .style
            .fill
            .gradient()
            .unwrap();
        studio_action(&ctx, &mut studio, "Gradient spread reflect");
        let gradient = studio
            .doc
            .find_shape(target.0, target.1)
            .unwrap()
            .style
            .fill
            .gradient()
            .unwrap();
        assert_eq!(gradient.spread, crate::gradient::Spread::Reflect);
        assert_eq!(gradient.focal, [0.25, -0.375]);
        assert_eq!(gradient.inner_radius, 0.125);
        assert_eq!(
            (gradient.from, gradient.to, gradient.radial_axis),
            (rest.from, rest.to, rest.radial_axis)
        );
        assert_eq!(
            studio.doc.motion, motion,
            "Design properties cannot replace or add gradient-angle keys"
        );
        assert_eq!(
            studio.doc.find_shape(target.0, target.1).unwrap().geom,
            geometry
        );
        assert_ne!(crate::project::encode(&studio.doc).unwrap(), authored);
        studio_frame(&ctx, &mut studio, keys::key(Key::F8, Modifiers::NONE));
        studio_frame(
            &ctx,
            &mut studio,
            keys::key(Key::Z, Modifiers::CTRL | Modifiers::COMMAND),
        );
        assert_eq!(
            studio
                .doc
                .find_shape(target.0, target.1)
                .unwrap()
                .style
                .fill
                .gradient()
                .unwrap(),
            before_spread
        );
        studio_frame(
            &ctx,
            &mut studio,
            keys::key(
                Key::Z,
                Modifiers::CTRL | Modifiers::COMMAND | Modifiers::SHIFT,
            ),
        );
        assert_eq!(
            studio
                .doc
                .find_shape(target.0, target.1)
                .unwrap()
                .style
                .fill
                .gradient()
                .unwrap(),
            gradient
        );
        let directory = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join(".artifacts")
            .join(format!(
                "keyboard-radial-{}-{}",
                std::process::id(),
                crate::document::next_id()
            ));
        std::fs::create_dir_all(&directory).unwrap();
        let path = directory.join("radial.oma");
        crate::project::save_to(&studio.doc, &path).unwrap();
        assert_eq!(
            crate::project::encode(&crate::project::load_from(&path).unwrap()).unwrap(),
            crate::project::encode(&studio.doc).unwrap()
        );
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn keyboard_gradient_add_select_sort_and_duplicate_stops_keep_native_identity() {
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        crate::ui::theme::apply(&ctx);
        let mut gradient = Gradient::new(GradientKind::Linear, Rgba::BLACK, Rgba::WHITE);
        action(&ctx, &mut gradient, "Select gradient stop 2");
        action(&ctx, &mut gradient, "Add gradient stop");
        action(&ctx, &mut gradient, "Add gradient stop");
        assert_eq!(gradient.stops.len(), 4);
        assert_eq!(
            gradient
                .stops
                .iter()
                .filter(|stop| stop.offset == 1. && stop.color == Rgba::WHITE)
                .count(),
            3
        );
        let output = keys::tab_to(&ctx, "Gradient stop position (%)", |events| {
            frame(&ctx, &mut gradient, "document-A", true, events)
        });
        keys::focused(&ctx, &output, "Gradient stop position (%)");
        let position = keys::named(&output, "Gradient stop position (%)").0;
        action(&ctx, &mut gradient, "Reverse");
        let output = keys::tab_to(&ctx, "Gradient stop position (%)", |events| {
            frame(&ctx, &mut gradient, "document-A", true, events)
        });
        assert_eq!(
            keys::named(&output, "Gradient stop position (%)").0,
            position
        );
        keys::replace("75", |events| {
            frame(&ctx, &mut gradient, "document-A", true, events)
        });
        let output = frame(&ctx, &mut gradient, "document-A", true, vec![]);
        keys::focused(&ctx, &output, "Gradient stop position (%)");
        assert_eq!(
            keys::named(&output, "Gradient stop position (%)").0,
            position
        );
        assert_eq!(
            gradient
                .stops
                .iter()
                .filter(|stop| stop.color == Rgba::WHITE && stop.offset == 0.)
                .count(),
            2
        );
        assert_eq!(
            gradient
                .stops
                .iter()
                .filter(|stop| stop.color == Rgba::WHITE && stop.offset == 0.75)
                .count(),
            1
        );
        let before = gradient.clone();
        frame(
            &ctx,
            &mut gradient,
            "document-A",
            false,
            vec![Event::Text("50".into())],
        );
        assert_eq!(gradient, before);
        let output = frame(&ctx, &mut gradient, "document-B", true, vec![]);
        assert_ne!(
            keys::named(&output, "Gradient stop position (%)").0,
            position
        );
    }
}
