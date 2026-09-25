#[path = "document_previews.rs"]
mod document_previews;

use crate::app::Studio;
use crate::geom::Pt;
use crate::tools::{Persona, Tool};
use crate::ui::icons::{self, ph};
use crate::ui::theme::{accent, bg_panel, bg_window, border, fg, fg_weak};
use eframe::egui::{
    Align, Button, Color32, Frame, Layout, Margin, Panel, RichText, ScrollArea, Ui, vec2,
};

pub fn top_bar(ui: &mut Ui, studio: &mut Studio) {
    Panel::top("top")
        .exact_size(44.0)
        .frame(
            Frame::new()
                .fill(bg_panel())
                .inner_margin(Margin::symmetric(12, 6)),
        )
        .show(ui, |ui| {
            let row = ui.max_rect();
            let modes = mode_tabs_rect(row);
            let compact = row.width() < 1120.0;
            let left = eframe::egui::Rect::from_min_max(
                row.min,
                eframe::egui::pos2(modes.left() - 12.0, row.bottom()),
            );
            ui.scope_builder(eframe::egui::UiBuilder::new().max_rect(left), |ui| {
                ui.horizontal_centered(|ui| {
                    if let Some(mark) = icons::svg_texture(
                        ui,
                        "title-wordmark",
                        include_bytes!("../../assets/omadesign-wordmark.svg"),
                    ) {
                        let size = mark.size_vec2() * (120. / mark.size_vec2().x);
                        if ui
                            .add(
                                Button::image(eframe::egui::Image::new((mark.id(), size)))
                                    .frame(false),
                            )
                            .on_hover_text("Config · Update · About · Docs")
                            .clicked()
                        {
                            studio.show_preferences = true;
                        }
                    }
                    ui.add_space(10.0);
                    ui.visuals_mut().widgets.inactive.weak_bg_fill = Color32::TRANSPARENT;
                    if compact {
                        ui.menu_button("Menu", |ui| main_menus(ui, studio));
                    } else {
                        main_menus(ui, studio);
                    }
                });
            });
            // Center against the entire title bar, independently of menu widths.
            ui.scope_builder(eframe::egui::UiBuilder::new().max_rect(modes), |ui| {
                persona_picker(ui, studio);
            });
            if !studio.show_welcome {
                let right = eframe::egui::Rect::from_min_max(
                    eframe::egui::pos2(modes.right() + 12.0, row.top()),
                    row.max,
                );
                ui.scope_builder(eframe::egui::UiBuilder::new().max_rect(right), |ui| {
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let photo = studio.persona == Persona::Photo;
                        if icons::icon_button(
                            ui,
                            ph::FRAME_CORNERS,
                            if photo {
                                "Fit photo  Ctrl+0"
                            } else {
                                "Fit artboard  Ctrl+0"
                            },
                            false,
                        ) {
                            if photo {
                                studio.photo.view_scale = 1.0;
                                studio.photo.view_offset = eframe::egui::Vec2::ZERO;
                            } else {
                                studio.need_fit = true;
                            }
                        }
                        ui.add_space(8.0);
                        if photo {
                            library_buttons(ui, studio);
                        }
                        if !compact && photo && ui.button("Place in Design").clicked() {
                            studio.send_photo_to_design();
                        }
                    });
                });
            }
        });
}

fn zoom_corner(ui: &mut Ui, studio: &mut Studio) {
    let photo = studio.persona == Persona::Photo;
    let actual = if photo {
        studio.photo.view_scale * studio.photo.fit_scale
    } else {
        studio.view.scale
    };
    if ui
        .small_button("100%")
        .on_hover_text("Zoom to 100% at the center")
        .clicked()
    {
        if photo {
            studio.photo.view_scale = 1.0 / studio.photo.fit_scale.max(0.001);
            studio.photo.view_offset = eframe::egui::Vec2::ZERO;
        } else {
            let anchor = studio
                .canvas_rect
                .map(|rect| {
                    let center = rect.center();
                    Pt::new(center.x, center.y)
                })
                .unwrap_or(Pt::ZERO);
            studio.zoom_by(1.0 / studio.view.scale.max(0.001), anchor);
        }
    }
    ui.label(
        RichText::new(format!("{:.0}%", actual * 100.0))
            .small()
            .color(fg()),
    );
}

fn main_menus(ui: &mut Ui, studio: &mut Studio) {
    file_menu(ui, studio);
    edit_menu(ui, studio);
    ui.menu_button("Select", |ui| super::selection::menu(ui, studio));
    ui.add_enabled_ui(studio.persona != Persona::Photo, |ui| {
        object_menu(ui, studio);
        arrange_menu(ui, studio);
    });
    view_menu(ui, studio);
    super::plugins::menu(ui, studio);
}

fn mode_tabs_rect(row: eframe::egui::Rect) -> eframe::egui::Rect {
    eframe::egui::Rect::from_center_size(row.center(), vec2(5.0 * 36.0 + 4.0 * 6.0, 30.0))
}

