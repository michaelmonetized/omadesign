//! Native layout workspace. Controls edit persisted document properties.
use super::theme::{accent, bg_panel, fg_weak};
use crate::app::{LayoutStackField, Studio};
use crate::document::Cmd;
use crate::geom::Geom;
use crate::layout::{
    AutoStack, Constraint, GridTrack, GridTrackSize, Sizing, StackAlign, StackAxis, StackFlow,
    StackJustify,
};
use crate::tools::{Persona, Tool};
use eframe::egui::{self, RichText, Ui};

#[path = "layout_keyboard.rs"]
pub(super) mod keyboard;

#[cfg(test)]
#[path = "layout_field_tests.rs"]
mod field_tests;

fn label(ui: &mut Ui, text: &str) {
    ui.add_space(8.0);
    ui.label(RichText::new(text).strong().size(11.0));
}
fn control(
    ui: &mut Ui,
    property: impl std::hash::Hash + std::fmt::Debug,
    name: &str,
    widget: impl FnOnce(&mut Ui) -> egui::Response,
) -> egui::Response {
    let owner = ui.make_persistent_id(property);
    super::svg_paint::control(ui, owner, name, widget)
}

fn sizing(ui: &mut Ui, id: &str, value: &mut Sizing) -> bool {
    let old = *value;
    super::keyboard_controls::choice(
        ui,
        id,
        if id == "layout-width" {
            "Width sizing"
        } else {
            "Height sizing"
        },
        value,
        &[Sizing::Fixed, Sizing::Hug, Sizing::Fill].map(|v| (v, format!("{v:?}"))),
    );
    old != *value
}

fn layout_number<N: egui::emath::Numeric>(
    ui: &mut Ui,
    selection: (usize, u64),
    property: impl std::hash::Hash + std::fmt::Debug,
    name: &str,
    value: &mut N,
    range: std::ops::RangeInclusive<N>,
    unit: &str,
) -> bool {
    let field = super::fields::number(
        ui,
        (selection, property),
        name,
        value,
        range,
        unit,
        None,
        Default::default(),
    );
    crate::app::input_history::record(field.response, field.changed)
}

fn stack_equal(left: &AutoStack, right: &AutoStack, field: LayoutStackField) -> bool {
    use LayoutStackField::*;
    match field {
        Enabled => true,
        Direction => left.direction == right.direction,
        Flow => left.flow == right.flow,
        Align => left.align == right.align,
        Justify => left.justify == right.justify,
        Gap => left.gap == right.gap,
        CrossGap => left.cross_gap == right.cross_gap,
        Columns => left.columns == right.columns,
        Padding(edge) => left.padding.get(edge) == right.padding.get(edge),
        ColumnTracks => left.column_tracks == right.column_tracks,
        RowTracks => left.row_tracks == right.row_tracks,
        ColumnTrackSize(i) => {
            left.column_tracks.get(i).map(|t| t.size) == right.column_tracks.get(i).map(|t| t.size)
        }
        ColumnTrackMin(i) => {
            left.column_tracks.get(i).map(|t| t.min) == right.column_tracks.get(i).map(|t| t.min)
        }
        ColumnTrackMax(i) => {
            left.column_tracks.get(i).map(|t| t.max) == right.column_tracks.get(i).map(|t| t.max)
        }
        RowTrackSize(i) => {
            left.row_tracks.get(i).map(|t| t.size) == right.row_tracks.get(i).map(|t| t.size)
        }
        RowTrackMin(i) => {
            left.row_tracks.get(i).map(|t| t.min) == right.row_tracks.get(i).map(|t| t.min)
        }
        RowTrackMax(i) => {
            left.row_tracks.get(i).map(|t| t.max) == right.row_tracks.get(i).map(|t| t.max)
        }
    }
}

fn stack_mixed(stacks: &[Option<AutoStack>], field: LayoutStackField) -> bool {
    let first = stacks.first().and_then(Option::as_ref);
    stacks.iter().any(|stack| match (first, stack.as_ref()) {
        (Some(left), Some(right)) => !stack_equal(left, right, field),
        (None, None) => false,
        _ => true,
    })
}

fn stack_intent(
    fields: &mut Vec<LayoutStackField>,
    field: LayoutStackField,
    accepted: bool,
) -> bool {
    if accepted && !fields.contains(&field) {
        fields.push(field);
    }
    accepted
}

fn stack_choice<T: Clone + PartialEq>(
    ui: &mut Ui,
    id: &str,
    name: &str,
    value: &mut T,
    options: &[(T, String)],
    mixed: bool,
) -> egui::Response {
    let owner = ui.make_persistent_id(id);
    super::fields::native_scope(ui, owner, |ui| {
        let label = ui.label(name);
        let mut accepted = false;
        let mut response = egui::ComboBox::from_id_salt("value")
            .selected_text(if mixed {
                "Mixed"
            } else {
                options
                    .iter()
                    .find(|(key, _)| key == value)
                    .map_or("Choose…", |(_, name)| name)
            })
            .show_ui(ui, |ui| {
                for (key, name) in options {
                    let choice = ui.selectable_value(value, key.clone(), name);
                    if choice.clicked() && choice.enabled() {
                        accepted = true;
                        ui.close();
                    }
                }
            })
            .response
            .labelled_by(label.id);
        if accepted {
            response.request_focus();
            response.mark_changed();
        }
        super::keyboard_controls::selection_keys(ui, &mut response, name, value, options);
        if mixed {
            ui.ctx()
                .accesskit_node_builder(response.id, |node| node.set_state_description("Mixed"));
        }
        response
    })
    .inner
}

fn stack_number<N: egui::emath::Numeric>(
    ui: &mut Ui,
    selection: (usize, u64),
    property: impl std::hash::Hash + std::fmt::Debug,
    name: &str,
    value: &mut N,
    range: std::ops::RangeInclusive<N>,
    unit: &str,
    stacks: &[Option<AutoStack>],
    fields: &mut Vec<LayoutStackField>,
    intent: LayoutStackField,
) -> bool {
    let field = super::fields::number(
        ui,
        (selection, property),
        name,
        value,
        range,
        unit,
        None,
        super::fields::FieldState {
            mixed: stack_mixed(stacks, intent),
            ..Default::default()
        },
    );
    let accepted = crate::app::input_history::record(field.response, field.changed);
    stack_intent(fields, intent, accepted)
}

