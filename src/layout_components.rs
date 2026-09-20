//! Reusable design components. Instances retain stable object IDs and track
//! property overrides against the last received definition, including after save.

use crate::document::{Document, Shape, next_id};
use crate::geom::{Geom, Pt};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum ComponentBinding {
    Main {
        family: u64,
        variant: String,
        /// Local shape ID -> identity shared by corresponding variant children.
        #[serde(default)]
        slots: BTreeMap<u64, u64>,
    },
    Instance {
        main: u64,
        nodes: Vec<InstanceNode>,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct InstanceNode {
    pub source: u64,
    pub instance: u64,
    pub slot: u64,
    /// Last inherited definition for a nested instance; separates source edits
    /// from an explicit variant override inside the containing instance.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nested_main: Option<u64>,
    /// Last inherited shape in component-relative coordinates. Its component
    /// binding is cleared, so snapshots cannot recursively contain themselves.
    pub baseline: Box<Shape>,
}

impl ComponentBinding {
    pub fn name(&self) -> &'static str {
        match self {
            Self::Main { .. } => "Component",
            Self::Instance { .. } => "Instance",
        }
    }
}

fn locate(doc: &Document, id: u64) -> Option<(usize, &Shape)> {
    doc.layers.iter().enumerate().find_map(|(li, layer)| {
        layer
            .kind
            .shapes()?
            .iter()
            .find(|s| s.id == id)
            .map(|s| (li, s))
    })
}

/// Paint-ordered subtree, bounded even while inspecting an invalid document.
fn subtree(doc: &Document, layer: usize, root: u64) -> Vec<Shape> {
    let Some(shapes) = doc.layers.get(layer).and_then(|l| l.kind.shapes()) else {
        return vec![];
    };
    let mut members = HashSet::from([root]);
    let mut children: HashMap<u64, Vec<u64>> = HashMap::new();
    for s in shapes {
        if let Some(parent) = s.layout.parent {
            children.entry(parent).or_default().push(s.id);
        }
    }
    let mut todo = vec![root];
    while let Some(id) = todo.pop() {
        for child in children.get(&id).into_iter().flatten() {
            if members.insert(*child) {
                todo.push(*child);
            }
        }
    }
    shapes
        .iter()
        .filter(|s| members.contains(&s.id))
        .cloned()
        .collect()
}

pub fn definitions(doc: &Document) -> Vec<(usize, u64, String, String)> {
    doc.layers
        .iter()
        .enumerate()
        .flat_map(|(li, l)| {
            l.kind
                .shapes()
                .unwrap_or_default()
                .iter()
                .filter_map(move |s| {
                    if let Some(ComponentBinding::Main { variant, .. }) = &s.layout.component {
                        Some((li, s.id, s.name.clone(), variant.clone()))
                    } else {
                        None
                    }
                })
        })
        .collect()
}

pub fn family_of(doc: &Document, main: u64) -> Option<u64> {
    match locate(doc, main)?.1.layout.component.as_ref()? {
        ComponentBinding::Main { family, .. } => Some(*family),
        ComponentBinding::Instance { main, .. } => family_of_main(doc, *main),
    }
}

fn family_of_main(doc: &Document, id: u64) -> Option<u64> {
    match locate(doc, id)?.1.layout.component.as_ref()? {
        ComponentBinding::Main { family, .. } => Some(*family),
        _ => None,
    }
}

pub fn instance_ancestor(doc: &Document, layer: usize, id: u64) -> Option<u64> {
    let mut current = Some(id);
    let mut seen = HashSet::new();
    while let Some(id) = current {
        if !seen.insert(id) {
            return None;
        }
        let shape = doc.find_shape(layer, id)?;
        if matches!(
            shape.layout.component,
            Some(ComponentBinding::Instance { .. })
        ) {
            return Some(id);
        }
        current = shape.layout.parent;
    }
    None
}

pub fn make_component(doc: &mut Document, layer: usize, root: u64) -> Result<(), String> {
    let frame = doc.find_shape(layer, root).ok_or("Select a frame")?;
    if !frame.layout.frame {
        return Err("Components start with a frame".into());
    }
    if frame.layout.component.is_some() {
        return Err("This frame is already a component or instance".into());
    }
    let slots = subtree(doc, layer, root)
        .iter()
        .map(|s| (s.id, s.id))
        .collect();
    doc.find_shape_mut(layer, root).unwrap().layout.component = Some(ComponentBinding::Main {
        family: root,
        variant: "Default".into(),
        slots,
    });
    Ok(())
}

pub fn remap_interactions(shape: &mut Shape, ids: &HashMap<u64, u64>) {
    use crate::layout_prototype::PrototypeAction;
    for interaction in &mut shape.layout.interactions {
        match &mut interaction.action {
            PrototypeAction::Navigate { target } | PrototypeAction::OpenOverlay { target } => {
                if let Some(mapped) = ids.get(target) {
                    *target = *mapped;
                }
            }
            PrototypeAction::SetVariant { component } => {
                if let Some(mapped) = ids.get(component) {
                    *component = *mapped;
                }
            }
            _ => {}
        }
    }
}

/// Call when duplicating a complete subtree, after remapping its root shape ID.
/// Snapshot coordinates are component-relative, so they must not be translated.
pub fn remap_duplicate(shape: &mut Shape, ids: &HashMap<u64, u64>) {
    remap_interactions(shape, ids);
    let Some(binding) = &mut shape.layout.component else {
        return;
    };
    match binding {
        ComponentBinding::Main { family, slots, .. } => {
            *family = ids.get(family).copied().unwrap_or(shape.id);
            *slots = std::mem::take(slots)
                .into_iter()
                .map(|(source, slot)| {
                    (
                        ids.get(&source).copied().unwrap_or(source),
                        ids.get(&slot).copied().unwrap_or(slot),
                    )
                })
                .collect();
        }
        ComponentBinding::Instance { main, nodes } => {
            let cloned_main = ids.get(main).copied();
            if let Some(main_id) = cloned_main {
                *main = main_id;
            }
            for node in nodes {
                node.instance = ids.get(&node.instance).copied().unwrap_or(node.instance);
                node.baseline.id = node.instance;
                node.nested_main = node
                    .nested_main
                    .map(|main| ids.get(&main).copied().unwrap_or(main));
                if cloned_main.is_some() {
                    node.source = ids.get(&node.source).copied().unwrap_or(node.source);
                    node.slot = ids.get(&node.slot).copied().unwrap_or(node.slot);
                }
                node.baseline.layout.parent = node
                    .baseline
                    .layout
                    .parent
                    .map(|p| ids.get(&p).copied().unwrap_or(p));
                remap_interactions(&mut node.baseline, ids);
            }
        }
    }
}

fn relative(mut shape: Shape, origin: Pt) -> Shape {
    shape.geom.translate(Pt::ZERO - origin);
    shape.layout.component = None;
    shape
}

fn depends_on(doc: &Document, main: u64, target: u64, seen: &mut HashSet<u64>) -> bool {
    if main == target {
        return true;
    }
    if !seen.insert(main) {
        return false;
    }
    let Some((layer, _)) = locate(doc, main) else {
        return false;
    };
    subtree(doc, layer, main).iter().any(|shape| {
        if let Some(ComponentBinding::Instance { main, .. }) = &shape.layout.component {
            depends_on(doc, *main, target, seen)
        } else {
            false
        }
    })
}

pub fn insert_instance(
    doc: &mut Document,
    main: u64,
    layer: usize,
    origin: Pt,
    parent: Option<u64>,
) -> Result<u64, String> {
    if !origin.x.is_finite() || !origin.y.is_finite() {
        return Err("Instance position must be finite".into());
    }
    if doc
        .layers
        .get(layer)
        .and_then(|l| l.kind.shapes())
        .is_none()
    {
        return Err("Choose a vector layer".into());
    }
    if let Some(parent) = parent {
        let mut cursor = Some(parent);
        let mut seen = HashSet::new();
        while let Some(id) = cursor {
            if !seen.insert(id) || id == main {
                return Err("A component cannot contain an instance of itself".into());
            }
            let shape = doc.find_shape(layer, id).ok_or("Parent frame is missing")?;
            if !shape.layout.frame {
                return Err("Instance parent must be a frame".into());
            }
            if matches!(shape.layout.component, Some(ComponentBinding::Main { .. }))
                && depends_on(doc, main, id, &mut HashSet::new())
            {
                return Err("This would create a circular component dependency".into());
            }
            cursor = shape.layout.parent;
        }
    }
    let (source_layer, source) = locate(doc, main).ok_or("Component definition is missing")?;
    let Some(ComponentBinding::Main { slots, .. }) = &source.layout.component else {
        return Err("Choose a main component".into());
    };
    let slots = slots.clone();
    let source_origin = source.geom.bbox().min;
    let source_shapes = subtree(doc, source_layer, main);
    let ids: HashMap<u64, u64> = source_shapes.iter().map(|s| (s.id, next_id())).collect();
    let root = ids[&main];
    let mut nodes = Vec::new();
    let mut shapes = Vec::new();
    for source in source_shapes {
        let source_id = source.id;
        let mut shape = source.clone();
        shape.id = ids[&source_id];
        shape.layout.parent = if source_id == main {
            parent
        } else {
            shape.layout.parent.and_then(|p| ids.get(&p).copied())
        };
        if source_id == main
            || !matches!(
                shape.layout.component,
                Some(ComponentBinding::Instance { .. })
            )
        {
            shape.layout.component = None;
        }
        shape.geom.translate(origin - source_origin);
        remap_duplicate(&mut shape, &ids);
        let baseline = relative(shape.clone(), origin);
        nodes.push(InstanceNode {
            source: source_id,
            instance: shape.id,
            slot: slots.get(&source_id).copied().unwrap_or(source_id),
            nested_main: match shape.layout.component {
                Some(ComponentBinding::Instance { main, .. }) => Some(main),
                _ => None,
            },
            baseline: Box::new(baseline),
        });
        shapes.push(shape);
    }
    shapes
        .iter_mut()
        .find(|s| s.id == root)
        .unwrap()
        .layout
        .component = Some(ComponentBinding::Instance { main, nodes });
    doc.layers[layer].kind.shapes_mut().unwrap().extend(shapes);
    Ok(root)
}

/// Duplicate a definition as a named variant. Shared slots retain overrides
/// when switching variants, even after variant children have been reordered.
pub fn add_variant(doc: &mut Document, main: u64, name: &str, origin: Pt) -> Result<u64, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("Give the variant a name".into());
    }
    let (layer, source) = locate(doc, main).ok_or("Component is missing")?;
    let Some(ComponentBinding::Main { family, slots, .. }) = &source.layout.component else {
        return Err("Select a main component".into());
    };
    let (family, slots) = (*family, slots.clone());
    if definitions(doc)
        .iter()
        .any(|(_, id, _, variant)| family_of_main(doc, *id) == Some(family) && variant == name)
    {
        return Err("A variant with that name already exists".into());
    }
    let old_origin = source.geom.bbox().min;
    let source = subtree(doc, layer, main);
    let ids: HashMap<u64, u64> = source.iter().map(|s| (s.id, next_id())).collect();
    let root = ids[&main];
    let new_slots = source
        .iter()
        .map(|s| (ids[&s.id], slots.get(&s.id).copied().unwrap_or(s.id)))
        .collect();
    let mut shapes: Vec<_> = source
        .into_iter()
        .map(|mut s| {
            let old_id = s.id;
            s.id = ids[&old_id];
            s.geom.translate(origin - old_origin);
            s.layout.parent = if old_id == main {
                None
            } else {
                s.layout.parent.and_then(|p| ids.get(&p).copied())
            };
            if old_id == main
                || !matches!(s.layout.component, Some(ComponentBinding::Instance { .. }))
            {
                s.layout.component = None;
            }
            remap_duplicate(&mut s, &ids);
            s
        })
        .collect();
    let frame = shapes.iter_mut().find(|s| s.id == root).unwrap();
    frame.name = format!("{} / {}", frame.name, name);
    frame.layout.component = Some(ComponentBinding::Main {
        family,
        variant: name.into(),
        slots: new_slots,
    });
    doc.layers[layer].kind.shapes_mut().unwrap().extend(shapes);
    Ok(root)
}