fn file_menu(ui: &mut Ui, studio: &mut Studio) {
    ui.menu_button("File", |ui| {
        if ui.button("Template library…").clicked() {
            studio.show_templates = true;
            ui.close();
        }
        if ui
            .add(Button::new("New…").shortcut_text("Ctrl+N"))
            .clicked()
        {
            studio.new_tab_welcome();
            ui.close();
        }
        if ui
            .add(Button::new("Open…").shortcut_text("Ctrl+O"))
            .clicked()
        {
            studio.open();
            ui.close();
        }
        ui.menu_button("Open recent", |ui| {
            if studio.recents.is_empty() {
                ui.label(RichText::new("No recent files").small().color(fg_weak()));
            }
            let recents = studio.recents.clone();
            for p in recents {
                let name = p
                    .file_name()
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_else(|| p.display().to_string());
                if ui
                    .button(&name)
                    .on_hover_text(p.display().to_string())
                    .clicked()
                {
                    studio.open_path(p);
                    ui.close();
                }
            }
        });
        ui.separator();
        if ui
            .add(
                Button::new(if studio.persona == Persona::Photo {
                    "Save selected photo settings"
                } else {
                    "Save"
                })
                .shortcut_text("Ctrl+S"),
            )
            .clicked()
        {
            studio.save();
            ui.close();
        }
        if ui
            .add_enabled(
                studio.persona != Persona::Photo,
                Button::new("Save as…").shortcut_text("Ctrl+Shift+S"),
            )
            .clicked()
        {
            studio.save_as();
            ui.close();
        }
        ui.separator();
        if ui
            .add(Button::new("Place…").shortcut_text("Ctrl+Shift+P"))
            .clicked()
        {
            studio.begin_place();
            ui.close();
        }
        ui.separator();
        ui.label(RichText::new("Export").small().color(fg_weak()));
        if studio.persona == Persona::Photo {
            ui.add_enabled_ui(!super::photo::is_exporting(ui.ctx()), |ui| {
                for (label, extension) in [
                    ("Export PNG…  Ctrl+E", "png"),
                    ("Export TIFF…", "tif"),
                    ("Export JPEG…", "jpg"),
                ] {
                    if ui.button(label).clicked() {
                        super::photo::export_developed(ui.ctx(), studio, extension);
                        ui.close();
                    }
                }
            });
            return;
        }
        ui.horizontal(|ui| {
            ui.label(RichText::new("Scale").small().color(fg_weak()));
            for s in [1u32, 2, 3] {
                if ui
                    .selectable_label(studio.export_scale == s, format!("{s}×"))
                    .clicked()
                {
                    studio.export_scale = s;
                }
            }
        });
        if ui
            .add(Button::new("Export PNG…").shortcut_text("Ctrl+E"))
            .clicked()
        {
            studio.export_png();
            ui.close();
        }
        if ui.button("Export JPEG…").clicked() {
            studio.export_jpeg();
            ui.close();
        }
        if ui.button("Export SVG…").clicked() {
            studio.export_svg();
            ui.close();
        }
        for (label, extension) in [
            ("Export Photoshop PSD…", "psd"),
            ("Export large Photoshop PSB…", "psb"),
            ("Export layered PDF…", "pdf"),
            ("Export OpenRaster…", "ora"),
        ] {
            if ui.button(label).clicked() {
                studio.export_layered(extension);
                ui.close();
            }
        }
        if ui.button("Export animated SVG…").clicked() {
            studio.export_animated_svg();
            ui.close();
        }
        if ui.button("Export Lottie…").clicked() {
            studio.export_lottie();
            ui.close();
        }
        ui.separator();
        ui.label(RichText::new("Layout frame").small().color(fg_weak()));
        if ui
            .add_enabled(
                studio.selected_frame().is_some(),
                Button::new("Export frame PNG…"),
            )
            .clicked()
        {
            studio.export_selected_frame_png();
            ui.close();
        }
        if ui
            .add_enabled(
                studio.selected_frame().is_some(),
                Button::new("Export frame SVG…"),
            )
            .clicked()
        {
            studio.export_selected_frame_svg();
            ui.close();
        }
        if ui
            .add_enabled(
                studio.selected_frame().is_some(),
                Button::new("Export frame HTML…"),
            )
            .clicked()
        {
            studio.export_selected_frame_html();
            ui.close();
        }
        ui.separator();
        ui.label(RichText::new("Cloud").small().color(fg_weak()));
        if crate::cloud::connected(&studio.cloud_identity) {
            let label = format!(
                "You're in · {}",
                crate::cloud::account_label(&studio.cloud_identity)
            );
            if ui.button(label).clicked() {
                studio.cloud_modal = crate::app::CloudModal::SignIn;
                ui.close();
            }
        } else if ui.button("Sign in…").clicked() {
            studio.cloud_modal = crate::app::CloudModal::SignIn;
            ui.close();
        }
        if ui.button("Push project + review export").clicked() {
            studio.enable_cloud_sync();
            ui.close();
        }
        if ui.button("Cloud projects…").clicked() {
            studio.show_cloud_projects();
            ui.close();
        }
        if ui.button("Review cloud annotations…").clicked() {
            studio.refresh_cloud_review();
            ui.close();
        }
        if ui.button("Upload project asset…").clicked() {
            studio.upload_cloud_asset();
            ui.close();
        }
        if ui.button("Invite collaborator…").clicked() {
            studio.cloud_modal = crate::app::CloudModal::Invite;
            ui.close();
        }
        if ui.button("Publish to showcase…").clicked() {
            studio.refresh_cloud_review();
            studio.cloud_modal = crate::app::CloudModal::Publish;
            ui.close();
        }
        ui.separator();
        if ui.button("Import Lottie…").clicked() {
            studio.import_lottie();
            ui.close();
        }
    });
}

