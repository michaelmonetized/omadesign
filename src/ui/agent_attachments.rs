use crate::{
    agent::{
        Workspace,
        attachments::{self, Attachment, Input},
    },
    app::Studio,
};
use eframe::egui::{
    self, Id, RichText,
    text::{CCursor, CCursorRange},
};
pub(super) fn field_id() -> Id {
    Id::new("agent-prompt-input")
}
fn range(ctx: &egui::Context, id: Id, len: usize) -> std::ops::Range<usize> {
    egui::TextEdit::load_state(ctx, id)
        .and_then(|s| s.cursor.char_range())
        .map(|r| {
            let [a, b] = r.sorted_cursors();
            a.index.0..b.index.0
        })
        .unwrap_or(len..len)
}
fn set_cursor(ctx: &egui::Context, id: Id, at: usize) {
    let mut state = egui::TextEdit::load_state(ctx, id).unwrap_or_default();
    state
        .cursor
        .set_char_range(Some(CCursorRange::one(CCursor::new(at))));
    state.store(ctx, id);
}
fn byte(text: &str, index: usize) -> usize {
    text.char_indices()
        .nth(index)
        .map(|(i, _)| i)
        .unwrap_or(text.len())
}
fn erase(request: &mut String, selection: std::ops::Range<usize>) {
    let start = byte(request, selection.start);
    let end = byte(request, selection.end);
    request.replace_range(start..end, "");
}
pub(super) fn chips(ui: &mut egui::Ui, items: &[Attachment], editable: bool) -> Option<String> {
    let mut remove = None;
    let width = ui.available_width().max(80.);
    ui.vertical(|ui| {
        for a in items {
            egui::Frame::new()
                .fill(super::theme::bg_widget())
                .corner_radius(4)
                .inner_margin(4)
                .show(ui, |ui| {
                    ui.set_width((width - 8.).max(1.));
                    ui.horizontal(|ui| {
                        if let Some(im) = &a.thumbnail {
                            let key = Id::new(("attachment-thumb", &a.id));
                            let texture = ui
                                .ctx()
                                .data_mut(|d| d.get_temp::<egui::TextureHandle>(key))
                                .unwrap_or_else(|| {
                                    let texture = ui.ctx().load_texture(
                                        format!("attachment-{}", a.id),
                                        egui::ColorImage::from_rgba_unmultiplied(
                                            [im.w as usize, im.h as usize],
                                            &im.data,
                                        ),
                                        egui::TextureOptions::LINEAR,
                                    );
                                    ui.ctx().data_mut(|d| d.insert_temp(key, texture.clone()));
                                    texture
                                });
                            ui.add(egui::Image::new(&texture).max_size(egui::vec2(26., 26.)));
                        }
                        let label = if a.available() {
                            format!("📎 {} · {:.1} KB", a.name, a.size as f64 / 1024.)
                        } else {
                            format!("Unavailable · 📎 {}", a.name)
                        };
                        let label_width =
                            (ui.available_width() - if editable { 28. } else { 0. }).max(1.);
                        ui.add_sized(
                            [label_width, 22.],
                            egui::Label::new(RichText::new(label).small()).truncate(),
                        )
                        .on_hover_text(a.tooltip());
                        if editable
                            && ui
                                .small_button("×")
                                .on_hover_text(format!("Remove {}", a.name))
                                .clicked()
                        {
                            remove = Some(a.id.clone());
                        }
                    });
                });
        }
    });
    remove
}
pub(super) fn composer(
    ui: &mut egui::Ui,
    studio: &Studio,
    agent: &mut Workspace,
) -> egui::Response {
    let id = field_id();
    if let Some(insert) = agent.poll_attachments() {
        let at = agent.insert_attachment_result(insert);
        set_cursor(ui.ctx(), id, at);
    }
    let panel = ui.max_rect();
    let drop = ui.input(|i| i.pointer.hover_pos().is_some_and(|p| panel.contains(p)));
    if drop && ui.input(|i| !i.raw.hovered_files.is_empty()) {
        ui.colored_label(
            super::theme::accent(),
            "Drop files to attach to this prompt",
        );
    }
    if drop {
        let files = ui.input_mut(|i| std::mem::take(&mut i.raw.dropped_files));
        let paths = files
            .into_iter()
            .map(|f| f.path().to_owned())
            .collect::<Vec<_>>();
        if !paths.is_empty() {
            let at = range(ui.ctx(), id, agent.request.chars().count()).start;
            agent.attach(studio, Input::Files(paths), at..at, ui.ctx());
        }
    }
    if ui.memory(|m| m.has_focus(id)) {
        let mut paste = None;
        let mut paste_key = false;
        let mut delete = None;
        ui.input_mut(|i| {
            i.events.retain(|event| match event {
                egui::Event::Paste(text) => {
                    paste = Some(text.clone());
                    false
                }
                egui::Event::Key {
                    key,
                    pressed: true,
                    modifiers,
                    ..
                } if (modifiers.command && *key == egui::Key::V)
                    || (modifiers.shift && *key == egui::Key::Insert) =>
                {
                    paste_key = true;
                    false
                }
                egui::Event::Key {
                    key, pressed: true, ..
                } if matches!(key, egui::Key::Backspace | egui::Key::Delete) => {
                    delete = Some(*key);
                    true
                }
                _ => true,
            })
        });
        let selection = range(ui.ctx(), id, agent.request.chars().count());
        if paste_key || paste.is_some() {
            let selection = atomic_range(&agent.request, &agent.attachments, selection, None);
            agent.attach(studio, Input::Clipboard(paste), selection, ui.ctx());
        } else if let Some(key) = delete {
            let expanded = atomic_range(
                &agent.request,
                &agent.attachments,
                selection.clone(),
                Some(key),
            );
            if expanded != selection {
                ui.input_mut(|i| {
                    i.events
                        .retain(|e| !matches!(e,egui::Event::Key{key:k,pressed:true,..} if *k==key))
                });
                erase(&mut agent.request, expanded.clone());
                attachments::reconcile(&agent.request, &mut agent.attachments);
                set_cursor(ui.ctx(), id, expanded.start);
            }
        }
    }
    egui::ScrollArea::vertical()
        .id_salt("prompt-attachment-chips")
        .max_height(116.)
        .show(ui, |ui| {
            if let Some(id) = chips(ui, &agent.attachments, true) {
                attachments::remove(&mut agent.request, &mut agent.attachments, &id);
            }
            if !agent.attachment_jobs.is_empty() {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.small("Attaching…");
                });
            }
        });
    let mut layouter = |ui: &egui::Ui, buffer: &dyn egui::TextBuffer, width: f32| {
        let text = buffer.as_str();
        let mut job = egui::text::LayoutJob::default();
        job.wrap.max_width = width;
        job.wrap.break_anywhere = true;
        let normal = egui::TextFormat {
            font_id: egui::TextStyle::Body.resolve(ui.style()),
            color: super::theme::fg(),
            ..Default::default()
        };
        let mut ranges = agent
            .attachments
            .iter()
            .flat_map(|a| {
                text.match_indices(&a.token())
                    .map(|(start, t)| (start, start + t.len()))
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        ranges.sort_unstable();
        let mut at = 0;
        for (start, end) in ranges {
            if start < at {
                continue;
            }
            job.append(&text[at..start], 0., normal.clone());
            let mut style = normal.clone();
            style.color = super::theme::accent();
            style.background = super::theme::bg_widget();
            job.append(&text[start..end], 0., style);
            at = end;
        }
        job.append(&text[at..], 0., normal);
        ui.fonts_mut(|f| f.layout_job(job))
    };
    let field = egui::ScrollArea::vertical()
        .id_salt("agent-prompt-scroll")
        .max_height(90.)
        .auto_shrink([false, true])
        .show(ui, |ui| {
            egui::TextEdit::multiline(&mut agent.request)
                .id(id)
                .desired_rows(3)
                .desired_width(f32::INFINITY)
                .layouter(&mut layouter)
                .hint_text("Describe a design · paste images, files or text…")
                .show(ui)
                .response
                .response
        })
        .inner;
    attachments::reconcile(&agent.request, &mut agent.attachments);
    field
}
fn atomic_range(
    text: &str,
    items: &[Attachment],
    mut range: std::ops::Range<usize>,
    key: Option<egui::Key>,
) -> std::ops::Range<usize> {
    for a in items {
        for (offset, token) in text.match_indices(&a.token()) {
            let start = text[..offset].chars().count();
            let end = start + token.chars().count();
            let intersects = if range.is_empty() {
                match key {
                    Some(egui::Key::Backspace) => range.start > start && range.start <= end,
                    Some(egui::Key::Delete) => range.start >= start && range.start < end,
                    _ => range.start > start && range.start < end,
                }
            } else {
                range.start < end && range.end > start
            };
            if intersects {
                range.start = range.start.min(start);
                range.end = range.end.max(end);
            }
        }
    }
    range
}

#[cfg(test)]
mod tests {
    use super::*;
    fn attachment() -> Attachment {
        Attachment {
            id: "a1".into(),
            name: "logo.svg".into(),
            mime: "image/svg+xml".into(),
            kind: attachments::Kind::Svg,
            size: 20,
            source: "/tmp/missing-logo.svg".into(),
            dimensions: None,
            delivery: String::new(),
            preview: None,
            thumbnail: None,
        }
    }
    #[test]
    fn attachment_tokens_delete_atomically_at_unicode_boundaries_and_selections() {
        let a = attachment();
        let token = a.token();
        let text = format!("你好 {token} end");
        let start = 3;
        let end = start + token.chars().count();
        assert_eq!(
            atomic_range(&text, &[a.clone()], end..end, Some(egui::Key::Backspace)),
            start..end
        );
        assert_eq!(
            atomic_range(&text, &[a.clone()], start..start, Some(egui::Key::Delete)),
            start..end
        );
        assert_eq!(
            atomic_range(
                &text,
                &[a.clone()],
                start + 2..start + 2,
                Some(egui::Key::Backspace)
            ),
            start..end
        );
        assert_eq!(
            atomic_range(&text, &[a.clone()], 0..start + 2, Some(egui::Key::Delete)),
            0..end
        );
        let mut value = text.clone();
        erase(&mut value, start..end);
        let mut items = vec![a];
        attachments::reconcile(&value, &mut items);
        assert!(items.is_empty());
        assert_eq!(value, "你好  end");
    }
    #[test]
    fn attachment_backspace_through_native_textedit_removes_token_and_chip() {
        let ctx = egui::Context::default();
        crate::ui::theme::apply(&ctx);
        let studio = Studio::new();
        let mut workspace = Workspace::default();
        let a = attachment();
        workspace.request = format!("Hello {}", a.token());
        workspace.attachments = vec![a];
        let mut render = |events: Vec<egui::Event>, focus: bool| {
            let mut result = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(640., 400.),
                    )),
                    events,
                    ..Default::default()
                },
                |ui| {
                    let field = composer(ui, &studio, &mut workspace);
                    if focus {
                        field.request_focus();
                    }
                },
            );
            result.textures_delta.clear();
        };
        render(vec![], true);
        render(vec![], true);
        set_cursor(&ctx, field_id(), usize::MAX);
        // TextEdit clamps the requested end position before the next event.
        render(vec![], true);
        render(
            vec![egui::Event::Key {
                key: egui::Key::Backspace,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
            false,
        );
        assert_eq!(workspace.request, "Hello ");
        assert!(workspace.attachments.is_empty());
    }
}

