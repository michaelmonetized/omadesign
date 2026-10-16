//! Retained capability checks before transport or typed document materialization.

use super::{Cell, Replica};
use serde_json::Value;

/// Derive the advertised capability from the implemented document readers and easing model.
/// Args: none. Returns complete common+Motion support only after all readers and typed Motion round trips pass.
pub fn local_maximum() -> u32 {
    static MAXIMUM: std::sync::OnceLock<u32> = std::sync::OnceLock::new();
    *MAXIMUM.get_or_init(|| {
        if !(1..=27).all(crate::project::supports_version) {
            return 2;
        }
        let Ok(document) =
            serde_json::to_value(crate::document::Document::new("Capability", 1., 1., 72.))
        else {
            return 2;
        };
        let probe = serde_json::json!({
            "duration":2.,"fps":30.,"looped":true,
            "tracks":[{"shape":1,"prop":"X","keys":[
                {"t":0.,"value":0.,"ease":"Hold"},
                {"t":1.,"value":1.,"ease":{"CubicBezier":{"x1":0.,"y1":0.,"x2":1.,"y2":1.}}}
            ]}],
            "expressions":[{"shape":1,"prop":"X","source":"time","enabled":true}],
            "compositions":[{"id":7,"name":"Capability","document":document,"end":2.}],
            "scene3d":{}
        });
        let result = serde_json::from_value::<crate::motion::Motion>(probe.clone())
            .ok()
            .and_then(|motion| serde_json::to_value(motion).ok());
        if result.is_some_and(|value| {
            value["tracks"] == probe["tracks"]
                && value["expressions"] == probe["expressions"]
                && value["compositions"][0]["document"] == probe["compositions"][0]["document"]
                && value["scene3d"]["camera"].is_object()
        }) {
            3
        } else {
            2
        }
    })
}

/// Refuse unsupported retained state before any transport or local replacement.
/// Args: complete replica and remote offer. Returns the required protocol within both reader limits.
pub fn require(replica: &Replica, offered: u32) -> Result<u32, String> {
    require_minimum(replica.protocol_requirement()?, offered)
}

pub(super) fn require_minimum(required: u32, offered: u32) -> Result<u32, String> {
    if !(1..=3).contains(&offered) || required > offered.min(local_maximum()) {
        return Err(if required == 2 { "This live document requires native-preservation protocol 2" }
            else { "This live document requires complete common+Motion preservation protocol 3 and a complete common+Motion reader" }.into());
    }
    Ok(required)
}

pub(super) fn cell(path: &str, cell: &Cell) -> Result<u32, String> {
    let native = if super::native_cell(path, cell) { 2 } else { 1 };
    let value = match cell {
        Cell::Object => serde_json::json!({}),
        Cell::List => serde_json::json!([]),
        Cell::Value(value) => return Ok(native.max(scan(path, value)?)),
        Cell::Variant(variant) => Value::String(variant.clone()),
        Cell::Pixels { .. } => Value::Null,
    };
    Ok(native.max(scan(path, &value)?))
}

pub(super) fn relative_path(mut path: &str) -> &str {
    loop {
        let stable = path
            .strip_prefix("/@compositions/")
            .and_then(|tail| tail.split_once('/'))
            .filter(|(id, _)| !id.is_empty())
            .map(|(_, tail)| tail);
        let array = path
            .strip_prefix("/motion/compositions/")
            .and_then(|tail| tail.split_once('/'))
            .filter(|(index, _)| index.parse::<usize>().is_ok())
            .map(|(_, tail)| tail);
        let document = stable
            .or(array)
            .and_then(|tail| tail.strip_prefix("document"))
            .filter(|tail| tail.is_empty() || tail.starts_with('/'));
        let Some(tail) = document else {
            return path;
        };
        path = tail;
    }
}

fn nonzero(value: &Value) -> Result<bool, String> {
    match value {
        Value::Number(n) => n
            .as_f64()
            .filter(|n| n.is_finite())
            .map(|n| n != 0.)
            .ok_or_else(|| "Unsupported retained numeric property".into()),
        Value::Array(items) => items
            .iter()
            .try_fold(false, |found, item| Ok(nonzero(item)? || found)),
        _ => Err("Unsupported retained numeric property".into()),
    }
}

