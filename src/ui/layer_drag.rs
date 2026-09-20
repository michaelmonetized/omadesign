//! Shared Design and Layout sidebar drag feedback and hierarchy moves.
use crate::app::{Studio, TreePlacement};
use eframe::egui::{self, CursorIcon, DragAndDrop, Pos2, Rect, Response, Stroke, Ui};

#[derive(Clone)]
enum DragItems {
    Layer(u64),
    Objects(Vec<(u64, u64)>),
}
#[derive(Clone)]
struct LayerDrag {
    document: String,
    items: DragItems,
}
#[derive(Clone, Copy)]
pub(super) enum TreeRow {
    Layer(usize),
    Object(usize, u64),
}
pub(super) struct TreeDrop {
    items: DragItems,
    target: TreeRow,
    place: TreePlacement,
}
impl TreeDrop {
    pub(super) fn apply(self, studio: &mut Studio) {
        match self.items {
            DragItems::Layer(id) => {
                if let Some(source) = studio.doc.layers.iter().position(|l| l.id == id) {
                    match self.target {
                        TreeRow::Layer(target) => {
                            studio.drop_layer(source, target, self.place);
                        }
                        TreeRow::Object(_, frame)
                            if self.place == TreePlacement::Inside
                                && studio.doc.layers[source].kind.is_placed_raster() =>
                        {
                            let previous = studio.selection.clone();
                            studio.selection = vec![(source, crate::document::RASTER_ID)];
                            if !studio.reparent_layout_selection_after(Some(frame), Vec::new()) {
                                studio.selection = previous;
                            }
                        }
                        TreeRow::Object(_, _) => {}
                    }
                }
            }
            DragItems::Objects(objects) => {
                let source: Vec<_> = objects
                    .into_iter()
                    .filter_map(|(layer, id)| {
                        studio
                            .doc
                            .layers
                            .iter()
                            .position(|l| l.id == layer)
                            .map(|li| (li, id))
                    })
                    .collect();
                match self.target {
                    TreeRow::Layer(li) => {
                        studio.drop_objects(&source, li, None, TreePlacement::Inside);
                    }
                    TreeRow::Object(li, id) => {
                        studio.drop_objects(&source, li, Some(id), self.place);
                    }
                }
            }
        }
    }
}

pub(super) fn source(response: &Response, studio: &Studio, index: usize) -> bool {
    if !studio.layer_unlocked(index) {
        return false;
    }
    response.dnd_set_drag_payload(LayerDrag {
        document: studio.swap_id.clone(),
        items: DragItems::Layer(studio.doc.layers[index].id),
    });
    response.drag_started()
}

pub(super) fn object_source(response: &Response, studio: &Studio, layer: usize, id: u64) -> bool {
    if !studio.layer_unlocked(layer) || studio.doc.find_shape(layer, id).is_none_or(|s| s.locked) {
        return false;
    }
    let selected = if studio.selection.contains(&(layer, id)) {
        studio.selection.clone()
    } else {
        vec![(layer, id)]
    };
    response.dnd_set_drag_payload(LayerDrag {
        document: studio.swap_id.clone(),
        items: DragItems::Objects(
            selected
                .iter()
                .map(|&(li, id)| (studio.doc.layers[li].id, id))
                .collect(),
        ),
    });
    response.drag_started()
}

