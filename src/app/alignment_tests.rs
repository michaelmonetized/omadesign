use super::*;
use crate::{
    align::AlignTo,
    document::{Artboard, Pixels},
};

fn fixture() -> Studio {
    let mut s = Studio::new();
    s.show_welcome = false;
    s.doc = Document::new("Align QA", 1000., 800., 72.);
    s.doc.layers = vec![Layer::vector("Artwork")];
    s.doc.artboards = vec![Artboard::new(0, Pt::new(100., 100.), Pt::new(300., 300.))];
    s.active_layer = Some(0);
    s.selection.clear();
    s
}
fn add(s: &mut Studio, x: f32, y: f32) -> u64 {
    let shape = Shape::new(
        Geom::Rect {
            origin: Pt::new(x, y),
            size: Pt::new(40., 30.),
            radius: 0.,
        },
        Style::default(),
    );
    let id = shape.id;
    s.doc.layers[0].kind.shapes_mut().unwrap().push(shape);
    id
}
fn saved(s: &Studio) -> String {
    crate::project::encode(&s.doc).unwrap()
}
fn history_roundtrip(s: &mut Studio, before: String, history: usize) {
    assert_eq!(s.history.len(), history + 1, "single batch");
    let after = saved(s);
    s.undo();
    assert_eq!(saved(s), before);
    s.redo();
    assert_eq!(saved(s), after);
}
#[test]
fn single_object_aligns_to_artboard_and_document_fallback() {
    for outside in [false, true] {
        for how in [
            Align::Left,
            Align::CenterX,
            Align::Right,
            Align::Top,
            Align::CenterY,
            Align::Bottom,
        ] {
            let mut s = fixture();
            let id = add(
                &mut s,
                if outside { 650. } else { 180. },
                if outside { 600. } else { 170. },
            );
            s.selection = vec![(0, id)];
            let before = saved(&s);
            let history = s.history.len();
            s.align_sel(how);
            let b = s.doc.find_shape(0, id).unwrap().world_bbox();
            let target = if outside {
                Bounds::from_min_size(Pt::ZERO, Pt::new(1000., 800.))
            } else {
                s.doc.artboards[0].bounds()
            };
            let error = match how {
                Align::Left => b.min.x - target.min.x,
                Align::CenterX => b.center().x - target.center().x,
                Align::Right => b.max.x - target.max.x,
                Align::Top => b.min.y - target.min.y,
                Align::CenterY => b.center().y - target.center().y,
                Align::Bottom => b.max.y - target.max.y,
            };
            assert!(error.abs() < 0.001, "{how:?} {outside}");
            history_roundtrip(&mut s, before, history);
        }
    }
}
#[test]
fn selected_vector_and_pixel_layers_align_as_one_unit() {
    let mut s = fixture();
    let a = add(&mut s, 170., 150.);
    let b = add(&mut s, 230., 220.);
    s.activate_layer_tree(0);
    assert!(s.selection.is_empty());
    assert!(s.can_align_selection());
    let before = saved(&s);
    let history = s.history.len();
    s.align_sel(Align::Left);
    assert_eq!(s.doc.find_shape(0, a).unwrap().world_bbox().min.x, 100.);
    assert_eq!(s.doc.find_shape(0, b).unwrap().world_bbox().min.x, 160.);
    history_roundtrip(&mut s, before, history);
    s.doc.layers[0].locked = true;
    assert!(!s.can_align_selection());
    let before = saved(&s);
    s.align_sel(Align::Right);
    assert_eq!(saved(&s), before);
    s.doc.layers[0].locked = false;
    s.doc.layers[0].visible = false;
    assert!(!s.can_align_selection());
    s.doc.layers[0].visible = true;
    s.doc.layers[0].kind.shapes_mut().unwrap().clear();
    assert!(!s.can_align_selection());
    let p = Pixels::from_rgba(8, 8, vec![255; 8 * 8 * 4]).unwrap();
    s.doc.layers[0] = Layer::placed_raster("Raster", p, Pt::new(150., 180.), Pt::new(50., 60.));
    s.activate_layer_tree(0);
    assert!(s.can_align_selection());
    let before = saved(&s);
    let history = s.history.len();
    s.align_sel(Align::Bottom);
    assert_eq!(s.doc.layers[0].kind.raster_bounds().unwrap().max.y, 400.);
    history_roundtrip(&mut s, before, history);
}
#[test]
fn single_group_aligns_to_artboard_as_unit() {
    for layer_selected in [false, true] {
        let mut s = fixture();
        let a = add(&mut s, 170., 150.);
        let b = add(&mut s, 230., 220.);
        s.selection = vec![(0, a), (0, b)];
        s.group_selected();
        let group = s.doc.layers.iter().position(|l| l.is_group).unwrap();
        if layer_selected {
            s.activate_layer_tree(group);
        }
        assert_eq!(s.alignment_item_count(), 1);
        let before = saved(&s);
        let history = s.history.len();
        let centers: Vec<_> = s
            .selection
            .iter()
            .map(|&(li, id)| s.doc.find_shape(li, id).unwrap().world_bbox().center())
            .collect();
        s.align_sel(Align::Right);
        let after: Vec<_> = s
            .selection
            .iter()
            .map(|&(li, id)| s.doc.find_shape(li, id).unwrap().world_bbox())
            .collect();
        assert_eq!(
            after
                .iter()
                .map(|b| b.max.x)
                .fold(f32::NEG_INFINITY, f32::max),
            400.
        );
        assert_eq!(
            centers[1] - centers[0],
            after[1].center() - after[0].center()
        );
        history_roundtrip(&mut s, before, history);
    }
}
#[test]
fn shift_multi_aligns_each_to_its_artboard() {
    let mut s = fixture();
    s.doc
        .artboards
        .push(Artboard::new(1, Pt::new(500., 100.), Pt::new(300., 300.)));
    let a = add(&mut s, 170., 150.);
    let b = add(&mut s, 580., 210.);
    let c = add(&mut s, 850., 600.);
    s.selection = vec![(0, a), (0, b), (0, c)];
    let before = saved(&s);
    let history = s.history.len();
    s.align_sel_to(Align::CenterX, AlignTo::EachToArtboard);
    for (id, x) in [(a, 250.), (b, 650.), (c, 500.)] {
        assert_eq!(s.doc.find_shape(0, id).unwrap().world_bbox().center().x, x);
    }
    history_roundtrip(&mut s, before, history);
}

