//! Portable colour libraries. `.omacolors` stores named palettes with hex swatches;
//! legacy RGBA-object JSON remains readable and migrates on the next save.

use crate::color::Rgba;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashSet;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

pub const EXTENSION: &str = "omacolors";
pub const MAX_FILE_BYTES: usize = 4 * 1024 * 1024;
pub const MAX_PALETTES: usize = 256;
pub const MAX_COLORS_PER_PALETTE: usize = 4096;
pub const MAX_TOTAL_COLORS: usize = 65536;
const MAX_NAME_CHARS: usize = 64;
static TEMP_ID: AtomicU64 = AtomicU64::new(0);

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

#[derive(Serialize)]
struct PortableFile {
    version: u8,
    palettes: Vec<PortablePalette>,
}
#[derive(Serialize)]
struct PortablePalette {
    name: String,
    colors: Vec<String>,
}

/// Encode a versioned portable library. Opaque colours use #RRGGBB and all other
/// colours use #RRGGBBAA, including the RGB channels of fully transparent colours.
pub fn encode(palettes: &[Palette]) -> Result<String, String> {
    validate(palettes)?;
    let mut palettes = palettes.to_vec();
    unique_names(&mut palettes, &[]);
    let file = PortableFile {
        version: 1,
        palettes: palettes
            .into_iter()
            .map(|palette| PortablePalette {
                name: palette.name,
                colors: palette.colors.into_iter().map(Rgba::hex).collect(),
            })
            .collect(),
    };
    let mut json = serde_json::to_string_pretty(&file).map_err(|error| error.to_string())?;
    json.push('\n');
    if json.len() > MAX_FILE_BYTES {
        return Err("Palette file exceeds the 4 MiB limit".into());
    }
    Ok(json)
}

/// Accept a versioned library, an array of named palettes, one named palette, or
/// a simple array of hex strings (named "Imported"). Empty palettes are valid.
/// Legacy {r,g,b,a} swatches are accepted alongside hex strings without conversion
/// through premultiplied colour. Duplicate names are disambiguated, never merged.
pub fn decode(json: &str) -> Result<Vec<Palette>, String> {
    if json.len() > MAX_FILE_BYTES {
        return Err("Palette file exceeds the 4 MiB limit".into());
    }
    let value: Value = serde_json::from_str(json.trim_start_matches('\u{feff}'))
        .map_err(|error| format!("Invalid palette JSON: {error}"))?;
    let mut palettes = match &value {
        Value::Object(object) if object.contains_key("palettes") => {
            if let Some(version) = object.get("version")
                && version.as_u64() != Some(1)
            {
                return Err("Unsupported palette version; this app reads version 1".into());
            }
            parse_palettes(object.get("palettes").unwrap())?
        }
        Value::Object(_) => vec![parse_palette(&value, 0)?],
        Value::Array(values) if !values.is_empty() && values.iter().all(Value::is_string) => {
            vec![Palette::new("Imported", parse_colors(&value, "Imported")?)]
        }
        Value::Array(_) => parse_palettes(&value)?,
        _ => {
            return Err("Expected a palette, a palette library, or an array of hex colors".into());
        }
    };
    validate(&palettes)?;
    unique_names(&mut palettes, &[]);
    Ok(palettes)
}

fn parse_palettes(value: &Value) -> Result<Vec<Palette>, String> {
    let entries = value
        .as_array()
        .ok_or("The palettes field must be an array")?;
    if entries.len() > MAX_PALETTES {
        return Err(format!(
            "A library can contain at most {MAX_PALETTES} palettes"
        ));
    }
    entries
        .iter()
        .enumerate()
        .map(|(index, value)| parse_palette(value, index))
        .collect()
}

fn parse_palette(value: &Value, index: usize) -> Result<Palette, String> {
    let name = value
        .get("name")
        .and_then(Value::as_str)
        .ok_or_else(|| format!("Palette {} needs a name", index + 1))?;
    validate_name(name).map_err(|error| format!("Palette {}: {error}", index + 1))?;
    let colors = value
        .get("colors")
        .ok_or_else(|| format!("Palette {name:?} needs a colors array"))?;
    Ok(Palette::new(name.trim(), parse_colors(colors, name)?))
}

