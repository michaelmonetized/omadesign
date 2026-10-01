//! Canvas composition stays outside the document until the input method commits.
use super::*;
use egui::{Event, ImeEvent, Key};
use std::ops::{Deref, Range};

#[derive(Clone, Default)]
pub struct TypeIme {
    pub preedit: Option<Preedit>,
    pub interrupt: bool,
}

#[derive(Clone)]
pub struct Preedit {
    pub text: String,
    pub active: Option<Range<usize>>,
}

impl Studio {
    pub(crate) fn owns_type_ime(&self, ctx: &egui::Context) -> bool {
        self.shortcut_focus(ctx) == shortcuts::ShortcutFocus::Text
            && !self.show_welcome
            && !self.show_preferences
            && !self.file_dialog_pending()
    }

    pub(crate) fn cancel_type_ime(&mut self) {
        if let Some(edit) = &mut self.type_edit
            && edit.ime.preedit.take().is_some()
        {
            edit.ime.interrupt = true;
            self.canvas_key = None;
            self.interaction_render.clear();
        }
    }

    /// Consume composition before ordinary typing and keyboard shortcuts.
    pub(super) fn type_ime_event(&mut self, event: &Event) -> bool {
        if matches!(event, Event::Ime(ImeEvent::Preedit { text, .. }) if !text.is_empty()) {
            // A clipboard read started before composition must not arrive after
            // a cancelled preedit and replace text at the unchanged caret.
            self.type_paste_jobs.clear();
        }
        let Some(edit) = self.type_edit.as_mut() else {
            return false;
        };
        match event {
            Event::Ime(ImeEvent::Preedit {
                text,
                active_range_chars,
            }) => {
                let count = text.chars().count();
                edit.ime.preedit = (!text.is_empty()).then(|| Preedit {
                    text: text.clone(),
                    active: active_range_chars
                        .as_ref()
                        .map(|r| r.start.min(count)..r.end.max(r.start).min(count)),
                });
            }
            Event::Ime(ImeEvent::Commit(text)) => {
                edit.ime.preedit = None;
                // Empty commits are cancellation, not deletion of the selection.
                if !text.is_empty() && text != "\n" && text != "\r" {
                    self.type_insert(text);
                }
            }
            Event::Ime(ImeEvent::DeleteSurrounding {
                before_chars,
                after_chars,
            }) => {
                edit.ime.preedit = None;
                let caret = edit.caret;
                let count = self
                    .selected_type()
                    .map_or(0, |r| r.content.chars().count());
                self.type_delete_range(
                    caret.saturating_sub(*before_chars),
                    caret.saturating_add(*after_chars).min(count),
                );
            }
            #[allow(deprecated)]
            Event::Ime(ImeEvent::Disabled) => {
                edit.ime.preedit = None;
            }
            #[allow(deprecated)]
            Event::Ime(ImeEvent::Enabled) => {}
            Event::Key {
                key: Key::Escape,
                pressed: true,
                ..
            } if edit.ime.preedit.is_some() => {
                self.cancel_type_ime();
            }
            // These keys belong to candidate selection while composition is active.
            Event::Key { modifiers, .. }
                if edit.ime.preedit.is_some()
                    && !(modifiers.ctrl
                        || modifiers.command
                        || modifiers.mac_cmd
                        || modifiers.alt) =>
            {
                return true;
            }
            Event::Text(_) if edit.ime.preedit.is_some() => return true,
            _ => return false,
        }
        self.canvas_key = None;
        self.interaction_render.clear();
        true
    }
}

/// Temporarily render the preedit through the normal text/layout compositor.
/// Only text geometry is copied; large raster layers remain borrowed. The guard
/// restores every geometry changed by reflow, including on unwind. No document
/// action, serialization, agent call or history mutation runs inside this guard.
pub(crate) struct Preview<'a> {
    doc: &'a mut Document,
    originals: Vec<(usize, u64, Geom)>,
}

