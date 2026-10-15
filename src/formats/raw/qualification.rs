//! Camera qualification contract, separate from ordinary synthetic decoder tests.
//! Register this module under `cfg(test)` in raw.rs; the coordinator owns that seam.

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};

#[path = "qualification/tests.rs"]
mod tests;
#[path = "qualification/workflow.rs"]
mod workflow;

const MANIFEST: &str = include_str!("../../../docs/qa/raw-camera-manifest.json");
const SOURCE_BUDGET: u64 = 512 * 1024 * 1024;
const TEMP_BUDGET: u64 = 1024 * 1024 * 1024;

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    schema_version: u32,
    scope: String,
    provenance: Value,
    budgets: Budgets,
    families: Vec<Family>,
    cases: Vec<Case>,
}

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct Budgets {
    source_bytes: u64,
    temporary_bytes: u64,
    parallel_cases: u32,
}

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct Family {
    extension: String,
    family: String,
    status: String,
    mode_axes: Vec<String>,
    candidate_cases: Vec<String>,
    unresolved_modes: Vec<String>,
    evidence: Vec<String>,
}

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct Case {
    id: String,
    extension: String,
    required: bool,
    status: String,
    identity: Identity,
    source: Source,
    required_reference_kinds: Vec<String>,
    references: Vec<Reference>,
    preview_tolerance: Tolerance,
    notes: Vec<String>,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Identity {
    make: Option<String>,
    model: Option<String>,
    firmware: Option<String>,
    compression: Option<String>,
    cfa: Option<String>,
    bit_depth: Option<u32>,
    sensor_dimensions: Option<[u32; 2]>,
    active_dimensions: Option<[u32; 2]>,
    output_dimensions: Option<[u32; 2]>,
    orientation: Option<i32>,
    camera_wb: Option<[f32; 4]>,
    profile: Option<String>,
    crop: Option<[u32; 4]>,
    opcodes: Option<String>,
    frames: Option<u32>,
}

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct Source {
    filename: String,
    url: Option<String>,
    sha256: Option<String>,
    size_bytes: Option<u64>,
    license: Option<String>,
    provenance: String,
}

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct Reference {
    id: String,
    kind: String,
    representation: String,
    filename: Option<String>,
    sha256: Option<String>,
    dimensions: Option<[u32; 2]>,
    sensor_fnv1a64: Option<String>,
    toolchain: Toolchain,
    provenance: String,
    tolerance: Tolerance,
}

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct Toolchain {
    name: String,
    version: Option<String>,
    executable_sha256: Option<String>,
    archive_sha256: Option<String>,
    platform: Option<String>,
    command: Vec<String>,
}

#[derive(Clone, Copy, Deserialize)]
#[serde(deny_unknown_fields)]
struct Tolerance {
    mean: f64,
    max: f64,
}

#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct Bindings {
    source: Option<PathBuf>,
    #[serde(default)]
    references: BTreeMap<String, PathBuf>,
    #[serde(default)]
    executables: BTreeMap<String, PathBuf>,
}

#[derive(Serialize)]
struct Problem {
    kind: &'static str,
    detail: String,
}

#[derive(Default, Serialize)]
struct Stats {
    samples: u64,
    changed: u64,
    mean: f64,
    max: f64,
    channel_mean: [f64; 3],
    channel_max: [f64; 3],
    #[serde(skip)]
    sums: [f64; 3],
    #[serde(skip)]
    counts: [u64; 3],
}

impl Stats {
    /// Record one normalized RGB error.
    /// Args: channel and absolute error in 0..1. Returns the updated accumulator.
    fn add(&mut self, channel: usize, error: f64) {
        self.samples += 1;
        self.changed += u64::from(error != 0.0);
        self.sums[channel] += error;
        self.counts[channel] += 1;
        self.channel_max[channel] = self.channel_max[channel].max(error);
        self.max = self.max.max(error);
    }

    /// Finish nonempty RGB statistics.
    /// Args: none. Returns means, maxima and changed counts, or a zero-comparison error.
    fn finish(self) -> Result<Self, String> {
        self.finish_channels(3)
    }
    fn finish_channels(mut self, channels: usize) -> Result<Self, String> {
        if self.samples == 0 || self.counts[..channels].contains(&0) {
            return Err("zero RGB comparisons".into());
        }
        self.mean = self.sums.iter().sum::<f64>() / self.samples as f64;
        for channel in 0..3 {
            self.channel_mean[channel] = if self.counts[channel] > 0 {
                self.sums[channel] / self.counts[channel] as f64
            } else {
                0.
            };
        }
        Ok(self)
    }
}

