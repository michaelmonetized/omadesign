//! Layer-row drag feedback. The document changes only on a valid release.
use crate::app::Studio;
use eframe::egui::{self, CursorIcon, DragAndDrop, Pos2, Rect, Response, Stroke, Ui};

#[derive(Clone)]
struct LayerDrag {
    document: String,
    layer: u64,
}

pub(super) fn source(response: &Response, studio: &Studio, index: usize) -> bool {
    if !studio.layer_unlocked(index) {
        return false;
    }
    response.dnd_set_drag_payload(LayerDrag {
        document: studio.swap_id.clone(),
        layer: studio.doc.layers[index].id,
    });
    response.drag_started()
}

/// Header plus expanded object rows. Group bounds are the union of these
/// entries, including visible descendant layers, so the insertion line always
/// describes where the whole subtree will land.
pub(super) fn drop_target(
    ui: &mut Ui,
    studio: &Studio,
    rows: &[(usize, Rect)],
) -> Option<(usize, usize, bool)> {
    let payload = DragAndDrop::payload::<LayerDrag>(ui.ctx())?;
    if payload.document != studio.swap_id || !ui.input(|i| i.focused) {
        DragAndDrop::clear_payload(ui.ctx());
        return None;
    }
    let source = studio
        .doc
        .layers
        .iter()
        .position(|l| l.id == payload.layer)?;
    let pos = ui.input(|i| i.pointer.interact_pos())?;
    let clip = ui.clip_rect();
    if !clip.contains(pos) {
        return None;
    }
    // Scroll a long tree while the pointer waits near its visible edge. The
    // speed uses frame time, not refresh rate, and stops immediately on release.
    if ui.input(|i| i.pointer.primary_down()) {
        let edge = 22.;
        let speed = if pos.y < clip.top() + edge {
            160.
        } else if pos.y > clip.bottom() - edge {
            -160.
        } else {
            0.
        };
        if speed != 0. {
            ui.scroll_with_delta(egui::vec2(0., speed * ui.input(|i| i.stable_dt.min(0.05))));
            ui.ctx().request_repaint();
        }
    }
    let (hovered, _) = rows
        .iter()
        .filter(|(_, r)| r.intersects(clip))
        .min_by(|(_, a), (_, b)| a.distance_to_pos(pos).total_cmp(&b.distance_to_pos(pos)))?;
    let Some(target) = studio.layer_drop_target(source, *hovered) else {
        ui.ctx().set_cursor_icon(CursorIcon::NotAllowed);
        return None;
    };
    let tree = studio.layer_tree_indices(target);
    let bounds = rows
        .iter()
        .filter(|(i, _)| tree.contains(i))
        .map(|(_, r)| *r)
        .reduce(Rect::union)?;
    let above = pos.y < bounds.center().y;
    let y = if above { bounds.top() } else { bounds.bottom() };
    let color = crate::ui::theme::accent();
    let left = bounds.left().max(clip.left()) + 4.;
    let right = bounds.right().min(clip.right()) - 4.;
    ui.painter().line_segment(
        [Pos2::new(left, y), Pos2::new(right, y)],
        Stroke::new(2., color),
    );
    ui.painter().circle_filled(Pos2::new(left, y), 3., color);
    ui.ctx().set_cursor_icon(CursorIcon::Grabbing);
    if ui.input(|i| i.pointer.primary_released()) {
        DragAndDrop::take_payload::<LayerDrag>(ui.ctx());
        return Some((source, target, above));
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
                super::super::layers_studio(ui, studio);
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
