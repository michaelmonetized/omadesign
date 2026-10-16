//! Serial real-file Photo checks; reference samples are never manufactured here.
use super::*;
use crate::photo::{PhotoImage, RgbaImage, edits};
use image::{DynamicImage, GenericImageView};
use std::io::BufReader;
use std::time::Instant;

/// Add a classified qualification failure.
/// Args: destination, category and explanation. Returns nothing.
fn problem(errors: &mut Vec<Problem>, kind: &'static str, detail: impl Into<String>) {
    errors.push(Problem {
        kind,
        detail: detail.into(),
    });
}

/// Own only the scratch files created for this case.
/// Args: file list. Returns a guard that removes those exact files on exit.
struct OwnedFiles(Vec<PathBuf>);
impl Drop for OwnedFiles {
    fn drop(&mut self) {
        for file in &self.0 {
            let _ = fs::remove_file(file);
        }
    }
}

/// Exercise one camera case and retain all missing-evidence failures.
/// Args: case, caller bindings, read-only corpus, new owned directory and mode.
/// Returns one compact receipt, including source hashes even when a check fails.
pub(super) fn run(
    case: &Case,
    bindings: &Bindings,
    corpus: &Path,
    directory: &Path,
    mode: &str,
) -> Value {
    let started = Instant::now();
    let mut resources = vec![resource_sample("before", started)];
    let source = bindings
        .source
        .clone()
        .unwrap_or_else(|| corpus.join("raw").join(&case.source.filename));
    let before = hash_file(&source).ok();
    let mut errors = Vec::new();
    let mut comparisons = Vec::new();
    let mut checks = Vec::new();
    let mut compared_kinds = BTreeSet::new();
    let mut tools = BTreeMap::new();
    let mut observed = Value::Null;
    let mut owned = OwnedFiles(Vec::new());
    identity_gaps(case, &mut errors);
    let source_valid = match verify_hash(&source, case.source.sha256.as_deref()) {
        Ok(_) => true,
        Err(error) => {
            problem(
                &mut errors,
                if before.is_none() || case.source.sha256.is_none() {
                    "missing"
                } else {
                    "invalid"
                },
                error,
            );
            false
        }
    };
    let mut ready = Vec::new();
    for reference in &case.references {
        if reference.kind == "display_look" {
            continue;
        }
        if let Some(path) = prepare_reference(reference, bindings, corpus, &mut errors, &mut tools)
        {
            ready.push((reference, path));
        }
    }
    let mut storage_guard = None;
    let mut checked = false;
    if source_valid {
        let result = (|| -> Result<(), String> {
            let size = fs::metadata(&source).map_err(|e| e.to_string())?.len();
            if size > SOURCE_BUDGET {
                problem(
                    &mut errors,
                    "scheduling_required",
                    format!(
                        "source is {size} bytes; needs parent resource scheduling above {SOURCE_BUDGET}-byte case acquisition budget; no product support conclusion"
                    ),
                );
                return Ok(());
            }
            if case
                .source
                .size_bytes
                .is_some_and(|expected| expected != size)
            {
                return Err("source size does not match the manifest".into());
            }
            storage_guard = Some(super::storage::CaseStorage::new(
                case, &source, &ready, directory,
            )?);
            let storage = storage_guard.as_ref().unwrap();
            let copy = directory.join(&case.source.filename);
            let mut destination = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&copy)
                .map_err(|e| e.to_string())?;
            owned.0.push(copy.clone());
            storage.copy(&source, &mut destination)?;
            drop(destination);
            verify_hash(&copy, case.source.sha256.as_deref())?;
            resources.push(resource_sample("camera_open_begin", started));
            let mut photo = PhotoImage::load(&copy)?;
            resources.push(resource_sample("camera_opened", started));
            if !photo.develop.is_default() {
                return Err("owned source unexpectedly loaded settings".into());
            }
            let raw = photo
                .raw
                .as_ref()
                .ok_or("Photo did not retain the RAW source")?;
            observed = json!({"make":raw.metadata.make,"model":raw.metadata.model,"dimensions":[raw.width,raw.height],
                "orientation":raw.metadata.orientation,"camera_wb":raw.metadata.camera_wb,"baseline_exposure":raw.metadata.baseline_exposure,"warnings":raw.warnings});
            if case
                .identity
                .make
                .as_ref()
                .is_some_and(|v| v != &raw.metadata.make)
                || case
                    .identity
                    .model
                    .as_ref()
                    .is_some_and(|v| v != &raw.metadata.model)
                || case
                    .identity
                    .output_dimensions
                    .is_some_and(|v| v != [raw.width, raw.height])
                || case
                    .identity
                    .orientation
                    .is_some_and(|v| v != raw.metadata.orientation)
                || case.identity.camera_wb.is_some_and(|v| {
                    v.iter()
                        .zip(raw.metadata.camera_wb)
                        .any(|(a, b)| (a - b).abs() > 0.0001)
                })
            {
                problem(
                    &mut errors,
                    "invalid",
                    "decoded camera/model/geometry/orientation/WB differs from the pinned identity",
                );
            }
            if raw.pixels.len() != raw.width as usize * raw.height as usize * 3 {
                return Err("invalid RGB16 layout".into());
            }
            storage.export(photo.dimensions())?;
            for (reference, path) in &ready {
                if reference.representation == "oriented-linear-srgb-rgb16le" {
                    record_reference(
                        reference,
                        path,
                        [raw.width, raw.height],
                        raw.pixels.as_slice(),
                        3,
                        &mut comparisons,
                        &mut compared_kinds,
                        &mut errors,
                    );
                }
            }
            if ready
                .iter()
                .any(|(reference, _)| reference.representation == "sensor-u16le-masked")
            {
                resources.push(resource_sample("sensor_reference_begin", started));
                let cancel = super::super::Cancellation::default();
                let mut encoded =
                    crate::resources::global().allocate::<u8, _>(size as usize, &cancel)?;
                File::open(&copy)
                    .map_err(|e| e.to_string())?
                    .read_exact(encoded.as_mut_slice())
                    .map_err(|e| e.to_string())?;
                let sensor = super::super::decode_sensor(&encoded, &cancel)?;
                for (reference, path) in &ready {
                    if reference.representation == "sensor-u16le-masked" {
                        record_reference(
                            reference,
                            path,
                            [sensor.width, sensor.height],
                            &sensor.samples,
                            1,
                            &mut comparisons,
                            &mut compared_kinds,
                            &mut errors,
                        );
                    }
                }
            }
            resources.push(resource_sample("baseline_preview_full_begin", started));
            check_render(&photo, &mut checks, case.preview_tolerance)?;
            resources.push(resource_sample("baseline_preview_full_checked", started));
            let mut baseline_export_hash = None;
            for extension in ["png", "tif"] {
                resources.push(resource_sample(
                    &format!("baseline_{extension}_encode_begin"),
                    started,
                ));
                let output = directory.join(format!("baseline.{extension}"));
                owned.0.push(output.clone());
                save_photo(
                    &photo,
                    &output,
                    if extension == "png" {
                        crate::export::Format::Png
                    } else {
                        crate::export::Format::Tiff
                    },
                    storage,
                )?;
                resources.push(resource_sample(
                    &format!("baseline_{extension}_decode_begin"),
                    started,
                ));
                let decoded = decode_export(&output, photo.dimensions())?;
                let pixels = decoded.as_raw().as_slice();
                let digest = hash_samples(pixels);
                if baseline_export_hash
                    .as_ref()
                    .is_some_and(|previous| previous != &digest)
                {
                    return Err("independently decoded PNG and TIFF pixels differ".into());
                }
                baseline_export_hash = Some(digest.clone());
                let precision = check_transfer(&photo, pixels)?;
                resources.push(resource_sample(&format!("{extension}_decoded"), started));
                checks.push(json!({"check":format!("{extension}_u16_codec_decode"),"dimensions":photo.dimensions(),"channel_bits":16,"pixel_sha256":digest,"stats":precision}));
                if extension == "png" {
                    for (reference, path) in &ready {
                        if reference.representation == "display-srgb-rgba16le" {
                            record_reference(
                                reference,
                                path,
                                [decoded.width(), decoded.height()],
                                pixels,
                                4,
                                &mut comparisons,
                                &mut compared_kinds,
                                &mut errors,
                            );
                        }
                    }
                }
                fs::remove_file(&output).map_err(|e| e.to_string())?;
            }
            photo.develop.exposure = 0.75;
            photo.develop.rotate = 90;
            photo.develop.crop = Some([0.0, 0.0, 1.0, 0.5]);
            resources.push(resource_sample("edited_preview_full_begin", started));
            check_render(&photo, &mut checks, case.preview_tolerance)?;
            resources.push(resource_sample("edited_tiff_encode_begin", started));
            let output = directory.join("edited.tif");
            owned.0.push(output.clone());
            save_photo(&photo, &output, crate::export::Format::Tiff, storage)?;
            let dimensions = photo
                .develop
                .output_dim(photo.dimensions().0, photo.dimensions().1);
            let decoded = decode_export(&output, dimensions)?;
            let edited_digest = hash_samples(decoded.as_raw().as_slice());
            let edited_stats = check_transfer(&photo, decoded.as_raw().as_slice())?;
            if baseline_export_hash.as_ref() == Some(&edited_digest) {
                return Err(
                    "changed exposure/crop/rotation did not change the exported image".into(),
                );
            }
            drop(decoded);
            fs::remove_file(&output).map_err(|e| e.to_string())?;
            let develop = photo.develop.clone();
            let identity = photo
                .source_identity
                .as_ref()
                .ok_or("missing Photo source identity")?;
            let sidecar = edits::sidecar_path(&copy);
            owned.0.push(sidecar.clone());
            storage.check_next(1024 * 1024)?;
            edits::save(&copy, identity, &develop)?;
            drop(photo);
            resources.push(resource_sample("settings_reopen_begin", started));
            let reopened = PhotoImage::load(&sidecar)?;
            resources.push(resource_sample("settings_reopened", started));
            if reopened.develop != develop {
                return Err("settings reopen changed the development parameters".into());
            }
            save_photo(&reopened, &output, crate::export::Format::Tiff, storage)?;
            let decoded = decode_export(&output, dimensions)?;
            if hash_samples(decoded.as_raw().as_slice()) != edited_digest {
                return Err("settings reopen changed the developed TIFF pixels".into());
            }
            verify_hash(&copy, case.source.sha256.as_deref())?;
            checks.push(json!({"check":"settings_exposure_crop_rotation_reopen","dimensions":dimensions,"pixel_sha256":edited_digest,"stats":edited_stats,"owned_source_sha256":hash_file(&copy)?}));
            checked = true;
            Ok(())
        })();
        if let Err(error) = result {
            let kind = if error.contains("storage") || error.contains("Storage") {
                "scheduling_required"
            } else {
                "workflow"
            };
            problem(&mut errors, kind, error);
        }
    }
    resources.push(resource_sample("released", started));
    if resources.last().unwrap()["admitted_bytes"]
        .as_u64()
        .unwrap()
        > resources[0]["admitted_bytes"].as_u64().unwrap()
    {
        problem(
            &mut errors,
            "workflow",
            "case retained admitted buffers after its last source/output owner dropped",
        );
    }
    let after = hash_file(&source).ok();
    if before != after {
        problem(
            &mut errors,
            "invalid",
            "original source changed during qualification",
        );
    }
    for kind in &case.required_reference_kinds {
        if !compared_kinds.contains(kind) {
            problem(
                &mut errors,
                "missing",
                format!("required reference kind {kind} has zero successful comparisons"),
            );
        }
    }
    if comparisons.is_empty() {
        problem(
            &mut errors,
            "missing",
            "zero external reference comparisons",
        );
    }
    let hard_failure = errors
        .iter()
        .any(|e| matches!(e.kind, "workflow" | "invalid"));
    let status = if errors.is_empty() {
        "passed"
    } else if mode == "optional" && !hard_failure {
        "skipped"
    } else {
        "failed"
    };
    json!({"schema_version":1,"case":case.id,"required":case.required,"status":status,"checked":checked,
        "source_before_sha256":before,"source_after_sha256":after,"required_reference_kinds":case.required_reference_kinds,
        "observed":observed,"reference_tools":tools,"comparisons":comparisons,"checks":checks,"errors":errors,
        "resources":{"measurement":"Linux procfs process RSS/high-water bytes and shared pool admission; process lifetime high-water includes test/reference work, not an isolated decoder peak","samples":resources},"storage":storage_guard.as_ref().map(|guard| guard.report())})
}