fn edit_menu(ui: &mut Ui, studio: &mut Studio) {
    ui.menu_button("Edit", |ui| {
        if ui
            .add_enabled(
                if studio.persona == Persona::Photo {
                    studio.photo.can_undo()
                } else {
                    studio.history.can_undo()
                },
                Button::new("Undo").shortcut_text("Ctrl+Z"),
            )
            .clicked()
        {
            studio.undo();
            ui.close();
        }
        if ui
            .add_enabled(
                if studio.persona == Persona::Photo {
                    studio.photo.can_redo()
                } else {
                    studio.history.can_redo()
                },
                Button::new("Redo").shortcut_text("Ctrl+Shift+Z"),
            )
            .clicked()
        {
            studio.redo();
            ui.close();
        }
        ui.separator();
        if studio.persona == Persona::Photo {
            ui.add_enabled_ui(!studio.photo.is_batching(), |ui| {
                if ui
                    .add_enabled(
                        studio.photo.selected().is_some(),
                        Button::new("Copy adjustments").shortcut_text("Ctrl+Shift+C"),
                    )
                    .clicked()
                {
                    super::photo::copy_adjustments(studio);
                    ui.close();
                }
                if ui
                    .add_enabled(
                        studio.photo.copied_adjustments.is_some(),
                        Button::new("Paste adjustments…").shortcut_text("Ctrl+Shift+V"),
                    )
                    .clicked()
                {
                    super::photo::paste_adjustments(ui.ctx(), studio);
                    ui.close();
                }
                if ui.button("Photo presets…").clicked() {
                    super::photo::preset_library(ui.ctx(), studio);
                    ui.close();
                }
            });
            return;
        }
        if ui.add(Button::new("Cut").shortcut_text("Ctrl+X")).clicked() {
            studio.cut_selection(ui.ctx());
            ui.close();
        }
        if ui
            .add(Button::new("Copy").shortcut_text("Ctrl+C"))
            .clicked()
        {
            studio.copy_selection(ui.ctx());
            ui.close();
        }
        if ui
            .add(Button::new("Paste").shortcut_text("Ctrl+V"))
            .clicked()
        {
            studio.request_clipboard_paste(ui.ctx(), None);
            ui.close();
        }
        if ui
            .add(Button::new("Duplicate").shortcut_text("Super+D"))
            .clicked()
        {
            studio.duplicate_selection();
            ui.close();
        }
        if ui.button("Delete").clicked() {
            studio.delete_selection();
            ui.close();
        }
        ui.separator();
        if ui
            .add(Button::new("Copy style").shortcut_text("Ctrl+Alt+C"))
            .clicked()
        {
            studio.copy_style();
            ui.close();
        }
        if ui
            .add_enabled(
                studio.style_clip.is_some(),
                Button::new("Paste style").shortcut_text("Ctrl+Alt+V"),
            )
            .clicked()
        {
            studio.paste_style();
            ui.close();
        }
    });
}

