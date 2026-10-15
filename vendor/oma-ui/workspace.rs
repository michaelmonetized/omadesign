//! Bounded panel placement without document or canvas ownership.
use eframe::egui::{self, Rect, Ui, pos2, vec2};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Place {
    Left,
    Right,
    Top,
    Bottom,
    Floating,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Group {
    pub panels: Vec<String>,
    pub active: usize,
    pub place: Place,
    pub extent: f32,
    pub rect: [f32; 4],
}
impl Default for Group {
    fn default() -> Self {
        Self {
            panels: Vec::new(),
            active: 0,
            place: Place::Right,
            extent: 288.0,
            rect: [40.0, 40.0, 320.0, 420.0],
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Workspace {
    pub groups: Vec<Group>,
    pub hidden: BTreeMap<String, Group>,
    pub focus: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Book {
    pub active: String,
    pub tasks: BTreeMap<String, Workspace>,
}

impl Workspace {
    /// Create an application preset.
    /// Args: panel IDs and their starting docks. Returns a movable layout with shared dock tabs.
    pub fn preset(panels: &[(&str, Place)]) -> Self {
        let mut layout = Self::default();
        for &(id, place) in panels {
            layout.move_panel(id, place);
        }
        layout
    }

    /// Validate saved panel identities and geometry.
    /// Args: application panel registry. Returns a normalized layout; unknown/duplicate panels disappear.
    pub fn normalize(&mut self, registry: &[&str]) {
        let mut seen = BTreeSet::new();
        self.groups.truncate(64);
        for group in &mut self.groups {
            group
                .panels
                .retain(|id| registry.contains(&id.as_str()) && seen.insert(id.clone()));
            group.active = group.active.min(group.panels.len().saturating_sub(1));
            if !group.extent.is_finite() {
                group.extent = 288.0;
            }
            group.extent = group.extent.clamp(80.0, 1200.0);
            if !group.rect.iter().all(|v| v.is_finite()) {
                group.rect = Group::default().rect;
            }
        }
        self.groups.retain(|group| !group.panels.is_empty());
        self.hidden
            .retain(|id, _| registry.contains(&id.as_str()) && !seen.contains(id));
        for &id in registry {
            if !seen.contains(id) && !self.hidden.contains_key(id) {
                let mut group = Group::default();
                group.panels.push(id.into());
                self.hidden.insert(id.into(), group);
            }
        }
        if self.focus.as_ref().is_some_and(|id| !seen.contains(id)) {
            self.focus = None;
        }
    }

    fn remove(&mut self, id: &str) -> Option<Group> {
        let mut origin = None;
        for group in &mut self.groups {
            if let Some(index) = group.panels.iter().position(|panel| panel == id) {
                origin = Some(group.clone());
                group.panels.remove(index);
                if index < group.active {
                    group.active -= 1;
                }
                group.active = group.active.min(group.panels.len().saturating_sub(1));
            }
        }
        self.groups.retain(|group| !group.panels.is_empty());
        origin.or_else(|| self.hidden.remove(id))
    }

    /// Move or restore a panel to a dock or floating window.
    /// Args: registered ID and destination. Returns no value; central canvas is never a destination.
    pub fn move_panel(&mut self, id: &str, place: Place) {
        let mut group = self.remove(id).unwrap_or_default();
        self.hidden.remove(id);
        if place != Place::Floating {
            if let Some(target) = self.groups.iter_mut().find(|group| group.place == place) {
                target.panels.push(id.into());
                target.active = target.panels.len() - 1;
                self.focus = Some(id.into());
                return;
            }
        }
        group.panels = vec![id.into()];
        group.active = 0;
        group.place = place;
        self.groups.push(group);
        self.focus = Some(id.into());
    }

    /// Put a panel beside another tab.
    /// Args: source and target panel IDs. Returns false if no visible target exists.
    pub fn tab_with(&mut self, id: &str, target: &str) -> bool {
        if id == target
            || !self
                .groups
                .iter()
                .any(|g| g.panels.iter().any(|p| p == target))
        {
            return false;
        }
        self.remove(id);
        self.hidden.remove(id);
        if let Some(group) = self
            .groups
            .iter_mut()
            .find(|g| g.panels.iter().any(|p| p == target))
        {
            group.panels.push(id.into());
            group.active = group.panels.len() - 1;
            self.focus = Some(id.into());
            return true;
        }
        false
    }

    /// Hide a panel while preserving its location.
    /// Args: visible panel ID. Returns no value; restore uses the retained group.
    pub fn close(&mut self, id: &str) {
        if let Some(group) = self.remove(id) {
            self.hidden.insert(id.into(), group);
        }
        if self.focus.as_deref() == Some(id) {
            self.focus = None;
        }
    }

    /// Hide a floating group without losing its tab membership.
    /// Args: original group. Returns no value; every tab retains the same restore location.
    fn close_group(&mut self, group: &Group) {
        for panel in &group.panels {
            self.close(panel);
        }
        for panel in &group.panels {
            self.hidden.insert(panel.clone(), group.clone());
        }
    }

    /// Restore a hidden panel.
    /// Args: panel ID. Returns no value; an existing group is preferred over a duplicate dock.
    pub fn restore(&mut self, id: &str) {
        if let Some(group) = self.hidden.remove(id) {
            if let Some(target) = group
                .panels
                .iter()
                .find(|p| p.as_str() != id && self.groups.iter().any(|g| g.panels.contains(p)))
                .cloned()
            {
                self.tab_with(id, &target);
                if let Some(destination) = self
                    .groups
                    .iter_mut()
                    .find(|g| g.panels.iter().any(|p| p == id))
                {
                    let origin = group.panels.iter().position(|p| p == id).unwrap_or(0);
                    destination.panels.retain(|p| p != id);
                    let index = group.panels[origin + 1..]
                        .iter()
                        .find_map(|panel| destination.panels.iter().position(|p| p == panel))
                        .or_else(|| {
                            group.panels[..origin].iter().rev().find_map(|panel| {
                                destination
                                    .panels
                                    .iter()
                                    .position(|p| p == panel)
                                    .map(|index| index + 1)
                            })
                        })
                        .unwrap_or_else(|| origin.min(destination.panels.len()));
                    destination.panels.insert(index, id.into());
                    destination.active = index;
                }
            } else {
                let place = group.place;
                self.hidden.insert(id.into(), group);
                self.move_panel(id, place);
            }
        }
    }

    /// Draw dock and floating panels around a protected center.
    /// Args: parent UI, stable workspace ID, panel registry with labels, and application panel body.
    /// Returns the remaining center rectangle; the application draws its canvas there.
    #[allow(deprecated)]
    pub fn show(
        &mut self,
        ui: &mut Ui,
        id: &str,
        registry: &[(&str, &str)],
        mut body: impl FnMut(&mut Ui, &str),
    ) -> Rect {
        let mut center = ui.available_rect_before_wrap();
        let viewport = ui
            .ctx()
            .input(|input| input.viewport().inner_rect)
            .unwrap_or(ui.max_rect());
        let panels: Vec<_> = self
            .groups
            .iter()
            .flat_map(|g| g.panels.iter().cloned())
            .collect();
        if ui.input(|input| input.key_pressed(egui::Key::F6)) && !panels.is_empty() {
            let current = self
                .focus
                .as_ref()
                .and_then(|id| panels.iter().position(|p| p == id))
                .unwrap_or(0);
            let next = if ui.input(|i| i.modifiers.shift) {
                (current + panels.len() - 1) % panels.len()
            } else {
                (current + 1) % panels.len()
            };
            self.focus = Some(panels[next].clone());
            for group in &mut self.groups {
                if let Some(index) = group.panels.iter().position(|p| p == &panels[next]) {
                    group.active = index;
                }
            }
        }
        let focus_key = egui::Id::new(id).with("focused-panel");
        let previous_focus: Option<String> = ui.ctx().data_mut(|d| d.get_temp(focus_key));
        let focus_next = if previous_focus != self.focus {
            self.focus.clone()
        } else {
            None
        };
        ui.ctx()
            .data_mut(|d| d.insert_temp(focus_key, self.focus.clone().unwrap_or_default()));
        let before = self.groups.clone();
        let mut command = None;
        for (index, saved) in before.iter().enumerate() {
            let mut group = saved.clone();
            let window_id = egui::Id::new(id).with(index).with(saved.panels.first());
            let panel_label = |panel: &str| {
                registry
                    .iter()
                    .find(|entry| entry.0 == panel)
                    .map_or_else(|| panel.to_string(), |entry| entry.1.to_string())
            };
            let mut content = |ui: &mut Ui| {
                ui.set_clip_rect(ui.max_rect().intersect(ui.clip_rect()));
                ui.horizontal_wrapped(|ui| {
                    for (tab, panel) in saved.panels.iter().enumerate() {
                        let response = ui.selectable_label(tab == group.active, panel_label(panel));
                        if focus_next.as_ref() == Some(panel) {
                            response.request_focus();
                        }
                        if response.clicked() {
                            group.active = tab;
                            self.focus = Some(panel.clone());
                        }
                        response.context_menu(|ui| {
                            for (label, place) in [
                                ("Move left", Place::Left),
                                ("Move right", Place::Right),
                                ("Move top", Place::Top),
                                ("Move bottom", Place::Bottom),
                                ("Float", Place::Floating),
                            ] {
                                if ui.button(label).clicked() {
                                    command = Some((panel.clone(), Some(place), None));
                                    ui.close();
                                }
                            }
                            ui.menu_button("Tab with", |ui| {
                                for other in before
                                    .iter()
                                    .flat_map(|g| &g.panels)
                                    .filter(|other| *other != panel)
                                {
                                    if ui.button(panel_label(other)).clicked() {
                                        command = Some((panel.clone(), None, Some(other.clone())));
                                        ui.close();
                                    }
                                }
                            });
                            if ui.button("Close").clicked() {
                                command = Some((panel.clone(), None, None));
                                ui.close();
                            }
                        });
                    }
                });
                ui.separator();
                if let Some(panel) = saved.panels.get(group.active) {
                    ui.push_id(panel, |ui| body(ui, panel));
                }
            };
            if saved.place == Place::Floating {
                let rect = safe_rect(saved.rect, viewport);
                let mut open = true;
                if let Some(shown) = egui::Window::new(panel_label(&saved.panels[saved.active]))
                    .id(window_id)
                    .open(&mut open)
                    .constrain_to(viewport)
                    .current_pos(rect.min)
                    .default_size(rect.size())
                    .min_width(0.0)
                    .min_height(0.0)
                    .max_size(viewport.size())
                    .vscroll(true)
                    .show(ui.ctx(), |ui| content(ui))
                {
                    group.rect = [
                        shown.response.rect.min.x,
                        shown.response.rect.min.y,
                        shown.response.rect.width(),
                        shown.response.rect.height(),
                    ];
                }
                if !open {
                    self.close_group(saved);
                }
            } else {
                let horizontal = matches!(saved.place, Place::Left | Place::Right);
                let available = if horizontal {
                    center.width()
                } else {
                    center.height()
                };
                let reserve = if horizontal { 240.0 } else { 160.0 };
                let extent = saved.extent.min((available - reserve).max(0.0)).max(0.0);
                let mut rect = center;
                match saved.place {
                    Place::Left => {
                        rect.max.x = rect.min.x + extent;
                        center.min.x = rect.max.x;
                    }
                    Place::Right => {
                        rect.min.x = rect.max.x - extent;
                        center.max.x = rect.min.x;
                    }
                    Place::Top => {
                        rect.max.y = rect.min.y + extent;
                        center.min.y = rect.max.y;
                    }
                    Place::Bottom => {
                        rect.min.y = rect.max.y - extent;
                        center.max.y = rect.min.y;
                    }
                    Place::Floating => {}
                }
                if extent > 20.0 {
                    ui.scope_builder(egui::UiBuilder::new().max_rect(rect.shrink(4.0)), |ui| {
                        egui::Frame::group(ui.style()).show(ui, |ui| {
                            ui.set_min_size((rect.size() - vec2(24.0, 24.0)).max(vec2(0.0, 0.0)));
                            content(ui);
                        });
                    });
                    let (edge, sign) = match saved.place {
                        Place::Left => (
                            Rect::from_center_size(rect.right_center(), vec2(6.0, rect.height())),
                            1.0,
                        ),
                        Place::Right => (
                            Rect::from_center_size(rect.left_center(), vec2(6.0, rect.height())),
                            -1.0,
                        ),
                        Place::Top => (
                            Rect::from_center_size(rect.center_bottom(), vec2(rect.width(), 6.0)),
                            1.0,
                        ),
                        _ => (
                            Rect::from_center_size(rect.center_top(), vec2(rect.width(), 6.0)),
                            -1.0,
                        ),
                    };
                    let drag = ui.interact(edge, window_id.with("resize"), egui::Sense::drag());
                    if drag.dragged() {
                        let delta = ui.input(|i| i.pointer.delta());
                        group.extent = (extent + sign * if horizontal { delta.x } else { delta.y })
                            .clamp(80.0, 1200.0);
                    }
                } else {
                    group.place = Place::Floating;
                    let safe = safe_rect(group.rect, viewport);
                    group.rect = [safe.min.x, safe.min.y, safe.width(), safe.height()];
                }
            }
            if let Some(actual) = self.groups.iter_mut().find(|g| g.panels == saved.panels) {
                *actual = group;
            }
        }
        if let Some((panel, place, target)) = command {
            if let Some(place) = place {
                self.move_panel(&panel, place);
            } else if let Some(target) = target {
                self.tab_with(&panel, &target);
            } else {
                self.close(&panel);
            }
        }
        center
    }
}

/// Keep floating geometry visible after a monitor or scale change.
/// Args: saved logical rect and current viewport. Returns a finite contained rectangle.
pub fn safe_rect(saved: [f32; 4], viewport: Rect) -> Rect {
    let fallback = [viewport.min.x, viewport.min.y, 320.0, 420.0];
    let saved = if saved.iter().all(|v| v.is_finite()) {
        saved
    } else {
        fallback
    };
    let width = saved[2].clamp(120.0, 1200.0).min(viewport.width().max(0.0));
    let height = saved[3].clamp(80.0, 1000.0).min(viewport.height().max(0.0));
    let x = saved[0].clamp(viewport.min.x, (viewport.max.x - width).max(viewport.min.x));
    let y = saved[1].clamp(
        viewport.min.y,
        (viewport.max.y - height).max(viewport.min.y),
    );
    Rect::from_min_size(pos2(x, y), vec2(width, height))
}

impl Book {
    /// Draw task and panel menus.
    /// Args: UI, application presets and labeled panel registry. Returns no value; task edits persist independently.
    pub fn toolbar(
        &mut self,
        ui: &mut Ui,
        presets: &[(&str, Workspace)],
        registry: &[(&str, &str)],
    ) {
        if !presets.iter().any(|(name, _)| *name == self.active) {
            self.active = presets.first().map_or("", |p| p.0).into();
        }
        for (name, preset) in presets {
            self.tasks
                .entry((*name).into())
                .or_insert_with(|| preset.clone());
        }
        ui.horizontal_wrapped(|ui| {
            for (name, _) in presets {
                if ui.selectable_label(self.active == *name, *name).clicked() {
                    self.active = (*name).into();
                }
            }
            if let Some(layout) = self.tasks.get_mut(&self.active) {
                ui.menu_button("Panels", |ui| {
                    for &(id, label) in registry {
                        let hidden = layout.hidden.contains_key(id);
                        if ui
                            .button(format!(
                                "{} {label}",
                                if hidden { "Restore" } else { "Focus" }
                            ))
                            .clicked()
                        {
                            layout.restore(id);
                            for group in &mut layout.groups {
                                if let Some(i) = group.panels.iter().position(|p| p == id) {
                                    group.active = i;
                                }
                            }
                            layout.focus = Some(id.into());
                        }
                    }
                    if ui.button("Restore all panels").clicked() {
                        for &(id, _) in registry {
                            layout.restore(id);
                        }
                    }
                    if ui.button("Reset this task").clicked() {
                        if let Some((_, preset)) = presets.iter().find(|p| p.0 == self.active) {
                            *layout = preset.clone();
                        }
                    }
                });
            }
        });
    }

    /// Read a saved workspace book.
    /// Args: settings path. Returns defaults for a missing file and an error for invalid data.
    pub fn read(path: &std::path::Path) -> Result<Self, String> {
        match std::fs::read(path) {
            Ok(bytes) if bytes.len() <= 1_048_576 => {
                serde_json::from_slice(&bytes).map_err(|e| e.to_string())
            }
            Ok(_) => Err("Workspace settings exceed 1 MiB".into()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(e.to_string()),
        }
    }

    /// Atomically save workspace choices outside documents.
    /// Args: settings path. Returns a descriptive error on failure.
    pub fn write(&self, path: &std::path::Path) -> Result<(), String> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let temporary = path.with_extension(format!("{}.pending", std::process::id()));
        let bytes = serde_json::to_vec_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(&temporary, bytes).map_err(|e| e.to_string())?;
        std::fs::rename(&temporary, path).map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn shared_workspace_restores_every_tab_permutation_after_reopen() {
        let permutations = [
            ["a", "b", "c"],
            ["a", "c", "b"],
            ["b", "a", "c"],
            ["b", "c", "a"],
            ["c", "a", "b"],
            ["c", "b", "a"],
        ];
        for original in permutations {
            let group = Group {
                panels: original.iter().map(|panel| (*panel).into()).collect(),
                place: Place::Floating,
                rect: [60.0, 70.0, 360.0, 440.0],
                ..Default::default()
            };
            for restore in permutations {
                let mut layout = Workspace {
                    groups: vec![group.clone()],
                    ..Default::default()
                };
                layout.close_group(&group);
                for (step, panel) in restore.iter().enumerate() {
                    layout = serde_json::from_slice(&serde_json::to_vec(&layout).unwrap()).unwrap();
                    layout.restore(panel);
                    assert_eq!(layout.groups.len(), 1);
                    let expected: Vec<_> = original
                        .iter()
                        .filter(|id| restore[..=step].contains(id))
                        .map(|id| (*id).to_string())
                        .collect();
                    assert_eq!(
                        layout.groups[0].panels, expected,
                        "original={original:?} restore={restore:?} step={step}"
                    );
                    assert_eq!(layout.groups[0].place, Place::Floating);
                    assert_eq!(layout.groups[0].rect, group.rect);
                    assert_eq!(layout.hidden.len(), 2 - step);
                }
            }
        }
    }
    #[test]
    fn shared_workspace_restores_tab_order_and_closed_floating_groups() {
        let mut layout = Workspace::preset(&[
            ("a", Place::Right),
            ("b", Place::Right),
            ("c", Place::Right),
        ]);
        layout.close("b");
        layout.restore("b");
        assert_eq!(layout.groups[0].panels, vec!["a", "b", "c"]);
        layout.move_panel("a", Place::Floating);
        layout.tab_with("b", "a");
        layout.tab_with("c", "a");
        let group = layout.groups[0].clone();
        layout.close_group(&group);
        assert!(layout.groups.is_empty());
        for panel in &group.panels {
            layout.restore(panel);
        }
        assert_eq!(layout.groups.len(), 1);
        assert_eq!(layout.groups[0].panels, group.panels);
        assert_eq!(layout.groups[0].place, Place::Floating);
    }
    #[test]
    fn shared_workspace_move_tab_close_restore_and_scale_safety() {
        let mut layout = Workspace::preset(&[("inspector", Place::Right), ("layers", Place::Left)]);
        layout.move_panel("inspector", Place::Floating);
        assert!(layout.tab_with("layers", "inspector"));
        layout.close("layers");
        layout.restore("layers");
        assert_eq!(layout.groups.len(), 1);
        assert_eq!(layout.groups[0].panels, vec!["inspector", "layers"]);
        for scale in [1.0, 1.5, 2.0] {
            let viewport = Rect::from_min_size(pos2(0.0, 0.0), vec2(1280.0 / scale, 720.0 / scale));
            let rect = safe_rect([8000.0, -300.0, 900.0, 700.0], viewport);
            assert!(viewport.contains_rect(rect));
        }
        let bytes = serde_json::to_vec(&layout).unwrap();
        assert_eq!(layout, serde_json::from_slice(&bytes).unwrap());
    }
    #[test]
    fn shared_workspace_rejects_unknown_duplicate_and_nonfinite_geometry() {
        let mut layout = Workspace::preset(&[("a", Place::Right)]);
        layout.groups.push(layout.groups[0].clone());
        layout.groups[0].extent = f32::NAN;
        layout.normalize(&["a", "b"]);
        assert_eq!(layout.groups.len(), 1);
        assert!(layout.groups[0].extent.is_finite());
        assert!(layout.hidden.contains_key("b"));
        layout.move_panel("canvas", Place::Left);
        layout.normalize(&["a", "b"]);
        assert!(
            !layout
                .groups
                .iter()
                .any(|g| g.panels.iter().any(|p| p == "canvas"))
        );
    }
}