fn preserve<T: PartialEq + Clone>(base: &T, local: &T, inherited: &T, reset: bool) -> T {
    if !reset && base != local {
        local.clone()
    } else {
        inherited.clone()
    }
}

// Geometries and layout settings merge by property, not whole text blocks. A
// content override can coexist with inherited font size, alignment and width.
fn merge_value(
    base: &serde_json::Value,
    local: &serde_json::Value,
    inherited: &serde_json::Value,
) -> serde_json::Value {
    if base == local {
        return inherited.clone();
    }
    if let (Some(b), Some(l), Some(n)) =
        (base.as_object(), local.as_object(), inherited.as_object())
    {
        let mut out = n.clone();
        for key in b.keys().chain(l.keys()).collect::<HashSet<_>>() {
            let bv = b.get(key).unwrap_or(&serde_json::Value::Null);
            let lv = l.get(key).unwrap_or(&serde_json::Value::Null);
            if bv != lv {
                if lv.is_null() && !l.contains_key(key) {
                    out.remove(key);
                } else {
                    out.insert(
                        key.clone(),
                        merge_value(bv, lv, n.get(key).unwrap_or(&serde_json::Value::Null)),
                    );
                }
            }
        }
        serde_json::Value::Object(out)
    } else {
        local.clone()
    }
}

