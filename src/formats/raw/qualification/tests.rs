use super::*;

/// Own a unique test directory under this checkout's ignored QA folder.
/// Args: none. Returns a directory guard; no shared or cached fixture is removed.
struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Self {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join(".qa-runs");
        fs::create_dir_all(&root).unwrap();
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        Self(
            create_scratch(&root.join(format!("raw-contract-{}-{nonce}", std::process::id())))
                .unwrap(),
        )
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        for entry in fs::read_dir(&self.0).unwrap() {
            let entry = entry.unwrap();
            if entry.file_type().unwrap().is_file() {
                fs::remove_file(entry.path()).unwrap();
            }
        }
        fs::remove_dir(&self.0).unwrap();
    }
}

/// Read a mutable original contract value.
/// Args: none. Returns the checked-in manifest as JSON for rejection tests.
fn manifest_json() -> Value {
    serde_json::from_str(MANIFEST).unwrap()
}

#[test]
fn raw_camera_contract_covers_every_advertised_extension() {
    let manifest = parse_manifest(MANIFEST).unwrap();
    assert_eq!(manifest.families.len(), super::super::EXTENSIONS.len());
    assert!(manifest.cases.iter().all(|case| case.required));
    assert!(
        manifest
            .cases
            .iter()
            .any(|case| case.references.iter().any(|r| r.sensor_fnv1a64.is_some()))
    );
}

#[test]
fn raw_camera_contract_rejects_future_invalid_and_zero_inventory() {
    for mutation in 0..8 {
        let mut value = manifest_json();
        match mutation {
            0 => value["schema_version"] = json!(2),
            1 => value["families"]
                .as_array_mut()
                .unwrap()
                .pop()
                .map(|_| ())
                .unwrap(),
            2 => value["cases"] = json!([]),
            3 => value["cases"][0]["source"]["filename"] = json!("../original.3fr"),
            4 => value["cases"][0]["source"]["sha256"] = json!("unverified"),
            5 => value["cases"][0]["preview_tolerance"] = json!({"mean":0.5,"max":0.1}),
            6 => value["cases"][0]["identity"]["bit_depth"] = json!(0),
            _ => value["cases"][0]["future_required_feature"] = json!(true),
        }
        assert!(
            parse_manifest(&value.to_string()).is_err(),
            "accepted invalid contract mutation {mutation}"
        );
    }
}

#[test]
fn raw_camera_contract_rejects_reference_downgrades_and_wrong_planes() {
    for mutation in 0..5 {
        let mut value = manifest_json();
        match mutation {
            0 => value["cases"][0]["required_reference_kinds"] = json!([]),
            1 => value["cases"][0]["required_reference_kinds"] = json!(["libraw_integration"]),
            2 => {
                value["cases"][0]["required_reference_kinds"] =
                    json!(["libraw_integration", "display_look"])
            }
            3 => {
                value["cases"][0]["references"][0]["representation"] = json!("sensor-u16le-masked")
            }
            _ => {
                value["cases"][0]["references"][0]["toolchain"]["executable_sha256"] =
                    json!("not-a-pin")
            }
        }
        assert!(parse_manifest(&value.to_string()).is_err());
    }
}

#[test]
fn raw_camera_contract_missing_corpus_is_failed_or_explicitly_skipped() {
    let scratch = Scratch::new();
    let manifest = parse_manifest(MANIFEST).unwrap();
    let case = &manifest.cases[0];
    let required = workflow::run(
        case,
        &Bindings::default(),
        &scratch.0,
        &scratch.0,
        "required",
    );
    assert_eq!(required["status"], "failed");
    assert_eq!(required["checked"], false);
    assert!(required["comparisons"].as_array().unwrap().is_empty());
    assert!(
        required["errors"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["detail"].as_str().unwrap().contains("zero"))
    );
    let optional = workflow::run(
        case,
        &Bindings::default(),
        &scratch.0,
        &scratch.0,
        "optional",
    );
    assert_eq!(optional["status"], "skipped");
    assert_eq!(optional["checked"], false);
}

#[test]
fn raw_camera_contract_rejects_missing_and_changed_file_hashes() {
    let scratch = Scratch::new();
    let path = scratch.0.join("own-bytes.raw");
    fs::write(&path, b"original contract bytes").unwrap();
    let expected = hash_file(&path).unwrap();
    assert!(verify_hash(&path, None).is_err());
    assert!(verify_hash(&path, Some(&"0".repeat(64))).is_err());
    assert_eq!(verify_hash(&path, Some(&expected)).unwrap(), expected);
    fs::write(&path, b"changed contract bytes").unwrap();
    assert!(verify_hash(&path, Some(&expected)).is_err());
    let mut case = parse_manifest(MANIFEST).unwrap().cases.remove(0);
    case.source.sha256 = Some(expected);
    let bindings = Bindings {
        source: Some(path.clone()),
        ..Default::default()
    };
    let report = workflow::run(&case, &bindings, &scratch.0, &scratch.0, "optional");
    assert_eq!(
        report["status"], "failed",
        "optional runs must still fail corrupt files"
    );
    assert!(path.exists());
}

