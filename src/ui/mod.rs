mod agent;
mod browsers;
mod canvas;
mod chrome;
mod cloud;
mod color_picker;
mod deform;
mod guides;
mod icons;
mod jobs;
mod key_hud;
#[cfg(test)]
mod key_hud_tests;
mod layer_drag;
mod layout;
mod layout_preview;
mod library;
mod masking;
mod motion_presets;
pub(crate) mod photo;
mod photo_detail;
mod preferences;
mod raster;
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

/// Open a raster preview with supplied settings; edits still require Apply.
pub fn preview_raster_filter(
    ctx: &eframe::egui::Context,
    studio: &mut Studio,
    settings: crate::raster::Settings,
) {
    raster::open_with_settings(ctx, studio, settings);
}

pub fn preview_raster_chroma(ctx: &eframe::egui::Context, studio: &mut Studio) {
    raster::open(ctx, studio, crate::raster::Kind::ChromaKey);
}

pub fn present_layout(ctx: &eframe::egui::Context, studio: &mut Studio) {
    layout_preview::start(ctx, studio);
}

/// Point an isolated native capture at its real fixture files without changing
/// HOME or the application's normal settings, typography, and recovery roots.
pub fn set_capture_catalog_root(ctx: &eframe::egui::Context, root: &std::path::Path) {
    welcome::set_capture_catalog_root(ctx, root);
}

pub fn run(ui: &mut Ui, studio: &mut Studio) {
    let ctx = ui.ctx().clone();
    if studio.updates.freezing {
        studio.poll_updates(&ctx, false);
        preferences::show(&ctx, studio);
        return;
    }
    if studio.file_dialog_pending() || studio.updates.freezing {
        ui.disable();
    }
    ctx.options_mut(|o| o.zoom_with_keyboard = false);
    let zoom = studio.startup_preferences.ui_font_size.clamp(10, 24) as f32 / 13.0;
    if (ctx.zoom_factor() - zoom).abs() > 1e-3 {
        ctx.set_zoom_factor(zoom);
    }
    // Give cancellable library loads Escape before canvas shortcuts consume it.
    library::tick(&ctx, studio);
    if !studio.file_dialog_pending()
        && !studio.show_preferences
        && !studio.updates.freezing
        && !agent::is_open(&ctx)
        && !welcome::modal_open(&ctx)
        && !studio.show_templates
    {
        guides::handle_shortcuts(&ctx, studio);
    }
    if !studio.file_dialog_pending()
        && !studio.show_preferences
        && !studio.updates.freezing
        && !layout_preview::is_open(&ctx)
        && !raster::is_open(&ctx)
        && !agent::is_open(&ctx)
        && !welcome::modal_open(&ctx)
        && !studio.show_templates
    {
        studio.handle_shortcuts(&ctx);
    }
    studio.tick_motion(&ctx);
    layout::poll_image(&ctx, studio);

    if !studio.show_welcome {
        chrome::top_bar(ui, studio);
        welcome::cancel(&ctx);
    }
    key_hud::show(ui, studio);

    if studio.show_welcome {
        if studio.tab_count() > 1 {
            chrome::doc_tabs(ui, studio);
        }
        chrome::status_bar(ui, studio);
        welcome::show(ui, studio);
        let files: Vec<_> = ctx.input(|i| i.raw.dropped_files.clone());
        for f in files {
            studio.ingest_dropped(f.path(), None);
        }
    } else if studio.persona == Persona::Photo {
        chrome::doc_tabs(ui, studio);
        chrome::status_bar(ui, studio);
        photo::show(ui, studio);
    } else {
        chrome::doc_tabs(ui, studio);
        layout::hierarchy(ui, studio);
        chrome::left_toolbar(ui, studio);
        studios::right_panel(ui, studio);
        chrome::status_bar(ui, studio);
        timeline::show(ui, studio);
        canvas::show(ui, studio);
    }

    if !studio.file_dialog_pending() {
        preferences::show(&ctx, studio);
        agent::show(&ctx, studio);
        browsers::show_shape_browser(ui, studio);
        browsers::show_asset_browser(ui, studio);
        templates::window(ui, studio);
        cloud::modal(ui, studio);
        layout_preview::show(ui, studio);
        raster::show(ui, studio);

        if studio.show_shortcuts {
            egui_shortcuts(ui, studio);
        }
        unsaved_dialog(ui, studio);
    }
    if !studio.updates.freezing {
        studio.tick_swap(&ctx);
    }
    studio.poll_updates(&ctx, jobs::any_running(&ctx) || raster::is_open(&ctx));
    studio.remember_current_mode();
    crate::telemetry::activity(
        studio.persona,
        studio.tool,
        ctx.input(|i| {
            i.events.iter().any(|e| {
                matches!(
                    e,
                    eframe::egui::Event::PointerButton { pressed: true, .. }
                        | eframe::egui::Event::Key { pressed: true, .. }
                )
            })
        }),
    );
}

/// Screenshot scenes wait for their actual template previews, not a fixed sleep.
pub fn scene_ready(ctx: &eframe::egui::Context, studio: &Studio) -> bool {
    !studio.cloud_busy()
        && (!studio.show_welcome || welcome::ready(ctx))
        && (studio.show_welcome || chrome::document_previews_ready(ctx))
        && !studio.photo.is_loading_previews()
        && library::ready(ctx, studio)
        && raster::ready(ctx)
        && (studio.persona != Persona::Photo || photo_detail::ready(ctx))
        && (!(studio.show_templates
            || (studio.show_welcome && studio.welcome_page == crate::app::WelcomePage::Templates))
            || templates::previews_ready(ctx))
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