fn merge_serialized<T: Serialize + for<'de> Deserialize<'de> + Clone + PartialEq>(
    base: &T,
    local: &T,
    inherited: &T,
    reset: bool,
) -> T {
    if reset || base == local {
        return inherited.clone();
    }
    if local == inherited {
        return local.clone();
    }
    let Ok(b) = serde_json::to_value(base) else {
        return local.clone();
    };
    let Ok(l) = serde_json::to_value(local) else {
        return local.clone();
    };
    let Ok(n) = serde_json::to_value(inherited) else {
        return local.clone();
    };
    serde_json::from_value(merge_value(&b, &l, &n)).unwrap_or_else(|_| local.clone())
}

fn inherit(base: &Shape, local: &Shape, inherited: &Shape, reset: bool) -> Shape {
    let mut out = inherited.clone();
    out.name = preserve(&base.name, &local.name, &inherited.name, reset);
    out.style = merge_serialized(&base.style, &local.style, &inherited.style, reset);
    out.opacity = preserve(&base.opacity, &local.opacity, &inherited.opacity, reset);
    out.blend = preserve(&base.blend, &local.blend, &inherited.blend, reset);
    out.rotation = preserve(&base.rotation, &local.rotation, &inherited.rotation, reset);
    out.corners = preserve(&base.corners, &local.corners, &inherited.corners, reset);
    out.filters = preserve(&base.filters, &local.filters, &inherited.filters, reset);
    out.visible = preserve(&base.visible, &local.visible, &inherited.visible, reset);
    out.locked = preserve(&base.locked, &local.locked, &inherited.locked, reset);
    out.guide = preserve(&base.guide, &local.guide, &inherited.guide, reset);
    let (mut base_layout, mut local_layout, mut inherited_layout) = (
        base.layout.clone(),
        local.layout.clone(),
        inherited.layout.clone(),
    );
    // Image payloads are shared immutable bytes. Never expand those potentially
    // large strings into JSON just to merge a padding or constraint override.
    base_layout.image = None;
    local_layout.image = None;
    inherited_layout.image = None;
    out.layout = merge_serialized(&base_layout, &local_layout, &inherited_layout, reset);
    out.layout.image = match (
        &base.layout.image,
        &local.layout.image,
        &inherited.layout.image,
    ) {
        (Some(base), Some(local), Some(inherited)) => Some(crate::layout_images::ImageFill {
            data: preserve(&base.data, &local.data, &inherited.data, reset),
            fit: preserve(&base.fit, &local.fit, &inherited.fit, reset),
            focal: preserve(&base.focal, &local.focal, &inherited.focal, reset),
        }),
        _ => preserve(
            &base.layout.image,
            &local.layout.image,
            &inherited.layout.image,
            reset,
        ),
    };
    // Definition hierarchy remains authoritative. Moving an instance is handled
    // by its root anchor, while local children can be added without losing them.
    out.layout.parent = inherited.layout.parent;
    out.layout.component = None;
    out.geom = match (&base.geom, &local.geom, &inherited.geom) {
        (Geom::Text(_), Geom::Text(_), Geom::Text(_)) => {
            merge_serialized(&base.geom, &local.geom, &inherited.geom, reset)
        }
        _ => preserve(&base.geom, &local.geom, &inherited.geom, reset),
    };
    if matches!(out.geom, Geom::Text(_)) {
        crate::text::fill_contours(&mut out.geom);
    }
    out
}

