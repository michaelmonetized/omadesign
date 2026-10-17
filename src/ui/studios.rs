#[path = "gradient_editor.rs"]
mod gradient_editor;
#[path = "object_appearance.rs"]
mod object_appearance;
#[path = "layer_rows.rs"]
mod layer_rows;

use super::layer_drag;
use crate::app::input_history;

#[cfg(test)]
#[path = "field_input_tests.rs"]
mod field_input_tests;

use crate::app::Studio;
use crate::color::Blend;
use crate::document::{Cap, Fill, Join, Stroke as DocStroke};
use crate::geom::Geom;
use crate::tools::{Persona, Tool};
use crate::ui::icons::{self, ph};
use crate::ui::theme::{accent, accent_soft, bg_panel, bg_widget, border, fg, fg_weak};
use eframe::egui::{
    Color32, ComboBox, Frame, Layout, Margin, Panel, RichText, ScrollArea, Slider, Stroke, Ui, vec2,
};

pub fn right_panel(ui: &mut Ui, studio: &mut Studio) {
    if studio.paint_mask
        && studio
            .active_layer
            .and_then(|index| studio.doc.layers.get(index))
            .is_none_or(|layer| layer.mask.is_none())
    {
        studio.paint_mask = false;
    }
    let width_id = eframe::egui::Id::new("studios-width");
    let saved = eframe::egui::containers::panel::PanelState::load(
        ui.ctx(),
        eframe::egui::Id::new("studios"),
    )
    .map_or(288.0, |state| state.outer_rect.width())
    .clamp(256.0, 420.0);
    let width = ui
        .ctx()
        .data_mut(|data| *data.get_persisted_mut_or_insert_with(width_id, || saved));
    let panel = Panel::right("studios")
        .resizable(false)
        .exact_size(width)
        .frame(
            Frame::new()
                .fill(bg_panel())
                .inner_margin(Margin::symmetric(14, 14)),
        )
        .show(ui, |ui| {
            inspector_content(ui, studio);
            layers_content(ui, studio);
        });
    let edge = eframe::egui::Rect::from_min_max(
        panel.response.rect.left_top() - vec2(3.0, 0.0),
        panel.response.rect.left_bottom() + vec2(3.0, 0.0),
    );
    let resize = ui
        .interact(edge, width_id.with("drag"), eframe::egui::Sense::drag())
        .on_hover_cursor(eframe::egui::CursorIcon::ResizeHorizontal);
    if resize.dragged() {
        let width = (width - resize.drag_delta().x).clamp(256.0, 420.0);
        ui.ctx()
            .data_mut(|data| data.insert_persisted(width_id, width));
    }
}

/// Draw property commands inside an application-placed panel.
/// Args: panel UI and studio. Returns no value; typed history stays studio-owned.
pub(super) fn inspector_content(ui: &mut Ui, studio: &mut Studio) {
    if studio.paint_mask
        && studio
            .active_layer
            .and_then(|index| studio.doc.layers.get(index))
            .is_none_or(|layer| layer.mask.is_none())
    {
        studio.paint_mask = false;
    }

            ui.set_max_width(ui.available_width());
            ui.spacing_mut().item_spacing = vec2(8.0, 6.0);
            if studio.libraries.sidebar != crate::app::libraries::Sidebar::Inspector {
                super::library::show(ui, studio);
                return;
            }
            inspector_title(ui, studio);
            let locked = studio.selection.iter().any(|&(layer,id)| !studio.layer_unlocked(layer) || studio.doc.find_shape(layer,id).is_some_and(|shape|shape.locked));
            if locked {ui.label("Selection locked");}
            ui.add_enabled_ui(!locked, |ui| {
            section_gap(ui);
            let design = studio.persona == Persona::Design || studio.persona == Persona::Layout;
            let motion = studio.persona == Persona::Motion;
            let layout = studio.persona == Persona::Layout;
            let paint = pixel_context(studio);
            let reshaping = studio.deformation.is_some();
            let typing = studio.type_edit.is_some()
                || studio.tool == Tool::Text
                || studio.selected_type().is_some();
            let properties_height = ui.available_height().max(0.0);
            ScrollArea::vertical()
                .id_salt("properties-scroll")
                .max_height(properties_height)
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    super::selection::arrange_panel(ui, studio);
                    super::text_geometry::wrap_inspector(ui, studio);
                    section_gap(ui);
                    if motion {
                        motion_studio(ui, studio);
                        section_gap(ui);
                        transform_studio(ui, studio, true);
                        section_gap(ui);
                    }
                    if reshaping {
                        super::deform::inspector(ui, studio);
                        section_gap(ui);
                    }
                    if layout {
                        super::layout::inspector(ui, studio);
                        section_gap(ui);
                    }
                    if design
                        && !reshaping
                        && (!studio.selection.is_empty()
                            || !studio.artboard_sel.is_empty()
                            || matches!(
                                studio.tool,
                                Tool::Rect
                                    | Tool::Polygon
                                    | Tool::Star
                                    | Tool::Artboard
                                    | Tool::Frame
                            ))
                    {
                        if typing || ui.ctx().viewport_rect().height() < 760.0 {
                            eframe::egui::CollapsingHeader::new("Layout")
                                .show(ui, |ui| transform_studio(ui, studio, false));
                        } else {
                            transform_studio(ui, studio, true);
                        }
                        section_gap(ui);
                    }
                    if studio.persona == Persona::Pixel {
                        super::raster::inspector(ui, studio);
                        section_gap(ui);
                    }
                    if paint || studio.paint_mask {
                        super::masking::inspector(ui, studio);
                        section_gap(ui);
                        super::masking::selection_tools(ui, studio);
                        brush_studio(ui, studio);
                        section_gap(ui);
                    }
                    if !studio.paint_mask {
                        if matches!(studio.tool, Tool::Brush | Tool::Fill | Tool::Eyedropper) {
                            paint_color_studio(ui, studio);
                        } else if !paint {
                            color_studio(ui, studio);
                        }
                    }
                    if studio.tool == Tool::Trace {
                        section_gap(ui);
                        trace_studio(ui, studio);
                    }
                    if design || motion {
                        section_gap(ui);
                        eframe::egui::CollapsingHeader::new("Effects")
                            .show(ui, |ui| fx_studio(ui, studio));
                    }
                });
            });

}

/// Draw layer commands inside an application-placed panel.
/// Args: panel UI and studio. Returns no value; layer edits retain typed history.
pub(super) fn layers_content(ui: &mut Ui, studio: &mut Studio) {
    ui.spacing_mut().item_spacing=vec2(3.0,3.0);
    studio.with_field_history(ui,"layer-settings",|ui,studio| {
        let n=studio.doc.layers.len();
    if let Some(i) = studio.active_layer.filter(|&i| i < n) {
        let layer_blend_owner = ui.make_persistent_id(("layer-blend", studio.doc.layers[i].id));
        if studio.doc.layers[i].is_group {
            let before = studio.doc.layers[i].pass_through;
            let mut after = before;
            if ui
                .add_enabled(
                    studio.layer_unlocked(i),
                    eframe::egui::Checkbox::new(&mut after, "Pass through"),
                )
                .on_hover_text("Allow layers in this group to blend with the layers below it")
                .changed()
            {
                studio.commit(crate::document::Cmd::SetGroupPassThrough {
                    index: i,
                    before,
                    after,
                });
            }
        }
        ui.vertical(|ui| {
            if !studio.layer_unlocked(i) {
                ui.disable();
            }
            let mut blend = studio.doc.layers[i].blend;
            let pass_through = studio.doc.layers[i].is_group && studio.doc.layers[i].pass_through;
            ui.add_enabled_ui(!pass_through, |ui| {
                ui.horizontal(|ui| {
                    let label = ui.label("Layer blend");
                    let mut response = super::fields::native_scope(ui, layer_blend_owner, |ui| ComboBox::from_id_salt("active-layer-blend")
                        .selected_text(if pass_through {
                            "Pass through"
                        } else {
                            blend.name()
                        })
                        .width(80.0)
                        .show_ui(ui, |ui| {
                            for value in Blend::ALL {
                                ui.selectable_value(&mut blend, value, value.name());
                            }
                        }).response).inner.labelled_by(label.id);
                    super::keyboard_controls::selection_keys(ui, &mut response, "Layer blend", &mut blend,
                        &Blend::ALL.into_iter().map(|mode| (mode, mode.name().into())).collect::<Vec<_>>());
                });
            });
            let mut opacity = studio.doc.layers[i].opacity * 100.0;
            ui.horizontal(|ui| {
                let label = ui.label("Layer opacity");
                let response = ui.add(
                    eframe::egui::DragValue::new(&mut opacity)
                        .range(0.0..=100.0)
                        .suffix("%")
                        .speed(0.5)
                        .max_decimals(0),
                ).labelled_by(label.id).on_hover_text("Layer opacity");
                response.widget_info(|| {
                    let mut info = eframe::egui::WidgetInfo::drag_value(response.enabled(), f64::from(opacity));
                    info.label = Some("Layer opacity".into());
                    info
                });
                input_history::changed(response);
            });
            let layer = &studio.doc.layers[i];
            if blend != layer.blend || (opacity / 100.0 - layer.opacity).abs() > 0.0001 {
                studio.commit(crate::document::Cmd::SetLayerMeta {
                    index: i,
                    name: layer.name.clone(),
                    visible: layer.visible,
                    locked: layer.locked,
                    opacity: opacity / 100.0,
                    blend,
                    before: (
                        layer.name.clone(),
                        layer.visible,
                        layer.locked,
                        layer.opacity,
                        layer.blend,
                    ),
                });
            }
        });
        ui.add_space(8.0);
    }

    });
    let height=(ui.available_height()-36.0).max(0.0);
    ScrollArea::vertical().id_salt("layers-scroll").min_scrolled_height(0.0).max_height(height).auto_shrink([false,false])
        .show(ui,|ui|layers_studio(ui,studio));
    ui.separator();
    ui.horizontal(|ui| {
        if icons::tiny_icon(ui,ph::PLUS,"New layer",false){studio.add_layer(studio.persona==Persona::Pixel);}
        if icons::tiny_icon(ui,ph::FOLDER_OPEN,"New group",false){studio.add_layer_group();}
        ui.add_enabled_ui(studio.active_layer.is_some(),|ui| {
            if icons::tiny_icon(ui,ph::SELECTION,"Add layer mask",false){if let Some(layer)=studio.active_layer {studio.add_layer_mask(layer,true);}}
            if icons::tiny_icon(ui,ph::MAGIC_WAND,"Layer effects",false){super::panel_workspace::open(ui.ctx(),studio,"effects");}
            if icons::tiny_icon(ui,ph::COPY,"Duplicate selection",false){studio.duplicate_selection();}
            if icons::tiny_icon(ui,ph::TRASH,"Delete selection or layer",false){if studio.selection.is_empty(){studio.delete_layer();}else{studio.delete_selection();}}
        });
    });
}

fn section_gap(ui: &mut Ui) {
    ui.add_space(12.0);
}

/// Place existing typed inspector controls in an independently dockable pane.
/// Args: pane UI, current editor and pane identity. Returns whether this pane owns the requested controls.
pub(super) fn pane(ui: &mut Ui, studio: &mut Studio, panel: &str) -> bool {
    if !matches!(panel,"transform"|"appearance"|"gradients"|"stroke"|"character"|"keyframes"|"effects"|"trace"|"brushes"|"transparency"|"color"){return false;}
    if panel == "color" && pixel_context(studio) && studio.tool != Tool::Text && studio.type_edit.is_none() {
        foreground_color_studio(ui, studio);
        return true;
    }
    let locked=studio.selection.iter().any(|&(layer,id)|!studio.layer_unlocked(layer)||studio.doc.find_shape(layer,id).is_some_and(|s|s.locked));
    ui.add_enabled_ui(!locked,|ui| match panel {
        "transform" => transform_studio(ui,studio,false),
        "appearance" => color_studio(ui,studio),
        "gradients" => paint_studio(ui,studio,true),
        "stroke" => stroke_studio(ui,studio),
        "character" => character_studio(ui,studio),
        "keyframes" => super::timeline_keyboard::editor(ui,studio),
        "effects" => fx_studio(ui,studio),
        "trace" => trace_studio(ui,studio),
        "brushes" => {super::brushes::pane(ui,studio);brush_studio(ui,studio);},
        "transparency" => object_appearance::show(ui,studio),
        "color" => studio.with_field_history(ui,"inline-color-pane",|ui,studio| {
            color_row(ui,studio,true);
            color_row(ui,studio,false);
            let style=inspected_style(studio);
            let mut color=if studio.fill_active {match &style.fill {
                Fill::Solid(color)|Fill::Linear{c0:color,..}|Fill::Radial{c0:color,..}=>*color,
                Fill::Gradient(gradient)=>gradient.sample(0.0),_=>studio.brush.color,
            }} else {style.stroke.as_ref().map_or(studio.brush.color,|stroke|stroke.color)};
            if super::color_picker::inline(ui,&mut color) {
                if studio.fill_active {
                    let style=inspected_style(studio).clone();
                    if studio.is_motion()&&let Some((_,id))=studio.primary(){
                        let rest=match &style.fill {Fill::Solid(c)|Fill::Linear{c0:c,..}|Fill::Radial{c0:c,..}=>*c,Fill::Gradient(g)=>g.sample(0.0),_=>color};
                        studio.key_fill(id,color,rest);
                    } else {
                        let fill=match style.fill {Fill::Gradient(mut gradient)=>{if let Some(stop)=gradient.stops.first_mut(){stop.color=color;}Fill::Gradient(gradient)},_=>Fill::Solid(color)};
                        studio.set_fill(fill);
                    }
                } else {studio.set_stroke_color(color);}
                studio.brush.color=color;
            }
        }),
        _=>{},
    });
    true
}

fn heading(ui: &mut Ui, title: &str) {
    if ui.data(|d|d.get_temp::<bool>(eframe::egui::Id::new("dock-pane-content"))).unwrap_or(false){return;}
    ui.label(RichText::new(title).strong().size(12.0).color(fg()));
}

fn geometry_icon(geometry: &Geom) -> &'static str {
    match geometry {
        Geom::Rect { .. } => ph::RECTANGLE,
        Geom::Ellipse { .. } => ph::CIRCLE,
        Geom::Polygon { .. } => ph::HEXAGON,
        Geom::Star { .. } => ph::STAR,
        Geom::Line { .. } => ph::LINE_SEGMENT,
        Geom::Text(_) => ph::TEXT_T,
        Geom::Path { .. } | Geom::Paths { .. } | Geom::Poly { .. } => ph::PATH,
    }
}

fn pixel_context(studio: &Studio) -> bool {
    studio.persona == Persona::Pixel
        || matches!(
            studio.tool,
            Tool::Brush
                | Tool::Eraser
                | Tool::Fill
                | Tool::Clone
                | Tool::Heal
                | Tool::Smudge
                | Tool::Wand
                | Tool::Eyedropper
                | Tool::Marquee
                | Tool::EllipseMarquee
                | Tool::Lasso
        )
}