fn native_number<N: egui::emath::Numeric + std::fmt::Display>(
    ui: &mut Ui,
    property: impl std::hash::Hash + std::fmt::Debug,
    name: &str,
    value: &mut N,
    range: std::ops::RangeInclusive<N>,
    speed: Option<f64>,
    prefix: &str,
    suffix: &str,
) -> bool {
    let field = keyboard::number(ui, property, name, value, range, speed, prefix, suffix);
    crate::app::input_history::record(field.response, field.changed)
}

fn grid_tracks(
    ui: &mut Ui,
    selection: (usize, u64),
    axis: &str,
    tracks: &mut Vec<GridTrack>,
    revision: u64,
    stacks: &[Option<AutoStack>],
    fields: &mut Vec<LayoutStackField>,
) -> bool {
    use LayoutStackField::*;
    let axis_field = if axis == "Column" {
        ColumnTracks
    } else {
        RowTracks
    };
    let size_field = |index| {
        if axis == "Column" {
            ColumnTrackSize(index)
        } else {
            RowTrackSize(index)
        }
    };
    let min_field = |index| {
        if axis == "Column" {
            ColumnTrackMin(index)
        } else {
            RowTrackMin(index)
        }
    };
    let max_field = |index| {
        if axis == "Column" {
            ColumnTrackMax(index)
        } else {
            RowTrackMax(index)
        }
    };
    let structural_allowed = !stack_mixed(stacks, axis_field);
    let mut changed = false;
    let mut rows = keyboard::Rows::begin(ui, axis, tracks, revision);
    let mut structural = false;
    egui::CollapsingHeader::new(format!("{axis} tracks"))
        .id_salt((selection, axis, "tracks"))
        .show(ui, |ui| {
            let mut remove = None;
            for (index, track) in tracks.iter_mut().enumerate() {
                let available = stacks.iter().all(|stack| {
                    stack.as_ref().is_some_and(|stack| {
                        if axis == "Column" {
                            stack.column_tracks.len() > index
                        } else {
                            stack.row_tracks.len() > index
                        }
                    })
                });
                super::fields::native_scope(ui, rows.id(index), |ui| {
                    if !available {
                        ui.disable();
                    }
                    ui.horizontal(|ui| {
                        let label = ui.label(format!("{axis} {index}"));
                        let mut kind = match track.size {
                            GridTrackSize::Fixed(_) => 0,
                            GridTrackSize::Hug => 1,
                            GridTrackSize::Fraction(_) => 2,
                        };
                        let original_kind = kind;
                        let response = stack_choice(
                            ui,
                            "sizing",
                            &format!("{axis} {index} sizing"),
                            &mut kind,
                            &[
                                (0, "Fixed".into()),
                                (1, "Hug".into()),
                                (2, "Fraction".into()),
                            ],
                            stack_mixed(stacks, size_field(index)),
                        )
                        .labelled_by(label.id);
                        let accepted = response.changed();
                        rows.watch(response);
                        if accepted {
                            if kind != original_kind {
                                track.size = match kind {
                                    0 => GridTrackSize::Fixed(100.0),
                                    1 => GridTrackSize::Hug,
                                    _ => GridTrackSize::Fraction(1.0),
                                };
                            }
                            changed |= stack_intent(fields, size_field(index), true);
                        }
                        let remove_id = ui.make_persistent_id("remove");
                        if rows
                            .watch(super::svg_paint::control(
                                ui,
                                remove_id,
                                &format!("Remove {axis} track {index}"),
                                |ui| {
                                    ui.add_enabled(
                                        structural_allowed,
                                        egui::Button::new("Remove").small(),
                                    )
                                },
                            ))
                            .clicked()
                        {
                            remove = Some(index);
                        }
                    });
                    match &mut track.size {
                        GridTrackSize::Fixed(value) => {
                            changed |= stack_intent(
                                fields,
                                size_field(index),
                                rows.shared_number(
                                    ui,
                                    (selection, (axis, index, "size")),
                                    &format!("{axis} {index} size"),
                                    value,
                                    0.0..=1_000_000.0,
                                    " px",
                                    stack_mixed(stacks, size_field(index)),
                                ),
                            )
                        }
                        GridTrackSize::Fraction(value) => {
                            changed |= stack_intent(
                                fields,
                                size_field(index),
                                rows.shared_number(
                                    ui,
                                    (selection, (axis, index, "fraction")),
                                    &format!("{axis} {index} share"),
                                    value,
                                    0.001..=1_000_000.0,
                                    " fr",
                                    stack_mixed(stacks, size_field(index)),
                                ),
                            )
                        }
                        GridTrackSize::Hug => {}
                    }
                    for (name, value, intent) in [
                        ("Minimum", &mut track.min, min_field(index)),
                        ("Maximum", &mut track.max, max_field(index)),
                    ] {
                        ui.push_id(name, |ui| {
                            let mut enabled = value.is_some();
                            let enabled_id = ui.make_persistent_id((name, "enabled"));
                            if rows
                                .watch(super::svg_paint::control(
                                    ui,
                                    enabled_id,
                                    &format!("{name} {axis} {index}"),
                                    |ui| {
                                        ui.add(
                                            egui::Checkbox::new(
                                                &mut enabled,
                                                format!("{name} {axis} {index}"),
                                            )
                                            .indeterminate(stack_mixed(stacks, intent)),
                                        )
                                    },
                                ))
                                .changed()
                            {
                                *value = enabled.then_some(if name == "Minimum" {
                                    0.0
                                } else {
                                    1_000_000.0
                                });
                                changed |= stack_intent(fields, intent, true);
                            }
                            if let Some(value) = value {
                                changed |= stack_intent(
                                    fields,
                                    intent,
                                    rows.shared_number(
                                        ui,
                                        (selection, (axis, index, name)),
                                        &format!("{axis} {index} {name}"),
                                        value,
                                        0.0..=1_000_000.0,
                                        " px",
                                        stack_mixed(stacks, intent),
                                    ),
                                );
                            }
                        });
                    }
                    ui.separator();
                });
            }
            if let Some(index) = remove {
                tracks.remove(index);
                structural = true;
                changed = true;
            }
            if tracks.len() < 1024
                && control(
                    ui,
                    ("add-track", axis),
                    &format!("Add {axis} track"),
                    |ui| {
                        ui.add_enabled(
                            structural_allowed,
                            egui::Button::new(format!("Add {axis} track")),
                        )
                    },
                )
                .clicked()
            {
                structural = true;
                tracks.push(if axis == "Row" {
                    GridTrack {
                        size: GridTrackSize::Hug,
                        ..Default::default()
                    }
                } else {
                    GridTrack::default()
                });
                changed = true;
            }
        });
    if structural {
        stack_intent(fields, axis_field, true);
    }
    rows.finish(ui, tracks, structural);
    changed
}

