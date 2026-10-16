use super::*;
use crate::export::{self, Format, Settings, Source};
use std::{path::PathBuf, sync::Arc};

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let parent = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(".qa-runs");
        std::fs::create_dir_all(&parent).unwrap();
        let path = parent.join(format!(
            "raw-native-placement-{}-{}",
            std::process::id(),
            crate::document::next_id()
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}

fn calibration() -> PhotoImage {
    let pixels = (0..2048 * 2 * 3)
        .map(|i| ((i * 13 + 7) % 65536) as u16)
        .collect::<Vec<_>>();
    PhotoImage::from_raw(
        "Linear calibration".into(),
        crate::formats::raw::RawImage {
            width: 2048,
            height: 2,
            pixels: pixels.into(),
            memory: None,
            metadata: Default::default(),
            warnings: vec![],
        },
    )
    .unwrap()
}

fn encode(doc: &Document, format: Format) -> Vec<u8> {
    export::encode(
        &Source::Document(doc.clone()),
        Settings {
            format,
            scale: 1,
            ai: None,
        },
        &crate::ml::NoProgress,
    )
    .unwrap()
    .0
}

#[test]
fn raw_native_placement_uses_linear_source_oetf_and_float_file_reopen() {
    let fixture = Fixture::new();
    let mut photo = calibration();
    let original = photo.raw.as_ref().unwrap().pixels.clone();
    photo.full = Arc::new(crate::photo::RgbaImage {
        w: 1,
        h: 1,
        data: vec![255, 0, 255, 255],
    });
    let doc = crate::formats::cli::photo_document(&photo).unwrap();
    assert_eq!(
        doc.color,
        DocumentColor {
            model: Model::Rgb,
            depth: Depth::F32,
            icc_profile: vec![],
            intent: 1
        }
    );
    let pixels = doc.layers[0].kind.pixels().unwrap();
    assert_eq!((pixels.w, pixels.h), (2048, 2));
    pixels.validate_native().unwrap();
    let samples = pixels.samples().unwrap();
    let mut fractional = 0;
    for (pixel, source) in samples.chunks_exact(4).zip(original.chunks_exact(3)) {
        assert_eq!(pixel[3], 1.0);
        for (value, source) in pixel[..3].iter().zip(source) {
            let linear = f64::from(*source) / 65535.;
            let expected = if linear <= 0.0031308 {
                linear * 12.92
            } else {
                1.055 * linear.powf(1. / 2.4) - 0.055
            };
            assert!((f64::from(*value) - expected).abs() < 0.000001);
            fractional += usize::from((*value * 255. - (*value * 255.).round()).abs() > 0.0001);
        }
    }
    assert!(
        fractional > 256,
        "native samples were quantized through the display cache"
    );
    let path = fixture.0.join("calibration.oma");
    crate::project::save_to(&doc, &path).unwrap();
    drop(doc);
    let reopened = crate::project::load_from(&path).unwrap();
    assert_eq!(reopened.color.intent, 1);
    assert_eq!(
        reopened.layers[0]
            .kind
            .pixels()
            .unwrap()
            .samples()
            .unwrap()
            .as_slice(),
        samples.as_slice()
    );
    let bytes = encode(&reopened, Format::Tiff);
    std::fs::write(fixture.0.join("calibration.tif"), &bytes).unwrap();
    let mut reader = tiff::decoder::Decoder::new(std::io::Cursor::new(bytes)).unwrap();
    assert_eq!(reader.dimensions().unwrap(), (2048, 2));
    let tiff::decoder::DecodingResult::F32(decoded) = reader.read_image().unwrap() else {
        panic!("float TIFF was reduced to integer samples")
    };
    assert_eq!(decoded.as_slice(), samples.as_slice());
    assert!(Arc::ptr_eq(&original, &photo.raw.as_ref().unwrap().pixels));
    assert_eq!(
        photo.raw.as_ref().unwrap().pixels.as_slice(),
        original.as_slice()
    );
}

#[test]
fn raw_native_placement_exports_png_only_after_explicit_depth_conversion() {
    let photo = calibration();
    let mut doc = document(&photo, None, &Cancellation::default()).unwrap();
    assert!(
        export::encode(
            &Source::Document(doc.clone()),
            Settings {
                format: Format::Png,
                scale: 1,
                ai: None
            },
            &crate::ml::NoProgress
        )
        .unwrap_err()
        .contains("Floating-point")
    );
    let mut target = doc.color.clone();
    target.depth = Depth::U16;
    let command = doc.color_conversion(target).unwrap();
    crate::document::apply(&mut doc, &command);
    let pixels = image::load_from_memory(&encode(&doc, Format::Png)).unwrap();
    assert_eq!(pixels.color(), image::ColorType::Rgba16);
    let source = doc.layers[0].kind.pixels().unwrap().samples().unwrap();
    for (decoded, sample) in pixels.to_rgba16().as_raw().iter().zip(source.iter()) {
        assert_eq!(*decoded, (*sample * 65535.).round() as u16);
    }
}

#[test]
fn raw_native_placement_preserves_actual_target_profile_and_intent() {
    let fixture = Fixture::new();
    let photo = calibration();
    let original = photo.raw.as_ref().unwrap().pixels.clone();
    for intent in [0, 3] {
        let target = DocumentColor {
            model: Model::Cmyk,
            depth: Depth::U16,
            icc_profile: include_bytes!("../psd/fixtures/fogra39l-cc0.icc").to_vec(),
            intent,
        };
        let doc = document(&photo, Some(&target), &Cancellation::default()).unwrap();
        assert_eq!(doc.color.model, target.model);
        assert_eq!(doc.color.depth, Depth::F32);
        assert_eq!(doc.color.intent, intent);
        assert_eq!(doc.color.icc_profile, target.icc_profile);
        let pixels = doc.layers[0].kind.pixels().unwrap();
        pixels.validate_native().unwrap();
        let native = pixels.native.as_ref().unwrap();
        assert_eq!(native.intent, intent);
        assert_eq!(native.depth, Depth::F32);
        assert_eq!(native.icc_profile.as_slice(), target.icc_profile.as_slice());
        let inks = native.cmyk.as_ref().unwrap();
        assert_eq!(inks.len(), 2048 * 2 * 5);
        assert!(inks.chunks_exact(5).all(|pixel| pixel[4] == 1.));
        let path = fixture.0.join(format!("cmyk-intent-{intent}.oma"));
        crate::project::save_to(&doc, &path).unwrap();
        let reopened = crate::project::load_from(&path).unwrap();
        assert_eq!(reopened.color, doc.color);
        let restored = reopened.layers[0]
            .kind
            .pixels()
            .unwrap()
            .native
            .as_ref()
            .unwrap();
        assert_eq!(restored.cmyk.as_ref().unwrap().as_slice(), inks.as_slice());
        assert_eq!(
            restored.icc_profile.as_slice(),
            target.icc_profile.as_slice()
        );
        assert_eq!(restored.intent, intent);
        let bytes = encode(&reopened, Format::Tiff);
        let mut reader = tiff::decoder::Decoder::new(std::io::Cursor::new(bytes)).unwrap();
        assert_eq!(reader.dimensions().unwrap(), (2048, 2));
        assert_eq!(
            reader
                .get_tag_u8_vec(tiff::tags::Tag::Unknown(34675))
                .unwrap(),
            target.icc_profile
        );
        let tiff::decoder::DecodingResult::F32(decoded) = reader.read_image().unwrap() else {
            panic!("CMYK TIFF lost float precision")
        };
        assert_eq!(decoded.len(), inks.len());
        let mut maximum = 0f64;
        let mut total = 0f64;
        for (actual, expected) in decoded.iter().zip(inks.iter()) {
            assert!(actual.is_finite());
            let difference = (f64::from(*actual) - f64::from(*expected)).abs();
            maximum = maximum.max(difference);
            total += difference;
            assert!(
                difference <= f64::from(f32::EPSILON) / 2.,
                "CMYK composite changed float samples by {difference}"
            );
        }
        assert!(decoded.chunks_exact(5).all(|pixel| pixel[4] == 1.));
        eprintln!(
            "CMYK intent{intent} float composite: {} samples, max{maximum}, mean{}; tolerance{}",
            decoded.len(),
            total / decoded.len() as f64,
            f64::from(f32::EPSILON) / 2.
        );
        let direct = crate::deep_color::tiff::encode(pixels).unwrap();
        let mut reader = tiff::decoder::Decoder::new(std::io::Cursor::new(direct)).unwrap();
        assert_eq!(reader.dimensions().unwrap(), (2048, 2));
        let tiff::decoder::DecodingResult::F32(exact) = reader.read_image().unwrap() else {
            panic!("native CMYK TIFF writer reduced precision")
        };
        assert_eq!(exact.len(), inks.len());
        assert!(
            exact
                .iter()
                .zip(inks.iter())
                .all(|(a, b)| a.to_bits() == b.to_bits()),
            "direct TIFF writer changed authored ink bits"
        );
    }
    assert!(Arc::ptr_eq(&original, &photo.raw.as_ref().unwrap().pixels));
}

#[test]
fn raw_native_placement_refuses_cancelled_or_invalid_profile_without_source_changes() {
    let photo = calibration();
    let original = photo.raw.as_ref().unwrap().pixels.clone();
    let cancel = Cancellation::default();
    cancel.cancel();
    assert!(
        document(&photo, None, &cancel)
            .unwrap_err()
            .contains("cancelled")
    );
    let invalid = DocumentColor {
        model: Model::Rgb,
        depth: Depth::F32,
        icc_profile: vec![1, 2, 3],
        intent: 0,
    };
    assert!(document(&photo, Some(&invalid), &Cancellation::default()).is_err());
    assert!(Arc::ptr_eq(&original, &photo.raw.as_ref().unwrap().pixels));
}
