mod catalog;
mod team;
mod workspace;
pub(crate) mod actions;
mod scrub;

use crate::app::{Studio, WelcomePage};
use crate::presets;
use crate::ui::theme::{accent, accent_soft, bg_panel, bg_widget_hover, fg, fg_weak};
use eframe::egui::{self, Align2, Button, Frame, Id, Margin, Response, RichText, ScrollArea, Sense, Stroke, Ui, vec2};
use std::{cell::RefCell, ops::RangeInclusive};

/// Name an existing native control and link its visible label.
/// Args: UI, native response, visible label and purpose including units. Returns no value.
pub(super) fn name_control(ui: &Ui, response: &Response, label: Id, purpose: &str) {
    let _ = response.clone().labelled_by(label);
    ui.ctx().accesskit_node_builder(response.id, |node| node.set_label(purpose));
    focus_control(ui, response);
}

/// Paint and reveal native keyboard focus.
/// Args: UI and actual widget response. Returns no value; it never changes focus ownership.
pub(super) fn focus_control(ui: &Ui, response: &Response) {
    if response.gained_focus() { response.scroll_to_me(Some(egui::Align::Center)); }
    if response.has_focus() {
        ui.painter().rect_stroke(response.rect.expand(2.), 3.,
            Stroke::new(2., ui.visuals().selection.stroke.color), egui::StrokeKind::Outside);
    }
}

/// Parse the native numeric grammar and observe its attempted draft.
/// Args: frame-local observer and native text. Returns a finite number for egui's existing range clamp.
pub(super) fn parse_number(attempt: &RefCell<Option<String>>, text: &str) -> Option<f64> {
    attempt.replace(Some(text.to_owned()));
    finite_number(text)
}

fn finite_number(text: &str) -> Option<f64> {
    text.chars().filter(|c| !c.is_whitespace()).map(|c| if c == '−' { '-' } else { c })
        .collect::<String>().parse::<f64>().ok().filter(|value| value.is_finite())
}

/// Validate an observed native edit using app-owned state.
/// Args: UI, native response, stable property owner, attempted text, result and native range. Returns whether the owner may change.
pub(super) fn native_number_valid(ui: &Ui, response: &Response, property: Id, attempt: Option<String>, value: f64, range: RangeInclusive<f64>) -> bool {
    if !response.enabled() { return false; }
    let key = property.with("number-validation");
    if (response.has_focus() || response.lost_focus()) && ui.input(|input| input.key_pressed(egui::Key::Escape)) {
        ui.data_mut(|data| data.remove::<String>(key));
        return value.is_finite() && range.contains(&value);
    }
    let invalid_action = ui.input(|input| input.accesskit_action_requests(response.id, egui::accesskit::Action::SetValue)
        .any(|request| matches!(request.data, Some(egui::accesskit::ActionData::NumericValue(value)) if !value.is_finite())));
    let invalid_value = !value.is_finite() || !range.contains(&value) || invalid_action;
    if let Some(text) = attempt {
        ui.data_mut(|data| {
            if finite_number(&text).is_none() || invalid_value { data.insert_temp(key, text); }
            else { data.remove::<String>(key); }
        });
    } else if invalid_value {
        ui.data_mut(|data| data.insert_temp(key, value.to_string()));
    } else if response.changed() {
        ui.data_mut(|data| data.remove::<String>(key));
    }
    !invalid_value && ui.data(|data| data.get_temp::<String>(key).is_none())
}

/// Associate one visible validation message with its native controls.
/// Args: UI, affected responses and plain error. Returns no value; native reader announcements are unproven.
pub(super) fn control_error(ui: &mut Ui, fields: &[&Response], message: &str) {
    let error = ui.colored_label(ui.visuals().error_fg_color, message);
    ui.ctx().accesskit_node_builder(error.id, |node| {
        node.set_live(egui::accesskit::Live::Polite);
        node.set_live_atomic();
    });
    for response in fields {
        ui.ctx().accesskit_node_builder(response.id, |node| {
            node.set_invalid(egui::accesskit::Invalid::True);
            node.clear_numeric_value();
            node.set_error_message(error.id.accesskit_id());
            node.push_described_by(error.id.accesskit_id());
        });
    }
}

const GROUPS: &[(&str, Option<&str>)] = &[
    ("All", None),
    ("Web", Some("Screen")),
    ("Print", Some("Print")),
    ("Social", Some("Social")),
    ("Photo", Some("Photo")),
    ("Identity", Some("Identity")),
];

