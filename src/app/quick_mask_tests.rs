use super::*;
use crate::{geom::Pt, tools::Tool};
pub(crate) fn fixture() -> Studio {
    let mut s = Studio::new();
    s.show_welcome = false;
    s.doc = crate::channels::tests::fixture();
    s.active_layer = Some(0);
    s.persona = Persona::Pixel;
    s.tool = Tool::Brush;
    s.brushes.selected = None;
    s.brush = crate::paint::Brush {
        color: Rgba::from_hex(0xE5484D),
        size: 3.,
        hardness: 1.,
        opacity: 0.6,
        flow: 1.,
        spacing: 0.2,
        ..Default::default()
    };
    s
}
fn bytes(s: &Studio) -> String {
    crate::project::encode(&s.doc).unwrap()
}
fn artwork(s: &Studio) -> serde_json::Value {
    let mut value = serde_json::to_value(&s.doc).unwrap();
    value.as_object_mut().unwrap().remove("quick_mask");
    value
}
#[test]
fn craft_quick_mask_enter_exit_restores_exact_translated_selection_and_history() {
    let mut s = fixture();
    let space = SelectionSpace {
        w: 2,
        h: 1,
        transform: tiny_skia::Transform::from_translate(2., 1.),
    };
    s.replace_pixel_selection(vec![64, 192], space);
    let original = s.pixel_sel.clone().unwrap();
    let before = bytes(&s);
    let history = s.history.len();
    let foreground = s.brush.color;
    s.set_quick_mask(true).unwrap();
    assert_eq!(s.history.len(), history + 1);
    assert!(s.pixel_sel.is_none());
    assert_eq!(s.brush.color, Rgba::BLACK);
    let mask = s.doc.quick_mask.as_ref().unwrap();
    assert!(!mask.unselected);
    assert_eq!(
        (mask.channel.plane.width, mask.channel.plane.height),
        (8, 4)
    );
    let mut expected = [0u8; 32];
    expected[10] = 64;
    expected[11] = 192;
    for (v, byte) in mask.channel.plane.samples.iter().zip(expected) {
        assert_eq!(*v, f32::from(byte) / 255.);
    }
    let entered = bytes(&s);
    s.undo();
    assert!(s.doc.quick_mask.is_none());
    assert!(Arc::ptr_eq(s.pixel_sel.as_ref().unwrap(), &original));
    assert_eq!(s.pixel_sel_space, Some(space));
    assert_eq!(s.brush.color, foreground);
    assert_eq!(bytes(&s), before);
    s.redo();
    assert_eq!(bytes(&s), entered);
    assert!(s.pixel_sel.is_none());
    s.set_quick_mask(false).unwrap();
    assert_eq!(s.pixel_sel.as_ref().unwrap().as_slice(), &expected.map(|v|f32::from(v)/255.));
    assert_eq!(
        s.pixel_selection_space().unwrap().transform,
        tiny_skia::Transform::identity()
    );
    assert_eq!(bytes(&s), before);
    s.undo();
    assert_eq!(bytes(&s), entered);
    assert!(s.pixel_sel.is_none());
    s.undo();
    assert_eq!(bytes(&s), before);
    assert!(Arc::ptr_eq(s.pixel_sel.as_ref().unwrap(), &original));
    s.redo();
    s.redo();
    assert_eq!(bytes(&s), before);
    assert_eq!(s.pixel_sel.as_ref().unwrap().as_slice(), &expected.map(|v|f32::from(v)/255.));
}
#[test]
fn craft_quick_mask_native_brush_eraser_cancel_curves_fill_and_reopen_leave_artwork_exact() {
    let mut s = fixture();
    let original = artwork(&s);
    let history = s.history.len();
    s.set_quick_mask(true).unwrap();
    let entered = bytes(&s);
    let id = s.doc.quick_mask.as_ref().unwrap().channel.id;
    assert!(s.start_channel_stroke(Pt::new(2., 2.), false));
    s.drag_channel_stroke(Pt::new(5., 2.));
    s.end_pixel_stroke(false);
    let painted = bytes(&s);
    assert_ne!(painted, entered);
    assert_eq!(s.history.len(), history + 2);
    assert_eq!(artwork(&s), original);
    let samples = s
        .doc
        .quick_mask
        .as_ref()
        .unwrap()
        .channel
        .plane
        .samples
        .clone();
    assert!(samples.iter().any(|v| *v < 1.));
    s.start_channel_stroke(Pt::new(2., 2.), true);
    s.drag_channel_stroke(Pt::new(5., 2.));
    s.end_pixel_stroke(false);
    let erased = bytes(&s);
    assert_ne!(erased, painted);
    assert!(
        s.doc
            .quick_mask
            .as_ref()
            .unwrap()
            .channel
            .plane
            .samples
            .iter()
            .zip(samples.iter())
            .any(|(a, b)| a > b)
    );
    s.start_channel_stroke(Pt::new(1., 1.), false);
    s.drag_channel_stroke(Pt::new(6., 1.));
    s.end_pixel_stroke(true);
    assert_eq!(bytes(&s), erased);
    s.undo();
    assert_eq!(bytes(&s), painted);
    s.redo();
    assert_eq!(bytes(&s), erased);
    let options = Options {
        color: Rgba::from_hex(0x36A8CC),
        opacity: 0.35,
        indicates: Indicates::Selected,
    };
    let source = s
        .doc
        .quick_mask
        .as_ref()
        .unwrap()
        .channel
        .plane
        .samples
        .clone();
    s.set_quick_mask_options(options).unwrap();
    assert!(Arc::ptr_eq(
        &s.doc.quick_mask.as_ref().unwrap().channel.plane.samples,
        &source
    ));
    let captured = crate::app::channel_adjustment::Captured::new(&mut s).unwrap();
    let curve = crate::channels::curve::Curve::Points(vec![[0., 1.], [1., 0.]]);
    let adjusted = captured.render(&curve, None, &storage::check).unwrap();
    captured.commit(&mut s, adjusted).unwrap();
    s.brush.color = Rgba::WHITE;
    s.brush.opacity = 1.;
    s.fill_tolerance = 0.;
    s.set_tool(Tool::Fill);
    let n = s.history.len();
    assert!(s.fill_channel(Pt::new(0., 0.)));
    assert_eq!(s.history.len(), n + 1);
    assert_eq!(s.doc.quick_mask.as_ref().unwrap().channel.id, id);
    assert_eq!(artwork(&s), original);
    let encoded = bytes(&s);
    let decoded = crate::project::decode(&encoded).unwrap();
    assert_eq!(crate::project::encode(&decoded).unwrap(), encoded);
    if let Some(root) = std::env::var_os("OMADESIGN_QUICK_MASK_QA") {
        let root = std::path::PathBuf::from(root);
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("painted-mask.oma"), encoded).unwrap();
        std::fs::write(root.join("artwork.json"), original.to_string()).unwrap();
    }
    s.set_quick_mask(false).unwrap();
    assert!(s.pixel_sel.is_some());
    assert!(captured.commit(&mut s, captured.source.clone()).is_err());
    assert_eq!(artwork(&s), original);
}
#[test]
fn craft_quick_mask_defaults_no_selection_options_and_cancelled_owners_are_atomic() {
    let mut s = fixture();
    let original = bytes(&s);
    let history = s.history.len();
    let options = Options {
        indicates: Indicates::Selected,
        ..Default::default()
    };
    s.set_quick_mask_options(options).unwrap();
    assert_eq!(s.history.len(), history);
    assert_eq!(bytes(&s), original);
    s.set_quick_mask(true).unwrap();
    assert!(
        s.doc
            .quick_mask
            .as_ref()
            .unwrap()
            .channel
            .plane
            .samples
            .iter()
            .all(|v| *v == 0.)
    );
    s.set_quick_mask(false).unwrap();
    assert!(s.pixel_sel.is_none());
    assert_eq!(bytes(&s), original);
    s.set_quick_mask(true).unwrap();
    let before = bytes(&s);
    s.start_channel_stroke(Pt::new(2., 2.), true);
    s.canvas_gen += 1;
    s.drag_channel_stroke(Pt::new(5., 2.));
    assert!(s.op.is_none());
    assert_eq!(bytes(&s), before);
    let tool = s.tool;
    s.set_tool(Tool::Clone);
    assert_eq!(s.tool, tool);
    assert!(s.status.contains("Exit Quick Mask"));
    assert_eq!(bytes(&s), before);
    let mut invalid = options;
    invalid.opacity = f32::NAN;
    assert!(s.set_quick_mask_options(invalid).is_err());
    assert_eq!(bytes(&s), before);
    s.ensure_tabs();
    s.new_tab();
    assert!(s.doc.quick_mask.is_none());
    s.switch_tab(0);
    assert_eq!(bytes(&s), before);
    assert_eq!(s.brush.color, Rgba::BLACK);
    s.set_quick_mask(false).unwrap();
    assert!(s.pixel_sel.is_none());
    assert_eq!(bytes(&s), original);
}

