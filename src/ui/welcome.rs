use crate::app::Studio;
use crate::presets;
use crate::ui::theme::{accent, bg_widget, border, fg_weak};
use eframe::egui::{Align, Align2, Frame, Layout, Margin, RichText, ScrollArea, Stroke, Ui, vec2};

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
    ui.vertical_centered(|ui| {
        ui.add_space(18.0);
        ui.label(RichText::new("omadesign").size(36.0).color(accent()).strong());
        ui.label(
            RichText::new("design  ·  paint  ·  photograph")
                .size(14.0)
                .color(fg_weak()),
        );
        ui.add_space(4.0);
        ui.label(
            RichText::new("A native Linux studio. Shortcuts match what you already know.")
                .size(13.0)
                .color(fg_weak()),
        );
    });
    ui.add_space(16.0);

    // Top actions row (outside the big box so it's always visible)
    ui.horizontal(|ui| {
        ui.add_space(ui.available_width() * 0.08);
        if ui
            .add_sized(vec2(160.0, 36.0), eframe::egui::Button::new("Open…  Ctrl+O"))
            .clicked()
        {
            studio.open();
        }
        if ui
            .add_sized(vec2(200.0, 36.0), eframe::egui::Button::new("Load demo document"))
            .clicked()
        {
            studio.seed_demo();
        }
        if ui
            .add_sized(vec2(180.0, 36.0), eframe::egui::Button::new("Photo samples"))
            .clicked()
        {
            studio.show_welcome = false;
            studio.persona = crate::tools::Persona::Photo;
            studio.photo.import_samples();
        }
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if ui.small_button("Shape Browser").clicked() {
                studio.show_shape_browser = true;
                studio.show_welcome = false;
            }
            if ui.small_button("Asset Browser").clicked() {
                studio.show_asset_browser = true;
                studio.show_welcome = false;
            }
        });
    });
    ui.add_space(10.0);

    // Big square box in the middle
    ui.horizontal(|ui| {
        // centre the frame
        let avail = ui.available_width();
        let box_w: f32 = 720.0;
        let pad = ((avail - box_w) * 0.5).max(8.0);
        ui.add_space(pad);
        Frame::new()
            .fill(bg_widget())
            .stroke(Stroke::new(1.0, border()))
            .corner_radius(16.0)
            .inner_margin(Margin::symmetric(18, 16))
            .show(ui, |ui| {
                ui.set_max_width(box_w - 36.0);
                ui.set_min_width(box_w - 36.0);
                ui.label(RichText::new("New document").strong().size(15.0));
                ui.add_space(10.0);

                // Tabs
                ui.horizontal(|ui| {
                    for tab in TAB_LABELS {
                        let on = studio.new_doc_group == *tab;
                        let btn = eframe::egui::Button::new(RichText::new(*tab).strong())
                            .selected(on);
                        if ui.add_sized(vec2(78.0, 28.0), btn).clicked() {
                            studio.new_doc_group = tab.to_string();
                        }
                    }
                });
                ui.add_space(8.0);
                ui.separator();
                ui.add_space(6.0);

                // Filtered presets
                let filter = tab_group(&studio.new_doc_group);
                let all: Vec<_> = presets::all()
                    .iter()
                    .filter(|p| filter.map(|g| p.group == g).unwrap_or(true))
                    .copied()
                    .collect();

                ScrollArea::vertical().max_height(320.0).show(ui, |ui| {
                    // Grid of larger preview cards
                    let cols = 3;
                    let card_w: f32 = 210.0;
                    let card_h: f32 = 112.0;
                    eframe::egui::Grid::new("preset-grid")
                        .num_columns(cols)
                        .spacing([12.0, 12.0])
                        .show(ui, |ui| {
                            for (idx, p) in all.iter().enumerate() {
                                let is_selected = false;
                                let _resp = ui.vertical(|ui| {
                                    // Preview box showing aspect ratio
                                    let max_preview_w: f32 = 180.0;
                                    let max_preview_h: f32 = 72.0;
                                    let asp = p.w / p.h.max(1.0);
                                    let (pw, ph) = if asp > 1.0 {
                                        (max_preview_w, max_preview_w / asp)
                                    } else {
                                        (max_preview_h * asp, max_preview_h)
                                    };
                                    let (rect, _) = ui.allocate_exact_size(vec2(card_w, card_h), eframe::egui::Sense::click());
                                    // Card background
                                    ui.painter().rect_filled(rect, 8.0, bg_widget());
                                    ui.painter().rect_stroke(rect, 8.0, Stroke::new(1.0, border()), eframe::egui::StrokeKind::Inside);
                                    // Preview rect centred
                                    let preview_rect = eframe::egui::Rect::from_center_size(rect.center(), vec2(pw, ph));
                                    // Fill with accent soft
                                    ui.painter().rect_filled(preview_rect, 4.0, accent().gamma_multiply(0.18));
                                    ui.painter().rect_stroke(preview_rect, 4.0, Stroke::new(1.0, accent()), eframe::egui::StrokeKind::Middle);
                                    // Labels
                                    let text_pos = rect.left_bottom() + vec2(8.0, -28.0);
                                    ui.painter().text(
                                        text_pos,
                                        Align2::LEFT_TOP,
                                        p.name,
                                        eframe::egui::FontId::proportional(12.0),
                                        crate::ui::theme::fg(),
                                    );
                                    let dim = format!("{:.0} × {:.0}  {} dpi", p.w, p.h, p.dpi as i32);
                                    ui.painter().text(
                                        text_pos + vec2(0.0, 14.0),
                                        Align2::LEFT_TOP,
                                        dim,
                                        eframe::egui::FontId::monospace(9.0),
                                        fg_weak(),
                                    );
                                });
                                // Click handling – the vertical's rect is not directly clickable, so we add a button overlay
                                // Instead we make the whole card a button via the allocated rect via custom sense – for MVP reuse the vertical's response via an invisible button
                                // Simpler: add a button below that triggers preset
                                let _ = is_selected;
                                // Add invisible click area via button
                                // We use a small button that covers the card – placed after
                                ui.allocate_ui_with_layout(vec2(0.0, 0.0), Layout::left_to_right(Align::Min), |_| {});
                                // Actually just add a button that is the card itself – we already have interaction via the vertical? We'll add a separate button
                                if ui
                                    .add_sized(vec2(card_w, 18.0), eframe::egui::Button::new("Create ▶").small())
                                    .clicked()
                                {
                                    studio.new_from_preset(*p);
                                }
                                if (idx + 1) % cols == 0 {
                                    ui.end_row();
                                }
                            }
                        });
                    if all.is_empty() {
                        ui.label(RichText::new("No presets in this group").small().color(fg_weak()));
                    }
                });

                ui.add_space(10.0);
                ui.separator();
                ui.add_space(6.0);
                // Options row
                ui.horizontal(|ui| {
                    ui.checkbox(&mut studio.new_doc_transparent, "Transparent bg");
                    ui.checkbox(&mut studio.new_doc_bleed, "Bleed marks");
                    ui.checkbox(&mut studio.new_doc_safe, "Safe area");
                });
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Artboards").small().color(fg_weak()));
                    ui.add(
                        eframe::egui::DragValue::new(&mut studio.new_doc_artboards)
                            .range(1..=16)
                            .speed(0.1)
                            .prefix("× "),
                    );
                    if studio.new_doc_artboards < 1 {
                        studio.new_doc_artboards = 1;
                    }
                    if studio.new_doc_artboards > 16 {
                        studio.new_doc_artboards = 16;
                    }
                    ui.label(
                        RichText::new(if studio.new_doc_artboards > 1 {
                            format!("→ {} artboards tiled horizontally (48px gutter)", studio.new_doc_artboards)
                        } else {
                            "single artboard".into()
                        })
                        .small()
                        .color(fg_weak()),
                    );
                });
                ui.add_space(4.0);
                ui.label(
                    RichText::new("Tip: transparent keeps the checker, bleed adds outer guides, safe area is inset by 36px. Artboards tile with a 48px gutter.")
                        .small()
                        .color(fg_weak()),
                );
            });
    });
    ui.add_space(10.0);
    ui.horizontal(|ui| {
        ui.add_space(ui.available_width() * 0.08);
        ui.label(
            RichText::new("F1 always shows the key list. Space pans. Ctrl+scroll zooms.")
                .small()
                .color(fg_weak()),
        );
        ui.with_layout(Layout::right_to_left(Align::Min), |ui| {
            if ui.small_button("Close welcome").clicked() {
                studio.show_welcome = false;
            }
        });
    });
}