fn merge_nested_binding(
    previous: Option<ComponentBinding>,
    inherited: Option<ComponentBinding>,
    baseline_main: Option<u64>,
) -> Option<ComponentBinding> {
    match (previous, inherited) {
        (
            Some(ComponentBinding::Instance { main, nodes }),
            Some(ComponentBinding::Instance {
                main: inherited_main,
                nodes: inherited_nodes,
            }),
        ) if main == baseline_main.unwrap_or(inherited_main) || main == inherited_main => {
            let mut previous: BTreeMap<_, _> =
                nodes.into_iter().map(|node| (node.slot, node)).collect();
            let mut merged = Vec::with_capacity(inherited_nodes.len());
            for inherited in inherited_nodes {
                if let Some(mut previous) = previous.remove(&inherited.slot) {
                    // Existing nodes keep their comparison baseline so a local
                    // text/style override is still detected by the inner sync.
                    previous.instance = inherited.instance;
                    previous.baseline.id = inherited.instance;
                    merged.push(previous);
                } else {
                    // The containing sync already inserted this shape. Give
                    // the nested instance its mapping now, so its next sync
                    // cannot add the same inherited child under another ID.
                    merged.push(inherited);
                }
            }
            // Keep removed-node mappings until inner sync can remove their old
            // instances and repair any locally added children parented to them.
            merged.extend(previous.into_values());
            Some(ComponentBinding::Instance {
                main: inherited_main,
                nodes: merged,
            })
        }
        (Some(previous), _) => Some(previous),
        (None, inherited) => inherited,
    }
}

pub fn sync_instance(
    doc: &mut Document,
    layer: usize,
    root: u64,
    reset: bool,
) -> Result<bool, String> {
    let frame = doc.find_shape(layer, root).ok_or("Instance is missing")?;
    let Some(ComponentBinding::Instance { main, .. }) = &frame.layout.component else {
        return Err("Select an instance".into());
    };
    sync_to(doc, layer, root, *main, reset)
}

pub fn swap_variant(
    doc: &mut Document,
    layer: usize,
    root: u64,
    main: u64,
) -> Result<bool, String> {
    let frame = doc.find_shape(layer, root).ok_or("Instance is missing")?;
    let Some(ComponentBinding::Instance { main: previous, .. }) = &frame.layout.component else {
        return Err("Select an instance".into());
    };
    if family_of_main(doc, *previous).is_none()
        || family_of_main(doc, *previous) != family_of_main(doc, main)
    {
        return Err("Choose a variant from this component family".into());
    }
    sync_to(doc, layer, root, main, false)
}

