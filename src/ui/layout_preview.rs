use crate::app::Studio;
use crate::document::{Document, Fill, Layer, Shape, Style};
use crate::geom::{Bounds, Geom, Pt};
use crate::layout_components::{self, ComponentBinding};
use crate::layout_prototype::{Interaction, PrototypeAction, PrototypeState, Transition, Trigger};
use eframe::egui::{self, RichText, Ui};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

struct Animation {
    previous: egui::TextureHandle,
    started: f64,
    duration: f64,
    transition: Transition,
}

struct Preview {
    state: PrototypeState,
    width: f32,
    texture: Option<egui::TextureHandle>,
    rendered: Option<Document>,
    interactive: Option<Document>,
    animation: Option<Animation>,
    pending_animation: bool,
    dirty: bool,
    hovered: Option<u64>,
    generation: u64,
    render_width: f32,
    error: Option<String>,
}

fn key() -> egui::Id {
    egui::Id::new("layout-preview-state")
}
pub fn is_open(ctx: &egui::Context) -> bool {
    ctx.data(|d| d.get_temp::<Arc<Mutex<Preview>>>(key()).is_some())
}
pub fn start(ctx: &egui::Context, studio: &mut Studio) {
    let frame = studio.selected_frame().map(|(_, id)| id).or_else(|| {
        studio
            .doc
            .layers
            .iter()
            .filter_map(|l| l.kind.shapes())
            .flatten()
            .find(|s| s.layout.frame && s.layout.parent.is_none())
            .map(|s| s.id)
    });
    let Some(frame) = frame else {
        studio.status = "Create a frame to present".into();
        return;
    };
    let mut state = PrototypeState::default();
    if let Err(e) = state.start(&studio.doc, frame) {
        studio.status = e;
        return;
    }
    let width = studio
        .doc
        .layers
        .iter()
        .filter_map(|l| l.kind.shapes())
        .flatten()
        .find(|s| s.id == frame)
        .map_or(390.0, |s| s.world_bbox().width());
    ctx.data_mut(|d| {
        d.insert_temp(
            key(),
            Arc::new(Mutex::new(Preview {
                state,
                width,
                texture: None,
                rendered: None,
                interactive: None,
                animation: None,
                pending_animation: false,
                dirty: true,
                hovered: None,
                generation: studio.canvas_gen,
                render_width: 0.0,
                error: None,
            })),
        )
    });
}

pub fn component_editor(ui: &mut Ui, studio: &mut Studio) {
    egui::CollapsingHeader::new("Components")
        .default_open(true)
        .show(ui, |ui| {
            let selected = studio.primary();
            let root = selected
                .and_then(|(li, id)| {
                    layout_components::instance_ancestor(&studio.doc, li, id).map(|root| (li, root))
                })
                .or(selected);
            let binding = root
                .and_then(|(li, id)| studio.doc.find_shape(li, id))
                .and_then(|s| s.layout.component.clone());
            match binding {
                Some(ComponentBinding::Main { variant, .. }) => {
                    ui.label(RichText::new(format!("Main component · {variant}")).small());
                    if ui.button("Insert instance").clicked()
                        && let Some((_, id)) = root
                    {
                        studio.insert_layout_instance(id);
                    }
                    let name_id = ui.id().with(("new-variant-name", root));
                    let mut name = ui.ctx().data_mut(|d| {
                        d.get_temp::<String>(name_id)
                            .unwrap_or_else(|| "Hover".into())
                    });
                    ui.horizontal(|ui| {
                        ui.add(
                            egui::TextEdit::singleline(&mut name)
                                .hint_text("Variant name")
                                .desired_width(100.0),
                        );
                        if ui
                            .add_enabled(!name.trim().is_empty(), egui::Button::new("Add variant"))
                            .clicked()
                        {
                            studio.add_layout_variant(&name);
                        }
                    });
                    ui.ctx().data_mut(|d| d.insert_temp(name_id, name));
                }
                Some(ComponentBinding::Instance { main, .. }) => {
                    ui.label(RichText::new("Instance · overrides stay yours").small());
                    ui.horizontal(|ui| {
                        if ui.small_button("Reset overrides").clicked() {
                            studio.reset_layout_instance();
                        }
                        if ui.small_button("Detach").clicked() {
                            studio.detach_layout_instance();
                        }
                    });
                    let family = layout_components::family_of(&studio.doc, main);
                    let variants: Vec<_> = layout_components::definitions(&studio.doc)
                        .into_iter()
                        .filter(|(_, id, _, _)| {
                            layout_components::family_of(&studio.doc, *id) == family
                        })
                        .collect();
                    egui::ComboBox::from_id_salt("component-swap")
                        .selected_text(
                            variants
                                .iter()
                                .find(|(_, id, _, _)| *id == main)
                                .map_or("Missing component", |(_, _, _, v)| v.as_str()),
                        )
                        .show_ui(ui, |ui| {
                            for (_, id, _, variant) in variants {
                                if ui.selectable_label(id == main, variant).clicked() {
                                    studio.swap_layout_variant(id);
                                }
                            }
                        });
                }
                None => {
                    if ui.button("Create component").clicked() {
                        studio.create_layout_component();
                    }
                }
            }
            let mains = layout_components::definitions(&studio.doc);
            if !mains.is_empty() {
                egui::ComboBox::from_id_salt("insert-component")
                    .selected_text("Insert from this document…")
                    .show_ui(ui, |ui| {
                        for (_, id, name, variant) in mains {
                            if ui.button(format!("{name} · {variant}")).clicked() {
                                studio.insert_layout_instance(id);
                            }
                        }
                    });
            }
        });
}