fn inspector_title(ui: &mut Ui, studio: &Studio) {
    let selected = studio
        .primary()
        .and_then(|(li, id)| studio.doc.find_shape(li, id));
    let selection_name =
        (studio.selection.len() > 1).then(|| format!("{} objects", studio.selection.len()));
    let (icon, name, description) = if let Some(channel) = studio.channel_paint_target() {
        (ph::SELECTION, "Alpha channel", channel.name.clone())
    } else if studio.paint_mask {
        (
            ph::SELECTION,
            "Layer mask",
            "Paint to reveal or hide".into(),
        )
    } else if pixel_context(studio) {
        let (icon, name) = match studio.tool {
            Tool::Heal => (ph::BANDAIDS, "Healing brush"),
            Tool::Clone => (ph::COPY, "Clone brush"),
            Tool::Smudge => (ph::DROP, "Smudge brush"),
            Tool::Eraser => (ph::ERASER, "Eraser"),
            Tool::Fill => (ph::PAINT_BUCKET, "Fill"),
            Tool::Wand => (ph::MAGIC_WAND, "Magic wand"),
            Tool::Eyedropper => (ph::EYEDROPPER, "Eyedropper"),
            Tool::Marquee => (ph::SELECTION, "Marquee"),
            Tool::EllipseMarquee => (ph::CIRCLE_DASHED, "Elliptical marquee"),
            Tool::Lasso => (ph::POLYGON, "Lasso"),
            _ => (ph::PAINT_BRUSH, studio.tool.label()),
        };
        let layer_name = studio
            .active_layer
            .and_then(|index| studio.doc.layers.get(index))
            .filter(|layer| layer.kind.pixels().is_some())
            .map_or_else(
                || "Choose a pixel layer".into(),
                |layer| format!("Pixels · {}", layer.name),
            );
        let description = if matches!(studio.tool, Tool::Eyedropper | Tool::Brush | Tool::Fill) {
            format!("{} · {}", studio.brush.color.hex(), layer_name)
        } else if let Some(mask) = &studio.pixel_sel {
            let n = crate::paint::selected_count(mask);
            if n == 0 {
                layer_name
            } else {
                format!("{n} px selected · {layer_name}")
            }
        } else {
            layer_name
        };
        (icon, name, description)
    } else if let Some(group) = studio
        .active_layer
        .and_then(|i| studio.doc.layers.get(i))
        .filter(|l| l.is_group)
    {
        (
            ph::FOLDER_OPEN,
            group.name.as_str(),
            "Group properties".into(),
        )
    } else if let Some(name) = &selection_name {
        (ph::STACK, name.as_str(), "Shared properties".into())
    } else if let Some(shape) = selected {
        (
            geometry_icon(&shape.geom),
            shape.name.as_str(),
            "Object properties".into(),
        )
    } else if !studio.artboard_sel.is_empty() {
        (ph::FRAME_CORNERS, "Artboard", "Canvas properties".into())
    } else if let Some(layer) = studio
        .primary()
        .and_then(|(li, _)| studio.doc.layers.get(li))
    {
        (ph::IMAGES, layer.name.as_str(), "Image properties".into())
    } else {
        (
            if studio.persona == Persona::Pixel {
                ph::PAINT_BRUSH
            } else {
                ph::SHAPES
            },
            studio.tool.label(),
            "Defaults for new objects".into(),
        )
    };
    ui.horizontal(|ui| {
        let (rect, _) = ui.allocate_exact_size(vec2(34.0, 38.0), eframe::egui::Sense::hover());
        let chip = matches!(studio.tool, Tool::Eyedropper | Tool::Brush | Tool::Fill)
            && (pixel_context(studio) || studio.persona == Persona::Pixel);
        if chip {
            let color = studio.brush.color.to_egui();
            ui.painter()
                .rect_filled(rect.shrink2(vec2(0.0, 2.0)), 9.0, color);
            ui.painter().rect_stroke(
                rect.shrink2(vec2(0.0, 2.0)),
                9.0,
                Stroke::new(1.0, border()),
                eframe::egui::StrokeKind::Middle,
            );
        } else {
            ui.painter()
                .rect_filled(rect.shrink2(vec2(0.0, 2.0)), 9.0, accent_soft());
            ui.painter().text(
                rect.center(),
                eframe::egui::Align2::CENTER_CENTER,
                icon,
                icons::font(22.0),
                accent(),
            );
        }
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 3.0;
            ui.add(eframe::egui::Label::new(RichText::new(name).size(14.0).strong()).truncate());
            ui.add(
                eframe::egui::Label::new(RichText::new(description).size(10.0).color(fg_weak()))
                    .truncate(),
            );
        });
    });
}

pub(super) fn inspected_style(studio: &Studio) -> &crate::document::Style {
    studio
        .primary()
        .and_then(|(li, id)| studio.doc.find_shape(li, id))
        .map(|shape| &shape.style)
        .unwrap_or(&studio.style)
}

fn number_field<N: eframe::egui::emath::Numeric>(
    ui: &mut Ui,
    label: &str,
    value: &mut N,
    range: std::ops::RangeInclusive<N>,
    suffix: &str,
) -> bool {
    let field = number_field_response(ui, label, value, range, suffix);
    input_history::record(field.response, field.changed)
}

fn number_field_response<N: eframe::egui::emath::Numeric>(
    ui: &mut Ui,
    label: &str,
    value: &mut N,
    range: std::ops::RangeInclusive<N>,
    suffix: &str,
) -> super::fields::FieldResponse {
    super::fields::number(ui, label, label, value, range, suffix, matches!(label, "X" | "Y" | "Rotate").then(|| N::from_f64(0.0)), Default::default())
}

fn bounds_fields(
    ui: &mut Ui,
    x: &mut f32,
    y: &mut f32,
    width: &mut f32,
    height: &mut f32,
    min_size: f32,
) -> bool {
    let mut changed = false;
    let before = (*width, *height);
    let lock_id = ui.make_persistent_id("constrain-proportions");
    let mut linked = ui.ctx().data_mut(|data| data.get_persisted::<bool>(lock_id)).unwrap_or(false);
    ui.columns(2, |columns| {
        changed |= number_field(&mut columns[0], "X", x, -100000.0..=100000.0, "");
        changed |= number_field(&mut columns[1], "Y", y, -100000.0..=100000.0, "");
    });
    let mut size_changed = [false; 2];
    let total = ui.available_width();
    let gap = ui.spacing().item_spacing.x;
    let field_width = ((total - 22.0 - gap * 2.0) * 0.5).max(24.0);
    let row_height = ui.text_style_height(&eframe::egui::TextStyle::Body) + ui.spacing().item_spacing.y + 24.0;
    let (_,rect) = ui.allocate_space(vec2(total,row_height));
    let width_row = ui.scope_builder(eframe::egui::UiBuilder::new().id_salt("bounds-width").max_rect(eframe::egui::Rect::from_min_size(rect.min,vec2(field_width,row_height))),|ui| {
        size_changed[0] = number_field(ui,"W",width,min_size..=20000.0,"");
    });
    let height_row = ui.scope_builder(eframe::egui::UiBuilder::new().id_salt("bounds-height").max_rect(eframe::egui::Rect::from_min_size(rect.min+vec2(field_width+gap,0.0),vec2(field_width,row_height))),|ui| {
        size_changed[1] = number_field(ui,"H",height,min_size..=20000.0,"");
    });
    let lock_rect=eframe::egui::Rect::from_min_size(eframe::egui::pos2(rect.right()-22.0,rect.bottom()-24.0),vec2(22.0,22.0));
    ui.scope_builder(eframe::egui::UiBuilder::new().id_salt("bounds-aspect").max_rect(lock_rect),|ui| {
        if icons::tiny_icon(ui,if linked {ph::LOCK} else {ph::LOCK_OPEN},"Constrain proportions",linked) {
            linked = !linked;
            ui.ctx().data_mut(|data|data.insert_persisted(lock_id,linked));
        }
    });
    let overflow=width_row.response.rect.bottom().max(height_row.response.rect.bottom())-rect.bottom();
    if overflow>0.0 {ui.add_space(overflow);}
    if linked && before.0 > 0.0 && before.1 > 0.0 {
        let ratio = before.0 / before.1;
        if ratio.is_finite() && ratio > 0.0 {
            if size_changed[0] {
                *width = width.clamp(min_size.max(min_size * ratio), 20000.0_f32.min(20000.0 * ratio));
                *height = *width / ratio;
            } else if size_changed[1] {
                *height = height.clamp(min_size.max(min_size / ratio), 20000.0_f32.min(20000.0 / ratio));
                *width = *height * ratio;
            }
        }
    }
    changed |= size_changed.into_iter().any(|value| value);
    changed
}

fn color_row(ui: &mut Ui, studio: &mut Studio, fill: bool) {
    studio.with_field_history(ui, ("color-row", fill), |ui, studio| {
        color_row_contents(ui, studio, fill)
    });
}

fn color_row_contents(ui: &mut Ui, studio: &mut Studio, fill: bool) {
    let style = inspected_style(studio);
    let source_fill = style.fill.clone();
    let source_stroke = style.stroke.clone();
    let original = if fill {
        match &source_fill {
            Fill::Solid(c) | Fill::Linear { c0: c, .. } | Fill::Radial { c0: c, .. } => Some(*c),
            Fill::Gradient(g) => Some(g.sample(0.)),
            Fill::None => None,
            Fill::Pattern(_) => Some(crate::color::Rgba::BLACK),
        }
    } else {
        source_stroke.as_ref().map(|stroke| {
            stroke
                .gradient
                .as_ref()
                .map(|g| g.sample(0.))
                .unwrap_or(stroke.color)
        })
    };
    if original.is_none() {
        let label = if fill { "Add fill" } else { "Add stroke" };
        let mut add = false;
        ui.horizontal(|ui| {
            add |= icons::tiny_icon(ui, ph::PLUS, label, false);
            ui.label(RichText::new(if fill {"Fill"} else {"Stroke"}).small().color(fg_weak()));
        });
        if add {
            let mut color = if fill {
                source_stroke
                    .as_ref()
                    .map(|stroke| stroke.color)
                    .unwrap_or(studio.brush.color)
            } else {
                match &source_fill {
                    Fill::Solid(color)
                    | Fill::Linear { c0: color, .. }
                    | Fill::Radial { c0: color, .. } => *color,
                    Fill::Gradient(g) => g.sample(0.),
                    Fill::None | Fill::Pattern(_) => studio.brush.color,
                }
            };
            color.a = 255;
            studio.fill_active = fill;
            if fill {
                studio.set_fill(Fill::Solid(color));
                studio.brush.color = color;
            } else {
                let stroke = DocStroke {
                    color,
                    ..Default::default()
                };
                studio.style.stroke = Some(stroke.clone());
                apply_stroke(studio, Some(stroke));
            }
        }
        return;
    }
    let rest_color = original.unwrap_or(studio.brush.color);
    let mut color = rest_color;
    if fill
        && studio.is_motion()
        && let Some((_, id)) = studio.primary()
        && let Some(keyed) = studio.live_pose(id).fill_color
    {
        color = keyed;
    }
    let mut changed = false;
    let mut remove = false;
    let label = if fill { "Fill" } else { "Stroke" };
    ui.push_id(label, |ui| {
        let width = ui.available_width();
        Frame::new()
            .fill(bg_widget())
            .corner_radius(7.0)
            .inner_margin(Margin::symmetric(7, 3))
            .show(ui, |ui| {
                ui.set_min_width(width - 14.0);
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 5.0;
                    if ui
                        .add_sized(
                            vec2(42.0, 22.0),
                            eframe::egui::Button::new(RichText::new(label).size(11.0).color(
                                if studio.fill_active == fill {
                                    fg()
                                } else {
                                    fg_weak()
                                },
                            ))
                            .frame(false),
                        )
                        .clicked()
                    {
                        studio.fill_active = fill;
                    }
                    ui.spacing_mut().interact_size = vec2(20.0, 20.0);
                    if crate::ui::color_picker::color_edit(ui, "appearance-color", &mut color) {
                        changed = true;
                        studio.fill_active = fill;
                    }
                    if crate::ui::color_picker::hex_field(
                        ui,
                        ui.id().with(("appearance-hex", fill, studio.primary())),
                        &mut color,
                        96.0,
                        false,
                    ) {
                        changed = true;
                        studio.fill_active = fill;
                    }
                    let mut opacity = f32::from(color.a) / 255.0 * 100.0;
                    if input_history::changed(ui
                        .add(
                            eframe::egui::DragValue::new(&mut opacity)
                                .range(0.0..=100.0)
                                .speed(0.5)
                                .max_decimals(0)
                                .suffix("%"),
                        )
                        .on_hover_text(format!("{label} opacity")))
                    {
                        color.a = (opacity / 100.0 * 255.0).round() as u8;
                        changed = true;
                    }
                    remove = icons::tiny_icon(
                        ui,
                        ph::MINUS,
                        &format!("Remove {}", label.to_lowercase()),
                        false,
                    );
                });
            });
    });
    if remove {
        if fill {
            studio.set_fill(Fill::None);
        } else {
            studio.style.stroke = None;
            apply_stroke(studio, None);
        }
    } else if changed {
        if fill
            && studio.is_motion()
            && let Some((_, id)) = studio.primary()
        {
            studio.key_fill(id, color, rest_color);
            studio.brush.color = color;
        } else if fill {
            let fill = match source_fill {
                Fill::Linear { from, to, c1, .. } => Fill::Linear {
                    from,
                    to,
                    c0: color,
                    c1,
                },
                Fill::Radial { c1, .. } => Fill::Radial { c0: color, c1 },
                Fill::Gradient(mut g) => {
                    if let Some(stop) = g.stops.first_mut() {
                        stop.color = color;
                    }
                    Fill::Gradient(g)
                }
                _ => Fill::Solid(color),
            };
            studio.set_fill(fill);
            studio.brush.color = color;
        } else {
            let mut stroke = source_stroke.unwrap_or_default();
            stroke.color = color;
            if let Some(g) = &mut stroke.gradient
                && let Some(stop) = g.stops.first_mut()
            {
                stop.color = color;
            }
            studio.style.stroke = Some(stroke.clone());
            apply_stroke(studio, Some(stroke));
        }
    }
}

fn color_studio(ui: &mut Ui, studio: &mut Studio) {paint_studio(ui,studio,false);}

