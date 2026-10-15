use super::*;
use crate::{
    document::{Cmd, Document, Layer},
    geom::Pt,
};

#[test]
fn native_samples_survive_project_masks_filters_undo_and_psd() {
    for depth in [Depth::U16, Depth::F32] {
        let values = if depth == Depth::U16 {
            vec![1001. / 65535., 1002. / 65535., 0.7, 1.]
        } else {
            vec![1.25, 0.1234567, -0.125, 1.]
        };
        let pixels = Pixels::from_samples(1, 1, depth, values.clone()).unwrap();
        let mut doc = Document::new("Native", 1., 1., 72.);
        doc.transparent = true;
        doc.layers = vec![Layer::placed_raster(
            "Native",
            pixels.clone(),
            Pt::ZERO,
            Pt::new(1., 1.),
        )];
        let saved = crate::project::encode(&doc).unwrap();
        let mut loaded = crate::project::decode(&saved).unwrap();
        assert_eq!(
            loaded.layers[0]
                .kind
                .pixels()
                .unwrap()
                .samples()
                .unwrap()
                .as_slice()
                .to_vec(),
            values
        );
        let mut settings = crate::raster::Settings::new(crate::raster::Kind::Exposure);
        settings.values[0] = 0.5;
        let edited = pixels.filtered(&settings, None).unwrap();
        assert!((edited.sample(0)[0] - values[0] * 2f32.sqrt()).abs() < 1e-6);
        let cmd = Cmd::NativePixels {
            layer: 0,
            mask: false,
            before: pixels.clone(),
            after: edited,
        };
        crate::document::apply(&mut loaded, &cmd);
        crate::document::apply(&mut loaded, &crate::document::invert_cmd(cmd));
        assert_eq!(
            loaded.layers[0]
                .kind
                .pixels()
                .unwrap()
                .samples()
                .unwrap()
                .as_slice()
                .to_vec(),
            values
        );
        for large in [false, true] {
            let out = crate::formats::psd::encode(&doc, large).unwrap();
            assert_eq!(
                u16::from_be_bytes(out.bytes[22..24].try_into().unwrap()),
                depth.bits()
            );
            let imported = crate::formats::psd::decode(&out.bytes, "Native").unwrap();
            let got = imported.document.layers[0].kind.pixels().unwrap().sample(0);
            for c in 0..4 {
                assert!(
                    (got[c] - values[c]).abs() < 1. / 65535. + 1e-7,
                    "{got:?} != {values:?}"
                );
            }
        }
        let mask =
            Pixels::from_samples(1, 1, Depth::U16, vec![0.50001, 0.50001, 0.50001, 1.]).unwrap();
        doc.layers[0].mask = Some(mask.clone());
        let out = composite(&doc, 1).unwrap();
        assert!((out.sample(0)[3] - 0.50001).abs() < 1e-6);
        let masked = pixels.masked(&mask).unwrap();
        assert!((masked.sample(0)[3] - 0.50001).abs() < 1e-6);
    }
}

#[test]
fn float_blending_retains_sub_byte_values_and_hdr() {
    let b = [0.123456, 0.4, 0.8, 1.];
    let s = [0.333333, 0.7, 1.5, 0.50001];
    let out = blend_pixel(b, s, crate::color::Blend::Normal, 1.);
    assert!((out[0] - (b[0] * (1. - s[3]) + s[0] * s[3])).abs() < 1e-7);
    assert!(blend_pixel([0.; 4], [2., -0.2, 1., 1.], crate::color::Blend::Normal, 1.)[0] > 1.);
}