#[test]
fn raw_camera_contract_zero_comparisons_and_nonfinite_bounds_fail() {
    assert!(Stats::default().finish().is_err());
    let mut stats = Stats::default();
    stats.add(0, 0.0);
    assert!(stats.finish().is_err());
    assert!(!valid_tolerance(Tolerance {
        mean: f64::NAN,
        max: 1.0
    }));
    assert!(!valid_tolerance(Tolerance {
        mean: 0.0,
        max: f64::INFINITY
    }));
    assert!(!valid_tolerance(Tolerance {
        mean: -0.1,
        max: 0.0
    }));
    let mut stats = Stats::default();
    for (c, error) in [0.0, 0.25, 0.5].into_iter().enumerate() {
        stats.add(c, error);
    }
    let stats = stats.finish().unwrap();
    assert_eq!(stats.samples, 3);
    assert_eq!(stats.changed, 2);
    assert_eq!(stats.mean, 0.25);
    assert_eq!(stats.max, 0.5);
    assert_eq!(stats.channel_mean, [0.0, 0.25, 0.5]);
}

#[test]
fn raw_camera_contract_never_compares_sensor_hash_to_developed_rgb() {
    let scratch = Scratch::new();
    let manifest = parse_manifest(MANIFEST).unwrap();
    let case = manifest
        .cases
        .iter()
        .find(|case| {
            case.references
                .iter()
                .any(|r| r.kind == "independent_sensor")
        })
        .unwrap();
    let report = workflow::run(
        case,
        &Bindings::default(),
        &scratch.0,
        &scratch.0,
        "required",
    );
    assert_eq!(report["status"], "failed");
    assert!(report["comparisons"].as_array().unwrap().is_empty());
    assert!(report["errors"].as_array().unwrap().iter().any(|error| {
        error["kind"] == "missing"
            && error["detail"]
                .as_str()
                .unwrap()
                .contains("independent_sensor")
    }));
}

#[test]
fn raw_camera_contract_owns_outputs_and_preserves_user_files() {
    let scratch = Scratch::new();
    let original = scratch.0.join("cached.original");
    fs::write(&original, b"user owned").unwrap();
    assert!(create_scratch(&scratch.0).is_err());
    assert!(create_scratch(Path::new("/tmp/raw-qualification")).is_err());
    assert!(!safe_name("../original.raw"));
    assert!(!safe_name(".hidden"));
    assert!(!safe_name("fixture/raw.dng"));
    assert_eq!(fs::read(original).unwrap(), b"user owned");
}