/// The upper/lower quarters insert between siblings; a container's centre nests.
pub(super) fn drop_target(
    ui: &mut Ui,
    studio: &Studio,
    rows: &[(TreeRow, Rect)],
) -> Option<TreeDrop> {
    let payload = DragAndDrop::payload::<LayerDrag>(ui.ctx())?;
    if payload.document != studio.swap_id || !ui.input(|i| i.focused) {
        DragAndDrop::clear_payload(ui.ctx());
        return None;
    }
    let pos = ui.input(|i| i.pointer.interact_pos())?;
    let clip = ui.clip_rect();
    if !clip.contains(pos) {
        return None;
    }
    if ui.input(|i| i.pointer.primary_down()) {
        let speed = if pos.y < clip.top() + 22. {
            160.
        } else if pos.y > clip.bottom() - 22. {
            -160.
        } else {
            0.
        };
        if speed != 0. {
            ui.scroll_with_delta(egui::vec2(0., speed * ui.input(|i| i.stable_dt.min(0.05))));
            ui.ctx().request_repaint();
        }
    }
    let &(target, bounds) = rows
        .iter()
        .filter(|(_, r)| r.intersects(clip))
        .min_by(|(_, a), (_, b)| a.distance_to_pos(pos).total_cmp(&b.distance_to_pos(pos)))?;
    let container = match target {
        TreeRow::Layer(li) => match payload.items {
            DragItems::Objects(_) => true,
            _ => studio.doc.layers[li].is_group,
        },
        TreeRow::Object(li, id) => {
            let can_nest = match &payload.items {
                DragItems::Objects(_) => true,
                DragItems::Layer(layer) => studio
                    .doc
                    .layers
                    .iter()
                    .any(|l| l.id == *layer && l.kind.is_placed_raster()),
            };
            can_nest
                && studio
                    .doc
                    .find_shape(li, id)
                    .is_some_and(|s| s.layout.frame)
        }
    };
    let inside = container
        && pos.y >= bounds.top() + bounds.height() * 0.25
        && pos.y <= bounds.bottom() - bounds.height() * 0.25;
    let place = if inside {
        TreePlacement::Inside
    } else if pos.y < bounds.center().y {
        TreePlacement::Above
    } else {
        TreePlacement::Below
    };
    let allowed = match &payload.items {
        DragItems::Layer(id) => studio
            .doc
            .layers
            .iter()
            .position(|l| l.id == *id)
            .is_some_and(|source| match target {
                TreeRow::Layer(li) => studio.can_drop_layer(source, li, place),
                TreeRow::Object(li, frame) => {
                    place == TreePlacement::Inside
                        && studio.doc.layer_editable(source)
                        && studio.doc.layers[source].kind.is_placed_raster()
                        && studio.doc.layer_editable(li)
                        && studio
                            .doc
                            .find_shape(li, frame)
                            .is_some_and(|s| s.layout.frame && !s.locked && s.visible)
                }
            }),
        DragItems::Objects(objects) => {
            let (li, target_id) = match target {
                TreeRow::Layer(li) => (li, None),
                TreeRow::Object(li, id) => (li, Some(id)),
            };
            studio.layer_unlocked(li)
                && (studio.doc.layers[li].is_group || studio.doc.layers[li].kind.shapes().is_some())
                && target_id
                    .is_none_or(|id| studio.doc.find_shape(li, id).is_some_and(|s| !s.locked))
                && !objects.iter().any(|&(layer, id)| {
                    layer == studio.doc.layers[li].id
                        && target_id.is_some_and(|target| {
                            target == id
                                || crate::layout::descendants(&studio.doc, li, id).contains(&target)
                        })
                })
        }
    };
    if !allowed {
        ui.ctx().set_cursor_icon(CursorIcon::NotAllowed);
        return None;
    }
    let color = crate::ui::theme::accent();
    if inside
        || (matches!(payload.items, DragItems::Objects(_)) && matches!(target, TreeRow::Layer(_)))
    {
        ui.painter()
            .rect_stroke(bounds, 4., Stroke::new(2., color), egui::StrokeKind::Inside);
    } else {
        let y = if place == TreePlacement::Above {
            bounds.top()
        } else {
            bounds.bottom()
        };
        let left = bounds.left().max(clip.left()) + 4.;
        let right = bounds.right().min(clip.right()) - 4.;
        ui.painter().line_segment(
            [Pos2::new(left, y), Pos2::new(right, y)],
            Stroke::new(2., color),
        );
        ui.painter().circle_filled(Pos2::new(left, y), 3., color);
    }
    ui.ctx().set_cursor_icon(CursorIcon::Grabbing);
    if ui.input(|i| i.pointer.primary_released()) {
        DragAndDrop::take_payload::<LayerDrag>(ui.ctx());
        if inside {
            match target {
                TreeRow::Object(_, id) => ui
                    .ctx()
                    .data_mut(|d| d.insert_temp(egui::Id::new(("layout-tree-open", id)), true)),
                TreeRow::Layer(li) => ui.ctx().data_mut(|d| {
                    d.insert_temp(
                        egui::Id::new(("layout-layer-open", studio.doc.layers[li].id)),
                        true,
                    )
                }),
            };
        }
        return Some(TreeDrop {
            items: payload.items.clone(),
            target,
            place,
        });
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        document::{History, Layer},
        geom::Pt,
    };
    use egui::{Event, Key, Modifiers, PointerButton};
    use std::collections::HashMap;

    fn frame(
        ctx: &egui::Context,
        studio: &mut Studio,
        events: Vec<Event>,
    ) -> HashMap<String, Rect> {
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(320., 640.))),
                events,
                ..Default::default()
            },
            |ui| {
                studio.handle_shortcuts(ui.ctx());
                if studio.persona == crate::tools::Persona::Layout {
                    super::super::layout::hierarchy(ui, studio);
                } else {
                    super::super::studios::layers_studio(ui, studio);
                }
            },
        );
        fn visit(shape: &egui::Shape, labels: &mut HashMap<String, Rect>) {
            match shape {
                egui::Shape::Text(t) => {
                    labels.insert(
                        t.galley.job.text.clone(),
                        t.galley.rect.translate(t.pos.to_vec2()),
                    );
                }
                egui::Shape::Vec(v) => {
                    for s in v {
                        visit(s, labels);
                    }
                }
                _ => {}
            }
        }
        let mut labels = HashMap::new();
        for shape in &output.shapes {
            visit(&shape.shape, &mut labels);
        }
        output.textures_delta.clear();
        labels
    }
    fn button(pos: Pos2, pressed: bool) -> Event {
        Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        }
    }
    fn key(key: Key, shift: bool) -> Event {
        Event::Key {
            key,
            physical_key: Some(key),
            pressed: true,
            repeat: false,
            modifiers: Modifiers {
                ctrl: true,
                command: true,
                shift,
                ..Modifiers::NONE
            },
        }
    }
    fn names(studio: &Studio) -> Vec<&str> {
        studio.doc.layers.iter().map(|l| l.name.as_str()).collect()
    }

    fn drag(ctx: &egui::Context, studio: &mut Studio, from: Pos2, to: Pos2) {
        frame(
            ctx,
            studio,
            vec![Event::PointerMoved(from), button(from, true)],
        );
        frame(
            ctx,
            studio,
            vec![Event::PointerMoved(from + egui::vec2(0., 10.))],
        );
        frame(ctx, studio, vec![Event::PointerMoved(to)]);
        frame(ctx, studio, vec![button(to, false)]);
    }

    #[test]
    fn design_object_rows_drag_into_layer_groups_and_undo() {
        let ctx = egui::Context::default();
        crate::ui::theme::apply(&ctx);
        let mut studio = Studio::new();
        studio.doc.layers = vec![Layer::vector("Source"), Layer::group("Destination")];
        let group = studio.doc.layers[1].id;
        studio.active_layer = Some(0);
        studio.finish_create(crate::app::CreateKind::Rect, Pt::ZERO, Pt::splat(20.));
        let id = studio.primary().unwrap().1;
        studio.doc.find_shape_mut(0, id).unwrap().name = "Movable".into();
        studio.layer_expanded.insert(studio.doc.layers[0].id);
        studio.history = History::default();
        let labels = frame(&ctx, &mut studio, vec![]);
        drag(
            &ctx,
            &mut studio,
            labels["Movable"].center(),
            labels["Destination"].center(),
        );
        let layer = studio
            .doc
            .layers
            .iter()
            .position(|l| l.find(id).is_some())
            .unwrap();
        assert_eq!(studio.doc.layers[layer].parent, Some(group));
        assert_eq!(studio.history.len(), 1);
        studio.undo();
        assert!(studio.doc.find_shape(0, id).is_some());
        assert_eq!(studio.doc.layers.len(), 2);
    }

    #[test]
    fn layout_object_rows_drag_into_frames() {
        let ctx = egui::Context::default();
        crate::ui::theme::apply(&ctx);
        let mut studio = Studio::new();
        studio.doc.layers = vec![Layer::vector("Artwork")];
        studio.show_welcome = false;
        studio.persona = crate::tools::Persona::Layout;
        studio.active_layer = Some(0);
        studio.finish_create(crate::app::CreateKind::Rect, Pt::ZERO, Pt::splat(20.));
        let id = studio.primary().unwrap().1;
        studio.doc.find_shape_mut(0, id).unwrap().name = "Movable".into();
        let mut parent = crate::layout::make_frame(Pt::splat(50.), Pt::splat(100.));
        parent.name = "Destination".into();
        let target = parent.id;
        studio.doc.layers[0].kind.shapes_mut().unwrap().push(parent);
        studio.history = History::default();
        let labels = frame(&ctx, &mut studio, vec![]);
        let from = labels
            .iter()
            .find(|(name, _)| name.ends_with("Movable"))
            .unwrap()
            .1
            .center();
        let to = labels
            .iter()
            .find(|(name, _)| name.ends_with("Destination"))
            .unwrap()
            .1
            .center();
        drag(&ctx, &mut studio, from, to);
        assert_eq!(
            studio.doc.find_shape(0, id).unwrap().layout.parent,
            Some(target)
        );
        assert_eq!(studio.history.len(), 1);
        studio.undo();
        assert_eq!(studio.doc.find_shape(0, id).unwrap().layout.parent, None);
    }

    #[test]
    fn placed_image_layer_drags_into_rotated_frame_preserving_position_appearance_and_undo() {
        use crate::document::{Pixels, Shape, Style};
        use crate::geom::Geom;
        let ctx = egui::Context::default();
        crate::ui::theme::apply(&ctx);
        let mut studio = Studio::new();
        let origin = Pt::new(20., 35.);
        let size = Pt::new(80., 60.);
        let mut photo = Layer::placed_raster(
            "Photo",
            Pixels::from_rgba(1, 1, vec![220, 20, 40, 180]).unwrap(),
            origin,
            size,
        );
        photo.kind.set_raster_xform(origin, size, 0.3);
        photo.opacity = 0.6;
        photo.blend = crate::color::Blend::Multiply;
        let mut frame_shape = crate::layout::make_frame(Pt::splat(100.), Pt::splat(150.));
        frame_shape.rotation = -0.5;
        frame_shape.name = "Destination".into();
        let frame_id = frame_shape.id;
        let mut layer = Layer::vector("Frames");
        layer.kind.shapes_mut().unwrap().push(frame_shape);
        studio.doc.layers = vec![photo, layer];
        studio.persona = crate::tools::Persona::Layout;
        studio.show_welcome = false;
        studio.history = History::default();
        let before = serde_json::to_string(&studio.doc.layers).unwrap();
        let labels = frame(&ctx, &mut studio, vec![]);
        let from = labels
            .iter()
            .find(|(name, _)| name.ends_with("Photo"))
            .unwrap()
            .1
            .center();
        let to = labels
            .iter()
            .find(|(name, _)| name.ends_with("Destination"))
            .unwrap()
            .1
            .center();
        drag(&ctx, &mut studio, from, to);
        assert_eq!(studio.history.len(), 1);
        assert_eq!(studio.doc.layers.len(), 1);
        let (li, id) = studio.primary().unwrap();
        let image = studio.doc.find_shape(li, id).unwrap();
        assert_eq!(image.layout.parent, Some(frame_id));
        assert!(image.layout.image.is_some());
        assert_eq!(image.opacity, 0.6);
        assert_eq!(image.blend, crate::color::Blend::Multiply);
        let mut expected = Shape::new(
            Geom::Rect {
                origin,
                size,
                radius: 0.,
            },
            Style::default(),
        );
        expected.rotation = 0.3;
        let expected: Vec<_> = expected.world_contours(12).into_iter().flatten().collect();
        let parent = studio.doc.find_shape(li, frame_id).unwrap();
        let actual: Vec<_> = image
            .world_contours(12)
            .into_iter()
            .flatten()
            .map(|p| p.rotate_about(parent.geom.bbox().center(), parent.rotation))
            .collect();
        assert_eq!(actual.len(), expected.len());
        for (a, b) in actual.iter().zip(&expected) {
            assert!(
                (a.x - b.x).abs() < 0.002 && (a.y - b.y).abs() < 0.002,
                "{a:?} != {b:?}"
            );
        }
        let after = serde_json::to_string(&studio.doc.layers).unwrap();
        studio.undo();
        assert_eq!(serde_json::to_string(&studio.doc.layers).unwrap(), before);
        studio.redo();
        assert_eq!(serde_json::to_string(&studio.doc.layers).unwrap(), after);
    }

    #[test]
    fn masked_image_drop_bakes_coverage_and_undo_restores_editable_mask() {
        use crate::document::{Fill, Pixels};
        use base64::Engine;
        // Both a matching mask and a shorter mask use the compositor's native pixel extent.
        for mask_width in [3, 2] {
            let ctx = egui::Context::default();
            crate::ui::theme::apply(&ctx);
            let mut studio = Studio::new();
            studio.doc.transparent = true;
            let source = vec![220, 20, 40, 255, 40, 200, 10, 128, 80, 60, 240, 255];
            let mut photo = Layer::placed_raster(
                "Masked photo",
                Pixels::from_rgba(3, 1, source.clone()).unwrap(),
                Pt::new(20., 35.),
                Pt::new(3., 1.),
            );
            let mut mask = vec![255, 255, 255, 255, 128, 128, 128, 255];
            if mask_width == 3 {
                mask.extend([255, 255, 255, 0]);
            }
            photo.mask = Pixels::from_rgba(mask_width, 1, mask.clone());
            let mut parent = crate::layout::make_frame(Pt::ZERO, Pt::splat(150.));
            parent.name = "Destination".into();
            parent.style.fill = Fill::None;
            parent.style.stroke = None;
            parent.layout.clip = false;
            let mut layer = Layer::vector("Frames");
            layer.kind.shapes_mut().unwrap().push(parent);
            studio.doc.layers = vec![photo, layer];
            studio.persona = crate::tools::Persona::Layout;
            studio.show_welcome = false;
            studio.history = History::default();
            let before = serde_json::to_string(&studio.doc.layers).unwrap();
            let rendered = crate::compositor::render_export(&studio.doc, 1).unwrap();
            let labels = frame(&ctx, &mut studio, vec![]);
            let from = labels
                .iter()
                .find(|(name, _)| name.ends_with("Masked photo"))
                .unwrap()
                .1
                .center();
            let to = labels
                .iter()
                .find(|(name, _)| name.ends_with("Destination"))
                .unwrap()
                .1
                .center();
            drag(&ctx, &mut studio, from, to);
            assert_eq!(studio.history.len(), 1);
            assert_eq!(studio.doc.layers.len(), 1);
            assert!(studio.status.contains("mask baked"));
            let (li, id) = studio.primary().unwrap();
            let fill = studio
                .doc
                .find_shape(li, id)
                .unwrap()
                .layout
                .image
                .as_ref()
                .unwrap();
            let png = base64::engine::general_purpose::STANDARD
                .decode(fill.data.as_ref())
                .unwrap();
            let pixels = image::load_from_memory(&png).unwrap().to_rgba8().into_raw();
            assert_eq!(
                pixels,
                vec![220, 20, 40, 255, 40, 200, 10, 64, 80, 60, 240, 0]
            );
            let after_rendered = crate::compositor::render_export(&studio.doc, 1).unwrap();
            for (a, b) in rendered.data().iter().zip(after_rendered.data()) {
                assert!(
                    (*a as i16 - *b as i16).abs() <= 1,
                    "Visible pixel changed: {a} != {b}"
                );
            }
            studio.undo();
            assert_eq!(serde_json::to_string(&studio.doc.layers).unwrap(), before);
            assert_eq!(studio.doc.layers[0].mask.as_ref().unwrap().data, mask);
            assert_eq!(studio.doc.layers[0].kind.pixels().unwrap().data, source);
            studio.redo();
            assert_eq!(studio.doc.layers.len(), 1);
        }
    }

    #[test]
    fn layer_rows_drag_and_bracket_keys_reorder_with_single_step_undo() {
        let ctx = egui::Context::default();
        crate::ui::theme::apply(&ctx);
        let mut studio = Studio::new();
        studio.doc.layers = vec![
            Layer::vector("Bottom"),
            Layer::vector("Middle"),
            Layer::vector("Top"),
        ];
        studio.history = History::default();
        studio.selection.clear();
        studio.active_layer = None;
        let id = studio.doc.layers[0].id;
        let labels = frame(&ctx, &mut studio, vec![]);
        let from = labels["Bottom"].center();
        let to = labels["Top"].center() - egui::vec2(0., 4.);
        frame(
            &ctx,
            &mut studio,
            vec![Event::PointerMoved(from), button(from, true)],
        );
        frame(
            &ctx,
            &mut studio,
            vec![Event::PointerMoved(from - egui::vec2(0., 10.))],
        );
        frame(&ctx, &mut studio, vec![Event::PointerMoved(to)]);
        frame(&ctx, &mut studio, vec![button(to, false)]);
        assert_eq!(names(&studio), ["Middle", "Top", "Bottom"]);
        assert_eq!(studio.selected_layer, Some(id));
        assert_eq!(studio.history.len(), 1);
        assert_eq!(studio.active_layer, Some(2));
        frame(&ctx, &mut studio, vec![key(Key::Z, false)]);
        assert_eq!(names(&studio), ["Bottom", "Middle", "Top"]);
        assert_eq!(studio.active_layer, Some(0));
        frame(&ctx, &mut studio, vec![key(Key::Z, true)]);
        assert_eq!(names(&studio), ["Middle", "Top", "Bottom"]);
        frame(&ctx, &mut studio, vec![key(Key::OpenBracket, false)]);
        assert_eq!(names(&studio), ["Middle", "Bottom", "Top"]);
        frame(&ctx, &mut studio, vec![key(Key::OpenCurlyBracket, true)]);
        assert_eq!(names(&studio), ["Bottom", "Middle", "Top"]);
        frame(&ctx, &mut studio, vec![key(Key::CloseBracket, false)]);
        assert_eq!(names(&studio), ["Middle", "Bottom", "Top"]);
        frame(&ctx, &mut studio, vec![key(Key::CloseCurlyBracket, true)]);
        assert_eq!(names(&studio), ["Middle", "Top", "Bottom"]);
        // A locked row cannot begin a drag, even if a previous drag selected it.
        studio.doc.layers[2].locked = true;
        let labels = frame(&ctx, &mut studio, vec![]);
        let from = labels["Bottom"].center();
        let to = labels["Middle"].center() + egui::vec2(0., 4.);
        frame(
            &ctx,
            &mut studio,
            vec![Event::PointerMoved(from), button(from, true)],
        );
        frame(&ctx, &mut studio, vec![Event::PointerMoved(to)]);
        frame(&ctx, &mut studio, vec![button(to, false)]);
        assert_eq!(names(&studio), ["Middle", "Top", "Bottom"]);
    }

    #[test]
    fn object_stack_edges_preserve_selected_order_and_undo_the_whole_reorder() {
        let mut studio = Studio::new();
        studio.doc.layers = vec![Layer::vector("Objects")];
        studio.active_layer = Some(0);
        let mut ids = vec![];
        for x in [0., 40., 80., 120.] {
            studio.finish_create(
                crate::app::CreateKind::Rect,
                Pt::new(x, 0.),
                Pt::new(x + 20., 20.),
            );
            ids.push(studio.primary().unwrap().1);
        }
        studio.history = History::default();
        let order = |s: &Studio| {
            s.doc.layers[0]
                .kind
                .shapes()
                .unwrap()
                .iter()
                .map(|s| s.id)
                .collect::<Vec<_>>()
        };
        studio.selection = vec![(0, ids[2]), (0, ids[3])];
        studio.bring_forward();
        assert_eq!(order(&studio), ids);
        assert_eq!(studio.history.len(), 0);
        studio.selection = vec![(0, ids[0]), (0, ids[2])];
        studio.bring_to_front();
        assert_eq!(order(&studio), vec![ids[1], ids[3], ids[0], ids[2]]);
        assert_eq!(studio.history.len(), 1);
        studio.undo();
        assert_eq!(order(&studio), ids);
        studio.redo();
        studio.send_to_back();
        assert_eq!(order(&studio), vec![ids[0], ids[2], ids[1], ids[3]]);
    }
}
