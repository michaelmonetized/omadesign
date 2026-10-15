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

#[test]
fn cmyk_profile_authoring_survives_masks_edits_projects_and_psd() {
    use super::cms::{DocumentColor, Model};
    let profile = include_bytes!("../formats/psd/fixtures/fogra39l-cc0.icc").to_vec();
    let inks = vec![
        1001. / 65535.,
        0.25,
        0.5,
        0.125,
        1.,
        1002. / 65535.,
        0.5,
        0.1,
        0.7,
        1.,
    ];
    let pixels = Pixels::from_cmyk(2, 1, Depth::U16, inks.clone(), profile.clone(), 1).unwrap();
    let device = Pixels::from_cmyk(2, 1, Depth::U16, inks.clone(), Vec::new(), 1).unwrap();
    assert_ne!(pixels.data.as_slice(), device.data.as_slice());
    let mut doc = Document::new("CMYK", 2., 1., 72.);
    doc.transparent = true;
    doc.color = DocumentColor {
        model: Model::Cmyk,
        depth: Depth::U16,
        icc_profile: profile.clone(),
        intent: 1,
    };
    doc.layers = vec![Layer::placed_raster(
        "Ink",
        pixels.clone(),
        Pt::ZERO,
        Pt::new(2., 1.),
    )];
    let doc = crate::project::decode(&crate::project::encode(&doc).unwrap()).unwrap();
    assert_eq!(doc.color.icc_profile, profile);
    assert_eq!(
        doc.layers[0]
            .kind
            .pixels()
            .unwrap()
            .native
            .as_ref()
            .unwrap()
            .cmyk
            .as_ref()
            .unwrap()
            .as_slice(),
        inks.as_slice()
    );
    let composite = super::composite(&doc, 1).unwrap();
    for (&a, &b) in composite
        .native
        .as_ref()
        .unwrap()
        .cmyk
        .as_ref()
        .unwrap()
        .iter()
        .zip(&inks)
    {
        assert!((a - b).abs() < 1e-6);
    }
    let mask = Pixels::from_samples(
        2,
        1,
        Depth::U16,
        vec![0.333333, 0.333333, 0.333333, 1., 1., 1., 1., 1.],
    )
    .unwrap();
    let masked = pixels.masked(&mask).unwrap();
    let got = masked.native.as_ref().unwrap().cmyk.as_ref().unwrap();
    assert_eq!(&got[..4], &inks[..4]);
    assert!((got[4] - 0.333333).abs() < 1e-6);
    let mut exposure = crate::raster::Settings::new(crate::raster::Kind::Exposure);
    exposure.values[0] = 0.2;
    let edited = pixels.filtered(&exposure, Some(&[255, 0])).unwrap();
    let got = edited.native.as_ref().unwrap().cmyk.as_ref().unwrap();
    assert_ne!(&got[..4], &inks[..4]);
    assert_eq!(&got[5..], &inks[5..]);
    for large in [false, true] {
        let out = crate::formats::psd::encode(&doc, large).unwrap();
        assert_eq!(out.bytes[25], 4);
        let reopened = crate::formats::psd::decode(&out.bytes, "Ink")
            .unwrap()
            .document;
        assert_eq!(reopened.color.icc_profile, profile);
        let got = reopened.layers[0]
            .kind
            .pixels()
            .unwrap()
            .native
            .as_ref()
            .unwrap()
            .cmyk
            .as_ref()
            .unwrap();
        for (&a, &b) in got.iter().zip(&inks) {
            assert!((a - b).abs() < 1. / 65535. + 1e-7);
        }
    }
}

#[test]
fn invalid_and_mismatched_icc_profiles_are_rejected() {
    let rgb = include_bytes!("../formats/psd/fixtures/srgb-cc0.icc");
    assert!(cms::validate_profile(rgb, cms::Model::Cmyk).is_err());
    assert!(cms::transform(&[0.1, 0.2, 0.3], cms::Model::Rgb, b"not ICC", true, 1).is_err());
    let values = [0.1234567, 0.5, 0.9];
    let expected = [31.5843 / 255., 127.5015 / 255., 229.4919 / 255.];
    let converted = cms::transform(&values, cms::Model::Rgb, rgb, true, 1).unwrap();
    for (a, b) in expected.iter().zip(converted.iter()) {
        assert!((a - b).abs() < 0.0002);
    }
}

