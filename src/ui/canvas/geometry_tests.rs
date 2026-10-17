use super::*;
use crate::document::{Layer, Shape as Object, Style};
use crate::motion::Pose;
use eframe::egui::{Event, Modifiers};

fn fixture() -> (eframe::egui::Context, Studio, [u64; 3]) {
    let (ctx, mut s) = super::tests::pen_input_studio();
    s.tool = Tool::Select;
    s.view.offset = Pt::ZERO;
    s.view.scale = 1.;
    let mut outer = crate::layout::make_frame(Pt::new(40., 30.), Pt::new(260., 220.));
    outer.rotation = 0.38;
    outer.layout.clip = false;
    outer.style.fill = Fill::None;
    outer.style.stroke = None;
    let mut inner = crate::layout::make_frame(Pt::new(80., 65.), Pt::new(170., 145.));
    inner.rotation = -0.21;
    inner.layout.parent = Some(outer.id);
    inner.layout.clip = false;
    inner.style.fill = Fill::None;
    inner.style.stroke = None;
    let mut object = Object::new(
        Geom::Path {
            anchors: vec![
                Anchor::corner(Pt::new(105., 95.)),
                Anchor::corner(Pt::new(175., 95.)),
                Anchor::corner(Pt::new(175., 155.)),
                Anchor::corner(Pt::new(105., 155.)),
            ],
            closed: true,
        },
        Style {
            fill: Fill::Solid(crate::color::Rgba::from_hex(0xd42c36)),
            stroke: None,
        },
    );
    object.rotation = 0.3;
    object.layout.parent = Some(inner.id);
    object.geom.anchor_mut(0).unwrap().h_out = Pt::new(20., 0.);
    let ids = [outer.id, inner.id, object.id];
    let mut layer = Layer::vector("Nested artwork");
    layer
        .kind
        .shapes_mut()
        .unwrap()
        .extend([outer, inner, object]);
    s.doc.layers = vec![layer];
    s.selection = vec![(0, ids[2])];
    s.active_layer = Some(0);
    s.dirty = false;
    (ctx, s, ids)
}
fn close(a: Pt, b: Pt) {
    assert!((a - b).length() < 0.003, "{a:?} != {b:?}");
}
fn expected(s: &Studio, ids: [u64; 3], mut point: Pt) -> Pt {
    for id in ids.into_iter().rev() {
        let shape = s.doc.find_shape(0, id).unwrap();
        point = shape.world_point(point);
        point = s.live_pose(id).map(shape.world_bbox().center(), point);
    }
    point
}
fn at(s: &Studio, point: Pt) -> Pos2 {
    win(s.canvas_rect.unwrap(), s.view, point)
}
fn button(point: Pos2, pressed: bool) -> Event {
    super::tests::pen_pointer_button(point, pressed, Modifiers::NONE)
}
fn frame(
    ctx: &eframe::egui::Context,
    s: &mut Studio,
    events: Vec<Event>,
) -> Vec<eframe::egui::epaint::ClippedShape> {
    super::tests::canvas_frame(ctx, s, events)
}
fn drag(ctx: &eframe::egui::Context, s: &mut Studio, from: Pt, to: Pt) {
    let from = at(s, from);
    let to = at(s, to);
    frame(ctx, s, vec![Event::PointerMoved(from), button(from, true)]);
    frame(ctx, s, vec![Event::PointerMoved(to)]);
    frame(ctx, s, vec![button(to, false)]);
    frame(ctx, s, vec![]);
}
fn bytes(s: &Studio) -> String {
    crate::project::encode(&s.doc).unwrap()
}
fn reopen(s: &Studio) -> crate::document::Document {
    crate::project::decode(&bytes(s)).unwrap()
}

