use super::keyboard_controls::choice;
use super::workspace::{Book, Group, Place, Workspace};
use eframe::egui::{self, Id, Key, Modifiers, Ui};

const ID: &str = "keyboard-panels";

/// Open the existing keyboard panel arranger from the Window menu.
/// Args: context, current workspace and registry. Returns nothing; Apply retains its existing validation.
pub(super) fn open(ctx: &egui::Context, book: &Book, registry: &[(&str, &str)]) {
    if let Some(dialog) = Dialog::new(book, registry, ctx) {
        ctx.data_mut(|data| data.insert_temp(Id::new(ID), dialog));
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Action {
    Focus,
    Left,
    Right,
    Top,
    Bottom,
    Float,
    Tab,
    Close,
    Restore,
    Resize,
}

impl Action {
    const ALL: [Self; 10] = [
        Self::Focus,
        Self::Left,
        Self::Right,
        Self::Top,
        Self::Bottom,
        Self::Float,
        Self::Tab,
        Self::Close,
        Self::Restore,
        Self::Resize,
    ];
    fn label(self) -> &'static str {
        match self {
            Self::Focus => "Focus panel",
            Self::Left => "Move left",
            Self::Right => "Move right",
            Self::Top => "Move top",
            Self::Bottom => "Move bottom",
            Self::Float => "Float panel",
            Self::Tab => "Tab with panel",
            Self::Close => "Close panel",
            Self::Restore => "Restore panel",
            Self::Resize => "Resize panel",
        }
    }
}

#[derive(Debug)]
pub(super) struct PanelRequest {
    pub focus: Option<String>,
    pub size: Option<(String, egui::Vec2)>,
}

#[derive(Clone)]
struct Dialog {
    instance: Id,
    task: String,
    baseline: Workspace,
    panel: String,
    action: Action,
    tab: String,
    geometry: Group,
    focus: bool,
    previous_focus: Option<Id>,
    error: Option<String>,
}

fn group<'a>(layout: &'a Workspace, panel: &str) -> Option<&'a Group> {
    layout
        .groups
        .iter()
        .find(|g| g.panels.iter().any(|p| p == panel))
        .or_else(|| layout.hidden.get(panel))
}

impl Dialog {
    fn new(book: &Book, registry: &[(&str, &str)], ctx: &egui::Context) -> Option<Self> {
        let baseline = book.tasks.get(&book.active)?.clone();
        let panel = baseline
            .focus
            .clone()
            .unwrap_or_else(|| registry.first().map_or("", |p| p.0).into());
        let geometry = group(&baseline, &panel).cloned().unwrap_or_default();
        let tab = registry
            .iter()
            .find(|p| p.0 != panel)
            .map_or("", |p| p.0)
            .into();
        Some(Self {
            instance: super::keyboard_controls::draft_id(ctx, ID),
            task: book.active.clone(),
            baseline,
            panel,
            action: Action::Focus,
            tab,
            geometry,
            focus: true,
            previous_focus: ctx.memory(|m| m.focused()),
            error: None,
        })
    }