pub(super) fn file_action_busy(ctx: &eframe::egui::Context) -> bool { actions::busy(ctx) }

pub(super) fn tick(ctx: &eframe::egui::Context, studio: &mut Studio) { actions::tick(ctx, studio); }

pub fn show(ui: &mut Ui, studio: &mut Studio) {
    workspace::show(ui, studio);
}
pub(super) fn set_capture_catalog_root(ctx: &eframe::egui::Context, root: &std::path::Path) {
    catalog::set_capture_catalog_root(ctx, root);
}
pub(super) fn ready(ctx: &eframe::egui::Context) -> bool {
    workspace::ready(ctx)
}
pub(super) fn modal_open(ctx: &eframe::egui::Context) -> bool {
    workspace::modal_open(ctx) || actions::busy(ctx)
}
pub(super) fn cancel(ctx: &eframe::egui::Context) {
    catalog::cancel(ctx);
    team::suspend(ctx);
    workspace::leave(ctx);
}

pub(super) fn startup_preferences(ui: &mut Ui, studio: &mut Studio) {
    let id = ui.make_persistent_id(("startup-preferences", &studio.swap_id));
    ui.scope_builder(egui::UiBuilder::new().id(id), |ui| {
        if studio.file_dialog_pending() || studio.updates.freezing { ui.disable(); }
        startup_controls(ui, studio);
    });
}

fn startup_controls(ui: &mut Ui, studio: &mut Studio) {
    use crate::app::startup::{StartPage, StartTab};
    use crate::tools::Persona;
    let previous = studio.startup_preferences.clone();
    let launch_label = ui.label("Open at launch").id;
    let label = match studio.startup_preferences.start_tab {
        StartTab::Welcome => "Start screen",
        StartTab::Mode(mode) => mode.name(),
        StartTab::RememberLast => "Remember last mode",
    };
    let mut chosen = false;
    let mode = eframe::egui::ComboBox::from_id_salt("startup-mode")
        .selected_text(label)
        .width(190.0)
        .show_ui(ui, |ui| {
            let choice = &mut studio.startup_preferences.start_tab;
            if ui.selectable_value(choice, StartTab::Welcome, "Start screen").clicked() { chosen = true; ui.close(); }
            for persona in [
                Persona::Design,
                Persona::Pixel,
                Persona::Layout,
                Persona::Photo,
                Persona::Motion,
            ] {
                if ui.selectable_value(choice, StartTab::Mode(persona), persona.name()).clicked() { chosen = true; ui.close(); }
            }
            if ui.selectable_value(choice, StartTab::RememberLast, "Remember last mode").clicked() { chosen = true; ui.close(); }
        });
    if chosen { mode.response.request_focus(); }
    name_control(ui, &mode.response, launch_label, "Open at launch");
    let page_label = ui.label("Start screen tab").id;
    let page_name = |page| match page {
        WelcomePage::New => "New document",
        WelcomePage::Templates => "Templates",
        WelcomePage::Recents => "Recent",
        WelcomePage::Recovered => "Recovered",
    };
    let label = match studio.startup_preferences.start_page {
        StartPage::Page(page) => page_name(page),
        StartPage::RememberLast => "Remember last tab",
    };
    let mut chosen = false;
    let page = eframe::egui::ComboBox::from_id_salt("startup-page")
        .selected_text(label)
        .width(190.0)
        .show_ui(ui, |ui| {
            let choice = &mut studio.startup_preferences.start_page;
            for page in [
                WelcomePage::New,
                WelcomePage::Templates,
                WelcomePage::Recents,
                WelcomePage::Recovered,
            ] {
                if ui.selectable_value(choice, StartPage::Page(page), page_name(page)).clicked() { chosen = true; ui.close(); }
            }
            if ui.selectable_value(choice, StartPage::RememberLast, "Remember last tab").clicked() { chosen = true; ui.close(); }
        });
    if chosen { page.response.request_focus(); }
    name_control(ui, &page.response, page_label, "Start screen tab");
    if studio.startup_preferences != previous {
        studio.save_startup_preferences();
    }
}

fn new_page(ui: &mut Ui, studio: &mut Studio) {
    let id = ui.make_persistent_id(("new-document", &studio.swap_id));
    ui.scope_builder(egui::UiBuilder::new().id(id), |ui| {
        if studio.file_dialog_pending() || studio.updates.freezing { ui.disable(); }
        new_controls(ui, studio);
    });
}