#[test]
fn parent_alignment_preserves_hidden_and_locked_group_descendants() {
    for from_layer in [false, true] {
        let mut s = fixture();
        let a = add(&mut s, 170., 150.);
        let b = add(&mut s, 230., 220.);
        s.selection = vec![(0,a),(0,b)]; s.group_selected();
        let members=s.selection.clone();
        let group=s.doc.layers.iter().position(|l|l.is_group).unwrap();
        let (li,id)=members[1];
        let child=s.doc.find_shape_mut(li,id).unwrap();child.visible=false;child.locked=true;
        s.doc.layers[li].visible=false;s.doc.layers[li].locked=true;
        if from_layer { s.activate_layer_tree(group); } else { s.selection=s.selection_for_hit(members[0]); }
        let centers:Vec<_>=members.iter().map(|&(li,id)|s.doc.find_shape(li,id).unwrap().world_bbox().center()).collect();
        let before=saved(&s);let history=s.history.len();
        assert_eq!(s.alignment_item_count(),1);
        s.align_sel(Align::Right);
        let after:Vec<_>=members.iter().map(|&(li,id)|s.doc.find_shape(li,id).unwrap().world_bbox()).collect();
        assert_eq!(after[1].center()-after[0].center(),centers[1]-centers[0]);
        assert_eq!(after.iter().map(|b|b.max.x).fold(f32::NEG_INFINITY,f32::max),400.);
        history_roundtrip(&mut s,before,history);
        s.doc.layers[group].locked=true;assert!(!s.can_align_selection());
        s.doc.layers[group].locked=false;s.enter_group_item(members[1]);assert!(!s.can_align_selection());
    }
}
#[test]
fn selected_visible_layer_carries_locked_objects_but_individual_selection_does_not() {
    let mut s=fixture();let a=add(&mut s,170.,150.);let b=add(&mut s,230.,220.);
    s.doc.find_shape_mut(0,b).unwrap().locked=true;
    s.activate_layer_tree(0);s.align_sel(Align::Left);
    assert_eq!(s.doc.find_shape(0,a).unwrap().world_bbox().min.x,100.);
    assert_eq!(s.doc.find_shape(0,b).unwrap().world_bbox().min.x,160.);
    s.selected_layer=None;s.selection=vec![(0,b)];assert!(!s.can_align_selection());
}

#[test]
fn selected_plain_layer_inside_group_uses_only_visible_shapes() {
    let mut s = fixture();
    let a = add(&mut s, 170., 150.);
    let b = add(&mut s, 230., 220.);
    let group = Layer::group("Parent");
    s.doc.layers[0].parent = Some(group.id);
    s.doc.layers.push(group);
    s.doc.find_shape_mut(0, b).unwrap().visible = false;
    s.doc.find_shape_mut(0, a).unwrap().locked = true;
    s.activate_layer_tree(0);
    let before = saved(&s);
    let history = s.history.len();
    assert_eq!(s.alignment_item_count(), 1);
    assert_eq!(s.align_targets().0, vec![(0, a)]);
    s.align_sel(Align::Right);
    assert_eq!(s.doc.find_shape(0, a).unwrap().world_bbox().max.x, 400.);
    assert_eq!(s.doc.find_shape(0, b).unwrap().world_bbox().min.x, 230.);
    history_roundtrip(&mut s, before, history);
}