fn breakpoints(
    ui: &mut Ui,
    shape: &crate::document::Shape,
    props: &mut crate::layout::FrameLayout,
    revision: u64,
) -> bool {
    let mut changed = false;
    let mut structural = false;
    let mut rows = keyboard::Rows::begin(ui, "breakpoints", &props.breakpoints, revision);
    egui::CollapsingHeader::new("Breakpoint overrides").show(ui, |ui| {
        ui.label(
            RichText::new("Rules apply at or below this viewport width.")
                .small()
                .color(fg_weak()),
        );
        let mut remove = None;
        for (index, point) in props.breakpoints.iter_mut().enumerate() {
            super::fields::native_scope(ui, rows.id(index), |ui| {
                ui.horizontal(|ui| {
                    changed |= rows.number(
                        ui,
                        "width",
                        &format!("Breakpoint {index} width (px)"),
                        &mut point.max_width,
                        240.0..=3840.0,
                        None,
                        "≤ ",
                        " px",
                    );
                    let owner = ui.make_persistent_id("remove");
                    if rows
                        .watch(super::svg_paint::control(
                            ui,
                            owner,
                            &format!("Remove breakpoint {index}"),
                            |ui| ui.small_button("×"),
                        ))
                        .clicked()
                    {
                        remove = Some(index);
                    }
                });
                if props.frame {
                    let old = point.flow;
                    let response = super::keyboard_controls::choice(
                        ui,
                        "breakpoint-flow",
                        &format!("Breakpoint {index} flow"),
                        &mut point.flow,
                        &[
                            (None, "Inherit layout".into()),
                            (Some(StackFlow::Stack), "Stack".into()),
                            (Some(StackFlow::Wrap), "Wrap".into()),
                            (Some(StackFlow::Grid), "Grid".into()),
                        ],
                    );
                    rows.watch(response);
                    changed |= old != point.flow;
                    ui.horizontal(|ui| {
                        for (name, axis) in [
                            ("Inherit", None),
                            ("Row", Some(StackAxis::Horizontal)),
                            ("Column", Some(StackAxis::Vertical)),
                        ] {
                            let owner = ui.make_persistent_id(("direction", name));
                            let response = super::svg_paint::control(
                                ui,
                                owner,
                                &format!("Breakpoint {index} direction {name}"),
                                |ui| ui.selectable_value(&mut point.direction, axis, name),
                            );
                            changed |= response.changed();
                            rows.watch(response);
                        }
                    });
                    ui.horizontal(|ui| {
                        let mut set = point.gap.is_some();
                        let owner = ui.make_persistent_id("gap-enabled");
                        let response = super::svg_paint::control(
                            ui,
                            owner,
                            &format!("Set breakpoint {index} gap"),
                            |ui| ui.checkbox(&mut set, "Gap"),
                        );
                        if response.changed() {
                            point.gap = set.then_some(16.0);
                            changed = true;
                        }
                        rows.watch(response);
                        if let Some(value) = &mut point.gap {
                            changed |= rows.number(
                                ui,
                                "gap",
                                &format!("Breakpoint {index} gap (px)"),
                                value,
                                0.0..=1000.0,
                                None,
                                "",
                                "",
                            );
                        }
                    });
                    ui.horizontal(|ui| {
                        let mut set = point.padding.is_some();
                        let owner = ui.make_persistent_id("padding-enabled");
                        let response = super::svg_paint::control(
                            ui,
                            owner,
                            &format!("Set breakpoint {index} padding"),
                            |ui| ui.checkbox(&mut set, "Padding"),
                        );
                        if response.changed() {
                            point.padding = set.then_some([24.0; 4]);
                            changed = true;
                        }
                        rows.watch(response);
                        if let Some(padding) = &mut point.padding {
                            let mut value = padding[0];
                            if rows.number(
                                ui,
                                "padding",
                                &format!("Breakpoint {index} padding (px)"),
                                &mut value,
                                0.0..=1000.0,
                                None,
                                "",
                                "",
                            ) {
                                *padding = [value; 4];
                                changed = true;
                            }
                        }
                    });
                    if point.flow == Some(StackFlow::Grid) {
                        let columns = point.columns.get_or_insert(2);
                        changed |= rows.number(
                            ui,
                            "columns",
                            &format!("Breakpoint {index} columns"),
                            columns,
                            1..=24,
                            None,
                            "Columns ",
                            "",
                        );
                    }
                }
                if let Geom::Text(run) = &shape.geom {
                    let size = point.text_size.get_or_insert(run.px);
                    changed |= rows.number(
                        ui,
                        "text-size",
                        &format!("Breakpoint {index} type size (px)"),
                        size,
                        1.0..=500.0,
                        None,
                        "",
                        " px type",
                    );
                }
                ui.separator();
            });
        }
        if let Some(index) = remove {
            props.breakpoints.remove(index);
            structural = true;
            changed = true;
        }
        ui.horizontal(|ui| {
            for (name, width) in [("+ Phone", 600.0), ("+ Tablet", 1024.0)] {
                if super::keyboard_controls::button(ui, name, name).clicked() {
                    props.breakpoints.push(crate::layout::LayoutBreakpoint {
                        max_width: width,
                        ..Default::default()
                    });
                    if let Geom::Text(run) = &shape.geom {
                        props.text_size.get_or_insert(run.px);
                    }
                    structural = true;
                    changed = true;
                }
            }
        });
    });
    rows.finish(ui, &props.breakpoints, structural);
    changed
}