fn paint_studio(ui: &mut Ui, studio: &mut Studio, gradients_only:bool) {
    ui.horizontal(|ui| {
        heading(ui, "Appearance");
        ui.with_layout(Layout::right_to_left(eframe::egui::Align::Center), |ui| {
            ui.menu_button("···", |ui| {
                if ui.button("Swap fill and stroke   X").clicked() {
                    studio.style = inspected_style(studio).clone();
                    studio.swap_fill_stroke();
                    ui.close();
                }
                for kind in crate::gradient::GradientKind::ALL {
                    if ui.button(format!("{} gradient", kind.name())).clicked() {
                        let gradient = crate::gradient::Gradient::new(
                            kind,
                            studio.gradient.0,
                            studio.gradient.1,
                        );
                        apply_gradient(studio, Some(gradient));
                        ui.close();
                    }
                }
                if ui.button("Solid color").clicked() {
                    apply_gradient(studio, None);
                    ui.close();
                }
            });
        });
    });
    if !gradients_only {color_row(ui, studio, true);color_row(ui, studio, false);}
    let style = inspected_style(studio).clone();
    let mut gradient = if studio.fill_active {
        style.fill.gradient()
    } else {
        style.stroke.as_ref().and_then(|s| s.gradient.clone())
    };
    ui.horizontal(|ui| {
        let label = ui.label(
            RichText::new(if studio.fill_active {
                "Fill type"
            } else {
                "Stroke type"
            })
            .small()
            .color(fg_weak()),
        );
        let current = gradient.as_ref().map(|g| g.kind);
        let pattern = matches!(style.fill, Fill::Pattern(_));
        let mut replace_pattern = false;
        let mut chosen = current;
        let mut choice_clicked = false;
        let owner = eframe::egui::Id::new(("paint-type", &studio.swap_id, &studio.selection, studio.fill_active));
        let picker = super::fields::native_scope(ui, owner, |ui| ComboBox::from_id_salt("value")
            .selected_text(if pattern { "Pattern" } else { current.map(|k| k.name()).unwrap_or("Solid") })
            .width(108.)
            .show_ui(ui, |ui| {
                if ui.selectable_value(&mut chosen, None, "Solid").clicked() {
                    replace_pattern = pattern;
                    choice_clicked = true;
                    ui.close();
                }
                for kind in crate::gradient::GradientKind::ALL {
                    if ui.selectable_value(&mut chosen, Some(kind), kind.name()).clicked() {
                        choice_clicked = true;
                        ui.close();
                    }
                }
            })).inner;
        super::welcome::name_control(ui, &picker.response, label.id, if studio.fill_active { "Fill type" } else { "Stroke type" });
        if chosen != current || replace_pattern {
            gradient = chosen.map(|kind| {
                let mut g = gradient.clone().unwrap_or_else(|| {
                    crate::gradient::Gradient::new(
                        kind,
                        if studio.fill_active {
                            style.fill.clone().solid_or(studio.gradient.0)
                        } else {
                            style
                                .stroke
                                .as_ref()
                                .map(|s| s.color)
                                .unwrap_or(studio.gradient.0)
                        },
                        studio.gradient.1,
                    )
                });
                if g.kind != kind {
                    g.kind = kind;
                    g.from = if kind == crate::gradient::GradientKind::Linear {
                        [0., 0.5]
                    } else {
                        [0.5, 0.5]
                    };
                    g.to = [1., 0.5];
                }
                g
            });
            apply_gradient(studio, gradient.clone());
        }
        if choice_clicked {
            picker.response.request_focus();
        }
    });
    if !gradients_only {super::svg_paint::show(ui, studio);}
    if let Some(mut gradient) = gradient {
        let bounds = studio
            .primary()
            .and_then(|(li, id)| studio.doc.find_shape(li, id))
            .map(|s| s.geom.bbox())
            .unwrap_or_else(|| {
                crate::geom::Bounds::from_min_size(
                    crate::geom::Pt::ZERO,
                    crate::geom::Pt::new(100., 100.),
                )
            });
        let mut shown_angle = gradient.angle(bounds);
        if studio.is_motion()
            && let Some((_, id)) = studio.primary()
        {
            let offset = studio.live_pose(id).gradient_angle.unwrap_or(0.0);
            shown_angle += offset;
        }
        let fill_active = studio.fill_active;
        studio.with_field_history(
            ui,
            ("gradient", fill_active),
            |ui, studio| {
                let edits = gradient_editor::editor(ui, &mut gradient, shown_angle);
                if edits.design {
                    for stop in &gradient.stops {
                        studio.push_recent(stop.color);
                    }
                    apply_gradient(studio, Some(gradient.clone()));
                }
                if let Some(angle) = edits.angle {
                    if studio.is_motion()
                        && let Some((_, id)) = studio.primary()
                    {
                        studio.key_prop(
                            id,
                            crate::motion::Prop::GradientAngle,
                            angle - gradient.angle(bounds),
                        );
                    } else {
                        gradient.set_angle(angle, bounds);
                        apply_gradient(studio, Some(gradient.clone()));
                    }
                }
            },
        );
    }
    if !gradients_only {
    ui.horizontal_wrapped(|ui| {
        let menu=ui.menu_button(RichText::new(ph::PALETTE).font(icons::font(16.0)), |ui| {
            ui.set_width(236.0);
            heading(ui, "Quick colors");
            color_grid(ui, studio, false);
            if !studio.recent.is_empty() {
                ui.add_space(8.0);
                heading(ui, "Recently used");
                color_grid(ui, studio, true);
            }
            if ui.button("Open palette library →").clicked() {
                super::panel_workspace::open(ui.ctx(),studio,"swatches");
                ui.close();
            }
        });
        super::agent::named(ui,menu.response,"Color library").on_hover_text("Color library");
    });
    if studio.selection.is_empty()
        || studio
            .selection
            .iter()
            .any(|&(li, id)| studio.doc.find_shape(li, id).is_some())
    {
        ui.add_space(8.0);
        heading(ui, "Stroke");
        stroke_studio(ui, studio);
    }
    object_appearance::show(ui, studio);
    }
}

fn apply_gradient(studio: &mut Studio, gradient: Option<crate::gradient::Gradient>) {
    let style = inspected_style(studio).clone();
    if let Some(g) = &gradient {
        studio.gradient = (g.sample(0.), g.sample(1.));
    }
    if studio.fill_active {
        studio.set_fill(
            gradient
                .map(Fill::Gradient)
                .unwrap_or_else(|| Fill::Solid(style.fill.solid_or(studio.brush.color))),
        );
    } else {
        let mut stroke = style.stroke.unwrap_or_default();
        if let Some(g) = &stroke.gradient {
            stroke.color = g.sample(0.);
        }
        stroke.gradient = gradient;
        studio.style.stroke = Some(stroke.clone());
        apply_stroke(studio, Some(stroke));
    }
}

fn color_grid(ui: &mut Ui, studio: &mut Studio, recent: bool) {
    let count = if recent {
        studio.recent.len()
    } else {
        studio.swatches.len()
    };
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = vec2(6.0, 6.0);
        for index in 0..count {
            let color = if recent {
                studio.recent[index]
            } else {
                studio.swatches[index]
            };
            let (rect, response) =
                ui.allocate_exact_size(vec2(20.0, 20.0), eframe::egui::Sense::click());
            ui.painter().rect_filled(rect, 5.0, color.to_egui());
            ui.painter().rect_stroke(
                rect,
                5.0,
                Stroke::new(1.0, border()),
                eframe::egui::StrokeKind::Inside,
            );
            let response = response.on_hover_text(color.hex());
            if response.clicked() {
                if studio.fill_active {
                    let style = inspected_style(studio);
                    let rest = match &style.fill {
                        Fill::Solid(current)
                        | Fill::Linear { c0: current, .. }
                        | Fill::Radial { c0: current, .. } => *current,
                        Fill::Gradient(gradient) => gradient.sample(0.),
                        Fill::None | Fill::Pattern(_) => color,
                    };
                    if studio.is_motion()
                        && !style.fill.is_none()
                        && let Some((_, id)) = studio.primary()
                    {
                        studio.key_fill(id, color, rest);
                    } else {
                        studio.set_fill(Fill::Solid(color));
                    }
                    studio.brush.color = color;
                } else {
                    studio.style = inspected_style(studio).clone();
                    studio.set_stroke_color(color);
                }
            } else if response.secondary_clicked() {
                studio.style = inspected_style(studio).clone();
                studio.set_stroke_color(color);
            }
        }
    });
}

fn stroke_studio(ui: &mut Ui, studio: &mut Studio) {
    studio.with_field_history(ui, "stroke", |ui, studio| {
        stroke_studio_contents(ui, studio)
    });
}

fn stroke_studio_contents(ui: &mut Ui, studio: &mut Studio) {
    let mut stroke = inspected_style(studio).stroke.clone().unwrap_or_default();
    let mut enabled = inspected_style(studio).stroke.is_some();
    if ui.checkbox(&mut enabled, "Enable stroke").changed() {
        studio.style.stroke = enabled.then(|| stroke.clone());
        apply_stroke(studio, studio.style.stroke.clone());
    }
    let rest_width = stroke.width;
    let mut width = rest_width;
    if studio.is_motion()
        && let Some((_, id)) = studio.primary()
    {
        width += studio.live_pose(id).stroke_width.unwrap_or(0.0);
    }
    let width_changed = number_field(ui, "Width", &mut width, 0.0..=100_000.0, " px");
    let mut changed = false;
    if width_changed {
        if studio.is_motion()
            && let Some((_, id)) = studio.primary()
        {
            studio.key_prop(id, crate::motion::Prop::StrokeWidth, width - rest_width);
        } else {
            stroke.width = width;
            changed = true;
        }
    }
    ui.horizontal_wrapped(|ui| {
        use crate::document::StrokeAlignment;
        ui.label(RichText::new("Position").small().color(fg_weak()));
        for (value, label) in [
            (StrokeAlignment::Inside, "Inside"),
            (StrokeAlignment::Center, "Center"),
            (StrokeAlignment::Outside, "Outside"),
        ] {
            changed |= ui
                .selectable_value(&mut stroke.alignment, value, label)
                .changed();
        }
    });
    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new("Ends").small().color(fg_weak()));
        for (cap, label) in [
            (Cap::Butt, "Flat"),
            (Cap::Round, "Round"),
            (Cap::Square, "Square"),
        ] {
            changed |= ui.selectable_value(&mut stroke.cap, cap, label).changed();
        }
    });
    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new("Joins").small().color(fg_weak()));
        for (join, label) in [
            (Join::Miter, "Sharp"),
            (Join::Round, "Round"),
            (Join::Bevel, "Bevel"),
        ] {
            changed |= ui.selectable_value(&mut stroke.join, join, label).changed();
        }
    });
    let mut dashed = stroke.dash.is_some();
    if ui.checkbox(&mut dashed, "Dashed line").changed() {
        stroke.dash = if dashed { Some((6.0, 4.0)) } else { None };
        if !dashed {
            stroke.dash_offset = 0.0;
        }
        changed = true;
    }
    if dashed {
        let (rest_dash, rest_gap) = stroke.dash.unwrap_or((6.0, 4.0));
        let mut dash = rest_dash;
        let mut gap = rest_gap;
        let mut length = stroke.dash_offset;
        if studio.is_motion()
            && let Some((_, id)) = studio.primary()
        {
            let pose = studio.live_pose(id);
            dash += pose.dash.unwrap_or(0.0);
            gap += pose.gap.unwrap_or(0.0);
            length += pose.dash_length.unwrap_or(0.0);
        }
        let dash_changed = number_field(ui, "Dash", &mut dash, 0.0..=10_000.0, " px");
        let gap_changed = number_field(ui, "Gap", &mut gap, 0.0..=10_000.0, " px");
        let length_changed = number_field(ui, "Length", &mut length, -10_000.0..=10_000.0, " px");
        if studio.is_motion()
            && let Some((_, id)) = studio.primary()
        {
            if dash_changed {
                studio.key_prop(id, crate::motion::Prop::Dash, dash - rest_dash);
            }
            if gap_changed {
                studio.key_prop(id, crate::motion::Prop::Gap, gap - rest_gap);
            }
            if length_changed {
                studio.key_prop(
                    id,
                    crate::motion::Prop::DashLength,
                    length - stroke.dash_offset,
                );
            }
        } else if dash_changed || gap_changed || length_changed {
            stroke.dash = Some((dash, gap));
            stroke.dash_offset = length;
            changed = true;
        }
    }
    if changed {
        studio.style.stroke = if stroke.width <= 0.01 {
            None
        } else {
            Some(stroke)
        };
        apply_stroke(studio, studio.style.stroke.clone());
    }
}

fn apply_stroke(studio: &mut Studio, stroke: Option<DocStroke>) {
    let commands = studio
        .selection
        .iter()
        .filter_map(|&(li, id)| {
            let s = studio.doc.find_shape(li, id)?;
            let mut after = s.style.clone();
            after.stroke = stroke.clone();
            (after != s.style).then(|| crate::document::Cmd::SetStyle {
                layer: li,
                id,
                before: s.style.clone(),
                after,
            })
        })
        .collect::<Vec<_>>();
    if !commands.is_empty() {
        studio.commit(crate::document::Cmd::Batch(commands));
    }
}

pub(super) fn character_studio(ui: &mut Ui, studio: &mut Studio) {
    studio.with_field_history(ui, "character", |ui, studio| {
        character_studio_contents(ui, studio)
    });
}

fn character_studio_contents(ui: &mut Ui, studio: &mut Studio) {
    heading(ui, "Typography");
    super::text_geometry::frame_inspector(ui, studio);
    super::text_geometry::path_inspector(ui, studio);
    let live = studio.selected_type();
    let metrics=studio.selected_character_metrics();
    let font = metrics.first().and_then(|s|s.font.clone()).or_else(||live
        .as_ref()
        .map(|t| t.font.clone()))
        .unwrap_or_else(|| studio.text_font.clone());
    let label = crate::text::label_for(&font);
    let mut chosen = font.clone();
    let owner = ui.make_persistent_id("font-picker");
    let font_button = super::svg_paint::control(ui, owner, "Font family", |ui| ui.add_sized(
        vec2(ui.available_width(), 28.0),
        eframe::egui::Button::new(RichText::new(&label).size(12.0)).truncate(),
    ));
    // The search field and scrollbar need clicks without dismissing the picker.
    let font_popup = eframe::egui::Popup::menu(&font_button)
        .close_behavior(eframe::egui::PopupCloseBehavior::CloseOnClickOutside);
    font_popup.show(|ui| {
        ui.set_width(260.0);
        let label = ui.label("Font search");
        super::svg_paint::control(ui, owner.with("local-search"), "Search local fonts", |ui| ui.add(
            eframe::egui::TextEdit::singleline(&mut studio.font_query)
                .id(owner.with("local-search"))
                .hint_text("Search fonts")
                .desired_width(220.0),
        ).labelled_by(label.id));
        let q = studio.font_query.to_ascii_lowercase();
        let recents = studio.font_recents.clone();
        let used = studio.used_fonts();
        let all = crate::text::all_fonts_cached();
        ScrollArea::vertical()
            .id_salt("font-list")
            .max_height(180.0)
            .show(ui, |ui| {
                if let Some(kit) = &studio.libraries.typography {
                    ui.label(RichText::new("Project fonts").small().color(fg_weak()));
                    for face in &kit.fonts {
                        let name = format!("{} · {}", face.role, face.family);
                        if (q.is_empty() || name.to_lowercase().contains(&q))
                            && super::svg_paint::control(ui, owner.with(("project", &face.id, &face.role)), &format!("Project font {name}"), |ui| ui.selectable_label(face.id == font, name)).clicked()
                        {
                            chosen = face.id.clone();
                            ui.close();
                        }
                    }
                }
                if !recents.is_empty() {
                    ui.label(RichText::new("Recents").small().color(fg_weak()));
                    for p in recents.iter().take(5) {
                        let name = crate::text::label_for(p);
                        if super::svg_paint::control(ui, owner.with(("recent", p)), &format!("Recent font {name}"), |ui| ui.selectable_label(*p == font, &name)).clicked() {
                            chosen = p.clone();
                            ui.close();
                        }
                    }
                }
                if !used.is_empty() {
                    ui.label(RichText::new("Used in document").small().color(fg_weak()));
                    for p in &used {
                        let name = crate::text::label_for(p);
                        if super::svg_paint::control(ui, owner.with(("used", p)), &format!("Used font {name}"), |ui| ui.selectable_label(*p == font, &name)).clicked() {
                            chosen = p.clone();
                            ui.close();
                        }
                    }
                }
                ui.label(RichText::new("All").small().color(fg_weak()));
                for f in all.iter() {
                    if !q.is_empty() && !f.name.to_ascii_lowercase().contains(&q) {
                        continue;
                    }
                    let path = f.path.to_string_lossy().to_string();
                    let resp = super::svg_paint::control(ui, owner.with(("installed", &path)), &format!("Installed font {}", f.name), |ui| ui.selectable_label(path == font, &f.name));
                    if path == font && studio.font_scroll_once {
                        resp.scroll_to_me(Some(eframe::egui::Align::Center));
                        studio.font_scroll_once = false;
                    }
                    if resp.clicked() {
                        chosen = path;
                        ui.close();
                    }
                }
            });
    });
    if chosen != font {
        let path = chosen.clone();
        studio.remember_font(&path);
        if live.is_some(){studio.patch_character(|s|s.font=Some(path.clone()));}else{studio.patch_type(|t| t.font = path.clone());}
        studio.text_font = path;
        studio.font_scroll_once = true;
    }
    eframe::egui::CollapsingHeader::new("Google Fonts")
        .default_open(false)
        .show(ui, |ui| google_fonts_ui(ui, studio));

    let mut px = metrics.first().and_then(|s|s.px).or_else(||live.as_ref().map(|t|t.px)).unwrap_or(studio.text_px);
    if number_field(ui, "Size", &mut px, 8.0..=400.0, " px") {
        if live.is_some(){studio.patch_character(|s|s.px=Some(px));}else{studio.patch_type(|t| t.px = px);}
    }
    super::character_spacing::show(ui,studio);
    super::opentype::show(ui, studio, &font);
    super::paragraph::show(ui, studio);
    if studio.type_edit.is_some() {
        ui.label(
            RichText::new("Typing on the canvas. Esc finishes.")
                .small()
                .color(accent()),
        );
    } else if live.is_none() {
        ui.label(
            RichText::new("Applies to the next type you place.")
                .small()
                .color(fg_weak()),
        );
    }
}

