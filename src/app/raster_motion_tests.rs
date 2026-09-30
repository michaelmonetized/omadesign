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