#[test]
fn independent_raw_depth_fixtures_keep_adjacent_samples_and_hdr() {
    for (bytes, depth, expected) in [
        (
            include_bytes!("../formats/psd/fixtures/independent-16.psd").as_slice(),
            Depth::U16,
            [1000. / 65535., 32767. / 65535., 60000. / 65535., 1.],
        ),
        (
            include_bytes!("../formats/psd/fixtures/independent-32.psd").as_slice(),
            Depth::F32,
            [1.25, 0.1234567, -0.125, 1.],
        ),
    ] {
        let doc = crate::formats::psd::decode(bytes, "Independent")
            .unwrap()
            .document;
        let p = doc.layers[0].kind.pixels().unwrap();
        assert_eq!(p.native.as_ref().unwrap().depth, depth);
        assert_eq!(p.sample(0), expected);
        assert_ne!(p.sample(0)[0], p.sample(1)[0]);
        let out = crate::formats::psd::encode(&doc, false).unwrap();
        if let Ok(dir) = std::env::var("OMADESIGN_DEEP_QA_DIR") {
            std::fs::write(
                std::path::Path::new(&dir).join(format!("native-{}.psd", depth.bits())),
                out.bytes,
            )
            .unwrap();
        }
    }
}

#[test]
fn psd_merged_alpha_uses_white_matte_without_changing_layer_samples() {
    for depth in [Depth::U8, Depth::U16, Depth::F32] {
        let values = if depth == Depth::F32 {
            vec![1.25, 0.1234567, -0.125, 0.5, 0.125, 0.5, 0.75, 0.25]
        } else {
            vec![0.25, 0.125, 0.75, 0.5, 0.125, 0.5, 0.75, 0.25]
        };
        let pixels = Pixels::from_samples(2, 1, depth, values).unwrap();
        let mut doc = Document::new("Merged alpha", 2., 1., 72.);
        doc.transparent = true;
        doc.layers = vec![Layer::placed_raster("Alpha", pixels.clone(), Pt::ZERO, Pt::new(2., 1.))];
        for large in [false, true] {
            let encoded = crate::formats::psd::encode(&doc, large).unwrap();
            let bps = depth.bits() as usize / 8;
            let merged = &encoded.bytes[encoded.bytes.len() - 8 * bps..];
            let sample = |c: usize, i: usize| {
                let at = (c * 2 + i) * bps;
                match depth {
                    Depth::U8 => merged[at] as f32 / 255.,
                    Depth::U16 => u16::from_be_bytes(merged[at..at + 2].try_into().unwrap()) as f32 / 65535.,
                    Depth::F32 => f32::from_be_bytes(merged[at..at + 4].try_into().unwrap()),
                }
            };
            let tolerance = match depth {Depth::U8 => 2. / 255., Depth::U16 => 2. / 65535., Depth::F32 => 1e-7};
            for i in 0..2 {
                let original = pixels.sample(i);
                for c in 0..3 {
                    assert!((sample(c, i) - (original[c] * original[3] + 1. - original[3])).abs() < tolerance);
                }
                assert!((sample(3, i) - original[3]).abs() < tolerance);
            }
            let loaded = crate::formats::psd::decode(&encoded.bytes, "Alpha").unwrap();
            let layer = loaded.document.layers[0].kind.pixels().unwrap();
            for i in 0..2 {for c in 0..4 {assert!((layer.sample(i)[c] - pixels.sample(i)[c]).abs() < tolerance);}}
            if !large && let Ok(dir) = std::env::var("OMADESIGN_DEEP_QA_DIR") {
                std::fs::write(std::path::Path::new(&dir).join(format!("native-alpha-{}.psd", depth.bits())), encoded.bytes).unwrap();
            }
        }
    }
}

#[test]
fn native_mask_resize_and_tiny_alpha_keep_authored_values() {
    let pixels =
        Pixels::from_samples(1, 1, Depth::F32, vec![1.25, 0.1234567, -0.125, 0.00001]).unwrap();
    assert!(!pixels.is_invisible());
    let mask = Pixels::from_samples(
        2,
        1,
        Depth::U16,
        vec![
            1001. / 65535.,
            1001. / 65535.,
            1001. / 65535.,
            1.,
            1002. / 65535.,
            1002. / 65535.,
            1002. / 65535.,
            1.,
        ],
    )
    .unwrap();
    let resized = mask.resized(4, 1).unwrap();
    assert!(resized.sample(1)[0] > mask.sample(0)[0]);
    assert!(resized.sample(1)[0] < mask.sample(1)[0]);
    let mut invalid = pixels;
    invalid.native.as_mut().unwrap().samples = storage::adopt(Vec::new()).unwrap();
    assert!(invalid.validate_native().is_err());
}