fn parse_colors(value: &Value, name: &str) -> Result<Vec<Rgba>, String> {
    let colors = value
        .as_array()
        .ok_or_else(|| format!("Palette {name:?}: colors must be an array"))?;
    if colors.len() > MAX_COLORS_PER_PALETTE {
        return Err(format!(
            "Palette {name:?} exceeds {MAX_COLORS_PER_PALETTE} colors"
        ));
    }
    colors
        .iter()
        .enumerate()
        .map(|(index, value)| {
            let color = match value {
                Value::String(hex) => {
                    let hex = hex.trim().strip_prefix('#').unwrap_or(hex.trim());
                    (matches!(hex.len(), 3 | 6 | 8)
                        && hex.bytes().all(|byte| byte.is_ascii_hexdigit()))
                    .then(|| Rgba::parse_hex(hex))
                    .flatten()
                }
                Value::Object(_) => serde_json::from_value::<Rgba>(value.clone()).ok(),
                _ => None,
            };
            color.ok_or_else(|| {
                format!(
                    "Palette {name:?}, color {}: use #RRGGBB or #RRGGBBAA",
                    index + 1
                )
            })
        })
        .collect()
}

pub fn validate_name(name: &str) -> Result<(), String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("name may not be empty".into());
    }
    if name.chars().count() > MAX_NAME_CHARS {
        return Err("name too long (max 64 characters)".into());
    }
    if name.chars().any(char::is_control) {
        return Err("name may not contain control characters".into());
    }
    Ok(())
}

fn validate(palettes: &[Palette]) -> Result<(), String> {
    if palettes.len() > MAX_PALETTES {
        return Err(format!(
            "A library can contain at most {MAX_PALETTES} palettes"
        ));
    }
    let mut total = 0;
    for palette in palettes {
        validate_name(&palette.name)?;
        if palette.colors.len() > MAX_COLORS_PER_PALETTE {
            return Err(format!(
                "Palette {:?} exceeds {MAX_COLORS_PER_PALETTE} colors",
                palette.name
            ));
        }
        total += palette.colors.len();
    }
    if total > MAX_TOTAL_COLORS {
        return Err(format!(
            "A library can contain at most {MAX_TOTAL_COLORS} colors"
        ));
    }
    Ok(())
}

fn name_key(name: &str) -> String {
    name.trim().to_lowercase()
}

fn unique_names(palettes: &mut [Palette], existing: &[Palette]) {
    let mut used: HashSet<_> = existing
        .iter()
        .map(|palette| name_key(&palette.name))
        .collect();
    // Reserve explicit names, so a duplicate "Brand" never takes a later palette's
    // deliberately chosen "Brand (2)" name.
    let reserved: HashSet<_> = palettes
        .iter()
        .map(|palette| name_key(&palette.name))
        .collect();
    for palette in palettes {
        palette.name = palette.name.trim().to_owned();
        if used.insert(name_key(&palette.name)) {
            continue;
        }
        for number in 2.. {
            let suffix = format!(" ({number})");
            let stem: String = palette
                .name
                .chars()
                .take(MAX_NAME_CHARS - suffix.len())
                .collect();
            let candidate = format!("{}{suffix}", stem.trim_end());
            let key = name_key(&candidate);
            if !reserved.contains(&key) && used.insert(key) {
                palette.name = candidate;
                break;
            }
        }
    }
}

/// Append an imported library atomically in memory. Names are matched without
/// case or surrounding whitespace; collisions receive (2), (3), … suffixes.
/// Existing palettes and duplicate swatch values remain untouched.
pub fn merge(existing: &mut Vec<Palette>, mut incoming: Vec<Palette>) -> Result<usize, String> {
    validate(existing)?;
    validate(&incoming)?;
    if existing.len() + incoming.len() > MAX_PALETTES {
        return Err(format!(
            "A library can contain at most {MAX_PALETTES} palettes"
        ));
    }
    let total: usize = existing
        .iter()
        .chain(&incoming)
        .map(|palette| palette.colors.len())
        .sum();
    if total > MAX_TOTAL_COLORS {
        return Err(format!(
            "A library can contain at most {MAX_TOTAL_COLORS} colors"
        ));
    }
    unique_names(&mut incoming, existing);
    let count = incoming.len();
    existing.extend(incoming);
    Ok(count)
}