pub fn inspector(ui: &mut Ui, studio: &mut Studio) {
    ui.horizontal(|ui| {
        if ui
            .button("Auto layout")
            .on_hover_text("Shift+A · wrap and arrange selected objects")
            .clicked()
        {
            studio.auto_layout_selection();
        }
        if ui
            .button("Present")
            .on_hover_text("Open interactive responsive preview")
            .clicked()
        {
            super::layout_preview::start(ui.ctx(), studio);
        }
    });
    let Some((layer, id)) = studio.primary() else {
        ui.add_space(12.0);
        ui.label(RichText::new("Start with a frame").strong());
        ui.label(
            RichText::new("F to draw · T for text · Shift+A to arrange")
                .small()
                .color(fg_weak()),
        );
        if ui.button("Open Fieldwork starter").clicked() {
            studio.use_layout_template("layout-fieldwork", 1280.0, 900.0, 96.0);
        }
        for (name, w, h) in [
            ("Phone", 390.0, 844.0),
            ("Tablet", 768.0, 1024.0),
            ("Desktop", 1440.0, 900.0),
        ] {
            if ui.button(format!("{name}   {w:.0} × {h:.0}")).clicked() {
                studio.insert_layout_frame(name, w, h);
            }
        }
        return;
    };
    let Some(shape) = studio.doc.find_shape(layer, id).cloned() else {
        return;
    };
    let blocked = keyboard::blocked(studio);
    studio.with_field_history(ui, "layout-properties", |ui, studio| {
        if blocked {
            ui.disable();
        }
        let mut props = shape.layout.clone();
        let Some(layout_target) = studio.layout_inspector_target() else {
            return;
        };
        let mut changed = false;
        let mut fields = Vec::new();
        let stacks: Vec<_> = studio
            .selection
            .iter()
            .filter_map(|&(layer, id)| {
                studio
                    .doc
                    .find_shape(layer, id)
                    .map(|shape| shape.layout.stack.clone())
            })
            .collect();
        egui::CollapsingHeader::new("Image fill")
            .default_open(shape.layout.image.is_some())
            .show(ui, |ui| {
                if super::jobs::is_running::<LoadedImage>(ui.ctx(), "layout-image-load") {
                    ui.spinner();
                    ui.label("Loading image…");
                } else if ui
                    .button(if props.image.is_some() {
                        "Replace image…"
                    } else {
                        "Choose image…"
                    })
                    .clicked()
                {
                    let document = studio.swap_id.clone();
                    studio.request_file_dialog(
                        || {
                            rfd::FileDialog::new()
                                .add_filter(
                                    "Images",
                                    &["png", "jpg", "jpeg", "webp", "gif", "tiff"],
                                )
                                .pick_file()
                        },
                        move |ctx, _, path| {
                            super::jobs::start(ctx, "layout-image-load", move || {
                                let image = crate::layout_images::load(&path)?;
                                Ok(LoadedImage {
                                    document,
                                    layer,
                                    id,
                                    image,
                                })
                            });
                        },
                    );
                }
                if let Some(image) = &mut props.image {
                    ui.horizontal(|ui| {
                        for fit in [
                            crate::layout_images::ImageFit::Cover,
                            crate::layout_images::ImageFit::Contain,
                            crate::layout_images::ImageFit::Stretch,
                        ] {
                            changed |= control(
                                ui,
                                ("image-fit", fit.name()),
                                &format!("Image fit {}", fit.name()),
                                |ui| ui.selectable_value(&mut image.fit, fit, fit.name()),
                            )
                            .changed();
                        }
                    });
                    ui.horizontal(|ui| {
                        ui.label("Focus");
                        changed |= native_number(
                            ui,
                            "image-focal-x",
                            "Image focus X",
                            &mut image.focal.x,
                            0.0..=1.0,
                            Some(0.01),
                            "",
                            "",
                        );
                        changed |= native_number(
                            ui,
                            "image-focal-y",
                            "Image focus Y",
                            &mut image.focal.y,
                            0.0..=1.0,
                            Some(0.01),
                            "",
                            "",
                        );
                    });
                    if control(ui, "remove-image", "Remove image", |ui| {
                        ui.small_button("Remove image")
                    })
                    .clicked()
                    {
                        props.image = None;
                        changed = true;
                    }
                }
            });
        label(ui, "Sizing");
        ui.horizontal(|ui| {
            changed |= sizing(ui, "layout-width", &mut props.width);
            changed |= sizing(ui, "layout-height", &mut props.height);
        });
        egui::CollapsingHeader::new("Min / max size").show(ui, |ui| {
            for (name, val) in [
                ("Min W", &mut props.min_width),
                ("Max W", &mut props.max_width),
                ("Min H", &mut props.min_height),
                ("Max H", &mut props.max_height),
            ] {
                ui.horizontal(|ui| {
                    let mut enabled = val.is_some();
                    if control(ui, ("limit-enabled", name), &format!("Set {name}"), |ui| {
                        ui.checkbox(&mut enabled, name)
                    })
                    .changed()
                    {
                        *val = enabled.then_some(1.0);
                        changed = true;
                    }
                    if let Some(v) = val {
                        changed |= native_number(
                            ui,
                            ("size-limit", name),
                            name,
                            v,
                            1.0..=10000.0,
                            None,
                            "",
                            "",
                        );
                    }
                });
            }
        });
        if props.frame {
            changed |= control(ui, "clip", "Clip content", |ui| {
                ui.checkbox(&mut props.clip, "Clip content")
            })
            .changed();
            label(ui, "Responsive frame");
            ui.horizontal(|ui| {
                for (name, width) in [("Phone", 390.0), ("Tablet", 768.0), ("Desktop", 1440.0)] {
                    if control(
                        ui,
                        ("resize-frame", name),
                        &format!("Resize frame {name}"),
                        |ui| ui.small_button(name),
                    )
                    .clicked()
                    {
                        studio.resize_layout_frame(width, shape.geom.bbox().height());
                    }
                }
            });
            label(ui, "Layout");
            let mut enabled = props.stack.is_some();
            if control(
                ui,
                "auto-layout-enabled",
                "Arrange children automatically",
                |ui| {
                    ui.add(
                        egui::Checkbox::new(&mut enabled, "Arrange children automatically")
                            .indeterminate(stack_mixed(&stacks, LayoutStackField::Enabled)),
                    )
                },
            )
            .changed()
            {
                props.stack = enabled.then(AutoStack::default);
                changed |= stack_intent(&mut fields, LayoutStackField::Enabled, true);
            }
            if let Some(stack) = &mut props.stack {
                ui.scope(|ui| {
                    if stacks.iter().any(Option::is_none)
                        && !fields.contains(&LayoutStackField::Enabled)
                    {
                        ui.disable();
                    }
                    ui.horizontal(|ui| {
                        for (name, v) in [
                            ("Stack", StackFlow::Stack),
                            ("Wrap", StackFlow::Wrap),
                            ("Grid", StackFlow::Grid),
                        ] {
                            let response = control(ui, ("stack-flow", name), name, |ui| {
                                ui.selectable_label(
                                    !stack_mixed(&stacks, LayoutStackField::Flow)
                                        && stack.flow == v,
                                    name,
                                )
                            });
                            if response.enabled() && response.clicked() {
                                stack.flow = v;
                            }
                            if stack_mixed(&stacks, LayoutStackField::Flow) {
                                ui.ctx().accesskit_node_builder(response.id, |node| {
                                    node.set_state_description("Mixed")
                                });
                            }
                            changed |= stack_intent(
                                &mut fields,
                                LayoutStackField::Flow,
                                response.enabled() && response.clicked(),
                            );
                        }
                    });
                    if stack.flow == StackFlow::Grid {
                        if stack.column_tracks.is_empty() {
                            changed |= stack_number(
                                ui,
                                (layer, id),
                                "uniform-columns",
                                "Uniform columns",
                                &mut stack.columns,
                                1..=1024,
                                " columns",
                                &stacks,
                                &mut fields,
                                LayoutStackField::Columns,
                            );
                        }
                        changed |= grid_tracks(
                            ui,
                            (layer, id),
                            "Column",
                            &mut stack.column_tracks,
                            studio.canvas_gen,
                            &stacks,
                            &mut fields,
                        );
                        changed |= grid_tracks(
                            ui,
                            (layer, id),
                            "Row",
                            &mut stack.row_tracks,
                            studio.canvas_gen,
                            &stacks,
                            &mut fields,
                        );
                    } else {
                        ui.horizontal(|ui| {
                            let response = control(ui, "stack-horizontal", "Horizontal", |ui| {
                                ui.selectable_label(
                                    !stack_mixed(&stacks, LayoutStackField::Direction)
                                        && stack.direction == StackAxis::Horizontal,
                                    "Horizontal",
                                )
                            });
                            if response.enabled() && response.clicked() {
                                stack.direction = StackAxis::Horizontal;
                            }
                            if stack_mixed(&stacks, LayoutStackField::Direction) {
                                ui.ctx().accesskit_node_builder(response.id, |node| {
                                    node.set_state_description("Mixed")
                                });
                            }
                            changed |= stack_intent(
                                &mut fields,
                                LayoutStackField::Direction,
                                response.enabled() && response.clicked(),
                            );
                            let response = control(ui, "stack-vertical", "Vertical", |ui| {
                                ui.selectable_label(
                                    !stack_mixed(&stacks, LayoutStackField::Direction)
                                        && stack.direction == StackAxis::Vertical,
                                    "Vertical",
                                )
                            });
                            if response.enabled() && response.clicked() {
                                stack.direction = StackAxis::Vertical;
                            }
                            if stack_mixed(&stacks, LayoutStackField::Direction) {
                                ui.ctx().accesskit_node_builder(response.id, |node| {
                                    node.set_state_description("Mixed")
                                });
                            }
                            changed |= stack_intent(
                                &mut fields,
                                LayoutStackField::Direction,
                                response.enabled() && response.clicked(),
                            );
                        });
                    }
                    let response = stack_choice(
                        ui,
                        "layout-align",
                        "Align",
                        &mut stack.align,
                        &[
                            StackAlign::Start,
                            StackAlign::Center,
                            StackAlign::End,
                            StackAlign::Stretch,
                            StackAlign::Baseline,
                        ]
                        .map(|v| (v, format!("{v:?}"))),
                        stack_mixed(&stacks, LayoutStackField::Align),
                    );
                    changed |=
                        stack_intent(&mut fields, LayoutStackField::Align, response.changed());
                    let response = stack_choice(
                        ui,
                        "layout-justify",
                        "Distribute",
                        &mut stack.justify,
                        &[
                            (StackJustify::Start, "Start".into()),
                            (StackJustify::Center, "Center".into()),
                            (StackJustify::End, "End".into()),
                            (StackJustify::SpaceBetween, "Space between".into()),
                            (StackJustify::SpaceAround, "Space around".into()),
                            (StackJustify::SpaceEvenly, "Space evenly".into()),
                        ],
                        stack_mixed(&stacks, LayoutStackField::Justify),
                    );
                    changed |=
                        stack_intent(&mut fields, LayoutStackField::Justify, response.changed());
                    ui.horizontal(|ui| {
                        changed |= stack_number(
                            ui,
                            (layer, id),
                            "layout-gap",
                            "Gap",
                            &mut stack.gap,
                            0.0..=1000.0,
                            " px",
                            &stacks,
                            &mut fields,
                            LayoutStackField::Gap,
                        );
                        if stack.flow != StackFlow::Stack {
                            changed |= stack_number(
                                ui,
                                (layer, id),
                                "layout-cross-gap",
                                "Row gap",
                                &mut stack.cross_gap,
                                0.0..=1000.0,
                                " px",
                                &stacks,
                                &mut fields,
                                LayoutStackField::CrossGap,
                            );
                        }
                    });
                    ui.label(
                        RichText::new("Padding · top / right / bottom / left")
                            .small()
                            .color(fg_weak()),
                    );
                    ui.horizontal(|ui| {
                        for (edge, (name, p)) in ["Top", "Right", "Bottom", "Left"]
                            .into_iter()
                            .zip(&mut stack.padding)
                            .enumerate()
                        {
                            changed |= stack_number(
                                ui,
                                (layer, id),
                                ("padding", name),
                                name,
                                p,
                                0.0..=1000.0,
                                " px",
                                &stacks,
                                &mut fields,
                                LayoutStackField::Padding(edge),
                            );
                        }
                    });
                });
            }
        }
        if props.parent.is_some() {
            if props
                .parent
                .and_then(|parent| studio.doc.find_shape(layer, parent))
                .and_then(|shape| shape.layout.stack.as_ref())
                .is_some_and(|s| s.flow == StackFlow::Grid)
            {
                label(ui, "Grid placement");
                ui.label(
                    RichText::new(
                        "Rows and columns start at zero. Unset anchors place automatically.",
                    )
                    .small()
                    .color(fg_weak()),
                );
                for (name, anchor) in [
                    ("Column", &mut props.grid.column),
                    ("Row", &mut props.grid.row),
                ] {
                    ui.push_id((layer, id, "grid-anchor", name), |ui| {
                        let mut enabled = anchor.is_some();
                        if control(
                            ui,
                            ("grid-anchor-enabled", name),
                            &format!("Set {name}"),
                            |ui| ui.checkbox(&mut enabled, format!("Set {name}")),
                        )
                        .changed()
                        {
                            *anchor = enabled.then_some(0);
                            changed = true;
                        }
                        if let Some(value) = anchor {
                            changed |= layout_number(
                                ui,
                                (layer, id),
                                ("grid-anchor", name),
                                name,
                                value,
                                0..=1023,
                                " index",
                            );
                        }
                    });
                }
                ui.horizontal(|ui| {
                    changed |= layout_number(
                        ui,
                        (layer, id),
                        "grid-column-span",
                        "Column span",
                        &mut props.grid.column_span,
                        1..=1024,
                        " tracks",
                    );
                    changed |= layout_number(
                        ui,
                        (layer, id),
                        "grid-row-span",
                        "Row span",
                        &mut props.grid.row_span,
                        1..=1024,
                        " tracks",
                    );
                });
            }
            label(ui, "Position in frame");
            changed |= control(ui, "absolute-position", "Absolute position", |ui| {
                ui.checkbox(&mut props.absolute, "Absolute position")
            })
            .changed();
            for (name, value) in [
                ("Horizontal", &mut props.constraint_x),
                ("Vertical", &mut props.constraint_y),
            ] {
                let old = *value;
                super::keyboard_controls::choice(
                    ui,
                    name,
                    &format!("{name} position constraint"),
                    value,
                    &Constraint::all()
                        .into_iter()
                        .map(|v| (v, v.name().into()))
                        .collect::<Vec<_>>(),
                );
                changed |= old != *value;
            }
            ui.horizontal(|ui| {
                if ui.small_button("Earlier").clicked() {
                    studio.reorder_layout_selection(false);
                }
                if ui.small_button("Later").clicked() {
                    studio.reorder_layout_selection(true);
                }
                if ui.small_button("Detach from frame").clicked() {
                    studio.reparent_layout_selection(None);
                }
            });
        }
        if props.frame {
            let mut locked = props.aspect_ratio.is_some();
            if control(ui, "aspect-ratio-locked", "Lock aspect ratio", |ui| {
                ui.checkbox(&mut locked, "Lock aspect ratio")
            })
            .changed()
            {
                props.aspect_ratio = locked
                    .then_some(shape.geom.bbox().width() / shape.geom.bbox().height().max(1.0));
                changed = true;
            }
        }
        if props.frame || matches!(shape.geom, Geom::Text(_)) {
            changed |= breakpoints(ui, &shape, &mut props, studio.canvas_gen);
        }
        if changed && studio.apply_layout_inspector_with_fields(&layout_target, &props, &fields) {
            keyboard::Rows::<GridTrack>::accepted(ui, "Column", studio.canvas_gen);
            keyboard::Rows::<GridTrack>::accepted(ui, "Row", studio.canvas_gen);
            keyboard::Rows::<crate::layout::LayoutBreakpoint>::accepted(
                ui,
                "breakpoints",
                studio.canvas_gen,
            );
        }
    });
    if matches!(shape.geom, Geom::Text(_)) {
        super::paragraph::show(ui, studio);
    }
    super::layout_preview::interaction_editor(ui, studio);
    super::layout_preview::component_editor(ui, studio);
    super::layout_properties::inspector(ui, studio);
    variables(ui, studio);
    if shape.layout.frame {
        label(ui, "Export frame");
        ui.horizontal(|ui| {
            if ui.button("PNG").clicked() {
                studio.export_selected_frame_png();
            }
            if ui.button("SVG").clicked() {
                studio.export_selected_frame_svg();
            }
            if ui.button("HTML").clicked() {
                studio.export_selected_frame_html();
            }
        });
    }
}

