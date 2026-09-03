use crate::app::{Studio, WelcomePage};
use crate::presets;
use crate::ui::theme::{accent, bg_panel, bg_widget, bg_widget_hover, bg_window, border, fg, fg_weak};
use eframe::egui::{
    vec2, Align, Align2, Frame, Layout, Margin, RichText, ScrollArea, Sense, Stroke, Ui, UiBuilder,
};

static TAB_LABELS: &[&str] = &["All", "Web", "Print", "Social", "Photo", "Identity"];

fn tab_group(label: &str) -> Option<&'static str> {
    match label {
        "Web" => Some("Screen"),
        "All" => None,
        "Print" => Some("Print"),
        "Social" => Some("Social"),
        "Photo" => Some("Photo"),
        "Identity" => Some("Identity"),
        _ => None,
    }
}

pub fn show(ui: &mut Ui, studio: &mut Studio) {
    let full = ui.available_rect_before_wrap();
    ui.painter().rect_filled(full, 0.0, bg_window());

    ui.scope_builder(
        UiBuilder::new()
            .max_rect(full)
            .layout(Layout::top_down(Align::Center)),
        |ui| {
            ui.add_space(20.0);
            ui.label(RichText::new("omadesign").size(32.0).color(accent()).strong());
            ui.label(
                RichText::new("design  ·  paint  ·  photograph  ·  motion")
                    .size(14.0)
                    .color(fg_weak()),
            );
            ui.label(
                RichText::new("A native Linux studio. Shortcuts match what you already know.")
                    .size(13.0)
                    .color(fg_weak()),
            );
            ui.add_space(14.0);

            ui.horizontal(|ui| {
                if ui.button("Open…  Ctrl+O").clicked() {
                    studio.open();
                }
                if ui.button("Demo").clicked() {
                    studio.seed_demo();
                }
                if ui.button("Photo samples").clicked() {
                    studio.show_welcome = false;
                    studio.persona = crate::tools::Persona::Photo;
                    studio.photo.import_samples();
                }
            });
            ui.add_space(16.0);

            let panel_w = (full.width() - 64.0).clamp(640.0, 1020.0);
            // Title block ~150, custom row ~90, frame chrome ~50.
            let panel_h = (full.height() - 220.0).clamp(400.0, 680.0);

            Frame::new()
                .fill(bg_panel())
                .stroke(Stroke::new(1.0, border()))
                .corner_radius(14.0)
                .inner_margin(Margin::symmetric(18, 16))
                .show(ui, |ui| {
                    ui.set_min_size(vec2(panel_w, panel_h));
                    ui.set_max_width(panel_w);
                    ui.vertical(|ui| {
                        ui.horizontal(|ui| {
                            for (page, label) in [
                                (WelcomePage::New, "New"),
                                (WelcomePage::Recents, "Recents"),
                                (WelcomePage::Recovered, "Recovered"),
                            ] {
                                let on = studio.welcome_page == page;
                                let btn = eframe::egui::Button::new(RichText::new(label).strong())
                                    .selected(on);
                                if ui.add_sized(vec2(96.0, 26.0), btn).clicked() {
                                    studio.welcome_page = page;
                                }
                            }
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                ui.label(
                                    RichText::new("F1 keys  ·  Space pans  ·  Ctrl+scroll zooms")
                                        .small()
                                        .color(fg_weak()),
                                );
                            });
                        });
                        ui.add_space(8.0);
                        if studio.welcome_page == WelcomePage::New {
                        ui.horizontal(|ui| {
                            for tab in TAB_LABELS {
                                let on = studio.new_doc_group == *tab;
                                let btn = eframe::egui::Button::new(RichText::new(*tab).strong())
                                    .selected(on);
                                if ui.add_sized(vec2(78.0, 26.0), btn).clicked() {
                                    studio.new_doc_group = tab.to_string();
                                }
                            }
                        });
                        ui.add_space(8.0);
                        ui.separator();
                        ui.add_space(8.0);

                        let filter = tab_group(&studio.new_doc_group);
                        let all: Vec<_> = presets::all()
                            .iter()
                            .filter(|p| filter.map(|g| p.group == g).unwrap_or(true))
                            .copied()
                            .collect();

                        let grid_h = (ui.available_height() - 92.0).max(240.0);
                        ScrollArea::vertical()
                            .id_salt("welcome-presets")
                            .max_height(grid_h)
                            .min_scrolled_height(grid_h)
                            .auto_shrink([false, false])
                            .show(ui, |ui| {
                                ui.set_min_width(panel_w - 12.0);
                                let cols = 3usize;
                                let gap = 10.0;
                                let inner = ui.available_width();
                                let card_w = ((inner - gap * (cols as f32 - 1.0)) / cols as f32)
                                    .clamp(180.0, 320.0);
                                eframe::egui::Grid::new("preset-grid")
                                    .num_columns(cols)
                                    .spacing([gap, gap])
                                    .min_col_width(card_w)
                                    .show(ui, |ui| {
                                        for (idx, p) in all.iter().enumerate() {
                                            if preset_card(ui, *p, card_w) {
                                                studio.new_from_preset(*p);
                                            }
                                            if (idx + 1) % cols == 0 {
                                                ui.end_row();
                                            }
                                        }
                                        if !all.is_empty() && all.len() % cols != 0 {
                                            ui.end_row();
                                        }
                                    });
                                if all.is_empty() {
                                    ui.label(
                                        RichText::new("No presets in this group")
                                            .small()
                                            .color(fg_weak()),
                                    );
                                }
                            });

                        ui.add_space(8.0);
                        ui.separator();
                        ui.add_space(6.0);
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("Custom").small().color(fg_weak()));
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
                                    .prefix("dpi  ")
                                    .speed(1.0),
                            );
                            if ui.button("Create").clicked() {
                                studio.new_custom();
                            }
                        });
                        ui.horizontal(|ui| {
                            ui.checkbox(&mut studio.new_doc_transparent, "Transparent");
                            ui.checkbox(&mut studio.new_doc_bleed, "Bleed");
                            ui.checkbox(&mut studio.new_doc_safe, "Safe area");
                            ui.label(RichText::new("Artboards").small().color(fg_weak()));
                            ui.add(
                                eframe::egui::DragValue::new(&mut studio.new_doc_artboards)
                                    .range(1..=16)
                                    .speed(0.1)
                                    .prefix("× "),
                            );
                        });
                        } else if studio.welcome_page == WelcomePage::Recents {
                            recents_page(ui, studio);
                        } else {
                            recovered_page(ui, studio);
                        }
                    });
                });
        },
    );
}

