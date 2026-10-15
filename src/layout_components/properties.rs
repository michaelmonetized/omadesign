use super::*;
use crate::document::Fill;

/// The native field exposed by a component property.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PropertyKind {
    Text,
    Visibility,
    Fill,
    Opacity,
    Variant,
}
impl PropertyKind {
    pub fn name(self) -> &'static str {
        match self {
            Self::Text => "Text",
            Self::Visibility => "Visible",
            Self::Fill => "Fill",
            Self::Opacity => "Opacity",
            Self::Variant => "Variant",
        }
    }
}
/// A named field bound to a stable component slot, shared across variants.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ComponentProperty {
    pub id: u64,
    pub name: String,
    pub slot: u64,
    pub kind: PropertyKind,
}
/// A typed editable native field value; local edits use the normal override merge.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum PropertyValue {
    Text(String),
    Visibility(bool),
    Fill(Fill),
    Opacity(f32),
    Variant(u64),
}

/// Locate the nearest definition or instance containing an editable object.
/// Args: document, layer and object ID. Returns component root, or None.
pub fn component_ancestor(doc: &Document, layer: usize, id: u64) -> Option<u64> {
    let mut current = Some(id);
    let mut seen = HashSet::new();
    let mut instance = None;
    while let Some(id) = current {
        if !seen.insert(id) {
            return None;
        }
        let shape = doc.find_shape(layer, id)?;
        match shape.layout.component {
            Some(ComponentBinding::Main { .. }) => return Some(id),
            Some(ComponentBinding::Instance { .. }) => {
                if instance.is_none() {
                    instance = Some(id);
                }
            }
            _ => {}
        }
        current = shape.layout.parent;
    }
    instance
}
/// List named properties of a main or instance using the current variant.
/// Args: document and component root. Returns stable property definitions.
pub fn properties(doc: &Document, root: u64) -> Vec<ComponentProperty> {
    let Some((_, shape)) = locate(doc, root) else {
        return Vec::new();
    };
    match &shape.layout.component {
        Some(ComponentBinding::Main { properties, .. }) => properties.clone(),
        Some(ComponentBinding::Instance { main, .. }) => {
            match locate(doc, *main).and_then(|(_, s)| s.layout.component.as_ref()) {
                Some(ComponentBinding::Main { properties, .. }) => properties.clone(),
                _ => Vec::new(),
            }
        }
        _ => Vec::new(),
    }
}
fn target(
    doc: &Document,
    layer: usize,
    root: u64,
    property: &ComponentProperty,
) -> Result<u64, String> {
    let shape = doc
        .find_shape(layer, root)
        .ok_or("The component no longer exists")?;
    match &shape.layout.component {
        Some(ComponentBinding::Main { slots, .. }) => slots
            .iter()
            .find_map(|(id, slot)| (*slot == property.slot).then_some(*id)),
        Some(ComponentBinding::Instance { nodes, .. }) => nodes
            .iter()
            .find_map(|n| (n.slot == property.slot).then_some(n.instance)),
        _ => None,
    }
    .filter(|id| doc.find_shape(layer, *id).is_some())
    .ok_or_else(|| format!("Property '{}' has no target in this variant", property.name))
}
fn read(shape: &Shape, kind: PropertyKind) -> Result<PropertyValue, String> {
    match kind {
        PropertyKind::Text => {
            if let Geom::Text(t) = &shape.geom {
                Ok(PropertyValue::Text(t.content.clone()))
            } else {
                Err("Text properties require a text object".into())
            }
        }
        PropertyKind::Visibility => Ok(PropertyValue::Visibility(shape.visible)),
        PropertyKind::Fill => Ok(PropertyValue::Fill(shape.style.fill.clone())),
        PropertyKind::Opacity => Ok(PropertyValue::Opacity(shape.opacity)),
        PropertyKind::Variant => {
            if let Some(ComponentBinding::Instance { main, .. }) = &shape.layout.component {
                Ok(PropertyValue::Variant(*main))
            } else {
                Err("Variant properties require a nested instance".into())
            }
        }
    }
}
/// Read the current native field value, including direct user overrides.
/// Args: document, layer, component root and property ID. Returns typed value.
pub fn property_value(
    doc: &Document,
    layer: usize,
    root: u64,
    id: u64,
) -> Result<PropertyValue, String> {
    let property = properties(doc, root)
        .into_iter()
        .find(|p| p.id == id)
        .ok_or("The named property no longer exists")?;
    read(
        doc.find_shape(layer, target(doc, layer, root, &property)?)
            .unwrap(),
        property.kind,
    )
}
/// Bind a name to a real field of a definition's existing stable slot.
/// Args: document, layer, definition root, target object, name and field kind.
/// Returns the new stable property ID; invalid names, types and bindings fail atomically.
pub fn expose_property(
    doc: &mut Document,
    layer: usize,
    root: u64,
    object: u64,
    name: &str,
    kind: PropertyKind,
) -> Result<u64, String> {
    let name = name.trim();
    if name.is_empty() || name.chars().count() > 120 {
        return Err("Use a property name between 1 and 120 characters".into());
    }
    let definition = doc
        .find_shape(layer, root)
        .ok_or("The component no longer exists")?;
    let Some(ComponentBinding::Main {
        slots, properties, ..
    }) = &definition.layout.component
    else {
        return Err("Expose properties on a main component".into());
    };
    if properties.iter().any(|p| p.name == name) {
        return Err("A property already uses this name".into());
    }
    if properties.len() >= 256 {
        return Err("A component can expose at most 256 properties".into());
    }
    let slot = slots
        .get(&object)
        .copied()
        .or_else(|| {
            subtree(doc, layer, root)
                .iter()
                .any(|s| s.id == object)
                .then_some(object)
        })
        .ok_or("Choose an object inside this component")?;
    read(
        doc.find_shape(layer, object)
            .ok_or("The property target no longer exists")?,
        kind,
    )?;
    if properties.iter().any(|p| p.slot == slot && p.kind == kind) {
        return Err("This field already has a named property".into());
    }
    let id = properties
        .iter()
        .map(|p| p.id)
        .max()
        .unwrap_or(0)
        .checked_add(1)
        .ok_or("Property identity space is exhausted")?;
    let family = family_of(doc, root).unwrap();
    let property = ComponentProperty {
        id,
        name: name.into(),
        slot,
        kind,
    };
    for layer in &mut doc.layers {
        for shape in layer.kind.shapes_mut().into_iter().flatten() {
            if let Some(ComponentBinding::Main {
                family: f,
                properties,
                slots,
                ..
            }) = &mut shape.layout.component
            {
                if *f == family {
                    properties.push(property.clone());
                    if shape.id == root {
                        slots.insert(object, slot);
                    }
                }
            }
        }
    }
    Ok(id)
}
/// Rename or remove a property without changing any instance artwork.
/// Args: document, layer, main root, property ID and optional new name.
/// Returns success, or a readable validation error.
pub fn edit_property_definition(
    doc: &mut Document,
    layer: usize,
    root: u64,
    id: u64,
    name: Option<&str>,
) -> Result<(), String> {
    let Some(ComponentBinding::Main { properties, .. }) = &doc
        .find_shape(layer, root)
        .ok_or("The component no longer exists")?
        .layout
        .component
    else {
        return Err("Edit properties on a main component".into());
    };
    if !properties.iter().any(|p| p.id == id) {
        return Err("The named property no longer exists".into());
    }
    if let Some(name) = name {
        let name = name.trim();
        if name.is_empty() || name.chars().count() > 120 {
            return Err("Use a property name between 1 and 120 characters".into());
        }
        if properties.iter().any(|p| p.id != id && p.name == name) {
            return Err("A property already uses this name".into());
        }
    }
    let family = family_of(doc, root).unwrap();
    for layer in &mut doc.layers {
        for shape in layer.kind.shapes_mut().into_iter().flatten() {
            if let Some(ComponentBinding::Main {
                family: f,
                properties,
                ..
            }) = &mut shape.layout.component
            {
                if *f != family {
                    continue;
                }
                if let Some(name) = name {
                    for p in properties.iter_mut().filter(|p| p.id == id) {
                        p.name = name.trim().into();
                    }
                } else {
                    properties.retain(|p| p.id != id);
                }
            }
        }
    }
    Ok(())
}
/// Write a named value through the editable object and its existing override model.
/// Args: document, layer, component root, property ID and typed value.
/// Returns success; reset, swaps, detach and synchronization retain normal native semantics.
pub fn set_property(
    doc: &mut Document,
    layer: usize,
    root: u64,
    id: u64,
    value: PropertyValue,
) -> Result<(), String> {
    let property = properties(doc, root)
        .into_iter()
        .find(|p| p.id == id)
        .ok_or("The named property no longer exists")?;
    let target = target(doc, layer, root, &property)?;
    match (&property.kind, &value) {
        (PropertyKind::Text, PropertyValue::Text(_))
        | (PropertyKind::Visibility, PropertyValue::Visibility(_))
        | (PropertyKind::Fill, PropertyValue::Fill(_))
        | (PropertyKind::Opacity, PropertyValue::Opacity(_))
        | (PropertyKind::Variant, PropertyValue::Variant(_)) => {}
        _ => return Err("The value does not match this property type".into()),
    }
    read(doc.find_shape(layer, target).unwrap(), property.kind)?;
    if let PropertyValue::Variant(main) = value {
        return swap_variant(doc, layer, target, main).map(|_| ());
    }
    if let PropertyValue::Opacity(v) = &value {
        if !v.is_finite() || !(0.0..=1.0).contains(v) {
            return Err("Opacity must be between 0 and 1".into());
        }
    }
    let shape = doc.find_shape_mut(layer, target).unwrap();
    match value {
        PropertyValue::Text(content) => {
            if let Geom::Text(run) = &mut shape.geom {
                let len = run.content.chars().count();
                run.replace_text(0, len, &content, None);
                crate::text::fill_contours(&mut shape.geom);
            }
        }
        PropertyValue::Visibility(v) => shape.visible = v,
        PropertyValue::Fill(v) => shape.style.fill = v,
        PropertyValue::Opacity(v) => shape.opacity = v,
        PropertyValue::Variant(_) => unreachable!(),
    }
    Ok(())
}

