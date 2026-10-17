use super::{
    ADVANCED_LAYOUT, QUICK_MASK, CHANNELS, COLOR, COMPOSITIONS, EXPORT_FOLDERS, EXPORT_RESOLUTION, EXPRESSIONS, NATIVE,
    PSD_TYPE, SCENE3D, SCREEN_EXPORT, SMART, SOURCE,
};
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
    explicit_grid: bool,
    baseline_align: bool,
    named_properties: bool,
}

/// Identify an advanced Layout field in retained history.
/// Args: logical property path. Returns its field tag for the streaming value check.
fn layout_property(path: &str) -> Option<&'static str> {
    let (_, tail) = path.rsplit_once("/layout/")?;
    let mut fields = tail.split('/');
    match (fields.next()?, fields.next()?) {
        ("grid", "row" | "column") => Some("layout-placement"),
        ("grid", "row_span" | "column_span") => Some("layout-span"),
        ("stack", "column_tracks" | "row_tracks") => Some(if fields.next().is_some() {
            "layout-track-member"
        } else {
            "layout-tracks"
        }),
        ("stack", "align") => Some("layout-align"),
        ("breakpoints", _) if fields.next() == Some("align") => Some("layout-align"),
        ("component", "Main") if fields.next() == Some("properties") => {
            Some(if fields.next().is_some() {
                "layout-property-member"
            } else {
                "layout-properties"
            })
        }
        _ => None,
    }
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
    if key.split('/').any(|field| field == "quick_mask") { return QUICK_MASK; }
    if key.split('/').any(|field| field == "saved_channels") { return CHANNELS; }
    if key
        .split('/')
        .any(|field| matches!(field, "export_folder" | "export_subfolders"))
    {
        return EXPORT_FOLDERS;
    }
    if key.split('/').any(|field| field == "export_resolution") {
        return EXPORT_RESOLUTION;
    }
    if key.split('/').any(|field| field == "screen_export") {
        return SCREEN_EXPORT;
    }
    if !value.exists {
        return 1;
    }
    let path = layout_property(key).unwrap_or_else(|| key.rsplit('/').next().unwrap_or(key));
    match path {
        "grid" if value.explicit_grid => ADVANCED_LAYOUT,
        "stack" | "breakpoints" if value.baseline_align => ADVANCED_LAYOUT,
        "Main" if value.named_properties => ADVANCED_LAYOUT,
        "column_tracks" | "row_tracks" | "layout-tracks" | "layout-properties"
            if value.present && value.word != Some("List") =>
        {
            ADVANCED_LAYOUT
        }
        "layout-placement" | "layout-track-member" | "layout-property-member" => ADVANCED_LAYOUT,
        "layout-span" if value.number != Some(1) => ADVANCED_LAYOUT,
        "layout-align" if value.word == Some("Baseline") => ADVANCED_LAYOUT,
        "collaboration" => 24,
        "screen_export" => SCREEN_EXPORT,
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
                    "Baseline" => Some("Baseline"),
                    "List" => Some("List"),
                    _ if s.starts_with('/') => {
                        if s.split('/').any(|field| field == "quick_mask") {
                            Some("quick_mask")
                        } else if s.split('/').any(|field| field == "saved_channels") {
                            Some("saved_channels")
                        } else if s.split('/').any(|field| field == "export_folder") {
                            Some("export_folder")
                        } else if s.split('/').any(|field| field == "export_subfolders") {
                            Some("export_subfolders")
                        } else if s.split('/').any(|field| field == "export_resolution") {
                            Some("export_resolution")
                        } else if s.split('/').any(|field| field == "screen_export") {
                            Some("screen_export")
                        } else {
                            layout_property(s).or_else(|| match s.rsplit('/').next() {
                                Some("source_attachment") => Some("source_attachment"),
                                Some("sources") => Some("sources"),
                                Some("screen_export") => Some("screen_export"),
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
                            })
                        }
                    }
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
                    result.baseline_align |= child.baseline_align;
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
                    result.explicit_grid |= match key.as_str() {
                        "row" | "column" => child.exists,
                        "row_span" | "column_span" => child.exists && child.number != Some(1),
                        _ => false,
                    };
                    result.baseline_align |= key == "align" && child.word == Some("Baseline");
                    result.named_properties |=
                        key == "properties" && child.present && child.word != Some("List");
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
                    result.explicit_grid = child.explicit_grid;
                    result.baseline_align = child.baseline_align;
                    result.named_properties = child.named_properties;
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
    for change in &replica.changes {
        if let crate::collaboration::Edit::Write { writes } = &change.edit {
            for write in writes.iter().filter(|write| write.value.is_none()) {
                version = version.max(field_version(&write.path, &Seen::default()));
            }
        }
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};

    #[test]
    fn layout28_retained_base_and_withdrawn_writes_share_the_canonical_predicates() {
        let fixtures: Vec<Value> = serde_json::from_str(include_str!(
            "../../../site/shared/liveProtocol.fixtures.json"
        ))
        .unwrap();
        let mut checked = 0;
        for fixture in fixtures
            .iter()
            .filter(|fixture| fixture["name"].as_str().unwrap().starts_with("layout28"))
        {
            let name = fixture["name"].as_str().unwrap();
            let path = fixture["path"].as_str().unwrap();
            let unknown = name.contains("unknown");
            let required = fixture["error"] == true && !unknown;
            for in_base in [true, false] {
                let mut replica = json!({"session":"layout-retained","actor":"a","base":{},"changes":[
                    {"id":{"actor":"a","seq":1},"clock":1,"seen":{},"edit":{"type":"write","writes":[]}},
                    {"id":{"actor":"a","seq":2},"clock":2,"seen":{},"edit":{"type":"toggle","target":{"actor":"a","seq":1},"enabled":false}}
                ]});
                if in_base {
                    replica["base"][path] = fixture["cell"].clone();
                } else {
                    replica["changes"][0]["edit"]["writes"] =
                        json!([{"path":path,"value":fixture["cell"]}]);
                }
                let typed: crate::collaboration::Replica =
                    serde_json::from_value(replica.clone()).unwrap();
                if unknown {
                    assert!(typed.protocol_requirement().is_err(), "{name}");
                }
                assert_eq!(
                    replica_version(&typed).unwrap() >= ADVANCED_LAYOUT,
                    required,
                    "{name}, base={in_base}"
                );
                let envelope = json!({"version":27,"doc":{"collaboration":replica}});
                let text = envelope.to_string();
                assert_eq!(
                    serialized_version(&text).unwrap().0 >= ADVANCED_LAYOUT,
                    required,
                    "{name}, base={in_base}"
                );
                assert_eq!(
                    crate::project::validate_retained_json(&text).is_err(),
                    required,
                    "{name}, base={in_base}"
                );
                assert_eq!(
                    value_version(&envelope).unwrap() >= ADVANCED_LAYOUT,
                    required,
                    "{name}, base={in_base}"
                );
            }
            checked += 1;
        }
        assert_eq!(checked, 75);
    }

    #[test]
    fn layout28_retained_source_keeps_empty_list_markers_legacy() {
        for path in [
            "/@entities/91/layout/stack/row_tracks",
            "/@entities/91/layout/component/Main/properties",
        ] {
            assert!(value_version(&json!({(path):"List"})).unwrap() < ADVANCED_LAYOUT);
        }
        let original = json!({"version":28,"doc":{"layers":[{"kind":{"Vector":{"shapes":[{"layout":{"grid":{"row":0}}}]}}}]}}).to_string().into_bytes();
        let source = crate::source_attachment::SourceAttachment::new(
            "retained.oma",
            "oma",
            original.clone(),
        )
        .unwrap();
        assert_eq!(source_version(&source), ADVANCED_LAYOUT);
        assert_eq!(source.bytes(), original);
    }
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
