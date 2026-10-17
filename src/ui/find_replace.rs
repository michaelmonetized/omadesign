//! Native literal Find and Replace keeps captured story owners and ordinary Type commands.
use crate::{
    app::{
        Studio,
        type_commands::{
            TextScope,
            search::{Query, Target, TextMatch},
        },
    },
    geom::{Geom, Pt},
};
use eframe::egui::{self, Id, Response, Ui};

pub(super) const ID: &str = "type-find-replace";
#[derive(Clone)]
struct Draft {
    instance: Id,
    owner: String,
    scope: TextScope,
    selection: Result<Target, String>,
    document: Result<Target, String>,
    query: Query,
    replacement: String,
    cached: Option<Query>,
    matches: std::sync::Arc<Vec<TextMatch>>,
    selected: Option<usize>,
    focus: bool,
    error: Option<String>,
    notice: Option<String>,
}
#[derive(Clone)]
struct Highlight {
    document: String,
    generation: u64,
    selection: Vec<(usize, u64)>,
    found: TextMatch,
}

fn control(ui: &mut Ui, id: Id, name: &str, widget: impl FnOnce(&mut Ui) -> Response) -> Response {
    let r = super::fields::native_scope(ui, id, widget).inner;
    ui.ctx()
        .accesskit_node_builder(r.id, |node| node.set_label(name));
    super::welcome::focus_control(ui, &r);
    r
}
/// Read the actual search modal lifetime.
/// Args: UI context. Returns whether the captured dialog exists.
pub(super) fn is_open(ctx: &egui::Context) -> bool {
    ctx.data(|d| d.get_temp::<Draft>(Id::new(ID)).is_some())
}
fn target(d: &Draft) -> Result<&Target, &String> {
    if d.scope == TextScope::Selection {
        d.selection.as_ref()
    } else {
        d.document.as_ref()
    }
}
fn refresh(d: &mut Draft, s: &Studio) {
    d.selection = s.capture_text_search(TextScope::Selection);
    d.document = s.capture_text_search(TextScope::Document);
    d.cached = None;
    d.selected = None;
}
fn remember(ctx: &egui::Context, query: &Query) {
    ctx.data_mut(|d| d.insert_temp(Id::new((ID, "query")), query.clone()));
}
fn highlight(ctx: &egui::Context, s: &Studio, found: TextMatch) {
    ctx.data_mut(|d| {
        d.insert_temp(
            Id::new((ID, "highlight")),
            Highlight {
                document: s.swap_id.clone(),
                generation: s.canvas_gen,
                selection: s.selection.clone(),
                found,
            },
        )
    });
}
fn open(ctx: &egui::Context, s: &Studio) {
    let d = Draft {
        instance: super::keyboard_controls::draft_id(ctx, ID),
        owner: s.swap_id.clone(),
        scope: TextScope::Document,
        selection: s.capture_text_search(TextScope::Selection),
        document: s.capture_text_search(TextScope::Document),
        query: ctx
            .data(|d| d.get_temp::<Query>(Id::new((ID, "query"))))
            .unwrap_or_default(),
        replacement: ctx
            .data(|d| d.get_temp::<String>(Id::new((ID, "replacement"))))
            .unwrap_or_default(),
        cached: None,
        matches: Default::default(),
        selected: None,
        focus: true,
        error: None,
        notice: None,
    };
    ctx.data_mut(|data| {
        data.remove::<Highlight>(Id::new((ID, "highlight")));
        data.insert_temp(Id::new(ID), d);
    });
}
fn next_story(ctx: &egui::Context, s: &mut Studio) {
    let Some(query) = ctx.data(|d| d.get_temp::<Query>(Id::new((ID, "query")))) else {
        open(ctx, s);
        return;
    };
    match s.find_next_text_story(&query) {
        Ok(Some(found)) => highlight(ctx, s, found),
        Ok(None) => {
            ctx.data_mut(|d| d.remove::<Highlight>(Id::new((ID, "highlight"))));
        }
        Err(error) => s.status = error,
    }
}
/// Add the reference Find commands to the existing Edit menu.
/// Args: native menu UI and editor. Returns no value; opening makes no document command.
pub(super) fn menu(ui: &mut Ui, s: &mut Studio) {
    let available = s.capture_text_search(TextScope::Document).is_ok();
    let owner = Id::new((ID, "menu", &s.swap_id));
    if control(ui, owner.with("open"), "Find and Replace…", |ui| {
        ui.add_enabled(
            available,
            egui::Button::new("Find and Replace…").shortcut_text("Ctrl+F"),
        )
    })
    .clicked()
    {
        open(ui.ctx(), s);
        ui.close();
    }
    let has_query = ui
        .ctx()
        .data(|d| d.get_temp::<Query>(Id::new((ID, "query"))))
        .is_some_and(|q| !q.find.is_empty());
    if control(ui, owner.with("next"), "Find Next", |ui| {
        ui.add_enabled(
            available && has_query,
            egui::Button::new("Find Next").shortcut_text("F3"),
        )
    })
    .clicked()
    {
        next_story(ui.ctx(), s);
        ui.close();
    }
}
/// Handle Find keys only when the actual artwork canvas owns editing.
/// Args: context and editor after the existing modal guards. Returns no value; fields keep their native keys.
pub(super) fn shortcuts(ctx: &egui::Context, s: &mut Studio) {
    if !ctx.input(|i| i.focused)
        || egui::Popup::is_any_open(ctx)
        || ctx.memory(|m| {
            m.top_modal_layer().is_some()
                || m.focused().is_some_and(|id| id != Id::new("studio-canvas"))
        })
    {
        return;
    }
    let open_key = ctx.input_mut(|i| {
        i.consume_key(egui::Modifiers::COMMAND, egui::Key::F)
            || i.consume_key(egui::Modifiers::CTRL, egui::Key::F)
    });
    if open_key && s.capture_text_search(TextScope::Document).is_ok() {
        open(ctx, s);
        return;
    }
    if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::F3)) {
        next_story(ctx, s);
    }
}