fn sync_to(
    doc: &mut Document,
    layer: usize,
    root: u64,
    main: u64,
    reset: bool,
) -> Result<bool, String> {
    let frame = doc.find_shape(layer, root).ok_or("Instance is missing")?;
    let Some(ComponentBinding::Instance { nodes, .. }) = &frame.layout.component else {
        return Err("Select an instance".into());
    };
    let nodes = nodes.clone();
    let origin = frame.geom.bbox().min;
    let parent = frame.layout.parent;
    let (source_layer, source_frame) = locate(doc, main)
        .ok_or("Component definition is missing; the instance remains editable")?;
    let Some(ComponentBinding::Main { slots, .. }) = &source_frame.layout.component else {
        return Err("Component definition is no longer a main component".into());
    };
    let slots = slots.clone();
    let source_origin = source_frame.geom.bbox().min;
    let source = subtree(doc, source_layer, main);
    let previous: HashMap<u64, &InstanceNode> = nodes.iter().map(|n| (n.slot, n)).collect();
    let mut ids = HashMap::new();
    for shape in &source {
        let slot = slots.get(&shape.id).copied().unwrap_or(shape.id);
        let id = if shape.id == main {
            root
        } else {
            previous
                .get(&slot)
                .map(|n| n.instance)
                .unwrap_or_else(next_id)
        };
        ids.insert(shape.id, id);
    }
    let mut protected_source = HashSet::new();
    let mut protected_roots = HashSet::new();
    let mut protected_local = HashMap::new();
    if !reset {
        for shape in &source {
            let Some(ComponentBinding::Instance {
                main: inherited_main,
                ..
            }) = &shape.layout.component
            else {
                continue;
            };
            let slot = slots.get(&shape.id).copied().unwrap_or(shape.id);
            let Some(previous) = previous.get(&slot) else {
                continue;
            };
            let Some(local) = doc.find_shape(layer, previous.instance) else {
                continue;
            };
            let Some(ComponentBinding::Instance {
                main: local_main, ..
            }) = &local.layout.component
            else {
                continue;
            };
            if *local_main != previous.nested_main.unwrap_or(*inherited_main)
                && local_main != inherited_main
            {
                protected_roots.insert(shape.id);
                protected_source.extend(
                    subtree(doc, source_layer, shape.id)
                        .into_iter()
                        .filter(|s| s.id != shape.id)
                        .map(|s| s.id),
                );
                protected_local.extend(
                    subtree(doc, layer, local.id)
                        .into_iter()
                        .filter(|s| s.id != local.id)
                        .map(|s| (s.id, s)),
                );
            }
        }
    }
    let mut updated = Vec::new();
    let mut new_nodes = Vec::new();
    for source in source {
        let source_id = source.id;
        if protected_source.contains(&source_id) {
            continue;
        }
        let nested_binding = if source_id != main
            && matches!(
                source.layout.component,
                Some(ComponentBinding::Instance { .. })
            ) {
            source.layout.component.clone()
        } else {
            None
        };
        let slot = slots.get(&source_id).copied().unwrap_or(source_id);
        let nested_main = match &nested_binding {
            Some(ComponentBinding::Instance { main, .. }) => Some(*main),
            _ => None,
        };
        let mut inherited = relative(source, source_origin);
        inherited.id = ids[&source_id];
        inherited.layout.parent = if source_id == main {
            parent
        } else {
            inherited.layout.parent.and_then(|p| ids.get(&p).copied())
        };
        remap_interactions(&mut inherited, &ids);
        let locally_deleted = !reset
            && source_id != main
            && previous
                .get(&slot)
                .is_some_and(|node| doc.find_shape(layer, node.instance).is_none());
        let mut final_shape = if let Some(node) = previous.get(&slot)
            && let Some(local) = doc.find_shape(layer, node.instance)
        {
            inherit(
                &node.baseline,
                &relative(local.clone(), origin),
                &inherited,
                reset,
            )
        } else {
            inherited.clone()
        };
        final_shape.geom.translate(origin);
        if protected_roots.contains(&source_id)
            && let Some(local) = doc.find_shape(layer, final_shape.id)
        {
            let geometry = final_shape.geom;
            let parent = final_shape.layout.parent;
            final_shape = local.clone();
            final_shape.geom = geometry;
            final_shape.layout.parent = parent;
        }
        if let Some(binding) = nested_binding {
            // Nested instances are still instances, with their own editable
            // overrides and variant controls inside the containing instance.
            let previous_binding = doc
                .find_shape(layer, final_shape.id)
                .and_then(|s| s.layout.component.clone());
            let mut remapped = inherited.clone();
            remapped.layout.component = Some(binding);
            remap_duplicate(&mut remapped, &ids);
            final_shape.layout.component = if reset {
                remapped.layout.component
            } else {
                merge_nested_binding(
                    previous_binding,
                    remapped.layout.component,
                    previous.get(&slot).and_then(|node| node.nested_main),
                )
            };
        }
        new_nodes.push(InstanceNode {
            source: source_id,
            instance: inherited.id,
            slot,
            nested_main,
            baseline: Box::new(inherited),
        });
        if !locally_deleted {
            updated.push(final_shape);
        }
    }
    new_nodes.extend(
        nodes
            .iter()
            .filter(|node| protected_local.contains_key(&node.instance))
            .cloned(),
    );
    // Preserve painter order for the selected local variant's actual subtree.
    if !protected_local.is_empty() {
        updated.extend(
            doc.layers[layer]
                .kind
                .shapes()
                .unwrap()
                .iter()
                .filter(|s| protected_local.contains_key(&s.id))
                .cloned(),
        );
    }
    let removed: HashSet<_> = nodes
        .iter()
        .map(|n| n.instance)
        .filter(|id| !updated.iter().any(|s| s.id == *id))
        .collect();
    let tracked: HashSet<_> = nodes.iter().map(|n| n.instance).collect();
    let represented: HashSet<_> = updated.iter().map(|s| s.id).collect();
    let originals = doc.layers[layer].kind.shapes().unwrap();
    // Retain user-created children. If their inherited parent was removed,
    // reparent them to the instance instead of leaving a dangling relationship.
    let mut local_children: Vec<_> = subtree(doc, layer, root)
        .into_iter()
        .filter(|s| !tracked.contains(&s.id) && !represented.contains(&s.id))
        .collect();
    for s in &mut local_children {
        if s.layout.parent.is_some_and(|p| removed.contains(&p)) {
            s.layout.parent = Some(root);
        }
    }
    updated.extend(local_children);
    updated
        .iter_mut()
        .find(|s| s.id == root)
        .unwrap()
        .layout
        .component = Some(ComponentBinding::Instance {
        main,
        nodes: new_nodes,
    });
    let replaced: HashSet<_> = updated
        .iter()
        .map(|s| s.id)
        .chain(tracked.iter().copied())
        .collect();
    let insertion = originals
        .iter()
        .position(|s| s.id == root)
        .unwrap_or(originals.len());
    let mut after = Vec::with_capacity(originals.len() + updated.len());
    let mut inserted = false;
    for (i, s) in originals.iter().enumerate() {
        if i == insertion {
            after.extend(updated.iter().cloned());
            inserted = true;
        }
        if !replaced.contains(&s.id) {
            after.push(s.clone());
        }
    }
    if !inserted {
        after.extend(updated);
    }
    if originals == after {
        return Ok(false);
    }
    *doc.layers[layer].kind.shapes_mut().unwrap() = after;
    Ok(true)
}