#[test]
fn craft_nested_selection_native_paint_handles_and_click_do_not_change_document() {
    let (ctx, mut s, ids) = fixture();
    frame(&ctx, &mut s, vec![]);
    let shape = s.doc.find_shape(0, ids[2]).unwrap();
    let original = bytes(&s);
    let history = s.history.len();
    let transform = shape_transform(&s, 0, shape);
    for anchor in shape.geom.anchors() {
        close(
            map_point(transform, anchor.pt),
            expected(&s, ids, anchor.pt),
        );
    }
    let angle = selection_angle(&s);
    assert!((angle - 0.47).abs() < 1e-5);
    let bounds = posed_bounds(&s).unwrap();
    let top = expected(&s, ids, shape.geom.bbox().min);
    close(bounds.min.rotate(angle), top);
    assert!(matches!(hit_handle(&s, top), Some(HandleKind::Scale(0, _))));
    let center = expected(&s, ids, shape.geom.bbox().center());
    assert_eq!(hit_shape(&s, center, 0.), Some((0, ids[2])));
    // Native pointer ownership selects the actual child without changing artwork or history.
    s.selection.clear();
    s.individual_object = Some((0, ids[2]));
    let pos = at(&s, center);
    frame(
        &ctx,
        &mut s,
        vec![Event::PointerMoved(pos), button(pos, true)],
    );
    frame(&ctx, &mut s, vec![button(pos, false)]);
    assert_eq!(s.selection, vec![(0, ids[2])]);
    assert_eq!(bytes(&s), original);
    assert_eq!(s.history.len(), history);
    let painted = frame(&ctx, &mut s, vec![]);
    let contour: Vec<_> = s.doc.find_shape(0, ids[2]).unwrap().world_contours(64)[0]
        .iter()
        .map(|p| {
            let mut p = *p;
            for id in [ids[1], ids[0]] {
                p = s.doc.find_shape(0, id).unwrap().world_point(p);
            }
            at(&s, p)
        })
        .collect();
    assert!(painted.iter().any(|item|matches!(&item.shape,eframe::egui::Shape::Path(path) if path.points.len()==contour.len() && path.points.iter().zip(&contour).all(|(a,b)|a.distance(*b)<0.003))),"actual painted selection path");
}

#[test]
fn craft_nested_selection_node_and_handle_native_drag_undo_reopen() {
    for handle in [false, true] {
        let (ctx, mut s, ids) = fixture();
        s.tool = Tool::Node;
        frame(&ctx, &mut s, vec![]);
        let original = bytes(&s);
        let history = s.history.len();
        let shape = s.doc.find_shape(0, ids[2]).unwrap();
        let anchor = *shape.geom.anchors().next().unwrap();
        let local = if handle {
            anchor.pt + anchor.h_out
        } else {
            anchor.pt
        };
        let from = expected(&s, ids, local);
        let to = from + Pt::new(17., -11.);
        let inverse = shape_transform(&s, 0, shape).invert().unwrap();
        let target = map_point(inverse, to);
        assert_eq!(
            hit_shape_node(&s, 0, shape, from, 2.),
            Some(if handle {
                NodeHit::HandleOut(0)
            } else {
                NodeHit::Point(0)
            })
        );
        drag(&ctx, &mut s, from, to);
        assert_eq!(s.history.len(), history + 1);
        let shape = s.doc.find_shape(0, ids[2]).unwrap();
        let actual = *shape.geom.anchors().next().unwrap();
        // A pivot-preserving geometry edit must land the actual painted node at the pointer.
        let point = if handle {
            actual.pt + actual.h_out
        } else {
            actual.pt
        };
        close(map_point(shape_transform(&s, 0, shape), point), to);
        assert!((target - local).length() > 1.);
        let after = bytes(&s);
        assert_ne!(after, original);
        let cold = reopen(&s);
        assert_eq!(crate::project::encode(&cold).unwrap(), after);
        s.undo();
        assert_eq!(bytes(&s), original);
        s.redo();
        assert_eq!(bytes(&s), after);
    }
}

#[test]
fn craft_nested_selection_subtree_resize_and_rotation_follow_actual_world_handles() {
    for rotate in [false, true] {
        let (ctx, mut s, ids) = fixture();
        s.selection = vec![(0, ids[1])];
        let child = s.doc.find_shape_mut(0, ids[2]).unwrap();
        child.layout.constraint_x = crate::layout::Constraint::Scale;
        child.layout.constraint_y = crate::layout::Constraint::Scale;
        frame(&ctx, &mut s, vec![]);
        let original = bytes(&s);
        let history = s.history.len();
        let point = Pt::new(175., 155.);
        let before = expected(&s, ids, point);
        let angle = selection_angle(&s);
        let bounds = posed_bounds(&s).unwrap();
        if rotate {
            let center = bounds.center().rotate(angle);
            let from = bounds.rotate_handle().rotate(angle);
            let to = center + (from - center).rotate(0.25);
            drag(&ctx, &mut s, from, to);
            let shape = s.doc.find_shape(0, ids[2]).unwrap();
            let anchor = shape.geom.anchors().nth(2).unwrap().pt;
            close(expected(&s, ids, anchor), before.rotate_about(center, 0.25));
        } else {
            let from = bounds.handle(2).rotate(angle);
            let to = (bounds.min + (bounds.max - bounds.min) * 1.3).rotate(angle);
            drag(&ctx, &mut s, from, to);
            let shape = s.doc.find_shape(0, ids[2]).unwrap();
            let anchor = shape.geom.anchors().nth(2).unwrap().pt;
            let destination =
                Bounds::from_min_size(bounds.min, Pt::new(bounds.width(), bounds.height()) * 1.3);
            close(
                expected(&s, ids, anchor),
                bounds
                    .map_pt(before.rotate(-angle), destination)
                    .rotate(angle),
            );
        }
        assert_eq!(s.history.len(), history + 1);
        let after = bytes(&s);
        assert_eq!(crate::project::encode(&reopen(&s)).unwrap(), after);
        s.undo();
        assert_eq!(bytes(&s), original);
        s.redo();
        assert_eq!(bytes(&s), after);
    }
}

