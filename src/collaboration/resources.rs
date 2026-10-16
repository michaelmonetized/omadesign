//! Archive references retained by property writes, without historical pixel decoding.

use super::{Cell, Replica};
use serde::Deserialize;
use serde_json::Value;
use std::{collections::BTreeSet, sync::Arc};

/// Match a typed character-span property.
/// Args: logical property path and relative field. Returns whether the index and field match.
fn span_field(path: &str, field: &str) -> bool {
    path.rsplit_once("/geom/Text/spans/")
        .is_some_and(|(_, tail)| {
            tail.split_once('/')
                .is_some_and(|(index, rest)| index.parse::<usize>().is_ok() && rest == field)
        })
}

/// Identify native text font fields.
/// Args: logical property path. Returns whether the value names a font.
fn font_slot(path: &str) -> bool {
    path.ends_with("/geom/Text/font") || span_field(path, "font")
}

/// Identify fields covered by the existing SVG archive.
/// Args: logical property path. Returns whether its string is a Pattern or filter definition.
fn source_slot(path: &str) -> bool {
    path.ends_with("/style/fill/Pattern/definitions")
        || span_field(path, "fill/Pattern/definitions")
        || path.ends_with("/svg_filter/definitions")
        || path.ends_with("/svg_filter/original_definitions")
}

/// Walk serialized native subtrees and their retained Cell payloads.
/// Args: logical root, native value and visitor. Keeps source bytes opaque and composition traversal bounded.
pub(super) fn values(
    path: &str,
    value: &Value,
    visit: &mut impl FnMut(&str, &Value),
) -> Result<(), String> {
    fn cell(
        path: &str,
        value: &Value,
        depth: usize,
        count: &mut usize,
        visit: &mut impl FnMut(&str, &Value),
    ) -> Result<(), String> {
        match value {
            Value::String(marker) if marker == "Object" => walk(
                path,
                &Value::Object(Default::default()),
                depth,
                count,
                visit,
            ),
            Value::String(marker) if marker == "List" => {
                walk(path, &Value::Array(vec![]), depth, count, visit)
            }
            Value::Object(object) if object.len() == 1 && object.contains_key("Variant") => {
                walk(path, &object["Variant"], depth, count, visit)
            }
            _ => walk(path, value, depth, count, visit),
        }
    }
    fn walk(
        path: &str,
        value: &Value,
        depth: usize,
        count: &mut usize,
        visit: &mut impl FnMut(&str, &Value),
    ) -> Result<(), String> {
        if depth > crate::motion::compositions::MAX_DEPTH {
            return Err("CompositionDepth: at most eight nested compositions".into());
        }
        if let Value::Object(object) = value
            && object.len() == 1
            && let Some(value) = object.get("Value")
        {
            return walk(path, value, depth, count, visit);
        }
        visit(path, value);
        if super::protocol::relative_path(path) == "/sources" {
            return Ok(());
        }
        match value {
            Value::Object(object) => {
                if let Some(property) = object
                    .get("path")
                    .and_then(Value::as_str)
                    .map(str::to_owned)
                    && let Some(value) = object.get("value")
                {
                    return cell(&property, value, depth, &mut 0, visit);
                }
                for (key, value) in object.iter() {
                    if key.starts_with('/') {
                        cell(key, value, depth, &mut 0, visit)?;
                    } else {
                        walk(&super::child(path, key), value, depth, count, visit)?;
                    }
                }
            }
            Value::Array(array) => {
                let compositions = path.ends_with("/motion/compositions");
                if compositions {
                    *count = count
                        .checked_add(array.len())
                        .filter(|count| *count <= crate::motion::compositions::MAX_COMPOSITIONS)
                        .ok_or("CompositionLimit: at most 128 compositions")?;
                }
                for (index, value) in array.iter().enumerate() {
                    walk(
                        &super::child(path, &index.to_string()),
                        value,
                        depth + usize::from(compositions),
                        count,
                        visit,
                    )?;
                }
            }
            _ => {}
        }
        Ok(())
    }
    walk(path, value, 0, &mut 0, visit)
}