/// Exercise the shared admitted export writer under the scheduled case guard.
/// Args: immutable photo, owned output, explicit codec and storage owner. Returns after an atomic guarded write.
fn save_photo(
    photo: &PhotoImage,
    output: &Path,
    format: crate::export::Format,
    storage: &super::storage::CaseStorage,
) -> Result<(), String> {
    let (w, h) = photo.dimensions();
    storage.export(photo.develop.output_dim(w, h))?;
    crate::export::save(
        &crate::export::Source::Photo(photo.clone()),
        crate::export::Settings {
            format,
            scale: 1,
            ai: None,
        },
        output,
        storage,
    )?;
    storage.check_next(0)
}

/// Record actual process memory alongside admitted buffer ownership.
/// Args: workflow stage and monotonic start. Returns no host names or local paths; unavailable OS counters remain null.
pub(super) fn resource_sample(stage: &str, started: Instant) -> Value {
    let status = fs::read_to_string("/proc/self/status").unwrap_or_default();
    let bytes = |counter: &str| {
        status.lines().find_map(|line| {
            let mut words = line.split_whitespace();
            (words.next()? == counter)
                .then(|| words.next()?.parse::<u64>().ok()?.checked_mul(1024))
                .flatten()
        })
    };
    let (admitted, limit) = crate::resources::global().usage();
    let sample = json!({"stage":stage,"elapsed_ms":started.elapsed().as_secs_f64()*1000.,
        "rss_bytes":bytes("VmRSS:"),"process_high_water_bytes":bytes("VmHWM:"),
        "host_available_bytes":crate::resources::available_memory(),"admitted_bytes":admitted,"pending_bytes":crate::resources::global().pending_bytes(),"pool_limit_bytes":limit});
    eprintln!("RAW_WORKFLOW_STAGE {sample}");
    sample
}