pub fn interaction_editor(ui: &mut Ui, studio: &mut Studio) {
    let Some((layer, id)) = studio.primary() else {
        return;
    };
    let Some(shape) = studio.doc.find_shape(layer, id) else {
        return;
    };
    let mut interactions = shape.layout.interactions.clone();
    let before = interactions.clone();
    let frames: Vec<_> = studio
        .doc
        .layers
        .iter()
        .filter_map(|l| l.kind.shapes())
        .flatten()
        .filter(|s| s.layout.frame)
        .map(|s| (s.id, s.name.clone()))
        .collect();
    let family = layout_components::instance_ancestor(&studio.doc, layer, id)
        .and_then(|root| studio.doc.find_shape(layer, root))
        .and_then(|s| match s.layout.component {
            Some(ComponentBinding::Instance { main, .. }) => {
                layout_components::family_of(&studio.doc, main)
            }
            _ => None,
        });
    let variants: Vec<_> = layout_components::definitions(&studio.doc)
        .into_iter()
        .filter(|(_, id, _, _)| {
            family.is_some() && layout_components::family_of(&studio.doc, *id) == family
        })
        .map(|(_, id, _, variant)| (id, variant))
        .collect();
    egui::CollapsingHeader::new("Interactions")
        .default_open(!interactions.is_empty())
        .show(ui, |ui| {
            let mut remove = None;
            for (index, interaction) in interactions.iter_mut().enumerate() {
                ui.push_id(index, |ui| {
                    ui.horizontal(|ui| {
                        egui::ComboBox::from_id_salt("trigger")
                            .selected_text(interaction.trigger.name())
                            .show_ui(ui, |ui| {
                                for v in [Trigger::Click, Trigger::Hover, Trigger::Press] {
                                    ui.selectable_value(&mut interaction.trigger, v, v.name());
                                }
                            });
                        if ui.small_button("Remove").clicked() {
                            remove = Some(index);
                        }
                    });
                    let mut kind = match interaction.action {
                        PrototypeAction::Navigate { .. } => 0,
                        PrototypeAction::Back => 1,
                        PrototypeAction::OpenOverlay { .. } => 2,
                        PrototypeAction::CloseOverlay => 3,
                        PrototypeAction::SetVariant { .. } => 4,
                    };
                    let previous = kind;
                    let names = [
                        "Navigate",
                        "Back",
                        "Open overlay",
                        "Close overlay",
                        "Change variant",
                    ];
                    egui::ComboBox::from_id_salt("action")
                        .selected_text(names[kind])
                        .show_ui(ui, |ui| {
                            for (i, name) in names.iter().enumerate() {
                                if i != 4 || !variants.is_empty() {
                                    ui.selectable_value(&mut kind, i, *name);
                                }
                            }
                        });
                    if previous != kind {
                        let target = frames
                            .iter()
                            .find(|f| f.0 != id)
                            .or(frames.first())
                            .map_or(id, |f| f.0);
                        interaction.action = match kind {
                            0 => PrototypeAction::Navigate { target },
                            1 => PrototypeAction::Back,
                            2 => PrototypeAction::OpenOverlay { target },
                            3 => PrototypeAction::CloseOverlay,
                            _ => PrototypeAction::SetVariant {
                                component: variants[0].0,
                            },
                        };
                    }
                    let choices = if kind == 4 { &variants } else { &frames };
                    let target = match &mut interaction.action {
                        PrototypeAction::Navigate { target }
                        | PrototypeAction::OpenOverlay { target } => Some(target),
                        PrototypeAction::SetVariant { component } => Some(component),
                        _ => None,
                    };
                    if let Some(target) = target {
                        egui::ComboBox::from_id_salt("target")
                            .selected_text(
                                choices
                                    .iter()
                                    .find(|f| f.0 == *target)
                                    .map_or("Missing target", |f| f.1.as_str()),
                            )
                            .show_ui(ui, |ui| {
                                for (f, name) in choices {
                                    ui.selectable_value(target, *f, name);
                                }
                            });
                    }
                    ui.horizontal(|ui| {
                        egui::ComboBox::from_id_salt("transition")
                            .selected_text(interaction.transition.name())
                            .show_ui(ui, |ui| {
                                for v in [
                                    Transition::Instant,
                                    Transition::Dissolve,
                                    Transition::SlideLeft,
                                    Transition::SlideRight,
                                ] {
                                    ui.selectable_value(&mut interaction.transition, v, v.name());
                                }
                            });
                        if interaction.transition != Transition::Instant {
                            ui.add(
                                egui::DragValue::new(&mut interaction.duration_ms)
                                    .range(50..=5000)
                                    .suffix(" ms"),
                            );
                        }
                    });
                    if let Err(error) = crate::layout_prototype::validate_interaction(
                        &studio.doc,
                        layer,
                        id,
                        interaction,
                    ) {
                        ui.colored_label(egui::Color32::LIGHT_RED, error);
                    }
                    ui.separator();
                });
            }
            if let Some(index) = remove {
                interactions.remove(index);
            }
            if ui
                .add_enabled(!frames.is_empty(), egui::Button::new("+ Interaction"))
                .clicked()
            {
                interactions.push(Interaction {
                    trigger: Trigger::Click,
                    action: PrototypeAction::Navigate {
                        target: frames
                            .iter()
                            .find(|f| f.0 != id)
                            .or(frames.first())
                            .map_or(id, |f| f.0),
                    },
                    transition: Transition::Dissolve,
                    duration_ms: 200,
                });
            }
        });
    if before != interactions {
        if let Some(error) = interactions.iter().find_map(|interaction| {
            crate::layout_prototype::validate_interaction(&studio.doc, layer, id, interaction).err()
        }) {
            studio.status = error;
        } else {
            studio.edit_layout(|l| l.interactions = interactions.clone());
        }
    }
}

