//! Portable named Photo looks. Presets never contain source paths or pixels.
pub use super::transfer::AdjustmentSnapshot as Preset;
use serde::{Deserialize, Serialize};
use std::{
    io::Read,
    path::{Path, PathBuf},
};

const MAX_BYTES: u64 = 2 * 1024 * 1024;
pub const MAX_PRESETS: usize = 256;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Collection {
    version: u32,
    presets: Vec<Preset>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SinglePreset {
    version: u32,
    name: String,
    params: super::DevelopParams,
    #[serde(default)]
    categories: super::transfer::Categories,
}

pub fn library_path() -> PathBuf {
    crate::project::data_dir().join("photo-presets.omapreset")
}

pub fn decode(bytes: &[u8]) -> Result<Vec<Preset>, String> {
    if bytes.len() as u64 > MAX_BYTES {
        return Err("Photo presets exceed the 2 MiB limit.".into());
    }
    let value: serde_json::Value =
        serde_json::from_slice(bytes).map_err(|e| format!("Could not read Photo presets: {e}"))?;
    let collection = if value.get("presets").is_some() {
        serde_json::from_value::<Collection>(value)
            .map_err(|e| format!("Could not read Photo presets: {e}"))?
    } else {
        let single: SinglePreset = serde_json::from_value(value)
            .map_err(|e| format!("Could not read Photo preset: {e}"))?;
        Collection {
            version: single.version,
            presets: vec![Preset {
                name: single.name,
                params: single.params,
                categories: single.categories,
            }],
        }
    };
    if collection.version != 1 {
        return Err("This Photo preset version is not supported.".into());
    }
    validate(&collection.presets)?;
    Ok(collection.presets)
}

pub fn encode(presets: &[Preset]) -> Result<Vec<u8>, String> {
    validate(presets)?;
    let bytes = serde_json::to_vec_pretty(&Collection {
        version: 1,
        presets: presets.to_vec(),
    })
    .map_err(|e| e.to_string())?;
    if bytes.len() as u64 > MAX_BYTES {
        return Err("Photo presets exceed the 2 MiB limit.".into());
    }
    Ok(bytes)
}

fn validate(presets: &[Preset]) -> Result<(), String> {
    if presets.len() > MAX_PRESETS {
        return Err(format!(
            "Keep at most {MAX_PRESETS} Photo presets in one collection."
        ));
    }
    let mut names = std::collections::HashSet::new();
    for preset in presets {
        preset.validate()?;
        if !names.insert(preset.name.trim().to_lowercase()) {
            return Err(format!(
                "The preset name {:?} appears more than once.",
                preset.name
            ));
        }
    }
    Ok(())
}

pub fn load_file(path: &Path) -> Result<Vec<Preset>, String> {
    let file =
        std::fs::File::open(path).map_err(|e| format!("Could not open Photo presets: {e}"))?;
    if !file.metadata().map_err(|e| e.to_string())?.is_file() {
        return Err("Choose a regular .omapreset file.".into());
    }
    let mut bytes = Vec::new();
    file.take(MAX_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    decode(&bytes)
}

pub fn save_file(path: &Path, presets: &[Preset]) -> Result<(), String> {
    let bytes = encode(presets)?;
    crate::formats::write_atomic(path, &bytes)
        .map_err(|e| format!("Could not save Photo presets: {e}"))
}

/// Keep both named looks on import; never overwrite an existing preset silently.
pub fn merge(current: &[Preset], incoming: &[Preset]) -> Result<Vec<Preset>, String> {
    let mut result = current.to_vec();
    for preset in incoming {
        if result.iter().any(|p| p == preset) {
            continue;
        }
        let mut preset = preset.clone();
        let base = preset.name.trim().to_string();
        let mut suffix = 2;
        while result
            .iter()
            .any(|p| p.name.eq_ignore_ascii_case(&preset.name))
        {
            preset.name = format!("{base} ({suffix})");
            suffix += 1;
        }
        result.push(preset);
    }
    validate(&result)?;
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::photo::{DevelopParams, transfer::Categories};

    #[test]
    fn portable_presets_roundtrip_categories_and_merge_without_overwriting_names() {
        let preset = Preset {
            name: "Evening".into(),
            params: DevelopParams {
                exposure: -0.5,
                rotate: 90,
                crop: Some([0.1, 0.2, 0.8, 0.9]),
                ..Default::default()
            },
            categories: Categories::default(),
        };
        assert_eq!(
            decode(&encode(std::slice::from_ref(&preset)).unwrap()).unwrap(),
            vec![preset.clone()]
        );
        let single = serde_json::json!({"version":1,"name":"Evening","params":preset.params});
        assert_eq!(
            decode(&serde_json::to_vec(&single).unwrap()).unwrap(),
            vec![preset.clone()]
        );
        let mut other = preset.clone();
        other.params.exposure = 1.5;
        let merged = merge(std::slice::from_ref(&preset), &[preset.clone(), other]).unwrap();
        assert_eq!(merged.len(), 2);
        assert_eq!(merged[0], preset);
        assert_eq!(merged[1].name, "Evening (2)");
        assert!(decode(br#"{"version":90,"presets":[]}"#).is_err());
        let mut invalid = preset;
        invalid.params.crop = Some([0.8, 0.1, 0.2, 0.9]);
        assert!(encode(&[invalid]).is_err());
        assert!(decode(&vec![b' '; MAX_BYTES as usize + 1]).is_err());
    }
}