/// Keep unknown camera/mode identity visible as missing evidence.
/// Args: manifest case and errors. Returns nothing; firmware remains optional when unavailable.
fn identity_gaps(case: &Case, errors: &mut Vec<Problem>) {
    for (name, missing) in [
        (
            "make",
            case.identity.make.as_ref().is_none_or(|s| s.is_empty()),
        ),
        (
            "model",
            case.identity.model.as_ref().is_none_or(|s| s.is_empty()),
        ),
        ("compression/mode", case.identity.compression.is_none()),
        ("CFA", case.identity.cfa.is_none()),
        ("bit depth", case.identity.bit_depth.is_none()),
        ("sensor geometry", case.identity.sensor_dimensions.is_none()),
        ("active geometry", case.identity.active_dimensions.is_none()),
        (
            "oriented output geometry",
            case.identity.output_dimensions.is_none(),
        ),
        ("orientation", case.identity.orientation.is_none()),
        ("white balance", case.identity.camera_wb.is_none()),
        ("profile", case.identity.profile.is_none()),
    ] {
        if missing {
            problem(errors, "missing", format!("unpinned {name}"));
        }
    }
    if case.source.url.is_none()
        || case.source.license.is_none()
        || case.source.provenance.is_empty()
    {
        problem(
            errors,
            "missing",
            "source URL/license/provenance is incomplete",
        );
    }
}