    fn apply(&self, book: &mut Book, registry: &[(&str, &str)]) -> Result<PanelRequest, String> {
        if book.active != self.task || book.tasks.get(&self.task) != Some(&self.baseline) {
            return Err("The workspace changed. Open Panels again.".into());
        }
        if !registry.iter().any(|p| p.0 == self.panel) {
            return Err("This panel is no longer available.".into());
        }
        if matches!(self.action, Action::Resize | Action::Float)
            && (!self.geometry.extent.is_finite()
                || !self.geometry.rect.iter().all(|v| v.is_finite())
                || !(80.0..=1200.).contains(&self.geometry.extent)
                || self.geometry.rect[2] < 160.
                || self.geometry.rect[3] < 120.)
        {
            return Err("Enter finite panel dimensions within the shown range.".into());
        }
        let mut layout = self.baseline.clone();
        match self.action {
            Action::Focus | Action::Restore => layout.restore(&self.panel),
            Action::Left => layout.move_panel(&self.panel, Place::Left),
            Action::Right => layout.move_panel(&self.panel, Place::Right),
            Action::Top => layout.move_panel(&self.panel, Place::Top),
            Action::Bottom => layout.move_panel(&self.panel, Place::Bottom),
            Action::Float => layout.move_panel(&self.panel, Place::Floating),
            Action::Tab => {
                if self.tab == self.panel
                    || !registry.iter().any(|p| p.0 == self.tab)
                    || !layout.groups.iter().any(|g| g.panels.contains(&self.tab))
                {
                    return Err("Choose another visible panel to tab with.".into());
                }
                layout.tab_with(&self.panel, &self.tab);
            }
            Action::Close => layout.close(&self.panel),
            Action::Resize => {
                if !layout.groups.iter().any(|g| g.panels.contains(&self.panel)) {
                    return Err("Restore the panel before resizing it.".into());
                }
            }
        }
        let mut request = None;
        if self.action != Action::Close {
            if let Some(group) = layout
                .groups
                .iter_mut()
                .find(|g| g.panels.contains(&self.panel))
            {
                if matches!(self.action, Action::Resize | Action::Float) {
                    group.extent = self.geometry.extent;
                    group.rect = self.geometry.rect;
                    if group.place == Place::Floating {
                        request =
                            Some((self.panel.clone(), egui::vec2(group.rect[2], group.rect[3])));
                    }
                }
                if let Some(active) = group.panels.iter().position(|p| *p == self.panel) {
                    group.active = active;
                }
            }
            if self.action == Action::Resize
                && let Some(place) = layout
                    .groups
                    .iter()
                    .find(|group| group.panels.contains(&self.panel))
                    .map(|group| group.place)
                    .filter(|place| *place != Place::Floating)
            {
                for group in layout.groups.iter_mut().filter(|group| group.place == place) {
                    group.extent = self.geometry.extent;
                }
            }
            layout.focus = Some(self.panel.clone());
        } else {
            layout.focus = layout
                .groups
                .first()
                .and_then(|g| g.panels.get(g.active))
                .cloned();
        }
        layout.normalize(&registry.iter().map(|p| p.0).collect::<Vec<_>>());
        let request = PanelRequest {
            focus: layout.focus.clone(),
            size: request,
        };
        book.tasks.insert(self.task.clone(), layout);
        Ok(request)
    }
}

/// Draw a preference-only numeric dimension.
/// Args: UI, property identity, label, draft value and range. Returns after accepting shared field changes.
fn number(
    ui: &mut Ui,
    id: &str,
    label: &str,
    value: &mut f32,
    range: std::ops::RangeInclusive<f32>,
) -> bool {
    let mut candidate = *value;
    let result = super::fields::number(
        ui,
        id,
        label,
        &mut candidate,
        range,
        " px",
        None,
        Default::default(),
    );
    if result.changed {
        *value = candidate;
    }
    super::fields::number_is_valid(&result)
}

pub(super) fn is_open(ctx: &egui::Context) -> bool {
    ctx.data(|data| data.get_temp::<Dialog>(Id::new(ID)).is_some())
}

