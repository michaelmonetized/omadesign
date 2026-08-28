use crate::app::Studio;
use crate::color::{Blend, Rgba};
use crate::document::{Cap, Fill, Join, Stroke as DocStroke};
use crate::geom::Geom;
use crate::ui::theme::{ACCENT, ACCENT_DIM, BG_WIDGET, FG_WEAK};
use eframe::egui::{
    vec2, Color32, ComboBox, Layout, Panel, RichText, ScrollArea, Slider, Stroke, Ui,
};

pub fn right_panel(ui: &mut Ui, studio: &mut Studio) {
    Panel::right("studios")
        .resizable(true)
        .default_size(280.0)
        .size_range(240.0..=420.0)
        .show(ui, |ui| {
            ScrollArea::vertical().show(ui, |ui| {
                color_studio(ui, studio);
                ui.add_space(10.0);
                character_studio(ui, studio);
                ui.add_space(10.0);
                stroke_studio(ui, studio);
                ui.add_space(10.0);
                transform_studio(ui, studio);
                ui.add_space(10.0);
                brush_studio(ui, studio);
                ui.add_space(10.0);
                layers_studio(ui, studio);
            });
        });
}

fn section(ui: &mut Ui, title: &str) {
    ui.label(RichText::new(title).strong().size(12.0).color(ACCENT));
    ui.add_space(4.0);
}

fn color_studio(ui: &mut Ui, studio: &mut Studio) {
    section(ui, "Colour");
    ui.horizontal(|ui| {
        fill_well(ui, studio);
        stroke_well(ui, studio);
        ui.vertical(|ui| {
            ui.label(RichText::new("Fill / Stroke").small().color(FG_WEAK));
            if ui.small_button("Swap  X").clicked() {
                if let Fill::Solid(f) = studio.style.fill {
                    if let Some(st) = &studio.style.stroke {
                        let sc = st.color;
                        studio.set_fill(Fill::Solid(sc));
                        studio.set_stroke_color(f);
                    }
                }
            }
            if ui.small_button("None fill").clicked() {
                studio.set_fill(Fill::None);
            }
        });
    });
    hsv_picker(ui, studio);
    ui.horizontal(|ui| {
        ui.label("#");
        if ui
            .add(eframe::egui::TextEdit::singleline(&mut studio.hex_buf).desired_width(80.0))
            .lost_focus()
        {
            if let Some(c) = Rgba::parse_hex(&studio.hex_buf) {
                studio.set_fill(Fill::Solid(c));
                studio.brush.color = c;
            }
        }
    });
    ui.add_space(4.0);
    ui.horizontal_wrapped(|ui| {
        for c in studio.swatches.clone() {
            let (rect, resp) =
                ui.allocate_exact_size(vec2(16.0, 16.0), eframe::egui::Sense::click());
            ui.painter().rect_filled(rect, 2.0, c.to_egui());
            if resp.clicked() {
                studio.set_fill(Fill::Solid(c));
                studio.brush.color = c;
            }
            if resp.secondary_clicked() {
                studio.set_stroke_color(c);
            }
        }
    });
    if !studio.recent.is_empty() {
        ui.label(RichText::new("Recent").small().color(FG_WEAK));
        ui.horizontal_wrapped(|ui| {
            for c in studio.recent.clone() {
                let (rect, resp) =
                    ui.allocate_exact_size(vec2(16.0, 16.0), eframe::egui::Sense::click());
                ui.painter().rect_filled(rect, 2.0, c.to_egui());
                if resp.clicked() {
                    studio.set_fill(Fill::Solid(c));
                }
            }
        });
    }
    ui.horizontal(|ui| {
        let (a, b) = studio.gradient;
        swatch_btn(ui, a, "A");
        swatch_btn(ui, b, "B");
        if ui.small_button("Linear fill").clicked() {
            studio.set_fill(Fill::Linear {
                from: [0.0, 0.0],
                to: [1.0, 0.0],
                c0: studio.gradient.0,
                c1: studio.gradient.1,
            });
        }
        if ui.small_button("Radial fill").clicked() {
            studio.set_fill(Fill::Radial {
                c0: studio.gradient.0,
                c1: studio.gradient.1,
            });
        }
    });
}