/// Validate a manifest without treating missing evidence as qualification.
/// Args: JSON text. Returns a versioned, complete extension inventory or a schema error.
fn parse_manifest(text: &str) -> Result<Manifest, String> {
    let manifest: Manifest = serde_json::from_str(text).map_err(|e| e.to_string())?;
    if manifest.schema_version != 1
        || manifest.budgets.source_bytes != SOURCE_BUDGET
        || manifest.budgets.temporary_bytes != TEMP_BUDGET
        || manifest.budgets.parallel_cases != 1
        || manifest.scope.is_empty()
        || !manifest.provenance.is_object()
    {
        return Err("unsupported manifest version, scope or acquisition budget".into());
    }
    let advertised: BTreeSet<_> = super::EXTENSIONS.iter().copied().collect();
    let listed: BTreeSet<_> = manifest
        .families
        .iter()
        .map(|f| f.extension.as_str())
        .collect();
    if advertised != listed || listed.len() != manifest.families.len() || manifest.cases.is_empty()
    {
        return Err("inventory must cover every advertised extension exactly once".into());
    }
    let mut ids = BTreeSet::new();
    for case in &manifest.cases {
        if !safe_name(&case.id)
            || !safe_name(&case.source.filename)
            || !ids.insert(case.id.as_str())
            || !advertised.contains(case.extension.as_str())
            || Path::new(&case.source.filename)
                .extension()
                .and_then(|e| e.to_str())
                != Some(case.extension.as_str())
            || !valid_status(&case.status)
            || !valid_tolerance(case.preview_tolerance)
            || case.required_reference_kinds.is_empty()
        {
            return Err(format!("invalid or duplicate camera case {}", case.id));
        }
        let mut reference_ids = BTreeSet::new();
        for reference in &case.references {
            if !safe_name(&reference.id)
                || !reference_ids.insert(&reference.id)
                || !valid_tolerance(reference.tolerance)
                || !matches!(
                    reference.kind.as_str(),
                    "libraw_integration"
                        | "independent_render"
                        | "independent_sensor"
                        | "display_look"
                )
                || !matches!(
                    reference.representation.as_str(),
                    "oriented-linear-srgb-rgb16le"
                        | "display-srgb-rgba16le"
                        | "sensor-u16le-masked"
                )
                || reference.filename.as_ref().is_some_and(|s| !safe_name(s))
                || reference.sha256.as_ref().is_some_and(|s| !valid_hash(s))
                || reference
                    .toolchain
                    .executable_sha256
                    .as_ref()
                    .is_some_and(|s| !valid_hash(s))
                || reference
                    .toolchain
                    .archive_sha256
                    .as_ref()
                    .is_some_and(|s| !valid_hash(s))
                || reference.provenance.is_empty()
                || reference.toolchain.name.is_empty()
            {
                return Err(format!("invalid reference {} in {}", reference.id, case.id));
            }
            if (reference.kind == "independent_sensor")
                != (reference.representation == "sensor-u16le-masked")
            {
                return Err("sensor references require exactly the sensor representation".into());
            }
        }
        let required: BTreeSet<_> = case.required_reference_kinds.iter().collect();
        if required.len() != case.required_reference_kinds.len()
            || !required.iter().any(|k| k.as_str() == "libraw_integration")
            || !required
                .iter()
                .any(|k| matches!(k.as_str(), "independent_render" | "independent_sensor"))
            || required.iter().any(|kind| {
                kind.as_str() == "display_look" || !case.references.iter().any(|r| &r.kind == *kind)
            })
            || case.source.sha256.as_ref().is_some_and(|s| !valid_hash(s))
            || case.identity.bit_depth.is_some_and(|n| n == 0 || n > 32)
            || [
                case.identity.sensor_dimensions,
                case.identity.active_dimensions,
                case.identity.output_dimensions,
            ]
            .into_iter()
            .flatten()
            .any(|d| d.contains(&0))
        {
            return Err(format!(
                "invalid required evidence or identity for {}",
                case.id
            ));
        }
    }
    for family in &manifest.families {
        if family.family.is_empty()
            || !valid_status(&family.status)
            || family.mode_axes.is_empty()
            || family.unresolved_modes.is_empty()
            || family.evidence.is_empty()
            || family.candidate_cases.is_empty()
            || family.candidate_cases.iter().any(|id| {
                !manifest
                    .cases
                    .iter()
                    .any(|c| c.id == *id && c.extension == family.extension)
            })
        {
            return Err(format!("incomplete family inventory {}", family.extension));
        }
    }
    if manifest.cases.iter().any(|case| {
        !manifest
            .families
            .iter()
            .any(|f| f.candidate_cases.contains(&case.id))
    }) {
        return Err("camera case missing from its family inventory".into());
    }
    Ok(manifest)
}