fn object_menu(ui: &mut Ui, studio: &mut Studio) {
    ui.menu_button("Object", |ui| {
        if ui
            .add_enabled(
                !studio.selection.is_empty(),
                Button::new("Free transform").shortcut_text("Ctrl+T"),
            )
            .clicked()
        {
            studio.free_transform();
            ui.close();
        }
        ui.menu_button("Guides", |ui| {
            if ui
                .button(if studio.doc.ruler.guides_locked {
                    "Unlock all guides"
                } else {
                    "Lock all guides"
                })
                .clicked()
            {
                studio.set_guides_locked(!studio.doc.ruler.guides_locked);
                ui.close();
            }
            if ui.button("Clear all guides").clicked() {
                studio.clear_guides();
                ui.close();
            }
            ui.separator();
            if ui
                .add_enabled(
                    studio.can_convert_to_guides(),
                    Button::new("Convert selection to guides"),
                )
                .clicked()
            {
                studio.convert_selection_to_guides();
                ui.close();
            }
            if ui
                .add_enabled(
                    studio.can_release_guides(),
                    Button::new("Release guides to artwork"),
                )
                .clicked()
            {
                studio.release_selected_guides();
                ui.close();
            }
        });
        if ui
            .add_enabled(
                !studio.selection.is_empty(),
                Button::new("Expand stroke to outline"),
            )
            .clicked()
        {
            studio.expand_strokes();
            ui.close();
        }
        ui.menu_button("Pathfinder", |ui| {
            super::selection::pathfinder_menu(ui, studio)
        });
        ui.menu_button("Reshape", |ui| {
            for mode in crate::deform::Mode::ALL {
                if ui
                    .add_enabled(studio.can_deform(), Button::new(mode.label()))
                    .clicked()
                {
                    studio.begin_deform(mode);
                    ui.close();
                }
            }
        });
        ui.separator();
        if ui
            .add_enabled(
                !studio.selection.is_empty() || studio.selected_layer.is_some(),
                Button::new("Group").shortcut_text("Ctrl+G"),
            )
            .clicked()
        {
            studio.group_selected();
            ui.close();
        }
        if ui
            .add(Button::new("Ungroup").shortcut_text("Ctrl+Shift+G"))
            .clicked()
        {
            studio.ungroup_selected();
            ui.close();
        }
        if ui
            .add_enabled(
                studio.selection.len() >= 2,
                Button::new("Compound shape").shortcut_text("Ctrl+8"),
            )
            .clicked()
        {
            studio.combine_selected();
            ui.close();
        }
        if ui.button("Release compound         Ctrl+Shift+8").clicked() {
            studio.release_compound();
            ui.close();
        }
        ui.separator();
        if let Some(horizontal) = super::selection::flip_buttons(ui, studio.can_flip_selection()) {
            studio.flip_selection(horizontal);
        }
        ui.separator();
        if ui
            .button("Convert to path")
            .on_hover_text("Convert shapes or text to editable vector outlines")
            .clicked()
        {
            if let Some((li, id)) = studio.primary() {
                studio.convert_object_to_path(li, id);
            }
            ui.close();
        }
        if ui
            .button("Trace to vector")
            .on_hover_text("Trace the active pixel layer. U selects the tool.")
            .clicked()
        {
            studio.trace_active_raster();
            ui.close();
        }
        if ui
            .add_enabled(
                !studio.node_sel.is_empty(),
                Button::new("Break path at point"),
            )
            .clicked()
        {
            studio.break_node();
            ui.close();
        }
        ui.separator();
        if ui
            .add(Button::new("Swap fill / stroke").shortcut_text("X"))
            .clicked()
        {
            studio.swap_fill_stroke();
            ui.close();
        }
        ui.separator();
        if ui
            .add_enabled(
                !studio.selection.is_empty(),
                Button::new("Wrap selection in frame"),
            )
            .clicked()
        {
            studio.wrap_selection_frame();
            ui.close();
        }
        if ui
            .add_enabled(
                !studio.selection.is_empty(),
                Button::new("Wrap selection in artboard"),
            )
            .clicked()
        {
            studio.wrap_selection_artboard();
            ui.close();
        }
        if ui
            .add_enabled(
                studio.artboard_sel.len() == 1,
                Button::new("Clone artboard"),
            )
            .clicked()
        {
            if let Some(&id) = studio.artboard_sel.first() {
                studio.clone_artboard(id);
            }
            ui.close();
        }
    });
}

fn arrange_menu(ui: &mut Ui, studio: &mut Studio) {
    ui.menu_button("Arrange", |ui| {
        ui.label(RichText::new("Align").small().color(fg_weak()));
        if ui.button("Left").clicked() {
            studio.align_sel(crate::align::Align::Left);
            ui.close();
        }
        if ui.button("Centre").clicked() {
            studio.align_sel(crate::align::Align::CenterX);
            ui.close();
        }
        if ui.button("Right").clicked() {
            studio.align_sel(crate::align::Align::Right);
            ui.close();
        }
        if ui.button("Top").clicked() {
            studio.align_sel(crate::align::Align::Top);
            ui.close();
        }
        if ui.button("Middle").clicked() {
            studio.align_sel(crate::align::Align::CenterY);
            ui.close();
        }
        if ui.button("Bottom").clicked() {
            studio.align_sel(crate::align::Align::Bottom);
            ui.close();
        }
        ui.separator();
        if ui.button("Distribute horizontally").clicked() {
            studio.distribute_sel(crate::align::Distribute::Horizontal);
            ui.close();
        }
        if ui.button("Distribute vertically").clicked() {
            studio.distribute_sel(crate::align::Distribute::Vertical);
            ui.close();
        }
        ui.separator();
        ui.label(RichText::new("Order").small().color(fg_weak()));
        if ui
            .add(Button::new("Bring to front").shortcut_text("Ctrl+Shift+]"))
            .clicked()
        {
            studio.bring_to_front();
            ui.close();
        }
        if ui
            .add(Button::new("Bring forward").shortcut_text("Ctrl+]"))
            .clicked()
        {
            studio.bring_forward();
            ui.close();
        }
        if ui
            .add(Button::new("Send backward").shortcut_text("Ctrl+["))
            .clicked()
        {
            studio.send_backward();
            ui.close();
        }
        if ui
            .add(Button::new("Send to back").shortcut_text("Ctrl+Shift+["))
            .clicked()
        {
            studio.send_to_back();
            ui.close();
        }
    });
}