#[cfg(test)]
#[path = "font_picker_tests.rs"]
mod font_picker_tests;

struct DownloadedFont {
    path: std::path::PathBuf,
    family: String,
    document: String,
    selection: Vec<(usize, u64)>,
}

fn google_fonts_ui(ui: &mut Ui, studio: &mut Studio) {
    use crate::ui::jobs;
    const CATALOG: &str = "google-font-catalog";
    const DOWNLOAD: &str = "google-font-download";
    if let Some(result) = jobs::poll::<Vec<crate::google_fonts::GoogleFont>>(ui.ctx(), CATALOG) {
        match result {
            Ok(catalog) => {
                studio.google_status = format!("{} families", catalog.len());
                studio.google_catalog = catalog;
            }
            Err(error) => studio.google_status = error,
        }
    }
    if let Some(result) = jobs::poll::<DownloadedFont>(ui.ctx(), DOWNLOAD) {
        match result {
            Ok(font) => {
                if studio.swap_id == font.document && studio.selection == font.selection {
                    let chosen = font.path.to_string_lossy().into_owned();
                    studio.patch_type(|text| text.font = chosen.clone());
                    studio.text_font = chosen;
                }
                studio.google_status = format!("Installed {}", font.family);
            }
            Err(error) => studio.google_status = error,
        }
    }
    if !studio.google_catalog_loaded {
        studio.google_catalog_loaded = true;
        studio.google_status = "Loading font families…".into();
        jobs::start(ui.ctx(), CATALOG, || Ok(crate::google_fonts::catalog()));
    }
    let loading = jobs::is_running::<Vec<crate::google_fonts::GoogleFont>>(ui.ctx(), CATALOG);
    let downloading = jobs::is_running::<DownloadedFont>(ui.ctx(), DOWNLOAD);
    let owner = ui.make_persistent_id("google-fonts");
    let mut affected = Vec::new();
    ui.horizontal(|ui| {
        let label = ui.label("Google font search");
        let response = super::svg_paint::control(ui, owner.with("search"), "Search Google fonts", |ui| ui.add(
            eframe::egui::TextEdit::singleline(&mut studio.google_query)
                .id(owner.with("search"))
                .hint_text("Search fonts")
                .desired_width(140.0),
        ).labelled_by(label.id));
        affected.push(response);
        let refresh = super::svg_paint::control(ui, owner.with("refresh"), "Refresh Google fonts", |ui| ui
            .add_enabled(!loading, eframe::egui::Button::new("Refresh"))
        );
        if refresh.clicked() {
            studio.google_status = "Refreshing font families…".into();
            jobs::start(ui.ctx(), CATALOG, crate::google_fonts::fetch_catalog);
        }
        affected.push(refresh);
    });
    ui.horizontal(|ui| {
        let label = ui.label(RichText::new("Variant").small().color(fg_weak()));
        let mut response = super::fields::native_scope(ui, owner.with("variant"), |ui| ComboBox::from_id_salt("google-variant")
            .selected_text(&studio.google_variant)
            .width(100.0)
            .show_ui(ui, |ui| {
                for (variant, label) in [
                    ("regular", "Regular"),
                    ("italic", "Italic"),
                    ("700", "Bold"),
                    ("700italic", "Bold italic"),
                ] {
                    let response = super::svg_paint::control(ui, owner.with(("variant-option", variant)), label, |ui| ui.selectable_value(&mut studio.google_variant, variant.to_owned(), label));
                    if response.clicked() { ui.close(); }
                }
            }).response).inner.labelled_by(label.id);
        super::keyboard_controls::selection_keys(ui, &mut response, "Google font variant", &mut studio.google_variant,
            &[("regular".into(),"Regular".into()),("italic".into(),"Italic".into()),("700".into(),"Bold".into()),("700italic".into(),"Bold italic".into())]);
    });
    let mut use_font = None;
    let mut download = None;
    let filtered = crate::google_fonts::search(&studio.google_catalog, &studio.google_query);
    let installed_fonts = crate::text::all_fonts_cached();
    ScrollArea::vertical()
        .id_salt("google-font-results")
        .max_height(180.0)
        .show(ui, |ui| {
            for font in filtered {
                ui.horizontal(|ui| {
                    ui.add(
                        eframe::egui::Label::new(RichText::new(&font.family).small()).truncate(),
                    );
                    let path =
                        crate::google_fonts::installed_path(&font.family, &studio.google_variant);
                    let installed = installed_fonts.iter().any(|face| face.path == path);
                    ui.with_layout(Layout::right_to_left(eframe::egui::Align::Center), |ui| {
                        if installed {
                            if super::svg_paint::control(ui, owner.with(("use", &font.family, &studio.google_variant)), &format!("Use Google font {} {}", font.family, studio.google_variant), |ui| ui.small_button("Use")).clicked() {
                                use_font = Some((font.family.clone(), path));
                            }
                        } else {
                          let response = super::svg_paint::control(ui, owner.with(("download", &font.family, &studio.google_variant)), &format!("Download Google font {} {}", font.family, studio.google_variant), |ui| ui
                            .add_enabled(!downloading, eframe::egui::Button::new("Download"))
                          );
                          if response.clicked() {
                            download = Some(font.clone());
                          }
                          affected.push(response);
                        }
                    });
                });
            }
        });
    if let Some((family, path)) = use_font {
        let chosen = path.to_string_lossy().into_owned();
        studio.patch_type(|text| text.font = chosen.clone());
        studio.text_font = chosen;
        studio.google_status = format!("Using {family}");
    }
    if let Some(font) = download {
        let variant = studio.google_variant.clone();
        let document = studio.swap_id.clone();
        let selection = studio.selection.clone();
        studio.google_status = format!("Downloading {}…", font.family);
        jobs::start(ui.ctx(), DOWNLOAD, move || {
            let path =
                crate::google_fonts::download(&font.family, &variant, std::slice::from_ref(&font))?;
            Ok(DownloadedFont {
                path,
                family: font.family,
                document,
                selection,
            })
        });
    }
    if !studio.google_status.is_empty() {
        let status = ui.label(
            RichText::new(&studio.google_status)
                .small()
                .color(fg_weak()),
        );
        ui.ctx().accesskit_node_builder(status.id, |node| { node.set_live(eframe::egui::accesskit::Live::Polite); node.set_live_atomic(); });
        for response in &affected { ui.ctx().accesskit_node_builder(response.id, |node| node.push_described_by(status.id.accesskit_id())); }
    }
}

fn motion_studio(ui: &mut Ui, studio: &mut Studio) {
    super::motion_presets::inspector(ui, studio);
    section_gap(ui);
    eframe::egui::CollapsingHeader::new("Keyframe controls").show(ui, |ui| motion_keys(ui, studio));
}

fn inspector_slider<N: eframe::egui::emath::Numeric>(
    ui: &mut Ui,
    label: &str,
    value: &mut N,
    range: std::ops::RangeInclusive<N>,
    suffix: &str,
) -> bool {
    ui.push_id(label, |ui| {
        let speed = (range.end().to_f64() - range.start().to_f64()) / 500.0;
        let (mut changed, label_id) = ui
            .horizontal(|ui| {
                ui.with_layout(Layout::right_to_left(eframe::egui::Align::Center), |ui| {
                    ui.spacing_mut().button_padding.x = 4.0;
                    let number = ui.add_sized(
                        [80.0, 24.0],
                        eframe::egui::DragValue::new(value)
                            .range(range.clone())
                            .speed(speed)
                            .max_decimals(3)
                            .suffix(suffix),
                    );
                    let label_id = ui
                        .with_layout(Layout::left_to_right(eframe::egui::Align::Center), |ui| {
                            ui.add(
                                eframe::egui::Label::new(
                                    RichText::new(label).small().color(fg_weak()),
                                )
                                .truncate(),
                            )
                            .id
                        })
                        .inner;
                    (
                        input_history::changed(number.labelled_by(label_id)),
                        label_id,
                    )
                })
                .inner
            })
            .inner;
        changed |= ui
            .scope(|ui| {
                // Keep the track inside the panel; the label and editable value
                // live above it so their widths never push the panel outward.
                ui.spacing_mut().slider_width = ui.available_width();
                input_history::changed(
                    ui.add(Slider::new(value, range).show_value(false))
                        .labelled_by(label_id),
                )
            })
            .inner;
        changed
    })
    .inner
}

fn motion_keys(ui: &mut Ui, studio: &mut Studio) {
    studio.with_field_history(ui, "motion-keys", |ui, studio| {
        motion_keys_contents(ui, studio)
    });
}

fn motion_keys_contents(ui: &mut Ui, studio: &mut Studio) {
    ui.label(
        RichText::new("Rest pose is Design. Keys are offsets.")
            .small()
            .color(fg_weak()),
    );
    ui.add_space(4.0);
    if studio.selection.is_empty() {
        ui.label(
            RichText::new("Select an object or image, then Key.")
                .small()
                .color(fg_weak()),
        );
        return;
    }
    let Some(target) = studio
        .primary()
        .and_then(|(li, id)| crate::motion::target(&studio.doc, li, id))
    else {
        return;
    };
    let id = target.id;
    let pose = studio.live_pose(id);
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(format!("X  {:+.1}", pose.dx))
                .small()
                .monospace(),
        );
        if ui.small_button("Key").clicked() {
            studio.key_prop(id, crate::motion::Prop::X, pose.dx);
        }
    });
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(format!("Y  {:+.1}", pose.dy))
                .small()
                .monospace(),
        );
        if ui.small_button("Key").clicked() {
            studio.key_prop(id, crate::motion::Prop::Y, pose.dy);
        }
    });
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(format!("R  {:+.1}°", pose.rotation.to_degrees()))
                .small()
                .monospace(),
        );
        if ui.small_button("Key").clicked() {
            studio.key_prop(id, crate::motion::Prop::Rotation, pose.rotation);
        }
    });
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(format!("S  {:.2}", pose.scale))
                .small()
                .monospace(),
        );
        if ui.small_button("Key").clicked() {
            studio.key_prop(id, crate::motion::Prop::Scale, pose.scale);
        }
    });
    let mut op = pose.opacity.unwrap_or(target.opacity);
    if inspector_slider(ui, "Opacity", &mut op, 0.0..=1.0, "") {
        studio.key_prop(id, crate::motion::Prop::Opacity, op);
    }
    for (property, label, value) in [
        (
            crate::motion::Prop::StrokeReveal,
            "Draw stroke",
            pose.stroke_reveal,
        ),
        (crate::motion::Prop::FillReveal, "Fill up", pose.fill_reveal),
    ] {
        if let Some(value) = value {
            let mut percent = value * 100.0;
            if inspector_slider(ui, label, &mut percent, 0.0..=100.0, "%") {
                studio.key_prop(id, property, percent / 100.0);
            }
        }
    }
    let paint = studio
        .doc
        .find_shape(studio.primary().map(|p| p.0).unwrap_or(0), id)
        .map(|shape| {
            let gradient = match &shape.style.fill {
                crate::document::Fill::Gradient(gradient) => {
                    Some(gradient.angle(shape.geom.bbox()))
                }
                _ => shape
                    .style
                    .stroke
                    .as_ref()
                    .and_then(|stroke| stroke.gradient.as_ref())
                    .map(|gradient| gradient.angle(shape.geom.bbox())),
            };
            let dash = shape.style.stroke.as_ref().map(|stroke| {
                let (on, off) = stroke.dash.unwrap_or((0.0, 0.0));
                (on, off, stroke.dash_offset)
            });
            (gradient, dash)
        });
    if let Some((rest, _)) = paint
        && let Some(rest) = rest
    {
        let mut shown = rest + pose.gradient_angle.unwrap_or(0.0);
        if inspector_slider(ui, "Gradient", &mut shown, -360.0..=360.0, "°") {
            studio.key_prop(id, crate::motion::Prop::GradientAngle, shown - rest);
        }
    }
    if let Some((_, Some((rest_dash, rest_gap, rest_length)))) = paint {
        let mut dash = rest_dash + pose.dash.unwrap_or(0.0);
        let mut gap = rest_gap + pose.gap.unwrap_or(0.0);
        let mut length = rest_length + pose.dash_length.unwrap_or(0.0);
        if inspector_slider(ui, "Dash", &mut dash, 0.0..=400.0, " px") {
            studio.key_prop(id, crate::motion::Prop::Dash, dash - rest_dash);
        }
        if inspector_slider(ui, "Gap", &mut gap, 0.0..=400.0, " px") {
            studio.key_prop(id, crate::motion::Prop::Gap, gap - rest_gap);
        }
        if inspector_slider(ui, "Length", &mut length, -400.0..=400.0, " px") {
            studio.key_prop(id, crate::motion::Prop::DashLength, length - rest_length);
        }
    }
}

fn artboard_transform(ui: &mut Ui, studio: &mut Studio) {
    use crate::app::ArtworkResize;
    ui.label("Artwork on resize");
    let mut scale=studio.artboard_options.resize==ArtworkResize::Scale;
    let mut movement=studio.artboard_options.resize==ArtworkResize::Move;
    if ui.checkbox(&mut scale,"Scale artwork").changed() {studio.artboard_options.resize=if scale {ArtworkResize::Scale}else{ArtworkResize::Keep};}
    if ui.checkbox(&mut movement,"Move artwork").on_hover_text("Move with the artboard origin, keeping artwork size").changed() {studio.artboard_options.resize=if movement {ArtworkResize::Move}else{ArtworkResize::Keep};}
    ui.checkbox(&mut studio.artboard_options.clone,"Clone artwork").on_hover_text("Keep originals and transform copies");
    ui.small("Start resize: Ctrl board only · Alt toggle clone");
    ui.small("Double-click outside: size dialog or wrap object");
    ui.separator();
    let Some(id) = studio.artboard_sel.first().copied() else {
        ui.label(
            RichText::new("Draw or click an artboard")
                .small()
                .color(fg_weak()),
        );
        if ui.button("Wrap selection in artboard").clicked() {
            studio.wrap_selection_artboard();
        }
        return;
    };
    let Some(idx) = studio.doc.artboards.iter().position(|a| a.id == id) else {
        return;
    };
    let mut a = studio.doc.artboards[idx].clone();
    if studio.artboard_rename.as_ref().map(|(i, _)| *i) == Some(id) {
        if let Some((_, buf)) = studio.artboard_rename.as_mut() {
            let r = ui.add(eframe::egui::TextEdit::singleline(buf).desired_width(160.0));
            if r.lost_focus() {
                let name = buf.trim().to_string();
                studio.artboard_rename = None;
                if !name.is_empty() {
                    a.name = studio.doc.unique_artboard_name(&name);
                    let mut after = studio.doc.artboards.clone();
                    if let Some(slot) = after.iter_mut().find(|x| x.id == id) {
                        *slot = a.clone();
                    }
                    studio.commit_artboards(after);
                }
            }
        }
    } else if ui.button(&a.name).on_hover_text("Rename").clicked() {
        studio.artboard_rename = Some((id, a.name.clone()));
    }
    let mut x = a.origin.x;
    let mut y = a.origin.y;
    let mut w = a.size.x;
    let mut h = a.size.y;
    let mut deg = a.rotation.to_degrees();
    let mut changed = bounds_fields(ui, &mut x, &mut y, &mut w, &mut h, 8.0);
    changed |= number_field(ui, "Rotation", &mut deg, -180.0..=180.0, "°");
    if changed {
        a.origin = crate::geom::Pt::new(x, y);
        a.size = crate::geom::Pt::new(w, h);
        a.rotation = crate::document::Artboard::snap_rotation(deg.to_radians());
        studio.preview_inspector_artboard(a.clone());
    }
    if !ui.input(|i| i.pointer.primary_down()) {
        studio.finish_inspector_artboard();
    }
    ui.horizontal(|ui| {
        if ui.button("Clone").clicked() {
            studio.clone_artboard(id);
        }
        if ui.button("Delete").clicked() {
            studio.delete_artboards();
        }
    });
}