#[test]
fn native_clipping_uses_authoritative_alpha_and_mask_samples() {
    let mut doc = Document::new("Clipping", 1., 1., 72.);
    let mut base = Layer::placed_raster(
        "Base",
        Pixels::from_samples(1, 1, Depth::F32, vec![1., 0., 0., 0.50001]).unwrap(),
        Pt::ZERO,
        Pt::new(1., 1.),
    );
    base.mask = Some(
        Pixels::from_samples(1, 1, Depth::F32, vec![0.333333, 0.333333, 0.333333, 1.]).unwrap(),
    );
    let mut clipped = Layer::placed_raster(
        "Clipped",
        Pixels::from_samples(1, 1, Depth::F32, vec![0., 1., 0., 1.]).unwrap(),
        Pt::ZERO,
        Pt::new(1., 1.),
    );
    clipped.mask =
        Some(Pixels::from_samples(1, 1, Depth::F32, vec![0.70001, 0.70001, 0.70001, 1.]).unwrap());
    doc.layers = vec![base, clipped];
    crate::formats::psd::test_native_clipping(&mut doc);
    assert!(
        (doc.layers[1].mask.as_ref().unwrap().sample(0)[0] - 0.50001 * 0.333333 * 0.70001).abs()
            < 1e-7
    );
}

#[test]
fn raster_export_preserves_native_color_and_float_mask_precision() {
    use crate::export::{self, Format, Settings, Source};
    let mut doc = Document::new("Export", 2., 1., 72.);
    doc.transparent = true;
    doc.layers = vec![Layer::placed_raster(
        "Native",
        Pixels::from_samples(
            2,
            1,
            Depth::U16,
            vec![1000. / 65535., 0.25, 0.5, 1., 1001. / 65535., 0.5, 0.75, 1.],
        )
        .unwrap(),
        Pt::ZERO,
        Pt::new(2., 1.),
    )];
    for format in [Format::Png, Format::Tiff] {
        let (bytes, _) = export::encode(
            &Source::Document(doc.clone()),
            Settings {
                format,
                scale: 1,
                ai: None,
            },
            &crate::ml::NoProgress,
        )
        .unwrap();
        let decoded = image::load_from_memory(&bytes).unwrap();
        assert_eq!(decoded.color(), image::ColorType::Rgba16);
        let pixels = decoded.to_rgba16();
        assert_eq!(pixels.get_pixel(0, 0)[0], 1000);
        assert_eq!(pixels.get_pixel(1, 0)[0], 1001);
        if format == Format::Png
            && let Ok(dir) = std::env::var("OMADESIGN_DEEP_QA_DIR")
        {
            std::fs::write(std::path::Path::new(&dir).join("native-16.png"), bytes).unwrap();
        }
    }
    doc.layers[0].mask = Some(
        Pixels::from_samples(
            2,
            1,
            Depth::F32,
            vec![
                0.333333, 0.333333, 0.333333, 1., 0.777777, 0.777777, 0.777777, 1.,
            ],
        )
        .unwrap(),
    );
    let (bytes, _) = export::encode(
        &Source::Document(doc.clone()),
        Settings {
            format: Format::Tiff,
            scale: 1,
            ai: None,
        },
        &crate::ml::NoProgress,
    )
    .unwrap();
    let decoded = image::load_from_memory(&bytes).unwrap();
    assert_eq!(decoded.color(), image::ColorType::Rgba32F);
    assert!((decoded.to_rgba32f().get_pixel(0, 0)[3] - 0.333333).abs() < 1e-7);
    if let Ok(dir) = std::env::var("OMADESIGN_DEEP_QA_DIR") {
        std::fs::write(
            std::path::Path::new(&dir).join("native-float-mask.tif"),
            bytes,
        )
        .unwrap();
    }
    assert!(crate::compositor::export_png(&doc, 1).is_err());
    assert!(crate::svg::export(&doc).is_err());
    assert!(crate::formats::pdf::write(&doc).is_err());
    assert!(crate::formats::openraster::write(&doc).is_err());
}

