//! Native text paste preserves the destination session while data is in flight.
use super::*;
use crate::clipboard::type_style::{self, Paste};
use std::sync::mpsc::{self, Receiver, TryRecvError};

pub(super) struct TypePasteJob {
    receiver: Receiver<Result<Paste, String>>,
    owner: String,
    edit: TypeEdit,
    source: String,
}
impl Studio {
    pub(super) fn request_type_clipboard_paste(
        &mut self,
        ctx: &egui::Context,
        payload: Option<&str>,
    ) {
        let Some(edit) = self.type_edit.clone() else {
            return;
        };
        let Some(source) = self.selected_type().map(|run| run.content) else {
            return;
        };
        let (sender, receiver) = mpsc::channel();
        let payload = payload.map(str::to_owned);
        #[cfg(not(test))]
        {
            let ctx = ctx.clone();
            std::thread::spawn(move || {
                let _ = sender.send(type_style::read(payload));
                ctx.request_repaint();
            });
        }
        #[cfg(test)]
        {
            let _ = sender.send(type_style::read(payload));
        }
        self.type_paste_jobs.push(TypePasteJob {
            receiver,
            owner: self.swap_id.clone(),
            edit,
            source,
        });
        self.poll_type_clipboard_jobs(ctx);
    }
    pub(super) fn poll_type_clipboard_jobs(&mut self, ctx: &egui::Context) {
        if self.file_dialog_pending() {
            return;
        }
        while let Some(job) = self.type_paste_jobs.first() {
            let result = match job.receiver.try_recv() {
                Ok(value) => value,
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => Err("Text clipboard worker stopped".into()),
            };
            let job = self.type_paste_jobs.remove(0);
            let same = self.swap_id == job.owner
                && self.type_edit.as_ref().is_some_and(|e| {
                    (e.layer, e.id, e.caret, e.anchor)
                        == (job.edit.layer, job.edit.id, job.edit.caret, job.edit.anchor)
                        && e.pending_style == job.edit.pending_style
                        && e.ime.preedit.is_none()
                })
                && self
                    .selected_type()
                    .is_some_and(|run| run.content == job.source);
            if !same {
                self.status =
                    "The text destination changed. Paste again at the intended caret.".into();
                continue;
            }
            match result {
                Ok(paste) => {
                    let start = self.type_sel_range().0;
                    self.type_insert(&paste.text);
                    if let Some(rich) = paste.rich {
                        if let Some(run) = self.live_type_mut() {
                            rich.apply_styles(run, start);
                        }
                        if let Some(edit) = &mut self.type_edit {
                            edit.pending_style = None;
                        }
                        self.reshape_live_type();
                    }
                }
                Err(error) => self.status = error,
            }
        }
        if !self.type_paste_jobs.is_empty() {
            ctx.request_repaint_after(Duration::from_millis(20));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pointer_reposition_and_drag_drop_pending_features() {
        let mut studio = Studio::new();
        studio.active_layer = Some(1);
        studio.place_text(Pt::ZERO);
        studio.type_insert("abcd");
        studio.patch_feature(*b"smcp", 1);
        assert!(studio.type_edit.as_ref().unwrap().pending_style.is_some());
        let hit = studio.selection[0];
        let point = crate::text::caret_pt(&studio.selected_type().unwrap(), 1);
        studio.begin_type_edit(hit, point);
        assert!(studio.type_edit.as_ref().unwrap().pending_style.is_none());
        studio.type_insert("x");
        assert_eq!(
            crate::text::feature_value(&studio.selected_type().unwrap(), 1, *b"smcp"),
            0
        );
        studio.patch_feature(*b"smcp", 1);
        studio.type_edit.as_mut().unwrap().pointer_caret(4, true);
        assert!(studio.type_edit.as_ref().unwrap().pending_style.is_none());
    }
    #[test]
    fn delayed_paste_does_not_follow_moved_caret_or_new_tab() {
        let mut studio = Studio::new();
        studio.active_layer = Some(1);
        studio.place_text(Pt::ZERO);
        studio.type_insert("ab");
        let original = studio.selected_type().unwrap().content;
        let (sender, receiver) = mpsc::channel();
        let edit = studio.type_edit.clone().unwrap();
        studio.type_paste_jobs.push(TypePasteJob {
            receiver,
            owner: studio.swap_id.clone(),
            edit,
            source: original.clone(),
        });
        studio.type_edit.as_mut().unwrap().pointer_caret(0, false);
        sender
            .send(Ok(Paste {
                text: "wrong".into(),
                rich: None,
            }))
            .unwrap();
        studio.poll_type_clipboard_jobs(&egui::Context::default());
        assert_eq!(studio.selected_type().unwrap().content, original);
        assert!(studio.status.contains("destination changed"));
    }
}