fn raster_transform(ui: &mut Ui, studio: &mut Studio, li: usize) {
    super::psd_smart::inspector(ui,studio,li);
    let Some(layer) = studio.doc.layers.get(li) else {
        return;
    };
    let Some((origin, size, rot)) = layer.kind.raster_xform() else {
        return;
    };
    let id = layer.id;
    let pose = studio.live_pose(id);
    let bounds = layer.kind.raster_bounds().unwrap();
    let shown = pose.map_bounds(bounds);
    let motion = studio.is_motion();
    let mut x = if motion { shown.min.x } else { origin.x };
    let mut y = if motion { shown.min.y } else { origin.y };
    let local_size = crate::geom::Pt::new(size.x * pose.scale * pose.width_scale, size.y * pose.scale * pose.height_scale);
    let mut w = if motion { local_size.x } else { size.x };
    let mut h = if motion { local_size.y } else { size.y };
    let mut deg = (rot + pose.rotation).to_degrees();
    let mut changed = bounds_fields(ui, &mut x, &mut y, &mut w, &mut h, 1.0);
    changed |= number_field(ui, "Rotation", &mut deg, -180.0..=180.0, "°");
    if changed && studio.doc.layer_editable(li) {
        if motion {
            let mut after = studio.doc.motion.clone();
            for (prop, value, previous) in [
                (crate::motion::Prop::X, pose.dx + x - shown.min.x, pose.dx),
                (crate::motion::Prop::Y, pose.dy + y - shown.min.y, pose.dy),
                (
                    crate::motion::Prop::Width,
                    pose.width_scale * w / local_size.x.max(1.0),
                    pose.width_scale,
                ),
                (
                    crate::motion::Prop::Height,
                    pose.height_scale * h / local_size.y.max(1.0),
                    pose.height_scale,
                ),
                (
                    crate::motion::Prop::Rotation,
                    deg.to_radians() - rot,
                    pose.rotation,
                ),
            ] {
                if (value - previous).abs() > 1e-5 {
                    after.set_key(
                        id,
                        prop,
                        studio.playhead,
                        value,
                        crate::motion::Ease::EaseInOut,
                    );
                }
            }
            studio.commit_motion(after);
            return;
        }
        studio.commit(crate::document::Cmd::SetRasterXform {
            layer: li,
            before: (origin, size, rot),
            after: (
                crate::geom::Pt::new(x, y),
                crate::geom::Pt::new(w, h),
                deg.to_radians(),
            ),
        });
    }
}

fn edit_shape_geometry(studio: &mut Studio, layer: usize, id: u64, edit: impl FnOnce(&mut Geom)) {
    let Some(shape) = studio.doc.find_shape(layer, id) else {
        return;
    };
    let mut after = shape.geom.clone();
    edit(&mut after);
    studio.commit(crate::document::Cmd::SetGeom {
        layer,
        id,
        before: shape.geom.clone(),
        after,
        rot_before: shape.rotation,
        rot_after: shape.rotation,
    });
}

fn transform_studio(ui: &mut Ui, studio: &mut Studio, title: bool) {
    studio.with_field_history(ui, "transform", |ui, studio| {
        transform_studio_contents(ui, studio, title)
    });
}

fn transform_studio_contents(ui: &mut Ui, studio: &mut Studio, title: bool) {
    if title {
        heading(ui, "Layout");
    }
    if studio.tool == Tool::Artboard
        || (studio.selection.is_empty() && !studio.artboard_sel.is_empty())
    {
        artboard_transform(ui, studio);
        return;
    }
    if studio.selection.is_empty() {
        match studio.tool {
            Tool::Polygon => {
                number_field(ui, "Sides", &mut studio.polygon_sides, 3..=12, "");
            }
            Tool::Star => {
                number_field(ui, "Points", &mut studio.star_points, 3..=12, "");
                number_field(ui, "Inner radius", &mut studio.star_inner, 0.15..=0.8, "");
            }
            Tool::Rect => {
                number_field(
                    ui,
                    "Corner radius",
                    &mut studio.rect_radius,
                    0.0..=80.0,
                    " px",
                );
            }
            _ => {}
        }
        return;
    }
    if super::shared_transform::mixed(ui, studio) { return; }
    let Some((layer, id)) = studio.primary() else {
        return;
    };
    if id == crate::document::RASTER_ID {
        raster_transform(ui, studio, layer);
        return;
    }
    let Some(shape) = studio.doc.find_shape(layer, id) else {
        return;
    };
    let bounds = shape.world_bbox();
    let motion = studio.is_motion();
    let pose = studio.live_pose(id);
    let shown = if motion {
        pose.map_bounds(bounds)
    } else {
        bounds
    };
    let layout_frame = shape.layout.frame;
    let rotation = shape.rotation;
    let polygon = if let Geom::Polygon { sides, .. } = shape.geom {
        Some(sides)
    } else {
        None
    };
    let star = if let Geom::Star { points, inner, .. } = shape.geom {
        Some((points, inner))
    } else {
        None
    };
    let radius = if let Geom::Rect { radius, .. } = shape.geom {
        Some(radius)
    } else {
        None
    };
    let compound = matches!(&shape.geom, Geom::Poly { contours, .. } if contours.len() > 1);
    let mut x = shown.min.x;
    let mut y = shown.min.y;
    let mut width = shown.width().max(1.0);
    let mut height = shown.height().max(1.0);
    let changed = bounds_fields(ui, &mut x, &mut y, &mut width, &mut height, 1.0);
    if changed {
        if motion {
            let dx = x - shown.min.x;
            let dy = y - shown.min.y;
            let sx = width / shown.width().max(1.0);
            let sy = height / shown.height().max(1.0);
            let mut after = studio.doc.motion.clone();
            let t = studio.playhead;
            if dx.abs() > 0.01 {
                after.set_key(
                    id,
                    crate::motion::Prop::X,
                    t,
                    pose.dx + dx,
                    crate::motion::Ease::EaseInOut,
                );
            }
            if dy.abs() > 0.01 {
                after.set_key(
                    id,
                    crate::motion::Prop::Y,
                    t,
                    pose.dy + dy,
                    crate::motion::Ease::EaseInOut,
                );
            }
            if (sx - 1.0).abs() > 0.001 {
                after.set_key(
                    id,
                    crate::motion::Prop::Width,
                    t,
                    pose.width_scale * sx,
                    crate::motion::Ease::EaseInOut,
                );
            }
            if (sy - 1.0).abs() > 0.001 {
                after.set_key(
                    id,
                    crate::motion::Prop::Height,
                    t,
                    pose.height_scale * sy,
                    crate::motion::Ease::EaseInOut,
                );
            }
            studio.commit_motion(after);
        } else {
            let destination = crate::geom::Bounds {
                min: crate::geom::Pt::new(x, y),
                max: crate::geom::Pt::new(x + width, y + height),
            };
            if layout_frame {
                studio.set_layout_frame_bounds(layer, id, destination);
            } else {
                studio.transform_shape_with_text_scale(layer, id, destination);
            }
        }
    }
    let mut degrees = if motion {
        (rotation + pose.rotation).to_degrees()
    } else {
        rotation.to_degrees()
    };
    let mut rotation_changed = false;
    ui.columns(2, |columns| {
        rotation_changed =
            number_field(&mut columns[0], "Rotate", &mut degrees, -180.0..=180.0, "°");
        super::selection::flip_icons(&mut columns[1], studio);
    });
    if rotation_changed && let Some(shape) = studio.doc.find_shape(layer, id) {
        if motion {
            studio.key_prop(
                id,
                crate::motion::Prop::Rotation,
                degrees.to_radians() - rotation,
            );
        } else {
            studio.commit(crate::document::Cmd::SetGeom {
                layer,
                id,
                before: shape.geom.clone(),
                after: shape.geom.clone(),
                rot_before: rotation,
                rot_after: degrees.to_radians(),
            });
        }
    }

    if let Some(mut count) = polygon
        && number_field(ui, "Sides", &mut count, 3..=16, "")
    {
        edit_shape_geometry(studio, layer, id, |geometry| {
            if let Geom::Polygon { sides, .. } = geometry {
                *sides = count;
            }
        });
    }
    if let Some((mut count, mut radius)) = star {
        let mut changed = false;
        let mut inner_percent = radius * 100.0;
        ui.columns(2, |columns| {
            changed |= number_field(&mut columns[0], "Points", &mut count, 3..=16, "");
            changed |= number_field(
                &mut columns[1],
                "Inner",
                &mut inner_percent,
                15.0..=85.0,
                "%",
            );
        });
        radius = inner_percent / 100.0;
        if changed {
            edit_shape_geometry(studio, layer, id, |geometry| {
                if let Geom::Star { points, inner, .. } = geometry {
                    *points = count;
                    *inner = radius;
                }
            });
        }
    }
    if let Some(mut value) = radius
        && number_field(ui, "Corner radius", &mut value, 0.0..=200.0, " px")
    {
        edit_shape_geometry(studio, layer, id, |geometry| {
            if let Geom::Rect { radius, .. } = geometry {
                *radius = value;
            }
        });
    }
    if studio.selection.len() >= 2 {
        ui.add_space(4.0);
        if ui
            .small_button("Group")
            .on_hover_text("Ctrl+G · create an editable layer group")
            .clicked()
        {
            studio.group_selected();
        }
        ui.horizontal_wrapped(|ui| {
            for operation in crate::boolean::BoolOp::all() {
                if ui.small_button(operation.name()).clicked() {
                    studio.apply_boolean_multi(operation);
                }
            }
            if ui
                .small_button("Compound")
                .on_hover_text("Ctrl+8")
                .clicked()
            {
                studio.combine_selected();
            }
        });
    }
    if studio.selection_has_path()
        && ui
            .small_button("Simplify")
            .on_hover_text("Fit each selected path. Corners stay sharp and straight lines stay straight.")
            .clicked()
    {
        studio.simplify_selection();
    }
    if compound
        && ui
            .small_button("Release compound")
            .on_hover_text("Ctrl+Shift+8")
            .clicked()
    {
        studio.release_compound();
    }
}

#[derive(Clone)]
struct EffectBlendOwners {
    expected: crate::filter::FilterStack,
    instance: eframe::egui::Id,
    widgets: Vec<eframe::egui::Id>,
}

impl EffectBlendOwners {
    /// Retire only the native blend selectors of a replaced effect list.
    /// Args: UI context. Returns nothing; unrelated numeric controls retain their owners.
    fn retire(&mut self, ctx: &eframe::egui::Context) {
        if let Some(focused) = ctx.memory(|memory| memory.focused()) && self.widgets.contains(&focused) {
            ctx.memory_mut(|memory| memory.surrender_focus(focused));
        }
        self.instance = super::keyboard_controls::draft_id(ctx, "effect-blend");
        self.widgets.clear();
    }
}

fn fx_stack_editor(ui: &mut Ui, stack: &mut crate::filter::FilterStack, salt: &str) {
    let original = stack.clone();
    let key = ui.make_persistent_id(("effect-blend-owners", salt));
    let mut owners = ui.data(|data| data.get_temp::<EffectBlendOwners>(key)).unwrap_or_else(|| EffectBlendOwners {
        expected: original.clone(), instance: super::keyboard_controls::draft_id(ui.ctx(), "effect-blend"), widgets: Vec::new(),
    });
    if owners.expected != original { owners.retire(ui.ctx()); }
    owners.widgets.clear();
    let mut structural = false;
    ui.horizontal(|ui| {
        ui.checkbox(&mut stack.enabled, "Enabled");
        ComboBox::from_id_salt(format!("fx-add-{salt}"))
            .selected_text("Add")
            .width(72.0)
            .show_ui(ui, |ui| {
                for (name, make) in crate::filter::Fx::catalog() {
                    if ui.selectable_label(false, *name).clicked() {
                        stack.items.push(make());
                        structural = true;
                    }
                }
            });
    });
    let mut remove = None;
    let mut bump = None;
    for (i, fx) in stack.items.iter_mut().enumerate() {
        let effect_name = fx.name();
        ui.add_space(4.0);
        ui.group(|ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new(fx.name()).small().strong().color(accent()));
                ui.with_layout(Layout::right_to_left(eframe::egui::Align::Center), |ui| {
                    if ui.small_button("−").clicked() {
                        remove = Some(i);
                    }
                    if ui.small_button("↓").clicked() {
                        bump = Some((i, 1));
                    }
                    if ui.small_button("↑").clicked() {
                        bump = Some((i, -1));
                    }
                });
            });
            if let Some((blend, opacity)) = fx.appearance_mut() {
                ui.horizontal(|ui| {
                    let label = ui.label("Blend");
                    let mut response = super::fields::native_scope(ui, owners.instance.with(i), |ui| ComboBox::from_id_salt(("effect-blend", salt, i))
                        .selected_text(blend.name())
                        .show_ui(ui, |ui| {
                            for mode in Blend::ALL {
                                ui.selectable_value(blend, mode, mode.name());
                            }
                        }).response).inner.labelled_by(label.id);
                    super::keyboard_controls::selection_keys(ui, &mut response, &format!("{effect_name} effect {} blend", i + 1), blend,
                        &Blend::ALL.into_iter().map(|mode| (mode, mode.name().into())).collect::<Vec<_>>());
                    owners.widgets.push(response.id);
                });
                let mut percent = *opacity * 100.;
                if inspector_slider(ui, "Opacity", &mut percent, 0.0..=100.0, "%") {
                    *opacity = percent / 100.;
                }
            }
            match fx {
                crate::filter::Fx::Blur { std } => {
                    inspector_slider(ui, "Blur", std, 0.0..=80.0, "");
                }
                crate::filter::Fx::Shadow {
                    dx,
                    dy,
                    blur,
                    color,
                    ..
                }
                | crate::filter::Fx::InnerShadow {
                    dx,
                    dy,
                    blur,
                    color,
                    ..
                } => {
                    inspector_slider(ui, "Offset X", dx, -80.0..=80.0, "");
                    inspector_slider(ui, "Offset Y", dy, -80.0..=80.0, "");
                    inspector_slider(ui, "Blur", blur, 0.0..=80.0, "");
                    ui.horizontal(|ui| {
                        super::color_picker::color_edit(ui, ("effect-color", salt, i), color);
                    });
                }
                crate::filter::Fx::OuterGlow {
                    blur,
                    spread,
                    color,
                    ..
                }
                | crate::filter::Fx::InnerGlow {
                    blur,
                    choke: spread,
                    color,
                    ..
                } => {
                    inspector_slider(ui, "Blur", blur, 0.0..=80.0, "");
                    inspector_slider(ui, "Spread / choke", spread, 0.0..=64.0, "");
                    super::color_picker::color_edit(ui, ("effect-color", salt, i), color);
                }
                crate::filter::Fx::ColorOverlay { color, .. } => {
                    super::color_picker::color_edit(ui, ("effect-color", salt, i), color);
                }
                crate::filter::Fx::Offset { dx, dy } => {
                    inspector_slider(ui, "Offset X", dx, -200.0..=200.0, "");
                    inspector_slider(ui, "Offset Y", dy, -200.0..=200.0, "");
                }
                crate::filter::Fx::Morphology { erode, radius } => {
                    ui.checkbox(erode, "Erode");
                    inspector_slider(ui, "Radius", radius, 0.0..=40.0, "");
                }
                crate::filter::Fx::Saturate { amount } => {
                    inspector_slider(ui, "Amount", amount, 0.0..=3.0, "");
                }
                crate::filter::Fx::HueRotate { degrees } => {
                    inspector_slider(ui, "Angle", degrees, -180.0..=180.0, "");
                }
                crate::filter::Fx::Brightness { amount } => {
                    inspector_slider(ui, "Amount", amount, 0.0..=3.0, "");
                }
                crate::filter::Fx::Contrast { amount } => {
                    inspector_slider(ui, "Amount", amount, 0.0..=3.0, "");
                }
                crate::filter::Fx::Invert { amount } => {
                    inspector_slider(ui, "amount", amount, 0.0..=1.0, "");
                }
                crate::filter::Fx::ColorMatrix { values } => {
                    inspector_slider(ui, "m00", &mut values[0], -2.0..=2.0, "");
                }
                crate::filter::Fx::Turbulence {
                    fractal,
                    base,
                    octaves,
                    seed,
                } => {
                    ui.checkbox(fractal, "fractalNoise");
                    inspector_slider(ui, "baseFrequency", base, 0.001..=0.5, "");
                    inspector_slider(ui, "numOctaves", octaves, 1..=8, "");
                    inspector_slider(ui, "seed", seed, 0..=9999, "");
                }
                crate::filter::Fx::Displacement { scale, x_ch, y_ch } => {
                    inspector_slider(ui, "scale", scale, 0.0..=120.0, "");
                    inspector_slider(ui, "xChannel", x_ch, 0..=3, "");
                    inspector_slider(ui, "yChannel", y_ch, 0..=3, "");
                }
                crate::filter::Fx::AppleGlass(glass) => {
                    inspector_slider(ui, "Refraction", &mut glass.ior, 1.0..=2.5, "");
                    inspector_slider(ui, "Color fringing", &mut glass.chromatic, 0.0..=0.2, "");
                    inspector_slider(ui, "Edge", &mut glass.edge_width, 1.0..=400.0, "");
                    inspector_slider(ui, "Highlight", &mut glass.spec_power, 1.0..=128.0, "");
                    inspector_slider(
                        ui,
                        "Highlight strength",
                        &mut glass.spec_intensity,
                        0.0..=2.0,
                        "",
                    );
                    inspector_slider(ui, "Rim light", &mut glass.fresnel_intensity, 0.0..=2.0, "");
                    inspector_slider(ui, "Blur", &mut glass.blur, 0.0..=5.0, "");
                    inspector_slider(ui, "Split", &mut glass.split, 0.0..=1.0, "");
                    inspector_slider(ui, "Split angle", &mut glass.split_angle, 0.0..=360.0, "");
                    inspector_slider(ui, "Falloff", &mut glass.falloff, 0.0..=300.0, "");
                }
            }
            match fx {
                crate::filter::Fx::Shadow {
                    knockout, spread, ..
                } => {
                    ui.checkbox(knockout, "Knockout");
                    inspector_slider(ui, "Spread", spread, 0.0..=64.0, "");
                }
                crate::filter::Fx::InnerShadow { choke, .. } => {
                    inspector_slider(ui, "Choke", choke, 0.0..=64.0, "");
                }
                crate::filter::Fx::InnerGlow { source, .. } => {
                    ui.horizontal(|ui| {
                        ui.label("Source");
                        ui.selectable_value(source, crate::filter::GlowSource::Edge, "Edge");
                        ui.selectable_value(source, crate::filter::GlowSource::Center, "Center");
                    });
                }
                _ => {}
            }
        });
    }
    if let Some(i) = remove {
        stack.items.remove(i);
        structural = true;
    }
    if let Some((i, dir)) = bump {
        let j = i as i32 + dir;
        if j >= 0 && (j as usize) < stack.items.len() {
            stack.items.swap(i, j as usize);
            structural = true;
        }
    }
    if *stack != original {
        stack.legacy_composite = false;
    }
    if structural { owners.retire(ui.ctx()); }
    owners.expected = stack.clone();
    ui.data_mut(|data| data.insert_temp(key, owners));
}