struct LoadedImage {
    document: String,
    layer: usize,
    id: u64,
    image: crate::layout_images::ImageFill,
}
pub fn poll_image(ctx: &egui::Context, studio: &mut Studio) {
    if studio.file_dialog_pending() {
        return;
    }
    if let Some(result) = super::jobs::poll::<LoadedImage>(ctx, "layout-image-load") {
        match result {
            Ok(image) if image.document == studio.swap_id => {
                if let Some(shape) = studio.doc.find_shape(image.layer, image.id) {
                    let before = shape.layout.clone();
                    let mut after = before.clone();
                    after.image = Some(image.image);
                    after.placeholder = false;
                    studio.commit(Cmd::SetLayout {
                        layer: image.layer,
                        id: image.id,
                        before,
                        after,
                    });
                    studio.status = "Image placed · embedded in this document".into();
                }
            }
            Ok(_) => studio.status = "Image load cancelled because the document changed".into(),
            Err(e) => studio.status = e,
        }
    }
}

/// Reveal the existing native value field created by an accepted numeric variable action.
/// Args: UI, real field, document request, token identity and revision. Returns without editing values.
fn focus_new_variable(ui: &Ui, response: &egui::Response, key: egui::Id, token: u64, revision: u64) {
    let pending = ui.data(|data| data.get_temp::<(u64, u64)>(key));
    if pending.is_some_and(|(_, expected)| expected != revision) {
        ui.data_mut(|data| data.remove::<(u64, u64)>(key));
        return;
    }
    if pending == Some((token, revision))
        && response.enabled()
        && ui.memory(|memory| memory.allows_interaction(response.layer_id))
    {
        response.request_focus();
        super::welcome::focus_control(ui, response);
        response.scroll_to_me_animation(None, egui::style::ScrollAnimation::none());
        if ui.clip_rect().contains_rect(response.rect) {
            ui.data_mut(|data| data.remove::<(u64, u64)>(key));
        }
        if ui.ctx().current_pass_index() == 0 {
            ui.ctx().request_discard("Reveal the newly created variable");
        }
    }
}