/// Walk serialized native subtrees and their retained Cell payloads.
/// Args: logical root, native value and visitor. Keeps source bytes opaque and composition traversal bounded.
fn values_mut(
    path: &str,
    value: &mut Value,
    visit: &mut impl FnMut(&str, &mut Value),
) -> Result<(), String> {
    fn cell(
        path: &str,
        value: &mut Value,
        depth: usize,
        count: &mut usize,
        visit: &mut impl FnMut(&str, &mut Value),
    ) -> Result<(), String> {
        if matches!(value.as_str(), Some("Object" | "List")) {
            return Ok(());
        }
        if let Value::Object(object) = value
            && object.len() == 1
            && let Some(value) = object.get_mut("Variant")
        {
            return walk(path, value, depth, count, visit);
        }
        walk(path, value, depth, count, visit)
    }
    fn walk(
        path: &str,
        value: &mut Value,
        depth: usize,
        count: &mut usize,
        visit: &mut impl FnMut(&str, &mut Value),
    ) -> Result<(), String> {
        if depth > crate::motion::compositions::MAX_DEPTH {
            return Err("CompositionDepth: at most eight nested compositions".into());
        }
        if let Value::Object(object) = value
            && object.len() == 1
            && let Some(value) = object.get_mut("Value")
        {
            return walk(path, value, depth, count, visit);
        }
        visit(path, value);
        if super::protocol::relative_path(path) == "/sources" {
            return Ok(());
        }
        match value {
            Value::Object(object) => {
                if let Some(property) = object
                    .get("path")
                    .and_then(Value::as_str)
                    .map(str::to_owned)
                    && let Some(value) = object.get_mut("value")
                {
                    return cell(&property, value, depth, &mut 0, visit);
                }
                for (key, value) in object.iter_mut() {
                    if key.starts_with('/') {
                        cell(key, value, depth, &mut 0, visit)?;
                    } else {
                        walk(&super::child(path, key), value, depth, count, visit)?;
                    }
                }
            }
            Value::Array(array) => {
                let compositions = path.ends_with("/motion/compositions");
                if compositions {
                    *count = count
                        .checked_add(array.len())
                        .filter(|count| *count <= crate::motion::compositions::MAX_COMPOSITIONS)
                        .ok_or("CompositionLimit: at most 128 compositions")?;
                }
                for (index, value) in array.iter_mut().enumerate() {
                    walk(
                        &super::child(path, &index.to_string()),
                        value,
                        depth + usize::from(compositions),
                        count,
                        visit,
                    )?;
                }
            }
            _ => {}
        }
        Ok(())
    }
    walk(path, value, 0, &mut 0, visit)
}

/// Collect portable font references from base and every write, including span-only faces.
/// Args: retained replica. Returns unique font IDs without loading or shaping text.
pub(crate) fn fonts(replica: &Replica) -> Result<BTreeSet<String>, String> {
    let mut fonts = BTreeSet::new();
    for (path, cell) in replica.retained_cells() {
        if let Cell::Value(value) = cell {
            values(path, value, &mut |path, value| {
                if font_slot(path)
                    && let Some(font) = value.as_str()
                {
                    fonts.insert(font.to_owned());
                }
            })?;
        }
    }
    Ok(fonts)
}

/// Share the existing SVG archive visitor with retained JSON resource slots.
/// Args: replica and the document's source visitor. Updates Pattern and filter references only.
pub(crate) fn visit_sources(
    replica: &mut Replica,
    visit: &mut impl FnMut(&mut Arc<str>),
) -> Result<(), String> {
    for (path, cell) in replica.retained_cells_mut() {
        if let Cell::Value(value) = cell {
            values_mut(path, value, &mut |path, value| {
                if source_slot(path)
                    && let Value::String(text) = value
                {
                    let mut source: Arc<str> = Arc::from(text.as_str());
                    visit(&mut source);
                    if source.as_ref() != text.as_str() {
                        *text = source.to_string();
                    }
                }
            })?;
        }
    }
    Ok(())
}

/// Validate original source archives and pixel descriptors without decoding historical pixels.
/// Args: replica. Returns the first integrity or transport-limit error, even in withdrawn writes.
pub(crate) fn validate(replica: &Replica) -> Result<(), String> {
    for (path, cell) in replica.retained_cells() {
        if matches!(cell, Cell::Pixels { .. }) && super::pixels::size(cell)? > super::PIXEL_BUDGET {
            return Err("Live pixels exceed the total pixel budget".into());
        }
        if let Cell::Value(value) = cell {
            let mut result = Ok(());
            values(path, value, &mut |path, value| {
                if path.ends_with("/sources") && result.is_ok() {
                    result = validate_sources(value);
                } else if let Some(pixels) = value.get("Pixels")
                    && result.is_ok()
                {
                    result = super::pixels::descriptor_size(
                        pixels["w"].as_u64().unwrap_or(0),
                        pixels["h"].as_u64().unwrap_or(0),
                        pixels.get("native"),
                    )
                    .map(|_| ());
                }
            })?;
            result?;
        }
    }
    Ok(())
}

/// Reuse original-source integrity validation for one historical archive vector.
/// Args: serialized source vector. Returns an error for corruption, duplicates or excess bytes.
fn validate_sources(value: &Value) -> Result<(), String> {
    let sources = Vec::<crate::source_attachment::SourceAttachment>::deserialize(value)
        .map_err(|error| format!("Invalid retained source archive: {error}"))?;
    let mut bytes = 0usize;
    let mut ids = BTreeSet::new();
    for source in &sources {
        source.validate()?;
        bytes = bytes
            .checked_add(source.bytes().len())
            .filter(|bytes| *bytes <= 512 * 1024 * 1024)
            .ok_or("Retained sources exceed 512 MiB")?;
        if !ids.insert(&source.id) {
            return Err("Duplicate retained source UUID".into());
        }
    }
    Ok(())
}