#[test]
fn craft_selection_precision_quick_mask_channel_transition_preserves_every_authored_bit() {
    let mut s=fixture();
    s.set_quick_mask(true).unwrap();
    let source=storage::collect((0..32).map(|i|match i{0=>-0.,1=>f32::from_bits(0x3e800001),2=>f32::from_bits(0x3eaaaaab),_=>i as f32/32.})).unwrap();
    s.doc.quick_mask.as_mut().unwrap().channel.plane.samples=source.clone();
    let quick=bytes(&s);
    s.set_quick_mask(false).unwrap();
    assert!(Arc::ptr_eq(s.pixel_sel.as_ref().unwrap(),&source));
    for(a,b)in s.pixel_sel.as_ref().unwrap().iter().zip(source.iter()){assert_eq!(a.to_bits(),b.to_bits());}
    let space=s.pixel_selection_space().unwrap();
    let target=s.capture_channels().unwrap();
    let plane=Plane::from_coverage(source.clone(),space.w,space.h,space.transform).unwrap();
    s.save_channel(&target,plane,"Exact selection").unwrap();
    let saved=bytes(&s);let reopened=crate::project::decode(&saved).unwrap();
    for(a,b)in reopened.channels.last().unwrap().plane.samples.iter().zip(source.iter()){assert_eq!(a.to_bits(),b.to_bits());}
    s.undo();s.undo();assert_eq!(bytes(&s),quick);s.redo();
    assert!(Arc::ptr_eq(s.pixel_sel.as_ref().unwrap(),&source));
    s.set_quick_mask(true).unwrap();assert!(Arc::ptr_eq(&s.doc.quick_mask.as_ref().unwrap().channel.plane.samples,&source));
    if let Ok(folder)=std::env::var("OMADESIGN_SELECTION_PRECISION_QA") {std::fs::create_dir_all(&folder).unwrap();std::fs::write(format!("{folder}/exact-selection.oma"),saved).unwrap();std::fs::write(format!("{folder}/quick-mask.oma"),quick).unwrap();}
}