fn swatch_btn(ui: &mut Ui, c: Rgba, tip: &str) {
    let (rect, resp) = ui.allocate_exact_size(vec2(22.0, 22.0), eframe::egui::Sense::click());
    ui.painter().rect_filled(rect, 3.0, c.to_egui());
    let _ = resp.on_hover_text(tip);
}

fn fill_well(ui: &mut Ui, studio: &mut Studio) {
    let c = match studio.style.fill {
        Fill::Solid(c) => c.to_egui(),
        Fill::Linear { c0, .. } | Fill::Radial { c0, .. } => c0.to_egui(),
        Fill::None => Color32::TRANSPARENT,
    };
    let (rect, _) = ui.allocate_exact_size(vec2(36.0, 36.0), eframe::egui::Sense::hover());
    ui.painter().rect_filled(rect, 4.0, BG_WIDGET);
    ui.painter().rect_filled(rect.shrink(4.0), 3.0, c);
    ui.painter().rect_stroke(
        rect,
        4.0,
        Stroke::new(1.0, ACCENT_DIM),
        eframe::egui::StrokeKind::Middle,
    );
}

fn stroke_well(ui: &mut Ui, studio: &mut Studio) {
    let c = studio
        .style
        .stroke
        .as_ref()
        .map(|s| s.color.to_egui())
        .unwrap_or(Color32::TRANSPARENT);
    let (rect, _) = ui.allocate_exact_size(vec2(28.0, 28.0), eframe::egui::Sense::hover());
    ui.painter().rect_stroke(
        rect.shrink(3.0),
        3.0,
        Stroke::new(3.0, c),
        eframe::egui::StrokeKind::Middle,
    );
}

fn hsv_picker(ui: &mut Ui, studio: &mut Studio) {
    let current = match studio.style.fill {
        Fill::Solid(c) => c,
        Fill::Linear { c0, .. } | Fill::Radial { c0, .. } => c0,
        Fill::None => studio.brush.color,
    };
    let [h, s, v, a] = current.to_hsva();
    let mut hh = h;
    let mut ss = s;
    let mut vv = v;
    let mut aa = a;
    let mut changed = false;
    changed |= ui.add(Slider::new(&mut hh, 0.0..=1.0).text("H")).changed();
    changed |= ui.add(Slider::new(&mut ss, 0.0..=1.0).text("S")).changed();
    changed |= ui.add(Slider::new(&mut vv, 0.0..=1.0).text("V")).changed();
    changed |= ui.add(Slider::new(&mut aa, 0.0..=1.0).text("A")).changed();
    if changed {
        let c = Rgba::from_hsva(hh, ss, vv, aa);
        studio.set_fill(Fill::Solid(c));
        studio.brush.color = c;
    }
}

fn stroke_studio(ui: &mut Ui, studio: &mut Studio) {
    section(ui, "Stroke");
    let mut width = studio.style.stroke.as_ref().map(|s| s.width).unwrap_or(0.0);
    if ui
        .add(Slider::new(&mut width, 0.0..=64.0).text("Width"))
        .changed()
    {
        let mut st = studio.style.stroke.clone().unwrap_or_default();
        st.width = width;
        studio.style.stroke = if width <= 0.01 {
            None
        } else {
            Some(st.clone())
        };
        apply_stroke(studio, studio.style.stroke.clone());
    }
    ui.horizontal(|ui| {
        for (cap, label) in [
            (Cap::Butt, "Butt"),
            (Cap::Round, "Round"),
            (Cap::Square, "Square"),
        ] {
            if ui
                .selectable_label(
                    studio.style.stroke.as_ref().map(|s| s.cap) == Some(cap),
                    label,
                )
                .clicked()
            {
                if let Some(st) = &mut studio.style.stroke {
                    st.cap = cap;
                }
                apply_stroke(studio, studio.style.stroke.clone());
            }
        }
    });
    ui.horizontal(|ui| {
        for (join, label) in [
            (Join::Miter, "Miter"),
            (Join::Round, "Round"),
            (Join::Bevel, "Bevel"),
        ] {
            if ui
                .selectable_label(
                    studio.style.stroke.as_ref().map(|s| s.join) == Some(join),
                    label,
                )
                .clicked()
            {
                if let Some(st) = &mut studio.style.stroke {
                    st.join = join;
                }
                apply_stroke(studio, studio.style.stroke.clone());
            }
        }
    });
    let dashed = studio.style.stroke.as_ref().and_then(|s| s.dash).is_some();
    let mut d = dashed;
    if ui.checkbox(&mut d, "Dashed").changed() {
        if let Some(st) = &mut studio.style.stroke {
            st.dash = if d { Some((6.0, 4.0)) } else { None };
        }
        apply_stroke(studio, studio.style.stroke.clone());
    }
}