fn fx_studio(ui: &mut Ui, studio: &mut Studio) {
    studio.with_field_history(ui, "effects", |ui, studio| fx_studio_contents(ui, studio));
}

fn fx_studio_contents(ui: &mut Ui, studio: &mut Studio) {
    super::svg_effect::show(ui, studio);
    let group_active = studio
        .active_layer
        .and_then(|i| studio.doc.layers.get(i))
        .is_some_and(|l| l.is_group);
    let shape_target = studio
        .primary()
        .filter(|_| !group_active)
        .and_then(|(li, id)| {
            if id == crate::document::RASTER_ID {
                None
            } else {
                studio.doc.find_shape(li, id).map(|_| (li, id))
            }
        });
    if let Some((li, id)) = shape_target {
        ui.label(RichText::new("Object").small().color(fg_weak()));
        let mut stack = studio
            .doc
            .find_shape(li, id)
            .map(|s| s.filters.clone())
            .unwrap_or_default();
        fx_stack_editor(ui, &mut stack, &format!("obj-{id}"));
        studio.commit_shape_filters(li, id, stack);
        ui.add_space(6.0);
        ui.separator();
        ui.label(RichText::new("Layer").small().color(fg_weak()));
    }
    let Some(li) = studio.active_layer else {
        ui.label(RichText::new("Select a layer").small().color(fg_weak()));
        return;
    };
    if li >= studio.doc.layers.len() {
        return;
    }
    if !studio.layer_unlocked(li) {
        ui.disable();
    }
    let layer = &studio.doc.layers[li];
    let mut fill = layer.fill_opacity * 100.;
    let before = layer.fill_opacity;
    let mut interior = layer.blend_interior;
    let before_interior = interior;
    if number_field(ui, "Layer fill opacity", &mut fill, 0.0..=100.0, "%") {
        studio.commit(crate::document::Cmd::SetFillOpacity {
            layer: li,
            id: None,
            before,
            after: fill / 100.,
        });
    }
    if ui
        .checkbox(&mut interior, "Blend layer interior effects as group")
        .changed()
    {
        studio.commit(crate::document::Cmd::SetBlendInterior {
            layer: li,
            id: None,
            before: before_interior,
            after: interior,
        });
    }
    let mut stack = studio.doc.layers[li].filters.clone();
    fx_stack_editor(ui, &mut stack, "layer");
    if stack != studio.doc.layers[li].filters {
        studio.commit_filters(li, stack);
    }
}

fn trace_studio(ui: &mut Ui, studio: &mut Studio) {
    heading(ui, "Trace");
    ui.label(
        RichText::new("Raster to vector on the active pixel layer.")
            .small()
            .color(fg_weak()),
    );
    inspector_slider(ui, "Colors", &mut studio.trace_opts.colors, 1..=12, "");
    if studio.trace_opts.colors <= 1 {
        inspector_slider(
            ui,
            "Threshold",
            &mut studio.trace_opts.threshold,
            0.05..=0.95,
            "",
        );
    }
    inspector_slider(
        ui,
        "Smoothness",
        &mut studio.trace_opts.smoothness,
        0.2..=8.0,
        "",
    );
    inspector_slider(
        ui,
        "Min area",
        &mut studio.trace_opts.min_area,
        1.0..=64.0,
        "",
    );
    ui.checkbox(&mut studio.trace_opts.ignore_white, "Ignore white");
    ui.add_space(4.0);
    if ui.button("Trace to vector").clicked() {
        studio.trace_active_raster();
    }
}

fn brush_studio(ui: &mut Ui, studio: &mut Studio) {
    if matches!(studio.tool, Tool::Fill | Tool::Wand) {
        heading(ui, studio.tool.label());
        number_field(ui, "Tolerance", &mut studio.fill_tolerance, 0.0..=180.0, "");
        return;
    }
    if !matches!(
        studio.tool,
        Tool::Brush | Tool::Eraser | Tool::Clone | Tool::Heal | Tool::Smudge
    ) {
        return;
    }
    heading(
        ui,
        match studio.tool {
            Tool::Heal => "Healing brush",
            Tool::Clone => "Clone",
            _ => "Brush",
        },
    );
    if matches!(studio.tool, Tool::Brush | Tool::Eraser) {
        super::brushes::inspector(ui, studio);
    }
    number_field(ui, "Size", &mut studio.brush.size, 1.0..=2048.0, " px");
    if studio.tool == Tool::Smudge {
        let mut strength = studio.brush.flow * 100.0;
        number_field(ui, "Strength", &mut strength, 5.0..=100.0, "%");
        studio.brush.flow = strength / 100.0;
        return;
    }
    let mut hardness = studio.brush.hardness * 100.0;
    let mut opacity = studio.brush.opacity * 100.0;
    let mut flow = studio.brush.flow * 100.0;
    ui.columns(2, |columns| {
        let round_edge = !matches!(studio.tool, Tool::Brush | Tool::Eraser) || studio.brushes.selected.is_none();
        columns[0].add_enabled_ui(round_edge, |ui| {
            number_field(ui, "Edge", &mut hardness, 0.0..=100.0, "%");
        }).response.on_hover_text("Sampled tips include their own edge.");
        number_field(&mut columns[1], "Opacity", &mut opacity, 5.0..=100.0, "%");
    });
    studio.brush.hardness = hardness / 100.0;
    studio.brush.opacity = opacity / 100.0;
    number_field(ui, "Flow", &mut flow, 5.0..=100.0, "%");
    studio.brush.flow = flow / 100.0;
    let mut spacing = studio.brush.spacing * 100.0;
    number_field(ui, "Spacing", &mut spacing, 1.0..=10000.0, "%");
    studio.brush.spacing = spacing / 100.0;
    if matches!(studio.tool, Tool::Brush | Tool::Eraser) {
        let mut angle = studio.brush.angle.to_degrees();
        number_field(ui, "Angle", &mut angle, -180.0..=180.0, "°");
        studio.brush.angle = angle.to_radians();
        ui.checkbox(
            &mut studio.brush.follow_direction,
            "Follow stroke direction",
        );
        ui.horizontal(|ui| {
            ui.label("Pen pressure");
            ui.checkbox(&mut studio.brush.pressure_size, "Size");
            ui.checkbox(&mut studio.brush.pressure_flow, "Flow");
        });
        eframe::egui::CollapsingHeader::new("Scatter and randomness").show(ui, |ui| {
            let mut scatter = studio.brush.scatter * 100.0;
            number_field(ui, "Scatter", &mut scatter, 0.0..=1000.0, "%");
            studio.brush.scatter = scatter / 100.0;
            let mut jitter = studio.brush.angle_jitter.to_degrees();
            number_field(ui, "Angle jitter", &mut jitter, 0.0..=180.0, "°");
            studio.brush.angle_jitter = jitter.to_radians();
            let mut size = studio.brush.size_jitter * 100.0;
            number_field(ui, "Size jitter", &mut size, 0.0..=99.0, "%");
            studio.brush.size_jitter = size / 100.0;
            let mut opacity = studio.brush.opacity_jitter * 100.0;
            number_field(ui, "Opacity jitter", &mut opacity, 0.0..=100.0, "%");
            studio.brush.opacity_jitter = opacity / 100.0;
        });
    }
    if studio.tool == Tool::Clone {
        ui.checkbox(&mut studio.clone_aligned, "Aligned")
            .on_hover_text("Keep the source offset. Off repeats the same source point.");
    }
    super::masking::retouch_hint(ui, studio);
}

fn paint_color_studio(ui: &mut Ui, studio: &mut Studio) {
    heading(ui, "Color");
    let mut color = match studio.style.fill {
        Fill::Solid(color) if studio.tool == Tool::Fill => color,
        _ => studio.brush.color,
    };
    ui.horizontal(|ui| {
        if super::color_picker::color_edit(ui, "paint", &mut color) {
            studio.brush.color = color;
            studio.style.fill = Fill::Solid(studio.brush.color);
        }
        ui.label(RichText::new(color.hex()).small().color(fg_weak()));
    });
}

fn foreground_color_studio(ui: &mut Ui, studio: &mut Studio) {
    let owner = studio.swap_id.clone();
    ui.push_id((owner, "paint-foreground"), |ui| {
        let mut color = match studio.style.fill {
            Fill::Solid(color) if studio.tool == Tool::Fill => color,
            _ => studio.brush.color,
        };
        let before = color;
        ui.horizontal(|ui| {
            super::color_picker::color_edit_named(ui, "foreground", &mut color, "Foreground color");
            ui.label("Foreground");
        });
        super::color_picker::inline(ui, &mut color);
        if color != before {
            studio.brush.color = color;
            studio.style.fill = Fill::Solid(color);
            studio.push_recent(color);
        }
    });
}

fn object_name(
    ui: &mut Ui,
    name: &str,
    width: f32,
    strong: bool,
    draggable: bool,
) -> eframe::egui::Response {
    let (rect, response) = ui.allocate_exact_size(
        vec2(width, 28.0),
        if draggable {
            eframe::egui::Sense::click_and_drag()
        } else {
            eframe::egui::Sense::click()
        },
    );
    if response.hovered() {
        ui.painter()
            .rect_filled(rect, 4.0, crate::ui::theme::bg_widget_hover());
    }
    let text = RichText::new(name)
        .size(12.0)
        .color(if strong { fg() } else { fg_weak() });
    let galley = eframe::egui::WidgetText::from(text).into_galley(
        ui,
        Some(eframe::egui::TextWrapMode::Truncate),
        width - 4.0,
        eframe::egui::TextStyle::Small,
    );
    let position = eframe::egui::pos2(rect.left() + 2.0, rect.center().y - galley.size().y / 2.0);
    ui.painter().galley(position, galley, fg());
    response.widget_info(|| {
        eframe::egui::WidgetInfo::labeled(
            eframe::egui::WidgetType::SelectableLabel,
            ui.is_enabled(),
            name,
        )
    });
    response
}

fn object_icon(ui: &mut Ui, icon: &str, color: Color32) {
    let (rect, _) = ui.allocate_exact_size(vec2(20.0, 24.0), eframe::egui::Sense::hover());
    ui.painter().text(
        rect.center(),
        eframe::egui::Align2::CENTER_CENTER,
        icon,
        icons::font(15.0),
        color,
    );
}

pub(super) fn layers_studio(ui: &mut Ui, studio: &mut Studio) {
    studio.with_field_history(ui, "layers", |ui, studio| {
        layers_studio_contents(ui, studio)
    });
}

