use super::{COLOR, COMPOSITIONS, EXPRESSIONS, NATIVE, PSD_TYPE, SCENE3D, SMART, SOURCE};
use serde::Deserialize;
use serde::de::{MapAccess, SeqAccess, Visitor};

#[derive(Clone, Copy, Default)]
struct Seen {
    version: u32,
    declared: Option<u32>,
    present: bool,
    exists: bool,
    word: Option<&'static str>,
    number: Option<u32>,
    items: usize,
    motion_count: usize,
    motion_depth: usize,
}

fn composition_bounds<E: serde::de::Error>(value: &Seen) -> Result<(usize, usize), E> {
    let count = value.items.saturating_add(value.motion_count);
    let depth = value.motion_depth + usize::from(value.items > 0);
    if count > crate::motion::compositions::MAX_COMPOSITIONS {
        return Err(E::custom("CompositionLimit: at most 128 compositions"));
    }
    if depth > crate::motion::compositions::MAX_DEPTH {
        return Err(E::custom(
            "CompositionDepth: at most eight nested compositions",
        ));
    }
    Ok((count, depth))
}

fn field_version(key: &str, value: &Seen) -> u32 {
    if !value.exists {
        return 1;
    }
    let path = key.rsplit('/').next().unwrap_or(key);
    match path {
        "collaboration" => 24,
        "expressions" if value.present => EXPRESSIONS,
        "compositions" if value.present => COMPOSITIONS,
        "scene3d" => SCENE3D,
        "ease" if matches!(value.word, Some("Hold" | "CubicBezier")) => COMPOSITIONS,
        "CubicBezier" => COMPOSITIONS,
        "sources" if value.present => SOURCE,
        "source_attachment" => SOURCE,
        "native" => NATIVE,
        "cmyk" => COLOR,
        "icc_profile" if value.present => COLOR,
        "model" if value.word == Some("Cmyk") => COLOR,
        "depth" if matches!(value.word, Some("U16" | "F32")) => NATIVE,
        "psd_type" => PSD_TYPE,
        "psd_smart" => SMART,
        _ => 1,
    }
}

impl<'de> Deserialize<'de> for Seen {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Scan;
        impl<'de> Visitor<'de> for Scan {
            type Value = Seen;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("retained document properties")
            }
            fn visit_unit<E: serde::de::Error>(self) -> Result<Seen, E> {
                Ok(Seen::default())
            }
            fn visit_bool<E: serde::de::Error>(self, _: bool) -> Result<Seen, E> {
                Ok(Seen {
                    present: true,
                    exists: true,
                    ..Default::default()
                })
            }
            fn visit_u64<E: serde::de::Error>(self, n: u64) -> Result<Seen, E> {
                Ok(Seen {
                    present: true,
                    exists: true,
                    number: u32::try_from(n).ok(),
                    ..Default::default()
                })
            }
            fn visit_i64<E: serde::de::Error>(self, n: i64) -> Result<Seen, E> {
                Ok(Seen {
                    present: true,
                    exists: true,
                    number: u32::try_from(n).ok(),
                    ..Default::default()
                })
            }
            fn visit_f64<E: serde::de::Error>(self, _: f64) -> Result<Seen, E> {
                Ok(Seen {
                    present: true,
                    exists: true,
                    ..Default::default()
                })
            }
            fn visit_str<E: serde::de::Error>(self, s: &str) -> Result<Seen, E> {
                let word = match s {
                    "U16" => Some("U16"),
                    "F32" => Some("F32"),
                    "Cmyk" => Some("Cmyk"),
                    "Hold" => Some("Hold"),
                    "CubicBezier" => Some("CubicBezier"),
                    _ if s.starts_with('/') => match s.rsplit('/').next() {
                        Some("source_attachment") => Some("source_attachment"),
                        Some("sources") => Some("sources"),
                        Some("native") => Some("native"),
                        Some("psd_type") => Some("psd_type"),
                        Some("psd_smart") => Some("psd_smart"),
                        Some("cmyk") => Some("cmyk"),
                        Some("icc_profile") => Some("icc_profile"),
                        Some("expressions") => Some("expressions"),
                        Some("compositions") => Some("compositions"),
                        Some("scene3d") => Some("scene3d"),
                        Some("ease") => Some("ease"),
                        _ => None,
                    },
                    _ => None,
                };
                Ok(Seen {
                    present: !s.is_empty(),
                    exists: true,
                    word,
                    ..Default::default()
                })
            }
            fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Seen, A::Error> {
                let mut result = Seen {
                    exists: true,
                    ..Default::default()
                };
                while let Some(child) = seq.next_element::<Seen>()? {
                    result.present = true;
                    result.version = result.version.max(child.version);
                    result.items = result.items.saturating_add(1);
                    result.motion_count = result.motion_count.saturating_add(child.motion_count);
                    result.motion_depth = result.motion_depth.max(child.motion_depth);
                }
                Ok(result)
            }
            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Seen, A::Error> {
                let mut result = Seen {
                    exists: true,
                    ..Default::default()
                };
                let mut count = 0;
                let mut wrapped = None;
                let mut profile = false;
                let mut profile_bytes = None;
                let mut samples = false;
                let mut intent = None;
                let mut color_model = false;
                let mut property = None;
                let mut written = None;
                while let Some(key) = map.next_key::<String>()? {
                    let mut child = map.next_value::<Seen>()?;
                    if key.rsplit('/').next() == Some("compositions") {
                        (child.motion_count, child.motion_depth) =
                            composition_bounds::<A::Error>(&child)?;
                    }
                    count += 1;
                    result.present = true;
                    result.version = result.version.max(child.version);
                    if key != "collaboration" {
                        result.motion_count =
                            result.motion_count.saturating_add(child.motion_count);
                        result.motion_depth = result.motion_depth.max(child.motion_depth);
                    }
                    if key == "icc_profile" {
                        profile = child.present;
                    } else {
                        result.version = result.version.max(field_version(&key, &child));
                    }
                    if key == "version" {
                        result.declared = child.number;
                    }
                    if key == "icc_bytes" {
                        profile_bytes = child.number;
                    }
                    if key == "intent" {
                        intent = child.number;
                    }
                    if key == "samples" {
                        samples = child.exists;
                    }
                    if key == "model" {
                        color_model = child.exists;
                    }
                    if key == "path" {
                        property = child.word;
                    }
                    if key == "value" {
                        written = Some(child);
                    }
                    if key == "Value" || key == "Variant" {
                        wrapped = Some(child);
                    }
                }
                if profile_bytes.map_or(profile, |bytes| bytes > 0)
                    || (samples && intent.is_some_and(|value| value != 1))
                    || (color_model && intent.is_some_and(|value| value != 0))
                {
                    result.version = result.version.max(COLOR);
                }
                if count == 1
                    && let Some(child) = wrapped
                {
                    result.present = child.present;
                    result.exists = child.exists;
                    result.word = child.word;
                    result.number = child.number;
                    result.items = child.items;
                }
                if let Some(property) = property
                    && let Some(value) = written
                {
                    result.version = result.version.max(field_version(property, &value));
                    if property == "compositions" {
                        (result.motion_count, result.motion_depth) =
                            composition_bounds::<A::Error>(&value)?;
                    }
                }
                Ok(result)
            }
        }
        deserializer.deserialize_any(Scan)
    }
}

