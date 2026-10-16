//! Scheduled high-resolution route checks retain an honest negative camera identity gate.
use super::*;
use std::time::Instant;

fn sample_hash(samples: &[f32]) -> String {
    let mut digest = Sha256::new();
    for chunk in samples.chunks(1024) {
        let mut bytes = [0u8; 4096];
        for (sample, target) in chunk.iter().zip(bytes.chunks_exact_mut(4)) {
            target.copy_from_slice(&sample.to_bits().to_le_bytes());
        }
        digest.update(&bytes[..chunk.len() * 4]);
    }
    format!("{:x}", digest.finalize())
}

/// Place actual camera pixels and reopen their native project without retaining duplicate documents.
/// Args: pinned source, owned directory and scheduled storage owner. Returns measured precision/ownership checks or the real resource/I/O failure.
fn native_roundtrip(
    source: &Path,
    directory: &Path,
    storage: &storage::CaseStorage,
) -> Result<Value, String> {
    use crate::{
        deep_color::{Depth, cms::Model},
        formats::{photo_placement, raw::Cancellation},
        photo::PhotoImage,
    };
    let started = Instant::now();
    let before = crate::resources::global().usage().0;
    let mut samples = vec![workflow::resource_sample("before_native", started)];
    let photo = PhotoImage::load(source)?;
    let raw = photo.raw.as_ref().ok_or("missing immutable RAW source")?;
    if (raw.width, raw.height) != (11664, 8750) {
        return Err("scheduled high-resolution geometry changed".into());
    }
    let linear = raw.pixels.clone();
    samples.push(workflow::resource_sample("native_camera_loaded", started));
    let doc = photo_placement::document(&photo, None, &Cancellation::default())?;
    let pixels = doc.layers[0].kind.pixels().ok_or("missing native raster")?;
    let native = pixels.samples()?;
    if doc.color.model != Model::Rgb
        || doc.color.depth != Depth::F32
        || !doc.color.icc_profile.is_empty()
        || doc.color.intent != 1
        || pixels
            .native
            .as_ref()
            .is_none_or(|native| native.depth != Depth::F32)
    {
        return Err("RAW placement did not preserve explicit RGB/F32/empty-ICC/intent1".into());
    }
    if !std::sync::Arc::ptr_eq(&linear, &photo.raw.as_ref().unwrap().pixels) {
        return Err("RAW source storage changed during placement".into());
    }
    let mut maximum = 0f64;
    let mut count = 0;
    for y in (0..raw.height).step_by(137) {
        for x in (0..raw.width).step_by(149) {
            let at = (y as usize * raw.width as usize + x as usize) * 3;
            let pixel = (y as usize * raw.width as usize + x as usize) * 4;
            for c in 0..3 {
                let value = f64::from(linear[at + c]) / 65535.;
                let expected = if value <= 0.0031308 {
                    value * 12.92
                } else {
                    1.055 * value.powf(1. / 2.4) - 0.055
                };
                maximum = maximum.max((f64::from(native[pixel + c]) - expected).abs());
                count += 1;
            }
        }
    }
    if maximum > 0.0000003 || native.chunks_exact(4).any(|pixel| pixel[3] != 1.) {
        return Err(format!("native OETF/straight alpha error {maximum}"));
    }
    let expected = sample_hash(&native);
    drop(native);
    drop(linear);
    drop(photo);
    samples.push(workflow::resource_sample(
        "native_placed_source_released",
        started,
    ));
    let path = directory.join("native.oma");
    struct Owned(PathBuf);
    impl Drop for Owned {
        fn drop(&mut self) {
            let _ = fs::remove_file(&self.0);
        }
    }
    let _owned = Owned(path.clone());
    let output = crate::project::encode_admitted(&doc)
        .map_err(|error| format!("native project encode: {error}"))?;
    storage.check_next(output.len() as u64)?;
    let encoded_sha256 = format!("{:x}", Sha256::digest(output.as_slice()));
    let encoded_bytes = output.len();
    samples.push(workflow::resource_sample("native_project_encoded", started));
    crate::project::save_encoded_to(&doc, &path, &output, storage)
        .map_err(|error| format!("native project write: {error}"))?;
    if hash_file(&path)? != encoded_sha256 {
        return Err("project write differs from admitted owner".into());
    }
    let weak = std::sync::Arc::downgrade(output.lease());
    let alias = output.clone();
    drop(output);
    if weak.upgrade().is_none() {
        return Err("encoded project lease ended before last alias".into());
    }
    drop(alias);
    if weak.upgrade().is_some() {
        return Err("encoded project retained output after final owner".into());
    }
    let color = doc.color.clone();
    drop(doc);
    samples.push(workflow::resource_sample(
        "native_project_written_released",
        started,
    ));
    let reopened = crate::project::load_from(&path)
        .map_err(|error| format!("native project reopen: {error}"))?;
    let reopened_pixels = reopened.layers[0]
        .kind
        .pixels()
        .ok_or("reopened native raster missing")?;
    let reopened_samples = reopened_pixels.samples()?;
    if reopened.color != color
        || reopened_pixels
            .native
            .as_ref()
            .is_none_or(|native| native.depth != Depth::F32)
        || sample_hash(&reopened_samples) != expected
    {
        return Err("native project reopen changed float bits/profile/intent".into());
    }
    samples.push(workflow::resource_sample(
        "native_project_reopened",
        started,
    ));
    drop(reopened_samples);
    drop(reopened);
    fs::remove_file(&path).map_err(|e| e.to_string())?;
    storage.check_next(0)?;
    let after = crate::resources::global().usage().0;
    samples.push(workflow::resource_sample(
        "native_final_owner_released",
        started,
    ));
    if after != before {
        return Err("native project leaked admission after final owner".into());
    }
    Ok(
        json!({"passed":true,"dimensions":[11664,8750],"native_depth":"F32","model":"RGB","icc_bytes":0,"intent":1,
        "sample_sha256":expected,"independent_oetf_samples":count,"maximum_oetf_error":maximum,"straight_alpha":true,
        "project_sha256":encoded_sha256,"project_bytes":encoded_bytes,"exact_float_bits_reopened":true,"output_last_alias_lease":true,
        "resources":samples,"storage":storage.report(),"boundary":"Actual headless placement/project write/reopen; no native Document TIFF compositor/export or window/device claim"}),
    )
}

