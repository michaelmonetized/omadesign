//! Find Font uses captured stories, real family/style choices and ordinary Type history.
use crate::{
    app::{
        Studio,
        type_commands::{
            TextScope,
            fonts::{Target, Usage},
        },
    },
    text::{font_catalog::Catalog, font_inventory::Status},
};
use eframe::egui::{self, Id, Response, Ui};
use std::sync::Arc;
pub(super) const ID: &str = "type-find-font";
const REFRESH: &str = "type-find-font-refresh";
#[derive(Clone)]
struct Draft {
    instance: Id,
    document: String,
    owner: Result<Target, String>,
    rows: Arc<Vec<Usage>>,
    catalog: Catalog,
    families: Arc<Vec<String>>,
    selected: Option<String>,
    family: String,
    style: Option<String>,
    filter: String,
    focus: bool,
    error: Option<String>,
    notice: Option<String>,
    refreshing: bool,
    recovery: super::find_font_recovery::State,
}
pub(super) fn control(
    ui: &mut Ui,
    id: Id,
    name: &str,
    widget: impl FnOnce(&mut Ui) -> Response,
) -> Response {
    let r = super::fields::native_scope(ui, id, widget).inner;
    ui.ctx()
        .accesskit_node_builder(r.id, |node| node.set_label(name));
    super::welcome::focus_control(ui, &r);
    r
}
fn refresh(d: &mut Draft, s: &Studio) {
    d.owner = s.capture_font_dialog();
    match s.font_usage() {
        Ok(rows) => d.rows = Arc::new(rows),
        Err(error) => {
            d.error = Some(error);
            d.rows = Default::default();
        }
    }
    d.catalog = s.font_catalog();
    d.families = Arc::new(d.catalog.families());
    if !d.rows.iter().any(|r| Some(&r.key) == d.selected.as_ref()) {
        d.selected = d.rows.first().map(|r| r.key.clone());
    }
    if !d.families.contains(&d.family) {
        d.family = d
            .rows
            .iter()
            .find(|r| Some(&r.key) == d.selected.as_ref())
            .map(|r| r.info.family.clone())
            .filter(|f| d.families.contains(f))
            .or_else(|| d.families.first().cloned())
            .unwrap_or_default();
        d.style = None;
    }
}
fn open(ctx: &egui::Context, s: &Studio) {
    let catalog = s.font_catalog();
    let mut d = Draft {
        instance: super::keyboard_controls::draft_id(ctx, ID),
        document: s.swap_id.clone(),
        owner: s.capture_font_dialog(),
        rows: Default::default(),
        families: Default::default(),
        catalog,
        selected: None,
        family: String::new(),
        style: None,
        filter: String::new(),
        focus: true,
        error: None,
        notice: None,
        refreshing: false,
        recovery: Default::default(),
    };
    refresh(&mut d, s);
    ctx.data_mut(|data| data.insert_temp(Id::new(ID), d));
}
/// Read the actual Find Font modal lifetime.
/// Args: context. Returns whether the captured dialog is open.
pub(super) fn instance_current(ctx: &egui::Context, instance: Id) -> bool {
    ctx.data(|d| {
        d.get_temp::<Draft>(Id::new(ID))
            .is_some_and(|d| d.instance == instance)
    })
}
pub(super) fn is_open(ctx: &egui::Context) -> bool {
    ctx.data(|d| d.get_temp::<Draft>(Id::new(ID)).is_some())
}
/// Add Find Font to the existing Type menu.
/// Args: native menu UI and editor. Returns no value; opening does not edit a document.
pub(super) fn menu(ui: &mut Ui, s: &Studio) {
    if control(ui, Id::new((ID, "open", &s.swap_id)), "Find Font…", |ui| {
        ui.add_enabled(
            s.capture_font_dialog().is_ok(),
            egui::Button::new("Find Font…"),
        )
    })
    .clicked()
    {
        open(ui.ctx(), s);
        ui.close();
    }
}
fn note(row: &Usage) -> String {
    let status = match row.info.status {
        Status::Missing => " — missing".to_owned(),
        Status::Substitute => format!(
            " — substituted by {}",
            row.info.resolved.as_deref().unwrap_or("fallback")
        ),
        Status::Exact => String::new(),
    };
    let glyphs = if row.info.missing_glyphs > 0 {
        format!("; {} missing characters", row.info.missing_glyphs)
    } else {
        String::new()
    };
    format!(
        "{} {}{status}{glyphs} ({} runs, {} objects)",
        row.info.family,
        row.info.style,
        row.runs,
        row.stories.len()
    )
}
fn actions(
    ui: &mut Ui,
    owner: Id,
    can_find: bool,
    can_change: bool,
    can_all: bool,
) -> Option<&'static str> {
    let mut action = None;
    ui.horizontal_wrapped(|ui| {
        for (key, label, enabled) in [
            ("find", "Find", can_find),
            ("change", "Change", can_change),
            ("all", "Change All", can_all),
            ("refresh", "Refresh", true),
        ] {
            if control(ui, owner.with(key), label, |ui| {
                ui.add_enabled(enabled, egui::Button::new(label))
            })
            .clicked()
            {
                action = Some(key);
            }
        }
    });
    action
}
/// Draw Find Font and accept only actions belonging to the captured document and selection.
/// Args: context and editor. Returns no value; dismissal and stale targets do not edit text.
pub(super) fn show(ctx: &egui::Context, s: &mut Studio) {
    let Some(mut d) = ctx.data(|data| data.get_temp::<Draft>(Id::new(ID))) else {
        return;
    };
    let mut current = d.owner.as_ref().is_ok_and(|t| s.font_target_current(t));
    if d.refreshing
        && let Some(result) = super::jobs::poll::<usize>(ctx, REFRESH)
    {
        d.refreshing = false;
        if current {
            match result {
                Ok(n) => {
                    s.refresh_font_rendering();
                    refresh(&mut d, s);
                    d.notice = Some(format!("{n} installed faces rescanned"));
                }
                Err(error) => d.error = Some(error),
            }
        }
    }
    current = d.owner.as_ref().is_ok_and(|t| s.font_target_current(t));
    let owner = Id::new((ID, &d.document, d.instance));
    let mut action = None;
    let mut added = false;
    let modal = egui::Modal::new(Id::new(ID))
        .backdrop_color(super::theme::bg_window().gamma_multiply(0.55))
        .show(ctx, |ui| {
            ui.set_width((ctx.content_rect().width() - 48.).clamp(160., 480.));
            ui.set_max_height((ctx.content_rect().height() - 64.).max(100.));
            ui.heading("Find Font");
            egui::ScrollArea::vertical()
                .id_salt(owner.with("body"))
                .animated(false)
                .min_scrolled_height(0.)
                .max_height((ctx.content_rect().height() - 110.).max(80.))
                .show(ui, |ui| {
                    if !current {
                        ui.colored_label(
                            ui.visuals().error_fg_color,
                            d.owner.as_ref().err().map(String::as_str).unwrap_or(
                                "The font destination changed. Close and reopen Find Font",
                            ),
                        );
                    }
                    ui.add_enabled_ui(current && !d.refreshing, |ui| {
                        ui.label(format!("Fonts in document: {}", d.rows.len()));
                        egui::ScrollArea::vertical()
                            .id_salt(owner.with("usage"))
                            .max_height(160.)
                            .min_scrolled_height(0.)
                            .show(ui, |ui| {
                                for row in d.rows.iter() {
                                    let label = note(row);
                                    let r = control(
                                        ui,
                                        owner.with((&row.key, "usage")),
                                        &label,
                                        |ui| {
                                            ui.add(
                                                egui::Button::selectable(
                                                    d.selected.as_ref() == Some(&row.key),
                                                    &label,
                                                )
                                                .wrap(),
                                            )
                                        },
                                    );
                                    if d.focus {
                                        r.request_focus();
                                        d.focus = false;
                                    }
                                    if r.clicked() {
                                        d.selected = Some(row.key.clone());
                                        d.error = None;
                                        d.notice = None;
                                    }
                                }
                            });
                        if let Some(row) =
                            d.rows.iter().find(|r| Some(&r.key) == d.selected.as_ref())
                        {
                            if let Some(version) = &row.info.version {
                                ui.label(version);
                            }
                        }
                        ui.separator();
                        let label = ui.label("Replace with");
                        control(ui, owner.with("filter"), "Filter font families", |ui| {
                            ui.add(
                                egui::TextEdit::singleline(&mut d.filter)
                                    .hint_text("Search font families")
                                    .desired_width(f32::INFINITY),
                            )
                            .labelled_by(label.id)
                        });
                        let filter = d.filter.to_lowercase();
                        let families: Vec<_> = d
                            .families
                            .iter()
                            .filter(|f| f.to_lowercase().contains(&filter) || **f == d.family)
                            .map(|f| (f.clone(), f.clone()))
                            .collect();
                        let old = d.family.clone();
                        ui.push_id(owner, |ui| {
                            super::keyboard_controls::choice(
                                ui,
                                "family",
                                "Font family",
                                &mut d.family,
                                &families,
                            );
                            if old != d.family {
                                d.style = None;
                            }
                            let mut styles = vec![(None, "Closest to original style".to_owned())];
                            styles.extend(
                                d.catalog
                                    .styles(&d.family)
                                    .iter()
                                    .map(|f| (Some(f.style.clone()), f.style.clone())),
                            );
                            super::keyboard_controls::choice(
                                ui,
                                "style",
                                "Font style",
                                &mut d.style,
                                &styles,
                            );
                        });
                        if let Some(error) = &d.error {
                            ui.colored_label(ui.visuals().error_fg_color, error);
                        }
                        if let Some(notice) = &d.notice {
                            ui.label(notice);
                        }
                        let row = d.rows.iter().find(|r| Some(&r.key) == d.selected.as_ref());
                        let find = row.and_then(|r| s.capture_font_find(&r.from).ok());
                        let change = row.and_then(|r| {
                            s.capture_font_change(TextScope::Selection, &r.from).ok()
                        });
                        let all = row
                            .and_then(|r| s.capture_font_change(TextScope::Document, &r.from).ok());
                        action =
                            actions(ui, owner, find.is_some(), change.is_some(), all.is_some());
                    });
                    if current
                        && !d.refreshing
                        && d.rows.iter().any(|r| r.info.status != Status::Exact)
                    {
                        let needs = d
                            .rows
                            .iter()
                            .filter(|r| r.info.status != Status::Exact)
                            .map(|r| crate::text::font_recovery::Need {
                                family: r.info.family.clone(),
                                style: r.info.style.clone(),
                                request: r.from.iter().find_map(|r| {
                                    if let crate::text::font_inventory::Request::Named {
                                        weight,
                                        slant,
                                        ..
                                    } = r
                                    {
                                        Some((*weight, *slant))
                                    } else {
                                        None
                                    }
                                }),
                            })
                            .collect();
                        added = super::find_font_recovery::show(
                            ui,
                            s,
                            d.instance,
                            needs,
                            &mut d.recovery,
                        );
                    }
                    if d.refreshing {
                        ui.spinner();
                        ui.label("Refreshing installed fonts…");
                    }
                    {
                        if control(ui, owner.with("close"), "Done", |ui| ui.button("Done"))
                            .clicked()
                        {
                            action = Some("done");
                        }
                    }
                });
        });
    if added && current {
        s.refresh_font_rendering();
        refresh(&mut d, s);
        d.notice = Some("Font files added. The document font list has been refreshed".into());
    }
    let close = modal.should_close() || action == Some("done");
    if !close && let Some(action) = action {
        let result = match action {
            "refresh" => {
                d.refreshing = true;
                super::jobs::start(ctx, REFRESH, || Ok(crate::text::refresh_fonts()));
                Ok(None)
            }
            "find" => d
                .rows
                .iter()
                .find(|r| Some(&r.key) == d.selected.as_ref())
                .ok_or("Choose a font".into())
                .and_then(|row| s.capture_font_find(&row.from))
                .and_then(|target| s.select_font_usage(&target))
                .map(|n| Some(format!("Selected {n} text objects"))),
            "change" | "all" => d
                .rows
                .iter()
                .find(|r| Some(&r.key) == d.selected.as_ref())
                .ok_or("Choose a font".into())
                .and_then(|row| {
                    let target = s.capture_font_change(
                        if action == "change" {
                            TextScope::Selection
                        } else {
                            TextScope::Document
                        },
                        &row.from,
                    )?;
                    let choice = d
                        .catalog
                        .choice(&d.family, d.style.as_deref(), &row.from[0])?;
                    let reference = d.catalog.reference(&choice)?;
                    s.replace_font_usage(&target, &reference)
                        .map(|n| Some(format!("Changed {n} text objects")))
                }),
            _ => Ok(None),
        };
        match result {
            Ok(notice) => {
                if action != "refresh" {
                    refresh(&mut d, s);
                }
                d.error = None;
                d.notice = notice;
            }
            Err(error) => d.error = Some(error),
        }
    }
    if close {
        super::find_font_recovery::cancel(ctx);
        super::jobs::cancel::<usize>(ctx, REFRESH);
        ctx.data_mut(|data| {
            data.remove::<Draft>(Id::new(ID));
            data.insert_temp(Id::new((ID, "return-focus")), d.document);
        });
        ctx.request_discard("Close Find Font before restoring canvas input");
        ctx.request_repaint();
    } else {
        ctx.data_mut(|data| data.insert_temp(Id::new(ID), d));
    }
}

#[cfg(test)]
#[path = "find_font_tests.rs"]
mod tests;