fn view_menu(ui: &mut Ui, studio: &mut Studio) {
    ui.menu_button("View", |ui| {
        ui.menu_button("Start tab", |ui| super::welcome::startup_preferences(ui, studio));
        ui.separator();
        if ui.button("Document conversion notes…").clicked() {
            studio.transfer_notes = studio.doc.import_notes.clone();
            studio.show_import_notes = true;
            ui.close();
        }
        for (label, tab) in [("Inspector", crate::app::libraries::Sidebar::Inspector), ("Palette library", crate::app::libraries::Sidebar::Palettes), ("Brand assets", crate::app::libraries::Sidebar::Brand)] {
            if ui.selectable_label(studio.libraries.sidebar == tab, label).clicked() {
                studio.libraries.sidebar=tab; studio.show_welcome=false; ui.close();
            }
        }
        ui.separator();
        if ui
            .add(Button::new("Zoom in").shortcut_text("Ctrl++"))
            .clicked()
        {
            let at = studio
                .cursor
                .map(|c| studio.view.to_screen(c))
                .unwrap_or(Pt::ZERO);
            studio.zoom_by(1.25, at);
            ui.close();
        }
        if ui
            .add(Button::new("Zoom out").shortcut_text("Ctrl+-"))
            .clicked()
        {
            let at = studio
                .cursor
                .map(|c| studio.view.to_screen(c))
                .unwrap_or(Pt::ZERO);
            studio.zoom_by(1.0 / 1.25, at);
            ui.close();
        }
        if ui
            .add(Button::new("Fit artboard").shortcut_text("Ctrl+0"))
            .clicked()
        {
            studio.need_fit = true;
            ui.close();
        }
        if ui.button("Fit selection").clicked() {
            studio.zoom_to_objects(true);
            ui.close();
        }
        if ui
            .add(Button::new("100%").shortcut_text("Ctrl+1"))
            .clicked()
        {
            studio.view.scale = 1.0;
            ui.close();
        }
        ui.separator();
        ui.checkbox(&mut studio.show_rulers, "Rulers").on_hover_text(
            "Drag down from the top ruler or right from the left ruler to add a guide. Drag the ruler corner to set zero; double-click it to reset."
        );
        ui.checkbox(&mut studio.show_key_hud, "Shortcut HUD")
            .on_hover_text("Live tool and modifier hints at the bottom of the window · Ctrl+/");
        ui.checkbox(&mut studio.doc.grid.visible, "Grid");
        if ui
            .add(
                Button::new(if studio.doc.ruler.guides_visible {
                    "Hide guides"
                } else {
                    "Show guides"
                })
                .shortcut_text("Ctrl+;"),
            )
            .clicked()
        {
            studio.toggle_guides();
            ui.close();
        }
        ui.menu_button("Guides", |ui| {
            if ui.add_enabled(!studio.doc.ruler.guides_locked, Button::new("Lock all guides")).clicked() {
                studio.set_guides_locked(true);
                ui.close();
            }
            if ui.add_enabled(studio.doc.ruler.guides_locked, Button::new("Unlock all guides")).clicked() {
                studio.set_guides_locked(false);
                ui.close();
            }
            if ui.button(if studio.doc.ruler.guides_visible { "Hide guides" } else { "Show guides" }).clicked() {
                studio.toggle_guides(); ui.close();
            }
        });
        ui.menu_button("Ruler units", |ui| {
            for unit in crate::document::RulerUnit::ALL {
                if ui
                    .selectable_label(studio.doc.ruler.unit == unit, unit.label())
                    .clicked()
                {
                    studio.set_ruler_unit(unit);
                    ui.close();
                }
            }
        });
        if ui.button("Reset ruler zero").clicked() {
            studio.set_ruler_origin(Pt::ZERO);
            ui.close();
        }
        if ui
            .add_enabled(!studio.doc.guides.is_empty(), Button::new("Clear ruler guides"))
            .clicked()
        {
            studio.clear_guides();
            ui.close();
        }
        ui.separator();
        if ui
            .add(
                Button::new(if studio.snap.enabled {
                    "Disable snapping"
                } else {
                    "Enable snapping"
                })
                .shortcut_text("Ctrl+Shift+;"),
            )
            .clicked()
        {
            studio.toggle_snapping();
            ui.close();
        }
        ui.label(
            RichText::new("Hold Ctrl during a drag to invert snapping")
                .small()
                .color(fg_weak()),
        );
        ui.checkbox(&mut studio.snap.grid, "Snap to grid");
        ui.checkbox(&mut studio.snap.guides, "Snap to guides");
        ui.checkbox(&mut studio.snap.objects, "Snap to objects");
        ui.checkbox(&mut studio.snap.artboards, "Snap to artboards");
        ui.checkbox(&mut studio.snap.spacing, "Equal spacing");
        ui.separator();
        if ui
            .add(Button::new("Keyboard shortcuts").shortcut_text("F1"))
            .clicked()
        {
            studio.show_shortcuts = true;
            ui.close();
        }
    });
}

fn switch_persona(studio: &mut Studio, persona: Persona) {
    studio.switch_persona(persona);
}

fn persona_picker(ui: &mut Ui, studio: &mut Studio) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        for persona in [
            Persona::Design,
            Persona::Pixel,
            Persona::Layout,
            Persona::Photo,
            Persona::Motion,
        ] {
            if icons::persona_button(
                ui,
                persona,
                !studio.show_welcome && studio.persona == persona,
            )
            .clicked()
            {
                switch_persona(studio, persona);
            }
        }
    });
}