/// Lay out real action buttons in rows sized from their current font.
/// Args: UI, stable owner and current availability. Returns the clicked action without squeezing native labels.
fn footer(ui: &mut Ui, owner: Id, has_matches: bool, selected: bool) -> Option<&'static str> {
    let items = [
        ("next", "Find next", has_matches),
        ("one", "Replace", has_matches && selected),
        ("all", "Replace all", has_matches),
        ("select", "Select match", has_matches && selected),
        ("close", "Close", true),
    ];
    let widths: Vec<_> = items
        .iter()
        .map(|(_, name, _)| {
            let text = egui::WidgetText::from(*name).into_galley(
                ui,
                Some(egui::TextWrapMode::Extend),
                f32::INFINITY,
                egui::TextStyle::Button,
            );
            (text.size().x + ui.spacing().button_padding.x * 2.)
                .max(ui.spacing().interact_size.x)
                .ceil()
                + 1.
        })
        .collect();
    let mut start = 0;
    let mut action = None;
    while start < items.len() {
        let room = ui.available_width();
        let mut end = start;
        let mut used = 0.;
        while end < items.len() {
            let next = widths[end]
                + if end > start {
                    ui.spacing().item_spacing.x
                } else {
                    0.
                };
            if end > start && used + next > room {
                break;
            }
            used += next;
            end += 1;
        }
        ui.horizontal(|ui| {
            for &(key, name, enabled) in &items[start..end] {
                let label = if key == "close" {
                    "Close Find and Replace"
                } else {
                    name
                };
                if control(ui, owner.with(key), label, |ui| {
                    ui.add_enabled(
                        enabled,
                        egui::Button::new(name).wrap_mode(egui::TextWrapMode::Extend),
                    )
                })
                .clicked()
                {
                    action = Some(key);
                }
            }
        });
        start = end;
    }
    action
}