/// Materialize a viewport and overlays without changing the working document.
/// Keep the full document for interaction target and variant validation.
fn presentation(
    doc: &Document,
    state: &PrototypeState,
    width: f32,
) -> Result<(Document, Document), String> {
    let mut full = state.preview_document(doc)?;
    let current = state.current_frame.ok_or("No starting frame")?;
    let layer = full
        .layers
        .iter()
        .position(|l| {
            l.kind
                .shapes()
                .is_some_and(|s| s.iter().any(|s| s.id == current))
        })
        .ok_or("Missing frame")?;
    let original: Vec<_> = full.layers[layer]
        .kind
        .shapes()
        .unwrap()
        .iter()
        .map(|s| (layer, s.id, s.geom.clone()))
        .collect();
    if let Some(s) = full.find_shape_mut(layer, current) {
        let b = s.geom.bbox();
        s.layout.parent = None;
        s.layout.width = crate::layout::Sizing::Fixed;
        crate::layout::set_bounds(
            &mut s.geom,
            Bounds::from_min_size(b.min, Pt::new(width.clamp(240.0, 2560.0), b.height())),
        );
    }
    crate::layout::apply_resize(&mut full, &original, &[(layer, current)]);
    crate::layout::reflow_roots(&mut full, layer);
    let mut canvas = crate::layout_export::frame_document(&full, layer, current)?;
    for overlay in &state.overlays {
        let layer = full
            .layers
            .iter()
            .position(|l| {
                l.kind
                    .shapes()
                    .is_some_and(|s| s.iter().any(|s| s.id == *overlay))
            })
            .ok_or("Overlay frame is missing")?;
        let mut content = crate::layout_export::frame_document(&full, layer, *overlay)?;
        let offset = Pt::new(
            ((canvas.width - content.width) * 0.5).max(0.0),
            ((canvas.height - content.height) * 0.5).max(0.0),
        );
        let scrim = Shape::new(
            Geom::Rect {
                origin: Pt::ZERO,
                size: Pt::new(canvas.width, canvas.height),
                radius: 0.0,
            },
            Style {
                fill: Fill::Solid(crate::color::Rgba::new(0, 0, 0, 100)),
                stroke: None,
            },
        );
        let mut dim = Layer::vector("Overlay backdrop");
        dim.kind.shapes_mut().unwrap().push(scrim);
        canvas.layers.push(dim);
        let ids: HashMap<_, _> = content
            .layers
            .iter()
            .map(|l| (l.id, crate::document::next_id()))
            .collect();
        for layer in &mut content.layers {
            layer.id = ids[&layer.id];
            layer.parent = layer.parent.and_then(|p| ids.get(&p).copied());
            layer.mask_origin += offset;
            if let Some(shapes) = layer.kind.shapes_mut() {
                for s in shapes {
                    s.geom.translate(offset);
                }
            }
        }
        canvas.layers.extend(content.layers);
    }
    Ok((full, canvas))
}

