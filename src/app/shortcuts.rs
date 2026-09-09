use super::*;
use eframe::egui::{Event, Key, Modifiers};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Shortcut {
    Save,
    SaveAs,
    Open,
    Place,
    New,
    Export,
    Undo,
    Redo,
    Copy,
    Cut,
    Paste,
    CopyStyle,
    PasteStyle,
    CopyAdjustments,
    PasteAdjustments,
    Duplicate,
    SelectAll,
    Combine,
    Release,
    Forward,
    Front,
    Backward,
    Back,
    Fit,
    ActualSize,
    ZoomIn,
    ZoomOut,
    Help,
    ToggleGuides,
    ToggleSnapping,
    FreeTransform,
    ToggleKeyHud,
}

impl Shortcut {
    pub(super) fn available(self, persona: Persona) -> bool {
        if persona == Persona::Photo {
            self.global()
                || matches!(
                    self,
                    Self::Undo
                        | Self::Redo
                        | Self::CopyAdjustments
                        | Self::PasteAdjustments
                        | Self::SelectAll
                        | Self::Fit
                        | Self::ActualSize
                        | Self::ZoomIn
                        | Self::ZoomOut
                )
        } else {
            !matches!(self, Self::CopyAdjustments | Self::PasteAdjustments)
        }
    }

    pub(super) fn global(self) -> bool {
        matches!(
            self,
            Self::Save
                | Self::SaveAs
                | Self::Open
                | Self::Place
                | Self::New
                | Self::Export
                | Self::Help
                | Self::ToggleKeyHud
        )
    }
}