fn new_controls(ui: &mut Ui, studio: &mut Studio) {
    ui.horizontal_wrapped(|ui| {
        for &(label, _) in GROUPS {
            let id = ui.make_persistent_id(("preset-group", label));
            let response = ui.scope_builder(egui::UiBuilder::new().id(id), |ui| {
                ui.selectable_label(studio.new_doc_group == label, label)
            }).inner;
            focus_control(ui, &response);
            if response.clicked() {
                studio.new_doc_group = label.to_owned();
            }
        }
    });
    ui.add_space(14.0);
    let filter = GROUPS
        .iter()
        .find(|(label, _)| *label == studio.new_doc_group)
        .and_then(|(_, group)| *group);
    let width = ui.available_width();
    let columns = if width >= 760.0 {
        3
    } else if width >= 480.0 {
        2
    } else {
        1
    };
    let gap = 10.0;
    let card_width = (width - gap * (columns - 1) as f32) / columns as f32;
    let preset_count = presets::all()
        .iter()
        .filter(|preset| filter.is_none_or(|group| preset.group == group))
        .count();
    let rows = preset_count.div_ceil(columns);
    let grid_height = (rows as f32 * 76.0 + rows.saturating_sub(1) as f32 * gap)
        .min(288.0)
        .min((ui.ctx().content_rect().height() - 300.).max(80.));
    ScrollArea::vertical()
        .id_salt("welcome-presets")
        .max_height(grid_height)
        .min_scrolled_height(grid_height)
        .auto_shrink([false, false])
        .show(ui, |ui| {
            eframe::egui::Grid::new("preset-grid")
                .num_columns(columns)
                .spacing([gap, gap])
                .show(ui, |ui| {
                    let mut count = 0;
                    for preset in presets::all()
                        .iter()
                        .copied()
                        .filter(|preset| filter.is_none_or(|group| preset.group == group))
                    {
                        if preset_card(ui, preset, card_width)
                            && ui.is_enabled() && !studio.file_dialog_pending() && !studio.updates.freezing
                        {
                            studio.new_from_preset(preset);
                        }
                        count += 1;
                        if count % columns == 0 {
                            ui.end_row();
                        }
                    }
                    if count % columns != 0 {
                        ui.end_row();
                    }
                });
        });
    ui.add_space(20.0);
    Frame::new()
        .fill(bg_panel())
        .corner_radius(8.0)
        .inner_margin(Margin::same(14))
        .show(ui, |ui| {
            ui.set_width((width - 28.0).max(180.0));
            let mut fields = Vec::new();
            let mut invalid = Vec::new();
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new("Custom size").strong().size(12.0));
                ui.add_space(8.0);
                let before = studio.custom_w;
                let id = ui.make_persistent_id("custom-width");
                let attempt = RefCell::new(None);
                let (response, label) = ui.scope_builder(egui::UiBuilder::new().id(id), |ui| {
                    let label = ui.label("Width (px)").id;
                    (ui.add(
                    eframe::egui::DragValue::new(&mut studio.custom_w)
                        .range(1.0..=65535.0)
                        .prefix("W  ")
                        .custom_parser(|text| parse_number(&attempt, text))
                        .speed(4.0),
                    ), label)
                }).inner;
                name_control(ui, &response, label, "Document width (px)");
                if !native_number_valid(ui, &response, id, attempt.into_inner(), f64::from(studio.custom_w), 1.0..=65535.) {
                    studio.custom_w = before;
                    if response.enabled() { invalid.push(response.id); }
                }
                fields.push(response);
                let before = studio.custom_h;
                let id = ui.make_persistent_id("custom-height");
                let attempt = RefCell::new(None);
                let (response, label) = ui.scope_builder(egui::UiBuilder::new().id(id), |ui| {
                    let label = ui.label("Height (px)").id;
                    (ui.add(
                    eframe::egui::DragValue::new(&mut studio.custom_h)
                        .range(1.0..=65535.0)
                        .prefix("H  ")
                        .custom_parser(|text| parse_number(&attempt, text))
                        .speed(4.0),
                    ), label)
                }).inner;
                name_control(ui, &response, label, "Document height (px)");
                if !native_number_valid(ui, &response, id, attempt.into_inner(), f64::from(studio.custom_h), 1.0..=65535.) {
                    studio.custom_h = before;
                    if response.enabled() { invalid.push(response.id); }
                }
                fields.push(response);
                let before = studio.custom_dpi;
                let id = ui.make_persistent_id("custom-dpi");
                let attempt = RefCell::new(None);
                let (response, label) = ui.scope_builder(egui::UiBuilder::new().id(id), |ui| {
                    let label = ui.label("Resolution (dpi)").id;
                    (ui.add(
                    eframe::egui::DragValue::new(&mut studio.custom_dpi)
                        .range(36.0..=600.0)
                        .suffix(" dpi")
                        .custom_parser(|text| parse_number(&attempt, text))
                        .speed(1.0),
                    ), label)
                }).inner;
                name_control(ui, &response, label, "Document resolution (dpi)");
                if !native_number_valid(ui, &response, id, attempt.into_inner(), f64::from(studio.custom_dpi), 36.0..=600.) {
                    studio.custom_dpi = before;
                    if response.enabled() { invalid.push(response.id); }
                }
                fields.push(response);
            });
            ui.add_space(6.0);
            ui.horizontal_wrapped(|ui| {
                for (property, label, value) in [
                    ("transparent", "Transparent", &mut studio.new_doc_transparent),
                    ("bleed", "Bleed", &mut studio.new_doc_bleed),
                    ("safe-area", "Safe area", &mut studio.new_doc_safe),
                ] {
                    let id = ui.make_persistent_id(property);
                    let response = ui.scope_builder(egui::UiBuilder::new().id(id), |ui| ui.checkbox(value, label)).inner;
                    focus_control(ui, &response);
                }
                ui.add_space(6.0);
                let before = studio.new_doc_artboards;
                let id = ui.make_persistent_id("custom-artboard-count");
                let attempt = RefCell::new(None);
                let (response, label) = ui.scope_builder(egui::UiBuilder::new().id(id), |ui| {
                    let label = ui.label(RichText::new("Artboards").small().color(fg_weak())).id;
                    (ui.add(
                    eframe::egui::DragValue::new(&mut studio.new_doc_artboards)
                        .range(1..=16)
                        .custom_parser(|text| parse_number(&attempt, text))
                        .speed(0.1),
                    ), label)
                }).inner;
                name_control(ui, &response, label, "Document artboards (count)");
                if !native_number_valid(ui, &response, id, attempt.into_inner(), f64::from(studio.new_doc_artboards), 1.0..=16.) {
                    studio.new_doc_artboards = before;
                    if response.enabled() { invalid.push(response.id); }
                }
                fields.push(response);
            });
            let valid = invalid.is_empty() && studio.new_canvas_size_valid(studio.custom_w, studio.custom_h)
                && studio.custom_dpi.is_finite() && (36.0..=600.).contains(&studio.custom_dpi)
                && (1..=16).contains(&studio.new_doc_artboards);
            let allowed = ui.is_enabled() && valid && !studio.file_dialog_pending() && !studio.updates.freezing;
            let id = ui.make_persistent_id("create-document");
            let response = ui.scope_builder(egui::UiBuilder::new().id(id), |ui| {
                ui.add_enabled(allowed, Button::new("Create document").fill(accent_soft()))
            }).inner;
            focus_control(ui, &response);
            if !valid {
                let affected: Vec<_> = fields.iter().filter(|field| invalid.is_empty() || invalid.contains(&field.id)).collect();
                control_error(ui, &affected, if invalid.is_empty() {
                    "Choose 1 to 16 artboards within 64 million pixels and a resolution from 36 to 600 dpi."
                } else { "Enter a finite number for each document size field." });
            }
            if allowed && response.clicked() { studio.new_custom(); }
        });
}