fn layers_studio_contents(ui: &mut Ui, studio: &mut Studio) {
    // Reveal a newly selected object once; a manual collapse stays collapsed.
    let primary = studio.primary();
    let reveal_id = ui.make_persistent_id("reveal-selection");
    let previous = ui
        .data(|d| d.get_temp::<Option<(usize, u64)>>(reveal_id))
        .flatten();
    if previous != primary {
        if let Some((li, _)) = primary
            && let Some(layer) = studio.doc.layers.get(li)
        {
            studio.layer_expanded.insert(layer.id);
            for ancestor in studio.doc.layer_ancestors(li) {
                studio.layer_expanded.insert(studio.doc.layers[ancestor].id);
            }
        }
        ui.data_mut(|d| d.insert_temp(reveal_id, primary));
    }

    if studio.pending_item_mask.is_some() {
        ui.label(
            RichText::new("Click the target for this mask")
                .small()
                .color(accent()),
        );
        if ui.small_button("Cancel mask").clicked() {
            studio.pending_item_mask = None;
        }
    }
    let mut delete_item = None;
    let mut artwork_action = None;
    let mut mask_item = None;
    let mut selection_mask_item = None;
    let mut remove_mask_item = None;
    let mut enter_shape = None;
    let mut rows = Vec::new();
    let mut activate = None;
    let mut mask_action = None;
    let mut vis = None;
    let mut lock = None;
    let mut up = None;
    let mut down = None;
    let mut start_rename = None;
    let mut toggle_expand = None;
    let mut pick_shape: Option<(usize, u64)> = None;
    let mut vis_shape: Option<(usize, u64)> = None;
    let mut lock_shape: Option<(usize, u64)> = None;
    let mut start_shape_rename: Option<(usize, u64)> = None;
    let mut shape_up: Option<(usize, usize)> = None;
    let mut shape_down: Option<(usize, usize)> = None;
    let mut flip_shape: Option<(usize, u64, bool)> = None;
    let n = studio.doc.layers.len();
    for i in (0..n).rev() {
        let ancestors = studio.doc.layer_ancestors(i);
        if ancestors
            .iter()
            .any(|&a| !studio.layer_expanded.contains(&studio.doc.layers[a].id))
        {
            continue;
        }
        let indent = (ancestors.len() as f32 * 12.0).min(ui.available_width()*0.3);
        super::fields::native_scope(ui,eframe::egui::Id::new(("layer-row",&studio.swap_id,studio.doc.layers[i].id)), |ui| {
            let active = studio.active_layer == Some(i);
            let header = Frame::new()
                .fill(if studio.selected_layer == Some(studio.doc.layers[i].id) {
                    accent_soft()
                } else if active {
                    bg_widget()
                } else {
                    Color32::TRANSPARENT
                })
                .corner_radius(5.0)
                .inner_margin(Margin::symmetric(3, 2))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        if !studio.layer_ancestors_unlocked(i) {
                            ui.disable();
                        }
                        ui.spacing_mut().item_spacing.x = 2.0;
                        ui.add_space(indent);
                        let layer = &studio.doc.layers[i];
                        let expanded = studio.layer_expanded.contains(&layer.id);
                        let has_children = layer.is_group
                            || layer.kind.shapes().is_some_and(|shapes| !shapes.is_empty())
                            || layer.kind.is_placed_raster();
                        if has_children {
                            if icons::tiny_icon(
                                ui,
                                if expanded {
                                    ph::CARET_DOWN
                                } else {
                                    ph::CARET_RIGHT
                                },
                                "Show objects",
                                false,
                            ) {
                                toggle_expand = Some(layer.id);
                            }
                        } else {
                            ui.add_space(22.0);
                        }
                        if layer.is_group {object_icon(ui,ph::FOLDER_OPEN,fg_weak());} else {
                            let thumbnail=super::pane_previews::thumbnail(ui,studio,i,None);
                            if thumbnail.clicked()||layer_drag::source(&thumbnail,studio,i){activate=Some(i);}
                        }
                        if layer.filters.active() { ui.label(RichText::new("fx").small().color(accent())); }
                        let name_width = (ui.available_width()
                            - if layer.mask.is_some() { 74.0 } else { 50.0 })
                        .max(42.0);
                        if studio.layer_rename.as_ref().map(|(index, _)| *index) == Some(i) {
                            if let Some((_, buffer)) = studio.layer_rename.as_mut() {
                                let focus_id=eframe::egui::Id::new(("focus-layer-rename",&studio.swap_id,layer.id));
                                let first=ui.data_mut(|data|data.remove_temp::<bool>(focus_id)).unwrap_or(false);
                                let response = ui.add(
                                    eframe::egui::TextEdit::singleline(buffer)
                                        .id(focus_id.with("edit"))
                                        .desired_width(name_width)
                                        .font(eframe::egui::TextStyle::Small),
                                );
                                let response=super::agent::named(ui,response,"Rename layer");
                                if first {response.request_focus();}
                                if response.has_focus() && ui.input(|input|input.key_pressed(eframe::egui::Key::Escape)) {
                                    studio.layer_rename=None;response.surrender_focus();
                                } else if !first && (response.lost_focus() || (response.has_focus() && ui.input(|input|input.key_pressed(eframe::egui::Key::Enter)))) {
                                    response.surrender_focus();start_rename=Some(usize::MAX);
                                }
                            }
                        } else {
                            let response = object_name(
                                ui,
                                &layer.name,
                                name_width,
                                studio.doc.layer_visible(i),
                                true,
                            );
                            if response.clicked() || layer_drag::source(&response, studio, i) {
                                activate = Some(i);
                            }
                            if response.double_clicked() && studio.layer_unlocked(i) {
                                start_rename = Some(i);
                            }
                            response
                                .on_hover_text(format!(
                                    "{} · {} · drag between rows to reorder, or onto a group to nest",
                                    layer.name,
                                    if layer.is_group {
                                        "Group"
                                    } else {
                                        layer.kind.tag()
                                    }
                                ))
                                .context_menu(|ui| {
                                    if ui.add_enabled(studio.layer_unlocked(i), eframe::egui::Button::new("Delete layer")).clicked() {
                                        delete_item=Some((i,None)); ui.close();
                                    }
                                    if let Some(action) = super::masking::artwork_menu(ui,studio,i,None) { artwork_action=Some((i,None,action)); }
                                    if ui.button("Use as mask…").clicked() { mask_item=Some((i,None)); ui.close(); }
                                    ui.separator();
                                    if ui
                                        .add_enabled(
                                            studio.layer_unlocked(i),
                                            eframe::egui::Button::new("Rename"),
                                        )
                                        .clicked()
                                    {
                                        start_rename = Some(i);
                                        ui.close();
                                    }
                                    if ui
                                        .add_enabled(
                                            studio.layer_unlocked(i)
                                                && studio.layer_sibling(i, true).is_some(),
                                            eframe::egui::Button::new("Move up"),
                                        )
                                        .clicked()
                                    {
                                        up = Some(i);
                                        ui.close();
                                    }
                                    if ui
                                        .add_enabled(
                                            studio.layer_unlocked(i)
                                                && studio.layer_sibling(i, false).is_some(),
                                            eframe::egui::Button::new("Move down"),
                                        )
                                        .clicked()
                                    {
                                        down = Some(i);
                                        ui.close();
                                    }
                                    ui.separator();
                                    ui.menu_button("Layer mask", |ui| {
                                        if let Some(action) = super::masking::menu(ui, studio, i) {
                                            mask_action = Some((i, action));
                                        }
                                    });
                                });
                        }
                        if layer.mask.is_some()
                            && icons::tiny_icon(
                                ui,
                                ph::SELECTION,
                                "Paint layer mask",
                                active && studio.paint_mask,
                            )
                        {
                            mask_action = Some((i, super::masking::Action::Edit(true)));
                        }
                        if icons::tiny_icon(
                            ui,
                            if studio.doc.layer_visible(i) {
                                ph::EYE
                            } else {
                                ph::EYE_SLASH
                            },
                            if layer.visible && !studio.doc.layer_visible(i) {
                                "Hidden by parent group"
                            } else {
                                "Layer visibility"
                            },
                            !studio.doc.layer_visible(i),
                        ) {
                            vis = Some(i);
                        }
                        if icons::tiny_icon(
                            ui,
                            if layer.locked {
                                ph::LOCK
                            } else {
                                ph::LOCK_OPEN
                            },
                            "Lock layer",
                            layer.locked,
                        ) {
                            lock = Some(i);
                        }
                    });
                })
                .response
                .rect;
            if !studio.doc.layers[i].is_group
                && studio.layer_expanded.contains(&studio.doc.layers[i].id)
            {
                if studio.doc.layers[i].kind.is_placed_raster() {
                    ui.horizontal(|ui| {
                        if !studio.doc.layer_editable(i) {
                            ui.disable();
                        }
                        ui.add_space(indent + 28.0);
                        if ui
                            .selectable_label(
                                studio.selection.contains(&(i, crate::document::RASTER_ID)),
                                &studio.doc.layers[i].name,
                            )
                            .clicked()
                        {
                            pick_shape = Some((i, crate::document::RASTER_ID));
                        }
                    });
                }
                let objects_unlocked = studio.layer_unlocked(i);
                let objects_editable = studio.doc.layer_editable(i);
                if let Some(shapes) = studio.doc.layers[i].kind.shapes() {
                    let count = shapes.len();
                    let spacing = ui.spacing().item_spacing.y;
                    let pitch = layer_rows::HEIGHT + spacing;
                    let rename = studio.shape_rename.as_ref()
                        .filter(|(layer, _, _)| *layer == i).map(|(_, id, _)| *id);
                    let visible = layer_rows::snapshot(shapes, ui.cursor().top(), ui.clip_rect(), spacing, rename);
                    let mut next_row = 0;
                    for shape in visible {
                        let index = shape.index;
                        let skipped = shape.visual_index - next_row;
                        if skipped > 0 {
                            ui.add_space(skipped as f32 * pitch);
                            ui.skip_ahead_auto_ids(skipped);
                        }
                        next_row = shape.visual_index + 1;
                        super::fields::native_scope(ui,eframe::egui::Id::new(("object-row",&studio.swap_id,studio.doc.layers[i].id,shape.id)), |ui| {
                            Frame::new()
                                .fill(if studio.selection.contains(&(i, shape.id)) {
                                    accent_soft()
                                } else {
                                    Color32::TRANSPARENT
                                })
                                .corner_radius(5.0)
                                .inner_margin(Margin::symmetric(3, 1))
                                .show(ui, |ui| {
                                    ui.horizontal(|ui| {
                                        ui.set_min_height(layer_rows::HEIGHT - 2.0);
                                        if !objects_unlocked {
                                            ui.disable();
                                        }
                                        ui.spacing_mut().item_spacing.x = 2.0;
                                        ui.add_space(indent + 24.0);
                                        let thumbnail=super::pane_previews::thumbnail(ui,studio,i,Some(shape.id));
                                        if thumbnail.clicked()&&objects_editable&&!shape.locked {pick_shape=Some((i,shape.id));}
                                        if layer_drag::object_source(&thumbnail,studio,i,shape.id){pick_shape=Some((i,shape.id));}
                                        let name_width = (ui.available_width() - 50.0).max(42.0);
                                        if studio
                                            .shape_rename
                                            .as_ref()
                                            .map(|(layer, id, _)| (*layer, *id))
                                            == Some((i, shape.id))
                                        {
                                            if let Some((_, _, buffer)) =
                                                studio.shape_rename.as_mut()
                                            {
                                                let response = ui.add(
                                                    eframe::egui::TextEdit::singleline(buffer)
                                                        .desired_width(name_width)
                                                        .font(eframe::egui::TextStyle::Small),
                                                );
                                                if response.lost_focus() {
                                                    start_shape_rename = Some((usize::MAX, 0));
                                                }
                                            }
                                        } else {
                                            if shape.overflow { ui.colored_label(eframe::egui::Color32::from_rgb(240,55,75), "+").on_hover_text("Text overflows its frame"); }
                                            let name_width = (ui.available_width() - 50.0).max(42.0);
                                            let guide_name;
                                            let name = if shape.guide {
                                                guide_name = format!("Guide · {}", shape.name);
                                                &guide_name
                                            } else {
                                                &shape.name
                                            };
                                            if shape.effects { ui.label(RichText::new("fx").small().color(accent())); }
                                            let response = object_name(
                                                ui,
                                                name,
                                                name_width - if shape.effects { 24. } else { 0. },
                                                studio.selection.contains(&(i, shape.id))
                                                    && shape.visible,
                                                true,
                                            );
                                            rows.push((layer_drag::TreeRow::Object(i, shape.id), response.rect));
                                            if layer_drag::object_source(&response, studio, i, shape.id)
                                                && !studio.selection.contains(&(i, shape.id))
                                            {
                                                pick_shape = Some((i, shape.id));
                                            }
                                            if response.clicked()
                                                && objects_editable
                                                && shape.visible
                                                && !shape.locked
                                            {
                                                pick_shape = Some((i, shape.id));
                                            }
                                            if response.double_clicked() && !shape.locked {
                                                enter_shape = Some((i, shape.id));
                                            }
                                            response.on_hover_text(&shape.name).context_menu(
                                                |ui| {
                                                    if ui.add_enabled(objects_editable && !shape.locked, eframe::egui::Button::new("Delete object")).clicked() {
                                                        delete_item=Some((i,Some(shape.id))); ui.close();
                                                    }
                                                    if let Some(action) = super::masking::artwork_menu(ui,studio,i,Some(shape.id)) { artwork_action=Some((i,Some(shape.id),action)); }
                                                    if ui.button("Use as mask…").clicked() { mask_item=Some((i,Some(shape.id))); ui.close(); }
                                                    if ui.add_enabled(objects_editable && !shape.locked && studio.pixel_sel.is_some(), eframe::egui::Button::new("Mask from selection")).clicked() {
                                                        selection_mask_item=Some((i,shape.id)); ui.close();
                                                    }
                                                    if shape.masked && ui.button("Remove object mask").clicked() { remove_mask_item=Some((i,shape.id)); ui.close(); }
                                                    ui.separator();
                                                    if ui.button("Rename").clicked() {
                                                        start_shape_rename = Some((i, shape.id));
                                                        ui.close();
                                                    }
                                                    ui.separator();
                                                    if let Some(horizontal) =
                                                        super::selection::transform_menu(
                                                            ui,
                                                            objects_editable
                                                                && shape.visible
                                                                && !shape.locked
                                                                && !shape.text
                                                                && (!shape.guide
                                                                    || studio
                                                                        .doc
                                                                        .ruler
                                                                        .guides_visible),
                                                        )
                                                    {
                                                        flip_shape =
                                                            Some((i, shape.id, horizontal));
                                                    }
                                                    ui.separator();
                                                    if ui
                                                        .add_enabled(
                                                            index + 1 < count,
                                                            eframe::egui::Button::new("Move up"),
                                                        )
                                                        .clicked()
                                                    {
                                                        shape_up = Some((i, index));
                                                        ui.close();
                                                    }
                                                    if ui
                                                        .add_enabled(
                                                            index > 0,
                                                            eframe::egui::Button::new("Move down"),
                                                        )
                                                        .clicked()
                                                    {
                                                        shape_down = Some((i, index));
                                                        ui.close();
                                                    }
                                                },
                                            );
                                        }
                                        if icons::tiny_icon(
                                            ui,
                                            if shape.visible {
                                                ph::EYE
                                            } else {
                                                ph::EYE_SLASH
                                            },
                                            "Object visibility",
                                            !shape.visible,
                                        ) {
                                            vis_shape = Some((i, shape.id));
                                        }
                                        if icons::tiny_icon(
                                            ui,
                                            if shape.locked {
                                                ph::LOCK
                                            } else {
                                                ph::LOCK_OPEN
                                            },
                                            "Lock object",
                                            shape.locked,
                                        ) {
                                            lock_shape = Some((i, shape.id));
                                        }
                                    });
                                });
                        });
                    }
                    if next_row < count {
                        ui.add_space((count - next_row) as f32 * pitch - spacing);
                        ui.skip_ahead_auto_ids(count - next_row);
                    }
                }
            }
            rows.push((layer_drag::TreeRow::Layer(i), header));
        });
    }
    let drop = layer_drag::drop_target(ui, studio, &rows);
    if let Some(i) = start_rename {
        if i == usize::MAX {
            if let Some((idx, name)) = studio.layer_rename.take()
                && let Some(l) = studio.doc.layers.get(idx)
            {
                let trimmed = name.trim().to_string();
                if !trimmed.is_empty() && trimmed != l.name {
                    studio.commit(crate::document::Cmd::SetLayerMeta {
                        index: idx,
                        name: trimmed,
                        visible: l.visible,
                        locked: l.locked,
                        opacity: l.opacity,
                        blend: l.blend,
                        before: (l.name.clone(), l.visible, l.locked, l.opacity, l.blend),
                    });
                }
            }
        }
    }
    if let Some(i) = activate {
        if studio.apply_pending_item_mask(i, None) {
        } else if studio.persona == Persona::Pixel && ui.input(|i| i.modifiers.ctrl) {
            studio.select_item_outline(i, None);
        } else {
            let previous = studio.selection.clone();
            studio.activate_layer_tree(i);
            if ui.input(|i| i.modifiers.shift) {
                if studio.selection.is_empty() && studio.doc.layers[i].kind.pixels().is_some() {
                    studio.selection.push((i, crate::document::RASTER_ID));
                }
                for hit in previous {
                    if !studio.selection.contains(&hit) {
                        studio.selection.push(hit);
                    }
                }
                studio.selected_layer = None;
            }
        }
    }
    if let Some(index)=start_rename.filter(|index|*index!=usize::MAX) {
        if let Some(layer)=studio.doc.layers.get(index) {
            studio.layer_rename=Some((index,layer.name.clone()));
            ui.data_mut(|data|data.insert_temp(eframe::egui::Id::new(("focus-layer-rename",&studio.swap_id,layer.id)),true));
        }
    }
    if let Some(i) = vis {
        let l = &studio.doc.layers[i];
        studio.commit(crate::document::Cmd::SetLayerMeta {
            index: i,
            name: l.name.clone(),
            visible: !l.visible,
            locked: l.locked,
            opacity: l.opacity,
            blend: l.blend,
            before: (l.name.clone(), l.visible, l.locked, l.opacity, l.blend),
        });
        studio
            .selection
            .retain(|(li, _)| studio.doc.layer_editable(*li));
    }
    if let Some(i) = lock {
        let l = &studio.doc.layers[i];
        studio.commit(crate::document::Cmd::SetLayerMeta {
            index: i,
            name: l.name.clone(),
            visible: l.visible,
            locked: !l.locked,
            opacity: l.opacity,
            blend: l.blend,
            before: (l.name.clone(), l.visible, l.locked, l.opacity, l.blend),
        });
        studio
            .selection
            .retain(|(li, _)| studio.doc.layer_editable(*li));
    }
    if let Some(i) = up {
        studio.move_layer_tree(i, true);
    }
    if let Some(i) = down {
        studio.move_layer_tree(i, false);
    }
    if let Some(id) = toggle_expand
        && !studio.layer_expanded.remove(&id)
    {
        studio.layer_expanded.insert(id);
    }
    if let Some((li, id)) = pick_shape {
        if studio.apply_pending_item_mask(li, Some(id)) {
        } else if studio.persona == Persona::Pixel && ui.input(|i| i.modifiers.ctrl) {
            studio.select_item_outline(li, Some(id));
        } else if enter_shape == Some((li, id)) {
            studio.enter_group_item((li, id));
        } else {
            if studio.active_layer != Some(li) {
                studio.paint_mask = false;
            }
            let hits = studio.selection_for_hit((li, id));
            if studio.individual_object != Some((li, id)) {
                studio.individual_object = None;
            }
            studio.selected_layer = None;
            if ui.input(|i| i.modifiers.shift) {
                if hits.iter().all(|hit| studio.selection.contains(hit)) {
                    studio.selection.retain(|hit| !hits.contains(hit));
                } else {
                    for hit in hits {
                        if !studio.selection.contains(&hit) {
                            studio.selection.push(hit);
                        }
                    }
                }
            } else {
                studio.selection = hits;
            }
            studio.active_layer = Some(li);
            studio.artboard_sel.clear();
        }
    }
    if let Some((li, id, horizontal)) = flip_shape {
        if !studio.selection.contains(&(li, id)) {
            studio.commit_type_edit();
            studio.end_deform(true);
            studio.selected_layer = None;
            studio.selection = vec![(li, id)];
            studio.node_sel.clear();
            studio.artboard_sel.clear();
            if studio.active_layer != Some(li) {
                studio.paint_mask = false;
            }
            studio.active_layer = Some(li);
        }
        studio.flip_selection(horizontal);
    }
    if let Some((li, id)) = vis_shape
        && let Some(s) = studio.doc.find_shape(li, id)
    {
        studio.commit(crate::document::Cmd::SetShapeMeta {
            layer: li,
            id,
            name: s.name.clone(),
            visible: !s.visible,
            locked: s.locked,
            before: (s.name.clone(), s.visible, s.locked),
        });
    }
    if let Some((li, id)) = lock_shape
        && let Some(s) = studio.doc.find_shape(li, id)
    {
        studio.commit(crate::document::Cmd::SetShapeMeta {
            layer: li,
            id,
            name: s.name.clone(),
            visible: s.visible,
            locked: !s.locked,
            before: (s.name.clone(), s.visible, s.locked),
        });
    }
    if let Some((li, id)) = start_shape_rename {
        if li == usize::MAX {
            if let Some((l, sid, name)) = studio.shape_rename.take()
                && let Some(s) = studio.doc.find_shape(l, sid)
            {
                let trimmed = name.trim().to_string();
                if !trimmed.is_empty() && trimmed != s.name {
                    studio.commit(crate::document::Cmd::SetShapeMeta {
                        layer: l,
                        id: sid,
                        name: trimmed,
                        visible: s.visible,
                        locked: s.locked,
                        before: (s.name.clone(), s.visible, s.locked),
                    });
                }
            }
        } else if let Some(s) = studio.doc.find_shape(li, id) {
            studio.shape_rename = Some((li, id, s.name.clone()));
        }
    }
    if let Some((li, from)) = shape_up
        && let Some(shapes) = studio.doc.layers.get(li).and_then(|l| l.kind.shapes())
        && from + 1 < shapes.len()
    {
        studio.commit(crate::document::Cmd::ReorderShape {
            layer: li,
            from,
            to: from + 1,
        });
    }
    if let Some((li, from)) = shape_down
        && from > 0
    {
        studio.commit(crate::document::Cmd::ReorderShape {
            layer: li,
            from,
            to: from - 1,
        });
    }
    if let Some((index, action)) = mask_action {
        action.run(studio, index);
    }
    if let Some((li,id,action)) = artwork_action { action.run(studio,li,id); }
    if let Some((li, id)) = mask_item {
        studio.begin_item_mask(li, id);
    }
    if let Some((li, id)) = selection_mask_item {
        studio.mask_object_from_selection(li, id);
    }
    if let Some((li, id)) = remove_mask_item {
        studio.remove_object_mask(li, id);
    }
    if let Some((li, id)) = delete_item {
        if let Some(id) = id {
            studio.delete_object_item(li, id);
        } else {
            studio.delete_layer_tree(li);
        }
    } else if let Some(drop) = drop {
        drop.apply(studio);
    }
}