/// Recognize evidence states without claiming camera support.
/// Args: state. Returns whether the manifest state is known.
fn valid_status(status: &str) -> bool {
    matches!(
        status,
        "not_yet_qualified" | "fixture_unavailable" | "needs_resource_scheduling"
    )
}

/// Validate a normalized error budget.
/// Args: mean and maximum bounds. Returns true for finite ordered bounds in 0..1.
fn valid_tolerance(t: Tolerance) -> bool {
    t.mean.is_finite() && t.max.is_finite() && t.mean >= 0.0 && t.max >= t.mean && t.max <= 1.0
}

/// Restrict identifiers and corpus filenames to one path component.
/// Args: name. Returns true for a nonhidden ASCII basename with no path traversal.
fn safe_name(name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with('.')
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
}

/// Recognize a SHA-256 identity.
/// Args: hexadecimal digest. Returns true for exactly 64 lowercase hexadecimal digits.
fn valid_hash(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// Hash a regular file without buffering it in memory.
/// Args: path. Returns the source SHA-256 or an I/O error.
fn hash_file(path: &Path) -> Result<String, String> {
    let mut file = File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    if !file.metadata().map_err(|e| e.to_string())?.is_file() {
        return Err(format!("{} is not a regular file", path.display()));
    }
    let mut hash = Sha256::new();
    let mut block = [0u8; 256 * 1024];
    loop {
        let count = file.read(&mut block).map_err(|e| e.to_string())?;
        if count == 0 {
            break;
        }
        hash.update(&block[..count]);
    }
    Ok(format!("{:x}", hash.finalize()))
}

/// Compare a file with an externally pinned identity.
/// Args: file and expected digest. Returns the observed hash, never a new reference pin.
fn verify_hash(path: &Path, expected: Option<&str>) -> Result<String, String> {
    let expected = expected
        .filter(|s| valid_hash(s))
        .ok_or("missing pinned SHA-256")?;
    let actual = hash_file(path)?;
    if actual != expected {
        return Err(format!(
            "SHA-256 mismatch for {}: expected {expected}, observed {actual}",
            path.display()
        ));
    }
    Ok(actual)
}

/// Restrict disposable files to a newly owned project directory.
/// Args: requested path. Returns a new canonical directory, refusing existing paths and symlink parents.
fn create_scratch(path: &Path) -> Result<PathBuf, String> {
    if !path.is_absolute()
        || !path.starts_with("/home")
        || path.components().any(|c| matches!(c, Component::ParentDir))
    {
        return Err("qualification output must remain under /home without traversal".into());
    }
    let parent = path.parent().ok_or("missing output parent")?;
    if parent.canonicalize().map_err(|e| e.to_string())? != parent {
        return Err("qualification output parent must be canonical".into());
    }
    fs::create_dir(path).map_err(|e| {
        format!(
            "refusing existing/unavailable output {}: {e}",
            path.display()
        )
    })?;
    path.canonicalize().map_err(|e| e.to_string())
}

/// Write one new receipt without overwriting user files.
/// Args: new receipt path and serializable value. Returns success or the I/O error.
fn write_json(path: &Path, value: &Value) -> Result<(), String> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|e| e.to_string())?;
    file.write_all(
        serde_json::to_string(value)
            .map_err(|e| e.to_string())?
            .as_bytes(),
    )
    .map_err(|e| e.to_string())
}

/// Identify the producing host without publishing its hostname.
/// Args: none. Returns SHA-256 of the trimmed system hostname.
fn host_identity() -> Result<String, String> {
    let name = fs::read_to_string("/proc/sys/kernel/hostname")
        .or_else(|_| {
            let output = std::process::Command::new("hostname").output()?;
            if !output.status.success() {
                return Err(std::io::Error::other("Host identity unavailable"));
            }
            String::from_utf8(output.stdout).map_err(std::io::Error::other)
        })
        .map_err(|_| "Could not identify qualification host")?;
    let name = name.trim();
    if name.is_empty() {
        return Err("Qualification hostname is empty".into());
    }
    Ok(format!("{:x}", Sha256::digest(name.as_bytes())))
}

