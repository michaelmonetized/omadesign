use crate::app::{Studio, WelcomePage};
use crate::presets;
use crate::ui::icons::{self, ph};
use crate::ui::theme::{accent, accent_soft, bg_panel, bg_widget_hover, bg_window, fg, fg_weak};
use eframe::egui::{
    Align, Align2, Button, Frame, Id, Layout, Margin, RichText, ScrollArea, Sense, Stroke, Ui, vec2,
};
use std::{path::PathBuf, sync::Arc};

const GROUPS: &[(&str, Option<&str>)] = &[
    ("All", None),
    ("Web", Some("Screen")),
    ("Print", Some("Print")),
    ("Social", Some("Social")),
    ("Photo", Some("Photo")),
    ("Identity", Some("Identity")),
];

#[derive(Clone)]
struct FileEntry {
    path: PathBuf,
    name: String,
    location: String,
    detail: String,
}

fn clear_file_cache(ui: &Ui) {
    crate::ui::jobs::cancel::<Arc<[FileEntry]>>(ui.ctx(), "welcome-recent-files");
    crate::ui::jobs::cancel::<Arc<[FileEntry]>>(ui.ctx(), "welcome-recovered-files");
    ui.data_mut(|data| {
        data.remove::<Arc<[FileEntry]>>(Id::new("welcome-recent-files"));
        data.remove::<Arc<[FileEntry]>>(Id::new("welcome-recovered-files"));
    });
}

pub fn show(ui: &mut Ui, studio: &mut Studio) {
    let frame = ui.ctx().cumulative_frame_nr();
    // File lists refresh on entry and on demand, never while moving the pointer.
    let previous = ui.data(|data| data.get_temp::<u64>(Id::new("welcome-previous-frame")));
    if previous.is_none_or(|previous| frame.saturating_sub(previous) > 1) {
        clear_file_cache(ui);
    }
    ui.data_mut(|data| data.insert_temp(Id::new("welcome-previous-frame"), frame));

    let full = ui.available_rect_before_wrap();
    ui.painter().rect_filled(full, 0.0, bg_window());
    ScrollArea::vertical()
        .id_salt("welcome-page")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            let width = (ui.available_width() - 48.0).clamp(240.0, 920.0);
            ui.add_space(if studio.welcome_page == WelcomePage::Templates {
                18.0
            } else {
                (full.height() * 0.065).clamp(20.0, 64.0)
            });
            ui.with_layout(Layout::top_down(Align::Center), |ui| {
                ui.allocate_ui_with_layout(vec2(width, 0.0), Layout::top_down(Align::Min), |ui| {
                    ui.set_max_width(width);
                    if studio.welcome_page != WelcomePage::Templates {
                        ui.horizontal(|ui| {
                            ui.vertical(|ui| {
                                ui.label(
                                    RichText::new("Make something.")
                                        .size(30.0)
                                        .strong()
                                        .color(fg()),
                                );
                                ui.add_space(4.0);
                                ui.label(
                                    RichText::new("A fresh canvas, or right where you left off.")
                                        .size(13.0)
                                        .color(fg_weak()),
                                );
                            });
                            if width >= 640.0 {
                                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                    if ui
                                        .add_sized(
                                            vec2(130.0, 34.0),
                                            Button::new("Open document").fill(accent_soft()),
                                        )
                                        .on_hover_text("Ctrl+O")
                                        .clicked()
                                    {
                                        studio.open();
                                    }
                                });
                            }
                        });
                        if width < 640.0
                            && ui.button("Open document").on_hover_text("Ctrl+O").clicked()
                        {
                            studio.open();
                        }
                        ui.add_space(24.0);
                    }
                    ui.horizontal(|ui| {
                        for (page, label) in [
                            (WelcomePage::New, "New document"),
                            (WelcomePage::Templates, "Templates · 52"),
                            (WelcomePage::Recents, "Recent"),
                            (WelcomePage::Recovered, "Recovered"),
                        ] {
                            if ui
                                .selectable_label(studio.welcome_page == page, label)
                                .clicked()
                            {
                                if studio.welcome_page != page {
                                    clear_file_cache(ui);
                                }
                                studio.welcome_page = page;
                            }
                        }
                    });
                    ui.add_space(8.0);
                    ui.separator();
                    ui.add_space(16.0);
                    match studio.welcome_page {
                        WelcomePage::New => new_page(ui, studio),
                        WelcomePage::Templates => super::templates::library(ui, studio),
                        WelcomePage::Recents => files_page(ui, studio, false),
                        WelcomePage::Recovered => files_page(ui, studio, true),
                    }
                    if studio.welcome_page != WelcomePage::Templates {
                        ui.add_space(22.0);
                        ui.horizontal_wrapped(|ui| {
                            ui.label(RichText::new("Explore").small().color(fg_weak()));
                            if ui.add(Button::new("Demo document").frame(false)).clicked() {
                                studio.seed_demo();
                            }
                            if ui.add(Button::new("Photo samples").frame(false)).clicked() {
                                studio.show_welcome = false;
                                studio.persona = crate::tools::Persona::Photo;
                                studio.photo.import_samples();
                            }
                        });
                    }
                    ui.add_space(24.0);
                });
            });
        });
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
    let grid_height = (rows as f32 * 76.0 + rows.saturating_sub(1) as f32 * gap).min(288.0);
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