/// Validate independently supplied reference and executable identities.
/// Args: reference, bindings, corpus, error/tool receipts. Returns a verified reference path when usable.
fn prepare_reference(
    reference: &Reference,
    bindings: &Bindings,
    corpus: &Path,
    errors: &mut Vec<Problem>,
    tools: &mut BTreeMap<String, Value>,
) -> Option<PathBuf> {
    let mut valid = true;
    let path = bindings.references.get(&reference.id).cloned().or_else(|| {
        reference
            .filename
            .as_ref()
            .map(|name| corpus.join("references").join(name))
    });
    match &path {
        Some(path) => {
            if let Err(e) = verify_hash(path, reference.sha256.as_deref()) {
                problem(
                    errors,
                    if path.is_file() && reference.sha256.is_some() {
                        "invalid"
                    } else {
                        "missing"
                    },
                    format!("{}: {e}", reference.id),
                );
                valid = false;
            }
        }
        None => {
            problem(
                errors,
                "missing",
                format!("{}: missing reference path/hash", reference.id),
            );
            valid = false;
        }
    }
    let executable = bindings.executables.get(&reference.id);
    match executable {
        Some(path) => match verify_hash(path, reference.toolchain.executable_sha256.as_deref()) {
            Ok(hash) => {
                tools.insert(reference.id.clone(),json!({"name":reference.toolchain.name,"version":reference.toolchain.version,"executable_sha256":hash,"platform":reference.toolchain.platform,"command":reference.toolchain.command}));
            }
            Err(e) => {
                problem(
                    errors,
                    if reference.toolchain.executable_sha256.is_some() {
                        "invalid"
                    } else {
                        "missing"
                    },
                    format!("{} tool: {e}", reference.id),
                );
                valid = false;
            }
        },
        None => {
            problem(
                errors,
                "missing",
                format!("{}: missing reference executable binding", reference.id),
            );
            valid = false;
        }
    }
    if reference.toolchain.version.is_none()
        || reference.toolchain.platform.is_none()
        || reference.toolchain.command.is_empty()
        || reference.dimensions.is_none()
    {
        problem(
            errors,
            "missing",
            format!(
                "{}: missing toolchain/recipe/platform/reference geometry",
                reference.id
            ),
        );
        valid = false;
    }
    if valid { path } else { None }
}