pub(super) fn variables(ui: &mut Ui, studio: &mut Studio) {
    use crate::layout_tokens::{DesignToken, TokenProperty, TokenValue};
    egui::CollapsingHeader::new("Design variables").show(ui, |ui| {
        let blocked = keyboard::blocked(studio);
        let focus = egui::Id::new(("new-layout-variable", &studio.swap_id, &studio.selection, studio.active_layer));
        studio.with_field_history(ui, "layout-design-variables", |ui, studio| {
            if blocked {
                ui.disable();
            }
            let mut tokens = studio.doc.layout_tokens.clone();
            let mut changed = false;
            let mut remove = None;
            for token in &mut tokens {
                let owner = ui.make_persistent_id(("design-variable", token.id));
                super::fields::native_scope(ui, owner, |ui| {
                    ui.horizontal(|ui| {
                        match &mut token.value {
                            TokenValue::Color(color) => {
                                ui.add(egui::Label::new(&token.name).truncate());
                                let owner = ui.make_persistent_id("color");
                                changed |= super::color_picker::color_edit(ui, owner, color);
                            }
                            TokenValue::Number(value) => {
                                let field = keyboard::number(
                                    ui,
                                    "value",
                                    &format!("Variable {}", token.name),
                                    value,
                                    0.0..=10000.0,
                                    None,
                                    "",
                                    "",
                                );
                                focus_new_variable(ui, &field.response, focus, token.id, studio.canvas_gen);
                                changed |= crate::app::input_history::record(field.response, field.changed);
                            }
                        }
                        let owner = ui.make_persistent_id("remove");
                        let response = super::svg_paint::control(
                            ui,
                            owner,
                            &format!("Remove variable {}", token.name),
                            |ui| {
                                ui.small_button("×")
                                    .on_hover_text("Remove variable, keeping current appearance")
                            },
                        );
                        if response.clicked() {
                            response.surrender_focus();
                            remove = Some(token.id);
                        }
                    });
                });
            }
            if changed {
                studio.set_layout_tokens(tokens);
            }
            if let Some(id) = remove {
                studio.remove_layout_token(id);
            }
            let key = ui.make_persistent_id("new-design-variable-name");
            let mut name = ui
                .data(|data| data.get_temp::<String>(key))
                .unwrap_or_default();
            let label = ui.label("Variable name");
            let response = ui
                .add(
                    egui::TextEdit::singleline(&mut name)
                        .id(key)
                        .hint_text("Variable name"),
                )
                .labelled_by(label.id);
            super::welcome::name_control(ui, &response, label.id, "Variable name");
            ui.data_mut(|data| data.insert_temp(key, name.clone()));
            ui.horizontal(|ui| {
                if control(ui, "add-color-variable", "+ Color", |ui| {
                    ui.add_enabled(!name.trim().is_empty(), egui::Button::new("+ Color"))
                })
                .clicked()
                {
                    let mut tokens = studio.doc.layout_tokens.clone();
                    let color = match studio.style.fill {
                        crate::document::Fill::Solid(color) => color,
                        _ => crate::color::Rgba::rgb(232, 106, 60),
                    };
                    tokens.push(DesignToken::color(name.trim(), color));
                    studio.set_layout_tokens(tokens);
                }
                if control(ui, "add-number-variable", "+ Number", |ui| {
                    ui.add_enabled(!name.trim().is_empty(), egui::Button::new("+ Number"))
                })
                .clicked()
                {
                    let mut tokens = studio.doc.layout_tokens.clone();
                    let token = DesignToken::number(name.trim(), 16.0);
                    let id = token.id;
                    tokens.push(token);
                    studio.set_layout_tokens(tokens);
                    if studio.doc.layout_tokens.iter().any(|token| token.id == id) {
                        ui.data_mut(|data| data.insert_temp(focus, (id, studio.canvas_gen)));
                        ui.ctx().request_repaint();
                    }
                }
            });
            if let Some((layer, id)) = studio.primary()
                && let Some(shape) = studio.doc.find_shape(layer, id).cloned()
            {
                for property in TokenProperty::all() {
                    if !property.supports(&shape) {
                        continue;
                    }
                    let current = shape.layout.tokens.get(property);
                    let mut selected = current;
                    let mut choices = vec![(None, "Local value".into())];
                    choices.extend(
                        studio
                            .doc
                            .layout_tokens
                            .iter()
                            .filter(|token| property.accepts(token.value))
                            .map(|token| (Some(token.id), token.name.clone())),
                    );
                    super::keyboard_controls::choice(
                        ui,
                        property.name(),
                        &format!("Bind {}", property.name()),
                        &mut selected,
                        &choices,
                    );
                    if selected != current {
                        studio.bind_layout_token(property, selected);
                    }
                }
            }
        });
    });
}

