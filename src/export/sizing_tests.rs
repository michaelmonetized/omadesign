use super::*;
use crate::{
    deep_color::Depth,
    document::{Layer, Pixels},
    geom::Pt,
    ml::NoProgress,
};
use std::io::Cursor;

fn source_fixture(depth: Depth) -> Source {
    let mut doc = Document::new("Sized artwork", 8., 4., 96.);
    doc.transparent = true;
    doc.color.depth = depth;
    let value = if depth == Depth::F32 {
        [1.25, -0.125, 0.250001, 0.5]
    } else {
        [0.25, 0.5, 0.75, 1.]
    };
    let pixels = if depth == Depth::U8 {
        Pixels::from_rgba(8, 4, vec![64, 128, 191, 255].repeat(32)).unwrap()
    } else {
        Pixels::from_samples(8, 4, depth, value.repeat(32)).unwrap()
    };
    doc.layers = vec![Layer::placed_raster(
        "Samples",
        pixels,
        Pt::ZERO,
        Pt::new(8., 4.),
    )];
    Source::Document(doc)
}
fn encode(source: &Source, format: Format, scale: f64, dpi: Option<f64>) -> EncodedBytes {
    encode_scaled_admitted(
        source,
        Settings {
            format,
            ..Default::default()
        },
        RasterOptions { scale, dpi },
        &NoProgress,
    )
    .unwrap()
    .0
}
fn retain(name: &str, bytes: &[u8]) {
    if let Some(root) = std::env::var_os("OMADESIGN_EXPORT_SIZING_QA") {
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(Path::new(&root).join(name), bytes).unwrap();
    }
}

#[test]
fn craft_export_sizing_fractional_native_float_retains_hdr_negative_alpha_and_legacy_integer_samples()
 {
    let source = source_fixture(Depth::F32);
    let Source::Document(doc) = &source else {
        unreachable!()
    };
    let before = crate::project::encode(doc).unwrap();
    for (scale, size) in [(0.5, (4, 2)), (1.5, (12, 6)), (2.0, (16, 8))] {
        let bytes = encode(&source, Format::Tiff, scale, None);
        let output = crate::deep_color::tiff::decode(bytes.as_slice())
            .unwrap()
            .unwrap();
        assert_eq!((output.w, output.h), size);
        let native = output.native.as_ref().unwrap();
        let i = ((size.1 / 2 * size.0 + size.0 / 2) * 4) as usize;
        assert_eq!(
            native.samples[i..i + 4]
                .iter()
                .map(|v| v.to_bits())
                .collect::<Vec<_>>(),
            [1.25f32, -0.125, 0.250001, 0.5].map(f32::to_bits)
        );
        if scale == 0.5 {
            retain("fractional-f32.tif", bytes.as_slice());
        }
        if scale == 2.0 {
            let legacy = encode_admitted(
                &source,
                Settings {
                    format: Format::Tiff,
                    scale: 2,
                    ai: None,
                },
                &NoProgress,
            )
            .unwrap()
            .0;
            assert_eq!(bytes.as_slice(), legacy.as_slice());
        }
    }
    assert_eq!(crate::project::encode(doc).unwrap(), before);
}

#[test]
fn craft_export_sizing_real_png_jpeg_tiff_density_and_native16_tags_samples() {
    let source = source_fixture(Depth::U8);
    for (format, name) in [
        (Format::Png, "density-u8.png"),
        (Format::Jpeg, "density-u8.jpg"),
        (Format::Tiff, "density-u8.tif"),
    ] {
        let bytes = encode(&source, format, 1.5, Some(144.));
        let decoded = image::load_from_memory(bytes.as_slice()).unwrap();
        assert_eq!((decoded.width(), decoded.height()), (12, 6));
        if format != Format::Jpeg {
            assert_eq!(decoded.to_rgba8().get_pixel(6, 3).0, [64, 128, 191, 255]);
        }
        retain(name, bytes.as_slice());
        match format {
            Format::Png => {
                let reader = png::Decoder::new(Cursor::new(bytes.as_slice()))
                    .read_info()
                    .unwrap();
                let density = reader.info().pixel_dims.unwrap();
                assert_eq!(
                    (density.xppu, density.yppu, density.unit),
                    (5669, 5669, png::Unit::Meter)
                );
            }
            Format::Jpeg => {
                let jfif = bytes.windows(5).position(|w| w == b"JFIF\0").unwrap();
                assert_eq!(&bytes[jfif + 7..jfif + 12], &[1, 0, 144, 0, 144]);
            }
            _ => {
                let mut reader =
                    tiff::decoder::Decoder::new(Cursor::new(bytes.as_slice())).unwrap();
                assert_eq!(
                    reader.get_tag_u32(tiff::tags::Tag::ResolutionUnit).unwrap(),
                    2
                );
                for tag in [tiff::tags::Tag::XResolution,tiff::tags::Tag::YResolution] {
                    let tiff::decoder::ifd::Value::Rational(n,d)=reader.get_tag(tag).unwrap() else {panic!("Resolution must be a TIFF rational")};
                    assert_eq!((n,d),(1_440_000,10_000));
                    assert_eq!(n as f64/d as f64,144.);
                }
                assert_eq!(
                    reader
                        .get_tag_u16_vec(tiff::tags::Tag::ExtraSamples)
                        .unwrap(),
                    [2]
                );
            }
        }
    }
    let source = source_fixture(Depth::U16);
    let bytes = encode(&source, Format::Png, 0.5, Some(300.));
    let output = image::load_from_memory(bytes.as_slice())
        .unwrap()
        .to_rgba16();
    assert_eq!(output.dimensions(), (4, 2));
    assert_eq!(output.get_pixel(2, 1).0, [16384, 32768, 49151, 65535]);
    retain("density-u16.png", bytes.as_slice());
    let bytes = encode(
        &source_fixture(Depth::F32),
        Format::Tiff,
        1.5,
        Some(144.),
    );
    let decoded = crate::deep_color::tiff::decode(bytes.as_slice())
        .unwrap()
        .unwrap();
    assert_eq!(decoded.native.as_ref().unwrap().samples[4 * 42], 1.25);
    retain("density-f32.tif", bytes.as_slice());
}