pub fn load_file(path: impl AsRef<Path>) -> Result<Vec<Palette>, String> {
    let path = path.as_ref();
    let read = || -> Result<String, String> {
        let metadata = fs::metadata(path).map_err(|error| error.to_string())?;
        if !metadata.is_file() {
            return Err("choose a regular palette file".into());
        }
        if metadata.len() > MAX_FILE_BYTES as u64 {
            return Err("palette file exceeds the 4 MiB limit".into());
        }
        let mut json = String::new();
        File::open(path)
            .map_err(|error| error.to_string())?
            .take(MAX_FILE_BYTES as u64 + 1)
            .read_to_string(&mut json)
            .map_err(|error| error.to_string())?;
        Ok(json)
    };
    let json = read().map_err(|error| format!("Could not read {}: {error}", path.display()))?;
    decode(&json).map_err(|error| format!("{}: {error}", path.display()))
}

/// Validate and serialize before touching disk, then replace the destination
/// with one same-directory rename. Concurrent saves never share a temp file.
pub fn save_file(path: impl AsRef<Path>, palettes: &[Palette]) -> Result<(), String> {
    let json = encode(palettes)?;
    let path = path.as_ref();
    let name = path.file_name().ok_or("Choose a palette filename")?;
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    fs::create_dir_all(parent)
        .map_err(|error| format!("Could not create {}: {error}", parent.display()))?;
    let mut temporary = None;
    for _ in 0..128 {
        let mut temp_name = std::ffi::OsString::from(".");
        temp_name.push(name);
        temp_name.push(format!(
            ".{}-{}.tmp",
            std::process::id(),
            TEMP_ID.fetch_add(1, Ordering::Relaxed)
        ));
        let temp_path = parent.join(temp_name);
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp_path)
        {
            Ok(file) => {
                temporary = Some((temp_path, file));
                break;
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(format!("Could not save {}: {error}", path.display())),
        }
    }
    let (temp_path, mut file) =
        temporary.ok_or("Could not create a unique temporary palette file")?;
    let written = file
        .write_all(json.as_bytes())
        .and_then(|_| file.sync_all());
    drop(file);
    let result = written.and_then(|_| fs::rename(&temp_path, path));
    if result.is_err() {
        let _ = fs::remove_file(&temp_path);
    }
    result.map_err(|error| format!("Could not save {}: {error}", path.display()))?;
    #[cfg(unix)]
    File::open(parent)
        .and_then(|file| file.sync_all())
        .map_err(|error| {
            format!(
                "Saved {}, but could not sync its directory: {error}",
                path.display()
            )
        })?;
    Ok(())
}

pub fn path() -> PathBuf {
    if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME")
        && !xdg.is_empty()
    {
        return PathBuf::from(xdg).join("omadesign/palettes.json");
    }
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/tmp"));
    home.join(".config/omadesign/palettes.json")
}

pub fn fallback_path() -> PathBuf {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/tmp"));
    home.join(".local/share/omadesign/palettes.json")
}

pub fn load() -> Vec<Palette> {
    for path in [path(), fallback_path()] {
        if let Ok(palettes) = load_file(path)
            && !palettes.is_empty()
        {
            return palettes;
        }
    }
    vec![Palette::new(
        "Oma Default",
        crate::color::default_swatches(),
    )]
}

