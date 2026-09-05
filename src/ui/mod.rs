mod browsers;
mod canvas;
mod chrome;
mod deform;
mod guides;
mod icons;
mod jobs;
mod masking;
mod motion_presets;
mod photo;
mod retouch;
mod selection;
mod studios;
mod templates;
pub mod theme;
mod timeline;
mod welcome;

use crate::app::Studio;
use crate::tools::Persona;
use eframe::egui::Ui;

pub fn run(ui: &mut Ui, studio: &mut Studio) {
    let ctx = ui.ctx().clone();
    ctx.options_mut(|o| o.zoom_with_keyboard = false);
    if (ctx.zoom_factor() - 1.0).abs() > 1e-3 {
        ctx.set_zoom_factor(1.0);
    }
    guides::handle_shortcuts(&ctx, studio);
    studio.handle_shortcuts(&ctx);
    studio.tick_motion(&ctx);

    chrome::top_bar(ui, studio);

    if studio.show_welcome {
        welcome::show(ui, studio);
        chrome::status_bar(ui, studio);
        let files: Vec<_> = ctx.input(|i| i.raw.dropped_files.clone());
        for f in files {
            studio.ingest_dropped(f.path(), None);
        }
    } else if studio.persona == Persona::Photo {
        chrome::doc_tabs(ui, studio);
        chrome::left_toolbar(ui, studio);
        chrome::status_bar(ui, studio);
        photo::show(ui, studio);
    } else {
        chrome::doc_tabs(ui, studio);
        chrome::left_toolbar(ui, studio);
        studios::right_panel(ui, studio);
        chrome::status_bar(ui, studio);
        timeline::show(ui, studio);
        canvas::show(ui, studio);
    }

    browsers::show_shape_browser(ui, studio);
    browsers::show_asset_browser(ui, studio);
    templates::window(ui, studio);

    if studio.show_shortcuts {
        egui_shortcuts(ui, studio);
    }
    unsaved_dialog(ui, studio);
    studio.tick_swap(&ctx);
}

/// Screenshot scenes wait for their actual template previews, not a fixed sleep.
pub fn scene_ready(ctx: &eframe::egui::Context, studio: &Studio) -> bool {
    (!(studio.show_templates
        || (studio.show_welcome && studio.welcome_page == crate::app::WelcomePage::Templates)))
        || templates::previews_ready(ctx)
}

fn unsaved_dialog(ui: &mut Ui, studio: &mut Studio) {
    if studio.pending_nav.is_none() {
        return;
    }
    let ctx = ui.ctx().clone();
    let title = match studio.pending_nav {
        Some(crate::app::PendingNav::CloseTab(i)) => {
            format!("Save changes to {}?", studio.tab_title(i).0)
        }
        _ => "Save your changes?".into(),
    };
    let dialog =
        eframe::egui::Modal::new(eframe::egui::Id::new("unsaved-changes")).show(&ctx, |ui| {
            ui.set_width(340.0);
            ui.heading(title);
            ui.add_space(6.0);
            ui.label("Your unsaved changes will be lost if you discard them.");
            ui.add_space(16.0);
            ui.horizontal(|ui| {
                if ui.button("Cancel").clicked() {
                    studio.pending_nav = None;
                }
                if ui.button("Discard").clicked() {
                    studio.execute_nav(&ctx, false);
                }
                if ui.button("Save").clicked() {
                    studio.execute_nav(&ctx, true);
                }
            });
        });
    if dialog.should_close() {
        studio.pending_nav = None;
    }
}

fn egui_shortcuts(ui: &mut Ui, studio: &mut Studio) {
    let height = (ui.ctx().viewport_rect().height() - 160.0).max(200.0);
    eframe::egui::Window::new("Keys")
        .collapsible(false)
        .resizable(true)
        .default_size([640.0, 560.0])
        .anchor(eframe::egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ui.ctx(), |ui| {
            eframe::egui::ScrollArea::vertical()
                .max_height(height)
                .show(ui, |ui| {
                    let groups = crate::tools::shortcut_groups();
                    ui.columns(2, |cols| {
                        let mid = groups.len().div_ceil(2);
                        for (col, slice) in [(0, &groups[..mid]), (1, &groups[mid..])] {
                            for (group, rows) in slice.iter() {
                                cols[col].add_space(6.0);
                                cols[col].label(
                                    eframe::egui::RichText::new(*group)
                                        .strong()
                                        .color(crate::ui::theme::accent()),
                                );
                                eframe::egui::Grid::new(*group)
                                    .num_columns(2)
                                    .spacing([16.0, 3.0])
                                    .show(&mut cols[col], |ui| {
                                        for row in *rows {
                                            ui.label(row.action);
                                            ui.label(
                                                eframe::egui::RichText::new(row.keys)
                                                    .monospace()
                                                    .small()
                                                    .color(crate::ui::theme::fg_weak()),
                                            );
                                            ui.end_row();
                                        }
                                    });
                            }
                        }
                    });
                });
            ui.add_space(8.0);
            if ui.button("Close").clicked() {
                studio.show_shortcuts = false;
            }
        });
}