/// Generate a tiny original sensor DNG from TIFF/DNG tag primitives.
/// Args: none. Returns a 64x48 Bayer ramp with no embedded preview or private pixels.
fn own_dng() -> Vec<u8> {
    let mut tags: Vec<(u16, u16, u32, Vec<u8>)> = Vec::new();
    for (id, value) in [
        (254, 0),
        (256, 64),
        (257, 48),
        (273, 0),
        (278, 48),
        (279, 64 * 48 * 2),
        (50717, 65535),
    ] {
        tags.push((id, 4, 1, u32::to_le_bytes(value).to_vec()));
    }
    for (id, value) in [
        (258, 16),
        (259, 1),
        (262, 32803),
        (274, 1),
        (277, 1),
        (284, 1),
        (50711, 1),
        (50778, 21),
    ] {
        tags.push((id, 3, 1, u16::to_le_bytes(value).to_vec()));
    }
    for (id, value) in [
        (271, "Omadesign"),
        (272, "Qualification Bayer"),
        (50708, "Omadesign Qualification Bayer"),
    ] {
        let mut bytes = value.as_bytes().to_vec();
        bytes.push(0);
        tags.push((id, 2, bytes.len() as u32, bytes));
    }
    tags.extend([
        (33421, 3, 2, vec![2, 0, 2, 0]),
        (33422, 1, 4, vec![0, 1, 1, 2]),
        (50706, 1, 4, vec![1, 4, 0, 0]),
        (50707, 1, 4, vec![1, 1, 0, 0]),
        (50710, 1, 3, vec![0, 1, 2]),
    ]);
    for (id, values, signed) in [
        (50714, vec![(0i32, 1i32)], false),
        (50718, vec![(1, 1), (1, 1)], false),
        (
            50721,
            vec![
                (1, 1),
                (0, 1),
                (0, 1),
                (0, 1),
                (1, 1),
                (0, 1),
                (0, 1),
                (0, 1),
                (1, 1),
            ],
            true,
        ),
        (50728, vec![(1, 1), (1, 1), (1, 1)], false),
        (50730, vec![(0, 1)], true),
    ] {
        let data = values
            .iter()
            .flat_map(|(a, b)| a.to_le_bytes().into_iter().chain(b.to_le_bytes()))
            .collect();
        tags.push((id, if signed { 10 } else { 5 }, values.len() as u32, data));
    }
    tags.sort_by_key(|tag| tag.0);
    let payload_start = 8 + 2 + 12 * tags.len() + 4;
    let mut payload = Vec::new();
    let mut directory = Vec::new();
    let mut strip_entry = 0;
    for (id, kind, count, data) in tags {
        directory.extend(id.to_le_bytes());
        directory.extend(kind.to_le_bytes());
        directory.extend(count.to_le_bytes());
        if id == 273 {
            strip_entry = directory.len();
        }
        if data.len() > 4 {
            directory.extend(((payload_start + payload.len()) as u32).to_le_bytes());
            payload.extend(data);
            if payload.len() % 2 != 0 {
                payload.push(0);
            }
        } else {
            let mut value = [0u8; 4];
            value[..data.len()].copy_from_slice(&data);
            directory.extend(value);
        }
    }
    directory[strip_entry..strip_entry + 4]
        .copy_from_slice(&((payload_start + payload.len()) as u32).to_le_bytes());
    let mut bytes = b"II*\0".to_vec();
    bytes.extend(8u32.to_le_bytes());
    bytes.extend(((directory.len() / 12) as u16).to_le_bytes());
    bytes.extend(directory);
    bytes.extend(0u32.to_le_bytes());
    bytes.extend(payload);
    for index in 0..64 * 48 {
        bytes.extend((1000 + index as u16 * 17).to_le_bytes());
    }
    bytes
}

#[test]
fn raw_camera_contract_real_photo_workflow_cannot_replace_external_references() {
    let scratch = Scratch::new();
    let original = scratch.0.join("own.dng");
    fs::write(&original, own_dng()).unwrap();
    let before = hash_file(&original).unwrap();
    let mut case = parse_manifest(MANIFEST).unwrap().cases.remove(0);
    case.id = "own-synthetic-contract".into();
    case.extension = "dng".into();
    case.source.filename = "owned.dng".into();
    case.source.sha256 = Some(before.clone());
    case.identity = Identity {
        make: Some("Omadesign".into()),
        model: Some("Qualification Bayer".into()),
        firmware: None,
        compression: Some("uncompressed".into()),
        cfa: Some("RGGB".into()),
        bit_depth: Some(16),
        sensor_dimensions: Some([64, 48]),
        active_dimensions: Some([64, 48]),
        output_dimensions: Some([64, 48]),
        orientation: Some(0),
        camera_wb: None,
        profile: Some("own identity matrix".into()),
        crop: None,
        opcodes: None,
        frames: Some(1),
    };
    let bindings = Bindings {
        source: Some(original.clone()),
        ..Default::default()
    };
    let report = workflow::run(&case, &bindings, &scratch.0, &scratch.0, "required");
    assert_eq!(
        report["status"], "failed",
        "workflow math is not an independent camera decoder"
    );
    assert_eq!(report["checked"], true, "{report}");
    assert_eq!(report["source_before_sha256"], before);
    assert_eq!(report["source_after_sha256"], before);
    let resources = report["resources"]["samples"].as_array().unwrap();
    assert_eq!(resources.first().unwrap()["stage"], "before");
    assert_eq!(resources.last().unwrap()["stage"], "released");
    assert_eq!(
        resources.first().unwrap()["admitted_bytes"],
        resources.last().unwrap()["admitted_bytes"]
    );
    assert!(
        resources
            .iter()
            .any(|sample| sample["stage"] == "camera_opened")
    );
    #[cfg(target_os = "linux")]
    assert!(resources.iter().all(|sample| {
        sample["rss_bytes"].as_u64().is_some_and(|n| n > 0)
            && sample["process_high_water_bytes"]
                .as_u64()
                .is_some_and(|n| n > 0)
    }));
    assert!(report["comparisons"].as_array().unwrap().is_empty());
    assert!(
        report["checks"]
            .as_array()
            .unwrap()
            .iter()
            .any(|check| check["check"] == "settings_exposure_crop_rotation_reopen")
    );
    assert!(original.exists());
}