#[test]
fn craft_nested_selection_motion_nonuniform_pose_picking_clipping_and_nodes_match_painter() {
    let (ctx, mut s, ids) = fixture();
    s.persona = Persona::Motion;
    s.playhead = 1.;
    s.pose_drag.insert(
        ids[0],
        Pose {
            dx: 12.,
            dy: -9.,
            rotation: 0.2,
            width_scale: 1.25,
            height_scale: 0.65,
            ..Pose::identity()
        },
    );
    s.pose_drag.insert(
        ids[1],
        Pose {
            dx: 4.,
            dy: 3.,
            rotation: -0.1,
            width_scale: 0.8,
            height_scale: 1.4,
            ..Pose::identity()
        },
    );
    s.pose_drag.insert(
        ids[2],
        Pose {
            rotation: 0.12,
            width_scale: 1.1,
            height_scale: 0.9,
            ..Pose::identity()
        },
    );
    let shape = s.doc.find_shape(0, ids[2]).unwrap();
    let center = expected(&s, ids, shape.geom.bbox().center());
    assert_eq!(hit_shape(&s, center, 0.), Some((0, ids[2])));
    let anchors = posed_anchors(&s, 0, shape).unwrap();
    for (a, b) in anchors.iter().zip(shape.geom.anchors()) {
        close(a.pt, expected(&s, ids, b.pt));
        close(a.pt + a.h_out, expected(&s, ids, b.pt + b.h_out));
    }
    assert_eq!(
        hit_shape_node(&s, 0, shape, anchors[0].pt, 1.),
        Some(NodeHit::Point(0))
    );
    let bounds = posed_bounds(&s).unwrap();
    for a in &anchors {
        assert!(bounds.inflate(0.001).contains(a.pt));
    }
    let pm = compositor::render_view_posed(
        &s.doc,
        compositor::View {
            scale: 1.,
            offset: Pt::ZERO,
        },
        400,
        300,
        Draft::none(),
        Some(1.),
        Some(&s.pose_drag),
    )
    .unwrap();
    assert!(
        pm.pixel(center.x.round() as u32, center.y.round() as u32)
            .unwrap()
            .red()
            > 150,
        "actual painted interior"
    );
    // A clipping frame and locked ancestor must reject picking, not merely hide an outline.
    s.doc.find_shape_mut(0, ids[1]).unwrap().locked = true;
    assert_ne!(hit_shape(&s, center, 0.), Some((0, ids[2])));
    s.doc.find_shape_mut(0, ids[1]).unwrap().locked = false;
    s.doc.find_shape_mut(0, ids[1]).unwrap().layout.clip = true;
    let outside = expected(&s, ids, Pt::new(260., 150.));
    assert!(
        s.doc
            .layout_hit_point_with(0, ids[2], outside, |id| s.live_pose(id))
            .is_none()
    );
    frame(&ctx, &mut s, vec![]);
}