/// Follow the renderer's frame tree, ancestor clipping and rotation. Hit testing
/// is restricted to the top overlay so background controls cannot fire through it.
fn interaction_targets(doc: &Document, root: u64, point: Pt) -> Option<[Option<u64>; 3]> {
    let (li, shapes) = doc
        .layers
        .iter()
        .enumerate()
        .filter_map(|(li, l)| Some((li, l.kind.shapes()?)))
        .find(|(_, s)| s.iter().any(|s| s.id == root))?;
    if !doc.layer_visible(li) || doc.layers[li].opacity <= 0.0 {
        return None;
    }
    let map: HashMap<_, _> = shapes.iter().map(|s| (s.id, s)).collect();
    let mut children: HashMap<u64, Vec<u64>> = HashMap::new();
    for s in shapes {
        if let Some(parent) = s.layout.parent {
            children.entry(parent).or_default().push(s.id);
        }
    }
    fn visit(
        id: u64,
        p: Pt,
        map: &HashMap<u64, &Shape>,
        children: &HashMap<u64, Vec<u64>>,
        seen: &mut HashSet<u64>,
    ) -> [Option<u64>; 3] {
        if !seen.insert(id) {
            return [None; 3];
        }
        let Some(s) = map.get(&id) else {
            return [None; 3];
        };
        if !s.visible || s.guide || s.opacity <= 0.0 {
            return [None; 3];
        }
        let inside = s.contains_world(p);
        if s.layout.frame && s.layout.clip && !inside {
            return [None; 3];
        }
        let local = p.rotate_about(s.geom.bbox().center(), -s.rotation);
        let mut targets = [None; 3];
        for child in children.get(&id).into_iter().flatten().rev() {
            let hit = visit(*child, local, map, children, seen);
            for index in 0..3 {
                if targets[index].is_none() {
                    targets[index] = hit[index];
                }
            }
            if targets.iter().all(Option::is_some) {
                break;
            }
        }
        if inside {
            for interaction in &s.layout.interactions {
                let index = match interaction.trigger {
                    Trigger::Click => 0,
                    Trigger::Hover => 1,
                    Trigger::Press => 2,
                };
                if targets[index].is_none() {
                    targets[index] = Some(id);
                }
            }
        }
        targets
    }
    Some(visit(root, point, &map, &children, &mut HashSet::new()))
}

#[cfg(test)]
fn interaction_at(doc: &Document, root: u64, point: Pt) -> Option<u64> {
    interaction_targets(doc, root, point)?[0]
}

