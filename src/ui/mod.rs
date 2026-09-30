mod artboard_dialog;
mod agent;
mod agent_attachments;
mod agent_picker;
pub(crate) mod anim_export;
mod browsers;
mod background_removal;
mod canvas;
mod chrome;
mod credits;
mod cloud;
mod color_picker;
mod deform;
pub(crate) mod export_dialog;
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
mod pixel_selection;
mod pixel_edit;
mod plugins;
mod preferences;
mod paragraph;
mod character_spacing;
mod opentype;
mod raster;
mod retouch;
mod selection;
mod studios;
mod text_geometry;
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

pub fn preview_background_removal(ctx: &eframe::egui::Context, studio: &mut Studio) {
    background_removal::open(ctx, studio);
}

/// Save an open still-export preview to a destination supplied by the native host.
/// Uses the same source validation and background job as the file chooser.
pub fn save_export_preview(ctx: &eframe::egui::Context, studio: &Studio, path: std::path::PathBuf) -> Result<(), String> {
    export_dialog::save_to(ctx, studio, path)
}

pub fn present_layout(ctx: &eframe::egui::Context, studio: &mut Studio) {
    layout_preview::start(ctx, studio);
}

/// Point an isolated native capture at its real fixture files without changing
/// HOME or the application's normal settings, typography, and recovery roots.
pub fn show_plugin_manager(ctx: &eframe::egui::Context) {
    plugins::open(ctx);
}

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
    plugins::tick(&ctx, studio);
    canvas::poll_screen_pick(&ctx, studio);
    theme::poll(&ctx);
    ctx.data_mut(|data| {
        data.insert_temp(
            eframe::egui::Id::new("oma-recent-colors"),
            studio.recent.clone(),
        )
    });
    studio.poll_pixel_edit(&ctx);
    agent::tick(&ctx, studio);
    if !studio.file_dialog_pending()
        && !studio.show_preferences
        && !studio.updates.freezing
        && !pixel_selection::is_open(&ctx)
        && !background_removal::is_open(&ctx)
        && !export_dialog::is_open(&ctx)
        && !plugins::is_open(&ctx)
        && !welcome::modal_open(&ctx)
        && !artboard_dialog::is_open(&ctx)
        && !studio.show_templates
    {
        guides::handle_shortcuts(&ctx, studio);
    }
    if !studio.file_dialog_pending()
        && !studio.show_preferences
        && !studio.updates.freezing
        && !layout_preview::is_open(&ctx)
        && !raster::is_open(&ctx)
        && !background_removal::is_open(&ctx)
        && !export_dialog::is_open(&ctx)
        && !pixel_selection::is_open(&ctx)
        && !plugins::is_open(&ctx)
        && !welcome::modal_open(&ctx)
        && !artboard_dialog::is_open(&ctx)
        && !studio.show_templates
    {
        studio.handle_shortcuts(&ctx);
    }
    studio.tick_motion(&ctx);
    layout::poll_image(&ctx, studio);

    // Keep the canvas visible for previews while preventing edits behind the dialog.
    if pixel_selection::is_open(&ctx) || background_removal::is_open(&ctx) || export_dialog::is_open(&ctx) {
        ui.disable();
    }

    chrome::top_bar(ui, studio);
    if !studio.show_welcome {
        welcome::cancel(&ctx);
    }
    key_hud::show(ui, studio);
    agent::panel(ui, studio);

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
        if !studio.agent.visible {
            studios::right_panel(ui, studio);
        }
        chrome::status_bar(ui, studio);
        timeline::show(ui, studio);
        canvas::show(ui, studio);
    }

    if !studio.file_dialog_pending() {
        preferences::show(&ctx, studio);
        plugins::show(&ctx, studio);
        browsers::show_shape_browser(ui, studio);
        browsers::show_asset_browser(ui, studio);
        templates::window(ui, studio);
        cloud::modal(ui, studio);
        layout_preview::show(ui, studio);
        raster::show(ui, studio);
        background_removal::show(&ctx, studio);
        export_dialog::show(&ctx, studio);
        artboard_dialog::show(&ctx, studio);
        anim_export::show(ui, studio);
        pixel_selection::show(&ctx, studio);

        if studio.show_shortcuts {
            egui_shortcuts(ui, studio);
        }
        unsaved_dialog(ui, studio);
        svg_export_dialog(ui, studio);
    }
    if !studio.updates.freezing {
        studio.tick_swap(&ctx);
    }
    studio.poll_updates(&ctx, jobs::any_running(&ctx) || raster::is_open(&ctx) || background_removal::is_open(&ctx) || export_dialog::is_open(&ctx));
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
    if studio.pixel_edit.as_ref().is_some_and(|e|e.pending()||e.job.is_some()) {return false;}
    !studio.cloud_busy()
        && (!studio.show_welcome || welcome::ready(ctx))
        && (studio.show_welcome || chrome::document_previews_ready(ctx))
        && !studio.photo.is_loading_previews()
        && library::ready(ctx, studio)
        && raster::ready(ctx)
        && background_removal::ready(ctx)
        && !export_dialog::busy(ctx)
        && pixel_selection::ready(ctx)
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

fn svg_export_dialog(ui: &mut Ui, studio: &mut Studio) {
    if studio.animated_svg_warnings().is_empty() {
        return;
    }
    let warnings = studio.animated_svg_warnings().to_vec();
    let ctx = ui.ctx().clone();
    let dialog =
        eframe::egui::Modal::new(eframe::egui::Id::new("svg-export-warnings")).show(&ctx, |ui| {
            ui.set_width(420.0);
            ui.heading("SVG can't carry these");
            ui.add_space(6.0);
            eframe::egui::ScrollArea::vertical()
                .max_height(240.0)
                .show(ui, |ui| {
                    for warning in &warnings {
                        ui.label(warning);
                    }
                });
            ui.add_space(12.0);
            ui.label("Continue writes the file anyway. Cancel leaves it unwritten.");
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                if ui.button("Cancel").clicked() {
                    studio.resolve_animated_svg(false);
                }
                if ui.button("Continue").clicked() {
                    studio.resolve_animated_svg(true);
                }
            });
        });
    if dialog.should_close() {
        studio.resolve_animated_svg(false);
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