#[test]
fn craft_nested_selection_gradient_native_drag_uses_same_local_coordinates() {
    let (ctx, mut s, ids) = fixture();
    s.tool = Tool::Gradient;
    frame(&ctx, &mut s, vec![]);
    let original = bytes(&s);
    let n = s.history.len();
    let shape = s.doc.find_shape(0, ids[2]).unwrap();
    let bounds = shape.geom.bbox();
    let size = bounds.max - bounds.min;
    let a = bounds.min + Pt::new(size.x * 0.2, size.y * 0.3);
    let b = bounds.min + Pt::new(size.x * 0.8, size.y * 0.7);
    let from = expected(&s, ids, a);
    let to = expected(&s, ids, b);
    drag(&ctx, &mut s, from, to);
    let gradient = s
        .doc
        .find_shape(0, ids[2])
        .unwrap()
        .style
        .fill
        .gradient()
        .unwrap();
    assert!((gradient.from[0] - 0.2).abs() < 0.001 && (gradient.from[1] - 0.3).abs() < 0.001);
    assert!((gradient.to[0] - 0.8).abs() < 0.001 && (gradient.to[1] - 0.7).abs() < 0.001);
    assert_eq!(s.history.len(), n + 1);
    let after = bytes(&s);
    assert_eq!(crate::project::encode(&reopen(&s)).unwrap(), after);
    s.undo();
    assert_eq!(bytes(&s), original);
}

#[test]
fn craft_nested_selection_motion_move_keys_only_the_top_target_in_parent_coordinates() {
    for whole_frame in [false, true] {
        let (_ctx, mut s, ids) = fixture();
        s.persona = Persona::Motion;
        s.playhead = 1.;
        for (id, prop, value) in [
            (ids[0], crate::motion::Prop::Rotation, 0.2),
            (ids[0], crate::motion::Prop::Width, 1.25),
            (ids[0], crate::motion::Prop::Height, 0.65),
        ] {
            s.doc
                .motion
                .set_key(id, prop, 1., value, crate::motion::Ease::Linear);
        }
        s.selection = vec![(0, if whole_frame { ids[1] } else { ids[2] })];
        let original = bytes(&s);
        let n = s.history.len();
        let point = s.doc.find_shape(0, ids[2]).unwrap().geom.bbox().center();
        let before = expected(&s, ids, point);
        s.op = Some(Op::Move {
            orig: snapshot_moving(&s),
            start: before,
            selection_on_click: s.selection.clone(),
        });
        let delta = Pt::new(21., -14.);
        continue_drag(&mut s, before + delta, false, false);
        end_drag(&mut s, before + delta, false, false, false);
        close(expected(&s, ids, point), before + delta);
        assert_eq!(s.history.len(), n + 1);
        if whole_frame {
            assert_eq!(s.doc.motion.pose(ids[2], 1.).dx, 0.);
            assert_eq!(s.doc.motion.pose(ids[2], 1.).dy, 0.);
        }
        let after = bytes(&s);
        assert_eq!(crate::project::encode(&reopen(&s)).unwrap(), after);
        s.undo();
        assert_eq!(bytes(&s), original);
        s.redo();
        assert_eq!(bytes(&s), after);
    }
}

#[test]
fn craft_nested_selection_animated_text_native_caret_matches_the_painted_story() {
    let (ctx, mut s, ids) = fixture();
    let mut run = crate::geom::TypeRun {
        origin: Pt::new(100., 130.),
        content: "Nested text".into(),
        px: 20.,
        ..Default::default()
    };
    run.contours = crate::text::shape(&run);
    let caret = crate::text::caret_pt(&run, 3);
    let index = crate::text::hit_char(&run, caret);
    s.doc.find_shape_mut(0, ids[2]).unwrap().geom = Geom::Text(run);
    s.tool = Tool::Text;
    s.persona = Persona::Motion;
    s.pose_drag.insert(
        ids[0],
        Pose {
            width_scale: 1.2,
            height_scale: 0.8,
            rotation: 0.1,
            ..Pose::identity()
        },
    );
    s.pose_drag.insert(
        ids[2],
        Pose {
            dx: 5.,
            dy: -3.,
            rotation: -0.1,
            ..Pose::identity()
        },
    );
    frame(&ctx, &mut s, vec![]);
    let position = at(&s, expected(&s, ids, caret));
    let original = bytes(&s);
    frame(
        &ctx,
        &mut s,
        vec![Event::PointerMoved(position), button(position, true)],
    );
    frame(&ctx, &mut s, vec![button(position, false)]);
    assert_eq!(s.type_edit.as_ref().expect("native text edit").caret, index);
    let run = s.selected_type().unwrap();
    let (top, bottom) = crate::text::caret_segment(&run, index);
    let expected_top = at(&s, expected(&s, ids, top));
    let expected_bottom = at(&s, expected(&s, ids, bottom));
    let painted = frame(&ctx, &mut s, vec![]);
    assert!(painted.iter().any(|item|matches!(&item.shape,eframe::egui::Shape::LineSegment{points,stroke} if (stroke.width-1.5).abs()<0.001&&points[0].distance(expected_top)<0.003&&points[1].distance(expected_bottom)<0.003)),"actual caret paint follows nested animated text");
    assert_eq!(bytes(&s), original);
    assert_eq!(s.history.len(), 0);
}