#[test]
fn cmyk_tiff_keeps_ink_alpha_profile_and_depth() {
    let profile = include_bytes!("../formats/psd/fixtures/fogra39l-cc0.icc").to_vec();
    for depth in [Depth::U8, Depth::U16, Depth::F32] {
        let inks = vec![0.1234567, 0.25, 0.5, 0.7, 0.333333];
        let pixels = Pixels::from_cmyk(1, 1, depth, inks.clone(), profile.clone(), 1).unwrap();
        let bytes = super::tiff::encode(&pixels).unwrap();
        let got = super::tiff::decode(&bytes).unwrap().unwrap();
        let native = got.native.unwrap();
        assert_eq!(native.depth, depth);
        assert_eq!(native.icc_profile.as_slice(), profile);
        let tolerance = match depth {
            Depth::U8 => 1. / 255.,
            Depth::U16 => 1. / 65535.,
            Depth::F32 => 1e-7,
        };
        for (&a, &b) in native.cmyk.unwrap().iter().zip(&inks) {
            assert!((a - b).abs() < tolerance);
        }
    }
}

#[test]
fn profile_aware_export_and_document_conversion_preserve_real_authoring_data() {
    use crate::export::{self, Format, Settings, Source};
    let profile = include_bytes!("../formats/psd/fixtures/srgb-cc0.icc").to_vec();
    let mut pixels = Pixels::from_samples(
        2,
        1,
        Depth::U16,
        vec![1000. / 65535., 0.25, 0.5, 1., 1001. / 65535., 0.5, 0.75, 1.],
    )
    .unwrap();
    pixels.assign_rgb_profile(profile.clone(), 1).unwrap();
    let mut doc = Document::new("Tagged export", 2., 1., 72.);
    doc.transparent = true;
    doc.color = cms::DocumentColor {
        depth: Depth::U16,
        icc_profile: profile.clone(),
        intent: 1,
        ..Default::default()
    };
    doc.layers = vec![Layer::placed_raster(
        "Tagged",
        pixels,
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
        let decoded = crate::formats::decode_image(&bytes).unwrap();
        assert_eq!(
            decoded.native.as_ref().unwrap().icc_profile.as_slice(),
            profile
        );
        assert!((decoded.sample(0)[0] - 1000. / 65535.).abs() < 1e-6);
        if let Ok(dir) = std::env::var("OMADESIGN_DEEP_QA_DIR") {
            std::fs::write(
                std::path::Path::new(&dir).join(format!("profile-16.{}", format.extension())),
                bytes,
            )
            .unwrap();
        }
    }
    let before = doc.layers[0]
        .kind
        .pixels()
        .unwrap()
        .samples()
        .unwrap()
        .as_slice()
        .to_vec();
    let inks = include_bytes!("../formats/psd/fixtures/fogra39l-cc0.icc").to_vec();
    let command = doc
        .color_conversion(cms::DocumentColor {
            model: cms::Model::Cmyk,
            depth: Depth::U16,
            icc_profile: inks.clone(),
            intent: 1,
        })
        .unwrap();
    crate::document::apply(&mut doc, &command);
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
    let decoded = crate::formats::decode_image(&bytes).unwrap();
    assert_eq!(decoded.native.as_ref().unwrap().depth, Depth::U16);
    assert_eq!(
        decoded.native.as_ref().unwrap().icc_profile.as_slice(),
        inks
    );
    assert!(decoded.native.as_ref().unwrap().cmyk.is_some());
    if let Ok(dir) = std::env::var("OMADESIGN_DEEP_QA_DIR") {
        std::fs::write(
            std::path::Path::new(&dir).join("profile-cmyk-16.tif"),
            bytes,
        )
        .unwrap();
    }
    doc = crate::project::decode(&crate::project::encode(&doc).unwrap()).unwrap();
    assert_eq!(doc.color.model, cms::Model::Cmyk);
    crate::document::apply(&mut doc, &crate::document::invert_cmd(command));
    assert_eq!(
        doc.layers[0]
            .kind
            .pixels()
            .unwrap()
            .samples()
            .unwrap()
            .as_slice()
            .to_vec(),
        before
    );
}

#[test]
fn authoritative_sample_validation_rejects_malformed_project_inks() {
    let pixels = Pixels::from_cmyk(
        1,
        1,
        Depth::U16,
        vec![0.1, 0.2, 0.3, 0.4, 1.],
        Vec::new(),
        1,
    )
    .unwrap();
    let mut doc = Document::new("Malformed", 1., 1., 72.);
    doc.layers = vec![Layer::placed_raster(
        "Ink",
        pixels,
        Pt::ZERO,
        Pt::new(1., 1.),
    )];
    let mut value: serde_json::Value =
        serde_json::from_str(&crate::project::encode(&doc).unwrap()).unwrap();
    value["doc"]["layers"][0]["kind"]["Raster"]["pixels"]["native"]["cmyk"] = serde_json::json!([]);
    assert!(crate::project::decode(&value.to_string()).is_err());
    assert_eq!(cms::runtime_probe().unwrap(), 2190);
}

#[test]
fn native_brush_growth_and_group_mask_resize_preserve_sub_byte_values() {
    let pixels = Pixels::from_cmyk(
        1,
        1,
        Depth::U16,
        vec![0.123456, 0.2, 0.3, 0.4, 0.5],
        Vec::new(),
        1,
    )
    .unwrap();
    let grown = pixels.reframed(3, 3, 1, 1).unwrap();
    assert_eq!(
        grown.native.as_ref().unwrap().cmyk.as_ref().unwrap()[20..25],
        [0.123456, 0.2, 0.3, 0.4, 0.5]
    );
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
}

#[test]
fn cmyk_native_retouch_and_all_blends_preserve_ink_authoring() {
    let profile = include_bytes!("../formats/psd/fixtures/fogra39l-cc0.icc").to_vec();
    let inks: Vec<f32> = (0..8 * 8)
        .flat_map(|i| [0.1 + i as f32 / 65535., 0.2, 0.3, 0.4, 0.75])
        .collect();
    let p = Pixels::from_cmyk(8, 8, Depth::U16, inks.clone(), profile.clone(), 1).unwrap();
    let brush = crate::paint::Brush {
        size: 4.,
        flow: 1.,
        opacity: 0.5,
        ..Default::default()
    };
    let edited = super::retouch::stroke(
        &p,
        &p,
        Pt::new(4., 4.),
        Pt::new(4., 4.),
        Pt::new(-2., 0.),
        crate::tools::Tool::Clone,
        &brush,
    )
    .unwrap();
    let n = edited.native.as_ref().unwrap();
    assert_eq!(n.depth, Depth::U16);
    assert_eq!(n.icc_profile.as_slice(),profile);
    assert_eq!(&n.cmyk.as_ref().unwrap()[..5], &inks[..5]);
    assert_ne!(
        edited.samples().unwrap().as_slice().to_vec(),
        p.samples().unwrap().as_slice().to_vec()
    );
    let mut doc = Document::new("Blend inks", 8., 8., 72.);
    doc.transparent = true;
    doc.color = cms::DocumentColor {
        model: cms::Model::Cmyk,
        depth: Depth::U16,
        icc_profile: profile.clone(),
        intent: 1,
    };
    doc.layers = vec![
        Layer::placed_raster("Ink", p, Pt::ZERO, Pt::new(8., 8.)),
        Layer::placed_raster("Edited", edited, Pt::ZERO, Pt::new(8., 8.)),
    ];
    for blend in [
        crate::color::Blend::Normal,
        crate::color::Blend::Multiply,
        crate::color::Blend::Screen,
        crate::color::Blend::Overlay,
        crate::color::Blend::Darken,
        crate::color::Blend::Lighten,
        crate::color::Blend::ColorDodge,
        crate::color::Blend::ColorBurn,
        crate::color::Blend::HardLight,
        crate::color::Blend::SoftLight,
        crate::color::Blend::Difference,
        crate::color::Blend::Exclusion,
        crate::color::Blend::Hue,
        crate::color::Blend::Saturation,
        crate::color::Blend::Color,
        crate::color::Blend::Luminosity,
    ] {
        doc.layers[1].blend = blend;
        let out = super::composite(&doc, 1).unwrap();
        let native = out.native.unwrap();
        assert_eq!(native.icc_profile.as_slice(),profile);
        assert_eq!(native.cmyk.unwrap().len(), 8 * 8 * 5);
        assert!(native.samples.iter().all(|v| v.is_finite()));
    }
    assert_eq!(
        doc.layers[0]
            .kind
            .pixels()
            .unwrap()
            .native
            .as_ref()
            .unwrap()
            .cmyk
            .as_ref()
            .unwrap().as_slice(),
        inks.as_slice()
    );
}

#[test]
fn cmyk_float_ffi_matches_independent_transicc_reference() {
    let profile = include_bytes!("../formats/psd/fixtures/fogra39l-cc0.icc");
    let samples = [
        0., 0., 0., 0., 0.1, 0.2, 0.3, 0.4, 1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 0., 1.,
    ];
    let expected = [
        255., 255., 255., 163.3108, 148.8488, 132.5648, -590.2940, 160.5262, 227.7222, 230.1068,
        12.1681, 128.1444, 43.0867, 42.7438, 41.8995,
    ];
    let transformed = cms::transform(&samples, cms::Model::Cmyk, profile, true, 1).unwrap();
    for (a, b) in transformed.iter().zip(expected) {
        assert!((a - b / 255.).abs() < 0.0002, "{a} != {b}/255");
    }
}

#[test]
fn interleaved_profile_conversion_preserves_alpha_and_matches_independent_transicc() {
    let profile = include_bytes!("../formats/psd/fixtures/srgb-cc0.icc");
    let source = [0.1234567, 0.5, 0.9, 0.333333];
    let got = cms::transform_alpha(&source,cms::Model::Rgb,profile,true,1).unwrap();
    let expected = [31.5843 / 255.,127.5015 / 255.,229.4919 / 255.];
    for (&a,b) in got[..3].iter().zip(expected) {assert!((a-b).abs() < 0.0002);}
    assert_eq!(got[3],source[3]);
    let profile = include_bytes!("../formats/psd/fixtures/fogra39l-cc0.icc");
    let rgba = [0.3,0.4,0.5,0.00001];
    let ink = cms::transform_alpha(&rgba,cms::Model::Cmyk,profile,false,1).unwrap();
    let components = cms::transform(&rgba[..3],cms::Model::Cmyk,profile,false,1).unwrap();
    assert_eq!(&ink[..4],components.as_slice());assert_eq!(ink[4],rgba[3]);
    let display = cms::transform_alpha(&ink,cms::Model::Cmyk,profile,true,1).unwrap();
    let components = cms::transform(&ink[..4],cms::Model::Cmyk,profile,true,1).unwrap();
    assert_eq!(&display[..3],components.as_slice());assert_eq!(display[3],rgba[3]);
}

#[test]
fn cmyk_to_rgb_uses_requested_source_rendering_intent() {
    let profile = include_bytes!("../formats/psd/fixtures/fogra39l-cc0.icc").to_vec();
    let inks = vec![0.,0.,0.,0.,0.5, 0.1,0.2,0.3,0.4,0.333333, 1.,0.,0.,0.,0.00001];
    let source = Pixels::from_cmyk(3,1,Depth::F32,inks.clone(),profile.clone(),1).unwrap();
    let expected = Pixels::from_cmyk(3,1,Depth::F32,inks,profile,0).unwrap();
    assert!(source.samples().unwrap().iter().zip(expected.samples().unwrap().iter()).any(|(a,b)| (a-b).abs() > 0.001));
    let converted = source.in_document_color(&cms::DocumentColor {model:cms::Model::Rgb,depth:Depth::F32,icc_profile:Vec::new(),intent:0}).unwrap();
    assert_eq!(converted.samples().unwrap().as_slice(),expected.samples().unwrap().as_slice());
    assert_eq!(converted.native.as_ref().unwrap().intent,0);
    for (a,b) in converted.samples().unwrap().chunks_exact(4).zip(source.samples().unwrap().chunks_exact(4)) {assert_eq!(a[3],b[3]);}
}

#[test]
fn admitted_tiff_decode_accepts_uncompressed_strips_larger_than_64k() {
    use ::tiff::encoder::{TiffEncoder,colortype};
    let mut values=[1000u16,2000,3000].repeat(256*128);
    values[3]=1001;
    let mut bytes=std::io::Cursor::new(Vec::new());
    {
        let mut encoder=TiffEncoder::new(&mut bytes).unwrap();
        let mut image=encoder.new_image::<colortype::RGB16>(256,128).unwrap();
        image.rows_per_strip(128).unwrap();
        image.write_data(&values).unwrap();
    }
    assert!(bytes.get_ref().len()>64*1024);
    let pixels=super::tiff::decode(bytes.get_ref()).unwrap().unwrap();
    assert_eq!((pixels.w,pixels.h),(256,128));
    assert_eq!(pixels.native.as_ref().unwrap().depth,Depth::U16);
    let samples=pixels.samples().unwrap();
    assert_eq!(samples[0],1000. / 65535.);
    assert_eq!(samples[4],1001. / 65535.);
    assert_eq!(pixels.data[0],pixels.data[4]);
}

#[test]
fn tiff_auxiliary_samples_are_refused_before_native_pixel_decode() {
    use ::tiff::{encoder::{TiffEncoder,colortype},tags::ExtraSamples};
    for alpha in [false,true] {
        let mut bytes = std::io::Cursor::new(Vec::new());
        {
            let mut encoder = TiffEncoder::new(&mut bytes).unwrap();
            let mut image = encoder.new_image::<colortype::RGB16>(2,1).unwrap();
            if alpha {
                image.extra_samples(&[ExtraSamples::UnassociatedAlpha,ExtraSamples::Unspecified]).unwrap();
                image.write_data(&[1000,2000,3000,32768,17,4000,5000,6000,16384,29]).unwrap();
            } else {
                image.extra_samples(&[ExtraSamples::Unspecified]).unwrap();
                image.write_data(&[1000,2000,3000,17,4000,5000,6000,29]).unwrap();
            }
        }
        let error = super::tiff::decode(bytes.get_ref()).err().expect("Auxiliary samples must not be treated as ordinary RGB(A)");
        assert!(error.contains("auxiliary samples"),"{error}");
    }
}

#[test]
fn unsupported_tagged_tiff_models_do_not_fall_back_to_unmanaged_pixels() {
    use ::tiff::{encoder::{TiffEncoder,colortype},tags::Tag};
    for tagged in [false,true] {
        let mut bytes = std::io::Cursor::new(Vec::new());
        {
            let mut encoder = TiffEncoder::new(&mut bytes).unwrap();
            let mut image = encoder.new_image::<colortype::Gray16>(1,1).unwrap();
            if tagged {image.encoder().write_tag(Tag::IccProfile,include_bytes!("../formats/psd/fixtures/srgb-cc0.icc").as_slice()).unwrap();}
            image.write_data(&[1001]).unwrap();
        }
        let result = super::tiff::decode(bytes.get_ref());
        if tagged {assert!(result.err().expect("Unsupported tagged model must not drop ICC").contains("profile-aware native decoder"));}
        else {assert!(result.unwrap().is_none());}
    }
}

#[test]
fn non_solid_type_span_paint_survives_project_and_refuses_native_reduction() {
    use crate::{color::Rgba,document::{Fill,Shape,Style},geom::{Geom,TypeRun,CharSpan}};
    let mut doc = Document::new("Native span paint",8.,8.,72.);
    let mut layer = Layer::vector("Type");
    layer.kind.shapes_mut().unwrap().push(Shape::new(Geom::Text(TypeRun {content:"A".into(),spans:vec![CharSpan {start:0,end:1,fill:Some(Fill::Radial {c0:Rgba::BLACK,c1:Rgba::WHITE}),..Default::default()}],..Default::default()}),Style::default()));
    doc.layers=vec![layer];
    let reopened = crate::project::decode(&crate::project::encode(&doc).unwrap()).unwrap();
    assert!(composite(&reopened,1).unwrap_err().contains("float paint renderer"));
}