#[test]
#[ignore = "failed native102MP save/reopen only; scheduled pinned source and shared heavy helper required"]
fn scheduled_x2d_native_project_roundtrip() {
    let manifest = parse_manifest(MANIFEST).unwrap();
    let case = manifest
        .cases
        .iter()
        .find(|case| case.id == "coverage-6565")
        .unwrap();
    assert!(case.identity.active_dimensions.is_none());
    let bindings: Bindings =
        serde_json::from_str(&std::env::var("OMA_RAW_CASE_BINDINGS").unwrap()).unwrap();
    let source = bindings.source.as_ref().expect("pinned source binding");
    verify_hash(source, case.source.sha256.as_deref()).unwrap();
    let path = PathBuf::from(std::env::var_os("OMA_RAW_CONSUMER_OUTPUT").unwrap());
    let scratch = storage::root::Scratch::create(&path).unwrap();
    let receipts = create_scratch(&PathBuf::from(
        std::env::var_os("OMA_RAW_CONSUMER_RECEIPTS").unwrap(),
    ))
    .unwrap();
    let references: Vec<_> = case
        .references
        .iter()
        .map(|reference| {
            (
                reference,
                bindings
                    .references
                    .get(&reference.id)
                    .expect("retained reference binding")
                    .clone(),
            )
        })
        .collect();
    let storage = storage::CaseStorage::new(case, source, &references, scratch.path()).unwrap();
    let started = Instant::now();
    let native = match native_roundtrip(source, scratch.path(), &storage) {
        Ok(receipt) => receipt,
        Err(error) => {
            json!({"passed":false,"error":error,"resources":workflow::resource_sample("native_failure_released",started)})
        }
    };
    let runner = std::env::current_exe().unwrap();
    let packet = json!({"schema_version":1,"case":case.id,"camera_qualified":false,"full_candidate":false,
        "active_dimensions":case.identity.active_dimensions,"native":native,
        "runner":{"revision":std::env::var("OMA_RAW_REVISION").unwrap(),"sha256":hash_file(&runner).unwrap(),"host_sha256":host_identity().unwrap(),"cargo_lock_sha256":hash_file(&Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.lock")).unwrap()},
        "source_after_sha256":hash_file(source).unwrap(),"manifest_sha256":format!("{:x}",Sha256::digest(MANIFEST.as_bytes())),
        "boundary":"Native102MP placement/project save/reopen only; earlier Photo/reference/export/sidecar/cancel proof remains at its original producer commit. No repeated Photo workflow, reference generation or camera-mode promotion."});
    storage.check_next(1024 * 1024).unwrap();
    write_json(&receipts.join("native-only.json"), &packet).unwrap();
    eprintln!("RAW_NATIVE_CONSUMER {packet}");
    assert_eq!(
        packet["source_after_sha256"].as_str(),
        case.source.sha256.as_deref()
    );
    assert_eq!(
        native["passed"], true,
        "actual native save/reopen remains failed"
    );
}

#[test]
#[ignore = "one scheduled pinned X2D case; global heavy helper, private bindings and storage plan required"]
fn scheduled_x2d_available_workflow_keeps_required_identity_failure() {
    let manifest = parse_manifest(MANIFEST).unwrap();
    let case = manifest
        .cases
        .iter()
        .find(|case| case.id == "coverage-6565")
        .unwrap();
    assert!(
        case.identity.active_dimensions.is_none(),
        "do not turn unavailable metadata into a pass"
    );
    let bindings: Bindings = serde_json::from_str(
        &std::env::var("OMA_RAW_CASE_BINDINGS").expect("OMA_RAW_CASE_BINDINGS"),
    )
    .unwrap();
    let source = bindings.source.as_ref().expect("pinned source binding");
    verify_hash(source, case.source.sha256.as_deref()).unwrap();
    let path = PathBuf::from(
        std::env::var_os("OMA_RAW_CONSUMER_OUTPUT").expect("OMA_RAW_CONSUMER_OUTPUT"),
    );
    let scratch = storage::root::Scratch::create(&path).unwrap();
    let directory = scratch.path();
    let receipts = if let Some(path) = std::env::var_os("OMA_RAW_CONSUMER_RECEIPTS") {
        create_scratch(&PathBuf::from(path)).unwrap()
    } else {
        directory.to_path_buf()
    };
    let references: Vec<_> = case
        .references
        .iter()
        .filter_map(|reference| {
            bindings
                .references
                .get(&reference.id)
                .map(|path| (reference, path.clone()))
        })
        .collect();
    assert_eq!(references.len(), case.references.len());
    let storage = storage::CaseStorage::new(case, source, &references, &directory).unwrap();
    let started = Instant::now();
    let before = crate::resources::global().usage().0;
    let native = match native_roundtrip(source, &directory, &storage) {
        Ok(receipt) => receipt,
        Err(error) => {
            json!({"passed":false,"error":error,"resources":workflow::resource_sample("native_failure_released", started)})
        }
    };
    storage.check_next(1024 * 1024).unwrap();
    write_json(&receipts.join("native.json"), &native).unwrap();
    let corpus = source.parent().unwrap();
    let report = workflow::run(case, &bindings, corpus, &directory, "required");
    let runner = std::env::current_exe().unwrap();
    let packet = json!({"schema_version":1,"case":case.id,"camera_qualified":false,"full_candidate":false,
        "runner":{"revision":std::env::var("OMA_RAW_REVISION").expect("exact revision"),"sha256":hash_file(&runner).unwrap(),"host_sha256":host_identity().unwrap(),"cargo_lock_sha256":hash_file(&Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.lock")).unwrap()},
        "source_after_sha256":hash_file(source).unwrap(),"manifest_sha256":format!("{:x}",Sha256::digest(MANIFEST.as_bytes())),"native":native,"workflow":report});
    storage.check_next(1024 * 1024).unwrap();
    write_json(&receipts.join("consumer.json"), &packet).unwrap();
    eprintln!("RAW_HIGHRES_CONSUMER {packet}");
    assert_eq!(crate::resources::global().usage().0, before);
    assert_eq!(
        packet["source_after_sha256"].as_str(),
        case.source.sha256.as_deref()
    );
    assert_eq!(report["status"], "failed");
    assert_eq!(
        report["errors"],
        json!([{"kind":"missing","detail":"unpinned active geometry"}]),
        "actual workflow/reference failures must remain failures"
    );
    assert_eq!(report["checked"], true);
    assert_eq!(
        report["comparisons"].as_array().unwrap().len(),
        case.references.len()
    );
    for comparison in report["comparisons"].as_array().unwrap() {
        assert!(comparison["stats"]["samples"].as_u64().unwrap_or(0) > 0);
    }
    assert_eq!(
        native["passed"], true,
        "actual native placement/project route failed"
    );
}