fn apply_stroke(studio: &mut Studio, stroke: Option<DocStroke>) {
    for (li, id) in studio.selection.clone() {
        if let Some(s) = studio.doc.find_shape(li, id) {
            let mut after = s.style.clone();
            after.stroke = stroke.clone();
            studio.commit(crate::document::Cmd::SetStyle {
                layer: li,
                id,
                before: s.style.clone(),
                after,
            });
        }
    }
}

fn character_studio(ui: &mut Ui, studio: &mut Studio) {
    section(ui, "Character");
    let live = studio.selected_type();
    let mut font = live
        .as_ref()
        .map(|t| t.font.clone())
        .unwrap_or_else(|| studio.text_font.clone());
    let label = crate::text::label_for(&font);
    ComboBox::from_id_salt("character-font")
        .selected_text(label)
        .width(220.0)
        .show_ui(ui, |ui| {
            for f in crate::text::fonts() {
                let path = f.path.to_string_lossy().to_string();
                ui.selectable_value(&mut font, path, &f.name);
            }
        });
    if live
        .as_ref()
        .map(|t| t.font.clone())
        .unwrap_or_else(|| studio.text_font.clone())
        != font
    {
        let chosen = font.clone();
        studio.patch_type(|t| t.font = chosen);
    }

    let mut px = live.as_ref().map(|t| t.px).unwrap_or(studio.text_px);
    if ui
        .add(Slider::new(&mut px, 8.0..=400.0).text("Size"))
        .changed()
    {
        studio.patch_type(|t| t.px = px);
    }
    let mut track = live
        .as_ref()
        .map(|t| t.tracking)
        .unwrap_or(studio.text_tracking);
    if ui
        .add(Slider::new(&mut track, -40.0..=80.0).text("Tracking"))
        .changed()
    {
        studio.patch_type(|t| t.tracking = track);
    }
    let mut lead = live
        .as_ref()
        .map(|t| t.leading)
        .unwrap_or(studio.text_leading);
    if ui
        .add(Slider::new(&mut lead, 0.0..=400.0).text("Leading (0 auto)"))
        .changed()
    {
        studio.patch_type(|t| t.leading = lead);
    }

    ui.add_space(4.0);
    ui.label(RichText::new("OpenType").small().color(FG_WEAK));
    let mut kern = live.as_ref().map(|t| t.kern).unwrap_or(studio.text_kern);
    let mut liga = live.as_ref().map(|t| t.liga).unwrap_or(studio.text_liga);
    let mut tnum = live.as_ref().map(|t| t.tnum).unwrap_or(studio.text_tnum);
    let mut smcp = live.as_ref().map(|t| t.smcp).unwrap_or(studio.text_smcp);
    ui.horizontal(|ui| {
        if ui.checkbox(&mut kern, "Kerning").changed() {
            studio.patch_type(|t| t.kern = kern);
        }
        if ui.checkbox(&mut liga, "Ligatures").changed() {
            studio.patch_type(|t| t.liga = liga);
        }
    });
    ui.horizontal(|ui| {
        if ui.checkbox(&mut tnum, "Tabular figs").changed() {
            studio.patch_type(|t| t.tnum = tnum);
        }
        if ui.checkbox(&mut smcp, "Small caps").changed() {
            studio.patch_type(|t| t.smcp = smcp);
        }
    });
    if studio.type_edit.is_some() {
        ui.label(
            RichText::new("Typing on the canvas. Esc finishes.")
                .small()
                .color(ACCENT),
        );
    } else if live.is_none() {
        ui.label(
            RichText::new("Applies to the next type you place.")
                .small()
                .color(FG_WEAK),
        );
    } else {
        ui.label(
            RichText::new("Double-click the type to type into it.")
                .small()
                .color(FG_WEAK),
        );
    }
}

