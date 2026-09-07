//! Portable development settings kept beside an untouched source photograph.
use super::DevelopParams;
use serde::{Deserialize, Serialize};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

const VERSION: u32 = 1;
const MAX_SETTINGS_BYTES: u64 = 64 * 1024;

/// Bind adjustments to the source that was actually decoded. The file's size
/// and nanosecond modification time detect replacement without hashing large
/// photographs again every time settings are saved.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceIdentity {
    pub size_bytes: u64,
    pub modified_seconds: u64,
    pub modified_nanos: u32,
}

impl SourceIdentity {
    pub fn read(path: &Path) -> Result<Self, String> {
        let metadata = std::fs::metadata(path)
            .map_err(|error| format!("Could not inspect the source photograph: {error}"))?;
        if !metadata.is_file() {
            return Err("Choose a regular photograph file.".into());
        }
        let modified = metadata
            .modified()
            .map_err(|error| format!("Could not inspect the source modification time: {error}"))?
            .duration_since(UNIX_EPOCH)
            .map_err(|_| "The source modification time is invalid.")?;
        Ok(Self {
            size_bytes: metadata.len(),
            modified_seconds: modified.as_secs(),
            modified_nanos: modified.subsec_nanos(),
        })
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Settings {
    version: u32,
    source: SourceIdentity,
    develop: DevelopParams,
}

pub fn sidecar_path(source: &Path) -> PathBuf {
    let mut path = source.as_os_str().to_owned();
    path.push(".omaphoto");
    PathBuf::from(path)
}

pub fn is_sidecar(path: &Path) -> bool {
    path.extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("omaphoto"))
}

/// Version 1 sidecars are named after their original, including its extension.
/// Resolve beside the chosen settings file so moving the pair needs no path edits.
pub fn source_path(settings: &Path) -> Result<PathBuf, String> {
    if !is_sidecar(settings) {
        return Err("Choose a .omaphoto settings file.".into());
    }
    let source = settings.with_extension("");
    if !source.is_file() {
        return Err(format!(
            "These settings need the original photo, {}. Keep both files together in the same folder.",
            source.file_name().unwrap_or_default().to_string_lossy()
        ));
    }
    Ok(source)
}

pub fn load(source: &Path, identity: &SourceIdentity) -> Result<Option<DevelopParams>, String> {
    let path = sidecar_path(source);
    match std::fs::metadata(&path) {
        Ok(_) => load_file(&path, identity).map(Some),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(format!("Could not read photo settings: {error}")),
    }
}

/// Opening settings explicitly must not silently substitute default development.
pub fn load_file(path: &Path, identity: &SourceIdentity) -> Result<DevelopParams, String> {
    let metadata = std::fs::metadata(path)
        .map_err(|error| format!("Could not read photo settings: {error}"))?;
    if !metadata.is_file() {
        return Err("Choose a regular .omaphoto settings file.".into());
    }
    if metadata.len() > MAX_SETTINGS_BYTES {
        return Err("Photo settings exceed the 64 KiB limit.".into());
    }
    let file = std::fs::File::open(path)
        .map_err(|error| format!("Could not read photo settings: {error}"))?;
    let mut bytes = Vec::new();
    file.take(MAX_SETTINGS_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("Could not read photo settings: {error}"))?;
    if bytes.len() as u64 > MAX_SETTINGS_BYTES {
        return Err("Photo settings exceed the 64 KiB limit.".into());
    }
    let settings: Settings = serde_json::from_slice(&bytes)
        .map_err(|error| format!("Could not read photo settings: {error}"))?;
    if settings.version != VERSION {
        return Err(format!(
            "Photo settings version {} is not supported.",
            settings.version
        ));
    }
    if &settings.source != identity {
        return Err("Saved photo settings belong to an older or different source. Open the matching original photo with its settings.".into());
    }
    validate(&settings.develop)?;
    Ok(settings.develop)
}

pub fn save(
    source: &Path,
    identity: &SourceIdentity,
    develop: &DevelopParams,
) -> Result<PathBuf, String> {
    save_to(&sidecar_path(source), source, identity, develop)
}

pub fn save_to(
    path: &Path,
    source: &Path,
    identity: &SourceIdentity,
    develop: &DevelopParams,
) -> Result<PathBuf, String> {
    validate(develop)?;
    if !is_sidecar(path) || source_path(path)?.canonicalize().ok() != source.canonicalize().ok() {
        return Err("Save settings beside the matching original photograph.".into());
    }
    if &SourceIdentity::read(source)? != identity {
        return Err(
            "The source photograph changed after it was opened. Reopen it before saving settings."
                .into(),
        );
    }
    let bytes = serde_json::to_vec_pretty(&Settings {
        version: VERSION,
        source: identity.clone(),
        develop: develop.clone(),
    })
    .map_err(|error| format!("Could not encode photo settings: {error}"))?;
    crate::formats::write_atomic(path, &bytes)
        .map_err(|error| format!("Could not save photo settings: {error}"))?;
    Ok(path.to_owned())
}

/// Sidecars are editable JSON; reject invalid geometry and nonfinite or extreme
/// adjustment values before they can reach the development pipeline.
pub fn validate(p: &DevelopParams) -> Result<(), String> {
    let within =
        |value: f32, low: f32, high: f32| value.is_finite() && (low..=high).contains(&value);
    let signed = [
        p.highlights,
        p.shadows,
        p.whites,
        p.blacks,
        p.clarity,
        p.vignette,
        p.dehaze,
        p.split_balance,
    ];
    let valid = within(p.exposure, -16.0, 16.0)
        && within(p.temperature, -100.0, 100.0)
        && within(p.tint, -100.0, 100.0)
        && within(p.contrast, 0.0, 4.0)
        && within(p.saturation, 0.0, 2.0)
        && within(p.vibrance, 0.0, 2.0)
        && within(p.grain, 0.0, 1.0)
        && within(p.hue, -180.0, 180.0)
        && signed.iter().all(|v| within(*v, -1.0, 1.0))
        && p.split_shadow
            .iter()
            .chain(p.split_highlight.iter())
            .all(|v| within(*v, -1.0, 1.0))
        && p.curve.iter().all(|v| within(*v, 0.0, 1.0))
        && p.hsl.iter().all(|band| {
            within(band.hue, -180.0, 180.0)
                && within(band.sat, -1.0, 1.0)
                && within(band.luma, -1.0, 1.0)
        })
        && matches!(p.rotate, 0 | 90 | 180 | 270)
        && p.crop.is_none_or(|crop| {
            crop.iter().all(|v| within(*v, 0.0, 1.0)) && crop[0] < crop[2] && crop[1] < crop[3]
        });
    if valid {
        Ok(())
    } else {
        Err("Photo settings contain invalid adjustment or crop values.".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "omadesign-photo-settings-{}-{}",
                std::process::id(),
                crate::document::next_id()
            ));
            std::fs::create_dir(&path).unwrap();
            Self(path)
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn settings_roundtrip_preserves_original_and_rejects_replaced_source() {
        let fixture = Fixture::new();
        let source = fixture.0.join("Photograph.NEF");
        let original = b"original sensor data";
        std::fs::write(&source, original).unwrap();
        let identity = SourceIdentity::read(&source).unwrap();
        assert!(load(&source, &identity).unwrap().is_none());
        let develop = DevelopParams {
            exposure: -1.5,
            rotate: 90,
            crop: Some([0.1, 0.2, 0.9, 0.8]),
            ..Default::default()
        };
        assert_eq!(
            save(&source, &identity, &develop).unwrap(),
            fixture.0.join("Photograph.NEF.omaphoto")
        );
        assert_eq!(load(&source, &identity).unwrap(), Some(develop.clone()));
        assert_eq!(std::fs::read(&source).unwrap(), original);
        std::fs::write(&source, b"a different photo").unwrap();
        assert!(
            load(&source, &SourceIdentity::read(&source).unwrap())
                .unwrap_err()
                .contains("different source")
        );
        assert!(
            save(&source, &identity, &develop)
                .unwrap_err()
                .contains("changed")
        );
    }

    #[test]
    fn settings_reject_invalid_crop_nonfinite_and_oversized_payloads() {
        let fixture = Fixture::new();
        let source = fixture.0.join("photo.dng");
        std::fs::write(&source, b"source").unwrap();
        let identity = SourceIdentity::read(&source).unwrap();
        let mut develop = DevelopParams {
            crop: Some([0.8, 0.0, 0.2, 1.0]),
            ..Default::default()
        };
        assert!(save(&source, &identity, &develop).is_err());
        develop.crop = None;
        develop.exposure = f32::NAN;
        assert!(validate(&develop).is_err());
        std::fs::write(
            sidecar_path(&source),
            vec![b' '; MAX_SETTINGS_BYTES as usize + 1],
        )
        .unwrap();
        assert!(load(&source, &identity).unwrap_err().contains("64 KiB"));
    }
}