/// Draw the actual search controls and apply only accepted captured actions.
/// Args: context and editor. Returns no value; cancellation and stale results never edit a story.
pub(super) fn show(ctx: &egui::Context, s: &mut Studio) {
    let Some(mut d) = ctx.data(|data| data.get_temp::<Draft>(Id::new(ID))) else {
        return;
    };
    let owner = Id::new((ID, &d.owner, d.instance));
    let mut action = None;
    let mut close = false;
    let modal = egui::Modal::new(Id::new(ID))
        .backdrop_color(super::theme::bg_window().gamma_multiply(0.55))
        .show(ctx, |ui| {
            ui.set_width((ctx.content_rect().width() - 48.).clamp(160., 430.));
            ui.set_max_height((ctx.content_rect().height() - 100.).max(100.));
            ui.heading("Find and Replace");
            egui::ScrollArea::vertical()
                .id_salt(owner.with("scroll"))
                .animated(false)
                .max_height((ctx.content_rect().height() - 130.).max(80.))
                .min_scrolled_height(0.)
                .show(ui, |ui| {
                    let current = target(&d).is_ok_and(|t| s.text_search_current(t));
                    let owner_error = (!current).then(|| {
                        target(&d).err().cloned().unwrap_or_else(|| {
                            "The text destination changed. Close and reopen Find and Replace".into()
                        })
                    });
                    let mut changed = false;
                    ui.add_enabled_ui(current, |ui| {
                        let label = ui.label("Find");
                        let r = control(ui, owner.with("find"), "Find text", |ui| {
                            ui.add(
                                egui::TextEdit::multiline(&mut d.query.find)
                                    .id(owner.with("find-editor"))
                                    .desired_rows(2)
                                    .desired_width(f32::INFINITY)
                                    .lock_focus(false),
                            )
                            .labelled_by(label.id)
                        });
                        changed |= r.changed();
                        if d.focus {
                            r.request_focus();
                            d.focus = false;
                        }
                        if let Some(error) = d.error.as_ref().or(owner_error.as_ref()) {
                            super::welcome::control_error(ui, &[&r], error);
                        }
                        let label = ui.label("Replace with");
                        control(ui, owner.with("replace"), "Replacement text", |ui| {
                            ui.add(
                                egui::TextEdit::multiline(&mut d.replacement)
                                    .id(owner.with("replace-editor"))
                                    .desired_rows(2)
                                    .desired_width(f32::INFINITY)
                                    .lock_focus(false),
                            )
                            .labelled_by(label.id)
                        });
                        ui.horizontal_wrapped(|ui| {
                            changed |= control(ui, owner.with("case"), "Match case", |ui| {
                                ui.checkbox(&mut d.query.match_case, "Match case")
                            })
                            .changed();
                            changed |= control(ui, owner.with("word"), "Whole words", |ui| {
                                ui.checkbox(&mut d.query.whole_word, "Whole words")
                            })
                            .changed();
                        });
                    });
                    ui.horizontal_wrapped(|ui| {
                        for (scope, name) in [
                            (TextScope::Selection, "Selected text"),
                            (TextScope::Document, "Document"),
                        ] {
                            let enabled = if scope == TextScope::Selection {
                                d.selection.is_ok()
                            } else {
                                d.document.is_ok()
                            };
                            if control(ui, owner.with(name), name, |ui| {
                                ui.add_enabled(
                                    enabled,
                                    egui::RadioButton::new(d.scope == scope, name),
                                )
                            })
                            .clicked()
                            {
                                d.scope = scope;
                                changed = true;
                            }
                        }
                    });
                    let current = target(&d).is_ok_and(|t| s.text_search_current(t));
                    if !current {
                        d.matches = Default::default();
                        d.selected = None;
                        d.cached = None;
                    }
                    if changed {
                        d.cached = None;
                        d.selected = None;
                        d.error = None;
                        d.notice = None;
                    }
                    if current && d.cached.as_ref() != Some(&d.query) {
                        d.matches = std::sync::Arc::new(if d.query.find.is_empty() {
                            vec![]
                        } else {
                            match s.text_search_matches(target(&d).unwrap(), &d.query) {
                                Ok(matches) => matches,
                                Err(error) => {
                                    d.error = Some(error);
                                    vec![]
                                }
                            }
                        });
                        d.cached = Some(d.query.clone());
                    }
                    let status = d.selected.filter(|&i| i < d.matches.len()).map_or_else(
                        || format!("{} matches", d.matches.len()),
                        |i| format!("{} of {}", i + 1, d.matches.len()),
                    );
                    ui.label(status);
                    if let Some(found) = d.selected.and_then(|i| d.matches.get(i))
                        && let Some(shape) = s.doc.find_shape(found.hit.0, found.hit.1)
                    {
                        ui.label(&shape.name);
                        if let Geom::Text(run) = &shape.geom {
                            let begin = found.range.start.saturating_sub(18);
                            let end = found.range.end.saturating_add(30);
                            let excerpt: String =
                                run.content.chars().skip(begin).take(end - begin).collect();
                            ui.label(excerpt);
                        }
                    }
                    if let Some(notice) = &d.notice {
                        ui.label(notice);
                    }
                    if let Some(clicked) = footer(
                        ui,
                        owner,
                        current && !d.matches.is_empty(),
                        d.selected.is_some(),
                    ) {
                        if clicked == "close" {
                            close = true;
                        } else {
                            action = Some(clicked);
                        }
                    }
                });
        });
    close |= modal.should_close();
    if !close && let Some(action) = action {
        match action {
            "next" => {
                d.selected = Some(d.selected.map_or(0, |i| (i + 1) % d.matches.len()));
                remember(ctx, &d.query);
            }
            "select" => {
                let found = d.selected.and_then(|i| d.matches.get(i)).cloned();
                if let Some(found) = found {
                    match s.select_text_search_match(target(&d).unwrap(), &d.query, &found) {
                        Ok(()) => {
                            remember(ctx, &d.query);
                            highlight(ctx, s, found);
                            close = true;
                        }
                        Err(error) => d.error = Some(error),
                    }
                }
            }
            "one" | "all" => {
                let one = (action == "one")
                    .then(|| d.selected.and_then(|i| d.matches.get(i)).cloned())
                    .flatten();
                match s.replace_text_search(
                    target(&d).unwrap(),
                    &d.query,
                    &d.replacement,
                    one.as_ref(),
                ) {
                    Ok(report) => {
                        remember(ctx, &d.query);
                        ctx.data_mut(|data| {
                            data.insert_temp(Id::new((ID, "replacement")), d.replacement.clone())
                        });
                        let noun = if report.matches == 1 {
                            "match"
                        } else {
                            "matches"
                        };
                        d.notice = Some(if report.stories == 0 {
                            format!("{} {noun}; no text changes", report.matches)
                        } else {
                            format!("Replaced {} {noun}", report.matches)
                        });
                        if report.stories > 0 {
                            refresh(&mut d, s);
                        } else {
                            d.cached = None;
                            d.selected = None;
                        }
                    }
                    Err(error) => d.error = Some(error),
                }
            }
            _ => (),
        }
    }
    if close {
        ctx.data_mut(|data| {
            data.remove::<Draft>(Id::new(ID));
            data.insert_temp(Id::new((ID, "return-focus")), d.owner);
        });
        ctx.request_discard("Close the search modal before restoring canvas input");
        ctx.request_repaint();
    } else {
        ctx.data_mut(|data| data.insert_temp(Id::new(ID), d));
    }
}