pub fn doc_tabs(ui: &mut Ui, studio: &mut Studio) {
    studio.ensure_tabs();
    let mut previews = document_previews::begin(ui.ctx(), studio);
    Panel::left("doc-tabs")
        .resizable(false)
        .exact_size(80.0)
        .frame(
            Frame::new()
                .fill(bg_window())
                .inner_margin(Margin::symmetric(8, 8)),
        )
        .show(ui, |ui| {
            let mut switch = None;
            let mut close = None;
            ScrollArea::vertical()
                .id_salt("document-tabs-scroll")
                .scroll_bar_visibility(eframe::egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = 8.0;
                    for i in 0..studio.tab_count() {
                        let (title, dirty) = studio.tab_title(i);
                        let active = i == studio.active_tab;
                        ui.push_id(i, |ui| {
                            let (rect, response) = ui.allocate_exact_size(
                                vec2(64.0, 64.0),
                                eframe::egui::Sense::click(),
                            );
                            response.widget_info(|| {
                                eframe::egui::WidgetInfo::selected(
                                    eframe::egui::WidgetType::Button,
                                    ui.is_enabled(),
                                    active,
                                    title,
                                )
                            });
                            let mut close_hovered = false;
                            if ui.is_rect_visible(rect) {
                                if let Some(texture) = previews.image(ui.ctx(), studio, i) {
                                    ui.painter().image(
                                        texture.id(),
                                        rect,
                                        eframe::egui::Rect::from_min_max(
                                            eframe::egui::Pos2::ZERO,
                                            eframe::egui::pos2(1.0, 1.0),
                                        ),
                                        Color32::WHITE,
                                    );
                                } else {
                                    ui.painter().rect_filled(rect, 4.0, bg_panel());
                                    ui.painter().text(
                                        rect.center(),
                                        eframe::egui::Align2::CENTER_CENTER,
                                        ph::FRAME_CORNERS,
                                        icons::font(24.0),
                                        fg_weak(),
                                    );
                                }
                                ui.painter().rect_stroke(
                                    rect,
                                    4.0,
                                    eframe::egui::Stroke::new(
                                        if active { 2.0 } else { 1.0 },
                                        if active || response.has_focus() {
                                            accent()
                                        } else {
                                            border()
                                        },
                                    ),
                                    eframe::egui::StrokeKind::Inside,
                                );
                                if dirty {
                                    ui.painter().circle_filled(
                                        rect.left_bottom() + vec2(7.0, -7.0),
                                        4.0,
                                        accent(),
                                    );
                                }
                                if ui.rect_contains_pointer(rect) || response.has_focus() {
                                    let close_rect = eframe::egui::Rect::from_min_size(
                                        rect.right_top() + vec2(-22.0, 2.0),
                                        vec2(20.0, 20.0),
                                    );
                                    let close_response = ui.interact(
                                        close_rect,
                                        response.id.with("close"),
                                        eframe::egui::Sense::click(),
                                    );
                                    close_hovered = close_response.hovered();
                                    ui.painter().rect_filled(close_rect, 4.0, bg_panel());
                                    ui.painter().text(
                                        close_rect.center(),
                                        eframe::egui::Align2::CENTER_CENTER,
                                        ph::X,
                                        icons::font(14.0),
                                        fg(),
                                    );
                                    if close_response.clicked() {
                                        close = Some(i);
                                    }
                                    close_response.on_hover_text("Close document");
                                }
                            }
                            if response.clicked() && !close_hovered {
                                switch = Some(i);
                            }
                            if response.middle_clicked() {
                                close = Some(i);
                            }
                            response
                                .on_hover_text(if dirty {
                                    format!("{title} • Unsaved changes")
                                } else {
                                    title.to_owned()
                                })
                                .context_menu(|ui| {
                                    if ui.button("Close document").clicked() {
                                        close = Some(i);
                                        ui.close();
                                    }
                                });
                        });
                    }
                    ui.add_space(2.0);
                    ui.horizontal(|ui| {
                        ui.add_space(17.0);
                        if icons::icon_button(ui, ph::PLUS, "New document  Ctrl+N", false) {
                            studio.new_tab_welcome();
                        }
                    });
                });
            if let Some(i) = switch {
                studio.switch_tab(i);
            }
            if let Some(i) = close {
                studio.request_close_tab(i);
            }
        });
    previews.store(ui.ctx());
}

pub fn document_previews_ready(ctx: &eframe::egui::Context) -> bool {
    document_previews::ready(ctx)
}