#[test]
fn craft_export_sizing_invalid_overflow_cancel_and_atomic_owner_release() {
    let source = source_fixture(Depth::F32);
    let baseline = crate::resources::global().usage().0;
    let pending = crate::resources::global().pending_bytes();
    for scale in [0., -1., f64::NAN, f64::INFINITY, f64::MAX] {
        assert!(
            encode_scaled_admitted(
                &source,
                Settings {
                    format: Format::Tiff,
                    ..Default::default()
                },
                RasterOptions { scale, dpi: None },
                &NoProgress
            )
            .is_err()
        );
    }
    for (format, dpi) in [
        (Format::Png, 0.),
        (Format::Jpeg, 100_000.),
        (Format::Tiff, f64::MAX),
    ] {
        assert!(
            dimensions_scaled(
                &source,
                Settings {
                    format,
                    ..Default::default()
                },
                RasterOptions {
                    scale: 1.,
                    dpi: Some(dpi)
                }
            )
            .is_err()
        );
    }
    struct Cancel;
    impl Progress for Cancel {
        fn report(&self, _: &str, _: usize, _: usize) {}
        fn cancelled(&self) -> bool {
            true
        }
    }
    assert!(
        encode_scaled_admitted(
            &source,
            Settings {
                format: Format::Tiff,
                ..Default::default()
            },
            RasterOptions {
                scale: 0.5,
                dpi: Some(300.)
            },
            &Cancel
        )
        .is_err()
    );
    let root = std::env::current_dir()
        .unwrap()
        .join(".artifacts")
        .join(format!("sized-cancel-{}", crate::document::next_id()));
    std::fs::create_dir_all(&root).unwrap();
    let path = root.join("accepted.tif");
    std::fs::write(&path, b"accepted").unwrap();
    assert!(
        save_scaled(
            &source,
            Settings {
                format: Format::Tiff,
                ..Default::default()
            },
            RasterOptions {
                scale: 0.5,
                dpi: Some(300.)
            },
            &path,
            &Cancel
        )
        .is_err()
    );
    assert_eq!(std::fs::read(path).unwrap(), b"accepted");
    assert_eq!(std::fs::read_dir(&root).unwrap().count(), 1);
    std::fs::remove_dir_all(root).unwrap();
    assert_eq!(
        (
            crate::resources::global().usage().0,
            crate::resources::global().pending_bytes()
        ),
        (baseline, pending)
    );
}

#[test]
fn craft_export_sizing_fractional_cmyk_retains_inks_icc_intent_and_tiff_density() {
    let mut doc = Document::new("Inks", 8., 4., 96.);
    doc.transparent = true;
    doc.color.model = crate::deep_color::cms::Model::Cmyk;
    doc.color.depth = Depth::F32;
    let profile = include_bytes!("../formats/psd/fixtures/fogra39l-cc0.icc").to_vec();
    doc.color.icc_profile = profile.clone();
    doc.color.intent = 1;
    let inks = [0.125, 0.25, 0.375, 0.5, 1.];
    doc.layers = vec![Layer::placed_raster(
        "Inks",
        Pixels::from_cmyk(8, 4, Depth::F32, inks.repeat(32), profile.clone(), 1).unwrap(),
        Pt::ZERO,
        Pt::new(8., 4.),
    )];
    let bytes = encode(&Source::Document(doc), Format::Tiff, 0.5, Some(300.));
    let output = crate::deep_color::tiff::decode(bytes.as_slice())
        .unwrap()
        .unwrap();
    assert_eq!((output.w, output.h), (4, 2));
    let native = output.native.unwrap();
    assert_eq!(&native.cmyk.unwrap()[30..35], &inks);
    assert_eq!(native.icc_profile.as_slice(), profile);
    assert_eq!(native.intent, 1);
    retain("density-cmyk.tif", bytes.as_slice());
}