fn transform_studio(ui: &mut Ui, studio: &mut Studio) {
    section(ui, "Transform");
    if studio.selection.is_empty() {
        ui.label(RichText::new("Nothing selected").small().color(FG_WEAK));
        ui.label(
            RichText::new("Polygon sides / star points apply to the next shape you draw.")
                .small()
                .color(FG_WEAK),
        );
        ui.add(Slider::new(&mut studio.polygon_sides, 3..=12).text("Sides"));
        ui.add(Slider::new(&mut studio.star_points, 3..=12).text("Star points"));
        ui.add(Slider::new(&mut studio.star_inner, 0.15..=0.8).text("Star inner"));
        ui.add(Slider::new(&mut studio.rect_radius, 0.0..=80.0).text("Corner radius"));
        return;
    }
    let Some((li, id)) = studio.primary() else {
        return;
    };
    let Some(shape) = studio.doc.find_shape(li, id).cloned() else {
        return;
    };
    let b = shape.world_bbox();
    ui.label(
        RichText::new(format!(
            "{}  {:.0} × {:.0}  at {:.0}, {:.0}",
            shape.name,
            b.width(),
            b.height(),
            b.min.x,
            b.min.y
        ))
        .small()
        .color(FG_WEAK),
    );
    let mut op = shape.opacity;
    if ui
        .add(Slider::new(&mut op, 0.0..=1.0).text("Opacity"))
        .changed()
    {
        studio.commit(crate::document::Cmd::SetOpacity {
            layer: li,
            id,
            before: shape.opacity,
            after: op,
        });
    }
    match &shape.geom {
        Geom::Text(_) => {
            ui.label(
                RichText::new(
                    "Type lives in the Character studio. Double-click the words to edit.",
                )
                .small()
                .color(FG_WEAK),
            );
        }
        Geom::Polygon { sides, .. } => {
            let mut n = *sides;
            if ui.add(Slider::new(&mut n, 3..=16).text("Sides")).changed() {
                let mut g = shape.geom.clone();
                if let Geom::Polygon { sides, .. } = &mut g {
                    *sides = n;
                }
                studio.commit(crate::document::Cmd::SetGeom {
                    layer: li,
                    id,
                    before: shape.geom.clone(),
                    after: g,
                    rot_before: shape.rotation,
                    rot_after: shape.rotation,
                });
            }
        }
        Geom::Star { points, inner, .. } => {
            let mut n = *points;
            let mut inn = *inner;
            if ui.add(Slider::new(&mut n, 3..=16).text("Points")).changed()
                || ui
                    .add(Slider::new(&mut inn, 0.15..=0.85).text("Inner"))
                    .changed()
            {
                let mut g = shape.geom.clone();
                if let Geom::Star { points, inner, .. } = &mut g {
                    *points = n;
                    *inner = inn;
                }
                studio.commit(crate::document::Cmd::SetGeom {
                    layer: li,
                    id,
                    before: shape.geom.clone(),
                    after: g,
                    rot_before: shape.rotation,
                    rot_after: shape.rotation,
                });
            }
        }
        Geom::Rect { radius, .. } => {
            let mut r = *radius;
            if ui
                .add(Slider::new(&mut r, 0.0..=200.0).text("Radius"))
                .changed()
            {
                let mut g = shape.geom.clone();
                if let Geom::Rect { radius, .. } = &mut g {
                    *radius = r;
                }
                studio.commit(crate::document::Cmd::SetGeom {
                    layer: li,
                    id,
                    before: shape.geom.clone(),
                    after: g,
                    rot_before: shape.rotation,
                    rot_after: shape.rotation,
                });
            }
        }
        _ => {}
    }
    ui.horizontal(|ui| {
        if ui.small_button("L").on_hover_text("Align left").clicked() {
            studio.align_sel(crate::align::Align::Left);
        }
        if ui.small_button("C").clicked() {
            studio.align_sel(crate::align::Align::CenterX);
        }
        if ui.small_button("R").clicked() {
            studio.align_sel(crate::align::Align::Right);
        }
        if ui.small_button("T").clicked() {
            studio.align_sel(crate::align::Align::Top);
        }
        if ui.small_button("M").clicked() {
            studio.align_sel(crate::align::Align::CenterY);
        }
        if ui.small_button("B").clicked() {
            studio.align_sel(crate::align::Align::Bottom);
        }
    });
}