impl<'a> Preview<'a> {
    pub(crate) fn new(doc: &'a mut Document, edit: Option<&TypeEdit>) -> Self {
        let mut preview = Self {
            doc,
            originals: vec![],
        };
        let Some(edit) = edit else {
            return preview;
        };
        let Some(preedit) = &edit.ime.preedit else {
            return preview;
        };
        for (li, layer) in preview.doc.layers.iter().enumerate() {
            for shape in layer.kind.shapes().unwrap_or(&[]) {
                if matches!(shape.geom, Geom::Text(_)) {
                    preview.originals.push((li, shape.id, shape.geom.clone()));
                }
            }
        }
        if let Some(shape) = preview.doc.find_shape_mut(edit.layer, edit.id)
            && let Geom::Text(run) = &mut shape.geom
        {
            run.replace_text(
                edit.caret.min(edit.anchor),
                edit.caret.max(edit.anchor),
                &preedit.text,
                edit.pending_style.clone(),
            );
            crate::text::fill_contours(&mut shape.geom);
        }
        crate::text_geometry::reflow(preview.doc);
        preview
    }
}

impl Deref for Preview<'_> {
    type Target = Document;
    fn deref(&self) -> &Document {
        self.doc
    }
}

impl Drop for Preview<'_> {
    fn drop(&mut self) {
        for (li, id, geom) in self.originals.drain(..) {
            if let Some(shape) = self.doc.find_shape_mut(li, id) {
                shape.geom = geom;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{Key, Modifiers};

    fn setup() -> (egui::Context, Studio) {
        let ctx = egui::Context::default();
        let mut studio = Studio::new();
        studio.show_welcome = false;
        studio.active_layer = Some(1);
        studio.place_text(Pt::new(40., 80.));
        (ctx, studio)
    }
    fn frame(ctx: &egui::Context, studio: &mut Studio, events: Vec<Event>) {
        let mut out = ctx.run_ui(
            egui::RawInput {
                events,
                ..Default::default()
            },
            |ui| studio.handle_shortcuts(ui.ctx()),
        );
        out.textures_delta.clear();
    }
    fn preedit(text: &str) -> Event {
        Event::Ime(ImeEvent::Preedit {
            text: text.into(),
            active_range_chars: Some(0..text.chars().count()),
        })
    }
    fn key(key: Key) -> Event {
        Event::Key {
            key,
            physical_key: Some(key),
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        }
    }

    #[test]
    fn composition_preview_never_changes_saved_document_and_commit_undoes_once() {
        let (ctx, mut studio) = setup();
        let before = serde_json::to_string(&studio.doc).unwrap();
        frame(&ctx, &mut studio, vec![preedit("ni hao")]);
        assert_eq!(serde_json::to_string(&studio.doc).unwrap(), before);
        let edit = studio.type_edit.clone();
        {
            let preview = Preview::new(&mut studio.doc, edit.as_ref());
            let e = edit.as_ref().unwrap();
            let Geom::Text(run) = &preview.find_shape(e.layer, e.id).unwrap().geom else {
                panic!()
            };
            assert_eq!(run.content, "ni hao");
        }
        assert_eq!(serde_json::to_string(&studio.doc).unwrap(), before);
        frame(
            &ctx,
            &mut studio,
            vec![
                preedit("你好"),
                Event::Ime(ImeEvent::Commit("你好日本語한국어".into())),
            ],
        );
        assert_eq!(studio.selected_type().unwrap().content, "你好日本語한국어");
        assert!(studio.type_edit.as_ref().unwrap().ime.preedit.is_none());
        studio.commit_type_edit();
        studio.undo();
        assert_eq!(studio.selected_type().unwrap().content, "Type");
        studio.redo();
        assert_eq!(studio.selected_type().unwrap().content, "你好日本語한국어");
    }

    #[test]
    fn empty_composition_events_preserve_selected_text_and_cancel_is_not_commit() {
        let (ctx, mut studio) = setup();
        for events in [
            vec![preedit("")],
            vec![Event::Ime(ImeEvent::Commit("".into()))],
            vec![preedit("ni"), preedit("")],
            vec![preedit("ni"), key(Key::Escape)],
        ] {
            frame(&ctx, &mut studio, events);
            assert_eq!(studio.selected_type().unwrap().content, "Type");
            let edit = studio.type_edit.as_ref().unwrap();
            assert_eq!((edit.anchor, edit.caret), (0, 4));
            assert!(edit.ime.preedit.is_none());
        }
        assert!(studio.type_edit.is_some());
    }

    #[test]
    fn candidate_navigation_does_not_edit_source_or_move_caret() {
        let (ctx, mut studio) = setup();
        frame(
            &ctx,
            &mut studio,
            vec![
                preedit("ni"),
                key(Key::ArrowLeft),
                key(Key::Enter),
                key(Key::Backspace),
                Event::Text("n".into()),
            ],
        );
        assert_eq!(studio.selected_type().unwrap().content, "Type");
        assert_eq!(studio.type_edit.as_ref().unwrap().caret, 4);
        frame(
            &ctx,
            &mut studio,
            vec![Event::Ime(ImeEvent::Commit("你".into()))],
        );
        assert_eq!(studio.selected_type().unwrap().content, "你");
        assert_eq!(studio.type_edit.as_ref().unwrap().caret, 1);
    }

    #[test]
    fn focus_transfer_cancels_preview_and_leaves_ime_for_the_field() {
        let (ctx, mut studio) = setup();
        frame(&ctx, &mut studio, vec![preedit("ni")]);
        ctx.memory_mut(|m| m.request_focus(egui::Id::new("other-field")));
        frame(
            &ctx,
            &mut studio,
            vec![Event::Ime(ImeEvent::Commit("你".into()))],
        );
        assert!(studio.type_edit.as_ref().unwrap().ime.preedit.is_none());
        assert_eq!(studio.selected_type().unwrap().content, "Type");
        assert!(ctx.input(|i| i.events.iter().any(|e| matches!(e, Event::Ime(_)))));
    }

    #[test]
    fn pointer_reposition_invalidates_composition_and_pending_clipboard_destination() {
        let (ctx, mut studio) = setup();
        frame(&ctx, &mut studio, vec![preedit("ni")]);
        let edit = studio.type_edit.as_ref().unwrap().clone();
        assert!(studio.type_pointer_caret(edit.frame, Pt::new(40., 80.), false));
        assert!(studio.type_edit.as_ref().unwrap().ime.preedit.is_none());
        assert!(studio.type_edit.as_ref().unwrap().ime.interrupt);
        assert_eq!(studio.selected_type().unwrap().content, "Type");
    }

    #[test]
    fn surrounding_deletion_uses_characters_and_clamps_to_document() {
        let (ctx, mut studio) = setup();
        frame(
            &ctx,
            &mut studio,
            vec![Event::Ime(ImeEvent::Commit("a你好b".into()))],
        );
        studio.type_edit.as_mut().unwrap().caret = 3;
        studio.type_edit.as_mut().unwrap().anchor = 3;
        frame(
            &ctx,
            &mut studio,
            vec![Event::Ime(ImeEvent::DeleteSurrounding {
                before_chars: 2,
                after_chars: usize::MAX,
            })],
        );
        assert_eq!(studio.selected_type().unwrap().content, "a");
    }

    #[test]
    fn canvas_outputs_ime_at_transformed_caret_and_disables_it_after_editing() {
        let (ctx, mut studio) = setup();
        crate::ui::theme::apply(&ctx);
        studio.need_fit = false;
        studio.view.scale = 1.75;
        studio.view.offset = Pt::new(65., 55.);
        let input = || egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1440., 900.),
            )),
            ..Default::default()
        };
        let mut output = ctx.run_ui(input(), |ui| crate::ui::run(ui, &mut studio));
        output.textures_delta.clear();
        let mut output = ctx.run_ui(input(), |ui| crate::ui::run(ui, &mut studio));
        output.textures_delta.clear();
        let ime = output
            .platform_output
            .ime
            .expect("canvas must enable native IME");
        assert!(ime.rect.width() < 10.);
        assert!(ime.rect.height() > 20.);
        assert!(studio.canvas_rect.unwrap().contains(ime.rect.center()));
        studio.commit_type_edit();
        let mut output = ctx.run_ui(input(), |ui| crate::ui::run(ui, &mut studio));
        output.textures_delta.clear();
        assert!(output.platform_output.ime.is_none());
    }
}