pub(super) fn key_shortcut(key: Key, mods: Modifiers) -> Option<Shortcut> {
    use Shortcut::*;
    if key == Key::F1 && mods.is_none() {
        return Some(Help);
    }
    if !(mods.command || mods.ctrl) {
        return None;
    }
    if mods.alt {
        return match (key, mods.shift) {
            (Key::C, false) => Some(CopyStyle),
            (Key::V, false) => Some(PasteStyle),
            _ => None,
        };
    }
    Some(match (key, mods.shift) {
        (Key::Semicolon, false) => ToggleGuides,
        (Key::Slash, false) => ToggleKeyHud,
        (Key::Semicolon | Key::Colon, true) => ToggleSnapping,
        (Key::S, false) => Save,
        (Key::S, true) => SaveAs,
        (Key::O, false) => Open,
        (Key::P, true) => Place,
        (Key::N, false) => New,
        (Key::E, false) => Export,
        (Key::Z, false) => Undo,
        (Key::Z, true) | (Key::Y, false) => Redo,
        (Key::C, false) => Copy,
        (Key::C, true) => CopyAdjustments,
        (Key::X, false) => Cut,
        (Key::V, false) => Paste,
        (Key::V, true) => PasteAdjustments,
        (Key::D, false) => Duplicate,
        (Key::T, false) => FreeTransform,
        (Key::A, false) => SelectAll,
        (Key::G, false) => Combine,
        (Key::G, true) => Release,
        (Key::CloseBracket, false) => Forward,
        (Key::CloseBracket | Key::CloseCurlyBracket, true) => Front,
        (Key::OpenBracket, false) => Backward,
        (Key::OpenBracket | Key::OpenCurlyBracket, true) => Back,
        (Key::Num0, false) => Fit,
        (Key::Num1, false) => ActualSize,
        (Key::Plus | Key::Equals, _) => ZoomIn,
        (Key::Minus, false) => ZoomOut,
        _ => return None,
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ShortcutFocus {
    Inactive,
    Modal,
    Popup,
    Field,
    Text,
    Canvas,
}

pub(super) fn held_modifiers(ctx: &egui::Context) -> Modifiers {
    ctx.input(|input| {
        if !input.focused
            || input
                .events
                .iter()
                .any(|event| matches!(event, Event::WindowFocused(false)))
        {
            Modifiers::NONE
        } else {
            input.modifiers
        }
    })
}

impl Studio {
    pub(super) fn shortcut_focus(&self, ctx: &egui::Context) -> ShortcutFocus {
        if !ctx.input(|input| input.focused) {
            ShortcutFocus::Inactive
        } else if self.pending_nav.is_some()
            || ctx.memory(|memory| memory.top_modal_layer().is_some())
        {
            ShortcutFocus::Modal
        } else if egui::Popup::is_any_open(ctx) {
            ShortcutFocus::Popup
        } else if ctx.memory(|memory| {
            memory.focused().is_some_and(|id| {
                id != egui::Id::new("studio-canvas") && id != egui::Id::new("studio-photo-canvas")
            })
        }) {
            ShortcutFocus::Field
        } else if self.type_edit.is_some() && self.persona != Persona::Photo {
            ShortcutFocus::Text
        } else {
            ShortcutFocus::Canvas
        }
    }

    pub fn handle_shortcuts(&mut self, ctx: &egui::Context) {
        let events = ctx.input(|i| i.events.clone());
        let final_modifiers = held_modifiers(ctx);
        // Clipboard events have no modifier field. Replay modifier changes in order,
        // retaining the previous frame's state when a held chord spans frames.
        let mut modifiers = ctx.data_mut(|data| {
            let id = egui::Id::new("studio-shortcut-modifiers");
            let previous = data.get_temp::<Modifiers>(id).unwrap_or(final_modifiers);
            data.insert_temp(id, final_modifiers);
            previous
        });
        let focus = self.shortcut_focus(ctx);
        if matches!(
            focus,
            ShortcutFocus::Inactive | ShortcutFocus::Modal | ShortcutFocus::Popup
        ) {
            return;
        }
        let field_focused = focus == ShortcutFocus::Field;
        let mut consumed = Vec::new();
        for (index, event) in events.iter().enumerate() {
            let shortcut = match event {
                Event::ModifiersChanged(m) => {
                    modifiers = *m;
                    continue;
                }
                Event::WindowFocused(false) => {
                    modifiers = Modifiers::NONE;
                    continue;
                }
                Event::Key {
                    key,
                    modifiers: pressed_modifiers,
                    pressed: true,
                    ..
                } => {
                    // The retained native press is also the reliable modifier
                    // snapshot for its following Copy/Cut/Paste event.
                    modifiers = *pressed_modifiers;
                    key_shortcut(*key, *pressed_modifiers)
                }
                Event::Copy => Some(if modifiers.alt {
                    Shortcut::CopyStyle
                } else if modifiers.shift && self.persona == Persona::Photo {
                    Shortcut::CopyAdjustments
                } else {
                    Shortcut::Copy
                }),
                Event::Cut => Some(Shortcut::Cut),
                Event::Paste(_) => Some(if modifiers.alt {
                    Shortcut::PasteStyle
                } else if modifiers.shift && self.persona == Persona::Photo {
                    Shortcut::PasteAdjustments
                } else {
                    Shortcut::Paste
                }),
                _ => None,
            };
            if let Some(shortcut) = shortcut {
                if (field_focused && !shortcut.global()) || !shortcut.available(self.persona) {
                    continue;
                }
                // egui-winit retains the key press immediately before its native
                // clipboard event. Dispatch that pair once, using the latter's
                // payload; an empty clipboard leaves the key press usable alone.
                if matches!(event, Event::Key { .. })
                    && matches!(
                        (shortcut, events.get(index + 1)),
                        (
                            Shortcut::Copy | Shortcut::CopyStyle | Shortcut::CopyAdjustments,
                            Some(Event::Copy)
                        ) | (Shortcut::Cut, Some(Event::Cut))
                            | (
                                Shortcut::Paste | Shortcut::PasteStyle | Shortcut::PasteAdjustments,
                                Some(Event::Paste(_))
                            )
                    )
                {
                    consumed.push(index);
                    continue;
                }
                let payload = match event {
                    Event::Paste(text) => Some(text.as_str()),
                    _ => None,
                };
                if focus == ShortcutFocus::Text && !shortcut.global() {
                    self.type_shortcut(ctx, shortcut, payload);
                } else {
                    self.run_shortcut(ctx, shortcut, payload);
                }
                consumed.push(index);
            } else if focus == ShortcutFocus::Text {
                if self.type_event(event) {
                    consumed.push(index);
                }
            } else if !field_focused
                && let Event::Key {
                    key,
                    modifiers,
                    pressed: true,
                    repeat,
                    ..
                } = event
                && !(modifiers.ctrl || modifiers.command || modifiers.alt || modifiers.mac_cmd)
                && self.canvas_key(*key, modifiers.shift, *repeat)
            {
                consumed.push(index);
            }
        }
        // The widgets rendered later in this frame must not also act on shortcuts
        // handled by the canvas. Do not hold an egui input lock while dispatching.
        ctx.input_mut(|input| {
            let mut index = 0;
            input.events.retain(|_| {
                let keep = !consumed.contains(&index);
                index += 1;
                keep
            });
        });
    }

    fn run_shortcut(&mut self, ctx: &egui::Context, shortcut: Shortcut, payload: Option<&str>) {
        match shortcut {
            Shortcut::ToggleGuides => self.toggle_guides(),
            Shortcut::ToggleKeyHud => self.show_key_hud = !self.show_key_hud,
            Shortcut::FreeTransform => self.free_transform(),
            Shortcut::ToggleSnapping => self.toggle_snapping(),
            Shortcut::Save => self.save(),
            Shortcut::SaveAs => self.save_as(),
            Shortcut::Open => {
                self.commit_type_edit();
                self.open();
            }
            Shortcut::Place => {
                self.commit_type_edit();
                self.begin_place();
            }
            Shortcut::New => self.new_tab(),
            Shortcut::Export => {
                self.commit_type_edit();
                if self.persona == Persona::Photo {
                    crate::ui::photo::export_developed(ctx, self, "png");
                } else {
                    self.export_png();
                }
            }
            Shortcut::Undo => self.undo(),
            Shortcut::Redo => self.redo(),
            Shortcut::Copy => self.copy_selection(ctx),
            Shortcut::Cut => self.cut_selection(ctx),
            Shortcut::Paste => self.paste_clipboard(payload),
            Shortcut::CopyStyle => {
                self.copy_style();
                // Publishing the style makes it portable between windows.
                if let Some(style) = &self.style_clip
                    && let Ok(json) = serde_json::to_string(style)
                {
                    ctx.copy_text(format!("omadesign-style:{json}"));
                }
            }
            Shortcut::PasteStyle => {
                if let Some(json) = payload.and_then(|p| p.strip_prefix("omadesign-style:"))
                    && let Ok(style) = serde_json::from_str(json)
                {
                    self.style_clip = Some(style);
                }
                self.paste_style();
            }
            Shortcut::CopyAdjustments => crate::ui::photo::copy_adjustments(self),
            Shortcut::PasteAdjustments => crate::ui::photo::paste_adjustments(ctx, self),
            Shortcut::Duplicate => self.duplicate_selection(),
            Shortcut::SelectAll => {
                if self.persona == Persona::Photo {
                    if !self.photo.is_batching() {
                        self.photo.select_all_images();
                    }
                } else {
                    self.select_all();
                }
            }
            Shortcut::Combine => self.combine_selected(),
            Shortcut::Release => self.release_compound(),
            Shortcut::Forward => self.bring_forward(),
            Shortcut::Front => self.bring_to_front(),
            Shortcut::Backward => self.send_backward(),
            Shortcut::Back => self.send_to_back(),
            Shortcut::Fit => {
                if self.persona == Persona::Photo {
                    self.photo.view_scale = 1.0;
                    self.photo.view_offset = egui::Vec2::ZERO;
                } else {
                    self.need_fit = true;
                }
            }
            Shortcut::ActualSize => {
                if self.persona == Persona::Photo {
                    self.photo.view_scale = 1.0 / self.photo.fit_scale.max(f32::EPSILON);
                    self.photo.view_offset = egui::Vec2::ZERO;
                } else {
                    self.view.scale = 1.0;
                    self.mark();
                }
            }
            Shortcut::ZoomIn | Shortcut::ZoomOut => {
                let factor = if shortcut == Shortcut::ZoomIn {
                    1.25
                } else {
                    1.0 / 1.25
                };
                if self.persona == Persona::Photo {
                    self.photo.view_scale = (self.photo.view_scale * factor).clamp(0.1, 8.0);
                } else {
                    self.zoom_by(factor, self.canvas_zoom_anchor());
                }
            }
            Shortcut::Help => self.show_shortcuts = !self.show_shortcuts,
        }
    }

    fn type_shortcut(&mut self, ctx: &egui::Context, shortcut: Shortcut, payload: Option<&str>) {
        match shortcut {
            Shortcut::FreeTransform => self.free_transform(),
            Shortcut::Copy | Shortcut::Cut => {
                let (lo, hi) = self.type_sel_range();
                if lo == hi {
                    return;
                }
                if let Some(run) = self.live_type_mut() {
                    let a = crate::text::char_to_byte(&run.content, lo);
                    let b = crate::text::char_to_byte(&run.content, hi);
                    ctx.copy_text(run.content[a..b].to_owned());
                    if shortcut == Shortcut::Cut {
                        self.type_delete_range(lo, hi);
                    }
                }
            }
            Shortcut::Paste => {
                if let Some(text) = payload {
                    self.type_insert(text);
                }
            }
            Shortcut::SelectAll => {
                let count = self
                    .live_type_mut()
                    .map_or(0, |run| run.content.chars().count());
                if let Some(edit) = &mut self.type_edit {
                    edit.anchor = 0;
                    edit.caret = count;
                }
            }
            Shortcut::Undo | Shortcut::Redo => {
                self.commit_type_edit();
                self.run_shortcut(ctx, shortcut, None);
            }
            _ => {}
        }
    }

    fn type_event(&mut self, event: &Event) -> bool {
        match event {
            Event::Text(text) => self.type_insert(text),
            Event::Key {
                key,
                modifiers,
                pressed: true,
                ..
            } if !(modifiers.command || modifiers.ctrl || modifiers.alt || modifiers.mac_cmd) => {
                let caret = self.type_edit.as_ref().map_or(0, |edit| edit.caret);
                match key {
                    Key::Escape => {
                        self.commit_type_edit();
                        self.tool = Tool::Select;
                    }
                    Key::Enter => self.type_insert("\n"),
                    Key::Backspace => self.type_backspace(),
                    Key::Delete => self.type_delete_fwd(),
                    Key::ArrowLeft => {
                        self.type_move_caret(caret.saturating_sub(1), modifiers.shift)
                    }
                    Key::ArrowRight => self.type_move_caret(caret + 1, modifiers.shift),
                    Key::Home | Key::ArrowUp | Key::End | Key::ArrowDown => {
                        if let Some(run) = self.live_type_mut() {
                            let target = if matches!(key, Key::Home | Key::ArrowUp) {
                                run.content
                                    .chars()
                                    .take(caret)
                                    .enumerate()
                                    .filter_map(|(i, ch)| (ch == '\n').then_some(i + 1))
                                    .last()
                                    .unwrap_or(0)
                            } else {
                                run.content
                                    .chars()
                                    .enumerate()
                                    .skip(caret)
                                    .find_map(|(i, ch)| (ch == '\n').then_some(i))
                                    .unwrap_or_else(|| run.content.chars().count())
                            };
                            self.type_move_caret(target, modifiers.shift);
                        }
                    }
                    _ => return false,
                }
            }
            _ => return false,
        }
        true
    }

    fn canvas_key(&mut self, key: Key, shift: bool, repeat: bool) -> bool {
        if self.persona == Persona::Photo {
            if self.photo.is_batching() {
                return false;
            }
            if matches!(key, Key::Enter | Key::Escape) {
                if let Some((start, cur)) = self.photo.crop_drag.take() {
                    if key == Key::Enter {
                        self.commit_photo_crop(start, cur);
                    } else {
                        self.photo.status = "Crop cancelled".into();
                    }
                }
                return true;
            }
            let tool = match (key, shift) {
                (Key::H, false) => Tool::Hand,
                (Key::Z, false) => Tool::Zoom,
                (Key::C, false) => Tool::Crop,
                (Key::I, false) => Tool::Eyedropper,
                _ => return false,
            };
            self.set_tool(tool);
            return true;
        }
        if self.persona == Persona::Motion {
            match key {
                Key::Space => {
                    self.playing = !self.playing;
                    self.status = if self.playing { "play" } else { "pause" }.into();
                    return true;
                }
                Key::K => {
                    self.key_selection(Ease::EaseInOut);
                    return true;
                }
                Key::Home => {
                    self.playhead = 0.0;
                    self.playing = false;
                    return true;
                }
                Key::End => {
                    self.playhead = self.doc.motion.duration;
                    self.playing = false;
                    return true;
                }
                _ => {}
            }
        }
        if self.deformation.is_some() && matches!(key, Key::Escape | Key::Enter) {
            self.end_deform(key == Key::Escape);
            return true;
        }
        let step = if shift { 10.0 } else { 1.0 };
        match key {
            Key::Delete | Key::Backspace => {
                if repeat {
                    if self.tool == Tool::Node && !self.node_sel.is_empty() {
                        self.delete_node();
                    } else {
                        self.remove_selected_motion();
                    }
                } else {
                    self.delete_selection();
                }
            }
            Key::Escape => {
                self.end_pixel_stroke(true);
                if self.pending_place.is_some() {
                    self.cancel_place();
                    self.op = None;
                    return true;
                }
                if let Some(Op::Pen {
                    anchors, source, ..
                }) = &mut self.op
                {
                    if anchors.len() > 1 {
                        anchors.pop();
                        self.sync_pen_source();
                        self.status = "point removed".into();
                        return true;
                    }
                    if let Some((li, id, orig)) = source.clone()
                        && let Some(shape) = self.doc.find_shape_mut(li, id)
                    {
                        shape.geom = orig;
                    }
                    self.mark();
                }
                self.op = None;
                self.bool_pick = None;
            }
            Key::Enter => {
                if self.pending_place.is_some() {
                    self.commit_place_at(Pt::new(self.doc.width * 0.5, self.doc.height * 0.5));
                    return true;
                }
                match self.op.take() {
                    Some(Op::Pen {
                        anchors, source, ..
                    }) => self.finish_pen(anchors, false, source),
                    Some(Op::CropPhoto { start, cur }) => self.commit_photo_crop(start, cur),
                    other => self.op = other,
                }
            }
            Key::ArrowLeft => self.nudge(-step, 0.0),
            Key::ArrowRight => self.nudge(step, 0.0),
            Key::ArrowUp => self.nudge(0.0, -step),
            Key::ArrowDown => self.nudge(0.0, step),
            Key::X => self.swap_fill_stroke(),
            Key::D => self.style = Style::default(),
            Key::OpenBracket | Key::OpenCurlyBracket => {
                if shift {
                    self.brush.hardness = (self.brush.hardness - 0.08).max(0.0);
                } else {
                    self.brush.size = (self.brush.size - 2.0).max(1.0);
                }
            }
            Key::CloseBracket | Key::CloseCurlyBracket => {
                if shift {
                    self.brush.hardness = (self.brush.hardness + 0.08).min(1.0);
                } else {
                    self.brush.size = (self.brush.size + 2.0).min(256.0);
                }
            }
            _ => {
                let tool = match (key, shift, self.persona) {
                    (Key::M, true, Persona::Pixel) => Tool::Marquee,
                    (Key::J, true, Persona::Pixel) => Tool::Heal,
                    (Key::O, true, Persona::Pixel) => Tool::EllipseMarquee,
                    (Key::O, true, Persona::Design) => Tool::Artboard,
                    (Key::V, false, _) => Tool::Select,
                    (Key::A, false, _) => Tool::Node,
                    (Key::P, false, _) => Tool::Pen,
                    (Key::N, false, _) => Tool::Pencil,
                    (Key::R, false, _) => Tool::Rect,
                    (Key::O, false, _) => Tool::Ellipse,
                    (Key::Y, false, _) => Tool::Polygon,
                    (Key::S, false, Persona::Design) => Tool::Star,
                    (Key::L, false, _) => Tool::Line,
                    (Key::T, false, _) => Tool::Text,
                    (Key::G, false, _) => Tool::Gradient,
                    (Key::I, false, _) => Tool::Eyedropper,
                    (Key::U, false, _) => Tool::Trace,
                    (Key::B, false, _) => Tool::Brush,
                    (Key::E, false, _) => Tool::Eraser,
                    (Key::K, false, _) => Tool::Fill,
                    (Key::J, false, _) => Tool::Clone,
                    (Key::M, false, Persona::Pixel) => Tool::Smudge,
                    (Key::C, false, _) => Tool::Crop,
                    (Key::W, false, _) => Tool::Wand,
                    (Key::Q, false, _) => Tool::Lasso,
                    (Key::H, false, _) => Tool::Hand,
                    (Key::Z, false, _) => Tool::Zoom,
                    _ => return false,
                };
                if !tool.in_persona(self.persona) {
                    return false;
                }
                self.set_tool(tool);
            }
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn context() -> egui::Context {
        let ctx = egui::Context::default();
        ctx.options_mut(|options| options.zoom_with_keyboard = false);
        ctx
    }

    fn key(key: Key, modifiers: Modifiers) -> Event {
        Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers,
        }
    }

    fn frame(
        ctx: &egui::Context,
        studio: &mut Studio,
        events: Vec<Event>,
    ) -> Vec<egui::OutputCommand> {
        let mut output = ctx.run_ui(
            egui::RawInput {
                events,
                ..Default::default()
            },
            |ui| {
                studio.handle_shortcuts(ui.ctx());
            },
        );
        output.textures_delta.clear();
        output.platform_output.commands
    }

    fn add_rectangle(studio: &mut Studio, x: f32) -> u64 {
        let shape = Shape::new(
            Geom::Rect {
                origin: Pt::new(x, 20.0),
                size: Pt::new(20.0, 30.0),
                radius: 0.0,
            },
            Style::default(),
        );
        let id = shape.id;
        studio.commit(Cmd::AddShape { layer: 1, shape });
        studio.selection = vec![(1, id)];
        studio.show_welcome = false;
        id
    }

    fn copied(commands: Vec<egui::OutputCommand>) -> String {
        commands
            .into_iter()
            .find_map(|command| match command {
                egui::OutputCommand::CopyText(text) => Some(text),
                _ => None,
            })
            .expect("native clipboard output")
    }

    fn count(studio: &Studio) -> usize {
        studio.doc.layers[1].kind.shapes().unwrap().len()
    }

    #[test]
    fn every_command_chord_routes_with_its_own_modifiers() {
        use Shortcut::*;
        let ctrl = Modifiers::CTRL;
        let shift = Modifiers::CTRL | Modifiers::SHIFT;
        let alt = Modifiers::CTRL | Modifiers::ALT;
        let cases = [
            (Key::S, ctrl, Save),
            (Key::S, shift, SaveAs),
            (Key::O, ctrl, Open),
            (Key::P, shift, Place),
            (Key::N, ctrl, New),
            (Key::E, ctrl, Export),
            (Key::Z, ctrl, Undo),
            (Key::Z, shift, Redo),
            (Key::Y, ctrl, Redo),
            (Key::C, ctrl, Copy),
            (Key::X, ctrl, Cut),
            (Key::V, ctrl, Paste),
            (Key::C, alt, CopyStyle),
            (Key::C, shift, CopyAdjustments),
            (Key::V, alt, PasteStyle),
            (Key::V, shift, PasteAdjustments),
            (Key::D, ctrl, Duplicate),
            (Key::T, ctrl, FreeTransform),
            (Key::A, ctrl, SelectAll),
            (Key::G, ctrl, Combine),
            (Key::G, shift, Release),
            (Key::CloseBracket, ctrl, Forward),
            (Key::CloseBracket, shift, Front),
            (Key::CloseCurlyBracket, shift, Front),
            (Key::OpenBracket, ctrl, Backward),
            (Key::OpenBracket, shift, Back),
            (Key::OpenCurlyBracket, shift, Back),
            (Key::Num0, ctrl, Fit),
            (Key::Num1, ctrl, ActualSize),
            (Key::Plus, shift, ZoomIn),
            (Key::Equals, ctrl, ZoomIn),
            (Key::Equals, shift, ZoomIn),
            (Key::Minus, ctrl, ZoomOut),
            (Key::F1, Modifiers::NONE, Help),
            (Key::Slash, ctrl, ToggleKeyHud),
            (Key::Semicolon, ctrl, ToggleGuides),
            (Key::Semicolon, shift, ToggleSnapping),
            (Key::Colon, shift, ToggleSnapping),
        ];
        for (key, modifiers, expected) in cases {
            assert_eq!(
                key_shortcut(key, modifiers),
                Some(expected),
                "{modifiers:?} {key:?}"
            );
            if modifiers.ctrl {
                let command = Modifiers {
                    ctrl: false,
                    command: true,
                    mac_cmd: true,
                    ..modifiers
                };
                assert_eq!(
                    key_shortcut(key, command),
                    Some(expected),
                    "command {key:?}"
                );
            }
        }
        for key in [
            Key::S,
            Key::O,
            Key::N,
            Key::E,
            Key::D,
            Key::A,
            Key::G,
            Key::Z,
            Key::Num0,
        ] {
            assert_eq!(
                key_shortcut(key, alt),
                None,
                "unassigned Alt chord must not fire {key:?}"
            );
        }
    }

    #[test]
    fn precision_chords_toggle_state_and_healing_keeps_its_shift_variant() {
        let ctx = context();
        let mut studio = Studio::new();
        studio.show_welcome = false;
        let guides = studio.doc.ruler.guides_visible;
        let snap = studio.snap.enabled;
        frame(
            &ctx,
            &mut studio,
            vec![
                key(Key::Semicolon, Modifiers::CTRL),
                Event::ModifiersChanged(Modifiers::NONE),
            ],
        );
        assert_eq!(studio.doc.ruler.guides_visible, !guides);
        frame(
            &ctx,
            &mut studio,
            vec![
                key(Key::Colon, Modifiers::CTRL | Modifiers::SHIFT),
                Event::ModifiersChanged(Modifiers::NONE),
            ],
        );
        assert_eq!(studio.snap.enabled, !snap);
        frame(
            &ctx,
            &mut studio,
            vec![key(Key::Semicolon, Modifiers::CTRL | Modifiers::SHIFT)],
        );
        assert_eq!(studio.snap.enabled, snap);
        studio.persona = Persona::Pixel;
        frame(&ctx, &mut studio, vec![key(Key::J, Modifiers::SHIFT)]);
        assert_eq!(studio.tool, Tool::Heal);
        frame(&ctx, &mut studio, vec![key(Key::J, Modifiers::NONE)]);
        assert_eq!(studio.tool, Tool::Clone);
    }

    #[test]
    fn free_transform_chord_preserves_the_selected_artwork_and_commits_live_type() {
        let ctx = context();
        let mut studio = Studio::new();
        let id = add_rectangle(&mut studio, 10.0);
        studio.tool = Tool::Node;
        let before = studio.doc.find_shape(1, id).unwrap().clone();
        let history = studio.history.len();
        frame(
            &ctx,
            &mut studio,
            vec![
                key(Key::T, Modifiers::CTRL),
                Event::ModifiersChanged(Modifiers::NONE),
            ],
        );
        assert_eq!(studio.tool, Tool::Select);
        assert_eq!(studio.selection, vec![(1, id)]);
        assert_eq!(studio.doc.find_shape(1, id), Some(&before));
        assert_eq!(studio.history.len(), history);
        studio.place_text(Pt::new(60.0, 80.0));
        studio.type_insert("Make it yours");
        let text = studio.primary().unwrap();
        frame(&ctx, &mut studio, vec![key(Key::T, Modifiers::CTRL)]);
        assert!(studio.type_edit.is_none());
        assert_eq!(studio.tool, Tool::Select);
        let Geom::Text(run) = &studio.doc.find_shape(text.0, text.1).unwrap().geom else {
            panic!("live text stays text")
        };
        assert_eq!(run.content, "Make it yours");
    }

    #[test]
    fn native_clipboard_events_copy_cut_paste_and_preserve_undo() {
        let ctx = context();
        let mut studio = Studio::new();
        let first = add_rectangle(&mut studio, 10.0);
        let payload = copied(frame(
            &ctx,
            &mut studio,
            vec![
                Event::ModifiersChanged(Modifiers::CTRL),
                key(Key::C, Modifiers::CTRL),
                Event::Copy,
                Event::ModifiersChanged(Modifiers::NONE),
            ],
        ));
        assert!(payload.starts_with(Studio::CLIP_PREFIX));
        assert_eq!(studio.clipboard[0].id, first);
        let history = studio.history.len();
        frame(
            &ctx,
            &mut studio,
            vec![key(Key::V, Modifiers::CTRL), Event::Paste(payload.clone())],
        );
        assert_eq!(
            studio.history.len(),
            history + 1,
            "one native paste creates one undo step"
        );
        assert_eq!(count(&studio), 2);
        assert_ne!(studio.selection[0].1, first);
        frame(
            &ctx,
            &mut studio,
            vec![key(Key::X, Modifiers::CTRL), Event::Cut],
        );
        assert_eq!(count(&studio), 1);
        frame(
            &ctx,
            &mut studio,
            vec![
                key(Key::Z, Modifiers::CTRL),
                Event::ModifiersChanged(Modifiers::NONE),
            ],
        );
        assert_eq!(
            count(&studio),
            2,
            "fast Ctrl release must still undo the cut"
        );
        frame(
            &ctx,
            &mut studio,
            vec![key(Key::Z, Modifiers::CTRL | Modifiers::SHIFT)],
        );
        assert_eq!(count(&studio), 1);
        frame(&ctx, &mut studio, vec![key(Key::Z, Modifiers::CTRL)]);
        frame(&ctx, &mut studio, vec![key(Key::Y, Modifiers::CTRL)]);
        assert_eq!(count(&studio), 1);
        let mut other_window = Studio::new();
        frame(&context(), &mut other_window, vec![Event::Paste(payload)]);
        assert_eq!(
            count(&other_window),
            1,
            "OS clipboard works without an internal clipboard"
        );
    }

    #[test]
    fn photo_adjustment_chords_fire_on_press_and_save_the_selected_set() {
        use crate::photo::{PhotoImage, edits};
        let ctx = context();
        let mut studio = Studio::new();
        studio.persona = Persona::Photo;
        studio.show_welcome = false;
        let folder = std::env::temp_dir().join(format!(
            "omadesign-photo-shortcuts-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir(&folder).unwrap();
        let paths: Vec<_> = (0..3)
            .map(|i| folder.join(format!("photo-{i}.png")))
            .collect();
        for path in &paths {
            image::RgbaImage::from_pixel(4, 4, image::Rgba([100, 80, 60, 255]))
                .save(path)
                .unwrap();
            studio.photo.images.push(PhotoImage::load(path).unwrap());
        }
        studio.photo.select_image(0);
        studio.photo.images[0].develop.exposure = 0.6;
        let shift = Modifiers::CTRL | Modifiers::COMMAND | Modifiers::SHIFT;
        let output = frame(
            &ctx,
            &mut studio,
            vec![
                key(Key::C, shift),
                Event::Copy,
                Event::ModifiersChanged(Modifiers::NONE),
            ],
        );
        assert!(
            output.is_empty(),
            "adjustment copy must not replace the system clipboard"
        );
        let copied = studio.photo.copied_adjustments.clone().unwrap();
        assert_eq!(copied.params.exposure, 0.6);
        assert!(!copied.categories.crop && !copied.categories.rotation);
        studio.photo.images[0].develop.exposure = 0.9;
        frame(
            &ctx,
            &mut studio,
            vec![
                key(Key::A, Modifiers::CTRL),
                Event::ModifiersChanged(Modifiers::NONE),
            ],
        );
        assert_eq!(studio.photo.selected_count(), 3);
        assert_eq!(
            studio.photo.selected,
            Some(0),
            "select all preserves the active source"
        );
        // No Paste event: this is exactly what the patched native bridge emits
        // when the OS clipboard is empty. Opening the dialog cannot modify photos.
        frame(
            &ctx,
            &mut studio,
            vec![key(Key::V, shift), Event::ModifiersChanged(Modifiers::NONE)],
        );
        fn contains_text(shape: &egui::Shape, text: &str) -> bool {
            match shape {
                egui::Shape::Text(text_shape) => text_shape.galley.text().contains(text),
                egui::Shape::Vec(shapes) => shapes.iter().any(|shape| contains_text(shape, text)),
                _ => false,
            }
        }
        let mut dialog_visible = false;
        // Egui's first modal pass measures the content before painting it.
        for _ in 0..3 {
            let mut output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1200., 800.),
                    )),
                    ..Default::default()
                },
                |ui| crate::ui::photo::show(ui, &mut studio),
            );
            output.textures_delta.clear();
            dialog_visible |= output
                .shapes
                .iter()
                .any(|shape| contains_text(&shape.shape, "Apply adjustments"));
        }
        assert!(
            dialog_visible,
            "empty OS clipboard still opens the category dialog on key press"
        );
        assert_eq!(studio.photo.images[1].develop.exposure, 0.0);
        assert_eq!(
            studio
                .photo
                .copied_adjustments
                .as_ref()
                .unwrap()
                .params
                .exposure,
            0.6,
            "copied adjustments are an immutable snapshot"
        );
        // Dismiss the modal before exercising the save chord independently.
        let save_ctx = context();
        studio.photo.select_with(1, false, false);
        studio.photo.select_with(2, true, false);
        studio.photo.images[1].develop.exposure = 0.4;
        studio.photo.images[2].develop.exposure = 0.8;
        frame(
            &save_ctx,
            &mut studio,
            vec![
                key(Key::S, Modifiers::CTRL),
                Event::ModifiersChanged(Modifiers::NONE),
            ],
        );
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while studio.photo.is_saving() && std::time::Instant::now() < deadline {
            studio.photo.poll(&save_ctx);
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        assert!(!studio.photo.is_saving());
        assert!(
            studio.photo.save_error().is_empty(),
            "{}",
            studio.photo.save_error()
        );
        assert!(
            !edits::sidecar_path(&paths[0]).exists(),
            "unselected source stays unsaved"
        );
        for (index, expected) in [(1, 0.4), (2, 0.8)] {
            assert_eq!(
                edits::load(
                    &paths[index],
                    &edits::SourceIdentity::read(&paths[index]).unwrap()
                )
                .unwrap()
                .unwrap()
                .exposure,
                expected
            );
        }
        std::fs::remove_dir_all(folder).unwrap();
    }

    #[test]
    fn photo_shortcuts_cannot_edit_hidden_design_artwork() {
        let ctx = context();
        let mut studio = Studio::new();
        let id = add_rectangle(&mut studio, 25.0);
        let shape = studio.doc.find_shape(1, id).unwrap().clone();
        let history = studio.history.len();
        let style = studio.style.clone();
        let payload = copied(frame(&ctx, &mut studio, vec![Event::Copy]));
        studio.persona = Persona::Photo;
        studio.tool = Tool::Hand;
        let ctrl = Modifiers::CTRL | Modifiers::COMMAND;
        for key_code in [
            Key::C,
            Key::X,
            Key::V,
            Key::D,
            Key::G,
            Key::T,
            Key::OpenBracket,
            Key::CloseBracket,
            Key::Semicolon,
        ] {
            for modifiers in [ctrl, ctrl | Modifiers::SHIFT, ctrl | Modifiers::ALT] {
                // Adjustment paste is separately tested; there is no copied look here.
                frame(
                    &ctx,
                    &mut studio,
                    vec![
                        key(key_code, modifiers),
                        Event::ModifiersChanged(Modifiers::NONE),
                    ],
                );
            }
        }
        frame(
            &ctx,
            &mut studio,
            vec![Event::Cut, Event::Paste(payload), Event::Copy],
        );
        for key_code in [
            Key::Delete,
            Key::Backspace,
            Key::ArrowLeft,
            Key::ArrowRight,
            Key::ArrowUp,
            Key::ArrowDown,
            Key::X,
            Key::D,
            Key::OpenBracket,
            Key::CloseBracket,
        ] {
            for modifiers in [Modifiers::NONE, Modifiers::SHIFT] {
                frame(&ctx, &mut studio, vec![key(key_code, modifiers)]);
            }
        }
        assert_eq!(studio.doc.find_shape(1, id), Some(&shape));
        assert_eq!(count(&studio), 1);
        assert_eq!(studio.selection, vec![(1, id)]);
        assert_eq!(studio.history.len(), history);
        assert_eq!(studio.style, style);
        assert_eq!(studio.tool, Tool::Hand);
        let hints = studio.key_hints(&ctx);
        assert!(!hints.keys.iter().any(|hint| matches!(
            hint.label,
            "Copy" | "Cut" | "Paste" | "Duplicate" | "Free transform" | "Combine paths"
        )));
    }

    #[test]
    fn retained_native_clipboard_keys_leave_focused_text_fields_in_charge() {
        let ctx = context();
        let mut studio = Studio::new();
        studio.persona = Persona::Photo;
        studio.show_welcome = false;
        let mut value = "Before".to_owned();
        let command = Modifiers::CTRL | Modifiers::COMMAND;
        for (index, events) in [
            vec![],
            vec![key(Key::A, command)],
            vec![
                key(Key::V, command | Modifiers::SHIFT),
                Event::Paste("After".into()),
                Event::ModifiersChanged(Modifiers::NONE),
            ],
        ]
        .into_iter()
        .enumerate()
        {
            let mut output = ctx.run_ui(
                egui::RawInput {
                    events,
                    ..Default::default()
                },
                |ui| {
                    studio.handle_shortcuts(ui.ctx());
                    let response = ui.text_edit_singleline(&mut value);
                    if index == 0 {
                        response.request_focus();
                    }
                },
            );
            output.textures_delta.clear();
        }
        assert_eq!(value, "After", "native text paste is applied exactly once");
        assert!(studio.photo.copied_adjustments.is_none());
        assert!(!studio.photo.status.contains("Copy adjustments"));
    }

    #[test]
    fn style_clipboard_replays_modifier_changes_within_and_between_frames() {
        let ctx = context();
        let mut studio = Studio::new();
        let source = add_rectangle(&mut studio, 10.0);
        studio.doc.find_shape_mut(1, source).unwrap().style.fill = Fill::None;
        let mods = Modifiers::CTRL | Modifiers::ALT;
        let payload = copied(frame(
            &ctx,
            &mut studio,
            vec![
                Event::ModifiersChanged(mods),
                Event::Copy,
                Event::ModifiersChanged(Modifiers::NONE),
            ],
        ));
        assert!(payload.starts_with("omadesign-style:"));
        assert!(
            studio.clipboard.is_empty(),
            "style copy must not copy objects"
        );
        let dest = add_rectangle(&mut studio, 60.0);
        studio.style_clip = None;
        frame(&ctx, &mut studio, vec![Event::ModifiersChanged(mods)]);
        frame(
            &ctx,
            &mut studio,
            vec![
                Event::Paste(payload),
                Event::ModifiersChanged(Modifiers::NONE),
            ],
        );
        assert_eq!(count(&studio), 2);
        assert_eq!(
            studio.doc.find_shape(1, dest).unwrap().style.fill,
            Fill::None
        );
        frame(&ctx, &mut studio, vec![key(Key::Z, Modifiers::CTRL)]);
        assert_eq!(
            studio.doc.find_shape(1, dest).unwrap().style,
            Style::default()
        );
    }

    #[test]
    fn select_duplicate_arrange_combine_and_nudge_use_native_chords() {
        let ctx = context();
        let mut studio = Studio::new();
        let a = add_rectangle(&mut studio, 10.0);
        let b = add_rectangle(&mut studio, 50.0);
        let c = add_rectangle(&mut studio, 90.0);
        studio.selection = vec![(1, a)];
        let order = |s: &Studio| {
            s.doc.layers[1]
                .kind
                .shapes()
                .unwrap()
                .iter()
                .map(|s| s.id)
                .collect::<Vec<_>>()
        };
        frame(
            &ctx,
            &mut studio,
            vec![key(Key::CloseBracket, Modifiers::CTRL)],
        );
        assert_eq!(order(&studio), vec![b, a, c]);
        frame(
            &ctx,
            &mut studio,
            vec![key(
                Key::CloseCurlyBracket,
                Modifiers::CTRL | Modifiers::SHIFT,
            )],
        );
        assert_eq!(order(&studio), vec![b, c, a]);
        frame(
            &ctx,
            &mut studio,
            vec![key(Key::OpenBracket, Modifiers::CTRL)],
        );
        assert_eq!(order(&studio), vec![b, a, c]);
        frame(
            &ctx,
            &mut studio,
            vec![key(
                Key::OpenCurlyBracket,
                Modifiers::CTRL | Modifiers::SHIFT,
            )],
        );
        assert_eq!(order(&studio), vec![a, b, c]);
        frame(
            &ctx,
            &mut studio,
            vec![
                key(Key::ArrowLeft, Modifiers::SHIFT),
                key(Key::ArrowUp, Modifiers::SHIFT),
                key(Key::ArrowRight, Modifiers::NONE),
                key(Key::ArrowDown, Modifiers::NONE),
            ],
        );
        let Geom::Rect { origin, .. } = studio.doc.find_shape(1, a).unwrap().geom else {
            panic!("rectangle");
        };
        assert_eq!(origin, Pt::new(1.0, 11.0));
        frame(&ctx, &mut studio, vec![key(Key::D, Modifiers::CTRL)]);
        assert_eq!(count(&studio), 4);
        frame(&ctx, &mut studio, vec![key(Key::Delete, Modifiers::NONE)]);
        assert_eq!(count(&studio), 3);
        studio.doc.find_shape_mut(1, c).unwrap().locked = true;
        frame(&ctx, &mut studio, vec![key(Key::A, Modifiers::CTRL)]);
        assert_eq!(studio.selection, vec![(1, a), (1, b)]);
        frame(&ctx, &mut studio, vec![key(Key::G, Modifiers::CTRL)]);
        assert_eq!(count(&studio), 2);
        frame(
            &ctx,
            &mut studio,
            vec![key(Key::G, Modifiers::CTRL | Modifiers::SHIFT)],
        );
        assert_eq!(count(&studio), 3);
        studio.doc.layers[1].visible = false;
        frame(&ctx, &mut studio, vec![key(Key::A, Modifiers::CTRL)]);
        assert!(studio.selection.is_empty());
        studio.doc.layers[1].visible = true;
        studio.doc.layers[1].locked = true;
        frame(&ctx, &mut studio, vec![key(Key::A, Modifiers::CTRL)]);
        assert!(studio.selection.is_empty());
    }

    #[test]
    fn type_editor_uses_native_clipboard_and_per_event_shift_selection() {
        let ctx = context();
        let mut studio = Studio::new();
        studio.place_text(Pt::new(40.0, 80.0));
        frame(&ctx, &mut studio, vec![Event::Text("héllo".into())]);
        frame(
            &ctx,
            &mut studio,
            vec![
                key(Key::Home, Modifiers::NONE),
                key(Key::ArrowRight, Modifiers::SHIFT),
                key(Key::ArrowRight, Modifiers::SHIFT),
            ],
        );
        assert_eq!(copied(frame(&ctx, &mut studio, vec![Event::Copy])), "hé");
        assert_eq!(copied(frame(&ctx, &mut studio, vec![Event::Cut])), "hé");
        assert_eq!(studio.live_type_mut().unwrap().content, "llo");
        frame(
            &ctx,
            &mut studio,
            vec![
                key(Key::A, Modifiers::CTRL),
                Event::Paste("Bonjour\nworld".into()),
            ],
        );
        assert_eq!(studio.live_type_mut().unwrap().content, "Bonjour\nworld");
        frame(&ctx, &mut studio, vec![key(Key::Home, Modifiers::SHIFT)]);
        assert_eq!(copied(frame(&ctx, &mut studio, vec![Event::Copy])), "world");
        frame(&ctx, &mut studio, vec![key(Key::D, Modifiers::CTRL)]);
        assert_eq!(
            count(&studio),
            1,
            "object commands must not affect live text"
        );
        frame(&ctx, &mut studio, vec![key(Key::Z, Modifiers::CTRL)]);
        assert!(studio.type_edit.is_none());
        frame(
            &ctx,
            &mut studio,
            vec![key(Key::Z, Modifiers::CTRL | Modifiers::SHIFT)],
        );
        let Geom::Text(run) = &studio.doc.layers[1].kind.shapes().unwrap()[0].geom else {
            panic!("text");
        };
        assert_eq!(run.content, "Bonjour\nworld");
    }

    #[test]
    fn focused_inspector_keeps_its_own_editing_shortcuts() {
        let ctx = context();
        let mut studio = Studio::new();
        let selected = add_rectangle(&mut studio, 10.0);
        let mut value = "Inspector value".to_owned();
        let mut run = |events, focus| {
            let mut output = ctx.run_ui(
                egui::RawInput {
                    events,
                    ..Default::default()
                },
                |ui| {
                    studio.handle_shortcuts(ui.ctx());
                    let response = ui.text_edit_singleline(&mut value);
                    if focus {
                        response.request_focus();
                    }
                },
            );
            output.textures_delta.clear();
            output.platform_output.commands
        };
        run(vec![], true);
        run(
            vec![key(Key::A, Modifiers::CTRL | Modifiers::COMMAND)],
            false,
        );
        assert_eq!(copied(run(vec![Event::Copy], false)), "Inspector value");
        assert_eq!(copied(run(vec![Event::Cut], false)), "Inspector value");
        run(
            vec![
                Event::Paste("32".into()),
                key(Key::D, Modifiers::CTRL | Modifiers::COMMAND),
            ],
            false,
        );
        run(
            vec![key(Key::Z, Modifiers::CTRL | Modifiers::COMMAND)],
            false,
        );
        run(
            vec![key(
                Key::Z,
                Modifiers::CTRL | Modifiers::COMMAND | Modifiers::SHIFT,
            )],
            false,
        );
        assert_eq!(count(&studio), 1);
        assert_eq!(studio.selection, vec![(1, selected)]);
        assert!(studio.clipboard.is_empty());
    }

    #[test]
    fn modal_and_popup_shortcuts_never_leak_to_the_document() {
        let ctx = context();
        let mut studio = Studio::new();
        add_rectangle(&mut studio, 10.0);
        let events = || {
            vec![
                Event::Cut,
                key(Key::D, Modifiers::CTRL),
                key(Key::N, Modifiers::CTRL),
                key(Key::F1, Modifiers::NONE),
            ]
        };
        studio.pending_nav = Some(PendingNav::CloseTab(0));
        frame(&ctx, &mut studio, events());
        assert_eq!(count(&studio), 1);
        assert_eq!(studio.tab_count(), 1);
        assert!(!studio.show_shortcuts);
        studio.pending_nav = None;
        egui::Popup::open_id(&ctx, egui::Id::new("test-popup"));
        frame(&ctx, &mut studio, events());
        assert_eq!(count(&studio), 1);
        assert!(!studio.show_shortcuts);
        egui::Popup::close_id(&ctx, egui::Id::new("test-popup"));
        let mut output = ctx.run_ui(Default::default(), |ui| {
            ui.ctx().memory_mut(|memory| {
                memory.set_modal_layer(egui::LayerId::new(
                    egui::Order::Foreground,
                    egui::Id::new("test-modal"),
                ))
            });
        });
        output.textures_delta.clear();
        frame(&ctx, &mut studio, events());
        assert_eq!(count(&studio), 1);
        assert_eq!(studio.tab_count(), 1);
        assert!(!studio.show_shortcuts);
    }

    #[test]
    fn zoom_new_help_and_shift_tool_chords_change_the_correct_state() {
        let ctx = context();
        let mut studio = Studio::new();
        studio.show_welcome = false;
        studio.view.scale = 1.0;
        frame(&ctx, &mut studio, vec![key(Key::Equals, Modifiers::CTRL)]);
        assert_eq!(studio.view.scale, 1.25);
        frame(&ctx, &mut studio, vec![key(Key::Minus, Modifiers::CTRL)]);
        assert_eq!(studio.view.scale, 1.0);
        studio.view.scale = 4.0;
        frame(&ctx, &mut studio, vec![key(Key::Num1, Modifiers::CTRL)]);
        assert_eq!(studio.view.scale, 1.0);
        studio.need_fit = false;
        frame(&ctx, &mut studio, vec![key(Key::Num0, Modifiers::CTRL)]);
        assert!(studio.need_fit);
        frame(&ctx, &mut studio, vec![key(Key::O, Modifiers::SHIFT)]);
        assert_eq!(studio.tool, Tool::Artboard);
        studio.persona = Persona::Pixel;
        frame(&ctx, &mut studio, vec![key(Key::M, Modifiers::SHIFT)]);
        assert_eq!(studio.tool, Tool::Marquee);
        frame(&ctx, &mut studio, vec![key(Key::O, Modifiers::SHIFT)]);
        assert_eq!(studio.tool, Tool::EllipseMarquee);
        studio.brush.hardness = 0.5;
        let size = studio.brush.size;
        frame(
            &ctx,
            &mut studio,
            vec![key(Key::CloseCurlyBracket, Modifiers::SHIFT)],
        );
        assert!((studio.brush.hardness - 0.58).abs() < 1e-5);
        assert_eq!(studio.brush.size, size);
        frame(
            &ctx,
            &mut studio,
            vec![key(Key::OpenCurlyBracket, Modifiers::SHIFT)],
        );
        assert!((studio.brush.hardness - 0.5).abs() < 1e-5);
        frame(
            &ctx,
            &mut studio,
            vec![
                key(Key::CloseBracket, Modifiers::NONE),
                key(Key::OpenBracket, Modifiers::NONE),
            ],
        );
        assert_eq!(studio.brush.size, size);
        studio.persona = Persona::Photo;
        frame(&ctx, &mut studio, vec![key(Key::Equals, Modifiers::CTRL)]);
        assert_eq!(studio.photo.view_scale, 1.25);
        frame(&ctx, &mut studio, vec![key(Key::Minus, Modifiers::CTRL)]);
        assert_eq!(studio.photo.view_scale, 1.0);
        studio.photo.fit_scale = 0.25;
        studio.photo.view_offset = egui::vec2(30.0, 20.0);
        frame(&ctx, &mut studio, vec![key(Key::Num1, Modifiers::CTRL)]);
        assert_eq!(studio.photo.view_scale, 4.0);
        assert_eq!(studio.photo.view_offset, egui::Vec2::ZERO);
        studio.photo.view_scale = 3.0;
        frame(&ctx, &mut studio, vec![key(Key::Num0, Modifiers::CTRL)]);
        assert_eq!(studio.photo.view_scale, 1.0);
        frame(&ctx, &mut studio, vec![key(Key::F1, Modifiers::NONE)]);
        assert!(studio.show_shortcuts);
        frame(&ctx, &mut studio, vec![key(Key::N, Modifiers::CTRL)]);
        assert_eq!(studio.tab_count(), 2);
    }

    #[test]
    fn every_advertised_tool_and_motion_transport_key_works() {
        let ctx = context();
        let mut studio = Studio::new();
        for (persona, tools) in [
            (Persona::Design, Tool::design_well()),
            (Persona::Pixel, Tool::pixel_well()),
            (Persona::Photo, Tool::photo_well()),
            (Persona::Motion, Tool::motion_well()),
        ] {
            studio.persona = persona;
            for &tool in tools {
                studio.tool = if tool == Tool::Hand {
                    Tool::Zoom
                } else {
                    Tool::Hand
                };
                let (name, mods) = tool
                    .key()
                    .strip_prefix("Shift+")
                    .map_or((tool.key(), Modifiers::NONE), |name| {
                        (name, Modifiers::SHIFT)
                    });
                let pressed = Key::from_name(name).expect("advertised egui key");
                frame(&ctx, &mut studio, vec![key(pressed, mods)]);
                assert_eq!(studio.tool, tool, "{persona:?}: {}", tool.key());
            }
        }
        studio.persona = Persona::Motion;
        let id = add_rectangle(&mut studio, 10.0);
        studio.playhead = 0.5;
        frame(&ctx, &mut studio, vec![key(Key::K, Modifiers::NONE)]);
        assert!(
            studio
                .doc
                .motion
                .tracks
                .iter()
                .any(|track| track.shape == id)
        );
        frame(&ctx, &mut studio, vec![key(Key::Space, Modifiers::NONE)]);
        assert!(studio.playing);
        frame(&ctx, &mut studio, vec![key(Key::Space, Modifiers::NONE)]);
        assert!(!studio.playing);
        frame(&ctx, &mut studio, vec![key(Key::End, Modifiers::NONE)]);
        assert_eq!(studio.playhead, studio.doc.motion.duration);
        frame(&ctx, &mut studio, vec![key(Key::Home, Modifiers::NONE)]);
        assert_eq!(studio.playhead, 0.0);
    }

    fn key_repeat(key: Key, modifiers: Modifiers) -> Event {
        Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: true,
            modifiers,
        }
    }

    #[test]
    fn motion_delete_strips_animation_and_ignores_a_held_key() {
        let ctx = context();
        let mut studio = Studio::new();
        studio.persona = Persona::Motion;
        let id = add_rectangle(&mut studio, 10.0);
        studio
            .doc
            .motion
            .set_key(id, Prop::X, 0.0, 0.0, Ease::Linear);
        studio
            .doc
            .motion
            .set_key(id, Prop::X, 1.0, 24.0, Ease::Linear);
        frame(&ctx, &mut studio, vec![key(Key::Delete, Modifiers::NONE)]);
        assert_eq!(count(&studio), 1);
        assert!(!studio.doc.motion.has_shape(id));
        frame(
            &ctx,
            &mut studio,
            vec![key_repeat(Key::Delete, Modifiers::NONE)],
        );
        assert_eq!(count(&studio), 1);
        assert!(studio.doc.find_shape(1, id).is_some());
        frame(
            &ctx,
            &mut studio,
            vec![Event::Key {
                key: Key::Delete,
                physical_key: None,
                pressed: false,
                repeat: false,
                modifiers: Modifiers::NONE,
            }],
        );
        frame(&ctx, &mut studio, vec![key(Key::Delete, Modifiers::NONE)]);
        assert_eq!(count(&studio), 0);
    }

    #[test]
    fn motion_delete_of_a_selected_key_does_not_take_the_object() {
        let ctx = context();
        let mut studio = Studio::new();
        studio.persona = Persona::Motion;
        let id = add_rectangle(&mut studio, 10.0);
        studio
            .doc
            .motion
            .set_key(id, Prop::Scale, 0.0, 0.2, Ease::EaseOut);
        studio
            .doc
            .motion
            .set_key(id, Prop::Scale, 1.0, 1.0, Ease::Linear);
        studio.selected_key = Some((id, Prop::Scale, 1));
        frame(&ctx, &mut studio, vec![key(Key::Delete, Modifiers::NONE)]);
        assert_eq!(count(&studio), 1);
        assert!(studio.doc.motion.has_shape(id));
        assert_eq!(studio.selected_key, Some((id, Prop::Scale, 0)));
        frame(&ctx, &mut studio, vec![key(Key::Delete, Modifiers::NONE)]);
        assert_eq!(count(&studio), 1);
        assert!(!studio.doc.motion.has_shape(id));
    }
}
