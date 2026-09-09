//! Read-only descriptions of the same contexts and chords used by dispatch.
use super::shortcuts::{Shortcut, ShortcutFocus, held_modifiers, key_shortcut};
use super::*;
use egui::{Key, Modifiers};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KeyHint {
    pub keys: &'static str,
    pub label: &'static str,
    pub active: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct KeyHints {
    pub context: &'static str,
    pub gestures: Vec<KeyHint>,
    pub keys: Vec<KeyHint>,
}

impl Shortcut {
    fn hint(self, shift: bool) -> (&'static str, &'static str) {
        use Shortcut::*;
        match self {
            Save => ("Ctrl+S", "Save"),
            SaveAs => ("Ctrl+Shift+S", "Save as"),
            Open => ("Ctrl+O", "Open"),
            Place => ("Ctrl+Shift+P", "Place artwork"),
            New => ("Ctrl+N", "New document"),
            Export => ("Ctrl+E", "Export PNG"),
            Undo => ("Ctrl+Z", "Undo"),
            Redo if shift => ("Ctrl+Shift+Z", "Redo"),
            Redo => ("Ctrl+Y", "Redo"),
            Copy => ("Ctrl+C", "Copy"),
            Cut => ("Ctrl+X", "Cut"),
            Paste => ("Ctrl+V", "Paste"),
            CopyStyle => ("Ctrl+Alt+C", "Copy style"),
            PasteStyle => ("Ctrl+Alt+V", "Paste style"),
            CopyAdjustments => ("Ctrl+Shift+C", "Copy adjustments"),
            PasteAdjustments => ("Ctrl+Shift+V", "Paste adjustments"),
            Duplicate => ("Ctrl+D", "Duplicate"),
            SelectAll => ("Ctrl+A", "Select all"),
            Combine => ("Ctrl+G", "Combine paths"),
            Release => ("Ctrl+Shift+G", "Release compound"),
            Forward => ("Ctrl+]", "Bring forward"),
            Front => ("Ctrl+Shift+]", "Bring to front"),
            Backward => ("Ctrl+[", "Send backward"),
            Back => ("Ctrl+Shift+[", "Send to back"),
            Fit => ("Ctrl+0", "Fit canvas"),
            ActualSize => ("Ctrl+1", "Actual size"),
            ZoomIn if shift => ("Ctrl+Shift+=", "Zoom in"),
            ZoomIn => ("Ctrl+=", "Zoom in"),
            ZoomOut => ("Ctrl+-", "Zoom out"),
            Help => ("F1", "Shortcut guide"),
            ToggleGuides => ("Ctrl+;", "Show / hide guides"),
            ToggleSnapping => ("Ctrl+Shift+;", "Toggle snapping"),
            FreeTransform => ("Ctrl+T", "Free transform"),
            ToggleKeyHud => ("Ctrl+/", "Show / hide key hints"),
        }
    }

    fn edits_text(self) -> bool {
        matches!(
            self,
            Self::Copy
                | Self::Cut
                | Self::Paste
                | Self::SelectAll
                | Self::Undo
                | Self::Redo
                | Self::FreeTransform
        )
    }
}

impl Studio {
    pub fn key_hints(&self, ctx: &egui::Context) -> KeyHints {
        let focus = self.shortcut_focus(ctx);
        let mut hints = KeyHints::default();
        match focus {
            ShortcutFocus::Inactive => hints.context = "Window inactive",
            ShortcutFocus::Modal => hints.context = "Dialog open",
            ShortcutFocus::Popup => hints.context = "Menu open",
            _ => {}
        }
        if !hints.context.is_empty() {
            return hints;
        }
        let mods = held_modifiers(ctx);
        let command = mods.ctrl || mods.command;
        if focus == ShortcutFocus::Field {
            hints.context = "Control focused";
        } else if focus == ShortcutFocus::Text {
            hints.context = "Editing text";
            hints.gestures = vec![
                KeyHint {
                    keys: "Esc",
                    label: "Finish text",
                    active: ctx.input(|i| i.key_down(Key::Escape)),
                },
                KeyHint {
                    keys: "Shift+← / →",
                    label: "Select text",
                    active: mods.shift && !command && !mods.alt,
                },
                KeyHint {
                    keys: "Enter",
                    label: "New line",
                    active: ctx.input(|i| i.key_down(Key::Enter)),
                },
            ];
        } else if self.show_welcome {
            hints.context = "Start a document";
        } else {
            (hints.context, hints.gestures) = self.gesture_hints(ctx, mods);
        }

        // This is presentation order, not a second binding table. Every candidate
        // is resolved through dispatch, and ALL includes future supported keys.
        let priority = [
            Key::S,
            Key::Z,
            Key::C,
            Key::X,
            Key::V,
            Key::D,
            Key::T,
            Key::A,
            Key::O,
            Key::N,
            Key::E,
        ];
        let mut seen = Vec::new();
        for &key in priority.iter().chain(Key::ALL) {
            let Some(shortcut) = key_shortcut(key, mods) else {
                continue;
            };
            if !shortcut.available(self.persona)
                || seen.contains(&shortcut)
                || ((focus == ShortcutFocus::Field || self.show_welcome) && !shortcut.global())
                || (focus == ShortcutFocus::Text && !shortcut.global() && !shortcut.edits_text())
            {
                continue;
            }
            seen.push(shortcut);
            let (keys, mut label) = shortcut.hint(mods.shift);
            if self.persona == Persona::Photo {
                label = match shortcut {
                    Shortcut::Fit => "Fit photo",
                    Shortcut::Save => "Save selected settings",
                    Shortcut::SelectAll => "Select all photos",
                    _ => label,
                };
            }
            hints.keys.push(KeyHint {
                keys,
                label,
                active: ctx.input(|i| i.key_down(key)),
            });
        }
        if focus == ShortcutFocus::Canvas
            && !self.show_welcome
            && !command
            && !mods.alt
            && !mods.mac_cmd
        {
            let tools = match self.persona {
                Persona::Design => Tool::design_well(),
                Persona::Pixel => Tool::pixel_well(),
                Persona::Photo => Tool::photo_well(),
                Persona::Motion => Tool::motion_well(),
            };
            let mut tool_keys: Vec<_> = tools
                .iter()
                .filter(|tool| !mods.shift || tool.key().starts_with("Shift+"))
                .map(|tool| KeyHint {
                    keys: tool.key(),
                    label: tool.label(),
                    active: self.tool == *tool,
                })
                .collect();
            tool_keys.append(&mut hints.keys);
            hints.keys = tool_keys;
        }
        hints.gestures.sort_by_key(|hint| !hint.active);
        hints
    }

    fn gesture_hints(&self, ctx: &egui::Context, mods: Modifiers) -> (&'static str, Vec<KeyHint>) {
        let mut hints = Vec::new();
        let command = mods.ctrl || mods.command;
        let plain_keys = !command && !mods.alt && !mods.mac_cmd;
        let mut add = |keys, label, active| {
            hints.push(KeyHint {
                keys,
                label,
                active,
            })
        };
        let mut context = self.tool.label();
        let mut snapping = false;
        if self.pending_place.is_some() {
            context = "Place artwork";
            add("Click", "Place here", false);
            add("Drag", "Choose size", false);
            add(
                "Enter",
                "Place in centre",
                plain_keys && ctx.input(|i| i.key_down(Key::Enter)),
            );
            add(
                "Esc",
                "Cancel placement",
                plain_keys && ctx.input(|i| i.key_down(Key::Escape)),
            );
            snapping = true;
        } else if let Some(session) = &self.deformation {
            context = session.cage.mode.label();
            add("Drag", "Move a handle", false);
            add("Shift", "Constrain movement", mods.shift);
            add(
                "Enter",
                "Finish reshape",
                plain_keys && ctx.input(|i| i.key_down(Key::Enter)),
            );
            add(
                "Esc",
                "Cancel reshape",
                plain_keys && ctx.input(|i| i.key_down(Key::Escape)),
            );
        } else if self.persona == Persona::Photo {
            context = "Photo";
            if self.tool == Tool::Crop {
                context = "Crop photo";
                add("Drag", "Choose crop", false);
                add("Release", "Apply crop", false);
                if self.photo.crop_drag.is_some() {
                    add(
                        "Enter",
                        "Apply crop",
                        plain_keys && ctx.input(|i| i.key_down(Key::Enter)),
                    );
                    add(
                        "Esc",
                        "Cancel crop",
                        plain_keys && ctx.input(|i| i.key_down(Key::Escape)),
                    );
                }
            }
            add(
                "Scroll",
                if self.tool == Tool::Zoom {
                    "Zoom photo"
                } else {
                    "Pan photo"
                },
                false,
            );
            if !self.photo.images.is_empty() && !self.photo.is_batching() {
                add(
                    "Ctrl+click",
                    "Toggle photo selection",
                    command && !mods.shift,
                );
                add("Shift+click", "Select photo range", mods.shift && !command);
            }
            add("Pinch", "Zoom photo", false);
            add("Ctrl+scroll", "Zoom photo", command);
            add("Alt+scroll", "Zoom photo", mods.alt);
        } else {
            match &self.op {
                Some(Op::Resize { handle, .. }) => {
                    context = "Resize selection";
                    if *handle < 4 {
                        add("Shift", "Keep proportions", mods.shift);
                    }
                    add("Alt", "Scale from centre", mods.alt);
                    snapping = true;
                }
                Some(Op::Rotate { .. }) => {
                    context = "Rotate selection";
                    add("Shift", "15° steps", mods.shift);
                }
                Some(Op::ArtboardResize { .. }) => {
                    context = "Resize artboard";
                    add("Drag", "Resize with artwork", false);
                    snapping = true;
                }
                Some(Op::ArtboardRotate { .. }) => {
                    context = "Rotate artboard";
                    add("Drag", "Rotate with artwork", false);
                }
                Some(Op::Move { .. } | Op::ArtboardMove { .. }) => {
                    context = if self.tool == Tool::Artboard {
                        "Move artboards"
                    } else {
                        "Move selection"
                    };
                    add("Shift", "45° movement", mods.shift);
                    snapping = true;
                }
                Some(Op::Node { which, .. }) => {
                    context = "Edit path";
                    if !matches!(which, NodeHit::Segment(_)) {
                        add("Shift", "45° angles", mods.shift);
                    }
                    if matches!(which, NodeHit::HandleIn(_) | NodeHit::HandleOut(_)) {
                        add("Alt", "Break handle pair", mods.alt);
                    } else {
                        snapping = true;
                    }
                }
                _ => match self.tool {
                    Tool::Pen => {
                        add("Shift", "45° angles", mods.shift);
                        add("Alt", "Break handle pair", mods.alt);
                        if let Some(Op::Pen { anchors, .. }) = &self.op {
                            add(
                                "Enter",
                                "Finish open path",
                                plain_keys && ctx.input(|i| i.key_down(Key::Enter)),
                            );
                            add(
                                "Esc",
                                if anchors.len() > 1 {
                                    "Remove last point"
                                } else {
                                    "Cancel path"
                                },
                                plain_keys && ctx.input(|i| i.key_down(Key::Escape)),
                            );
                            add("First point", "Close path", false);
                        } else {
                            add("Click", "Add corner", false);
                            add("Drag", "Add smooth point", false);
                        }
                        snapping = true;
                    }
                    Tool::Select | Tool::Artboard => {
                        if mods.alt && mods.shift {
                            add("Alt+Shift+drag", "Clone at 45°", true);
                        }
                        add("Shift+drag", "45° movement", mods.shift);
                        add("Alt+drag", "Clone and move", mods.alt);
                        add("Shift+click", "Add to selection", mods.shift);
                        if !self.selection.is_empty() || !self.artboard_sel.is_empty() {
                            let label = if self.is_motion() && self.selected_key.is_some() {
                                "Remove key"
                            } else if self.is_motion() && self.selection_has_motion() {
                                "Remove animation"
                            } else {
                                "Delete selection"
                            };
                            add(
                                "Delete",
                                label,
                                plain_keys && ctx.input(|i| i.key_down(Key::Delete)),
                            );
                        }
                        snapping = true;
                    }
                    Tool::Node => {
                        add("Shift+click", "Toggle point selection", mods.shift);
                        add("Alt+click", "Corner / smooth", mods.alt);
                        add("Alt+drag", "Break handle pair", mods.alt);
                        add("Shift+drag", "Constrain point / handle", mods.shift);
                        if !self.node_sel.is_empty() {
                            add(
                                "Delete",
                                "Delete points",
                                plain_keys && ctx.input(|i| i.key_down(Key::Delete)),
                            );
                        }
                        snapping = true;
                    }
                    Tool::Brush
                    | Tool::Eraser
                    | Tool::Clone
                    | Tool::Heal
                    | Tool::Smudge
                    | Tool::Pencil => {
                        if matches!(self.tool, Tool::Clone | Tool::Heal) && self.op.is_none() {
                            add("Alt+click", "Set source", mods.alt);
                        }
                        add("Shift", "45° strokes", mods.shift);
                        if self.tool != Tool::Pencil {
                            add(
                                "[ / ]",
                                "Brush size",
                                plain_keys
                                    && !mods.shift
                                    && ctx.input(|i| {
                                        i.key_down(Key::OpenBracket)
                                            || i.key_down(Key::CloseBracket)
                                    }),
                            );
                            add("Shift+[ / ]", "Brush hardness", plain_keys && mods.shift);
                        }
                    }
                    Tool::Rect | Tool::Ellipse | Tool::Polygon | Tool::Star => {
                        add("Shift", "Equal width and height", mods.shift);
                        add("Drag", "Draw shape", false);
                        snapping = true;
                    }
                    Tool::Line => {
                        add("Shift", "45° angles", mods.shift);
                        add("Drag", "Draw line", false);
                        snapping = true;
                    }
                    Tool::Zoom => {
                        add(
                            "Click",
                            "Zoom in",
                            !mods.alt && !command && ctx.input(|i| i.pointer.primary_down()),
                        );
                        add("Alt+click", "Zoom out", mods.alt && !command);
                        add("Ctrl+click", "Fit artboard", command && !mods.shift);
                        add(
                            "Ctrl+Shift+click",
                            "Fit selection / all",
                            command && mods.shift,
                        );
                        add("Drag", "Zoom to area", false);
                    }
                    Tool::Text => {
                        add("Click", "Place or edit text", false);
                    }
                    Tool::Gradient => {
                        add("Drag", "Set linear fill", false);
                        snapping = true;
                    }
                    Tool::Eyedropper => {
                        add("Click", "Sample fill color", false);
                    }
                    Tool::Fill => {
                        add("Click", "Flood fill", false);
                    }
                    Tool::Trace => {
                        add("Click", "Trace active pixels", false);
                    }
                    Tool::Marquee | Tool::EllipseMarquee | Tool::Lasso => {
                        add("Drag", "Select pixels", false);
                    }
                    Tool::Wand => {
                        add("Click", "Select similar color", false);
                    }
                    Tool::Hand => {
                        add("Drag", "Pan canvas", false);
                    }
                    Tool::Crop => {
                        add("Drag", "Choose crop", false);
                        add("Release", "Apply crop", false);
                    }
                },
            }
        }
        if snapping {
            add(
                "Ctrl",
                if self.snap.enabled {
                    "Temporarily unsnap"
                } else {
                    "Temporarily snap"
                },
                command,
            );
        }
        if self.op.is_none()
            && self.persona != Persona::Photo
            && self
                .selection
                .iter()
                .any(|(layer, id)| self.doc.find_shape(*layer, *id).is_some())
        {
            add(
                if mods.shift { "Shift+arrows" } else { "Arrows" },
                if mods.shift {
                    "Nudge artwork 10 px"
                } else {
                    "Nudge artwork 1 px"
                },
                plain_keys && mods.shift,
            );
        }
        if self.is_motion() {
            add(
                "Space",
                if self.playing { "Pause" } else { "Play" },
                plain_keys && ctx.input(|i| i.key_down(Key::Space)),
            );
            if !self.selection.is_empty() {
                add(
                    "K",
                    "Key selection",
                    plain_keys && ctx.input(|i| i.key_down(Key::K)),
                );
            }
            add("Home / End", "Timeline start / end", false);
            add(
                "Middle-drag",
                "Pan canvas",
                ctx.input(|i| i.pointer.middle_down()),
            );
        } else {
            add(
                "Space+drag",
                "Pan canvas",
                ctx.input(|i| i.key_down(Key::Space)),
            );
        }
        if self.is_motion() {
            context = "Motion";
            hints.sort_by_key(|hint| {
                !matches!(
                    hint.keys,
                    "Space" | "K" | "Delete" | "Home / End" | "Middle-drag"
                )
            });
        }
        (context, hints)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hints(studio: &Studio, modifiers: Modifiers) -> KeyHints {
        let ctx = egui::Context::default();
        let mut hints = KeyHints::default();
        let mut output = ctx.run_ui(
            egui::RawInput {
                events: vec![egui::Event::ModifiersChanged(modifiers)],
                ..Default::default()
            },
            |ui| {
                hints = studio.key_hints(ui.ctx());
            },
        );
        output.textures_delta.clear();
        hints
    }

    #[test]
    fn motion_delete_hint_names_the_animation_not_the_object() {
        let mut studio = Studio::new();
        studio.show_welcome = false;
        studio.persona = Persona::Motion;
        let shape = Shape::new(
            Geom::Rect {
                origin: Pt::new(8.0, 8.0),
                size: Pt::new(20.0, 20.0),
                radius: 0.0,
            },
            Style::default(),
        );
        let id = shape.id;
        studio.commit(Cmd::AddShape { layer: 1, shape });
        studio.selection = vec![(1, id)];
        studio
            .doc
            .motion
            .set_key(id, Prop::Opacity, 0.0, 0.0, Ease::Linear);
        let animated = hints(&studio, Modifiers::NONE);
        assert!(
            animated
                .gestures
                .iter()
                .any(|hint| hint.keys == "Delete" && hint.label == "Remove animation")
        );
        studio.selected_key = Some((id, Prop::Opacity, 0));
        let keyed = hints(&studio, Modifiers::NONE);
        assert!(
            keyed
                .gestures
                .iter()
                .any(|hint| hint.keys == "Delete" && hint.label == "Remove key")
        );
        studio.selected_key = None;
        studio.doc.motion.drop_shape(id);
        let idle = hints(&studio, Modifiers::NONE);
        assert!(
            idle.gestures
                .iter()
                .any(|hint| hint.keys == "Delete" && hint.label == "Delete selection")
        );
    }

    #[test]
    fn hints_follow_the_live_transform_and_photo_route_instead_of_the_selected_tool() {
        let mut studio = Studio::new();
        studio.show_welcome = false;
        studio.tool = Tool::Rect;
        studio.op = Some(Op::Rotate {
            orig: vec![],
            center: Pt::ZERO,
            start_angle: 0.0,
        });
        let rotation = hints(&studio, Modifiers::SHIFT);
        assert_eq!(rotation.context, "Rotate selection");
        assert!(
            rotation
                .gestures
                .iter()
                .any(|hint| hint.label == "15° steps" && hint.active)
        );
        assert!(
            !rotation
                .gestures
                .iter()
                .any(|hint| hint.label == "Equal width and height")
        );
        studio.op = Some(Op::Resize {
            orig: vec![],
            handle: 0,
            start_box: Bounds::from_pt(Pt::ZERO),
        });
        let resize = hints(&studio, Modifiers::SHIFT | Modifiers::ALT);
        assert!(
            resize
                .gestures
                .iter()
                .any(|hint| hint.label == "Keep proportions" && hint.active)
        );
        assert!(
            resize
                .gestures
                .iter()
                .any(|hint| hint.label == "Scale from centre" && hint.active)
        );

        studio.op = None;
        studio.persona = Persona::Photo;
        studio.tool = Tool::Crop;
        let idle_crop = hints(&studio, Modifiers::CTRL);
        assert!(
            idle_crop
                .gestures
                .iter()
                .any(|hint| hint.keys == "Release" && hint.label == "Apply crop")
        );
        assert!(
            !idle_crop
                .gestures
                .iter()
                .any(|hint| matches!(hint.keys, "Enter" | "Esc" | "Ctrl+click" | "Ctrl"))
        );
        studio.photo.crop_drag = Some((Pt::ZERO, Pt::splat(100.0)));
        let crop = hints(&studio, Modifiers::NONE);
        assert!(
            crop.gestures
                .iter()
                .any(|hint| hint.keys == "Enter" && hint.label == "Apply crop")
        );
        assert!(
            crop.gestures
                .iter()
                .any(|hint| hint.keys == "Esc" && hint.label == "Cancel crop")
        );
    }

    #[test]
    fn modifier_hints_match_real_chords_and_reading_them_preserves_toggle_input() {
        let mut studio = Studio::new();
        studio.show_welcome = false;
        let style = hints(&studio, Modifiers::CTRL | Modifiers::ALT);
        assert_eq!(
            style.keys.iter().map(|hint| hint.label).collect::<Vec<_>>(),
            vec!["Copy style", "Paste style"]
        );
        assert!(
            hints(&studio, Modifiers::CTRL | Modifiers::ALT | Modifiers::SHIFT)
                .keys
                .is_empty()
        );
        let shifted = hints(&studio, Modifiers::CTRL | Modifiers::SHIFT);
        assert!(shifted.keys.iter().any(|hint| hint.label == "Save as"));
        assert!(!shifted.keys.iter().any(|hint| hint.label == "Save"));
        assert!(
            !shifted
                .keys
                .iter()
                .any(|hint| hint.label == "Copy adjustments")
        );
        studio.persona = Persona::Photo;
        let photo_shifted = hints(&studio, Modifiers::CTRL | Modifiers::SHIFT);
        assert!(
            photo_shifted
                .keys
                .iter()
                .any(|hint| hint.keys == "Ctrl+Shift+C" && hint.label == "Copy adjustments")
        );
        assert!(
            photo_shifted
                .keys
                .iter()
                .any(|hint| hint.keys == "Ctrl+Shift+V" && hint.label == "Paste adjustments")
        );
        let photo_commands = hints(&studio, Modifiers::CTRL);
        assert!(
            photo_commands
                .keys
                .iter()
                .any(|hint| hint.keys == "Ctrl+S" && hint.label == "Save selected settings")
        );
        assert!(
            photo_commands
                .keys
                .iter()
                .any(|hint| hint.keys == "Ctrl+A" && hint.label == "Select all photos")
        );

        let ctx = egui::Context::default();
        let initial = studio.show_key_hud;
        let mut output = ctx.run_ui(
            egui::RawInput {
                events: vec![egui::Event::Key {
                    key: Key::Slash,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: Modifiers::CTRL,
                }],
                ..Default::default()
            },
            |ui| {
                let before = ui.input(|input| input.events.clone());
                let _ = studio.key_hints(ui.ctx());
                assert_eq!(
                    ui.input(|input| input.events.clone()),
                    before,
                    "the HUD is read-only"
                );
                studio.handle_shortcuts(ui.ctx());
            },
        );
        output.textures_delta.clear();
        assert_eq!(
            studio.show_key_hud, !initial,
            "a fast Ctrl release still toggles the HUD"
        );
    }
}