#[test]
fn craft_selection_precision_float_weights_reach_scalar_curves_native_filters_and_brush() {
    let mut s=fixture();let space=SelectionSpace{w:8,h:4,transform:tiny_skia::Transform::identity()};
    let weight=f32::from_bits(0x3eaa8001);s.replace_pixel_selection_float(vec![weight;32],space);
    let plane=Plane::from_coverage(storage::collect(std::iter::repeat_n(0.25,32)).unwrap(),8,4,space.transform).unwrap();
    let target=s.capture_channels().unwrap();let id=s.save_channel(&target,plane,"Weighted alpha").unwrap();s.select_channel_target(Some(id)).unwrap();
    s.brush.color=Rgba::WHITE;s.brush.opacity=0.6;
    let before=s.doc.channels[0].plane.samples.clone();s.start_channel_stroke(Pt::new(3.,2.),false);s.drag_channel_stroke(Pt::new(5.,2.));s.end_pixel_stroke(false);
    assert!(s.doc.channels[0].plane.samples.iter().zip(before.iter()).any(|(a,b)|a.to_bits()!=b.to_bits()));
    s.undo();assert_eq!(s.doc.channels[0].plane.samples,before);
    let result=crate::channels::curve::apply(&s.doc.channels[0].plane,&crate::channels::curve::Curve::Points(vec![[0.,1.],[1.,1.]]),Some(s.pixel_sel.as_ref().unwrap()),&storage::check).unwrap();
    let expected=0.25+(1.-0.25)*weight;let rounded=0.25+(1.-0.25)*(weight*255.).round()/255.;
    assert_eq!(result.samples[0].to_bits(),expected.to_bits());assert!((result.samples[0]-rounded).abs()>1e-4);
    let pixels=crate::document::Pixels::from_samples(1,1,crate::deep_color::Depth::F32,vec![0.25,0.5,0.75,1.]).unwrap();
    let out=pixels.filtered(&crate::raster::Settings::new(crate::raster::Kind::Invert),Some(&[weight])).unwrap();
    for(c,a)in out.sample(0).iter().take(3).enumerate(){let v=pixels.sample(0)[c];assert!((*a-(v+(1.-v-v)*weight)).abs()<1e-7);}
}
