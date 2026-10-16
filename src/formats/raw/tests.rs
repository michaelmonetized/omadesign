use super::*;

const DNG: &[u8] = include_bytes!("synthetic.dng");

#[test]
fn native_result_retains_its_memory_and_cancellable_decoder_ownership() {
    use std::time::Duration;
    let baseline = resources::global().usage().0;
    let native = native_decode(DNG, &Cancellation::default(), false).unwrap();
    assert_eq!(
        resources::global().usage().0,
        baseline + native.lease.bytes()
    );
    let token = Cancellation::default();
    let worker_token = token.clone();
    let (started_tx, started_rx) = std::sync::mpsc::channel();
    let (done_tx, done_rx) = std::sync::mpsc::channel();
    let worker = std::thread::spawn(move || {
        started_tx.send(()).unwrap();
        done_tx
            .send(decode_cancellable(DNG, &worker_token))
            .unwrap();
    });
    started_rx.recv().unwrap();
    token.cancel();
    assert!(
        done_rx
            .recv_timeout(Duration::from_secs(1))
            .unwrap()
            .unwrap_err()
            .contains("cancelled")
    );
    worker.join().unwrap();
    assert_eq!(
        resources::global().usage().0,
        baseline + native.lease.bytes()
    );
    assert!(native.image.data_size > 0 && !native.image.data.is_null());
    drop(native);
    assert_eq!(resources::global().usage().0, baseline);
    let next = decode(DNG).unwrap();
    assert_eq!((next.width, next.height), (64, 48));
    drop(next);
    assert_eq!(resources::global().usage().0, baseline);
}

#[test]
fn sensor_reference_is_unscaled_unoriented_mosaic_including_borders() {
    let sensor = decode_sensor(DNG, &Cancellation::default()).unwrap();
    assert_eq!((sensor.width, sensor.height), (64, 48));
    let raw = &DNG[DNG.len() - 64 * 48 * 2..];
    for (sample, expected) in sensor.samples.iter().zip(raw.as_chunks::<2>().0) {
        assert_eq!(*sample, u16::from_le_bytes(*expected));
    }
}

/// Create a bounded Bayer frame that takes the real native interpolation path.
/// Args: sensor width and height. Returns an original MIT calibration DNG without an embedded preview.
fn calibration_dng(w: u32, h: u32) -> Vec<u8> {
    let mut bytes = DNG.to_vec();
    let offset = bytes.len() as u32;
    for (tag, value) in [
        (256, w),
        (257, h),
        (278, h),
        (273, offset),
        (279, w * h * 2),
    ] {
        set_tag(&mut bytes, tag, &value.to_le_bytes());
    }
    for y in 0..h {
        for x in 0..w {
            let value = 3000 + (x * 13 + y * 7) % 25000;
            bytes.extend_from_slice(&(value as u16).to_le_bytes());
        }
    }
    bytes
}

#[test]
fn active_native_interpolation_cancels_promptly_then_next_decode_succeeds() {
    use std::time::{Duration, Instant};
    let bytes = calibration_dng(2048, 1536);
    let token = Cancellation::default();
    let worker_token = token.clone();
    let (tx, rx) = std::sync::mpsc::channel();
    let worker = std::thread::spawn(move || {
        let _ = tx.send(decode_cancellable(&bytes, &worker_token));
    });
    let started = Instant::now();
    while token.native_progress() < (1 << 11) && started.elapsed() < Duration::from_secs(20) {
        std::thread::sleep(Duration::from_millis(1));
    }
    let phase = token.native_progress();
    let active = token.native_active();
    let cancelled = Instant::now();
    token.cancel();
    let result = rx.recv().unwrap();
    let latency = cancelled.elapsed();
    worker.join().unwrap();
    assert!(
        active,
        "cancellation must be requested during the native call"
    );
    assert_eq!(
        phase,
        1 << 11,
        "must cancel inside actual native interpolation, stage={phase}"
    );
    assert!(result.unwrap_err().contains("cancelled"));
    assert!(
        latency < Duration::from_millis(500),
        "native cancel latency {latency:?}"
    );
    let next = decode(DNG).unwrap();
    assert_eq!((next.width, next.height), (64, 48));
    eprintln!("active native AHD cancellation: {latency:?}; immediate next decode 64x48 succeeded");
}