fn preset_card(ui: &mut Ui, preset: presets::Preset, width: f32) -> bool {
    let id = ui.make_persistent_id(("preset", preset.group, preset.name));
    let (_, rect) = ui.allocate_space(vec2(width, 76.0));
    let response = ui.interact(rect, id, Sense::click());
    response.widget_info(|| {
        eframe::egui::WidgetInfo::labeled(eframe::egui::WidgetType::Button, response.enabled(), format!("{} · {} × {} px · {} dpi", preset.name, preset.w, preset.h, preset.dpi))
    });
    if ui.is_rect_visible(rect) {
        let highlighted = response.hovered() || response.has_focus();
        ui.painter().rect_filled(
            rect,
            8.0,
            if highlighted {
                bg_widget_hover()
            } else {
                bg_panel()
            },
        );
        if response.has_focus() {
            ui.painter().rect_stroke(
                rect,
                8.0,
                Stroke::new(1.0, accent()),
                eframe::egui::StrokeKind::Inside,
            );
        }
        let aspect = preset.w / preset.h.max(1.0);
        let preview_size = if aspect > 1.0 {
            vec2(28.0, 28.0 / aspect)
        } else {
            vec2(28.0 * aspect, 28.0)
        };
        let preview = eframe::egui::Rect::from_center_size(
            rect.left_center() + vec2(30.0, 0.0),
            preview_size,
        );
        ui.painter().rect_stroke(
            preview,
            2.0,
            Stroke::new(1.0, if highlighted { accent() } else { fg_weak() }),
            eframe::egui::StrokeKind::Inside,
        );
        ui.painter().text(
            rect.left_center() + vec2(58.0, -10.0),
            Align2::LEFT_CENTER,
            preset.name,
            eframe::egui::FontId::proportional(12.0),
            fg(),
        );
        ui.painter().text(
            rect.left_center() + vec2(58.0, 11.0),
            Align2::LEFT_CENTER,
            format!("{:.0} × {:.0}", preset.w, preset.h),
            eframe::egui::FontId::proportional(11.0),
            fg_weak(),
        );
    }
    response
        .on_hover_text(format!("{} · {} dpi", preset.group, preset.dpi as u32))
        .clicked()
}