/// Resolve a named field's actual editable object.
/// Args: document, layer, component root and property ID. Returns stable object ID.
pub fn property_object(doc: &Document, layer: usize, root: u64, id: u64) -> Result<u64, String> {
    let property = properties(doc, root)
        .into_iter()
        .find(|p| p.id == id)
        .ok_or("The named property no longer exists")?;
    target(doc, layer, root, &property)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::{Layer, Style};
    use crate::geom::TypeRun;
    fn fixture() -> (Document, u64, u64, u64) {
        let mut doc = Document::new("Named fields", 800., 600., 96.);
        doc.layers = vec![Layer::vector("UI")];
        let main = crate::layout::make_frame(Pt::ZERO, Pt::new(120., 60.));
        let root = main.id;
        let mut label = Shape::new(
            Geom::Text(TypeRun {
                content: "Café 日本語".into(),
                origin: Pt::new(10., 30.),
                ..Default::default()
            }),
            Style::default(),
        );
        label.layout.parent = Some(root);
        let child = label.id;
        crate::text::fill_contours(&mut label.geom);
        let private = crate::layout::make_frame(Pt::new(600., 500.), Pt::new(50., 50.));
        let private_id = private.id;
        doc.layers[0]
            .kind
            .shapes_mut()
            .unwrap()
            .extend([main, label, private]);
        make_component(&mut doc, 0, root).unwrap();
        (doc, root, child, private_id)
    }
    #[test]
    fn named_fields_follow_slots_override_swaps_reset_detach_and_native_reopen() {
        let (mut doc, root, child, private) = fixture();
        let untouched = doc.find_shape(0, private).unwrap().clone();
        let alternate = add_variant(&mut doc, root, "Alternate", Pt::new(0., 100.)).unwrap();
        let label = expose_property(&mut doc, 0, root, child, "Label", PropertyKind::Text).unwrap();
        let paint = expose_property(&mut doc, 0, root, child, "Ink", PropertyKind::Fill).unwrap();
        let instance = insert_instance(&mut doc, root, 0, Pt::new(180., 0.), None).unwrap();
        let other = insert_instance(&mut doc, root, 0, Pt::new(360., 0.), None).unwrap();
        let mapped = property_object(&doc, 0, instance, label).unwrap();
        set_property(
            &mut doc,
            0,
            instance,
            label,
            PropertyValue::Text("Local Ω ✨".into()),
        )
        .unwrap();
        set_property(
            &mut doc,
            0,
            instance,
            paint,
            PropertyValue::Fill(Fill::Solid(crate::color::Rgba::from_hex(0x123456))),
        )
        .unwrap();
        set_property(
            &mut doc,
            0,
            root,
            label,
            PropertyValue::Text("Updated definition".into()),
        )
        .unwrap();
        synchronize_all(&mut doc).unwrap();
        assert_eq!(
            property_value(&doc, 0, other, label).unwrap(),
            PropertyValue::Text("Updated definition".into())
        );
        let mut doc = crate::project::decode(&crate::project::encode(&doc).unwrap()).unwrap();
        swap_variant(&mut doc, 0, instance, alternate).unwrap();
        assert_eq!(property_object(&doc, 0, instance, label).unwrap(), mapped);
        assert_eq!(
            property_value(&doc, 0, instance, label).unwrap(),
            PropertyValue::Text("Local Ω ✨".into())
        );
        assert_eq!(
            property_value(&doc, 0, instance, paint).unwrap(),
            PropertyValue::Fill(Fill::Solid(crate::color::Rgba::from_hex(0x123456)))
        );
        edit_property_definition(&mut doc, 0, root, label, Some("Title")).unwrap();
        assert_eq!(
            properties(&doc, instance)
                .iter()
                .find(|p| p.id == label)
                .unwrap()
                .name,
            "Title"
        );
        sync_instance(&mut doc, 0, instance, true).unwrap();
        assert_eq!(
            property_value(&doc, 0, instance, label).unwrap(),
            PropertyValue::Text("Café 日本語".into())
        );
        set_property(
            &mut doc,
            0,
            instance,
            label,
            PropertyValue::Text("Detached".into()),
        )
        .unwrap();
        detach(&mut doc, 0, instance).unwrap();
        let Geom::Text(run) = &doc.find_shape(0, mapped).unwrap().geom else {
            panic!()
        };
        assert_eq!(run.content, "Detached");
        assert_eq!(doc.find_shape(0, private).unwrap(), &untouched);
        assert!(
            expose_property(&mut doc, 0, root, private, "Private", PropertyKind::Fill).is_err()
        );
        let ids: HashSet<_> = doc.layers[0]
            .kind
            .shapes()
            .unwrap()
            .iter()
            .map(|s| s.id)
            .collect();
        assert_eq!(ids.len(), doc.layers[0].kind.shapes().unwrap().len());
    }
    #[test]
    fn named_nested_variant_and_visibility_fields_remain_real_editable_objects() {
        let (mut doc, button, label, _) = fixture();
        let v = add_variant(&mut doc, button, "Pressed", Pt::new(0., 100.)).unwrap();
        let card = crate::layout::make_frame(Pt::new(0., 250.), Pt::new(200., 120.));
        let root = card.id;
        doc.layers[0].kind.shapes_mut().unwrap().push(card);
        let nested = insert_instance(&mut doc, button, 0, Pt::new(10., 260.), Some(root)).unwrap();
        make_component(&mut doc, 0, root).unwrap();
        let variant = expose_property(
            &mut doc,
            0,
            root,
            nested,
            "Button state",
            PropertyKind::Variant,
        )
        .unwrap();
        let visible = expose_property(
            &mut doc,
            0,
            root,
            nested,
            "Show button",
            PropertyKind::Visibility,
        )
        .unwrap();
        let instance = insert_instance(&mut doc, root, 0, Pt::new(250., 250.), None).unwrap();
        set_property(&mut doc, 0, instance, variant, PropertyValue::Variant(v)).unwrap();
        set_property(
            &mut doc,
            0,
            instance,
            visible,
            PropertyValue::Visibility(false),
        )
        .unwrap();
        synchronize_all(&mut doc).unwrap();
        let doc = crate::project::decode(&crate::project::encode(&doc).unwrap()).unwrap();
        assert_eq!(
            property_value(&doc, 0, instance, variant).unwrap(),
            PropertyValue::Variant(v)
        );
        assert_eq!(
            property_value(&doc, 0, instance, visible).unwrap(),
            PropertyValue::Visibility(false)
        );
        assert!(doc.find_shape(0, label).is_some());
    }
}

/// Validate bounded named-property metadata without deleting a missing binding.
/// Args: decoded document. Returns readable malformed-metadata errors.
pub(crate) fn validate_properties(doc: &Document) -> Result<(), String> {
    for shape in doc.layers.iter().filter_map(|l| l.kind.shapes()).flatten() {
        let Some(ComponentBinding::Main { properties, .. }) = &shape.layout.component else {
            continue;
        };
        if properties.len() > 256 {
            return Err("A component can expose at most 256 properties".into());
        }
        let mut ids = HashSet::new();
        let mut names = HashSet::new();
        let mut fields = HashSet::new();
        for p in properties {
            if p.name.trim().is_empty()
                || p.name.chars().count() > 120
                || !ids.insert(p.id)
                || !names.insert(&p.name)
                || !fields.insert((p.slot, p.kind as u8))
            {
                return Err(
                    "Component properties need unique IDs, names and valid field bindings".into(),
                );
            }
        }
    }
    Ok(())
}
