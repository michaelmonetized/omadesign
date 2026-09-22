mod catalog;
mod team;
mod workspace;

use crate::app::{Studio, WelcomePage};
use crate::presets;
use crate::ui::theme::{accent, accent_soft, bg_panel, bg_widget_hover, fg, fg_weak};
use eframe::egui::{Align2, Button, Frame, Margin, RichText, ScrollArea, Sense, Stroke, Ui, vec2};

const GROUPS: &[(&str, Option<&str>)] = &[
    ("All", None),
    ("Web", Some("Screen")),
    ("Print", Some("Print")),
    ("Social", Some("Social")),
    ("Photo", Some("Photo")),
    ("Identity", Some("Identity")),
];

pub fn show(ui: &mut Ui, studio: &mut Studio) {
    workspace::show(ui, studio);
}
pub(super) fn ready(ctx: &eframe::egui::Context) -> bool {
    workspace::ready(ctx)
}
pub(super) fn modal_open(ctx: &eframe::egui::Context) -> bool {
    workspace::modal_open(ctx)
}
pub(super) fn cancel(ctx: &eframe::egui::Context) {
    catalog::cancel(ctx);
    team::suspend(ctx);
    workspace::leave(ctx);
}

pub(super) fn startup_preferences(ui: &mut Ui, studio: &mut Studio) {
    use crate::app::startup::{StartPage, StartTab};
    use crate::tools::Persona;
    let previous = studio.startup_preferences.clone();
    ui.label("Open at launch");
    let label = match studio.startup_preferences.start_tab {
        StartTab::Welcome => "Start screen",
        StartTab::Mode(mode) => mode.name(),
        StartTab::RememberLast => "Remember last mode",
    };
    eframe::egui::ComboBox::from_id_salt("startup-mode")
        .selected_text(label)
        .width(190.0)
        .show_ui(ui, |ui| {
            let choice = &mut studio.startup_preferences.start_tab;
            ui.selectable_value(choice, StartTab::Welcome, "Start screen");
            for persona in [
                Persona::Design,
                Persona::Pixel,
                Persona::Layout,
                Persona::Photo,
                Persona::Motion,
            ] {
                ui.selectable_value(choice, StartTab::Mode(persona), persona.name());
            }
            ui.selectable_value(choice, StartTab::RememberLast, "Remember last mode");
        });
    ui.label("Start screen tab");
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
    eframe::egui::ComboBox::from_id_salt("startup-page")
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
                ui.selectable_value(choice, StartPage::Page(page), page_name(page));
            }
            ui.selectable_value(choice, StartPage::RememberLast, "Remember last tab");
        });
    if studio.startup_preferences != previous {
        studio.save_startup_preferences();
    }
}

fn new_page(ui: &mut Ui, studio: &mut Studio) {
    ui.horizontal_wrapped(|ui| {
        for &(label, _) in GROUPS {
            if ui
                .selectable_label(studio.new_doc_group == label, label)
                .clicked()
            {
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
                        if preset_card(ui, preset, card_width) {
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
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new("Custom size").strong().size(12.0));
                ui.add_space(8.0);
                ui.add(
                    eframe::egui::DragValue::new(&mut studio.custom_w)
                        .range(32.0..=16000.0)
                        .prefix("W  ")
                        .speed(4.0),
                );
                ui.add(
                    eframe::egui::DragValue::new(&mut studio.custom_h)
                        .range(32.0..=16000.0)
                        .prefix("H  ")
                        .speed(4.0),
                );
                ui.add(
                    eframe::egui::DragValue::new(&mut studio.custom_dpi)
                        .range(36.0..=600.0)
                        .suffix(" dpi")
                        .speed(1.0),
                );
                if ui
                    .add(Button::new("Create document").fill(accent_soft()))
                    .clicked()
                {
                    studio.new_custom();
                }
            });
            ui.add_space(6.0);
            ui.horizontal_wrapped(|ui| {
                ui.checkbox(&mut studio.new_doc_transparent, "Transparent");
                ui.checkbox(&mut studio.new_doc_bleed, "Bleed");
                ui.checkbox(&mut studio.new_doc_safe, "Safe area");
                ui.add_space(6.0);
                ui.label(RichText::new("Artboards").small().color(fg_weak()));
                ui.add(
                    eframe::egui::DragValue::new(&mut studio.new_doc_artboards)
                        .range(1..=16)
                        .speed(0.1),
                );
            });
        });
}

fn preset_card(ui: &mut Ui, preset: presets::Preset, width: f32) -> bool {
    let (rect, response) = ui.allocate_exact_size(vec2(width, 76.0), Sense::click());
    response.widget_info(|| {
        eframe::egui::WidgetInfo::labeled(eframe::egui::WidgetType::Button, true, preset.name)
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