fn common(path: &str, value: &Value) -> Result<u32, String> {
    let p = relative_path(path);
    refuse_future_layout(p, value)?;
    let parts: Vec<_> = p.split('/').skip(1).collect();
    let root = parts.first().copied().unwrap_or("");
    let document_fields = [
        "publishing",
        "color",
        "text_wrap_above_only",
        "name",
        "workspace",
        "width",
        "height",
        "dpi",
        "layers",
        "import_notes",
        "sources",
        "guides",
        "ruler",
        "grid",
        "transparent",
        "artboards",
        "artboardless",
        "show_bleed",
        "show_safe",
        "bleed",
        "motion",
        "cloud",
        "collaboration",
        "comments",
        "layout_tokens",
        "@entities",
        "@parent",
        "@position",
        "@compositions",
    ];
    if (!root.is_empty() && !document_fields.contains(&root))
        || (p.is_empty()
            && value.as_object().is_some_and(|map| {
                map.keys()
                    .any(|key| !document_fields.contains(&key.as_str()))
            })
            && !(value.get("version").is_some() && value.get("doc").is_some()))
    {
        return Err("Unsupported retained document property".into());
    }
    if root == "sources" {
        return Ok(
            if parts.len() > 1 || value.as_array().is_some_and(|items| !items.is_empty()) {
                3
            } else {
                1
            },
        );
    }
    if root == "publishing" {
        let Some(field) = parts.get(1).copied() else {
            return Ok(1);
        };
        let rich = match field {
            "facing_pages" => !value.is_null() && value != &Value::Bool(false),
            "bleed" => !value.is_null() && nonzero(value)?,
            "swatches"
            | "master_stories"
            | "master_layers"
            | "master_assets"
            | "page_sides"
            | "page_masters"
            | "links"
            | "inherited_fingerprints"
            | "inherited_pages" => {
                parts.len() > 2 || value.as_object().is_some_and(|map| !map.is_empty())
            }
            "styles" | "masters" | "inherited_layers" => {
                parts.len() > 2 || value.as_array().is_some_and(|items| !items.is_empty())
            }
            _ => return Err("Unsupported retained publishing feature".into()),
        };
        return Ok(if rich { 3 } else { 1 });
    }
    let fill_tail = p
        .rsplit_once("/style/fill")
        .map(|(_, tail)| tail)
        .or_else(|| {
            p.rsplit_once("/geom/Text/spans/")
                .and_then(|(_, tail)| tail.split_once('/'))
                .filter(|(index, _)| index.parse::<usize>().is_ok())
                .and_then(|(_, tail)| tail.strip_prefix("fill"))
        });
    if let Some(tail) = fill_tail {
        let variants = ["Pattern", "Gradient", "None", "Solid", "Linear", "Radial"];
        let unsupported = if tail.starts_with('/') {
            !variants.contains(&tail.split('/').nth(1).unwrap_or(""))
        } else if let Some(name) = value.as_str() {
            !variants.contains(&name)
        } else {
            value
                .as_object()
                .is_some_and(|map| map.keys().any(|key| !variants.contains(&key.as_str())))
        };
        if unsupported {
            return Err("Unsupported retained fill variant".into());
        }
    }
    if ["@entities", "layers"].contains(&root)
        && parts.get(2).is_some_and(|field| {
            ["svg_filter", "source_attachment", "psd_type", "psd_smart"].contains(field)
        })
        && !value.is_null()
    {
        return Ok(3);
    }
    let text = p.rsplit_once("/geom/Text/").map(|(_, tail)| tail);
    if let Some(text) = text {
        let fields: Vec<_> = text.split('/').collect();
        if fields[0] == "font" && value.as_str().is_some_and(|font| !font.is_empty()) {
            return Ok(3);
        }
        if ["transform", "svg_text"].contains(&fields[0]) && !value.is_null() {
            return Ok(3);
        }
        if fields
            .get(1)
            .is_some_and(|index| index.parse::<usize>().is_ok())
            && !value.is_null()
            && ((fields[0] == "spans"
                && fields.get(2).is_some_and(|field| {
                    ["publishing_style", "font", "px", "fill"].contains(field)
                }))
                || (fields[0] == "paragraphs" && fields.get(2) == Some(&"publishing_style")))
        {
            return Ok(3);
        }
    }
    let span_fill = text
        .and_then(|tail| tail.strip_prefix("spans/"))
        .and_then(|tail| tail.split_once('/'))
        .filter(|(index, _)| index.parse::<usize>().is_ok())
        .map(|(_, tail)| tail);
    if (p.ends_with("/style/fill") || span_fill == Some("fill"))
        && value.as_str() == Some("Pattern")
    {
        return Ok(3);
    }
    if p.ends_with("/style/fill/Pattern")
        || p.contains("/style/fill/Pattern/")
        || span_fill.is_some_and(|tail| tail == "fill/Pattern" || tail.starts_with("fill/Pattern/"))
    {
        return Ok(3);
    }
    let gradient = p
        .rsplit_once("/style/fill/Gradient")
        .or_else(|| p.rsplit_once("/style/stroke/gradient"))
        .map(|(_, tail)| tail)
        .or_else(|| span_fill.and_then(|tail| tail.strip_prefix("fill/Gradient")));
    if let Some(tail) = gradient {
        let field = tail.split('/').nth(1).unwrap_or("");
        if !field.is_empty()
            && ![
                "kind",
                "from",
                "to",
                "stops",
                "radial_axis",
                "spread",
                "focal",
                "inner_radius",
            ]
            .contains(&field)
        {
            return Err("Unsupported retained gradient feature".into());
        }
        if field == "radial_axis" && !value.is_null() {
            return Ok(3);
        }
        if field == "spread" && !value.is_null() {
            return match value.as_str() {
                Some("Pad") => Ok(1),
                Some("Repeat" | "Reflect") => Ok(3),
                _ => Err("Unsupported retained gradient spread".into()),
            };
        }
        if ["focal", "inner_radius"].contains(&field) && !value.is_null() && nonzero(value)? {
            return Ok(3);
        }
    }
    Ok(1)
}

