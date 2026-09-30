use super::*;
use crate::document::Pixels;
use crate::{clipboard::ClipboardContent, motion_presets::Preset};

fn fixture() -> Studio {
    let mut s = Studio::new();
    s.doc = Document::new("Raster motion", 240.0, 160.0, 72.0);
    s.doc.layers.clear();
    s.doc.transparent = true;
    s.persona = Persona::Motion;
    for (name, x) in [("Pasted red", 40.0), ("Pasted blue", 140.0)] {
        s.insert_clipboard_content(
            ClipboardContent::Image {
                name: name.into(),
                image: RgbaImage {
                    w: 20,
                    h: 20,
                    data: [255, 0, 0, 255].repeat(400),
                },
            },
            Pt::new(x, 40.0),
        )
        .unwrap();
    }
    s.selection = vec![(0, RASTER_ID), (1, RASTER_ID)];
    s.history.clear();
    s
}

#[test]
fn raster_motion_pasted_images_key_independently_and_roundtrip() {
    let mut s = fixture();
    let rest = crate::project::encode(&s.doc).unwrap();
    s.key_selection(Ease::Linear);
    let ids: Vec<_> = s.doc.layers.iter().map(|l| l.id).collect();
    assert!(!s.doc.motion.has_shape(RASTER_ID));
    for id in &ids {
        assert!(s.doc.motion.has_shape(*id));
    }
    s.playhead = 1.0;
    s.key_prop(ids[0], Prop::X, 60.0);
    s.key_prop(ids[1], Prop::Y, 50.0);
    assert_eq!(s.doc.motion.pose(ids[0], 1.0).dx, 60.0);
    assert_eq!(s.doc.motion.pose(ids[0], 1.0).dy, 0.0);
    assert_eq!(s.doc.motion.pose(ids[1], 1.0).dx, 0.0);
    let saved = crate::project::encode(&s.doc).unwrap();
    let reopened = crate::project::decode(&saved).unwrap();
    assert_eq!(reopened.motion, s.doc.motion);
    assert_eq!(
        crate::motion::selection(&reopened, ids[1]),
        Some((1, RASTER_ID))
    );
    for _ in 0..3 {
        s.undo();
    }
    assert_eq!(crate::project::encode(&s.doc).unwrap(), rest);
    for _ in 0..3 {
        s.redo();
    }
    assert_eq!(crate::project::encode(&s.doc).unwrap(), saved);
}

#[test]
fn raster_motion_all_image_presets_render_and_preserve_pixels() {
    for preset in Preset::ALL {
        let mut s = fixture();
        let before = s.doc.layers[0].kind.pixels().unwrap().data.clone();
        assert_eq!(
            s.motion_preset_count(preset),
            if preset == Preset::DrawStroke { 0 } else { 2 }
        );
        if preset == Preset::DrawStroke {
            continue;
        }
        s.apply_motion_preset(preset);
        assert_eq!(s.history.len(), 1, "{}", preset.name());
        assert_eq!(s.doc.motion.shapes().len(), 2);
        let frames: Vec<_> = [0.0, 0.25, 0.8]
            .map(|t| crate::compositor::render_export_at(&s.doc, 1.0, t, true).unwrap())
            .into();
        assert!(
            frames.windows(2).any(|f| f[0].data() != f[1].data()),
            "{} did not animate",
            preset.name()
        );
        assert_eq!(s.doc.layers[0].kind.pixels().unwrap().data, before);
        let svg = crate::svg::export_animated(&s.doc).unwrap();
        roxmltree::Document::parse(&svg).unwrap();
        for layer in &s.doc.layers {
            assert!(svg.contains(&format!("animation-name: oma-{};", layer.id)));
        }
        if preset == Preset::FillUp {
            assert!(svg.contains("oma-raster-reveal-"));
        }
    }
}