pub fn save(palettes: &[Palette]) -> Result<(), String> {
    save_file(path(), palettes)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TempDir(PathBuf);
    impl TempDir {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "omadesign-palette-test-{}-{}",
                std::process::id(),
                TEMP_ID.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&path).unwrap();
            Self(path)
        }
    }
    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn portable_file_roundtrip_preserves_alpha_order_empty_palettes_and_repeated_swatches() {
        let directory = TempDir::new();
        let path = directory.0.join("brand.omacolors");
        let translucent = Rgba::new(12, 34, 56, 128);
        let palettes = vec![
            Palette::new(
                "Brand",
                vec![
                    Rgba::WHITE,
                    translucent,
                    Rgba::new(200, 50, 100, 0),
                    translucent,
                ],
            ),
            Palette::new("Ready for ideas", vec![]),
        ];
        save_file(&path, &palettes).unwrap();
        assert_eq!(load_file(&path).unwrap(), palettes);
        let json = fs::read_to_string(&path).unwrap();
        assert!(json.contains("#0C223880") && json.contains("#C8326400"));
        assert_eq!(
            serde_json::from_str::<Value>(&json).unwrap()["palettes"][0]["colors"][0],
            "#FFFFFF"
        );
        assert_eq!(
            fs::read_dir(&directory.0).unwrap().count(),
            1,
            "no staging files remain"
        );
    }

    #[test]
    fn handwritten_formats_and_legacy_files_migrate_without_losing_colours() {
        let expected = vec![Palette::new(
            "Brand",
            vec![Rgba::new(17, 34, 51, 68), Rgba::WHITE],
        )];
        for json in [
            r##"{"name":"Brand","colors":["#11223344","fff"]}"##.to_owned(),
            r##"[{"name":"Brand","colors":["11223344","#FFF"]}]"##.to_owned(),
            serde_json::to_string(&expected).unwrap(),
        ] {
            assert_eq!(decode(&json).unwrap(), expected);
        }
        assert_eq!(
            decode(r##"["#11223344","fff"]"##).unwrap()[0].colors,
            expected[0].colors
        );
        let directory = TempDir::new();
        let path = directory.0.join("palettes.json");
        fs::write(&path, serde_json::to_vec(&expected).unwrap()).unwrap();
        let migrated = load_file(&path).unwrap();
        save_file(&path, &migrated).unwrap();
        assert_eq!(load_file(&path).unwrap(), expected);
        assert_eq!(
            serde_json::from_str::<Value>(&fs::read_to_string(path).unwrap()).unwrap()["version"],
            1
        );
    }

    #[test]
    fn importing_colliding_names_preserves_existing_palettes_and_explicit_suffixes() {
        let mut existing = vec![Palette::new("Brand", vec![Rgba::BLACK])];
        let imported = decode(r##"[{"name":"brand","colors":["#ff0000"]},{"name":"Brand (2)","colors":["#00ff00"]},{"name":"brand","colors":["#0000ff"]}]"##).unwrap();
        assert_eq!(merge(&mut existing, imported).unwrap(), 3);
        assert_eq!(existing[0], Palette::new("Brand", vec![Rgba::BLACK]));
        assert_eq!(
            existing
                .iter()
                .map(|palette| name_key(&palette.name))
                .collect::<HashSet<_>>()
                .len(),
            4
        );
        assert_eq!(existing[2].name, "Brand (2)");
        assert_eq!(
            existing
                .iter()
                .flat_map(|palette| palette.colors.iter().copied())
                .collect::<Vec<_>>(),
            vec![
                Rgba::BLACK,
                Rgba::rgb(255, 0, 0),
                Rgba::rgb(0, 255, 0),
                Rgba::rgb(0, 0, 255)
            ]
        );
        let before = existing.clone();
        assert!(merge(&mut existing, vec![Palette::new("", vec![Rgba::WHITE])]).is_err());
        assert_eq!(existing, before, "invalid imports never partly append");
    }

    #[test]
    fn malformed_and_oversized_inputs_cannot_replace_a_saved_library() {
        let directory = TempDir::new();
        let path = directory.0.join("safe.omacolors");
        let palettes = [Palette::new("Keep me", vec![Rgba::BLACK])];
        save_file(&path, &palettes).unwrap();
        let before = fs::read(&path).unwrap();
        for bad in [
            r###"{"name":"bad","colors":["##112233"]}"###,
            r##"{"name":"bad","colors":["#112233GG"]}"##,
            r##"{"name":"bad","colors":[{"r":300,"g":0,"b":0,"a":255}]}"##,
            r##"{"version":2,"palettes":[]}"##,
            r##"{"name":"bad","colors":"#112233"}"##,
        ] {
            assert!(decode(bad).is_err(), "{bad}");
        }
        assert!(
            save_file(
                &path,
                &[Palette::new(
                    "bad",
                    vec![Rgba::BLACK; MAX_COLORS_PER_PALETTE + 1]
                )]
            )
            .is_err()
        );
        assert!(save_file(&path, &[Palette::new("bad\nname", vec![])]).is_err());
        assert_eq!(fs::read(&path).unwrap(), before);
        let blocked = directory.0.join("a-directory.omacolors");
        fs::create_dir(&blocked).unwrap();
        fs::write(blocked.join("keep.txt"), "untouched").unwrap();
        assert!(save_file(&blocked, &palettes).is_err());
        assert_eq!(
            fs::read_to_string(blocked.join("keep.txt")).unwrap(),
            "untouched"
        );
        let large = directory.0.join("large.omacolors");
        File::create(&large)
            .unwrap()
            .set_len(MAX_FILE_BYTES as u64 + 1)
            .unwrap();
        assert!(load_file(&large).unwrap_err().contains("4 MiB"));
        assert_eq!(
            fs::read_dir(&directory.0).unwrap().count(),
            3,
            "failed replacement removes its staging file"
        );
    }
}