/// Compare same-plane u16 samples against an independently pinned file.
/// Args: reference, path, actual geometry/samples/channel count and receipts. Returns nothing.
fn record_reference(
    reference: &Reference,
    path: &Path,
    dimensions: [u32; 2],
    pixels: &[u16],
    channels: usize,
    comparisons: &mut Vec<Value>,
    kinds: &mut BTreeSet<String>,
    errors: &mut Vec<Problem>,
) {
    eprintln!(
        "RAW_REFERENCE_BEGIN {} samples={}",
        reference.id,
        pixels.len()
    );
    let result = (|| -> Result<Stats, String> {
        if reference.dimensions != Some(dimensions)
            || pixels.len() != dimensions[0] as usize * dimensions[1] as usize * channels
        {
            return Err("reference geometry/plane mismatch".into());
        }
        if fs::metadata(path).map_err(|e| e.to_string())?.len() != pixels.len() as u64 * 2 {
            return Err("reference byte length mismatch".into());
        }
        let mut reader = BufReader::new(File::open(path).map_err(|e| e.to_string())?);
        let mut stats = Stats::default();
        for (index, actual) in pixels.iter().enumerate() {
            let mut bytes = [0u8; 2];
            reader.read_exact(&mut bytes).map_err(|e| e.to_string())?;
            let expected = u16::from_le_bytes(bytes);
            let channel = index % channels;
            if channel < 3 {
                stats.add(channel, f64::from(actual.abs_diff(expected)) / 65535.0);
            } else if *actual != expected {
                return Err("reference alpha differs".into());
            }
        }
        let stats = stats.finish_channels(channels.min(3))?;
        verify_hash(path, reference.sha256.as_deref())?;
        Ok(stats)
    })();
    match result {
        Ok(stats) => {
            let passed =
                stats.mean <= reference.tolerance.mean && stats.max <= reference.tolerance.max;
            comparisons.push(json!({"reference":reference.id,"kind":reference.kind,"representation":reference.representation,"reference_sha256":reference.sha256,"passed":passed,"stats":stats}));
            if passed {
                kinds.insert(reference.kind.clone());
            } else {
                problem(
                    errors,
                    "invalid",
                    format!(
                        "{}: reference error mean {} max {} exceeds declared bounds",
                        reference.id, stats.mean, stats.max
                    ),
                );
            }
        }
        Err(error) => problem(errors, "invalid", format!("{}: {error}", reference.id)),
    }
    eprintln!("RAW_REFERENCE_END {}", reference.id);
}

/// Keep codec validation samples admitted until their final owner drops.
struct DecodedExport {
    image: image::ImageBuffer<image::Rgba<u16>, Vec<u16>>,
    _memory: std::sync::Arc<crate::resources::Lease>,
}
impl std::ops::Deref for DecodedExport {
    type Target = image::ImageBuffer<image::Rgba<u16>, Vec<u16>>;
    fn deref(&self) -> &Self::Target {
        &self.image
    }
}

