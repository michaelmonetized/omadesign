//! Custom colour palettes persisted as JSON.

use crate::color::Rgba;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Palette {
    pub name: String,
    pub colors: Vec<Rgba>,
}

impl Palette {
    pub fn new(name: impl Into<String>, colors: Vec<Rgba>) -> Self {
        Self {
            name: name.into(),
            colors,
        }
    }
}

pub fn path() -> PathBuf {
    // XDG config home, fallback to ~/.config
    if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME") {
        if !xdg.is_empty() {
            return PathBuf::from(format!("{xdg}/omadesign/palettes.json"));
        }
    }
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
    PathBuf::from(format!("{home}/.config/omadesign/palettes.json"))
}

pub fn fallback_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
    PathBuf::from(format!("{home}/.local/share/omadesign/palettes.json"))
}

pub fn load() -> Vec<Palette> {
    for p in [path(), fallback_path()] {
        if let Ok(s) = std::fs::read_to_string(&p) {
            if let Ok(v) = serde_json::from_str::<Vec<Palette>>(&s) {
                if !v.is_empty() {
                    return v;
                }
            }
        }
    }
    vec![Palette::new("Oma Default", crate::color::default_swatches())]
}

pub fn save(palettes: &[Palette]) -> Result<(), String> {
    let p = path();
    if let Some(parent) = p.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let s = serde_json::to_string_pretty(palettes).map_err(|e| e.to_string())?;
    // Atomic write: write to temp then rename
    let tmp = p.with_extension("json.tmp");
    std::fs::write(&tmp, s).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, &p).map_err(|e| e.to_string())?;
    Ok(())
}

pub fn validate_name(name: &str) -> Result<(), String> {
    let t = name.trim();
    if t.is_empty() {
        return Err("name may not be empty".into());
    }
    if t.len() > 64 {
        return Err("name too long (max 64)".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let palettes = vec![
            Palette::new("Brand", vec![Rgba::from_hex(0xFF0000), Rgba::from_hex(0x00FF00)]),
            Palette::new("Mono", vec![Rgba::BLACK, Rgba::WHITE]),
        ];
        let s = serde_json::to_string(&palettes).unwrap();
        let back: Vec<Palette> = serde_json::from_str(&s).unwrap();
        assert_eq!(back, palettes);
    }

    #[test]
    fn validate_rejects_empty() {
        assert!(validate_name("").is_err());
        assert!(validate_name("  ").is_err());
        assert!(validate_name("Brand").is_ok());
    }

    #[test]
    fn fallback_is_default() {
        // Ensure default_swatches produces Oma Default when no file exists;
        // we test that load() at least returns one palette.
        let p = load();
        assert!(!p.is_empty());
        assert!(!p[0].colors.is_empty());
    }
}