/// Refuse advanced layout before a protocol-3 reader can discard it.
/// Args: document-relative property and stored value. Returns an error for feature 28 or unknown layout variants.
fn refuse_future_layout(path: &str, value: &Value) -> Result<(), String> {
    let Some((owner, tail)) = path.rsplit_once("/layout") else {
        return Ok(());
    };
    if !(owner.starts_with("/@entities/") || owner.starts_with("/layers/"))
        || (!tail.is_empty() && !tail.starts_with('/'))
    {
        return Ok(());
    }
    let fields: Vec<_> = tail.split('/').skip(1).collect();
    let field = fields.first().copied().unwrap_or("");
    let unsupported = match field {
        "grid" => match fields.get(1).copied() {
            None => false,
            Some("row" | "column") => !value.is_null(),
            Some("row_span" | "column_span") => value.as_u64() != Some(1),
            _ => true,
        },
        "stack" | "breakpoints" => {
            let property = if field == "breakpoints" {
                fields.get(2).copied()
            } else {
                fields.get(1).copied()
            };
            match property {
                Some("column_tracks" | "row_tracks") => {
                    fields.len() > 2 || !value.as_array().is_some_and(Vec::is_empty)
                }
                Some("align") if !value.is_null() => {
                    !matches!(value.as_str(), Some("Start" | "Center" | "End" | "Stretch"))
                }
                Some("flow") if !value.is_null() => {
                    !matches!(value.as_str(), Some("Stack" | "Wrap" | "Grid"))
                }
                Some("direction") if !value.is_null() => {
                    !matches!(value.as_str(), Some("Vertical" | "Horizontal"))
                }
                Some("justify") if !value.is_null() => !matches!(
                    value.as_str(),
                    Some(
                        "Start" | "Center" | "End" | "SpaceBetween" | "SpaceAround" | "SpaceEvenly"
                    )
                ),
                _ => false,
            }
        }
        "component" => match fields.get(1).copied() {
            None if value.is_null() => false,
            None => match value {
                Value::String(variant) => !["Main", "Instance"].contains(&variant.as_str()),
                Value::Object(map) => map
                    .keys()
                    .any(|key| !["Main", "Instance"].contains(&key.as_str())),
                _ => true,
            },
            Some("Main") if fields.get(2) == Some(&"properties") => {
                fields.len() > 3 || !value.as_array().is_some_and(Vec::is_empty)
            }
            Some("Main" | "Instance") => false,
            _ => true,
        },
        _ => false,
    };
    if unsupported {
        return Err("Unsupported retained Layout feature or variant (feature 28 is outside live protocol 3)".into());
    }
    Ok(())
}

fn scan(path: &str, value: &Value) -> Result<u32, String> {
    let mut result = Ok(1u32);
    super::resources::values(path, value, &mut |property, value| {
        if result.is_err() || (property != path && relative_path(property).starts_with("/color/")) {
            return;
        }
        let depth = property
            .matches('/')
            .count()
            .saturating_sub(path.matches('/').count());
        match property_protocol(property, value, depth) {
            Ok(required) => {
                if let Ok(current) = &mut result {
                    *current = (*current).max(required);
                }
            }
            Err(error) => result = Err(error),
        }
    })?;
    result
}