pub fn left_toolbar(ui: &mut Ui, studio: &mut Studio) {
    Panel::left("tools")
        .resizable(false)
        .exact_size(52.0)
        .frame(
            Frame::new()
                .fill(bg_panel())
                .inner_margin(Margin::symmetric(8, 8)),
        )
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 3.0;
            let well = match studio.persona {
                Persona::Design => Tool::design_well(),
                Persona::Pixel => Tool::pixel_well(),
                Persona::Photo => Tool::photo_well(),
                Persona::Motion => Tool::motion_well(),
                Persona::Layout => Tool::layout_well(),
            };
            let libraries = matches!(
                studio.persona,
                Persona::Design | Persona::Pixel | Persona::Layout | Persona::Motion
            );
            let reserved = if libraries { 78.0 } else { 0.0 };
            ScrollArea::vertical()
                .id_salt("tools-scroll")
                .max_height((ui.available_height() - reserved).max(40.0))
                .scroll_bar_visibility(eframe::egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
                .show(ui, |ui| {
                    let mut last_group = "";
                    for t in well.iter().copied() {
                        let group = match t {
                            Tool::Select | Tool::Node => "sel",
                            Tool::Pen | Tool::Pencil => "path",
                            Tool::Rect
                            | Tool::Ellipse
                            | Tool::Polygon
                            | Tool::Star
                            | Tool::Line
                            | Tool::Artboard
                            | Tool::Frame => "shape",
                            Tool::Text | Tool::Gradient | Tool::Eyedropper | Tool::Trace => "look",
                            Tool::Brush
                            | Tool::Eraser
                            | Tool::Fill
                            | Tool::Clone
                            | Tool::Heal
                            | Tool::Smudge => "paint",
                            Tool::Marquee | Tool::EllipseMarquee | Tool::Lasso | Tool::Wand => {
                                "selpx"
                            }
                            Tool::Hand | Tool::Zoom | Tool::Crop => "nav",
                        };
                        if last_group != group && !last_group.is_empty() {
                            icons::well_separator(ui);
                        }
                        last_group = group;
                        if icons::tool_button(ui, t, studio.tool == t) {
                            studio.set_tool(t);
                        }
                    }
                });
            if libraries {
                ui.add_space(4.0);
                icons::well_separator(ui);
                library_buttons(ui, studio);
            }
        });
}

fn library_buttons(ui: &mut Ui, studio: &mut Studio) {
    if icons::icon_button(
        ui,
        ph::SHAPES,
        "Shape library",
        studio.show_shape_browser,
    ) {
        studio.show_shape_browser = !studio.show_shape_browser;
    }
    if icons::icon_button(
        ui,
        ph::IMAGES,
        "Free photos",
        studio.show_asset_browser,
    ) {
        studio.show_asset_browser = !studio.show_asset_browser;
    }
}