#[test]
fn native_mask_on_vector_layer_keeps_float_coverage() {
    let mut doc = Document::new("Vector mask", 2., 2., 72.);
    doc.transparent = true;
    let mut layer = Layer::vector("Vector");
    layer
        .kind
        .shapes_mut()
        .unwrap()
        .push(crate::document::Shape::new(
            crate::geom::Geom::Rect {
                origin: Pt::ZERO,
                size: Pt::new(2., 2.),
                radius: 0.,
            },
            crate::document::Style::default(),
        ));
    layer.mask = Some(
        Pixels::from_samples(2, 2, Depth::F32, [0.50001, 0.50001, 0.50001, 1.].repeat(4)).unwrap(),
    );
    doc.layers = vec![layer];
    let out = composite(&doc, 1).unwrap();
    assert!((out.sample(0)[3] - 0.50001).abs() < 1e-7);
}

#[test]
fn native_storage_shares_history_and_adopts_one_allocation_lease() {
    use std::sync::Arc;
    let mut authored = resources::global()
        .allocate::<f32, _>(8, &storage::check)
        .unwrap();
    authored
        .as_mut_slice()
        .copy_from_slice(&[1.25, -0.125, 0.1234567, 0.50001, 0.1, 0.2, 0.3, 1.]);
    let (values, lease) = authored.into_parts();
    let original =
        Pixels::from_admitted_samples(2, 1, Depth::F32, values, lease, &storage::check).unwrap();
    let before = original.clone();
    assert!(Arc::ptr_eq(&original.data, &before.data));
    assert!(Arc::ptr_eq(
        &original.native.as_ref().unwrap().samples,
        &before.native.as_ref().unwrap().samples
    ));
    let admitted = resources::global().usage().0;
    let history = vec![before.clone(); 32];
    assert_eq!(resources::global().usage().0, admitted);
    let mut working = before.copy_samples(&storage::check).unwrap();
    working.as_mut_slice()[0] = 1.125;
    let mut after = before.clone();
    after.set_sample_allocation(Arc::new(working)).unwrap();
    assert_eq!(original.sample(0)[0], 1.25);
    assert_eq!(after.sample(0)[0], 1.125);
    assert!(!Arc::ptr_eq(&after.data, &before.data));
    assert_eq!(history[0].sample(0), original.sample(0));
    assert!(after.bytes_mut().is_err());
    assert_eq!(storage::count(12_000, 9_000, 4).unwrap(), 432_000_000);
    assert!(storage::count(u32::MAX, u32::MAX, 4).is_err());
    let cancelled = || Err("cancelled".to_string());
    assert!(before.copy_samples(&cancelled).is_err());
    assert_eq!(before.sample(0), original.sample(0));
}

#[test]
fn byte_history_swaps_prepared_storage_when_admission_is_full() {
    use std::sync::Arc;
    use crate::document::{Cmd, History, apply as apply_command};
    let pool = resources::Budget::new(12, false);
    let before = Arc::new(pool.copy_now(&[90u8,90,90,90], &storage::check).unwrap());
    let after = Arc::new(pool.copy_now(&[123u8,45,67,89], &storage::check).unwrap());
    let full = pool.reserve_now(4, &storage::check).unwrap();
    let mut doc = Document::new("Prepared byte history", 1.,1.,72.);
    doc.layers = vec![Layer::placed_raster("Pixels", Pixels::from_byte_allocation(1,1,before.clone()).unwrap(),Pt::ZERO,Pt::new(1.,1.))];
    let cmd = Cmd::Pixels {layer:0,mask:false,before:before.clone(),after:after.clone()};
    let mut history = History::default();
    apply_command(&mut doc, &cmd);history.push(cmd);
    assert!(Arc::ptr_eq(&doc.layers[0].kind.pixels().unwrap().data,&after));
    apply_command(&mut doc, &history.undo().unwrap());
    assert!(Arc::ptr_eq(&doc.layers[0].kind.pixels().unwrap().data,&before));
    apply_command(&mut doc, &history.redo().unwrap());
    assert!(Arc::ptr_eq(&doc.layers[0].kind.pixels().unwrap().data,&after));
    assert_eq!(pool.usage().0,12);drop(full);
}