fn property_protocol(path: &str, value: &Value, depth: usize) -> Result<u32, String> {
    if depth > 128 {
        return Err("Live property nesting exceeds the policy limit".into());
    }
    let mut required = common(path, value)?;
    if relative_path(path) != path {
        required = 3;
    }
    let path = relative_path(path);
    let motion = path
        .strip_prefix("/motion")
        .filter(|tail| tail.is_empty() || tail.starts_with('/'));
    if let Some(motion) = motion {
        let fields = [
            "duration",
            "fps",
            "looped",
            "tracks",
            "expressions",
            "compositions",
            "scene3d",
        ];
        let field = motion.split('/').nth(1).unwrap_or("");
        if (!field.is_empty() && !fields.contains(&field))
            || (motion.is_empty()
                && value
                    .as_object()
                    .is_some_and(|map| map.keys().any(|key| !fields.contains(&key.as_str()))))
        {
            return Err("Unsupported retained Motion feature".into());
        }
        let present = !value.is_null() && value.as_array().is_none_or(|a| !a.is_empty());
        if (["expressions", "compositions"].contains(&field)
            && (motion.split('/').count() > 2 || present))
            || (field == "scene3d" && !value.is_null())
        {
            required = 3;
        }
        if motion.ends_with("/ease") && !value.is_null() {
            match value.as_str() {
                Some("Linear" | "EaseIn" | "EaseOut" | "EaseInOut") => {}
                Some("Hold" | "CubicBezier") => required = 3,
                _ if value
                    .as_object()
                    .is_some_and(|m| m.len() == 1 && m.contains_key("CubicBezier")) =>
                {
                    required = 3
                }
                _ => return Err("Unsupported retained Motion easing".into()),
            }
        }
        if let Some((_, suffix)) = motion.split_once("/ease/") {
            if suffix != "CubicBezier"
                && ![
                    "CubicBezier/x1",
                    "CubicBezier/y1",
                    "CubicBezier/x2",
                    "CubicBezier/y2",
                ]
                .contains(&suffix)
            {
                return Err("Unsupported retained Motion easing".into());
            }
            required = 3;
        }
    }
    let parts: Vec<_> = path.split('/').collect();
    if parts.get(1) == Some(&"@compositions")
        && parts.get(2).is_some_and(|id| !id.is_empty())
        && parts.get(3) == Some(&"document")
    {
        required = 3;
    }
    if (path == "/color" || path.ends_with("/document/color")) && !value.is_null() {
        let neutral = value["model"] == "Rgb"
            && value["depth"] == "U8"
            && value["intent"] == 0
            && value["icc_profile"].as_array().is_some_and(Vec::is_empty);
        if !neutral {
            required = required.max(2);
        }
        return Ok(required);
    }
    if path.starts_with("/color/") && !value.is_null() {
        required = required.max(2);
    }
    if value.get("doc").is_some()
        && value
            .get("version")
            .and_then(Value::as_f64)
            .is_some_and(|v| !(1. ..=27.).contains(&v))
    {
        return Err("Unsupported retained document version".into());
    }
    Ok(required)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collaboration::{Change, Edit, Id, Write};
    #[test]
    fn advertisement_requires_every_reader_and_the_actual_typed_motion_model() {
        let expected = if (1..=27).all(crate::project::supports_version) {
            3
        } else {
            2
        };
        assert_eq!(local_maximum(), expected);
    }
    #[test]
    fn canonical_fixtures_match_retained_rust_policy() {
        let fixtures: Value =
            serde_json::from_str(include_str!("../../site/shared/liveProtocol.fixtures.json"))
                .unwrap();
        for case in fixtures.as_array().unwrap() {
            let cell: Cell = serde_json::from_value(case["cell"].clone()).unwrap();
            let result = super::cell(case["path"].as_str().unwrap(), &cell);
            if let Some(required) = case["required"].as_u64() {
                assert_eq!(result.unwrap(), required as u32, "{}", case["name"]);
            } else {
                assert!(result.is_err(), "{}", case["name"]);
            }
        }
    }
    #[test]
    fn future_layout_refuses_retained_receive_and_materialization_without_mutation() {
        let fixtures: Value =
            serde_json::from_str(include_str!("../../site/shared/liveProtocol.fixtures.json"))
                .unwrap();
        let doc = crate::document::Document::new("Layout refusal", 8., 8., 72.);
        for case in fixtures.as_array().unwrap().iter().filter(|case| {
            case["name"]
                .as_str()
                .is_some_and(|name| name.starts_with("layout28"))
                && case["error"] == true
        }) {
            let mut replica = Replica::new(
                "layout".into(),
                "a".repeat(64),
                crate::collaboration::snapshot(&doc).unwrap(),
            );
            let path = case["path"].as_str().unwrap().to_owned();
            let value: Cell = serde_json::from_value(case["cell"].clone()).unwrap();
            let before = serde_json::to_vec(&replica).unwrap();
            let change = Change {
                id: Id {
                    actor: "b".repeat(64),
                    seq: 1,
                },
                clock: 1,
                seen: Default::default(),
                edit: Edit::Write {
                    writes: vec![Write {
                        path: path.clone(),
                        value: Some(value.clone()),
                    }],
                },
            };
            assert!(replica.receive([change]).is_err(), "{}", case["name"]);
            assert_eq!(
                serde_json::to_vec(&replica).unwrap(),
                before,
                "{}",
                case["name"]
            );
            replica.base.insert(path, value);
            let retained = serde_json::to_vec(&replica).unwrap();
            assert!(
                replica.document_for_protocol(&doc, 3).is_err(),
                "{}",
                case["name"]
            );
            assert_eq!(serde_json::to_vec(&replica).unwrap(), retained);
        }
    }
    #[test]
    fn retained_motion_survives_withdrawal_and_explicit_offer_cannot_bypass_local_reader() {
        let doc = crate::document::Document::new("protocol", 8., 8., 72.);
        let mut replica = Replica::new(
            "session".into(),
            "actor".into(),
            crate::collaboration::snapshot(&doc).unwrap(),
        );
        let id = Id {
            actor: "other".into(),
            seq: 1,
        };
        replica.changes.push(Change {
            id: id.clone(),
            clock: 1,
            seen: Default::default(),
            edit: Edit::Write {
                writes: vec![Write {
                    path: "/motion/expressions".into(),
                    value: Some(Cell::Value(serde_json::json!([{"expression":"time"}]))),
                }],
            },
        });
        replica.changes.push(Change {
            id: Id {
                actor: "other".into(),
                seq: 2,
            },
            clock: 2,
            seen: Default::default(),
            edit: Edit::Toggle {
                target: id,
                enabled: false,
            },
        });
        assert_eq!(replica.protocol_requirement().unwrap(), 3);
        assert_eq!(replica.required_protocol(), 3);
        if !crate::project::supports_version(27) {
            assert_eq!(local_maximum(), 2);
            let before = serde_json::to_vec(&replica).unwrap();
            assert!(replica.document_for_protocol(&doc, 3).is_err());
            assert_eq!(serde_json::to_vec(&replica).unwrap(), before);
        }
        replica.base.insert(
            "/motion/layout28".into(),
            Cell::Value(serde_json::json!({})),
        );
        assert!(replica.protocol_requirement().is_err());
        assert_eq!(replica.required_protocol(), u32::MAX);
    }
}