#[test]
#[ignore = "pinned real camera source; scheduled with the strict corpus and shared heavy lock"]
fn real_camera_active_cancel_keeps_edits_and_immediately_opens_next() {
    use sha2::{Digest, Sha256};
    use std::io::Read;
    use std::time::{Duration, Instant};
    fn hash(path: &std::path::Path) -> String {
        let mut file = std::fs::File::open(path).unwrap();
        let mut digest = Sha256::new();
        let mut bytes = [0u8; 65536];
        loop {
            let n = file.read(&mut bytes).unwrap();
            if n == 0 {
                break;
            }
            digest.update(&bytes[..n]);
        }
        format!("{:x}", digest.finalize())
    }
    let source =
        std::path::PathBuf::from(std::env::var_os("OMA_RAW_FIXTURE").expect("OMA_RAW_FIXTURE"));
    let expected = std::env::var("OMA_RAW_SOURCE_SHA256").expect("OMA_RAW_SOURCE_SHA256");
    assert_eq!(expected.len(), 64);
    assert_eq!(hash(&source), expected);
    struct NextSource(super::qualification::ScheduledScratch, std::path::PathBuf);
    impl Drop for NextSource {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.1);
            let _ = std::fs::remove_dir(self.0.path());
        }
    }
    let directory = super::qualification::scheduled_scratch("raw-cancel-next").unwrap();
    let next_path = directory.path().join(source.file_name().unwrap());
    let next_source = NextSource(directory, next_path);
    let fixture_storage = super::qualification::copy_scheduled_fixture(
        &source,
        &next_source.1,
        next_source.0.path(),
        &expected,
    )
    .unwrap();
    if fixture_storage.is_none() {
        std::fs::copy(&source, &next_source.1).unwrap();
    }
    assert_eq!(hash(&next_source.1), expected);
    let started = Instant::now();
    let admitted_before = resources::global().usage().0;
    let mut observations = vec![super::qualification::resource_observation(
        "before_open",
        started,
    )];
    let baseline_cancel = Cancellation::default();
    let photo = crate::photo::PhotoImage::load_cancellable(&source, &baseline_cancel).unwrap();
    let dimensions = photo.dimensions();
    if let Ok(expected_dimensions) = std::env::var("OMA_RAW_EXPECT_DIMENSIONS") {
        let expected_dimensions = expected_dimensions
            .split('x')
            .map(|n| n.parse::<u32>().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(
            expected_dimensions.as_slice(),
            &[dimensions.0, dimensions.1]
        );
    }
    let source_samples = photo.raw.as_ref().unwrap().pixels.len();
    assert_eq!(
        source_samples,
        dimensions.0 as usize * dimensions.1 as usize * 3
    );
    let admitted_baseline = resources::global().usage().0;
    observations.push(super::qualification::resource_observation(
        "baseline_loaded",
        started,
    ));
    let metadata = photo.raw.as_ref().unwrap().metadata.clone();
    let original = photo.raw.as_ref().unwrap().pixels.clone();
    let mut session = crate::app::PhotoSession::new();
    session.import_photo(photo);
    session.images[0].develop.exposure = 0.625;
    let current = session.images[0].develop.clone();
    session.import_file(&source);
    let first = session.import_token(&source).expect("active import owner");
    let opening = Instant::now();
    while !(first.native_active() && first.native_progress() >= 8)
        && opening.elapsed() < Duration::from_secs(30)
    {
        std::thread::sleep(Duration::from_millis(1));
    }
    let active = first.native_active();
    let stage = first.native_progress();
    observations.push(super::qualification::resource_observation(
        "active_native_cancel",
        started,
    ));
    let cancelled = Instant::now();
    session.cancel_imports();
    let cleared = !session.is_loading();
    session.import_file(&next_source.1);
    let next = session
        .import_token(&next_source.1)
        .expect("next import starts immediately");
    let mut next_live_until_completion = !next.is_cancelled();
    while first.native_active() && cancelled.elapsed() < Duration::from_secs(5) {
        std::thread::sleep(Duration::from_millis(1));
    }
    let native_stop = cancelled.elapsed();
    observations.push(super::qualification::resource_observation(
        "cancelled_native_stopped",
        started,
    ));
    let ctx = eframe::egui::Context::default();
    while session.is_loading() && cancelled.elapsed() < Duration::from_secs(180) {
        next_live_until_completion &= !next.is_cancelled();
        session.poll(&ctx);
        std::thread::sleep(Duration::from_millis(2));
    }
    let next_open = cancelled.elapsed();
    observations.push(super::qualification::resource_observation(
        "next_open_finished",
        started,
    ));
    let source_after = hash(&source);
    let source_kept =
        std::sync::Arc::ptr_eq(&session.images[0].raw.as_ref().unwrap().pixels, &original);
    let kept = session.images[0].develop == current && source_kept;
    let next_accepted = session.images.len() == 2
        && session.images[1].source.as_ref() == Some(&next_source.1.canonicalize().unwrap())
        && session.images[1].dimensions() == session.images[0].dimensions()
        && session.images[1].develop.is_default();
    let admitted_after_next = resources::global().usage().0;
    let loaded_photos = session.images.len();
    let errors = session.import_errors().to_vec();
    let idle = !session.is_loading();
    let requests = [
        baseline_cancel.native_memory_request(),
        first.native_memory_request(),
        next.native_memory_request(),
    ];
    drop(session);
    drop(original);
    let released = Instant::now();
    while resources::global().usage().0 != admitted_before
        && released.elapsed() < Duration::from_secs(5)
    {
        std::thread::sleep(Duration::from_millis(2));
    }
    let admitted_after_release = resources::global().usage().0;
    observations.push(super::qualification::resource_observation(
        "final_owner_released",
        started,
    ));
    let result = serde_json::json!({"source_before_sha256":expected,"source_after_sha256":source_after,"metadata":metadata,"dimensions":dimensions,"linear_u16_samples":source_samples,"fixture_storage":fixture_storage,"admitted_before_bytes":admitted_before,"admitted_baseline_bytes":admitted_baseline,"admitted_after_next_bytes":admitted_after_next,"admitted_after_final_owner_bytes":admitted_after_release,"native_memory_requests_bytes":requests,"resource_observations":observations,"native_active_at_cancel":active,"native_progress_at_cancel":stage,"native_stop_ms":native_stop.as_secs_f64()*1000.,"next_open_ms":next_open.as_secs_f64()*1000.,"current_edits_and_source_kept":kept,"slots_cleared_immediately":cleared,"next_live_until_completion":next_live_until_completion,"next_native_progress":next.native_progress(),"next_distinct_source_accepted":next_accepted,"completed_owner_cleanup_signalled":next.is_cancelled(),"loaded_photos":loaded_photos,"errors":errors,"proof":"headless actual PhotoSession import/cancel/distinct-path next-open; sampled process-lifetime RSS/HWM is not a decoder-only peak; completed ImportJob drop defensively cancels its token after accepting its result; no native window/device/physical proof"});
    if let Some(path) = std::env::var_os("OMA_RAW_CANCEL_RECEIPT") {
        use std::io::Write;
        let path = std::path::PathBuf::from(path);
        assert!(path.is_absolute() && path.starts_with("/home"));
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .unwrap();
        writeln!(file, "{result}").unwrap();
    }
    eprintln!("RAW_CANCEL {result}");
    assert!(active && stage >= 8, "real native stage not observed");
    assert!(
        native_stop < Duration::from_millis(500),
        "active native stop took {native_stop:?}"
    );
    assert!(cleared && kept && next_live_until_completion);
    assert!(next_accepted && next.native_progress() > 0);
    assert!(idle);
    assert_eq!(loaded_photos, 2, "stale result must never publish");
    assert!(errors.is_empty());
    if let Ok(expected_memory) = std::env::var("OMA_RAW_EXPECT_NATIVE_MEMORY") {
        let expected_memory = expected_memory.parse::<u64>().unwrap();
        assert_eq!(
            requests, [expected_memory; 3],
            "the actual decoder route must use its pinned live-buffer bound"
        );
    }
    assert_eq!(
        admitted_after_release, admitted_before,
        "final sample and session owners must release the same pool"
    );
    assert_eq!(hash(&source), expected);
}

#[test]
fn shared_raw_samples_keep_legacy_json_array_and_restore_admission() {
    let image = decode(DNG).unwrap();
    let encoded = serde_json::to_string(&image).unwrap();
    assert!(encoded.contains("\"pixels\":["));
    assert!(!encoded.contains("memory"));
    let mut restored: RawImage = serde_json::from_str(&encoded).unwrap();
    let before = resources::global().retained(&restored.pixels).unwrap();
    restored.admit_memory(&Cancellation::default()).unwrap();
    assert_eq!(
        true,
        std::sync::Arc::ptr_eq(&before, restored.memory.as_ref().unwrap()),
        "restored samples must reuse their preallocation lease"
    );
    assert_eq!(image.pixels, restored.pixels);
    let clone = restored.clone();
    assert!(std::sync::Arc::ptr_eq(&clone.pixels, &restored.pixels));
}

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
