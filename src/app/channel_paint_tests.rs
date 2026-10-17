use super::*;
use crate::color::Rgba;

fn studio() -> Studio {
    let mut s = Studio::new();
    s.show_welcome = false;
    s.doc = crate::channels::tests::fixture();
    let plane = Plane {
        width: 32,
        height: 16,
        transform: [1., 0., 0., 1., 0., 0.],
        samples: storage::collect((0..512).map(|i| f32::from_bits(0x3e800001 + i))).unwrap(),
    };
    s.doc.channels = vec![Channel {
        id: crate::document::next_id(),
        name: "Paint alpha".into(),
        plane,
    }];
    s.active_layer = Some(0);
    s.brushes.selected = None;
    s.brush = paint::Brush {
        size: 6.,
        hardness: 1.,
        flow: 1.,
        opacity: 0.6,
        spacing: 0.18,
        color: Rgba::WHITE,
        ..Default::default()
    };
    s.select_channel_target(Some(s.doc.channels[0].id)).unwrap();
    assert_eq!(s.persona, crate::tools::Persona::Pixel);
    assert!(Tool::Eraser.in_persona(s.persona));
    s
}
fn bytes(s: &Studio) -> String {
    crate::project::encode(&s.doc).unwrap()
}
#[test]
fn craft_channel_paint_round_tip_soft_selection_keeps_scalar_bits_and_one_full_undo_reopen() {
    let mut s = studio();
    let mut mask = vec![0; 512];
    for y in 0..16 {
        for x in 8..24 {
            mask[y * 32 + x] = 128;
        }
    }
    let space = SelectionSpace {
        w: 32,
        h: 16,
        transform: tiny_skia::Transform::identity(),
    };
    s.replace_pixel_selection(mask.clone(), space);
    let source = s.doc.channels[0].plane.samples.clone();
    let before = bytes(&s);
    let n = s.history.len();
    let mut coverage = Surface::new(32, 16).unwrap();
    let brush = paint::Brush {
        opacity: 1.,
        ..s.brush
    };
    paint::stamp(&mut coverage, Pt::new(5., 8.), &brush, false);
    let mut distance = brush.spacing_distance();
    paint::stroke_spaced(
        &mut coverage,
        Pt::new(5., 8.),
        Pt::new(25., 8.),
        &brush,
        false,
        &mut distance,
    );
    s.start_channel_stroke(Pt::new(5., 8.), false);
    s.drag_channel_stroke(Pt::new(25., 8.));
    s.end_pixel_stroke(false);
    assert_eq!(s.history.len(), n + 1);
    assert!(!Arc::ptr_eq(&source, &s.doc.channels[0].plane.samples));
    for i in 0..512 {
        let amount =
            f32::from(coverage.pixels()[i].alpha()) / 255. * 0.6 * (f32::from(mask[i]) / 255.);
        let expected = if amount > 0. {
            (source[i] + (1. - source[i]) * amount).clamp(0., 1.)
        } else {
            source[i]
        };
        assert_eq!(
            s.doc.channels[0].plane.samples[i].to_bits(),
            expected.to_bits(),
            "sample {i}"
        );
    }
    let after = bytes(&s);
    assert_ne!(after, before);
    s.undo();
    assert_eq!(bytes(&s), before);
    s.redo();
    assert_eq!(bytes(&s), after);
    assert_eq!(
        crate::project::encode(&crate::project::decode(&after).unwrap()).unwrap(),
        after
    );
    let mut doc = s.doc.clone();
    doc.channels.clear();
    let mut original = crate::project::decode(&before).unwrap();
    original.channels.clear();
    assert_eq!(
        crate::project::encode(&doc).unwrap(),
        crate::project::encode(&original).unwrap()
    );
    if let Some(root) = std::env::var_os("OMADESIGN_CHANNEL_PAINT_QA") {
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(
            std::path::Path::new(&root).join("painted-channel.oma"),
            after,
        )
        .unwrap();
    }
}
#[test]
fn craft_channel_paint_pack_tip_rotated_grid_eraser_and_final_alias_use_existing_dabs() {
    let mut s = studio();
    s.select_brush(0, 0);
    s.brush.size = 8.;
    s.brush.opacity = 0.5;
    s.brush.color = Rgba::BLACK;
    let transform = tiny_skia::Transform::from_translate(10., 15.).pre_rotate(30.);
    s.doc.channels[0].plane.transform = [
        transform.sx,
        transform.ky,
        transform.kx,
        transform.sy,
        transform.tx,
        transform.ty,
    ];
    let before = s.doc.channels[0].plane.samples.clone();
    let alias = Arc::downgrade(before.lease());
    let world = |x, y| {
        let mut p = tiny_skia::Point::from_xy(x, y);
        transform.map_point(&mut p);
        Pt::new(p.x, p.y)
    };
    let brush = paint::Brush {
        color: Rgba::WHITE,
        opacity: 1.,
        ..s.brush
    };
    s.start_channel_stroke(world(8., 8.), true);
    s.end_pixel_stroke(false);
    assert!(
        s.doc.channels[0]
            .plane
            .samples
            .iter()
            .zip(before.iter())
            .any(|(a, b)| a < b)
    );
    let mut mapped = tiny_skia::Point::from_xy(world(8., 8.).x, world(8., 8.).y);
    transform.invert().unwrap().map_point(&mut mapped);
    let mut exact = Surface::new(32, 16).unwrap();
    crate::brushes::stamp(
        &mut exact,
        Pt::new(mapped.x, mapped.y),
        &brush,
        s.brushes.tip().unwrap(),
        false,
    );
    for i in 0..512 {
        let amount = f32::from(exact.pixels()[i].alpha()) / 255. * 0.5;
        let expected = if amount > 0. {
            before[i] + (0. - before[i]) * amount
        } else {
            before[i]
        };
        assert_eq!(
            s.doc.channels[0].plane.samples[i].to_bits(),
            expected.to_bits()
        );
    }
    let held = s.doc.channels[0].plane.samples.clone();
    let lease = Arc::downgrade(held.lease());
    drop(s);
    drop(before);
    assert!(alias.upgrade().is_none());
    assert!(lease.upgrade().is_some());
    drop(held);
    assert!(lease.upgrade().is_none());
}
#[test]
fn craft_channel_paint_escape_selection_change_tab_and_target_changes_retire_gesture() {
    let mut s = studio();
    let before = bytes(&s);
    let n = s.history.len();
    s.start_channel_stroke(Pt::new(10., 8.), false);
    assert_ne!(bytes(&s), before);
    s.undo();
    assert_eq!(bytes(&s), before);
    assert_eq!(s.history.len(), n);
    s.start_channel_stroke(Pt::new(10., 8.), false);
    s.pixel_sel_gen += 1;
    s.drag_channel_stroke(Pt::new(12., 8.));
    assert!(s.op.is_none());
    assert_eq!(bytes(&s), before);
    s.start_channel_stroke(Pt::new(10., 8.), false);
    s.set_tool(Tool::Select);
    assert_eq!(s.channel_target, None);
    assert_eq!(bytes(&s), before);
    s.select_channel_target(Some(s.doc.channels[0].id)).unwrap();
    s.start_channel_stroke(Pt::new(10., 8.), false);
    s.ensure_tabs();
    s.new_tab();
    assert_eq!(s.channel_target, None);
    s.switch_tab(0);
    assert_eq!(bytes(&s), before);
    assert_eq!(s.channel_target, Some(s.doc.channels[0].id));
}
#[test]
fn craft_channel_paint_clear_and_clipboard_refusal_never_touch_layer_samples() {
    let mut s = studio();
    let source = bytes(&s);
    let original = serde_json::to_value(&s.doc.layers[0]).unwrap();
    let ctx = eframe::egui::Context::default();
    s.copy_selection(&ctx);
    assert!(s.status.contains("not available"));
    s.cut_selection(&ctx);
    s.request_clipboard_paste(&ctx, Some("omadesign-shapes:[]"));
    assert_eq!(bytes(&s), source);
    s.clear_selected_pixels();
    assert!(s.doc.channels[0].plane.samples.iter().all(|v| *v == 0.));
    assert_eq!(serde_json::to_value(&s.doc.layers[0]).unwrap(), original);
    s.undo();
    assert_eq!(bytes(&s), source);
    s.paint_mask = true;
    s.channel_target = None;
    assert!(s.channel_paint_target().is_none());
}
#[test]
fn craft_channel_paint_clips_extreme_off_grid_segments_without_unbounded_dab_work() {
    let mut remaining = 0.1;
    let brush = paint::Brush {
        size: 2.,
        spacing: 0.25,
        ..Default::default()
    };
    let radius = brush.stroke_radius();
    let (a, b, _) = bounded_segment(
        Pt::new(-1e20, 8.),
        Pt::new(1e20, 8.),
        32,
        16,
        &brush,
        &mut remaining,
    )
    .unwrap();
    assert_eq!(a, Pt::new(-radius, 8.));
    assert_eq!(b, Pt::new(32. + radius, 8.));
    let (a, b, _) = bounded_segment(
        Pt::new(-1e20, -1e20),
        Pt::new(1e20, 1e20),
        32,
        16,
        &brush,
        &mut remaining,
    )
    .unwrap();
    assert_eq!(a.x, a.y);
    assert_eq!(b.x, b.y);
    assert!(a.x >= -radius && b.y <= 16. + radius);
    let mut s = studio();
    let now = std::time::Instant::now();
    s.start_channel_stroke(Pt::new(-1e20, 8.), false);
    s.drag_channel_stroke(Pt::new(1e20, 8.));
    s.end_pixel_stroke(false);
    assert!(now.elapsed() < std::time::Duration::from_secs(5));
    assert!(s.doc.channels[0].plane.samples.iter().any(|v| *v > 0.3));
}
#[test]
fn craft_channel_paint_invalid_owners_and_brushes_preserve_document_and_admission() {
    let mut s = studio();
    let source = bytes(&s);
    let used = crate::resources::global().usage().0;
    let pending = crate::resources::global().pending_bytes();
    s.brush.opacity = f32::NAN;
    assert!(s.start_channel_stroke(Pt::new(8., 8.), false));
    assert!(s.op.is_none());
    assert_eq!(bytes(&s), source);
    s.brush.opacity = 0.5;
    assert!(s.start_channel_stroke(Pt::new(f32::INFINITY, 8.), false));
    assert!(s.op.is_none());
    assert_eq!(bytes(&s), source);
    assert!(s.select_channel_target(Some(u64::MAX)).is_err());
    assert_eq!(bytes(&s), source);
    s.start_channel_stroke(Pt::new(8., 8.), false);
    s.end_pixel_stroke(true);
    assert_eq!(bytes(&s), source);
    assert_eq!(crate::resources::global().usage().0, used);
    assert_eq!(crate::resources::global().pending_bytes(), pending);
}