#[test]
fn raster_motion_masks_opacity_and_reveal_follow_exported_pixels() {
    let mut s = fixture();
    s.doc.layers.truncate(1);
    let l = &mut s.doc.layers[0];
    let id = l.id;
    let mut mask = vec![0; 20 * 20 * 4];
    for y in 0..20 {
        for x in 0..10 {
            mask[(y * 20 + x) * 4..(y * 20 + x + 1) * 4].copy_from_slice(&[255; 4]);
        }
    }
    l.mask = Some(Pixels::from_rgba(20, 20, mask).unwrap());
    l.opacity = 0.2;
    l.kind
        .set_raster_xform(Pt::new(-80., 30.), Pt::splat(20.), 0.);
    s.doc.motion.set_key(id, Prop::X, 0.0, 170.0, Ease::Linear);
    s.doc
        .motion
        .set_key(id, Prop::Opacity, 0.0, 0.5, Ease::Linear);
    s.doc
        .motion
        .set_key(id, Prop::FillReveal, 0.0, 0.5, Ease::Linear);
    let pm = crate::compositor::render_export_at(&s.doc, 1.0, 0.0, true).unwrap();
    assert!((pm.pixel(95, 45).unwrap().alpha() as i32 - 128).abs() <= 1);
    for (x, y) in [(35, 45), (105, 45), (95, 35)] {
        assert_eq!(pm.pixel(x, y).unwrap().alpha(), 0);
    }
    // An opacity key can reveal a layer whose rest opacity is zero.
    s.doc.layers[0].opacity = 0.0;
    let pm = crate::compositor::render_export_at(&s.doc, 1.0, 0.0, true).unwrap();
    assert!(pm.pixel(95, 45).unwrap().alpha() > 100);
    // Nested rendering must not cull a raster at its invisible rest position.
    let mut group = Layer::group("Group");
    group.opacity = 0.5;
    s.doc.layers[0].parent = Some(group.id);
    s.doc.layers.push(group);
    let pm = crate::compositor::render_export_at(&s.doc, 1.0, 0.0, true).unwrap();
    assert!((pm.pixel(95, 45).unwrap().alpha() as i32 - 64).abs() <= 1);
}

#[test]
fn raster_motion_selection_and_deletion_follow_layer_tracks() {
    let mut s = fixture();
    let id = s.doc.layers[0].id;
    s.selection = vec![(0, RASTER_ID)];
    s.key_selection(Ease::Linear);
    s.key_prop(id, Prop::X, 60.0);
    let overrides = HashMap::new();
    assert_eq!(
        crate::motion::hit_test(&s.doc, 0.0, &overrides, Pt::new(100.0, 40.0), 0.0),
        Some((0, RASTER_ID))
    );
    assert_eq!(
        crate::motion::hit_test(&s.doc, 0.0, &overrides, Pt::new(40.0, 40.0), 0.0),
        None
    );
    assert_eq!(
        crate::motion::hits_in_rect(
            &s.doc,
            0.0,
            &overrides,
            Bounds::from_min_size(Pt::new(95.0, 35.0), Pt::splat(10.0))
        ),
        vec![(0, RASTER_ID)]
    );
    s.selected_key = Some((id, Prop::X, 0));
    s.forget_stale_key();
    assert!(s.selected_key.is_some());
    s.delete_selection();
    assert_eq!(s.doc.motion.value(id, Prop::X, 0.0), None);
    s.selected_key = None;
    s.delete_selection();
    assert!(!s.doc.motion.has_shape(id));
    assert_eq!(s.doc.layers.len(), 2);
    s.undo();
    assert!(s.doc.motion.has_shape(id));
}

#[test]
fn raster_motion_skips_hidden_locked_and_ancestor_locked_layers() {
    let mut s = fixture();
    s.doc.layers[0].locked = true;
    s.doc.layers[1].visible = false;
    s.key_selection(Ease::Linear);
    s.apply_motion_preset(Preset::PopIn);
    assert!(s.doc.motion.is_empty());
    s.doc.layers[0].locked = false;
    let mut group = Layer::group("Locked parent");
    group.locked = true;
    s.doc.layers[0].parent = Some(group.id);
    s.doc.layers.push(group);
    assert_eq!(s.motion_preset_count(Preset::PopIn), 0);
}

#[test]
fn raster_motion_duplicate_and_delete_preserve_undoable_tracks() {
    let mut s = fixture();
    s.selection = vec![(0, RASTER_ID)];
    s.apply_motion_preset(Preset::Fly);
    let original = s.doc.layers[0].id;
    let before = crate::project::encode(&s.doc).unwrap();
    s.duplicate_selection();
    let copy = s.doc.layers[2].id;
    assert_ne!(copy, original);
    assert_eq!(
        s.doc.motion.pose(copy, 0.3).dx,
        s.doc.motion.pose(original, 0.3).dx
    );
    s.undo();
    assert_eq!(crate::project::encode(&s.doc).unwrap(), before);
    s.redo();
    s.persona = Persona::Design;
    s.selection = vec![(2, RASTER_ID)];
    s.delete_selection();
    assert!(!s.doc.motion.has_shape(copy));
    s.undo();
    assert!(s.doc.motion.has_shape(copy));
    s.delete_layer_tree(2);
    assert!(!s.doc.motion.has_shape(copy));
    s.undo();
    assert!(s.doc.motion.has_shape(copy));
}