pub fn hierarchy(ui: &mut Ui, studio: &mut Studio) {
    if studio.persona != Persona::Layout || studio.show_welcome {
        return;
    }
    egui::Panel::left("layout-hierarchy")
        .resizable(true)
        .default_size(200.0)
        .size_range(170.0..=320.0)
        .frame(egui::Frame::new().fill(bg_panel()).inner_margin(12))
        .show(ui, |ui| {
            hierarchy_content(ui, studio);
        });
}

/// Draw Layout's existing node hierarchy inside a movable panel.
/// Args: panel UI and studio. Returns no value; node/component operations remain app-owned.
pub(super) fn hierarchy_content(ui: &mut Ui, studio: &mut Studio) {
    ui.horizontal(|ui| {
        ui.label(RichText::new("Layers").strong());
        if ui.small_button("+ Frame").clicked() {
            studio.insert_layout_frame("Frame", 390.0, 844.0);
        }
    });
    ui.add_space(8.0);
    egui::ScrollArea::vertical().show(ui, |ui| {
        let mut rows = Vec::new();
        let roots: Vec<_> = studio
            .doc
            .layers
            .iter()
            .enumerate()
            .filter(|(_, layer)| layer.parent.is_none())
            .map(|(li, _)| li)
            .rev()
            .collect();
        for li in roots {
            layer_row(ui, studio, li, 0, &mut rows);
        }
        if let Some(drop) = super::layer_drag::drop_target(ui, studio, &rows) {
            drop.apply(studio);
        }
    });
}