#[test]
fn craft_nested_selection_motion_frame_handles_key_the_parent_once_and_keep_children_relative() {
    for rotate in [false, true] {
        let (ctx, mut s, ids) = fixture();
        s.persona = Persona::Motion;
        s.playhead = 1.;
        s.doc.motion.set_key(
            ids[0],
            crate::motion::Prop::Rotation,
            1.,
            0.2,
            crate::motion::Ease::Linear,
        );
        s.doc.motion.set_key(
            ids[0],
            crate::motion::Prop::Scale,
            1.,
            0.85,
            crate::motion::Ease::Linear,
        );
        s.selection = vec![(0, ids[1])];
        frame(&ctx, &mut s, vec![]);
        let original = bytes(&s);
        let n = s.history.len();
        let point = Pt::new(175., 155.);
        let before = expected(&s, ids, point);
        let bounds = posed_bounds(&s).unwrap();
        if rotate {
            let center = bounds.center();
            let from = bounds.rotate_handle();
            let to = center + (from - center).rotate(0.25);
            drag(&ctx, &mut s, from, to);
            close(expected(&s, ids, point), before.rotate_about(center, 0.25));
        } else {
            let to = bounds.min + (bounds.max - bounds.min) * 1.2;
            drag(&ctx, &mut s, bounds.max, to);
            close(
                expected(&s, ids, point),
                bounds.min + (before - bounds.min) * 1.2,
            );
        }
        assert_eq!(s.doc.motion.pose(ids[2], 1.), Pose::identity());
        assert_eq!(s.history.len(), n + 1);
        let after = bytes(&s);
        assert_eq!(crate::project::encode(&reopen(&s)).unwrap(), after);
        s.undo();
        assert_eq!(bytes(&s), original);
        s.redo();
        assert_eq!(bytes(&s), after);
    }
}

#[test]
fn craft_nested_selection_nonuniform_frame_resize_preserves_typed_child_rotation_and_constraints() {
    let (ctx, mut s, ids) = fixture();
    let mut run = crate::geom::TypeRun {
        origin: Pt::new(100., 130.),
        content: "Nested text".into(),
        px: 20.,
        tracking: 1.5,
        ..Default::default()
    };
    run.contours = crate::text::shape(&run);
    s.doc.find_shape_mut(0, ids[2]).unwrap().geom = Geom::Text(run);
    s.selection = vec![(0, ids[1])];
    frame(&ctx, &mut s, vec![]);
    let original = bytes(&s);
    let n = s.history.len();
    let before = s.doc.find_shape(0, ids[2]).unwrap().clone();
    let frame_before = s.doc.find_shape(0, ids[1]).unwrap().geom.bbox();
    let angle = selection_angle(&s);
    let bounds = posed_bounds(&s).unwrap();
    let to = (bounds.min + Pt::new(bounds.width() * 1.15, bounds.height() * 1.05)).rotate(angle);
    drag(&ctx, &mut s, bounds.max.rotate(angle), to);
    let after = s.doc.find_shape(0, ids[2]).unwrap();
    assert_eq!(after.rotation, before.rotation);
    assert_eq!(after.layout, before.layout);
    assert_eq!(after.style, before.style);
    let (Geom::Text(mut old), Geom::Text(new)) = (before.geom, after.geom.clone()) else {
        panic!("editable type must survive frame resizing")
    };
    let frame_after = s.doc.find_shape(0, ids[1]).unwrap().geom.bbox();
    old.origin += frame_after.min - frame_before.min;
    assert_eq!(
        serde_json::to_value(old).unwrap(),
        serde_json::to_value(new).unwrap()
    );
    close(
        map_point(
            shape_transform(&s, 0, s.doc.find_shape(0, ids[1]).unwrap()),
            frame_after.max,
        ),
        to,
    );
    assert_eq!(s.history.len(), n + 1);
    let encoded = bytes(&s);
    assert_eq!(crate::project::encode(&reopen(&s)).unwrap(), encoded);
    s.undo();
    assert_eq!(bytes(&s), original);
    s.redo();
    assert_eq!(bytes(&s), encoded);
}