#[test]
fn inspector_keeps_its_width_across_frames_and_personas() {
    for scene in [
        "design",
        "design-tools",
        "type",
        "pixel",
        "motion",
        "masking",
        "healing",
    ] {
        let ctx = eframe::egui::Context::default();
        crate::ui::theme::apply(&ctx);
        let mut studio = Studio::new();
        crate::shots::apply(&mut studio, scene).unwrap();
        for _ in 0..8 {
            let mut width = 0.0;
            let mut output = ctx.run_ui(
                eframe::egui::RawInput {
                    screen_rect: Some(eframe::egui::Rect::from_min_size(
                        eframe::egui::Pos2::ZERO,
                        vec2(960.0, 640.0),
                    )),
                    ..Default::default()
                },
                |ui| {
                    right_panel(ui, &mut studio);
                    width = 960.0 - ui.available_width();
                },
            );
            output.textures_delta.clear();
            assert!(width <= 304.0, "{scene} inspector grew to {width}px");
        }
    }
}

#[test]
fn expanded_keyframes_and_effects_respect_the_resized_panel() {
    use crate::ui::workspace::{Place, Workspace};
    use eframe::egui::{self, Event, Modifiers, PointerButton, Pos2, Rect};

    fn frame(
        ctx: &egui::Context,
        studio: &mut Studio,
        layout: &mut Workspace,
        time: &mut f64,
        events: Vec<Event>,
    ) -> (f32, Vec<(String, Rect)>) {
        *time += 0.05;
        let mut width = 0.0;
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1600.0, 1400.0))),
                time: Some(*time),
                events,
                ..Default::default()
            },
            |ui| {
                let outer = ui.available_rect_before_wrap();
                let center = layout.show(
                    ui,
                    "resized-inspector-fixture",
                    &[("properties", "Properties")],
                    |ui, _| inspector_content(ui, studio),
                );
                width = outer.right() - center.right();
            },
        );
        fn texts(shape: &egui::Shape, out: &mut Vec<(String, Rect)>) {
            match shape {
                egui::Shape::Text(text) => out.push((
                    text.galley.text().to_owned(),
                    text.galley.rect.translate(text.pos.to_vec2()),
                )),
                egui::Shape::Vec(shapes) => shapes.iter().for_each(|s| texts(s, out)),
                _ => {}
            }
        }
        let mut labels = Vec::new();
        for shape in &output.shapes {
            texts(&shape.shape, &mut labels);
        }
        output.textures_delta.clear();
        (width, labels)
    }

    fn open_section(
        ctx: &egui::Context,
        studio: &mut Studio,
        layout: &mut Workspace,
        time: &mut f64,
        title: &str,
    ) {
        let mut labels = frame(ctx, studio, layout, time, vec![]).1;
        for _ in 0..8 {
            if labels.iter().any(|(text, _)| text == title) {
                break;
            }
            labels = frame(
                ctx,
                studio,
                layout,
                time,
                vec![
                    Event::PointerMoved(egui::pos2(1490., 800.)),
                    Event::MouseWheel {
                        unit: egui::MouseWheelUnit::Point,
                        delta: vec2(0., -300.),
                        modifiers: Modifiers::NONE,
                        phase: egui::TouchPhase::Move,
                    },
                ],
            )
            .1;
            for _ in 0..4 {
                labels = frame(ctx, studio, layout, time, vec![]).1;
            }
        }
        let pos = labels
            .iter()
            .find(|(text, _)| text == title)
            .unwrap_or_else(|| panic!("Missing {title}"))
            .1
            .center();
        frame(ctx, studio, layout, time, vec![Event::PointerMoved(pos)]);
        for pressed in [true, false] {
            frame(
                ctx,
                studio,
                layout,
                time,
                vec![Event::PointerButton {
                    pos,
                    button: PointerButton::Primary,
                    pressed,
                    modifiers: Modifiers::NONE,
                }],
            );
        }
        frame(ctx, studio, layout, time, vec![]);
        frame(ctx, studio, layout, time, vec![]);
    }

    for target_width in [256.0, 288.0, 420.0] {
        let ctx = egui::Context::default();
        crate::ui::theme::apply(&ctx);
        let mut layout = Workspace::preset(&[("properties", Place::Right)]);
        layout.groups[0].extent = target_width;
        let mut studio = Studio::new();
        studio.doc = crate::document::Document::new("Panel sizing", 1.0, 1.0, 72.0);
        let shape = crate::document::Shape::new(
            Geom::Rect {
                origin: crate::geom::Pt::ZERO,
                size: crate::geom::Pt::splat(100.0),
                radius: 0.0,
            },
            crate::document::Style::default(),
        );
        let id = shape.id;
        let mut layer = crate::document::Layer::vector("Artwork");
        layer.kind.shapes_mut().unwrap().push(shape);
        studio.doc.layers = vec![layer];
        studio.selection = vec![(0, id)];
        studio.active_layer = Some(0);
        studio.persona = Persona::Motion;
        let mut time = 0.0;
        frame(&ctx, &mut studio, &mut layout, &mut time, vec![]);
        open_section(
            &ctx,
            &mut studio,
            &mut layout,
            &mut time,
            "Keyframe controls",
        );
        for reveal in [0.0, 0.000123, 0.5000123, 0.99021, 1.0] {
            studio.pose_drag.insert(
                id,
                crate::motion::Pose {
                    opacity: Some(reveal),
                    stroke_reveal: Some(reveal),
                    fill_reveal: Some(reveal),
                    ..crate::motion::Pose::identity()
                },
            );
            let (width, labels) = frame(&ctx, &mut studio, &mut layout, &mut time, vec![]);
            assert!(
                (width - target_width).abs() <= 1.0,
                "Keyframes grew {target_width}px panel to {width}px at {reveal}"
            );
            for name in ["Opacity", "Draw stroke", "Fill up"] {
                let (_, rect) = labels.iter().find(|(text, _)| text == name).unwrap();
                assert!(rect.left() >= 1600.0 - width && rect.right() <= 1600.0);
            }
        }
        studio.pose_drag.clear();
        studio.persona = Persona::Design;
        open_section(&ctx, &mut studio, &mut layout, &mut time, "Effects");
        for (name, make) in crate::filter::Fx::catalog() {
            studio.doc.find_shape_mut(0, id).unwrap().filters.items = vec![make()];
            for _ in 0..3 {
                let (width, _) = frame(&ctx, &mut studio, &mut layout, &mut time, vec![]);
                assert!(
                    (width - target_width).abs() <= 1.0,
                    "{name} grew {target_width}px panel to {width}px"
                );
            }
        }
        studio.tool = Tool::Trace;
        studio.trace_opts.colors = 1;
        for _ in 0..3 {
            let (width, _) = frame(&ctx, &mut studio, &mut layout, &mut time, vec![]);
            assert!(
                (width - target_width).abs() <= 1.0,
                "Trace grew {target_width}px panel to {width}px"
            );
        }
    }
}

#[test]
fn area_text_inspector_keeps_compact_width_while_typing() {
    let ctx=eframe::egui::Context::default();crate::ui::theme::apply(&ctx);
    let mut studio=Studio::new();studio.active_layer=Some(1);studio.persona=Persona::Design;studio.text_px=25.;
    studio.place_area_text(crate::geom::Bounds::from_min_size(crate::geom::Pt::new(70.,100.),crate::geom::Pt::new(350.,250.)));
    studio.type_insert(&"Editable area text wraps and retains hidden source. ".repeat(12));
    for editing in [true,false] {
        if !editing {studio.commit_type_edit();}
        for _ in 0..8 {
            let mut width=0.;let mut output=ctx.run_ui(eframe::egui::RawInput{screen_rect:Some(eframe::egui::Rect::from_min_size(eframe::egui::Pos2::ZERO,vec2(960.,640.))),..Default::default()},|ui|{right_panel(ui,&mut studio);width=960.-ui.available_width();});
            output.textures_delta.clear();assert!(width<=304.,"area inspector grew to {width}px while editing={editing}");
        }
    }
}

#[cfg(test)]
mod sidebar_resize_tests {
    use super::*;
    use eframe::egui::{Event, Modifiers, PointerButton, Pos2, RawInput, Rect};

    #[test]
    fn canvas_hover_and_vertical_motion_cannot_resize_the_sidebar() {
        let ctx = eframe::egui::Context::default();
        crate::ui::theme::apply(&ctx);
        let mut studio = Studio::new();
        studio.persona = Persona::Pixel;
        studio.tool = Tool::Brush;
        let id = eframe::egui::Id::new("studios-width");
        let frame = |events, studio: &mut Studio| {
            let mut output = ctx.run_ui(
                RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1000., 800.))),
                    events,
                    ..Default::default()
                },
                |ui| right_panel(ui, studio),
            );
            output.textures_delta.clear();
            ctx.data_mut(|data| data.get_persisted::<f32>(id).unwrap())
        };
        assert_eq!(frame(vec![], &mut studio), 288.);
        for y in (20..780).step_by(20) {
            assert_eq!(
                frame(
                    vec![Event::PointerMoved(Pos2::new(350., y as f32))],
                    &mut studio
                ),
                288.
            );
        }
        frame(
            vec![Event::PointerMoved(Pos2::new(712., 400.))],
            &mut studio,
        );
        frame(
            vec![Event::PointerButton {
                pos: Pos2::new(712., 400.),
                button: PointerButton::Primary,
                pressed: true,
                modifiers: Modifiers::NONE,
            }],
            &mut studio,
        );
        let width = frame(
            vec![Event::PointerMoved(Pos2::new(672., 400.))],
            &mut studio,
        );
        assert!((width - 328.).abs() < 1.);
        frame(
            vec![Event::PointerButton {
                pos: Pos2::new(672., 400.),
                button: PointerButton::Primary,
                pressed: false,
                modifiers: Modifiers::NONE,
            }],
            &mut studio,
        );
        assert_eq!(
            frame(
                vec![Event::PointerMoved(Pos2::new(350., 700.))],
                &mut studio
            ),
            width
        );
    }
}

#[cfg(test)]
mod brush_spacing_review_tests {
    use super::*;

    #[test]
    fn inspector_retains_spacing_above_four_brush_sizes() {
        let ctx = eframe::egui::Context::default();
        crate::ui::theme::apply(&ctx);
        let mut studio = Studio::new();
        studio.persona = Persona::Pixel;
        studio.tool = Tool::Brush;
        studio.brush.spacing = 80.0;
        let mut output = ctx.run_ui(eframe::egui::RawInput { screen_rect: Some(eframe::egui::Rect::from_min_size(eframe::egui::Pos2::ZERO, vec2(600., 1200.))), ..Default::default() }, |ui| brush_studio(ui, &mut studio));
        output.textures_delta.clear();
        assert_eq!(studio.brush.spacing, 80.0);
    }
}