fn recents_page(ui: &mut Ui, studio: &mut Studio) {
    let recents = crate::project::load_recents_all();
    studio.recents = crate::project::load_recents();
    if recents.is_empty() {
        ui.label(
            RichText::new("No recent documents yet. Save a .oma and it shows up here.")
                .small()
                .color(fg_weak()),
        );
        return;
    }
    let mut open = None;
    let mut remove = None;
    ScrollArea::vertical()
        .id_salt("welcome-recents")
        .max_height(ui.available_height())
        .show(ui, |ui| {
            for p in &recents {
                ui.horizontal(|ui| {
                    let name = p
                        .file_name()
                        .map(|s| s.to_string_lossy().into_owned())
                        .unwrap_or_else(|| p.display().to_string());
                    let meta = std::fs::metadata(p)
                        .and_then(|m| m.modified())
                        .ok()
                        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                        .map(|d| {
                            let secs = d.as_secs();
                            format!("{secs}")
                        })
                        .unwrap_or_else(|| "missing".into());
                    if ui.button(&name).on_hover_text(p.display().to_string()).clicked() {
                        open = Some(p.clone());
                    }
                    ui.label(RichText::new(p.display().to_string()).small().color(fg_weak()));
                    ui.label(RichText::new(meta).small().color(fg_weak()));
                    if ui.small_button("×").clicked() {
                        remove = Some(p.clone());
                    }
                });
            }
        });
    if let Some(p) = open {
        studio.open_path(p);
    }
    if let Some(p) = remove {
        crate::project::remove_recent(&p);
        studio.recents = crate::project::load_recents();
    }
}

fn recovered_page(ui: &mut Ui, studio: &mut Studio) {
    let swaps = crate::project::list_swaps();
    if swaps.is_empty() {
        ui.label(
            RichText::new("Nothing to recover. Crash swaps land in ~/.local/share/omadesign.")
                .small()
                .color(fg_weak()),
        );
        return;
    }
    let mut open = None;
    let mut drop = None;
    ScrollArea::vertical()
        .id_salt("welcome-recovered")
        .max_height(ui.available_height())
        .show(ui, |ui| {
            for (path, meta) in &swaps {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(&meta.name).strong());
                    if let Some(orig) = &meta.original {
                        ui.label(
                            RichText::new(orig.display().to_string())
                                .small()
                                .color(fg_weak()),
                        );
                    }
                    ui.label(
                        RichText::new(format!("saved {}", meta.saved_at))
                            .small()
                            .color(fg_weak()),
                    );
                    if ui.button("Open").clicked() {
                        open = Some(path.clone());
                    }
                    if ui.small_button("Delete").clicked() {
                        drop = Some(path.clone());
                    }
                });
            }
        });
    if let Some(p) = open {
        studio.recover_swap(p);
    }
    if let Some(p) = drop {
        studio.delete_swap_file(&p);
    }
}

fn preset_card(ui: &mut Ui, p: presets::Preset, card_w: f32) -> bool {
    let card_h = 100.0;
    let (rect, resp) = ui.allocate_exact_size(vec2(card_w, card_h), Sense::click());
    let hover = resp.hovered();
    ui.painter().rect_filled(
        rect,
        8.0,
        if hover { bg_widget_hover() } else { bg_widget() },
    );
    ui.painter().rect_stroke(
        rect,
        8.0,
        Stroke::new(1.0, if hover { accent() } else { border() }),
        eframe::egui::StrokeKind::Inside,
    );

    let max_preview_w: f32 = card_w - 28.0;
    let max_preview_h: f32 = 46.0;
    let asp = p.w / p.h.max(1.0);
    let (pw, ph) = if asp >= 1.0 {
        let w = max_preview_w.min(max_preview_h * asp);
        (w, w / asp)
    } else {
        let h = max_preview_h;
        (h * asp, h)
    };
    let preview =
        eframe::egui::Rect::from_center_size(rect.center() + vec2(0.0, -14.0), vec2(pw, ph));
    ui.painter()
        .rect_filled(preview, 3.0, accent().gamma_multiply(0.22));
    ui.painter().rect_stroke(
        preview,
        3.0,
        Stroke::new(1.0, accent()),
        eframe::egui::StrokeKind::Middle,
    );
    ui.painter().text(
        rect.left_bottom() + vec2(10.0, -28.0),
        Align2::LEFT_TOP,
        p.name,
        eframe::egui::FontId::proportional(12.0),
        fg(),
    );
    ui.painter().text(
        rect.left_bottom() + vec2(10.0, -13.0),
        Align2::LEFT_TOP,
        format!("{:.0} × {:.0}  {} dpi", p.w, p.h, p.dpi as i32),
        eframe::egui::FontId::monospace(9.0),
        fg_weak(),
    );
    resp.clicked()
}