fn layer_row(
    ui: &mut Ui,
    studio: &mut Studio,
    li: usize,
    depth: usize,
    rows: &mut Vec<(super::layer_drag::TreeRow, egui::Rect)>,
) {
    if depth > 64 {
        return;
    }
    let layer = &studio.doc.layers[li];
    let id = layer.id;
    let is_group = layer.is_group;
    let name = layer.name.clone();
    let key = egui::Id::new(("layout-layer-open", id));
    let mut open = ui.ctx().data(|d| d.get_temp::<bool>(key)).unwrap_or(true);
    let row = ui
        .horizontal(|ui| {
            ui.add_space((depth as f32 * 10.).min(70.));
            if ui.small_button(if open { "▾" } else { "▸" }).clicked() {
                open = !open;
                ui.ctx().data_mut(|d| d.insert_temp(key, open));
            }
            ui.add(
                egui::Button::selectable(
                    studio.selected_layer == Some(id),
                    format!("{}  {name}", if is_group { "▱" } else { "≡" }),
                )
                .sense(egui::Sense::click_and_drag())
                .truncate(),
            )
        })
        .inner;
    rows.push((super::layer_drag::TreeRow::Layer(li), row.rect));
    if row.clicked() || super::layer_drag::source(&row, studio, li) {
        if !studio.apply_pending_item_mask(li, None) {
            let previous = ui
                .input(|i| i.modifiers.shift)
                .then(|| studio.selection.clone());
            studio.activate_layer_tree(li);
            if let Some(previous) = previous {
                for item in previous {
                    if !studio.selection.contains(&item) {
                        studio.selection.push(item);
                    }
                }
            }
            studio.tool = Tool::Select;
        }
    }
    row.context_menu(|ui| {
        if let Some(action) = super::masking::artwork_menu(ui, studio, li, None) {
            action.run(studio, li, None);
        }
        if ui.button("Use as mask…").clicked() {
            studio.begin_item_mask(li, None);
            ui.close();
        }
        if ui
            .add_enabled(studio.layer_unlocked(li), egui::Button::new("Delete layer"))
            .clicked()
        {
            studio.delete_layer_tree(li);
            ui.close();
        }
    });
    if li >= studio.doc.layers.len() {
        return;
    }
    if open {
        if is_group {
            let children: Vec<_> = studio
                .doc
                .layers
                .iter()
                .enumerate()
                .filter(|(_, l)| l.parent == Some(id))
                .map(|(i, _)| i)
                .rev()
                .collect();
            for child in children {
                layer_row(ui, studio, child, depth + 1, rows);
            }
        } else {
            let objects: Vec<_> = studio.doc.layers[li]
                .kind
                .shapes()
                .into_iter()
                .flatten()
                .filter(|s| s.layout.parent.is_none())
                .map(|s| s.id)
                .rev()
                .collect();
            for object in objects {
                tree_row(ui, studio, li, object, depth + 1, rows);
            }
        }
    }
}

fn tree_row(
    ui: &mut Ui,
    studio: &mut Studio,
    li: usize,
    id: u64,
    depth: usize,
    rows: &mut Vec<(super::layer_drag::TreeRow, egui::Rect)>,
) {
    if depth > 64 {
        return;
    }
    let Some(shape) = studio.doc.find_shape(li, id) else {
        return;
    };
    let name = shape.name.clone();
    let icon = if shape.layout.component.is_some() {
        "◇"
    } else if shape.layout.frame {
        "▣"
    } else if matches!(shape.geom, Geom::Text(_)) {
        "T"
    } else {
        "▪"
    };
    let children = crate::layout::children(&studio.doc, li, id);
    let selected = studio.selection.contains(&(li, id));
    let key = egui::Id::new(("layout-tree-open", id));
    let mut open = ui
        .ctx()
        .data(|d| d.get_temp::<bool>(key))
        .unwrap_or(depth == 0);
    let row = ui
        .horizontal(|ui| {
            ui.add_space((depth as f32 * 10.0).min(70.0));
            if !children.is_empty() {
                if ui.small_button(if open { "▾" } else { "▸" }).clicked() {
                    open = !open;
                    ui.ctx().data_mut(|d| d.insert_temp(key, open));
                }
            } else {
                ui.add_space(18.0);
            }
            ui.add(
                egui::Button::selectable(
                    selected,
                    RichText::new(format!("{icon}  {name}")).color(if selected {
                        accent()
                    } else {
                        super::theme::fg()
                    }),
                )
                .sense(egui::Sense::click_and_drag())
                .truncate(),
            )
        })
        .inner;
    rows.push((super::layer_drag::TreeRow::Object(li, id), row.rect));
    if row.double_clicked() {
        studio.enter_group_item((li, id));
    } else if row.clicked() || (super::layer_drag::object_source(&row, studio, li, id) && !selected)
    {
        if !studio.apply_pending_item_mask(li, Some(id)) {
            let hits = studio.selection_for_hit((li, id));
            if studio.individual_object != Some((li, id)) {
                studio.individual_object = None;
            }
            if !ui.input(|i| i.modifiers.shift) {
                studio.selection.clear();
            }
            for hit in hits {
                if !studio.selection.contains(&hit) {
                    studio.selection.push(hit);
                }
            }
            studio.active_layer = Some(li);
            studio.selected_layer = None;
            studio.tool = Tool::Select;
        }
    }
    row.context_menu(|ui| {
        if ui.button("Select").clicked() {
            studio.selection = vec![(li, id)];
            ui.close();
        }
        if ui
            .add_enabled(
                studio
                    .doc
                    .find_shape(li, id)
                    .is_some_and(|s| s.layout.frame),
                egui::Button::new("Nest selection here"),
            )
            .clicked()
        {
            studio.reparent_layout_selection(Some(id));
            ui.close();
        }
        if ui.button("Duplicate").clicked() {
            studio.selection = vec![(li, id)];
            studio.duplicate_selection();
            ui.close();
        }
        if let Some(action) = super::masking::artwork_menu(ui, studio, li, Some(id)) {
            action.run(studio, li, Some(id));
        }
        if ui.button("Use as mask…").clicked() {
            studio.begin_item_mask(li, Some(id));
            ui.close();
        }
        if ui
            .add_enabled(
                studio.pixel_sel.is_some(),
                egui::Button::new("Mask from selection"),
            )
            .clicked()
        {
            studio.mask_object_from_selection(li, id);
            ui.close();
        }
        if ui.button("Delete object").clicked() {
            studio.delete_object_item(li, id);
            ui.close();
        }
    });
    if open {
        for child in children.into_iter().rev() {
            tree_row(ui, studio, li, child, depth + 1, rows);
        }
    }
}