#[cfg(test)]
mod layout_tests {
    use super::*;
    #[test]
    fn attachment_panel_keeps_full_native_canvas_bounds_finite() {
        let ctx = egui::Context::default();
        crate::ui::theme::apply(&ctx);
        let mut studio = Studio::new();
        studio.show_welcome = false;
        studio.doc = crate::document::Document::new("Attachment UI", 960., 640., 96.);
        studio.agent.loaded = true;
        studio.agent.visible = true;
        studio.agent.focus_prompt = true;
        studio.agent.settings.profile.name = "Attachment QA provider".into();
        for _ in 0..3 {
            let mut out = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1600., 900.),
                    )),
                    ..Default::default()
                },
                |ui| crate::ui::run(ui, &mut studio),
            );
            out.textures_delta.clear();
            let rect = studio.canvas_rect.unwrap();
            assert!(rect.width() <= 1600. && rect.height() <= 900., "{rect:?}");
        }
    }
}

#[cfg(test)]
mod drop_tests {
    use super::*;
    #[derive(Debug)]
    struct File(std::path::PathBuf);
    impl egui::DroppedFile for File {
        fn path(&self) -> &std::path::Path {
            &self.0
        }
        fn bytes(&self) -> Result<Vec<u8>, String> {
            std::fs::read(&self.0).map_err(|e| e.to_string())
        }
    }
    #[test]
    fn attachment_panel_consumes_file_drop_before_canvas_import() {
        let directory =
            std::env::temp_dir().join(format!("omadesign-drop-{}", crate::project::new_swap_id()));
        std::fs::create_dir_all(&directory).unwrap();
        let file = directory.join("brand.pdf");
        std::fs::write(&file, b"%PDF fixture").unwrap();
        let ctx = egui::Context::default();
        crate::ui::theme::apply(&ctx);
        let mut studio = Studio::new();
        studio.show_welcome = false;
        studio.agent.loaded = true;
        studio.agent.visible = true;
        studio.agent.settings.directory = directory.clone();
        let original = crate::project::encode(&studio.doc).unwrap();
        let mut frame = |drop: bool| {
            let mut input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1600., 900.),
                )),
                events: vec![egui::Event::PointerMoved(egui::pos2(1500., 500.))],
                ..Default::default()
            };
            if drop {
                input
                    .dropped_files
                    .push(std::sync::Arc::new(File(file.clone())));
            }
            let mut output = ctx.run_ui(input, |ui| crate::ui::run(ui, &mut studio));
            output.textures_delta.clear();
        };
        frame(false);
        frame(true);
        assert!(ctx.input(|i| i.raw.dropped_files.is_empty()));
        assert_eq!(studio.agent.attachment_jobs.len(), 1);
        assert_eq!(crate::project::encode(&studio.doc).unwrap(), original);
        let start = std::time::Instant::now();
        loop {
            if let Some(insert) = studio.agent.poll_attachments() {
                studio.agent.insert_attachment_result(insert);
                break;
            }
            assert!(start.elapsed() < std::time::Duration::from_secs(5));
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert_eq!(studio.agent.attachments[0].source, file);
        assert!(studio.agent.request.contains("brand.pdf"));
        std::fs::remove_dir_all(directory).unwrap();
    }
}