#[cfg(test)]
pub(super) mod keyboard_tests {
    use super::*;
    use egui::{Event, Key, Modifiers, accesskit};

    pub fn key(key: Key, modifiers: Modifiers) -> Vec<Event> {
        [true, false].into_iter().map(|pressed| Event::Key {
            key, physical_key: Some(key), pressed, repeat: false, modifiers,
        }).collect()
    }

    pub fn render(ctx: &egui::Context, mut events: Vec<Event>, show: impl FnMut(&mut Ui)) -> egui::FullOutput {
        let modifiers = events.iter().find_map(|event| match event {
            Event::Key { modifiers, .. } => Some(*modifiers), _ => None,
        }).unwrap_or_default();
        events.insert(0, Event::ModifiersChanged(modifiers));
        let mut output = ctx.run_ui(egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, vec2(1200., 1000.))),
            events, ..Default::default()
        }, show);
        output.textures_delta.clear();
        output
    }

    pub fn named<'a>(output: &'a egui::FullOutput, name: &str) -> (accesskit::NodeId, &'a accesskit::Node) {
        output.platform_output.accesskit_update.as_ref().unwrap().nodes.iter()
            .find_map(|(id, node)| (node.label() == Some(name)).then_some((*id, node)))
            .unwrap_or_else(|| panic!("missing native widget {name}"))
    }

    pub fn tab(backwards: bool, mut frame: impl FnMut(Vec<Event>) -> egui::FullOutput) -> egui::FullOutput {
        frame(key(Key::Tab, if backwards { Modifiers::SHIFT } else { Modifiers::NONE }));
        frame(vec![])
    }

    pub fn tab_to(ctx: &egui::Context, name: &str, mut frame: impl FnMut(Vec<Event>) -> egui::FullOutput) -> egui::FullOutput {
        let mut output = frame(vec![]);
        for _ in 0..128 {
            if ctx.memory(|memory| memory.focused()).map(|id| id.accesskit_id()) == Some(named(&output, name).0) {
                return output;
            }
            output = tab(false, &mut frame);
        }
        panic!("native Tab did not reach {name}");
    }

    pub fn replace(text: &str, mut frame: impl FnMut(Vec<Event>) -> egui::FullOutput) -> egui::FullOutput {
        frame(key(Key::A, Modifiers { ctrl: true, command: true, ..Modifiers::NONE }));
        frame(vec![Event::Text(text.into())])
    }

    pub fn focused(ctx: &egui::Context, output: &egui::FullOutput, name: &str) {
        let (id, node) = named(output, name);
        focused_node(ctx, output, name, id, node);
    }

    pub fn focused_role(ctx: &egui::Context, output: &egui::FullOutput, name: &str, role: accesskit::Role) {
        let (id,node)=output.platform_output.accesskit_update.as_ref().unwrap().nodes.iter()
            .find(|(_,node)|node.label()==Some(name)&&node.role()==role)
            .unwrap_or_else(||panic!("missing native {role:?}: {name}"));
        focused_node(ctx,output,name,*id,node);
    }

    fn focused_node(ctx: &egui::Context, output: &egui::FullOutput, name: &str, id: accesskit::NodeId, node: &accesskit::Node) {
        let native=ctx.memory(|memory|memory.focused()).map(|id|id.accesskit_id());
        let actual=output.platform_output.accesskit_update.as_ref().unwrap().nodes.iter()
            .find(|(id,_)|Some(*id)==native).map(|(_,node)|(node.role(),node.label()));
        assert_eq!(native,Some(id),"native focus: {name}; actual node {actual:?}, expected {:?}",node.role());
        assert_eq!(output.platform_output.accesskit_update.as_ref().unwrap().focus, id);
        let bounds = node.bounds().unwrap();
        let outline = egui::Rect::from_min_max(egui::pos2(bounds.x0 as f32, bounds.y0 as f32), egui::pos2(bounds.x1 as f32, bounds.y1 as f32)).expand(2.);
        fn painted(shape: &egui::Shape, outline: egui::Rect) -> bool {
            match shape {
                egui::Shape::Rect(rect) => rect.rect == outline && rect.stroke.width == 2. && rect.stroke.color.a() > 0,
                egui::Shape::Vec(shapes) => shapes.iter().any(|shape| painted(shape, outline)),
                _ => false,
            }
        }
        assert!(output.shapes.iter().any(|shape| painted(&shape.shape, outline)), "visible native focus: {name}");
    }

    pub fn associated_error(output: &egui::FullOutput, name: &str) {
        let (_, node) = named(output, name);
        assert_eq!(node.invalid(), Some(accesskit::Invalid::True));
        let id = node.error_message().expect("explicit native error association");
        assert!(node.described_by().contains(&id));
        let nodes = &output.platform_output.accesskit_update.as_ref().unwrap().nodes;
        let error = &nodes.iter().find(|(node_id, _)| *node_id == id).unwrap().1;
        assert_eq!(error.live(), Some(accesskit::Live::Polite));
        assert!(error.is_live_atomic());
        let text = error.value().unwrap();
        let painted = output.shapes.iter().filter_map(|shape| match &shape.shape {
            egui::Shape::Text(shape) => Some(shape.galley.job.text.as_str()), _ => None,
        }).collect::<Vec<_>>();
        assert_eq!(painted.iter().filter(|painted| **painted == text).count(), 1,
            "The actual error for {name} must paint once: {text:?}; bounds={:?}; painted={painted:?}", error.bounds());
    }

    pub fn action(target_node: accesskit::NodeId, action: accesskit::Action, data: Option<accesskit::ActionData>) -> Vec<Event> {
        vec![Event::AccessKitActionRequest(accesskit::ActionRequest {
            action, target_tree: accesskit::TreeId::ROOT, target_node, data,
        })]
    }

    fn frame(ctx: &egui::Context, studio: &mut Studio, events: Vec<Event>) -> egui::FullOutput {
        render(ctx, events, |ui| new_page(ui, studio))
    }

    #[test]
    fn welcome_keyboard_custom_document_uses_native_fields_and_create() {
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        let mut studio = Studio::new();
        for (name, text) in [
            ("Document width (px)", "128"), ("Document height (px)", "96"),
            ("Document resolution (dpi)", "144"), ("Document artboards (count)", "2"),
        ] {
            let output = tab_to(&ctx, name, |events| frame(&ctx, &mut studio, events));
            focused(&ctx, &output, name);
            assert!(!named(&output, name).1.labelled_by().is_empty());
            replace(text, |events| frame(&ctx, &mut studio, events));
            frame(&ctx, &mut studio, key(Key::Enter, Modifiers::NONE));
        }
        assert_eq!((studio.custom_w, studio.custom_h, studio.custom_dpi, studio.new_doc_artboards), (128., 96., 144., 2));
        let output = tab_to(&ctx, "Create document", |events| frame(&ctx, &mut studio, events));
        focused(&ctx, &output, "Create document");
        let output = tab(true, |events| frame(&ctx, &mut studio, events));
        focused(&ctx, &output, "Document artboards (count)");
        tab_to(&ctx, "Create document", |events| frame(&ctx, &mut studio, events));
        let owner = studio.swap_id.clone();
        frame(&ctx, &mut studio, key(Key::Enter, Modifiers::NONE));
        assert_ne!(studio.swap_id, owner);
        assert_eq!(studio.doc.dpi, 144.);
        assert_eq!(studio.doc.artboards.len(), 2);
        assert!(studio.doc.artboards.iter().all(|board| board.size == crate::geom::Pt::new(128., 96.)));
    }

    #[test]
    fn welcome_keyboard_invalid_size_and_disabled_actions_cannot_create() {
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        let mut studio = Studio::new();
        let owner = studio.swap_id.clone();
        tab_to(&ctx, "Document width (px)", |events| frame(&ctx, &mut studio, events));
        let before = studio.custom_w;
        let output = replace("NaN", |events| frame(&ctx, &mut studio, events));
        associated_error(&output, "Document width (px)");
        let output = tab(false, |events| frame(&ctx, &mut studio, events));
        associated_error(&output, "Document width (px)");
        focused(&ctx, &output, "Document height (px)");
        tab(true, |events| frame(&ctx, &mut studio, events));
        assert_eq!(studio.custom_w, before);
        let create = named(&output, "Create document");
        assert!(create.1.is_disabled());
        frame(&ctx, &mut studio, action(create.0, accesskit::Action::Click, None));
        assert_eq!(studio.swap_id, owner);
        replace("65535", |events| frame(&ctx, &mut studio, events));
        tab_to(&ctx, "Document height (px)", |events| frame(&ctx, &mut studio, events));
        replace("65535", |events| frame(&ctx, &mut studio, events));
        frame(&ctx, &mut studio, key(Key::Enter, Modifiers::NONE));
        let output = frame(&ctx, &mut studio, vec![]);
        associated_error(&output, "Document width (px)");
        assert!(named(&output, "Create document").1.is_disabled());
        studio.updates.freezing = true;
        let before = studio.custom_h;
        let output = frame(&ctx, &mut studio, vec![]);
        let height = named(&output, "Document height (px)").0;
        frame(&ctx, &mut studio, action(height, accesskit::Action::SetValue, Some(accesskit::ActionData::NumericValue(512.))));
        assert_eq!(studio.custom_h, before);
        assert_eq!(studio.swap_id, owner);
    }

    #[test]
    fn welcome_keyboard_filter_preserves_property_and_preset_identity() {
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        let mut studio = Studio::new();
        let initial = frame(&ctx, &mut studio, vec![]);
        let width = named(&initial, "Document width (px)").0;
        let a4 = named(&initial, "A4 · 2480 × 3508 px · 300 dpi").0;
        tab_to(&ctx, "Print", |events| frame(&ctx, &mut studio, events));
        frame(&ctx, &mut studio, key(Key::Enter, Modifiers::NONE));
        let output = frame(&ctx, &mut studio, vec![]);
        assert_eq!(studio.new_doc_group, "Print");
        assert_eq!(named(&output, "Document width (px)").0, width);
        assert_eq!(named(&output, "A4 · 2480 × 3508 px · 300 dpi").0, a4);
        let output = tab_to(&ctx, "Document width (px)", |events| frame(&ctx, &mut studio, events));
        focused(&ctx, &output, "Document width (px)");
        replace("384", |events| frame(&ctx, &mut studio, events));
        assert_eq!(studio.custom_w, 384.);
        studio.swap_id.push_str("-different-document");
        let output = frame(&ctx, &mut studio, vec![]);
        assert_ne!(named(&output, "Document width (px)").0, width);
    }

    #[test]
    fn welcome_keyboard_startup_controls_have_linked_names_and_reverse_focus() {
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        let mut studio = Studio::new();
        let initial = studio.startup_preferences.clone();
        let output = tab_to(&ctx, "Start screen tab", |events| render(&ctx, events, |ui| startup_preferences(ui, &mut studio)));
        focused(&ctx, &output, "Start screen tab");
        let output = tab(true, |events| render(&ctx, events, |ui| startup_preferences(ui, &mut studio)));
        focused(&ctx, &output, "Open at launch");
        assert!(!named(&output, "Open at launch").1.labelled_by().is_empty());
        render(&ctx, key(Key::Space, Modifiers::NONE), |ui| startup_preferences(ui, &mut studio));
        render(&ctx, key(Key::Escape, Modifiers::NONE), |ui| startup_preferences(ui, &mut studio));
        assert_eq!(studio.startup_preferences, initial);
    }
}