/// Paint a found range using the existing native text selection geometry.
/// Args: actual canvas painter/rect and editor. Returns no value; the highlight is UI-only and retires with its owner.
pub(super) fn paint(p: &egui::Painter, rect: egui::Rect, s: &Studio) {
    let draft = p.ctx().data(|d| d.get_temp::<Draft>(Id::new(ID)));
    let found = if let Some(d) = &draft {
        if !target(d).is_ok_and(|t| s.text_search_current(t)) {
            return;
        }
        d.selected.and_then(|i| d.matches.get(i)).cloned()
    } else {
        p.ctx()
            .data(|data| data.get_temp::<Highlight>(Id::new((ID, "highlight"))))
            .filter(|h| {
                h.document == s.swap_id
                    && h.generation == s.canvas_gen
                    && h.selection == s.selection
                    && s.type_edit.is_none()
            })
            .map(|h| h.found)
    };
    let Some(found) = found else {
        return;
    };
    for (li, layer) in s.doc.layers.iter().enumerate() {
        for shape in layer.kind.shapes().unwrap_or(&[]) {
            if s.story_head((li, shape.id)) != found.hit {
                continue;
            }
            let Geom::Text(run) = &shape.geom else {
                continue;
            };
            let source = run;
            let quads =
                crate::text_geometry::selection_quads(&source, found.range.start, found.range.end)
                    .unwrap_or_else(|| {
                        crate::text::selection_rects(&source, found.range.start, found.range.end)
                            .into_iter()
                            .map(|(a, b)| [a, Pt::new(b.x, a.y), b, Pt::new(a.x, b.y)])
                            .collect()
                    });
            let parent =
                crate::compositor::shape_parent_transform_with(&s.doc, li, shape.id, |id| {
                    s.live_pose(id)
                });
            let placement =
                crate::compositor::frame_transform(shape, parent, s.live_pose(shape.id));
            for quad in quads {
                let points = quad
                    .into_iter()
                    .map(|point| {
                        let mut point = tiny_skia::Point::from_xy(point.x, point.y);
                        placement.map_point(&mut point);
                        let point = s.view.world_to_window(
                            Pt::new(rect.min.x, rect.min.y),
                            Pt::new(point.x, point.y),
                        );
                        egui::pos2(point.x, point.y)
                    })
                    .collect();
                p.add(egui::Shape::convex_polygon(
                    points,
                    super::theme::select_fill(),
                    egui::Stroke::new(1., super::theme::accent()),
                ));
            }
        }
    }
}

#[cfg(test)]
#[path = "find_replace_tests.rs"]
mod tests;
