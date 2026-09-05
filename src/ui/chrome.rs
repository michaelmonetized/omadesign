use crate::app::Studio;
use crate::geom::Pt;
use crate::tools::{Persona, Tool};
use crate::ui::icons::{self, ph};
use crate::ui::theme::{accent_soft, bg_panel, bg_window, fg, fg_weak};
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
            let compact = ui.available_width() < 1100.0;
            ui.horizontal_centered(|ui| {
                ui.label(RichText::new("omadesign").strong().size(14.0).color(fg()));
                ui.add_space(10.0);
                ui.scope(|ui| {
                    ui.visuals_mut().widgets.inactive.weak_bg_fill = Color32::TRANSPARENT;
                    file_menu(ui, studio);
                    edit_menu(ui, studio);
                    ui.menu_button("Select", |ui| super::selection::menu(ui, studio));
                    object_menu(ui, studio);
                    arrange_menu(ui, studio);
                    view_menu(ui, studio);
                });
                if !studio.show_welcome {
                    ui.add_space(14.0);
                    persona_picker(ui, studio, compact);
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
                        let actual_zoom = if photo {
                            studio.photo.view_scale * studio.photo.fit_scale
                        } else {
                            studio.view.scale
                        };
                        eframe::egui::ComboBox::from_id_salt("zoom-preset")
                            .selected_text(format!("{:.0}%", actual_zoom * 100.0))
                            .width(64.0)
                            .show_ui(ui, |ui| {
                                for (zoom, label) in [
                                    (0.25, "25%"),
                                    (0.5, "50%"),
                                    (1.0, "100%"),
                                    (1.5, "150%"),
                                    (2.0, "200%"),
                                    (4.0, "400%"),
                                ] {
                                    if ui
                                        .selectable_label((actual_zoom - zoom).abs() < 0.01, label)
                                        .clicked()
                                    {
                                        if photo {
                                            studio.photo.view_scale =
                                                zoom / studio.photo.fit_scale.max(0.001);
                                            studio.photo.view_offset = eframe::egui::Vec2::ZERO;
                                        } else {
                                            studio.zoom_by(
                                                zoom / studio.view.scale.max(0.001),
                                                studio.canvas_zoom_anchor(),
                                            );
                                        }
                                    }
                                }
                            });
                        ui.add_space(8.0);
                        if icons::icon_button(
                            ui,
                            ph::IMAGES,
                            "Free photos",
                            studio.show_asset_browser,
                        ) {
                            studio.show_asset_browser = !studio.show_asset_browser;
                        }
                        if icons::icon_button(
                            ui,
                            ph::SHAPES,
                            "Shape library",
                            studio.show_shape_browser,
                        ) {
                            studio.show_shape_browser = !studio.show_shape_browser;
                        }
                        if !compact
                            && studio.persona == Persona::Photo
                            && ui.button("Place in Design").clicked()
                        {
                            studio.send_photo_to_design();
                        }
                    });
                }
            });
        });
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
            .add(Button::new("Save").shortcut_text("Ctrl+S"))
            .clicked()
        {
            studio.save();
            ui.close();
        }
        if ui
            .add(Button::new("Save as…").shortcut_text("Ctrl+Shift+S"))
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
        if ui.button("Export animated SVG…").clicked() {
            studio.export_animated_svg();
            ui.close();
        }
        if ui.button("Export Lottie…").clicked() {
            studio.export_lottie();
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
                studio.history.can_undo(),
                Button::new("Undo").shortcut_text("Ctrl+Z"),
            )
            .clicked()
        {
            studio.undo();
            ui.close();
        }
        if ui
            .add_enabled(
                studio.history.can_redo(),
                Button::new("Redo").shortcut_text("Ctrl+Shift+Z"),
            )
            .clicked()
        {
            studio.redo();
            ui.close();
        }
        ui.separator();
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
            studio.paste_clipboard(None);
            ui.close();
        }
        if ui
            .add(Button::new("Duplicate").shortcut_text("Ctrl+D"))
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
                studio.selection.len() >= 2,
                Button::new("Combine").shortcut_text("Ctrl+G"),
            )
            .clicked()
        {
            studio.combine_selected();
            ui.close();
        }
        if ui.button("Release         Ctrl+Shift+G").clicked() {
            studio.release_compound();
            ui.close();
        }
        ui.separator();
        if ui.button("Flip horizontal").clicked() {
            flip(studio, true);
            ui.close();
        }
        if ui.button("Flip vertical").clicked() {
            flip(studio, false);
            ui.close();
        }
        ui.separator();
        if ui.button("Convert to path").clicked() {
            if let Some((li, id)) = studio.primary() {
                studio.ensure_path(li, id);
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

fn flip(studio: &mut Studio, horizontal: bool) {
    studio.flip_selection(horizontal);
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
    if studio.persona == persona {
        return;
    }
    studio.end_deform(true);
    studio.end_pixel_stroke(true);
    studio.commit_type_edit();
    studio.reset_snap_gesture();
    studio.persona = persona;
    studio.op = None;
    studio.playing = false;
    studio.tool = match persona {
        Persona::Design | Persona::Motion => Tool::Select,
        Persona::Pixel => Tool::Brush,
        Persona::Photo => Tool::Hand,
    };
    studio.show_welcome = false;
}

fn persona_picker(ui: &mut Ui, studio: &mut Studio, compact: bool) {
    let personas = [
        Persona::Design,
        Persona::Pixel,
        Persona::Photo,
        Persona::Motion,
    ];
    if compact {
        eframe::egui::ComboBox::from_id_salt("studio-persona")
            .selected_text(studio.persona.name())
            .width(88.0)
            .show_ui(ui, |ui| {
                for persona in personas {
                    if ui
                        .selectable_label(studio.persona == persona, persona.name())
                        .on_hover_text(persona.hint())
                        .clicked()
                    {
                        switch_persona(studio, persona);
                    }
                }
                if studio.persona == Persona::Photo {
                    ui.separator();
                    if ui.button("Place in Design").clicked() {
                        studio.send_photo_to_design();
                        ui.close();
                    }
                }
            });
    } else {
        for persona in personas {
            let active = studio.persona == persona;
            let button = Button::new(RichText::new(persona.name()).color(if active {
                fg()
            } else {
                fg_weak()
            }))
            .fill(if active {
                accent_soft()
            } else {
                Color32::TRANSPARENT
            });
            if ui
                .add_sized(vec2(62.0, 28.0), button)
                .on_hover_text(persona.hint())
                .clicked()
            {
                switch_persona(studio, persona);
            }
        }
    }
}

pub fn doc_tabs(ui: &mut Ui, studio: &mut Studio) {
    studio.ensure_tabs();
    Panel::top("doc-tabs")
        .exact_size(38.0)
        .frame(
            Frame::new()
                .fill(bg_window())
                .inner_margin(Margin::symmetric(8, 4)),
        )
        .show(ui, |ui| {
            let mut switch = None;
            let mut close = None;
            ScrollArea::horizontal()
                .id_salt("document-tabs-scroll")
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 4.0;
                        for i in 0..studio.tab_count() {
                            let (title, dirty) = studio.tab_title(i);
                            let active = i == studio.active_tab;
                            ui.push_id(i, |ui| {
                                let label = if dirty {
                                    format!("{title} •")
                                } else {
                                    title.to_owned()
                                };
                                Frame::new()
                                    .fill(if active {
                                        bg_panel()
                                    } else {
                                        Color32::TRANSPARENT
                                    })
                                    .corner_radius(6.0)
                                    .inner_margin(Margin::symmetric(3, 0))
                                    .show(ui, |ui| {
                                        ui.horizontal(|ui| {
                                            let response = ui.add_sized(
                                                vec2(136.0, 28.0),
                                                Button::new(
                                                    RichText::new(label).size(12.0).color(
                                                        if active { fg() } else { fg_weak() },
                                                    ),
                                                )
                                                .frame(false)
                                                .truncate(),
                                            );
                                            if response.clicked() {
                                                switch = Some(i);
                                            }
                                            if response.middle_clicked() {
                                                close = Some(i);
                                            }
                                            response.on_hover_text(title).context_menu(|ui| {
                                                if ui.button("Close document").clicked() {
                                                    close = Some(i);
                                                    ui.close();
                                                }
                                            });
                                            if icons::tiny_icon(ui, ph::X, "Close document", false)
                                            {
                                                close = Some(i);
                                            }
                                        });
                                    });
                            });
                        }
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
            };
            ScrollArea::vertical()
                .id_salt("tools-scroll")
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
                            | Tool::Artboard => "shape",
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
        });
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
                ui.label(
                    RichText::new("F1  Keyboard shortcuts")
                        .small()
                        .color(fg_weak()),
                );
                return;
            }
            let width = ui.available_width();
            // Reserve metadata before the hint, so a long tool description truncates.
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
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
