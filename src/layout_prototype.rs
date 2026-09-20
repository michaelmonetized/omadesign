//! Persisted prototype interactions and a non-destructive presentation session.

use crate::document::Document;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Trigger {
    #[default]
    Click,
    Hover,
    Press,
}

impl Trigger {
    pub fn name(self) -> &'static str {
        match self {
            Self::Click => "Click",
            Self::Hover => "Hover",
            Self::Press => "Press",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PrototypeAction {
    Navigate { target: u64 },
    Back,
    OpenOverlay { target: u64 },
    CloseOverlay,
    SetVariant { component: u64 },
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Transition {
    #[default]
    Instant,
    Dissolve,
    SlideLeft,
    SlideRight,
}

impl Transition {
    pub fn name(self) -> &'static str {
        match self {
            Self::Instant => "Instant",
            Self::Dissolve => "Dissolve",
            Self::SlideLeft => "Slide left",
            Self::SlideRight => "Slide right",
        }
    }
}

fn default_duration() -> u32 {
    240
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Interaction {
    #[serde(default)]
    pub trigger: Trigger,
    pub action: PrototypeAction,
    #[serde(default)]
    pub transition: Transition,
    #[serde(default = "default_duration")]
    pub duration_ms: u32,
}

#[derive(Clone, Debug, Default)]
pub struct PrototypeState {
    pub current_frame: Option<u64>,
    pub history: Vec<u64>,
    pub overlays: Vec<u64>,
    pub active_variants: BTreeMap<u64, u64>,
    /// Exposed for native presentation to animate actual transition progress.
    pub last_transition: Transition,
    pub duration_ms: u32,
    hover_restore: BTreeMap<u64, Option<u64>>,
}

fn frame_exists(doc: &Document, id: u64) -> bool {
    doc.layers.iter().any(|l| {
        l.kind
            .shapes()
            .is_some_and(|s| s.iter().any(|s| s.id == id && s.layout.frame))
    })
}

impl PrototypeState {
    pub fn start(&mut self, doc: &Document, frame: u64) -> Result<(), String> {
        if !frame_exists(doc, frame) {
            return Err("Choose a frame to preview".into());
        }
        *self = Self {
            current_frame: Some(frame),
            ..Self::default()
        };
        Ok(())
    }

    pub fn back(&mut self) -> bool {
        if self.close_overlay() {
            return true;
        }
        let Some(previous) = self.history.pop() else {
            return false;
        };
        self.current_frame = Some(previous);
        self.clear_hover();
        true
    }

    pub fn close_overlay(&mut self) -> bool {
        self.overlays.pop().is_some()
    }

    /// Hover variants restore their previous state when the pointer leaves.
    pub fn clear_hover(&mut self) {
        for (instance, previous) in std::mem::take(&mut self.hover_restore) {
            if let Some(previous) = previous {
                self.active_variants.insert(instance, previous);
            } else {
                self.active_variants.remove(&instance);
            }
        }
    }

    pub fn activate(
        &mut self,
        doc: &Document,
        source: u64,
        trigger: Trigger,
    ) -> Result<bool, String> {
        let (layer, shape) = doc
            .layers
            .iter()
            .enumerate()
            .find_map(|(li, l)| {
                l.kind
                    .shapes()?
                    .iter()
                    .find(|s| s.id == source)
                    .map(|s| (li, s))
            })
            .ok_or("Prototype object is missing")?;
        let actions: Vec<_> = shape
            .layout
            .interactions
            .iter()
            .filter(|i| i.trigger == trigger)
            .cloned()
            .collect();
        for interaction in &actions {
            validate_interaction(doc, layer, source, interaction)?;
        }
        let mut changed = false;
        for interaction in actions {
            self.last_transition = interaction.transition;
            self.duration_ms = interaction.duration_ms.min(10_000);
            match interaction.action {
                PrototypeAction::Navigate { target } => {
                    if self.current_frame != Some(target) {
                        if let Some(previous) = self.current_frame {
                            self.history.push(previous);
                        }
                        self.current_frame = Some(target);
                        self.overlays.clear();
                        self.clear_hover();
                        changed = true;
                    }
                }
                PrototypeAction::Back => changed |= self.back(),
                PrototypeAction::OpenOverlay { target } => {
                    if self.current_frame != Some(target) && !self.overlays.contains(&target) {
                        self.overlays.push(target);
                        changed = true;
                    }
                }
                PrototypeAction::CloseOverlay => changed |= self.close_overlay(),
                PrototypeAction::SetVariant { component } => {
                    let instance = crate::layout_components::instance_ancestor(doc, layer, source)
                        .ok_or("Variant interactions belong to an instance")?;
                    if trigger == Trigger::Hover {
                        self.hover_restore
                            .entry(instance)
                            .or_insert_with(|| self.active_variants.get(&instance).copied());
                    } else {
                        self.hover_restore.remove(&instance);
                    }
                    changed |= self.active_variants.insert(instance, component) != Some(component);
                }
            }
        }
        Ok(changed)
    }

    pub fn preview_document(&self, doc: &Document) -> Result<Document, String> {
        let mut preview = doc.layout_snapshot();
        for (instance, main) in &self.active_variants {
            let Some(layer) = preview.layers.iter().position(|l| {
                l.kind
                    .shapes()
                    .is_some_and(|s| s.iter().any(|s| s.id == *instance))
            }) else {
                continue;
            };
            crate::layout_components::swap_variant(&mut preview, layer, *instance, *main)?;
            crate::layout::reflow(&mut preview, layer, *instance);
        }
        Ok(preview)
    }
}

pub fn validate_interaction(
    doc: &Document,
    layer: usize,
    source: u64,
    interaction: &Interaction,
) -> Result<(), String> {
    if doc.find_shape(layer, source).is_none() {
        return Err("Interaction source is missing".into());
    }
    match interaction.action {
        PrototypeAction::Navigate { target } | PrototypeAction::OpenOverlay { target } => {
            if !frame_exists(doc, target) {
                return Err("Interaction target must be an existing frame".into());
            }
        }
        PrototypeAction::SetVariant { component } => {
            let root = crate::layout_components::instance_ancestor(doc, layer, source)
                .ok_or("Choose an instance for a variant interaction")?;
            let instance = doc.find_shape(layer, root).unwrap();
            let Some(crate::layout_components::ComponentBinding::Instance { main, .. }) =
                instance.layout.component
            else {
                unreachable!()
            };
            let family = crate::layout_components::family_of(doc, main)
                .ok_or("The component definition is missing")?;
            if crate::layout_components::family_of(doc, component) != Some(family) {
                return Err("Choose a variant in this component family".into());
            }
        }
        PrototypeAction::Back | PrototypeAction::CloseOverlay => {}
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::Layer;
    use crate::geom::Pt;
    use crate::layout::make_frame;

    #[test]
    fn navigation_overlays_and_back_do_not_mutate_design() {
        let mut doc = Document::new("Prototype", 800.0, 600.0, 96.0);
        doc.layers = vec![Layer::vector("Screens")];
        let mut first = make_frame(Pt::ZERO, Pt::new(100.0, 200.0));
        let mut second = make_frame(Pt::new(120.0, 0.0), Pt::new(100.0, 200.0));
        let overlay = make_frame(Pt::new(240.0, 0.0), Pt::new(80.0, 80.0));
        let (a, b, o) = (first.id, second.id, overlay.id);
        first.layout.interactions.push(Interaction {
            trigger: Trigger::Click,
            action: PrototypeAction::Navigate { target: b },
            transition: Transition::Dissolve,
            duration_ms: 180,
        });
        second.layout.interactions.push(Interaction {
            trigger: Trigger::Click,
            action: PrototypeAction::OpenOverlay { target: o },
            transition: Transition::Instant,
            duration_ms: 0,
        });
        doc.layers[0]
            .kind
            .shapes_mut()
            .unwrap()
            .extend([first, second, overlay]);
        let before = serde_json::to_string(&doc).unwrap();
        let mut state = PrototypeState::default();
        state.start(&doc, a).unwrap();
        assert!(state.activate(&doc, a, Trigger::Click).unwrap());
        assert_eq!(state.current_frame, Some(b));
        state.activate(&doc, b, Trigger::Click).unwrap();
        assert_eq!(state.overlays, vec![o]);
        assert!(state.back());
        assert_eq!(state.current_frame, Some(b));
        assert!(state.back());
        assert_eq!(state.current_frame, Some(a));
        assert_eq!(serde_json::to_string(&doc).unwrap(), before);
    }

    #[test]
    fn hover_materializes_variant_without_mutating_source_and_restores_on_leave() {
        let mut doc = Document::new("Interactive component", 800.0, 600.0, 96.0);
        doc.layers = vec![Layer::vector("Components")];
        let main = make_frame(Pt::ZERO, Pt::new(100.0, 50.0));
        let main_id = main.id;
        doc.layers[0].kind.shapes_mut().unwrap().push(main);
        crate::layout_components::make_component(&mut doc, 0, main_id).unwrap();
        let hover =
            crate::layout_components::add_variant(&mut doc, main_id, "Hover", Pt::new(0.0, 100.0))
                .unwrap();
        doc.find_shape_mut(0, hover).unwrap().opacity = 0.5;
        let instance = crate::layout_components::insert_instance(
            &mut doc,
            main_id,
            0,
            Pt::new(300.0, 0.0),
            None,
        )
        .unwrap();
        doc.find_shape_mut(0, instance)
            .unwrap()
            .layout
            .interactions
            .push(Interaction {
                trigger: Trigger::Hover,
                action: PrototypeAction::SetVariant { component: hover },
                transition: Transition::Dissolve,
                duration_ms: 100,
            });
        let mut state = PrototypeState::default();
        state.start(&doc, instance).unwrap();
        assert!(state.activate(&doc, instance, Trigger::Hover).unwrap());
        let preview = state.preview_document(&doc).unwrap();
        assert_eq!(preview.find_shape(0, instance).unwrap().opacity, 0.5);
        assert_eq!(doc.find_shape(0, instance).unwrap().opacity, 1.0);
        state.clear_hover();
        assert!(state.active_variants.is_empty());
        let selected = crate::layout_components::add_variant(
            &mut doc,
            main_id,
            "Selected",
            Pt::new(0.0, 200.0),
        )
        .unwrap();
        doc.find_shape_mut(0, instance)
            .unwrap()
            .layout
            .interactions
            .push(Interaction {
                trigger: Trigger::Click,
                action: PrototypeAction::SetVariant {
                    component: selected,
                },
                transition: Transition::Instant,
                duration_ms: 0,
            });
        state.activate(&doc, instance, Trigger::Hover).unwrap();
        state.activate(&doc, instance, Trigger::Click).unwrap();
        state.clear_hover();
        assert_eq!(state.active_variants.get(&instance), Some(&selected));
    }

    #[test]
    fn invalid_target_does_not_partially_apply_a_trigger() {
        let mut doc = Document::new("Invalid links", 300.0, 200.0, 96.0);
        doc.layers = vec![Layer::vector("UI")];
        let mut a = make_frame(Pt::ZERO, Pt::new(100.0, 50.0));
        let b = make_frame(Pt::new(120.0, 0.0), Pt::new(100.0, 50.0));
        let (a_id, b_id) = (a.id, b.id);
        for target in [b_id, u64::MAX] {
            a.layout.interactions.push(Interaction {
                trigger: Trigger::Click,
                action: PrototypeAction::Navigate { target },
                transition: Transition::Instant,
                duration_ms: 0,
            });
        }
        doc.layers[0].kind.shapes_mut().unwrap().extend([a, b]);
        let mut state = PrototypeState::default();
        state.start(&doc, a_id).unwrap();
        assert!(state.activate(&doc, a_id, Trigger::Click).is_err());
        assert_eq!(state.current_frame, Some(a_id));
        assert!(state.history.is_empty());
    }
}