fn relative_time(saved_at: u64) -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|time| time.as_secs())
        .unwrap_or(saved_at);
    let elapsed = now.saturating_sub(saved_at);
    match elapsed {
        0..60 => "Just now".into(),
        60..3600 => format!("{}m ago", elapsed / 60),
        3600..86400 => format!("{}h ago", elapsed / 3600),
        _ => format!("{}d ago", elapsed / 86400),
    }
}

fn load_files(recovered: bool) -> Arc<[FileEntry]> {
    if recovered {
        crate::project::list_swaps()
            .into_iter()
            .map(|(path, meta)| FileEntry {
                path,
                name: meta.name,
                location: meta
                    .original
                    .map(|path| path.display().to_string())
                    .unwrap_or_else(|| "Unsaved document".into()),
                detail: relative_time(meta.saved_at),
            })
            .collect()
    } else {
        crate::project::load_recents_all()
            .into_iter()
            .map(|path| {
                let detail = std::fs::metadata(&path)
                    .and_then(|meta| meta.modified())
                    .ok()
                    .and_then(|modified| modified.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|time| relative_time(time.as_secs()))
                    .unwrap_or_else(|| "Unavailable".into());
                FileEntry {
                    name: path
                        .file_name()
                        .map(|name| name.to_string_lossy().into_owned())
                        .unwrap_or_else(|| path.display().to_string()),
                    location: path
                        .parent()
                        .map(|parent| parent.display().to_string())
                        .unwrap_or_default(),
                    path,
                    detail,
                }
            })
            .collect()
    }
}

fn files_page(ui: &mut Ui, studio: &mut Studio, recovered: bool) {
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(if recovered {
                "Pick up an unsaved document."
            } else {
                "Your latest documents."
            })
            .color(fg_weak()),
        );
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if ui.add(Button::new("Refresh").frame(false)).clicked() {
                clear_file_cache(ui);
            }
        });
    });
    ui.add_space(12.0);
    let name = if recovered {
        "welcome-recovered-files"
    } else {
        "welcome-recent-files"
    };
    let key = Id::new(name);
    if let Some(Ok(files)) = crate::ui::jobs::poll::<Arc<[FileEntry]>>(ui.ctx(), name) {
        ui.data_mut(|data| data.insert_temp(key, files));
    }
    let Some(files) = ui.data(|data| data.get_temp::<Arc<[FileEntry]>>(key)) else {
        if !crate::ui::jobs::is_running::<Arc<[FileEntry]>>(ui.ctx(), name) {
            crate::ui::jobs::start(ui.ctx(), name, move || Ok(load_files(recovered)));
        }
        ui.label(RichText::new("Loading documents…").color(fg_weak()));
        return;
    };
    if files.is_empty() {
        ui.add_space(28.0);
        ui.label(
            RichText::new(if recovered {
                "You're all caught up."
            } else {
                "Your next project starts here."
            })
            .size(17.0)
            .color(fg()),
        );
        ui.label(
            RichText::new(if recovered {
                "No unsaved documents to recover."
            } else {
                "Open a document or create a canvas to get started."
            })
            .color(fg_weak()),
        );
        ui.add_space(36.0);
        return;
    }
    let mut open = None;
    let mut remove = None;
    for entry in files.iter() {
        Frame::new()
            .inner_margin(Margin::symmetric(8, 10))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(ph::FOLDER_OPEN)
                            .font(icons::font(22.0))
                            .color(fg_weak()),
                    );
                    let text_width = (ui.available_width() - 136.0).max(80.0);
                    ui.allocate_ui_with_layout(
                        vec2(text_width, 42.0),
                        Layout::top_down(Align::Min),
                        |ui| {
                            if ui
                                .add(
                                    Button::new(RichText::new(&entry.name).color(fg()))
                                        .frame(false),
                                )
                                .clicked()
                            {
                                open = Some(entry.path.clone());
                            }
                            ui.add(
                                eframe::egui::Label::new(
                                    RichText::new(&entry.location).small().color(fg_weak()),
                                )
                                .truncate(),
                            );
                        },
                    );
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if icons::tiny_icon(
                            ui,
                            ph::X,
                            if recovered {
                                "Delete recovery copy"
                            } else {
                                "Remove from recent documents"
                            },
                            false,
                        ) {
                            remove = Some(entry.path.clone());
                        }
                        ui.label(RichText::new(&entry.detail).small().color(fg_weak()));
                    });
                });
            });
        ui.separator();
    }
    if let Some(path) = open {
        if recovered {
            studio.recover_swap(path);
        } else {
            studio.open_path(path);
        }
    }
    if let Some(path) = remove {
        if recovered {
            studio.delete_swap_file(&path);
        } else {
            crate::project::remove_recent(&path);
            studio.recents = crate::project::load_recents();
        }
        clear_file_cache(ui);
    }
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