#[test]
fn paint_surface_retains_transferred_allocation_until_drop() {
    let pool = resources::Budget::new(4, false);
    let mut bytes = pool.allocate_now::<u8,_>(4,&storage::check).unwrap();
    bytes.as_mut_slice().copy_from_slice(&[31,16,8,90]);
    let surface = surface::Surface::from_allocation(1,1,bytes).unwrap();
    assert_eq!(surface.data(), &[31,16,8,90]);
    assert_eq!(surface.admitted_bytes(),4);
    assert!(pool.reserve_now(1,&storage::check).is_err());
    drop(surface);assert_eq!(pool.usage().0,0);
}

#[test]
fn native_fill_distinguishes_adjacent_samples_sharing_one_display_byte() {
    let pixels = Pixels::from_samples(3,1,Depth::U16,vec![1000. / 65535.,0.2,0.3,1.,1001. / 65535.,0.2,0.3,1.,1000. / 65535.,0.2,0.3,1.]).unwrap();
    assert_eq!(&pixels.data[..4], &pixels.data[4..8]);
    let region = retouch::flood_region(&pixels,Pt::ZERO,0.,None).unwrap();
    assert_eq!(region.as_slice(), &[255,0,0]);
    let region = retouch::flood_region(&pixels,Pt::ZERO,1.,Some(&[128,0,255])).unwrap();
    assert_eq!(region.as_slice(), &[128,0,0]);
}

#[test]
fn native_export_dimensions_use_checked_storage_instead_of_ai_canvas_limits() {
    use crate::export::{self, Source, Settings, Format};
    let mut doc = Document::new("Wide native export",100_000.,1.,72.);
    doc.layers=vec![Layer::placed_raster("Source",Pixels::from_samples(1,1,Depth::F32,vec![1.25,0.1234567,-0.125,1.]).unwrap(),Pt::ZERO,Pt::new(100_000.,1.))];
    assert_eq!(export::dimensions(&Source::Document(doc),Settings {format:Format::Tiff,..Default::default()}).unwrap(),(100_000,1));
}

#[test]
fn byte_vector_effects_are_refused_before_native_export_reduction() {
    let mut doc = Document::new("Native vector effect boundary",2.,2.,72.);
    let mut layer = Layer::vector("Filtered vector");
    let mut shape = crate::document::Shape::new(crate::geom::Geom::Rect {origin:Pt::ZERO,size:Pt::new(2.,2.),radius:0.},crate::document::Style::default());
    shape.filters.items.push(crate::filter::Fx::Blur {std:1.});
    layer.kind.shapes_mut().unwrap().push(shape);
    layer.mask = Some(Pixels::from_samples(2,2,Depth::F32,[0.50001,0.50001,0.50001,1.].repeat(4)).unwrap());
    doc.layers=vec![layer];
    let before=doc.layers[0].mask.as_ref().unwrap().sample(0);
    assert!(composite(&doc,1).unwrap_err().contains("float paint renderer"));
    let reopened=crate::project::decode(&crate::project::encode(&doc).unwrap()).unwrap();
    assert_eq!(reopened.layers[0].mask.as_ref().unwrap().sample(0),before);
    assert!(reopened.layers[0].kind.shapes().unwrap()[0].filters.active());
}