/// Offer keyboard alternatives to the existing workspace operations.
/// Args: root UI, current workspace book and panel registry. Returns a one-frame floating-size request.
/// The caller forwards that request through the canonical workspace size contract before rendering panels.
pub(super) fn show(
    ui: &mut Ui,
    book: &mut Book,
    registry: &[(&str, &str)],
) -> Option<PanelRequest> {
    let ctx = ui.ctx().clone();
    let activate = ui.input_mut(|input| {
        input.consume_key(Modifiers::CTRL, Key::F6)
            || input.consume_key(Modifiers::COMMAND, Key::F6)
    });
    let mut dialog = ctx.data(|data| data.get_temp::<Dialog>(Id::new(ID)));
    if activate && ui.is_enabled() && dialog.is_none() {
        dialog = Dialog::new(book, registry, &ctx);
    }
    let Some(mut dialog) = dialog else {
        return None;
    };
    ui.input_mut(|input| {
        input.consume_key(Modifiers::NONE, Key::F6);
        input.consume_key(Modifiers::SHIFT, Key::F6);
    });
    let mut apply = false;
    let mut cancel = false;
    let mut valid = true;
    let modal = egui::Modal::new(Id::new(ID)).show(&ctx, |ui| {
        ui.set_width(360.);
        ui.heading("Panels");
        let before = dialog.panel.clone();
        let choices = registry
            .iter()
            .map(|(id, name)| (id.to_string(), name.to_string()))
            .collect::<Vec<_>>();
        let response = choice(ui, "panel", "Panel", &mut dialog.panel, &choices);
        if dialog.focus && !ui.is_sizing_pass() {
            response.request_focus();
            dialog.focus = false;
        }
        if dialog.panel != before {
            dialog.geometry = group(&dialog.baseline, &dialog.panel)
                .cloned()
                .unwrap_or_default();
        }
        ui.push_id((ID, dialog.instance, &dialog.task, &dialog.panel), |ui| {
            let actions = Action::ALL
                .into_iter()
                .map(|action| (action, action.label().into()))
                .collect::<Vec<_>>();
            let action_response =
                choice(ui, "action", "Panel action", &mut dialog.action, &actions);
            if dialog.action == Action::Tab {
                let targets = choices
                    .iter()
                    .filter(|(id, _)| {
                        *id != dialog.panel
                            && dialog
                                .baseline
                                .groups
                                .iter()
                                .any(|group| group.panels.contains(id))
                    })
                    .cloned()
                    .collect::<Vec<_>>();
                choice(ui, "tab", "Tab with panel", &mut dialog.tab, &targets);
            }
            if matches!(dialog.action, Action::Resize | Action::Float) {
                let floating =
                    dialog.action == Action::Float || dialog.geometry.place == Place::Floating;
                if floating {
                    let viewport = ui.ctx().viewport_rect();
                    valid &= number(
                        ui,
                        "x",
                        "Window X",
                        &mut dialog.geometry.rect[0],
                        0.0..=viewport.width().max(0.),
                    );
                    valid &= number(
                        ui,
                        "y",
                        "Window Y",
                        &mut dialog.geometry.rect[1],
                        0.0..=viewport.height().max(0.),
                    );
                    valid &= number(
                        ui,
                        "width",
                        "Window width",
                        &mut dialog.geometry.rect[2],
                        160.0..=viewport.width().max(160.),
                    );
                    valid &= number(
                        ui,
                        "height",
                        "Window height",
                        &mut dialog.geometry.rect[3],
                        120.0..=viewport.height().max(120.),
                    );
                } else {
                    valid &= number(
                        ui,
                        "extent",
                        "Dock size",
                        &mut dialog.geometry.extent,
                        80.0..=1200.,
                    );
                }
            }
            if let Some(error) = &dialog.error {
                super::welcome::control_error(ui, &[&action_response], error);
            }
            ui.horizontal(|ui| {
                let response = ui.add_enabled(valid, egui::Button::new("Apply panel"));
                super::welcome::focus_control(ui, &response);
                apply = valid && response.clicked();
                let response = ui.button("Cancel");
                super::welcome::focus_control(ui, &response);
                cancel = response.clicked();
            });
            if ui.input_mut(|input| {
                input.consume_key(Modifiers::CTRL, Key::Enter)
                    || input.consume_key(Modifiers::COMMAND, Key::Enter)
            }) {
                apply = valid;
            }
        });
    });
    let mut requested_size = None;
    let applied = if apply {
        match dialog.apply(book, registry) {
            Ok(request) => {
                requested_size = Some(request);
                true
            }
            Err(error) => {
                dialog.error = Some(error);
                false
            }
        }
    } else {
        false
    };
    let close = applied || cancel || modal.should_close();
    if close && !applied {
        requested_size = Some(PanelRequest {
            focus: dialog.baseline.focus.clone(),
            size: None,
        });
        if dialog.baseline.focus.is_none()
            && let Some(id) = dialog.previous_focus
        {
            ctx.memory_mut(|m| m.request_focus(id));
        }
    }
    ctx.data_mut(|data| {
        if close {
            data.remove::<Dialog>(Id::new(ID));
        } else {
            data.insert_temp(Id::new(ID), dialog);
        }
    });
    requested_size
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::welcome::keyboard_tests as keys;
    const REGISTRY: &[(&str, &str)] = &[("properties", "Properties"), ("layers", "Layers")];

    fn fixture() -> (egui::Context, Book) {
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        super::super::theme::apply(&ctx);
        let mut book = Book::default();
        book.active = "Design".into();
        book.tasks.insert(
            "Design".into(),
            Workspace::preset(&[("properties", Place::Right), ("layers", Place::Left)]),
        );
        (ctx, book)
    }

    fn key(key: Key, modifiers: Modifiers) -> Vec<egui::Event> {
        [true, false]
            .into_iter()
            .map(|pressed| egui::Event::Key {
                key,
                pressed,
                physical_key: Some(key),
                repeat: false,
                modifiers,
            })
            .collect()
    }

    fn frame(
        ctx: &egui::Context,
        book: &mut Book,
        mut events: Vec<egui::Event>,
    ) -> (egui::FullOutput, f32) {
        let modifiers = events
            .iter()
            .find_map(|event| match event {
                egui::Event::Key { modifiers, .. } => Some(*modifiers),
                _ => None,
            })
            .unwrap_or_default();
        events.insert(0, egui::Event::ModifiersChanged(modifiers));
        let mut left = 0.;
        let mut output = ctx.run_ui(
            egui::RawInput {
                events,
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1000., 720.),
                )),
                ..Default::default()
            },
            |ui| {
                if let Some(request) = show(ui, book, REGISTRY) {
                    let layout = book.tasks.get_mut("Design").unwrap();
                    layout.normalize(&REGISTRY.iter().map(|panel| panel.0).collect::<Vec<_>>());
                    if let Some((panel, size)) = request.size {
                        assert!(layout.request_floating_size(
                            ctx,
                            "keyboard-test-workspace",
                            &panel,
                            size
                        ));
                    }
                    if let Some(panel) = request.focus {
                        assert!(layout.request_focus(ctx, "keyboard-test-workspace", &panel));
                    }
                }
                let origin = ui.available_rect_before_wrap().left();
                let center = book.tasks.get_mut("Design").unwrap().show(
                    ui,
                    "keyboard-test-workspace",
                    REGISTRY,
                    |ui, panel| {
                        ui.label(panel);
                    },
                );
                left = center.left() - origin;
            },
        );
        output.textures_delta.clear();
        (output, left)
    }

    fn tab_to(ctx: &egui::Context, book: &mut Book, label: &str) {
        for _ in 0..24 {
            let (output, _) = frame(ctx, book, vec![]);
            let tree = output.platform_output.accesskit_update.unwrap();
            let id = tree
                .nodes
                .iter()
                .find(|(_, n)| n.label() == Some(label))
                .unwrap_or_else(|| panic!("missing {label}"))
                .0;
            if ctx.memory(|m| m.focused()).map(|id| id.accesskit_id()) == Some(id) {
                return;
            }
            frame(ctx, book, key(Key::Tab, Modifiers::NONE));
        }
        panic!("Tab did not reach {label}");
    }

    fn action(ctx: &egui::Context, book: &mut Book, label: &str) {
        tab_to(ctx, book, "Panel action");
        frame(ctx, book, key(Key::Home, Modifiers::NONE));
        let index = Action::ALL
            .iter()
            .position(|action| action.label() == label)
            .unwrap();
        for _ in 0..index {
            frame(ctx, book, key(Key::ArrowDown, Modifiers::NONE));
        }
        let (output, _) = frame(ctx, book, vec![]);
        super::super::welcome::keyboard_tests::focused(ctx, &output, "Panel action");
        assert_eq!(
            ctx.data(|data| data.get_temp::<Dialog>(Id::new(ID)))
                .unwrap()
                .action
                .label(),
            label
        );
    }

    fn apply(ctx: &egui::Context, book: &mut Book) -> egui::FullOutput {
        tab_to(ctx, book, "Apply panel");
        let first=frame(ctx, book, key(Key::Enter, Modifiers::NONE)).0;
        assert!(!is_open(ctx));
        let panel=book.tasks["Design"].focus.as_deref().unwrap();
        let label=REGISTRY.iter().find(|entry|entry.0==panel).unwrap().1;
        let observed=|output:&egui::FullOutput|output.platform_output.events.iter().any(|event|
            matches!(event,egui::output::OutputEvent::FocusGained(info) if info.label.as_deref()==Some(label)&&info.enabled));
        let mut native_event=observed(&first);
        let mut output = frame(ctx, book, vec![]).0;
        native_event|=observed(&output);
        for _ in 0..2 {
            output = frame(ctx, book, vec![]).0;
            native_event|=observed(&output);
        }
        assert!(native_event,"Actual native FocusGained must name the requested panel {label}");
        output
    }

    fn open(ctx: &egui::Context, book: &mut Book) {
        frame(
            ctx,
            book,
            key(Key::F6, Modifiers::CTRL | Modifiers::COMMAND),
        );
        frame(ctx, book, vec![]);
        assert!(is_open(ctx));
        tab_to(ctx, book, "Panel");
        frame(ctx, book, key(Key::Home, Modifiers::NONE));
    }

    fn focused_panel(ctx: &egui::Context, output: &egui::FullOutput) {
        keys::focused_role(ctx,output,"Properties",egui::accesskit::Role::Button);
    }

    #[test]
    fn panel_keyboard_native_actions_float_tab_close_restore_and_repeat_focus() {
        let (ctx, mut book) = fixture();
        frame(&ctx, &mut book, vec![]);
        for name in [
            "Move left",
            "Move right",
            "Move top",
            "Move bottom",
            "Float panel",
        ] {
            open(&ctx, &mut book);
            action(&ctx, &mut book, name);
            if name == "Float panel" {
                for (label, text) in [("Window width", "480"), ("Window height", "300")] {
                    tab_to(&ctx, &mut book, label);
                    keys::replace(text, |events| frame(&ctx, &mut book, events).0);
                }
            }
            let output = apply(&ctx, &mut book);
            focused_panel(&ctx, &output);
        }
        let floating = frame(&ctx, &mut book, vec![]).0;
        let (_, window) = floating
            .platform_output
            .accesskit_update
            .as_ref()
            .unwrap()
            .nodes
            .iter()
            .find(|(_, node)| {
                node.role() == egui::accesskit::Role::Window && node.label() == Some("Properties")
            })
            .unwrap();
        let bounds = window.bounds().unwrap();
        assert!((bounds.x1 - bounds.x0 - 480.).abs() <= 1.);
        assert!((bounds.y1 - bounds.y0 - 300.).abs() <= 1.);
        for _ in 0..2 {
            open(&ctx, &mut book);
            action(&ctx, &mut book, "Focus panel");
            let output = apply(&ctx, &mut book);
            focused_panel(&ctx, &output);
        }
        open(&ctx, &mut book);
        action(&ctx, &mut book, "Tab with panel");
        tab_to(&ctx, &mut book, "Tab with panel");
        frame(&ctx, &mut book, key(Key::Home, Modifiers::NONE));
        let output = apply(&ctx, &mut book);
        focused_panel(&ctx, &output);
        assert!(
            group(&book.tasks["Design"], "properties")
                .unwrap()
                .panels
                .contains(&"layers".to_string())
        );
        open(&ctx, &mut book);
        action(&ctx, &mut book, "Close panel");
        apply(&ctx, &mut book);
        assert!(book.tasks["Design"].hidden.contains_key("properties"));
        open(&ctx, &mut book);
        tab_to(&ctx, &mut book, "Panel");
        frame(&ctx, &mut book, key(Key::Home, Modifiers::NONE));
        action(&ctx, &mut book, "Restore panel");
        let output = apply(&ctx, &mut book);
        assert!(!book.tasks["Design"].hidden.contains_key("properties"));
        focused_panel(&ctx, &output);
        let before = book.clone();
        open(&ctx, &mut book);
        tab_to(&ctx, &mut book, "Cancel");
        let first = frame(&ctx, &mut book, key(Key::Enter, Modifiers::NONE)).0;
        let observed=|output:&egui::FullOutput|output.platform_output.events.iter().any(|event|
            matches!(event,egui::output::OutputEvent::FocusGained(info) if info.label.as_deref()==Some("Properties")&&info.enabled));
        let mut event = observed(&first);
        let mut output = first;
        for _ in 0..3 { output=frame(&ctx,&mut book,vec![]).0; event|=observed(&output); }
        assert!(event,"Cancel must deliver the actual native Properties focus event after the modal layer closes");
        focused_panel(&ctx, &output);
        assert_eq!(book, before);
    }

    #[test]
    fn panel_keyboard_rejected_native_size_disables_apply_and_keeps_workspace() {
        let (ctx, mut book) = fixture();
        frame(&ctx, &mut book, vec![]);
        let before = book.clone();
        open(&ctx, &mut book);
        action(&ctx, &mut book, "Resize panel");
        tab_to(&ctx, &mut book, "Dock size");
        keys::replace("-", |events| frame(&ctx, &mut book, events).0);
        let output = frame(&ctx, &mut book, vec![]).0;
        keys::associated_error(&output, "Dock size");
        assert!(keys::named(&output, "Apply panel").1.is_disabled());
        frame(
            &ctx,
            &mut book,
            key(Key::Enter, Modifiers::CTRL | Modifiers::COMMAND),
        );
        assert!(is_open(&ctx));
        assert_eq!(book, before);
        let output = keys::tab(false, |events| frame(&ctx, &mut book, events).0);
        keys::focused(&ctx, &output, "Cancel");
        frame(&ctx, &mut book, key(Key::Enter, Modifiers::NONE));
        assert!(!is_open(&ctx));
        assert_eq!(book, before);
        open(&ctx, &mut book);
        action(&ctx, &mut book, "Resize panel");
        let output = frame(&ctx, &mut book, vec![]).0;
        assert_eq!(keys::named(&output, "Dock size").1.invalid(), None);
        assert!(!keys::named(&output, "Apply panel").1.is_disabled());
        tab_to(&ctx, &mut book, "Dock size");
        keys::replace("320", |events| frame(&ctx, &mut book, events).0);
        apply(&ctx, &mut book);
        assert_eq!(
            group(&book.tasks["Design"], "properties").unwrap().extent,
            320.
        );
    }

    #[test]
    fn panel_keyboard_moves_resizes_and_reopens_preferences_without_artwork_commands() {
        let (ctx, mut book) = fixture();
        frame(&ctx, &mut book, vec![]);
        frame(
            &ctx,
            &mut book,
            key(Key::F6, Modifiers::CTRL | Modifiers::COMMAND),
        );
        let dialog = ctx.data(|d| d.get_temp::<Dialog>(Id::new(ID))).unwrap();
        assert_eq!(
            book.tasks["Design"], dialog.baseline,
            "Ctrl+F6 must not also cycle the underlying F6 route"
        );
        tab_to(&ctx, &mut book, "Panel");
        frame(&ctx, &mut book, key(Key::Home, Modifiers::NONE));
        action(&ctx, &mut book, "Move left");
        frame(
            &ctx,
            &mut book,
            key(Key::Enter, Modifiers::CTRL | Modifiers::COMMAND),
        );
        frame(&ctx, &mut book, vec![]);
        assert_eq!(
            group(&book.tasks["Design"], "properties").unwrap().place,
            Place::Left
        );
        frame(
            &ctx,
            &mut book,
            key(Key::F6, Modifiers::CTRL | Modifiers::COMMAND),
        );
        action(&ctx, &mut book, "Resize panel");
        tab_to(&ctx, &mut book, "Dock size");
        frame(
            &ctx,
            &mut book,
            key(Key::A, Modifiers::CTRL | Modifiers::COMMAND),
        );
        frame(&ctx, &mut book, vec![egui::Event::Text("360".into())]);
        frame(
            &ctx,
            &mut book,
            key(Key::Enter, Modifiers::CTRL | Modifiers::COMMAND),
        );
        let (_, left) = frame(&ctx, &mut book, vec![]);
        assert!(
            (left - 360.).abs() <= 1.,
            "actual rendered dock width: {left}"
        );
        assert_eq!(
            group(&book.tasks["Design"], "properties").unwrap().extent,
            360.
        );
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join(".artifacts")
            .join(format!(
                "keyboard-panels-test-{}-{}",
                std::process::id(),
                crate::document::next_id()
            ));
        let path = root.join("workspaces.json");
        book.write(&path).unwrap();
        assert_eq!(Book::read(&path).unwrap(), book);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn panel_keyboard_native_resize_changes_the_whole_shared_dock_column() {
        let (ctx, mut book) = fixture();
        for (index, group) in book.tasks.get_mut("Design").unwrap().groups.iter_mut().enumerate() {
            group.place = Place::Left;
            group.extent = if index == 0 { 320. } else { 420. };
        }
        frame(&ctx, &mut book, vec![]);
        for extent in [256., 420.] {
            open(&ctx, &mut book);
            action(&ctx, &mut book, "Resize panel");
            tab_to(&ctx, &mut book, "Dock size");
            keys::replace(&format!("{extent}"), |events| frame(&ctx, &mut book, events).0);
            let output = apply(&ctx, &mut book);
            focused_panel(&ctx, &output);
            let (_, actual) = frame(&ctx, &mut book, vec![]);
            assert!((actual - extent).abs() <= 1., "real shared column stayed {actual} after {extent} request");
            assert!(book.tasks["Design"].groups.iter().all(|group| group.extent == extent));
        }
    }

    #[test]
    fn panel_keyboard_shared_dock_resize_preserves_other_edges_hidden_and_floating_groups() {
        let registry = &[("properties", "Properties"), ("layers", "Layers"), ("history", "History"), ("actions", "Actions")];
        for place in [Place::Left, Place::Right, Place::Top, Place::Bottom] {
            let (ctx, mut book) = fixture();
            let layout = book.tasks.get_mut("Design").unwrap();
            for group in &mut layout.groups { group.place = place; }
            let mut other = Group::default();
            other.panels = vec!["history".into()];
            other.place = if place == Place::Left { Place::Right } else { Place::Left };
            other.extent = 512.;
            layout.groups.push(other.clone());
            let mut hidden = Group::default();
            hidden.panels = vec!["actions".into()];
            hidden.place = place;
            hidden.extent = 700.;
            layout.hidden.insert("actions".into(), hidden.clone());
            let mut draft = Dialog::new(&book, registry, &ctx).unwrap();
            draft.panel = "properties".into();
            draft.action = Action::Resize;
            draft.geometry.extent = 256.;
            assert!(draft.apply(&mut book, registry).unwrap().size.is_none());
            assert_eq!(group(&book.tasks["Design"], "properties").unwrap().extent, 256.);
            assert_eq!(group(&book.tasks["Design"], "layers").unwrap().extent, 256.);
            assert_eq!(group(&book.tasks["Design"], "history").unwrap(), &other);
            assert_eq!(book.tasks["Design"].hidden["actions"], hidden);
        }
        let (ctx, mut book) = fixture();
        for group in &mut book.tasks.get_mut("Design").unwrap().groups { group.place = Place::Floating; }
        let other = group(&book.tasks["Design"], "layers").unwrap().clone();
        let mut draft = Dialog::new(&book, REGISTRY, &ctx).unwrap();
        draft.panel = "properties".into();
        draft.action = Action::Resize;
        draft.geometry.rect[2] = 480.;
        draft.geometry.rect[3] = 300.;
        assert_eq!(draft.apply(&mut book, REGISTRY).unwrap().size, Some(("properties".into(), egui::vec2(480., 300.))));
        assert_eq!(group(&book.tasks["Design"], "layers").unwrap(), &other);
    }

    #[test]
    fn panel_keyboard_close_restore_tab_float_and_stale_cancel_use_existing_workspace_model() {
        let (ctx, mut book) = fixture();
        for action in [
            Action::Close,
            Action::Restore,
            Action::Tab,
            Action::Float,
            Action::Right,
            Action::Top,
            Action::Bottom,
        ] {
            let mut draft = Dialog::new(&book, REGISTRY, &ctx).unwrap();
            draft.panel = "properties".into();
            draft.tab = "layers".into();
            draft.geometry = group(&draft.baseline, &draft.panel)
                .cloned()
                .unwrap_or_default();
            draft.action = action;
            let request = draft.apply(&mut book, REGISTRY).unwrap();
            if action == Action::Float {
                let (panel, size) = request.size.unwrap();
                assert_eq!(panel, "properties");
                assert_eq!(
                    size,
                    egui::vec2(draft.geometry.rect[2], draft.geometry.rect[3])
                );
            } else {
                assert!(request.size.is_none());
            }
            assert!(
                book.tasks["Design"].hidden.contains_key("properties") == (action == Action::Close)
            );
        }
        let before = book.clone();
        let draft = Dialog::new(&book, REGISTRY, &ctx).unwrap();
        book.active = "Motion".into();
        assert!(
            draft
                .apply(&mut book, REGISTRY)
                .unwrap_err()
                .contains("changed")
        );
        book = before;
        frame(
            &ctx,
            &mut book,
            key(Key::F6, Modifiers::CTRL | Modifiers::COMMAND),
        );
        let before = book.clone();
        frame(&ctx, &mut book, key(Key::Escape, Modifiers::NONE));
        assert_eq!(book, before);
        assert!(ctx.data(|d| d.get_temp::<Dialog>(Id::new(ID))).is_none());
    }
}
