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
    owner: egui::Id,
    previous_focus: Option<egui::Id>,
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
    let owner = super::keyboard_controls::draft_id(ctx, "layout-preview");
    let previous_focus = ctx.memory(|memory| memory.focused());
    ctx.data_mut(|d| {
        d.insert_temp(
            key(),
            Arc::new(Mutex::new(Preview {
                owner,
                previous_focus,
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
    let owner = ui.make_persistent_id(("layout-components", &studio.swap_id, &studio.selection));
    let eligible = studio.selectable_objects();
    let available = !studio.file_dialog_pending()
        && !studio.updates.freezing
        && studio
            .selection
            .iter()
            .all(|target| eligible.contains(target));
    super::fields::native_scope(ui, owner, |ui| {
        ui.add_enabled_ui(available, |ui| component_controls(ui, studio))
    });
}

fn component_controls(ui: &mut Ui, studio: &mut Studio) {
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
                    if super::keyboard_controls::button(ui, "insert-instance", "Insert instance")
                        .clicked()
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
                        let error = name.trim().is_empty().then_some("Enter a variant name.");
                        super::keyboard_authoring::input(
                            ui,
                            "variant-name",
                            "Variant name",
                            &mut name,
                            error,
                            false,
                        );
                        if ui
                            .add_enabled_ui(!name.trim().is_empty(), |ui| {
                                super::keyboard_controls::button(ui, "add-variant", "Add variant")
                            })
                            .inner
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
                        if super::keyboard_controls::button(
                            ui,
                            "reset-overrides",
                            "Reset overrides",
                        )
                        .clicked()
                        {
                            studio.reset_layout_instance();
                        }
                        if super::keyboard_controls::button(ui, "detach", "Detach").clicked() {
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
                    let choices = variants
                        .into_iter()
                        .map(|(_, id, _, variant)| (id, variant))
                        .collect::<Vec<_>>();
                    let mut target = main;
                    if super::keyboard_controls::choice(
                        ui,
                        "component-swap",
                        "Component variant",
                        &mut target,
                        &choices,
                    )
                    .changed()
                    {
                        studio.swap_layout_variant(target);
                    }
                }
                None => {
                    if super::keyboard_controls::button(ui, "create-component", "Create component")
                        .clicked()
                    {
                        studio.create_layout_component();
                    }
                }
            }
            let mains = layout_components::definitions(&studio.doc);
            if !mains.is_empty() {
                let choices = mains
                    .into_iter()
                    .map(|(_, id, name, variant)| (id, format!("{name} · {variant}")))
                    .collect::<Vec<_>>();
                let mut target = 0;
                if super::keyboard_controls::choice(
                    ui,
                    "insert-component",
                    "Insert document component",
                    &mut target,
                    &choices,
                )
                .changed()
                {
                    studio.insert_layout_instance(target);
                }
            }
        });
}

pub fn interaction_editor(ui: &mut Ui, studio: &mut Studio) {
    let owner = ui.make_persistent_id(("layout-interactions", &studio.swap_id, &studio.selection));
    let eligible = studio.selectable_objects();
    let available = !studio.file_dialog_pending()
        && !studio.updates.freezing
        && studio
            .selection
            .iter()
            .all(|target| eligible.contains(target));
    super::fields::native_scope(ui, owner, |ui| {
        ui.add_enabled_ui(available, |ui| interaction_controls(ui, studio))
    });
}

fn interaction_controls(ui: &mut Ui, studio: &mut Studio) {
    let Some((layer, id)) = studio.primary() else {
        return;
    };
    let Some(shape) = studio.doc.find_shape(layer, id) else {
        return;
    };
    let mut interactions = shape.layout.interactions.clone();
    let before = interactions.clone();
    let rows_key = ui.make_persistent_id("interaction-rows");
    let mut rows = ui
        .data(|data| data.get_temp::<(Vec<Interaction>, Vec<egui::Id>)>(rows_key))
        .filter(|(expected, _)| expected == &interactions)
        .map(|(_, rows)| rows)
        .unwrap_or_else(|| {
            interactions
                .iter()
                .map(|_| super::keyboard_controls::draft_id(ui.ctx(), "prototype-interaction"))
                .collect()
        });
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
                ui.push_id(rows[index], |ui| {
                    ui.vertical(|ui| {
                        let choices = [Trigger::Click, Trigger::Hover, Trigger::Press]
                            .into_iter()
                            .map(|value| (value, value.name().to_string()))
                            .collect::<Vec<_>>();
                        super::keyboard_controls::choice(
                            ui,
                            "trigger",
                            "Interaction trigger",
                            &mut interaction.trigger,
                            &choices,
                        );
                        if super::keyboard_controls::button(ui, "remove", "Remove interaction")
                            .clicked()
                        {
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
                    let choices = names
                        .iter()
                        .enumerate()
                        .filter(|(index, _)| *index != 4 || !variants.is_empty())
                        .map(|(index, name)| (index, name.to_string()))
                        .collect::<Vec<_>>();
                    let action = super::keyboard_controls::choice(
                        ui,
                        "action",
                        "Interaction action",
                        &mut kind,
                        &choices,
                    );
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
                        super::keyboard_controls::choice(
                            ui,
                            "target",
                            "Interaction target",
                            target,
                            choices,
                        );
                    }
                    ui.vertical(|ui| {
                        let choices = [
                            Transition::Instant,
                            Transition::Dissolve,
                            Transition::SlideLeft,
                            Transition::SlideRight,
                        ]
                        .into_iter()
                        .map(|value| (value, value.name().to_string()))
                        .collect::<Vec<_>>();
                        super::keyboard_controls::choice(
                            ui,
                            "transition",
                            "Interaction transition",
                            &mut interaction.transition,
                            &choices,
                        );
                        if interaction.transition != Transition::Instant {
                            let mut duration = interaction.duration_ms;
                            let field = super::fields::number(
                                ui,
                                "duration",
                                "Transition duration (ms)",
                                &mut duration,
                                50..=5000,
                                " ms",
                                None,
                                Default::default(),
                            );
                            if field.changed {
                                interaction.duration_ms = duration;
                            }
                        }
                    });
                    if let Err(error) = crate::layout_prototype::validate_interaction(
                        &studio.doc,
                        layer,
                        id,
                        interaction,
                    ) {
                        super::welcome::control_error(ui, &[&action], &error);
                    }
                    ui.separator();
                });
            }
            if let Some(index) = remove {
                interactions.remove(index);
                rows.remove(index);
            }
            if ui
                .add_enabled_ui(!frames.is_empty(), |ui| {
                    super::keyboard_controls::button(ui, "add-interaction", "+ Interaction")
                })
                .inner
                .clicked()
            {
                rows.push(super::keyboard_controls::draft_id(
                    ui.ctx(),
                    "prototype-interaction",
                ));
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
    if studio
        .doc
        .find_shape(layer, id)
        .is_some_and(|shape| shape.layout.interactions == interactions)
    {
        ui.data_mut(|data| data.insert_temp(rows_key, (interactions, rows)));
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
            let mut width = p.width;
            let field = super::fields::number(
                ui,
                p.owner.with("width"),
                "Preview width (px)",
                &mut width,
                240.0..=2560.0,
                " px",
                None,
                Default::default(),
            );
            if field.changed {
                p.width = width;
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
        let previous_focus = p.previous_focus;
        drop(p);
        ctx.data_mut(|d| d.remove::<Arc<Mutex<Preview>>>(key()));
        if let Some(id) = previous_focus {
            ctx.memory_mut(|memory| memory.request_focus(id));
        }
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

#[cfg(test)]
mod keyboard_tests {
    use super::*;
    use crate::ui::welcome::keyboard_tests as keys;
    use egui::{Event, Key, Modifiers};

    fn frame(ctx: &egui::Context, studio: &mut Studio, events: Vec<Event>) -> egui::FullOutput {
        keys::render(ctx, events, |ui| {
            studio.handle_shortcuts(ui.ctx());
            ui.set_max_width(500.);
            component_editor(ui, studio);
            interaction_editor(ui, studio);
            ui.interact(
                ui.available_rect_before_wrap(),
                egui::Id::new("studio-canvas"),
                egui::Sense::click(),
            );
            super::super::keyboard_authoring::show(ctx, studio);
        })
    }

    fn press(ctx: &egui::Context, studio: &mut Studio, name: &str) {
        keys::tab_to(ctx, name, |events| frame(ctx, studio, events));
        frame(ctx, studio, keys::key(Key::Enter, Modifiers::NONE));
        frame(ctx, studio, vec![]);
    }

    #[test]
    fn keyboard_layout_creates_component_variant_instance_and_interaction_without_pointer_setup() {
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        super::super::theme::apply(&ctx);
        let mut studio = Studio::new();
        studio.doc = Document::new("Keyboard prototype authoring", 800., 600., 72.);
        studio.show_welcome = false;
        studio.persona = crate::tools::Persona::Layout;
        assert!(studio.selection.is_empty());
        frame(&ctx, &mut studio, vec![]);
        frame(&ctx, &mut studio, keys::key(Key::F8, Modifiers::NONE));
        frame(&ctx, &mut studio, keys::key(Key::F, Modifiers::NONE));
        frame(&ctx, &mut studio, keys::key(Key::Enter, Modifiers::NONE));
        frame(&ctx, &mut studio, vec![]);
        frame(
            &ctx,
            &mut studio,
            keys::key(Key::Enter, Modifiers::CTRL | Modifiers::COMMAND),
        );
        frame(&ctx, &mut studio, vec![]);
        let (layer, main) = studio.selected_frame().unwrap();
        press(&ctx, &mut studio, "Create component");
        assert!(matches!(
            studio.doc.find_shape(layer, main).unwrap().layout.component,
            Some(ComponentBinding::Main { .. })
        ));
        keys::tab_to(&ctx, "Variant name", |events| {
            frame(&ctx, &mut studio, events)
        });
        frame(
            &ctx,
            &mut studio,
            keys::key(Key::A, Modifiers::CTRL | Modifiers::COMMAND),
        );
        frame(
            &ctx,
            &mut studio,
            keys::key(Key::Backspace, Modifiers::NONE),
        );
        let output = frame(&ctx, &mut studio, vec![]);
        keys::associated_error(&output, "Variant name");
        assert!(keys::named(&output, "Add variant").1.is_disabled());
        keys::replace("Hover state", |events| frame(&ctx, &mut studio, events));
        press(&ctx, &mut studio, "Add variant");
        assert_eq!(layout_components::definitions(&studio.doc).len(), 2);
        press(&ctx, &mut studio, "Insert instance");
        let (layer, instance) = studio.primary().unwrap();
        let output = keys::tab_to(&ctx, "Component variant", |events| {
            frame(&ctx, &mut studio, events)
        });
        keys::focused(&ctx, &output, "Component variant");
        frame(&ctx, &mut studio, keys::key(Key::Home, Modifiers::NONE));
        assert!(
            matches!(studio.doc.find_shape(layer, instance).unwrap().layout.component, Some(ComponentBinding::Instance { main: target, .. }) if target == main)
        );
        press(&ctx, &mut studio, "Interactions");
        press(&ctx, &mut studio, "+ Interaction");
        assert_eq!(
            studio
                .doc
                .find_shape(layer, instance)
                .unwrap()
                .layout
                .interactions
                .len(),
            1
        );
        let output = keys::tab_to(&ctx, "Transition duration (ms)", |events| {
            frame(&ctx, &mut studio, events)
        });
        keys::focused(&ctx, &output, "Transition duration (ms)");
        let duration_id = keys::named(&output, "Transition duration (ms)").0;
        let output = keys::replace("-", |events| frame(&ctx, &mut studio, events));
        keys::associated_error(&output, "Transition duration (ms)");
        assert_eq!(
            studio
                .doc
                .find_shape(layer, instance)
                .unwrap()
                .layout
                .interactions[0]
                .duration_ms,
            200
        );
        keys::replace("350", |events| frame(&ctx, &mut studio, events));
        frame(&ctx, &mut studio, keys::key(Key::Enter, Modifiers::NONE));
        assert_eq!(
            studio
                .doc
                .find_shape(layer, instance)
                .unwrap()
                .layout
                .interactions[0]
                .duration_ms,
            350
        );
        keys::tab_to(&ctx, "Interaction trigger", |events| {
            frame(&ctx, &mut studio, events)
        });
        frame(&ctx, &mut studio, keys::key(Key::End, Modifiers::NONE));
        assert_eq!(
            studio
                .doc
                .find_shape(layer, instance)
                .unwrap()
                .layout
                .interactions[0]
                .trigger,
            Trigger::Press
        );
        let output = frame(&ctx, &mut studio, vec![]);
        assert_eq!(
            keys::named(&output, "Transition duration (ms)").0,
            duration_id
        );
        studio.doc.layers[layer].locked = true;
        let before = studio
            .doc
            .find_shape(layer, instance)
            .unwrap()
            .layout
            .interactions
            .clone();
        let output = frame(&ctx, &mut studio, vec![]);
        assert!(
            keys::named(&output, "Transition duration (ms)")
                .1
                .is_disabled()
        );
        frame(
            &ctx,
            &mut studio,
            keys::action(
                duration_id,
                egui::accesskit::Action::SetValue,
                Some(egui::accesskit::ActionData::NumericValue(900.)),
            ),
        );
        assert_eq!(
            studio
                .doc
                .find_shape(layer, instance)
                .unwrap()
                .layout
                .interactions,
            before
        );
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join(".artifacts")
            .join(format!(
                "keyboard-prototype-{}-{}",
                std::process::id(),
                crate::document::next_id()
            ));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("prototype.oma");
        crate::project::save_to(&studio.doc, &path).unwrap();
        let reopened = crate::project::load_from(&path).unwrap();
        assert_eq!(
            reopened
                .find_shape(layer, instance)
                .unwrap()
                .layout
                .interactions,
            before
        );
        assert!(matches!(
            reopened
                .find_shape(layer, instance)
                .unwrap()
                .layout
                .component,
            Some(ComponentBinding::Instance { .. })
        ));
        std::fs::remove_dir_all(root).unwrap();
    }
}