#[test]
fn keying_translucent_raster_and_vector_later_preserves_rest_opacity() {
    let mut s = fixture();
    s.doc.layers[0].opacity = 0.3;
    let mut layer = Layer::vector("Vector");
    let mut shape = crate::layout::make_frame(Pt::ZERO, Pt::splat(20.0));
    shape.opacity = 0.4;
    let vector = shape.id;
    layer.kind.shapes_mut().unwrap().push(shape);
    s.doc.layers.push(layer);
    s.selection = vec![(0, RASTER_ID), (2, vector)];
    s.playhead = 1.0;
    let before = s.doc.motion.clone();
    s.key_selection(Ease::Linear);
    for t in [0.0, 0.5, 1.0] {
        assert_eq!(s.doc.motion.pose(s.doc.layers[0].id, t).opacity, Some(0.3));
        assert_eq!(s.doc.motion.pose(vector, t).opacity, Some(0.4));
    }
    s.undo();
    assert_eq!(s.doc.motion, before);
}

#[test]
fn animated_raster_import_and_frame_reparent_keep_tracks_and_undo() {
    let mut source = fixture();
    source.doc.layers.truncate(1);
    source.doc.layers[0].opacity = 0.3;
    let original = source.doc.layers[0].id;
    source.doc.motion.set_key(original, Prop::X, 1.0, 40.0, Ease::Linear);
    source.doc.motion.set_key(original, Prop::Opacity, 0.0, 0.3, Ease::Linear);
    let mut s = fixture();
    s.doc.layers.clear();
    let mut layer = Layer::vector("Destination");
    let frame = crate::layout::make_frame(Pt::ZERO, Pt::new(240.0, 160.0));
    let frame_id = frame.id;
    layer.kind.shapes_mut().unwrap().push(frame);
    s.doc.layers.push(layer);
    s.selection.clear();
    let before = crate::project::encode(&s.doc).unwrap();
    let full = Bounds::from_min_size(Pt::ZERO, source.doc.size());
    s.place_imported_at(crate::import::Imported::Document(source.doc.clone()), Pt::ZERO, Some((full, full))).unwrap();
    let imported_id = s.doc.layers[1].id;
    assert_eq!(s.doc.motion.pose(imported_id, 1.0).dx, 40.0);
    let imported = crate::project::encode(&s.doc).unwrap();
    s.selection = vec![(1, RASTER_ID)];
    s.reparent_layout_selection(Some(frame_id));
    let (_, moved) = s.selection[0];
    assert_ne!(moved, RASTER_ID);
    assert!(!s.doc.motion.has_shape(imported_id));
    assert_eq!(s.doc.motion.pose(moved, 1.0).dx, 40.0);
    s.undo();
    assert_eq!(crate::project::encode(&s.doc).unwrap(), imported);
    s.undo();
    assert_eq!(crate::project::encode(&s.doc).unwrap(), before);
    s.place_imported_in_frame(crate::import::Imported::Document(source.doc), Pt::new(120.0, 80.0), (0, frame_id), Some(full)).unwrap();
    let track = s.doc.motion.tracks.iter().find(|t| t.prop == Prop::X).unwrap();
    assert_eq!(s.doc.motion.pose(track.shape, 1.0).dx, 40.0);
    let wrapper = s.doc.find_shape(0, track.shape).unwrap();
    assert_eq!(wrapper.opacity, 0.3);
    assert_eq!(wrapper.geom.bbox().size(), Pt::splat(20.0));
    s.undo();
    assert_eq!(crate::project::encode(&s.doc).unwrap(), before);
}

#[test]
fn raster_nudges_and_first_opacity_property_key_use_layer_identity_and_rest_alpha() {
    let mut s = fixture();
    let id = s.doc.layers[0].id;
    s.selection = vec![(0, RASTER_ID)];
    s.doc.layers[0].opacity = 0.3;
    s.playhead = 1.;
    s.key_prop(id, Prop::Opacity, 0.7);
    assert_eq!(s.doc.motion.pose(id, 0.).opacity, Some(0.3));
    assert_eq!(s.doc.motion.pose(id, 1.).opacity, Some(0.7));
    s.nudge(10., -5.);
    assert_eq!(s.doc.motion.pose(id, 1.).dx, 10.);
    assert_eq!(s.doc.motion.pose(id, 1.).dy, -5.);
    assert!(!s.doc.motion.has_shape(RASTER_ID));
}