pub fn detach(doc: &mut Document, layer: usize, root: u64) -> Result<(), String> {
    let shape = doc
        .find_shape_mut(layer, root)
        .ok_or("Instance is missing")?;
    if !matches!(
        shape.layout.component,
        Some(ComponentBinding::Instance { .. })
    ) {
        return Err("Select an instance".into());
    }
    shape.layout.component = None;
    Ok(())
}

/// Update all instances after document commands. Missing definitions deliberately
/// retain the last good instance; they never destroy a designer's artwork.
pub fn synchronize_all(doc: &mut Document) -> Result<usize, String> {
    let instances: Vec<_> = doc
        .layers
        .iter()
        .enumerate()
        .flat_map(|(li, l)| {
            l.kind
                .shapes()
                .unwrap_or_default()
                .iter()
                .filter_map(move |s| {
                    if let Some(ComponentBinding::Instance { main, .. }) = &s.layout.component {
                        Some((li, s.id, *main))
                    } else {
                        None
                    }
                })
        })
        .collect();
    let lookup: HashMap<_, _> = instances
        .iter()
        .map(|(li, id, main)| (*id, (*li, *main)))
        .collect();
    fn order(
        doc: &Document,
        id: u64,
        lookup: &HashMap<u64, (usize, u64)>,
        done: &mut HashSet<u64>,
        visiting: &mut HashSet<u64>,
        sorted: &mut Vec<u64>,
    ) -> Result<(), String> {
        if done.contains(&id) {
            return Ok(());
        }
        if !visiting.insert(id) {
            return Err("Circular component dependency; detach an instance to repair it".into());
        }
        let Some((_, main)) = lookup.get(&id) else {
            return Ok(());
        };
        if let Some((layer, _)) = locate(doc, *main) {
            for shape in subtree(doc, layer, *main) {
                if lookup.contains_key(&shape.id) {
                    order(doc, shape.id, lookup, done, visiting, sorted)?;
                }
            }
        }
        visiting.remove(&id);
        done.insert(id);
        sorted.push(id);
        Ok(())
    }
    let mut sorted = Vec::new();
    let mut done = HashSet::new();
    for (_, id, _) in &instances {
        order(
            doc,
            *id,
            &lookup,
            &mut done,
            &mut HashSet::new(),
            &mut sorted,
        )?;
    }
    let mut count = 0;
    for id in sorted {
        let (li, main) = lookup[&id];
        if family_of_main(doc, main).is_some() && sync_instance(doc, li, id, false)? {
            count += 1;
        }
    }
    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::{Layer, Style};
    use crate::geom::TypeRun;
    use crate::layout::make_frame;

    fn fixture() -> (Document, u64, u64) {
        let mut doc = Document::new("Components", 800.0, 600.0, 96.0);
        doc.layers = vec![Layer::vector("UI")];
        let root = make_frame(Pt::new(20.0, 30.0), Pt::new(200.0, 80.0));
        let root_id = root.id;
        let mut child = Shape::new(
            Geom::Text(TypeRun {
                origin: Pt::new(40.0, 60.0),
                content: "Buy".into(),
                px: 16.0,
                ..Default::default()
            }),
            Style::default(),
        );
        child.layout.parent = Some(root_id);
        let child_id = child.id;
        doc.layers[0]
            .kind
            .shapes_mut()
            .unwrap()
            .extend([root, child]);
        make_component(&mut doc, 0, root_id).unwrap();
        (doc, root_id, child_id)
    }

    #[test]
    fn sync_preserves_text_override_and_position_but_inherits_typography_after_save() {
        let (mut doc, main, child) = fixture();
        let instance = insert_instance(&mut doc, main, 0, Pt::new(300.0, 120.0), None).unwrap();
        let instance_child = crate::layout::children(&doc, 0, instance)[0];
        if let Geom::Text(run) = &mut doc.find_shape_mut(0, instance_child).unwrap().geom {
            run.content = "Subscribe".into();
        }
        let serialized = serde_json::to_string(&doc).unwrap();
        let mut doc: Document = serde_json::from_str(&serialized).unwrap();
        if let Geom::Text(run) = &mut doc.find_shape_mut(0, child).unwrap().geom {
            run.content = "Purchase".into();
            run.px = 22.0;
        }
        sync_instance(&mut doc, 0, instance, false).unwrap();
        let Geom::Text(run) = &doc.find_shape(0, instance_child).unwrap().geom else {
            panic!()
        };
        assert_eq!(run.content, "Subscribe");
        assert_eq!(run.px, 22.0);
        assert_eq!(
            doc.find_shape(0, instance).unwrap().geom.bbox().min,
            Pt::new(300.0, 120.0)
        );
    }

    #[test]
    fn variants_keep_slot_ids_and_overrides_and_reset_restores_definition() {
        let (mut doc, main, _) = fixture();
        let instance = insert_instance(&mut doc, main, 0, Pt::new(300.0, 0.0), None).unwrap();
        let child = crate::layout::children(&doc, 0, instance)[0];
        doc.find_shape_mut(0, child).unwrap().opacity = 0.4;
        let alternate = add_variant(&mut doc, main, "Hover", Pt::new(20.0, 160.0)).unwrap();
        doc.find_shape_mut(0, alternate).unwrap().opacity = 0.6;
        swap_variant(&mut doc, 0, instance, alternate).unwrap();
        assert_eq!(crate::layout::children(&doc, 0, instance), vec![child]);
        assert_eq!(doc.find_shape(0, child).unwrap().opacity, 0.4);
        assert_eq!(doc.find_shape(0, instance).unwrap().opacity, 0.6);
        sync_instance(&mut doc, 0, instance, true).unwrap();
        assert_eq!(doc.find_shape(0, child).unwrap().opacity, 1.0);
    }

    #[test]
    fn sync_updates_structure_without_losing_local_children() {
        let (mut doc, main, child) = fixture();
        let instance = insert_instance(&mut doc, main, 0, Pt::new(300.0, 0.0), None).unwrap();
        let inherited_child = crate::layout::children(&doc, 0, instance)[0];
        let mut local = make_frame(Pt::new(310.0, 5.0), Pt::new(10.0, 10.0));
        local.layout.parent = Some(instance);
        let local_id = local.id;
        doc.layers[0].kind.shapes_mut().unwrap().push(local);
        doc.layers[0]
            .kind
            .shapes_mut()
            .unwrap()
            .retain(|s| s.id != child);
        sync_instance(&mut doc, 0, instance, false).unwrap();
        assert!(doc.find_shape(0, inherited_child).is_none());
        assert!(doc.find_shape(0, local_id).is_some());
        assert!(!sync_instance(&mut doc, 0, instance, false).unwrap());
        detach(&mut doc, 0, instance).unwrap();
        assert!(
            doc.find_shape(0, instance)
                .unwrap()
                .layout
                .component
                .is_none()
        );
    }

    #[test]
    fn direct_component_recursion_is_rejected() {
        let (mut doc, main, _) = fixture();
        assert!(insert_instance(&mut doc, main, 0, Pt::ZERO, Some(main)).is_err());
    }

    #[test]
    fn nested_component_changes_propagate_in_dependency_order_and_cycles_are_rejected() {
        let (mut doc, inner, child) = fixture();
        let outer = make_frame(Pt::new(10.0, 200.0), Pt::new(250.0, 150.0));
        let outer_id = outer.id;
        doc.layers[0].kind.shapes_mut().unwrap().push(outer);
        make_component(&mut doc, 0, outer_id).unwrap();
        let nested =
            insert_instance(&mut doc, inner, 0, Pt::new(30.0, 220.0), Some(outer_id)).unwrap();
        let outer_instance =
            insert_instance(&mut doc, outer_id, 0, Pt::new(350.0, 200.0), None).unwrap();
        assert!(insert_instance(&mut doc, outer_id, 0, Pt::ZERO, Some(inner)).is_err());
        doc.find_shape_mut(0, child).unwrap().opacity = 0.25;
        assert!(synchronize_all(&mut doc).unwrap() >= 2);
        let nested_child = crate::layout::children(&doc, 0, nested)[0];
        assert_eq!(doc.find_shape(0, nested_child).unwrap().opacity, 0.25);
        let instance_children = crate::layout::descendants(&doc, 0, outer_instance);
        assert!(instance_children.iter().any(|id| {
            doc.find_shape(0, *id)
                .is_some_and(|s| matches!(s.geom, Geom::Text(_)) && s.opacity == 0.25)
        }));
    }

    #[test]
    fn local_deletion_survives_sync_and_reset_restores_it() {
        let (mut doc, main, _) = fixture();
        let instance = insert_instance(&mut doc, main, 0, Pt::new(300.0, 0.0), None).unwrap();
        let child = crate::layout::children(&doc, 0, instance)[0];
        doc.layers[0]
            .kind
            .shapes_mut()
            .unwrap()
            .retain(|s| s.id != child);
        sync_instance(&mut doc, 0, instance, false).unwrap();
        assert!(doc.find_shape(0, child).is_none());
        sync_instance(&mut doc, 0, instance, true).unwrap();
        assert!(doc.find_shape(0, child).is_some());
    }

    #[test]
    fn nested_structural_updates_do_not_duplicate_children_or_erase_overrides() {
        let (mut doc, inner, _) = fixture();
        let outer = make_frame(Pt::new(10.0, 200.0), Pt::new(250.0, 150.0));
        let outer_id = outer.id;
        doc.layers[0].kind.shapes_mut().unwrap().push(outer);
        make_component(&mut doc, 0, outer_id).unwrap();
        insert_instance(&mut doc, inner, 0, Pt::new(30.0, 220.0), Some(outer_id)).unwrap();
        let outer_instance =
            insert_instance(&mut doc, outer_id, 0, Pt::new(350.0, 200.0), None).unwrap();
        let nested = crate::layout::children(&doc, 0, outer_instance)[0];
        let label = crate::layout::children(&doc, 0, nested)[0];
        if let Geom::Text(run) = &mut doc.find_shape_mut(0, label).unwrap().geom {
            run.content = "Custom label".into();
        }
        let mut added = Shape::new(
            Geom::Rect {
                origin: Pt::new(30.0, 40.0),
                size: Pt::new(10.0, 10.0),
                radius: 0.0,
            },
            Style::default(),
        );
        added.name = "New inherited icon".into();
        added.layout.parent = Some(inner);
        let added_id = added.id;
        doc.layers[0].kind.shapes_mut().unwrap().push(added);
        synchronize_all(&mut doc).unwrap();
        synchronize_all(&mut doc).unwrap();
        let kids = crate::layout::children(&doc, 0, nested);
        assert_eq!(
            kids.len(),
            2,
            "one label and one inherited icon, without a duplicate"
        );
        assert_eq!(
            kids.iter()
                .filter(|id| doc.find_shape(0, **id).unwrap().name == "New inherited icon")
                .count(),
            1
        );
        let Geom::Text(run) = &doc.find_shape(0, label).unwrap().geom else {
            panic!()
        };
        assert_eq!(run.content, "Custom label");
        doc.layers[0]
            .kind
            .shapes_mut()
            .unwrap()
            .retain(|shape| shape.id != added_id);
        synchronize_all(&mut doc).unwrap();
        assert_eq!(crate::layout::children(&doc, 0, nested), vec![label]);
        assert!(doc.validate_hierarchy().is_ok());
    }

    #[test]
    fn nested_source_variant_changes_inherit_while_local_variant_override_survives_save() {
        let (mut doc, inner, _) = fixture();
        let hover = add_variant(&mut doc, inner, "Hover", Pt::new(20.0, 400.0)).unwrap();
        doc.find_shape_mut(0, hover).unwrap().opacity = 0.4;
        let mut icon = Shape::new(
            Geom::Rect {
                origin: Pt::new(30.0, 410.0),
                size: Pt::new(8.0, 8.0),
                radius: 0.0,
            },
            Style::default(),
        );
        icon.layout.parent = Some(hover);
        icon.name = "Hover-only icon".into();
        doc.layers[0].kind.shapes_mut().unwrap().push(icon);
        let outer = make_frame(Pt::new(10.0, 200.0), Pt::new(250.0, 150.0));
        let outer_id = outer.id;
        doc.layers[0].kind.shapes_mut().unwrap().push(outer);
        make_component(&mut doc, 0, outer_id).unwrap();
        let source_nested =
            insert_instance(&mut doc, inner, 0, Pt::new(30.0, 220.0), Some(outer_id)).unwrap();
        let outer_instance =
            insert_instance(&mut doc, outer_id, 0, Pt::new(350.0, 200.0), None).unwrap();
        let nested = crate::layout::children(&doc, 0, outer_instance)[0];
        swap_variant(&mut doc, 0, source_nested, hover).unwrap();
        synchronize_all(&mut doc).unwrap();
        let Some(ComponentBinding::Instance { main, .. }) =
            &doc.find_shape(0, nested).unwrap().layout.component
        else {
            panic!()
        };
        assert_eq!(*main, hover, "source variant switch must propagate");
        assert_eq!(doc.find_shape(0, nested).unwrap().opacity, 0.4);
        assert_eq!(crate::layout::children(&doc, 0, nested).len(), 2);

        // Override just this nested instance back to Default. Its shorter
        // structure must survive source Hover edits, sync and persistence.
        swap_variant(&mut doc, 0, nested, inner).unwrap();
        synchronize_all(&mut doc).unwrap();
        let encoded = crate::project::encode(&doc).unwrap();
        let mut doc = crate::project::decode(&encoded).unwrap();
        doc.find_shape_mut(0, hover).unwrap().opacity = 0.7;
        synchronize_all(&mut doc).unwrap();
        synchronize_all(&mut doc).unwrap();
        let Some(ComponentBinding::Instance { main, .. }) =
            &doc.find_shape(0, nested).unwrap().layout.component
        else {
            panic!()
        };
        assert_eq!(*main, inner, "explicit local variant remains selected");
        assert_eq!(doc.find_shape(0, nested).unwrap().opacity, 1.0);
        assert_eq!(
            crate::layout::children(&doc, 0, nested).len(),
            1,
            "Hover-only structure must not leak into the local Default variant"
        );
        assert!(doc.validate_hierarchy().is_ok());
    }

    #[test]
    fn duplicating_an_instance_keeps_independent_node_ids_and_overrides() {
        let (mut doc, main, _) = fixture();
        let instance = insert_instance(&mut doc, main, 0, Pt::new(300.0, 0.0), None).unwrap();
        let shapes = subtree(&doc, 0, instance);
        let ids: HashMap<_, _> = shapes.iter().map(|s| (s.id, next_id())).collect();
        let copied = ids[&instance];
        for mut shape in shapes {
            shape.id = ids[&shape.id];
            shape.layout.parent = shape
                .layout
                .parent
                .map(|p| ids.get(&p).copied().unwrap_or(p));
            shape.geom.translate(Pt::new(0.0, 200.0));
            remap_duplicate(&mut shape, &ids);
            doc.layers[0].kind.shapes_mut().unwrap().push(shape);
        }
        synchronize_all(&mut doc).unwrap();
        let original_child = crate::layout::children(&doc, 0, instance)[0];
        let copied_child = crate::layout::children(&doc, 0, copied)[0];
        assert_ne!(original_child, copied_child);
        assert_eq!(
            doc.find_shape(0, copied).unwrap().geom.bbox().min,
            Pt::new(300.0, 200.0)
        );
        doc.find_shape_mut(0, copied_child).unwrap().opacity = 0.6;
        synchronize_all(&mut doc).unwrap();
        assert_eq!(doc.find_shape(0, original_child).unwrap().opacity, 1.0);
    }
}