fn activate(p: &mut Preview, id: u64, trigger: Trigger) {
    let Some(doc) = p.interactive.as_ref() else {
        return;
    };
    match p.state.activate(doc, id, trigger) {
        Ok(true) => {
            p.dirty = true;
            p.pending_animation = true;
        }
        Ok(false) => {}
        Err(error) => p.error = Some(error),
    }
}

pub fn show(ui: &mut Ui, studio: &mut Studio) {
    let ctx = ui.ctx().clone();
    let Some(shared) = ctx.data(|d| d.get_temp::<Arc<Mutex<Preview>>>(key())) else {
        return;
    };
    let Ok(mut p) = shared.lock() else {
        return;
    };
    let mut close = ctx.input(|i| i.key_pressed(egui::Key::Escape));
    let viewport = ctx.viewport_rect();
    egui::Modal::new(egui::Id::new("layout-present")).show(&ctx, |ui| {
        ui.set_width((viewport.width() - 90.0).max(320.0));
        ui.set_height((viewport.height() - 100.0).max(240.0));
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new("Preview").strong().size(18.0));
            if ui
                .add_enabled(
                    !p.state.history.is_empty() || !p.state.overlays.is_empty(),
                    egui::Button::new("← Back"),
                )
                .clicked()
            {
                p.state.back();
                p.state.last_transition = Transition::SlideRight;
                p.state.duration_ms = 240;
                p.pending_animation = true;
                p.dirty = true;
            }
            for (name, w) in [("Phone", 390.0), ("Tablet", 768.0), ("Desktop", 1440.0)] {
                if ui
                    .selectable_label((p.width - w).abs() < 1.0, name)
                    .clicked()
                {
                    p.width = w;
                    p.dirty = true;
                }
            }
            if ui
                .add(
                    egui::DragValue::new(&mut p.width)
                        .range(240.0..=2560.0)
                        .suffix(" px"),
                )
                .changed()
            {
                p.dirty = true;
            }
            if ui.button("Close · Esc").clicked() {
                close = true;
            }
        });
        ui.separator();
        let available_width = (ui.available_width() - 24.0).max(120.0);
        if p.generation != studio.canvas_gen || (p.render_width - available_width).abs() > 1.0 {
            p.dirty = true;
            p.generation = studio.canvas_gen;
            p.render_width = available_width;
        }
        if p.dirty {
            p.error = None;
            match presentation(&studio.doc, &p.state, p.width) {
                Ok((full, canvas)) => {
                    let scale = (available_width / canvas.width)
                        .min(1.0)
                        .min(8192.0 / canvas.height.max(1.0));
                    let view = crate::compositor::View {
                        scale,
                        offset: Pt::ZERO,
                    };
                    if let Some(pm) = crate::compositor::render_view(
                        &canvas,
                        view,
                        (canvas.width * scale).ceil().max(1.0) as u32,
                        (canvas.height * scale).ceil().max(1.0) as u32,
                        crate::compositor::Draft::none(),
                    ) {
                        let pixels = pm
                            .pixels()
                            .iter()
                            .flat_map(|c| {
                                let c = c.demultiply();
                                [c.red(), c.green(), c.blue(), c.alpha()]
                            })
                            .collect::<Vec<_>>();
                        let image = egui::ColorImage::from_rgba_unmultiplied(
                            [pm.width() as usize, pm.height() as usize],
                            &pixels,
                        );
                        if p.pending_animation
                            && p.state.last_transition != Transition::Instant
                            && p.state.duration_ms > 0
                        {
                            p.animation = p.texture.clone().map(|previous| Animation {
                                previous,
                                started: ctx.input(|i| i.time),
                                duration: p.state.duration_ms as f64 / 1000.0,
                                transition: p.state.last_transition,
                            });
                        } else {
                            p.animation = None;
                        }
                        p.texture = Some(ctx.load_texture(
                            "layout-preview",
                            image,
                            egui::TextureOptions::LINEAR,
                        ));
                        p.rendered = Some(canvas);
                        p.interactive = Some(full);
                    } else {
                        p.error = Some("The preview is too large to render".into());
                    }
                }
                Err(error) => {
                    p.error = Some(error);
                    p.texture = None;
                    p.rendered = None;
                    p.interactive = None;
                }
            }
            p.pending_animation = false;
            p.dirty = false;
        }
        if let Some(error) = &p.error {
            ui.colored_label(egui::Color32::LIGHT_RED, error);
        }
        if let Some(texture) = p.texture.clone() {
            egui::ScrollArea::both()
                .max_height((ui.available_height() - 8.0).max(100.0))
                .show(ui, |ui| {
                    let (rect, response) =
                        ui.allocate_exact_size(texture.size_vec2(), egui::Sense::click());
                    let painter = ui.painter().with_clip_rect(rect.intersect(ui.clip_rect()));
                    let uv = egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0));
                    let now = ctx.input(|i| i.time);
                    let progress = p
                        .animation
                        .as_ref()
                        .map(|a| ((now - a.started) / a.duration.max(0.001)).clamp(0.0, 1.0) as f32)
                        .unwrap_or(1.0);
                    if let Some(animation) = p.animation.as_ref().filter(|_| progress < 1.0) {
                        let t = progress * progress * (3.0 - 2.0 * progress);
                        match animation.transition {
                            Transition::Dissolve => {
                                painter.image(
                                    animation.previous.id(),
                                    rect,
                                    uv,
                                    egui::Color32::WHITE,
                                );
                                painter.image(
                                    texture.id(),
                                    rect,
                                    uv,
                                    egui::Color32::from_white_alpha((t * 255.0) as u8),
                                );
                            }
                            Transition::SlideLeft | Transition::SlideRight => {
                                let direction = if animation.transition == Transition::SlideLeft {
                                    -1.0
                                } else {
                                    1.0
                                };
                                painter.image(
                                    animation.previous.id(),
                                    rect.translate(egui::vec2(direction * rect.width() * t, 0.0)),
                                    uv,
                                    egui::Color32::WHITE,
                                );
                                painter.image(
                                    texture.id(),
                                    rect.translate(egui::vec2(
                                        -direction * rect.width() * (1.0 - t),
                                        0.0,
                                    )),
                                    uv,
                                    egui::Color32::WHITE,
                                );
                            }
                            Transition::Instant => {
                                painter.image(texture.id(), rect, uv, egui::Color32::WHITE);
                            }
                        }
                        ctx.request_repaint();
                    } else {
                        p.animation = None;
                        painter.image(texture.id(), rect, uv, egui::Color32::WHITE);
                    }
                    let root = p.state.overlays.last().copied().or(p.state.current_frame);
                    let hits = response
                        .hover_pos()
                        .filter(|_| progress >= 1.0)
                        .and_then(|at| {
                            p.rendered.as_ref().and_then(|doc| {
                                let point = Pt::new(
                                    (at.x - rect.min.x) / rect.width() * doc.width,
                                    (at.y - rect.min.y) / rect.height() * doc.height,
                                );
                                interaction_targets(doc, root?, point)
                            })
                        })
                        .unwrap_or([None; 3]);
                    if hits[0].is_some() || hits[2].is_some() {
                        response
                            .clone()
                            .on_hover_cursor(egui::CursorIcon::PointingHand);
                    }
                    let hover = hits[1];
                    if progress >= 1.0 && hover != p.hovered {
                        let before = p.state.active_variants.clone();
                        p.state.clear_hover();
                        if before != p.state.active_variants {
                            p.dirty = true;
                            p.pending_animation = true;
                        }
                        if let Some(id) = hover {
                            activate(&mut p, id, Trigger::Hover);
                        }
                        p.hovered = hover;
                    }
                    if progress >= 1.0
                        && response.clicked()
                        && let Some(id) = hits[0]
                    {
                        activate(&mut p, id, Trigger::Click);
                    } else if progress >= 1.0
                        && response.hovered()
                        && ui.input(|i| i.pointer.primary_pressed())
                        && let Some(id) = hits[2]
                    {
                        activate(&mut p, id, Trigger::Press);
                    }
                });
        }
        if p.dirty {
            ctx.request_repaint();
        }
    });
    if close {
        ctx.data_mut(|d| d.remove::<Arc<Mutex<Preview>>>(key()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn overlay_is_centered_visible_and_blocks_underlying_interactions() {
        let mut doc = Document::new("Preview", 500.0, 400.0, 96.0);
        doc.layers = vec![Layer::vector("Frames")];
        let mut screen = crate::layout::make_frame(Pt::new(10.0, 20.0), Pt::new(300.0, 200.0));
        let mut overlay = crate::layout::make_frame(Pt::new(400.0, 20.0), Pt::new(100.0, 80.0));
        screen.layout.interactions.push(Interaction {
            trigger: Trigger::Click,
            action: PrototypeAction::Back,
            transition: Transition::Instant,
            duration_ms: 0,
        });
        overlay.layout.interactions.push(Interaction {
            trigger: Trigger::Click,
            action: PrototypeAction::CloseOverlay,
            transition: Transition::Dissolve,
            duration_ms: 200,
        });
        let (screen_id, overlay_id) = (screen.id, overlay.id);
        doc.layers[0]
            .kind
            .shapes_mut()
            .unwrap()
            .extend([screen, overlay]);
        let before = serde_json::to_string(&doc).unwrap();
        let mut state = PrototypeState::default();
        state.start(&doc, screen_id).unwrap();
        state.overlays.push(overlay_id);
        let (_, canvas) = presentation(&doc, &state, 300.0).unwrap();
        assert_eq!(
            interaction_at(&canvas, overlay_id, Pt::new(150.0, 100.0)),
            Some(overlay_id)
        );
        assert_eq!(
            interaction_at(&canvas, overlay_id, Pt::new(10.0, 10.0)),
            None
        );
        assert!(canvas.layers.len() > 1);
        let pixels = crate::compositor::render_export(&canvas, 1).unwrap();
        assert!(pixels.pixel(150, 100).unwrap().red() > pixels.pixel(10, 10).unwrap().red() + 50);
        assert_eq!(serde_json::to_string(&doc).unwrap(), before);
    }

    #[test]
    fn clipped_or_hidden_descendants_cannot_receive_prototype_clicks() {
        let mut doc = Document::new("Hit test", 300.0, 200.0, 96.0);
        let frame = crate::layout::make_frame(Pt::ZERO, Pt::new(100.0, 100.0));
        let root = frame.id;
        let mut button = crate::layout::make_frame(Pt::new(80.0, 20.0), Pt::new(80.0, 40.0));
        button.layout.parent = Some(root);
        button.layout.interactions.push(Interaction {
            trigger: Trigger::Click,
            action: PrototypeAction::Back,
            transition: Transition::Instant,
            duration_ms: 0,
        });
        let button_id = button.id;
        doc.layers = vec![Layer::vector("UI")];
        doc.layers[0]
            .kind
            .shapes_mut()
            .unwrap()
            .extend([frame, button]);
        assert_eq!(
            interaction_at(&doc, root, Pt::new(90.0, 30.0)),
            Some(button_id)
        );
        assert_eq!(interaction_at(&doc, root, Pt::new(130.0, 30.0)), None);
        doc.find_shape_mut(0, root).unwrap().visible = false;
        assert_eq!(interaction_at(&doc, root, Pt::new(90.0, 30.0)), None);
    }

    #[test]
    fn child_click_does_not_swallow_its_parent_hover_interaction() {
        let mut doc = Document::new("Bubbling", 200.0, 100.0, 96.0);
        doc.layers = vec![Layer::vector("UI")];
        let mut parent = crate::layout::make_frame(Pt::ZERO, Pt::new(180.0, 80.0));
        let mut child = crate::layout::make_frame(Pt::new(20.0, 20.0), Pt::new(100.0, 40.0));
        let (parent_id, child_id) = (parent.id, child.id);
        child.layout.parent = Some(parent_id);
        parent.layout.interactions.push(Interaction {
            trigger: Trigger::Hover,
            action: PrototypeAction::Back,
            transition: Transition::Instant,
            duration_ms: 0,
        });
        child.layout.interactions.push(Interaction {
            trigger: Trigger::Click,
            action: PrototypeAction::Back,
            transition: Transition::Instant,
            duration_ms: 0,
        });
        doc.layers[0]
            .kind
            .shapes_mut()
            .unwrap()
            .extend([parent, child]);
        assert_eq!(
            interaction_targets(&doc, parent_id, Pt::new(40.0, 30.0)),
            Some([Some(child_id), Some(parent_id), None])
        );
    }
}