fn brush_studio(ui: &mut Ui, studio: &mut Studio) {
    section(ui, "Brush");
    ui.add(Slider::new(&mut studio.brush.size, 1.0..=256.0).text("Size"));
    ui.add(Slider::new(&mut studio.brush.hardness, 0.0..=1.0).text("Hardness"));
    ui.add(Slider::new(&mut studio.brush.opacity, 0.05..=1.0).text("Opacity"));
    ui.add(Slider::new(&mut studio.brush.flow, 0.05..=1.0).text("Flow"));
    ui.add(Slider::new(&mut studio.fill_tolerance, 0.0..=180.0).text("Fill / wand tolerance"));
    if studio.clone_source.is_some() {
        ui.label(RichText::new("Clone source set").small().color(ACCENT));
    } else {
        ui.label(
            RichText::new("Alt-click sets clone source")
                .small()
                .color(FG_WEAK),
        );
    }
}

fn layers_studio(ui: &mut Ui, studio: &mut Studio) {
    ui.horizontal(|ui| {
        section(ui, "Layers");
        ui.with_layout(Layout::right_to_left(eframe::egui::Align::Center), |ui| {
            if ui.small_button("+Px").clicked() {
                studio.add_layer(true);
            }
            if ui.small_button("+V").clicked() {
                studio.add_layer(false);
            }
            if ui.small_button("–").clicked() {
                studio.delete_layer();
            }
        });
    });

    let mut activate = None;
    let mut vis = None;
    let mut lock = None;
    let mut up = None;
    let mut down = None;
    let n = studio.doc.layers.len();
    for i in (0..n).rev() {
        ui.push_id(studio.doc.layers[i].id, |ui| {
            ui.horizontal(|ui| {
                let layer = &studio.doc.layers[i];
                let on = studio.active_layer == Some(i);
                let eye = if layer.visible { "●" } else { "○" };
                if ui.small_button(eye).clicked() {
                    vis = Some(i);
                }
                let lock_s = if layer.locked { "🔒" } else { " " };
                if ui.small_button(lock_s).clicked() {
                    lock = Some(i);
                }
                let fill = if on { ACCENT_DIM } else { Color32::TRANSPARENT };
                let label = format!("{}  {}", layer.kind.tag(), layer.name);
                if ui
                    .add(eframe::egui::Button::new(RichText::new(label).size(12.0)).fill(fill))
                    .clicked()
                {
                    activate = Some(i);
                }
                if ui.small_button("↑").clicked() {
                    up = Some(i);
                }
                if ui.small_button("↓").clicked() {
                    down = Some(i);
                }
            });
        });
        ui.add(Slider::new(&mut studio.doc.layers[i].opacity, 0.0..=1.0).show_value(false));
        let mut blend = studio.doc.layers[i].blend;
        ComboBox::from_id_salt(format!("blend-{i}"))
            .selected_text(blend.name())
            .width(140.0)
            .show_ui(ui, |ui| {
                for b in Blend::ALL {
                    ui.selectable_value(&mut blend, b, b.name());
                }
            });
        if blend != studio.doc.layers[i].blend {
            let l = &studio.doc.layers[i];
            studio.commit(crate::document::Cmd::SetLayerMeta {
                index: i,
                name: l.name.clone(),
                visible: l.visible,
                locked: l.locked,
                opacity: l.opacity,
                blend,
                before: (l.name.clone(), l.visible, l.locked, l.opacity, l.blend),
            });
        }
    }
    if let Some(i) = activate {
        studio.active_layer = Some(i);
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
    }
    if let Some(i) = up {
        if i + 1 < n {
            studio.commit(crate::document::Cmd::ReorderLayer { from: i, to: i + 1 });
            studio.active_layer = Some(i + 1);
        }
    }
    if let Some(i) = down {
        if i > 0 {
            studio.commit(crate::document::Cmd::ReorderLayer { from: i, to: i - 1 });
            studio.active_layer = Some(i - 1);
        }
    }
}