#[test]
#[ignore = "real camera corpus; use serialized-cargo with pinned runner and corpus environment"]
fn raw_camera_corpus() {
    let manifest_path = std::env::var_os("OMA_RAW_MANIFEST").map(PathBuf::from);
    let text = manifest_path
        .as_ref()
        .map(|p| fs::read_to_string(p).expect("read manifest"))
        .unwrap_or_else(|| MANIFEST.into());
    let manifest = parse_manifest(&text).expect("valid camera manifest");
    let mode = std::env::var("OMA_RAW_MODE").unwrap_or_else(|_| "required".into());
    assert!(
        matches!(mode.as_str(), "required" | "optional"),
        "unknown qualification mode"
    );
    let corpus = PathBuf::from(std::env::var_os("OMA_RAW_CORPUS").expect("OMA_RAW_CORPUS"));
    let output = PathBuf::from(std::env::var_os("OMA_RAW_OUTPUT").expect("OMA_RAW_OUTPUT"));
    let output = create_scratch(&output).expect("new owned output directory");
    let selection = std::env::var("OMA_RAW_CASE").ok();
    let cases: Vec<_> = manifest
        .cases
        .iter()
        .filter(|c| selection.as_ref().is_none_or(|id| &c.id == id))
        .collect();
    assert!(!cases.is_empty(), "zero camera cases selected");
    assert!(
        selection.is_some() || std::env::var_os("OMA_RAW_CASE_BINDINGS").is_none(),
        "per-case bindings require one case"
    );
    let bindings: Bindings = serde_json::from_str(
        &std::env::var("OMA_RAW_CASE_BINDINGS").unwrap_or_else(|_| "{}".into()),
    )
    .expect("valid case bindings");
    let runner = std::env::current_exe().expect("runner path");
    let runner_hash = verify_hash(
        &runner,
        std::env::var("OMA_RAW_RUNNER_SHA256").ok().as_deref(),
    )
    .expect("pinned test executable");
    let revision = std::env::var("OMA_RAW_REVISION").expect("OMA_RAW_REVISION");
    assert!(
        revision.len() == 40 && revision.bytes().all(|b| b.is_ascii_hexdigit()),
        "full source revision"
    );
    let rustc = std::process::Command::new("rustc")
        .args(["--version", "--verbose"])
        .output()
        .expect("rustc identity");
    assert!(rustc.status.success(), "rustc identity failed");
    let mut reports = Vec::new();
    let host_sha256 = host_identity().expect("hashed producer host identity");
    for case in cases {
        let directory = output.join(&case.id);
        fs::create_dir(&directory).expect("new case directory");
        let mut report = workflow::run(case, &bindings, &corpus, &directory, &mode);
        report["manifest_sha256"] = json!(format!("{:x}", Sha256::digest(text.as_bytes())));
        report["runner"] = json!({"sha256":runner_hash,"revision":revision,"host_sha256":host_sha256,"decoder":super::VERSION,
            "rustc":String::from_utf8_lossy(&rustc.stdout),"os":std::env::consts::OS,"arch":std::env::consts::ARCH,
            "cargo_lock_sha256":hash_file(&Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.lock")).expect("lock identity")});
        write_json(&directory.join("result.json"), &report).expect("case receipt");
        println!("RAW_QUALIFICATION {}", report);
        reports.push(report);
    }
    let checked = reports.iter().filter(|r| r["checked"] == true).count();
    let skipped = reports.iter().filter(|r| r["status"] == "skipped").count();
    let failed = reports.iter().filter(|r| r["status"] == "failed").count();
    let summary = json!({"schema_version":1,"mode":mode,"selected":reports.len(),"checked":checked,"skipped":skipped,"failed":failed});
    write_json(&output.join("summary.json"), &summary).expect("aggregate receipt");
    println!("RAW_QUALIFICATION_SUMMARY {summary}");
    assert_eq!(
        failed, 0,
        "camera qualification failures; see aggregate and case receipts"
    );
    assert!(
        mode == "optional" || (checked > 0 && skipped == 0),
        "required qualification cannot skip or compare zero cases"
    );
}