pub fn status_bar(ui: &mut Ui, studio: &mut Studio) {
    Panel::bottom("status")
        .exact_size(28.0)
        .frame(
            Frame::new()
                .fill(bg_panel())
                .inner_margin(Margin::symmetric(12, 3)),
        )
        .show(ui, |ui| {
            if studio.show_welcome {
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.label(
                        RichText::new("F1  Keyboard shortcuts")
                            .small()
                            .color(fg_weak()),
                    );
                    ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                        ui.add(
                            eframe::egui::Label::new(
                                RichText::new(&studio.status).small().color(fg_weak()),
                            )
                            .truncate(),
                        )
                        .on_hover_text(&studio.status);
                    });
                });
                return;
            }
            let width = ui.available_width();
            if studio.persona == Persona::Photo {
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    zoom_corner(ui, studio);
                    ui.add_space(12.0);
                    if let Some(image) = studio.photo.selected() {
                        let (w, h) = image.dimensions();
                        let (w, h) = image.develop.output_dim(w, h);
                        ui.label(
                            RichText::new(format!("{w} × {h} px"))
                                .small()
                                .color(fg_weak()),
                        );
                    }
                    ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                        ui.add(
                            eframe::egui::Label::new(
                                RichText::new(&studio.photo.status).small().color(fg_weak()),
                            )
                            .truncate(),
                        );
                    });
                });
                return;
            }
            // Reserve metadata before the hint, so a long tool description truncates.
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                zoom_corner(ui, studio);
                ui.add_space(12.0);
                ui.label(
                    RichText::new(format!(
                        "{:.0} × {:.0} px",
                        studio.doc.width, studio.doc.height
                    ))
                    .small()
                    .color(fg_weak()),
                )
                .on_hover_text(format!("{} dpi", studio.doc.dpi as i32));
                if width > 760.0 && !studio.selection.is_empty() {
                    ui.add_space(10.0);
                    ui.label(
                        RichText::new(format!("{} selected", studio.selection.len()))
                            .small()
                            .color(fg_weak()),
                    );
                }
                if width > 1000.0
                    && let Some(cursor) = studio.cursor
                {
                    ui.add_space(10.0);
                    ui.label(
                        RichText::new(format!("{:.0}, {:.0}", cursor.x, cursor.y))
                            .small()
                            .color(fg_weak()),
                    );
                }
                ui.add_space(12.0);
                ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                    ui.label(RichText::new(studio.tool.label()).small().color(fg()));
                    ui.label(RichText::new(studio.tool.key()).small().color(fg_weak()));
                    ui.add_space(8.0);
                    ui.add(
                        eframe::egui::Label::new(
                            RichText::new(&studio.status).small().color(fg_weak()),
                        )
                        .truncate(),
                    )
                    .on_hover_text(&studio.status);
                });
            });
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chrome_frame(
        ctx: &eframe::egui::Context,
        studio: &mut Studio,
        size: eframe::egui::Vec2,
        events: Vec<eframe::egui::Event>,
    ) -> Vec<eframe::egui::epaint::ClippedShape> {
        let mut output = ctx.run_ui(
            eframe::egui::RawInput {
                screen_rect: Some(eframe::egui::Rect::from_min_size(
                    eframe::egui::Pos2::ZERO,
                    size,
                )),
                events,
                ..Default::default()
            },
            |ui| {
                top_bar(ui, studio);
                doc_tabs(ui, studio);
                left_toolbar(ui, studio);
            },
        );
        output.textures_delta.clear();
        output.shapes
    }

    #[test]
    fn mode_icons_stay_centered_and_document_previews_form_a_64px_left_column() {
        for size in [vec2(960.0, 640.0), vec2(1600.0, 1000.0)] {
            let ctx = eframe::egui::Context::default();
            super::super::theme::apply(&ctx);
            let mut studio = Studio::new();
            studio.show_welcome = false;
            studio.persona = Persona::Design;
            studio.new_tab();
            let mut shapes = vec![];
            for _ in 0..3 {
                shapes = chrome_frame(&ctx, &mut studio, size, vec![]);
            }
            let modes: Vec<_> = shapes
                .iter()
                .filter_map(|shape| {
                    if let eframe::egui::Shape::Text(text) = &shape.shape {
                        let center = text.galley.rect.translate(text.pos.to_vec2()).center();
                        if center.y < 44.0
                            && (center.x - size.x * 0.5).abs() < 102.0
                            && [
                                Persona::Design,
                                Persona::Pixel,
                                Persona::Layout,
                                Persona::Photo,
                                Persona::Motion,
                            ]
                            .iter()
                            .any(|&persona| text.galley.text() == icons::persona_glyph(persona))
                        {
                            return Some(center);
                        }
                    }
                    None
                })
                .collect();
            assert_eq!(modes.len(), 5, "all modes must remain directly available");
            let center = (modes[0].x + modes[4].x) * 0.5;
            assert!(
                (center - size.x * 0.5).abs() < 1.0,
                "mode group at {center} on {size:?}"
            );
            let mut previews: Vec<_> = shapes
                .iter()
                .filter_map(|shape| match &shape.shape {
                    eframe::egui::Shape::Rect(rect) if rect.rect.size() == vec2(64.0, 64.0) => {
                        Some(rect.rect)
                    }
                    _ => None,
                })
                .collect();
            previews.dedup();
            assert_eq!(previews.len(), 2);
            assert_eq!(
                previews[0].left(),
                8.0,
                "thumbnail column sits at the window's left edge"
            );
            assert_eq!(previews[0].left(), previews[1].left());
            assert_eq!(previews[1].top() - previews[0].bottom(), 8.0);
            let pos = previews[0].center();
            for pressed in [true, false] {
                chrome_frame(
                    &ctx,
                    &mut studio,
                    size,
                    vec![
                        eframe::egui::Event::PointerMoved(pos),
                        eframe::egui::Event::PointerButton {
                            pos,
                            button: eframe::egui::PointerButton::Primary,
                            pressed,
                            modifiers: eframe::egui::Modifiers::NONE,
                        },
                    ],
                );
            }
            assert_eq!(
                studio.active_tab, 0,
                "clicking a preview switches documents"
            );
            let pos = previews[1].center();
            for pressed in [true, false] {
                chrome_frame(
                    &ctx,
                    &mut studio,
                    size,
                    vec![
                        eframe::egui::Event::PointerMoved(pos),
                        eframe::egui::Event::PointerButton {
                            pos,
                            button: eframe::egui::PointerButton::Middle,
                            pressed,
                            modifiers: eframe::egui::Modifiers::NONE,
                        },
                    ],
                );
            }
            assert_eq!(
                studio.tab_count(),
                1,
                "middle-click still closes a document"
            );
            let pos = previews[0].right_top() + vec2(-12.0, 12.0);
            chrome_frame(
                &ctx,
                &mut studio,
                size,
                vec![eframe::egui::Event::PointerMoved(pos)],
            );
            for pressed in [true, false] {
                chrome_frame(
                    &ctx,
                    &mut studio,
                    size,
                    vec![eframe::egui::Event::PointerButton {
                        pos,
                        button: eframe::egui::PointerButton::Primary,
                        pressed,
                        modifiers: eframe::egui::Modifiers::NONE,
                    }],
                );
            }
            assert!(
                studio.show_welcome,
                "the hover close button closes the final document"
            );
        }
    }

    #[test]
    fn switching_personas_cancels_a_live_deformation_before_hiding_the_canvas() {
        let mut studio = Studio::new();
        studio.finish_create(
            crate::app::CreateKind::Rect,
            Pt::new(60.0, 60.0),
            Pt::new(160.0, 160.0),
        );
        let (layer, id) = studio.selection[0];
        let before = studio.doc.find_shape(layer, id).unwrap().clone();
        let history = studio.history.len();
        studio.begin_deform(crate::deform::Mode::Mesh);
        let start = studio.deformation.as_ref().unwrap().cage.handles()[0];
        studio.deformation_drag_start(0, start);
        studio.deformation_drag_to(start - Pt::new(20.0, 10.0), false);
        switch_persona(&mut studio, Persona::Photo);
        assert!(studio.deformation.is_none());
        assert_eq!(studio.persona, Persona::Photo);
        assert_eq!(studio.doc.find_shape(layer, id), Some(&before));
        assert_eq!(studio.history.len(), history);
    }
}
