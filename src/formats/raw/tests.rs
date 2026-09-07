use super::*;

const DNG: &[u8] = include_bytes!("synthetic.dng");

fn set_tag(bytes: &mut [u8], tag: u16, value: &[u8]) {
    let count = u16::from_le_bytes(bytes[8..10].try_into().unwrap()) as usize;
    for index in 0..count {
        let entry = 10 + index * 12;
        if u16::from_le_bytes(bytes[entry..entry + 2].try_into().unwrap()) == tag {
            let start = if value.len() > 4 {
                u32::from_le_bytes(bytes[entry + 8..entry + 12].try_into().unwrap()) as usize
            } else {
                entry + 8
            };
            bytes[start..start + value.len()].copy_from_slice(value);
            return;
        }
    }
    panic!("missing generated TIFF tag {tag}");
}

#[test]
fn demosaics_real_sensor_tags_without_an_embedded_preview() {
    let original = DNG.to_vec();
    let image = decode(DNG).unwrap();
    assert_eq!((image.width, image.height), (64, 48));
    assert_eq!(image.pixels.len(), 64 * 48 * 3);
    assert_eq!(image.metadata.make, "Omadesign");
    assert_eq!(image.metadata.model, "Synthetic Bayer");
    assert!((image.metadata.shutter - 1.0 / 125.0).abs() < 0.00001);
    assert!((image.metadata.aperture - 2.8).abs() < 0.001);
    assert_eq!(image.metadata.focal_length, 50.0);
    assert_eq!(image.metadata.baseline_exposure, 1.0);
    assert!(image.warnings.is_empty());
    let levels: std::collections::HashSet<_> = image.pixels.iter().step_by(3).collect();
    assert!(levels.len() > 256, "RAW was reduced to an 8-bit preview");
    assert_eq!(DNG, original);
}

#[test]
fn orients_pixels_once_and_reports_unapplied_baseline_exposure() {
    let normal = decode(DNG).unwrap();
    let mut rotated = DNG.to_vec();
    set_tag(&mut rotated, 274, &6u16.to_le_bytes());
    let rotated = decode(&rotated).unwrap();
    assert_eq!((rotated.width, rotated.height), (48, 64));
    assert_eq!(rotated.metadata.orientation, 6);
    for y in 0..48usize {
        for x in 0..64usize {
            let source = (y * 64 + x) * 3;
            let target = (x * 48 + 47 - y) * 3;
            assert_eq!(
                normal.pixels[source..source + 3],
                rotated.pixels[target..target + 3]
            );
        }
    }
    let mut baseline = DNG.to_vec();
    set_tag(&mut baseline, 50730, &[2, 0, 0, 0, 1, 0, 0, 0]);
    let baseline = decode(&baseline).unwrap();
    assert_eq!(baseline.metadata.baseline_exposure, 2.0);
    assert_eq!(baseline.pixels, normal.pixels);
}

#[test]
fn rejects_truncated_and_oversized_sensor_data() {
    assert!(decode(&[]).is_err());
    assert!(decode(b"not a camera raw file").is_err());
    assert!(decode(&DNG[..DNG.len() - 800]).is_err());
    let mut oversized = DNG.to_vec();
    set_tag(&mut oversized, 256, &65535u32.to_le_bytes());
    set_tag(&mut oversized, 257, &65535u32.to_le_bytes());
    assert!(decode(&oversized).is_err());
    let mut stretched = DNG.to_vec();
    set_tag(&mut stretched, 256, &10000u32.to_le_bytes());
    set_tag(&mut stretched, 257, &6000u32.to_le_bytes());
    set_tag(
        &mut stretched,
        50718,
        &[10, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0, 0],
    );
    let error = decode(&stretched).unwrap_err();
    assert!(error.contains("pixel-aspect"), "{error}");
}

#[test]
#[ignore = "private camera fixture supplied with OMA_RAW_FIXTURE"]
fn private_camera_fixture() {
    let path = std::env::var("OMA_RAW_FIXTURE").expect("OMA_RAW_FIXTURE");
    let image = decode(&std::fs::read(&path).unwrap()).unwrap();
    eprintln!(
        "{}: {} × {}, {}, {:?}",
        path, image.width, image.height, image.metadata.model, image.warnings
    );
    if let Ok(reference) = std::env::var("OMA_RAW_REFERENCE") {
        let reference = std::fs::read(reference).unwrap();
        assert_eq!(reference.len(), image.pixels.len() * 2);
        let differences: Vec<_> = image
            .pixels
            .iter()
            .zip(reference.as_chunks::<2>().0)
            .map(|(&actual, reference)| actual.abs_diff(u16::from_le_bytes(*reference)))
            .collect();
        let mean = differences.iter().map(|&n| u64::from(n)).sum::<u64>() as f64
            / differences.len() as f64;
        eprintln!(
            "16-bit reference error: mean {mean}, max {:?}",
            differences.iter().max()
        );
        assert!(
            mean < 0.05,
            "native decoder diverged from reference: {mean}"
        );
    }
}