/// Inspect original JSON before a reader could discard unknown retained state.
/// Args: project or recovery JSON. Returns the greatest retained feature and declared envelope version.
pub(crate) fn serialized_version(text: &str) -> Result<(u32, Option<u32>), String> {
    let seen: Seen = serde_json::from_str(text).map_err(|e| e.to_string())?;
    Ok((seen.version.max(1), seen.declared))
}

/// Inspect retained properties in live base and change cells without materializing pixels.
/// Args: JSON property tree. Returns its greatest retained feature version.
pub(crate) fn value_version(value: &serde_json::Value) -> Result<u32, String> {
    Seen::deserialize(value)
        .map(|seen| seen.version.max(1))
        .map_err(|e| e.to_string())
}

/// Inspect every retained live property without copying or decoding historical buffers.
/// Args: base and all writes. Returns their greatest required reader version.
pub(super) fn replica_version(replica: &crate::collaboration::Replica) -> Result<u32, String> {
    let mut version = 24;
    for (path, cell) in replica.retained_cells() {
        let seen = match cell {
            crate::collaboration::Cell::Value(value) => {
                Seen::deserialize(value).map_err(|e| e.to_string())?
            }
            crate::collaboration::Cell::Variant(name) => {
                Seen::deserialize(serde_json::Value::String(name.clone()))
                    .map_err(|e| e.to_string())?
            }
            crate::collaboration::Cell::Pixels {
                native: Some(native),
                ..
            } => {
                version = version.max(NATIVE);
                Seen::deserialize(native).map_err(|e| e.to_string())?
            }
            _ => Seen::default(),
        };
        version = version.max(seen.version).max(field_version(path, &seen));
    }
    Ok(version)
}

/// Inspect an immutable original without decoding its pixels or copying its bytes.
/// Args: retained source. Returns its source gate plus any embedded native document gate.
pub(super) fn source_version(source: &crate::source_attachment::SourceAttachment) -> u32 {
    let bytes = source.bytes();
    if matches!(
        source.format.to_ascii_lowercase().as_str(),
        "oma" | "omadesign"
    ) {
        return std::str::from_utf8(bytes)
            .ok()
            .and_then(|text| serialized_version(text).ok())
            .map_or(SOURCE, |(version, _)| version.max(SOURCE));
    }
    if bytes.starts_with(b"8BPS") && bytes.len() >= 26 {
        let depth = u16::from_be_bytes([bytes[22], bytes[23]]);
        let model = u16::from_be_bytes([bytes[24], bytes[25]]);
        return if model == 4 || psd_profile(bytes) {
            COLOR
        } else if matches!(depth, 16 | 32) {
            NATIVE
        } else {
            SOURCE
        };
    }
    SOURCE
}

fn psd_profile(bytes: &[u8]) -> bool {
    fn length(bytes: &[u8], at: usize) -> Option<usize> {
        Some(u32::from_be_bytes(bytes.get(at..at.checked_add(4)?)?.try_into().ok()?) as usize)
    }
    let Some(start) = length(bytes, 26).and_then(|n| n.checked_add(30)) else {
        return false;
    };
    let Some(end) = length(bytes, start)
        .and_then(|n| start.checked_add(4)?.checked_add(n))
        .filter(|&n| n <= bytes.len())
    else {
        return false;
    };
    let mut at = start + 4;
    while at.checked_add(7).is_some_and(|n| n <= end) && bytes.get(at..at + 4) == Some(b"8BIM") {
        let id = u16::from_be_bytes([bytes[at + 4], bytes[at + 5]]);
        let name = bytes[at + 6] as usize + 1;
        let Some(size_at) = at
            .checked_add(6)
            .and_then(|n| n.checked_add(name + name % 2))
        else {
            return false;
        };
        let Some(size) = length(bytes, size_at) else {
            return false;
        };
        let Some(next) = size_at
            .checked_add(4)
            .and_then(|n| n.checked_add(size))
            .and_then(|n| n.checked_add(size % 2))
            .filter(|&n| n <= end)
        else {
            return false;
        };
        if id == 0x040f && size > 0 {
            return true;
        }
        at = next;
    }
    false
}
