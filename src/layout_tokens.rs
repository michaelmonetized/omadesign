//! Document-owned design variables. A binding is explicit and persists across
//! save/reopen; removing a variable detaches its references at their last value.
use crate::color::Rgba;
use crate::document::{Document, Fill, Shape, Stroke, next_id};
use crate::geom::Geom;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum TokenValue {
    Color(Rgba),
    Number(f32),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DesignToken {
    pub id: u64,
    pub name: String,
    pub value: TokenValue,
}

impl DesignToken {
    pub fn color(name: impl Into<String>, color: Rgba) -> Self {
        Self {
            id: next_id(),
            name: name.into(),
            value: TokenValue::Color(color),
        }
    }
    pub fn number(name: impl Into<String>, value: f32) -> Self {
        Self {
            id: next_id(),
            name: name.into(),
            value: TokenValue::Number(value),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum TokenProperty {
    Fill,
    Stroke,
    Gap,
    Padding,
    Radius,
}
impl TokenProperty {
    pub fn name(self) -> &'static str {
        match self {
            Self::Fill => "Fill",
            Self::Stroke => "Stroke",
            Self::Gap => "Gap",
            Self::Padding => "Padding",
            Self::Radius => "Corners",
        }
    }
    pub fn all() -> [Self; 5] {
        [
            Self::Fill,
            Self::Stroke,
            Self::Gap,
            Self::Padding,
            Self::Radius,
        ]
    }
    pub fn accepts(self, value: TokenValue) -> bool {
        matches!(
            (self, value),
            (Self::Fill | Self::Stroke, TokenValue::Color(_))
                | (
                    Self::Gap | Self::Padding | Self::Radius,
                    TokenValue::Number(_)
                )
        )
    }
    pub fn supports(self, shape: &Shape) -> bool {
        match self {
            Self::Fill | Self::Stroke => true,
            Self::Gap | Self::Padding => shape.layout.frame && shape.layout.stack.is_some(),
            Self::Radius => matches!(shape.geom, Geom::Rect { .. }),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TokenBindings {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fill: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stroke: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gap: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub padding: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub radius: Option<u64>,
}
impl TokenBindings {
    pub fn is_empty(&self) -> bool {
        TokenProperty::all().iter().all(|&p| self.get(p).is_none())
    }
    pub fn get(self, property: TokenProperty) -> Option<u64> {
        match property {
            TokenProperty::Fill => self.fill,
            TokenProperty::Stroke => self.stroke,
            TokenProperty::Gap => self.gap,
            TokenProperty::Padding => self.padding,
            TokenProperty::Radius => self.radius,
        }
    }
    pub fn set(&mut self, property: TokenProperty, id: Option<u64>) {
        *match property {
            TokenProperty::Fill => &mut self.fill,
            TokenProperty::Stroke => &mut self.stroke,
            TokenProperty::Gap => &mut self.gap,
            TokenProperty::Padding => &mut self.padding,
            TokenProperty::Radius => &mut self.radius,
        } = id;
    }
}

pub fn validate_token_list(tokens: &[DesignToken]) -> Result<(), String> {
    let mut ids = HashSet::new();
    let mut names = HashSet::new();
    for token in tokens {
        if token.id == 0 || !ids.insert(token.id) {
            return Err("Design variables must have unique nonzero identities".into());
        }
        let name = token.name.trim();
        if name.is_empty() || name.len() > 128 {
            return Err("Name design variables using 1–128 characters".into());
        }
        if !names.insert(name.to_lowercase()) {
            return Err(format!("A design variable named {name} already exists"));
        }
        if let TokenValue::Number(value) = token.value
            && (!value.is_finite() || !(0.0..=1_000_000.0).contains(&value))
        {
            return Err(format!(
                "{} needs a number between 0 and 1000000",
                token.name
            ));
        }
    }
    Ok(())
}

pub fn validate_tokens(doc: &Document) -> Result<(), String> {
    validate_token_list(&doc.layout_tokens)?;
    let tokens: HashMap<_, _> = doc.layout_tokens.iter().map(|t| (t.id, t)).collect();
    for layer in &doc.layers {
        for shape in layer.kind.shapes().unwrap_or_default() {
            for property in TokenProperty::all() {
                let Some(id) = shape.layout.tokens.get(property) else {
                    continue;
                };
                let token = tokens.get(&id).ok_or_else(|| {
                    format!("{} references a missing design variable", shape.name)
                })?;
                if !property.accepts(token.value) {
                    return Err(format!(
                        "{} cannot use {} for {}",
                        shape.name,
                        token.name,
                        property.name()
                    ));
                }
                // Bindings remain valid when auto layout is disabled or a shape
                // is temporarily converted; re-enabling the property restores
                // the same variable without destroying design intent.
            }
        }
    }
    Ok(())
}

/// Link selected objects atomically. Unlinking preserves their current values.
pub fn bind(
    doc: &mut Document,
    layer: usize,
    ids: &[u64],
    property: TokenProperty,
    token: Option<u64>,
) -> Result<(), String> {
    if let Some(id) = token {
        let token = doc
            .layout_tokens
            .iter()
            .find(|t| t.id == id)
            .ok_or("Design variable no longer exists")?;
        if !property.accepts(token.value) {
            return Err(format!(
                "Choose a {} variable for {}",
                if matches!(property, TokenProperty::Fill | TokenProperty::Stroke) {
                    "color"
                } else {
                    "number"
                },
                property.name()
            ));
        }
    }
    for &id in ids {
        let shape = doc
            .find_shape(layer, id)
            .ok_or("The selected object no longer exists")?;
        if token.is_some() && !property.supports(shape) {
            return Err(format!(
                "{} does not support a {} variable",
                shape.name,
                property.name()
            ));
        }
    }
    for &id in ids {
        if let Some(shape) = doc.find_shape_mut(layer, id) {
            shape.layout.tokens.set(property, token);
        }
    }
    apply_tokens(doc);
    Ok(())
}

fn resolve_shape(shape: &mut Shape, tokens: &HashMap<u64, TokenValue>) -> bool {
    let mut changed = false;
    for property in TokenProperty::all() {
        let Some(value) = shape
            .layout
            .tokens
            .get(property)
            .and_then(|id| tokens.get(&id))
            .copied()
        else {
            continue;
        };
        match (property, value) {
            (TokenProperty::Fill, TokenValue::Color(color)) => {
                let fill = Fill::Solid(color);
                if shape.style.fill != fill {
                    shape.style.fill = fill;
                    changed = true;
                }
            }
            (TokenProperty::Stroke, TokenValue::Color(color)) => {
                if shape.style.stroke.as_ref().is_none_or(|s| s.color != color) {
                    let stroke = shape.style.stroke.get_or_insert_with(Stroke::default);
                    stroke.color = color;
                    changed = true;
                }
            }
            (TokenProperty::Gap, TokenValue::Number(value))
                if value.is_finite() && value >= 0.0 =>
            {
                if let Some(stack) = &mut shape.layout.stack
                    && (stack.gap != value || stack.cross_gap != value)
                {
                    stack.gap = value;
                    stack.cross_gap = value;
                    changed = true;
                }
            }
            (TokenProperty::Padding, TokenValue::Number(value))
                if value.is_finite() && value >= 0.0 =>
            {
                if let Some(stack) = &mut shape.layout.stack
                    && stack.padding != [value; 4]
                {
                    stack.padding = [value; 4];
                    changed = true;
                }
            }
            (TokenProperty::Radius, TokenValue::Number(value))
                if value.is_finite() && value >= 0.0 =>
            {
                if let Geom::Rect { radius, .. } = &mut shape.geom
                    && (*radius != value || shape.corners != [value; 4])
                {
                    *radius = value;
                    shape.corners = [value; 4];
                    changed = true;
                }
            }
            _ => {}
        }
    }
    changed
}

/// Resolve variables into the existing render/layout properties in a single
/// indexed pass. Rendering needs no token lookup, and undo records real values.
pub fn apply_tokens(doc: &mut Document) -> usize {
    if doc.layout_tokens.is_empty() {
        return 0;
    }
    let tokens: HashMap<_, _> = doc.layout_tokens.iter().map(|t| (t.id, t.value)).collect();
    let mut count = 0;
    for layer in &mut doc.layers {
        let Some(shapes) = layer.kind.shapes_mut() else {
            continue;
        };
        for shape in shapes {
            if resolve_shape(shape, &tokens) {
                count += 1;
            }
        }
    }
    count
}

/// Detach every reference before removing a variable. Materialized geometry and
/// colors are retained so deleting a library entry cannot alter the artwork.
pub fn remove_token(doc: &mut Document, id: u64) -> Result<DesignToken, String> {
    let index = doc
        .layout_tokens
        .iter()
        .position(|t| t.id == id)
        .ok_or("Design variable no longer exists")?;
    apply_tokens(doc);
    for layer in &mut doc.layers {
        let Some(shapes) = layer.kind.shapes_mut() else {
            continue;
        };
        for shape in shapes {
            for property in TokenProperty::all() {
                if shape.layout.tokens.get(property) == Some(id) {
                    shape.layout.tokens.set(property, None);
                }
            }
        }
    }
    Ok(doc.layout_tokens.remove(index))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::{Cmd, apply};
    use crate::geom::Pt;
    use crate::layout::{self, AutoStack};

    fn setup() -> (Document, u64) {
        let mut doc = Document::new("Design system", 800.0, 600.0, 72.0);
        let mut frame = layout::make_frame(Pt::ZERO, Pt::new(300.0, 200.0));
        frame.layout.stack = Some(AutoStack::default());
        let id = frame.id;
        apply(
            &mut doc,
            &Cmd::AddShape {
                layer: 1,
                shape: frame,
            },
        );
        (doc, id)
    }

    #[test]
    fn linked_color_and_spacing_changes_materialize_and_are_idempotent() {
        let (mut doc, frame) = setup();
        let color = DesignToken::color("Brand/Primary", Rgba::rgb(10, 40, 240));
        let space = DesignToken::number("Space/Comfortable", 24.0);
        let (color_id, space_id) = (color.id, space.id);
        doc.layout_tokens = vec![color, space];
        bind(&mut doc, 1, &[frame], TokenProperty::Fill, Some(color_id)).unwrap();
        bind(
            &mut doc,
            1,
            &[frame],
            TokenProperty::Padding,
            Some(space_id),
        )
        .unwrap();
        bind(&mut doc, 1, &[frame], TokenProperty::Gap, Some(space_id)).unwrap();
        bind(&mut doc, 1, &[frame], TokenProperty::Radius, Some(space_id)).unwrap();
        doc.layout_tokens[0].value = TokenValue::Color(Rgba::rgb(200, 40, 20));
        doc.layout_tokens[1].value = TokenValue::Number(16.0);
        assert_eq!(apply_tokens(&mut doc), 1);
        let shape = doc.find_shape(1, frame).unwrap();
        assert_eq!(shape.style.fill, Fill::Solid(Rgba::rgb(200, 40, 20)));
        assert_eq!(shape.layout.stack.as_ref().unwrap().padding, [16.0; 4]);
        assert_eq!(shape.layout.stack.as_ref().unwrap().gap, 16.0);
        assert_eq!(shape.corners, [16.0; 4]);
        assert_eq!(apply_tokens(&mut doc), 0);
    }

    #[test]
    fn type_mismatch_rejects_binding_without_partial_changes() {
        let (mut doc, frame) = setup();
        let color = DesignToken::color("Brand", Rgba::BLACK);
        let id = color.id;
        doc.layout_tokens.push(color);
        assert!(bind(&mut doc, 1, &[frame], TokenProperty::Padding, Some(id)).is_err());
        assert!(doc.find_shape(1, frame).unwrap().layout.tokens.is_empty());
        assert!(
            bind(
                &mut doc,
                1,
                &[frame, u64::MAX],
                TokenProperty::Fill,
                Some(id)
            )
            .is_err()
        );
        assert!(doc.find_shape(1, frame).unwrap().layout.tokens.is_empty());
    }

    #[test]
    fn removing_a_variable_detaches_references_without_changing_rendered_values() {
        let (mut doc, frame) = setup();
        let token = DesignToken::color("Ink", Rgba::rgb(20, 24, 30));
        let id = token.id;
        doc.layout_tokens.push(token);
        bind(&mut doc, 1, &[frame], TokenProperty::Fill, Some(id)).unwrap();
        let fill = doc.find_shape(1, frame).unwrap().style.fill.clone();
        remove_token(&mut doc, id).unwrap();
        assert_eq!(doc.find_shape(1, frame).unwrap().style.fill, fill);
        assert!(doc.find_shape(1, frame).unwrap().layout.tokens.is_empty());
        assert!(validate_tokens(&doc).is_ok());
    }

    #[test]
    fn save_reopen_preserves_variable_links_and_shared_updates() {
        let (mut doc, frame) = setup();
        let token = DesignToken::number("Space", 12.0);
        let id = token.id;
        doc.layout_tokens.push(token);
        bind(&mut doc, 1, &[frame], TokenProperty::Padding, Some(id)).unwrap();
        let data = serde_json::to_vec(&doc).unwrap();
        let mut reopened: Document = serde_json::from_slice(&data).unwrap();
        assert!(validate_tokens(&reopened).is_ok());
        reopened.layout_tokens[0].value = TokenValue::Number(32.0);
        apply_tokens(&mut reopened);
        assert_eq!(
            reopened
                .find_shape(1, frame)
                .unwrap()
                .layout
                .stack
                .as_ref()
                .unwrap()
                .padding,
            [32.0; 4]
        );
    }

    #[test]
    fn invalid_variable_data_is_rejected() {
        let (mut doc, _) = setup();
        doc.layout_tokens = vec![DesignToken::number("Gap", -2.0)];
        assert!(validate_tokens(&doc).is_err());
        doc.layout_tokens = vec![
            DesignToken::number("Gap", 2.0),
            DesignToken::number(" gap ", 3.0),
        ];
        assert!(validate_tokens(&doc).is_err());
    }
}