#[cfg(test)]
mod common_tests {
    use super::*;
    #[test]
    fn known_common_payloads_refuse_old_local_reader_without_mutating_log_or_document() {
        let mut document = crate::document::Document::new("Common", 1., 1., 72.);
        let mut replica = Replica::new(
            "room".into(),
            "a".repeat(64),
            crate::collaboration::snapshot(&document).unwrap(),
        );
        document.publishing.facing_pages = true;
        let doc_before = serde_json::to_vec(&document).unwrap();
        let log_before = serde_json::to_vec(&replica).unwrap();
        if local_maximum() == 2 {
            assert!(replica.record(&document).is_err());
            assert_eq!(serde_json::to_vec(&replica).unwrap(), log_before);
            let operation = super::super::Change {
                id: super::super::Id {
                    actor: "remote".into(),
                    seq: 1,
                },
                clock: 1,
                seen: Default::default(),
                edit: super::super::Edit::Write {
                    writes: vec![super::super::Write {
                        path: "/sources".into(),
                        value: Some(Cell::Value(serde_json::json!([{"id":"opaque"}]))),
                    }],
                },
            };
            assert!(replica.receive([operation]).is_err());
            assert_eq!(serde_json::to_vec(&replica).unwrap(), log_before);
            replica.base.insert(
                "/publishing/facing_pages".into(),
                Cell::Value(Value::Bool(true)),
            );
            assert_eq!(replica.protocol_requirement().unwrap(), 3);
            let before = serde_json::to_vec(&replica).unwrap();
            assert!(replica.document_for_protocol(&document, 3).is_err());
            assert_eq!(serde_json::to_vec(&replica).unwrap(), before);
        }
        assert_eq!(serde_json::to_vec(&document).unwrap(), doc_before);
    }
}