/// Decode the actual encoded export through the codec reader.
/// Args: exported PNG/TIFF and expected dimensions. Returns admitted RGBA16 without an 8-to-16 promotion or fixed pixel ceiling.
fn decode_export(path: &Path, dimensions: (u32, u32)) -> Result<DecodedExport, String> {
    let bytes = u64::from(dimensions.0)
        .checked_mul(u64::from(dimensions.1))
        .and_then(|pixels| pixels.checked_mul(16))
        .and_then(|bytes| bytes.checked_add(64 * 1024 * 1024))
        .ok_or("export validation allocation overflow")?;
    let cancel = super::super::Cancellation::default();
    let memory = crate::resources::global().reserve(bytes, &cancel)?;
    let mut reader = image::ImageReader::open(path)
        .map_err(|e| e.to_string())?
        .with_guessed_format()
        .map_err(|e| e.to_string())?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(dimensions.0);
    limits.max_image_height = Some(dimensions.1);
    limits.max_alloc = Some(bytes);
    reader.limits(limits);
    let image = reader.decode().map_err(|e| e.to_string())?;
    if image.dimensions() != dimensions {
        return Err("export geometry mismatch".into());
    }
    match image {
        DynamicImage::ImageRgba16(image) => {
            memory.resize(image.as_raw().capacity() as u64 * 2, &cancel)?;
            Ok(DecodedExport {
                image,
                _memory: memory,
            })
        }
        _ => Err("RAW export codec did not decode sixteen-bit RGBA channels".into()),
    }
}

/// Hash canonical little-endian samples independently of encoded file layout.
/// Args: sample slice. Returns its SHA-256.
fn hash_samples(samples: &[u16]) -> String {
    let mut hash = Sha256::new();
    for chunk in samples.chunks(4096) {
        let mut bytes = [0u8; 8192];
        for (sample, target) in chunk.iter().zip(bytes.chunks_exact_mut(2)) {
            target.copy_from_slice(&sample.to_le_bytes());
        }
        hash.update(&bytes[..chunk.len() * 2]);
    }
    format!("{:x}", hash.finalize())
}

#[cfg(test)]
#[test]
fn canonical_u16_hash_keeps_little_endian_bytes_across_chunk_boundaries() {
    let samples: Vec<_> = (0..8201u32).map(|i| (i * 137) as u16).collect();
    let mut reference = Sha256::new();
    for sample in &samples {
        reference.update(sample.to_le_bytes());
    }
    assert_eq!(
        hash_samples(&samples),
        format!("{:x}", reference.finalize())
    );
}

/// Evaluate the simple qualification exposure/framing recipe independently.
/// Args: linear RGB16, source/output dimensions, exposure, quarter-turn and half crop.
/// Returns normalized sRGB channel values at an output pixel; no production developer is invoked.
fn expected_pixel(
    pixels: &[u16],
    dimensions: (u32, u32),
    output: (u32, u32),
    exposure: f32,
    rotated: bool,
    x: u32,
    y: u32,
) -> [f64; 3] {
    let (w, h) = dimensions;
    let (sx, sy) = if rotated {
        let rotated_y = ((f64::from(y) + 0.5) * 0.5 * f64::from(w) / f64::from(output.1) - 0.5)
            .round()
            .clamp(0.0, f64::from(w - 1)) as u32;
        (rotated_y, h - 1 - x)
    } else {
        (x, y)
    };
    let offset = (sy as usize * w as usize + sx as usize) * 3;
    std::array::from_fn(|c| {
        let linear = (f64::from(pixels[offset + c]) / 65535.0 * f64::from(2f32.powf(exposure)))
            .clamp(0.0, 1.0);
        if linear <= 0.0031308 {
            linear * 12.92
        } else {
            1.055 * linear.powf(1.0 / 2.4) - 0.055
        }
    })
}

/// Check full-resolution u16 export against independent transfer/framing math.
/// Args: loaded photo and codec-decoded RGBA16. Returns normalized RGB error statistics.
fn check_transfer(photo: &PhotoImage, pixels: &[u16]) -> Result<Stats, String> {
    let raw = photo.raw.as_ref().ok_or("missing raw")?;
    let dimensions = photo.develop.output_dim(raw.width, raw.height);
    if pixels.len() != dimensions.0 as usize * dimensions.1 as usize * 4 {
        return Err("invalid export sample length".into());
    }
    let mut stats = Stats::default();
    for y in 0..dimensions.1 {
        for x in 0..dimensions.0 {
            let expected = expected_pixel(
                raw.pixels.as_slice(),
                (raw.width, raw.height),
                dimensions,
                photo.develop.exposure + raw.metadata.baseline_exposure,
                photo.develop.rotate == 90,
                x,
                y,
            );
            let offset = (y as usize * dimensions.0 as usize + x as usize) * 4;
            if pixels[offset + 3] != 65535 {
                return Err("export lost opaque sensor coverage".into());
            }
            for c in 0..3 {
                stats.add(
                    c,
                    (f64::from(pixels[offset + c]) / 65535.0 - expected[c]).abs(),
                );
            }
        }
    }
    let stats = stats.finish()?;
    if stats.max > 3.0 / 65535.0 {
        return Err(format!(
            "u16 transfer/framing error {} exceeds three channel steps",
            stats.max
        ));
    }
    Ok(stats)
}

/// Check preview and full rendering against the same linear-first recipe.
/// Args: source photo, receipt list and measured preview bounds. Returns success or discrepancy.
fn check_render(
    photo: &PhotoImage,
    checks: &mut Vec<Value>,
    tolerance: Tolerance,
) -> Result<(), String> {
    let raw = photo.raw.as_ref().ok_or("missing raw")?;
    let preview = photo.render_preview()?;
    let scale = (1600.0 / f64::from(raw.width.max(raw.height))).min(1.0);
    let dims = (
        (f64::from(raw.width) * scale).round().max(1.0) as u32,
        (f64::from(raw.height) * scale).round().max(1.0) as u32,
    );
    let source = image::ImageBuffer::<image::Rgb<u16>, _>::from_raw(
        raw.width,
        raw.height,
        raw.pixels.as_slice(),
    )
    .ok_or("invalid source")?;
    let oracle_bytes = u64::from(raw.width)
        .checked_mul(u64::from(dims.1))
        .and_then(|pixels| pixels.checked_mul(16))
        .and_then(|bytes| bytes.checked_add(u64::from(dims.0) * u64::from(dims.1) * 6))
        .and_then(|bytes| bytes.checked_add(32 * 1024 * 1024))
        .ok_or("preview reference allocation overflow")?;
    let cancel = super::super::Cancellation::default();
    let oracle_memory = crate::resources::global().reserve(oracle_bytes, &cancel)?;
    let resized = image::imageops::resize(
        &source,
        dims.0,
        dims.1,
        image::imageops::FilterType::Triangle,
    );
    oracle_memory.resize(resized.as_raw().capacity() as u64 * 2, &cancel)?;
    let full = photo.render_full()?;
    for (name, actual, linear, source_dims) in [
        (
            "normalized_preview",
            &*preview,
            resized.as_raw().as_slice(),
            dims,
        ),
        (
            "normalized_full",
            &*full,
            raw.pixels.as_slice(),
            (raw.width, raw.height),
        ),
    ] {
        let expected_dims = if photo.develop.rotate == 90 {
            (
                source_dims.1,
                (source_dims.0 as f32 * 0.5).round().max(1.0) as u32,
            )
        } else {
            source_dims
        };
        if (actual.w, actual.h) != expected_dims {
            return Err(format!("{name}: render geometry mismatch"));
        }
        let stats = compare_render(
            actual,
            linear,
            source_dims,
            photo.develop.exposure + raw.metadata.baseline_exposure,
            photo.develop.rotate == 90,
        )?;
        checks.push(json!({"check":name,"normalization":"Triangle resize in linear RGB16 before sRGB transfer and framing","dimensions":expected_dims,"stats":stats}));
        if stats.mean > tolerance.mean || stats.max > tolerance.max {
            return Err(format!("{name}: normalized error exceeds declared bounds"));
        }
    }
    Ok(())
}

/// Compare a rendered image with the independently evaluated neutral recipe.
/// Args: RGBA8 output, RGB16 source, geometry, exposure and framing. Returns normalized RGB statistics.
fn compare_render(
    actual: &RgbaImage,
    linear: &[u16],
    dimensions: (u32, u32),
    exposure: f32,
    rotated: bool,
) -> Result<Stats, String> {
    let mut stats = Stats::default();
    for y in 0..actual.h {
        for x in 0..actual.w {
            let expected = expected_pixel(
                linear,
                dimensions,
                (actual.w, actual.h),
                exposure,
                rotated,
                x,
                y,
            );
            let offset = (y as usize * actual.w as usize + x as usize) * 4;
            if actual.data[offset + 3] != 255 {
                return Err("render lost opaque sensor coverage".into());
            }
            for c in 0..3 {
                stats.add(
                    c,
                    (f64::from(actual.data[offset + c]) / 255.0 - expected[c]).abs(),
                );
            }
        }
    }
    stats.finish()
}
