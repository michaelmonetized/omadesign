//! Typography edits are explicit worker-backed saves, scoped to their project.
use super::*;
const ACTION: &str = "brand-typography-action";
struct Completed {
    root: PathBuf,
    result: Result<Option<crate::typography::LoadedKit>, String>,
}
pub(super) fn busy(ctx: &egui::Context) -> bool {
    jobs::is_running::<Completed>(ctx, ACTION)
}
fn start(
    ctx: &egui::Context,
    root: PathBuf,
    work: impl FnOnce() -> Result<Option<crate::typography::LoadedKit>, String> + Send + 'static,
) {
    jobs::start(ctx, ACTION, move || {
        Ok(Completed {
            root,
            result: work(),
        })
    });
}
pub(super) fn receive(
    state: &mut Libraries,
    loaded: crate::typography::LoadedKit,
) -> Result<(), String> {
    if state
        .typography_edit_stamp
        .is_some_and(|stamp| stamp != loaded.stamp)
    {
        return Err("Typography changed on disk. Your edits are kept here; Reload typography to use the saved version.".into());
    }
    crate::typography::register(&loaded)?;
    let selected = state
        .typography
        .as_ref()
        .and_then(|old| old.kit.roles.get(state.typography_selected))
        .cloned();
    state.typography_selected = selected
        .as_ref()
        .and_then(|role| {
            loaded
                .kit
                .roles
                .iter()
                .position(|candidate| candidate == role)
        })
        .unwrap_or(0);
    state.typography_refresh |= state.typography.as_ref().is_none_or(|old| {
        old.fonts
            .iter()
            .map(|font| &font.id)
            .ne(loaded.fonts.iter().map(|font| &font.id))
    });
    if state.typography_edit_stamp.is_none() {
        state.typography_name = loaded.kit.name.clone();
        state.typography_role = loaded
            .kit
            .roles
            .get(state.typography_selected)
            .map(|role| role.name.clone())
            .unwrap_or_default();
    }
    state.typography = Some(Arc::new(loaded));
    Ok(())
}

fn edited_kit(
    loaded: &crate::typography::LoadedKit,
    state: &Libraries,
) -> crate::typography::TypeKit {
    let mut kit = loaded.kit.clone();
    kit.name = state.typography_name.trim().into();
    if let Some(role) = kit.roles.get_mut(state.typography_selected) {
        role.name = state.typography_role.trim().into();
    }
    kit
}

pub(super) fn tick(ctx: &egui::Context, studio: &mut Studio) {
    if let Some(result) = jobs::poll::<Completed>(ctx, ACTION) {
        jobs::cancel::<Snapshot>(ctx, SYNC);
        studio.libraries.next_sync = Instant::now();
        match result {
            Ok(done) => match done.result {
                Ok(loaded) => {
                    if studio.libraries.root.as_ref() == Some(&done.root) {
                        studio.libraries.typography_message =
                            "Typography saved. Ready to share.".into();
                        if let Some(loaded) = loaded {
                            studio.libraries.typography_edit_stamp = None;
                            if let Err(error) = receive(&mut studio.libraries, loaded) {
                                studio.libraries.typography_message = error;
                            }
                        }
                    }
                }
                Err(error) => {
                    if studio.libraries.close_requested {
                        studio.libraries.close_error = error.clone();
                    }
                    if studio.libraries.root.as_ref() == Some(&done.root) {
                        studio.libraries.typography_message = error;
                    } else {
                        studio.status = error;
                    }
                }
            },
            Err(error) => studio.status = error,
        }
    }
    if std::mem::take(&mut studio.libraries.typography_refresh) {
        studio.refresh_project_type();
    }
}

fn save_edits(
    ctx: &egui::Context,
    root: &std::path::Path,
    loaded: &crate::typography::LoadedKit,
    state: &Libraries,
) {
    let kit = edited_kit(loaded, state);
    let target = root.to_path_buf();
    let expected = Some(loaded.stamp);
    start(ctx, target.clone(), move || {
        crate::typography::save(&target, &kit, expected).map(Some)
    });
}

fn file_actions(
    ui: &mut Ui,
    root: &std::path::Path,
    expected: Option<u128>,
    busy: bool,
    has_edits: bool,
) -> bool {
    let available = !busy && !has_edits;
    let mut reload = false;
    ui.horizontal(|ui| {
        if ui
            .add_enabled(available, egui::Button::new("Add fonts…"))
            .clicked()
            && let Some(paths) = rfd::FileDialog::new()
                .add_filter("Font files", &["ttf", "otf"])
                .pick_files()
        {
            let target = root.to_path_buf();
            start(ui.ctx(), target.clone(), move || {
                crate::typography::add_fonts(&target, &paths, expected).map(Some)
            });
        }
        ui.add_enabled_ui(!busy, |ui| {
            ui.menu_button("···", |ui| {
                if ui
                    .add_enabled(available, egui::Button::new("Load kit…"))
                    .clicked()
                {
                    ui.close();
                    if let Some(path) = rfd::FileDialog::new()
                        .add_filter("Typography kit", &["omatype", "json"])
                        .pick_file()
                    {
                        let target = root.to_path_buf();
                        start(ui.ctx(), target.clone(), move || {
                            crate::typography::import_file(&path, &target, expected).map(Some)
                        });
                    }
                }
                if ui
                    .add_enabled(
                        available && expected.is_some(),
                        egui::Button::new("Save copy…"),
                    )
                    .clicked()
                {
                    ui.close();
                    if let Some(target) = crate::project::dialog_folder() {
                        let source = root.to_path_buf();
                        start(ui.ctx(), source.clone(), move || {
                            crate::typography::export_copy(&source, &target)?;
                            Ok(None)
                        });
                    }
                }
                ui.separator();
                if ui
                    .button("Reload typography")
                    .on_hover_text("Discard pending name or role edits and reload this project kit")
                    .clicked()
                {
                    ui.close();
                    reload = true;
                }
            })
            .response
            .on_hover_text("Load, save a copy or reload typography");
        });
    });
    reload
}

fn name_editor(
    ui: &mut Ui,
    name: &mut String,
    hint: &str,
    save_label: &str,
    enabled: bool,
) -> (bool, bool) {
    let mut changed = false;
    let mut save = false;
    ui.horizontal(|ui| {
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            save = ui
                .add_enabled(enabled, egui::Button::new(save_label))
                .clicked();
            changed = ui
                .add_enabled(
                    enabled,
                    egui::TextEdit::singleline(name)
                        .desired_width(ui.available_width())
                        .hint_text(hint),
                )
                .changed();
        });
    });
    (changed, save)
}

pub(super) fn show(ui: &mut Ui, studio: &mut Studio, state: &mut Libraries) {
    let Some(root) = state.root.clone() else {
        return;
    };
    egui::CollapsingHeader::new("Typography")
        .id_salt("brand-typography")
        .default_open(state.typography.is_some())
        .show(ui, |ui| {
            let busy = busy(ui.ctx()) || jobs::is_running::<BrandResult>(ui.ctx(), BRAND_ACTION);
            let loaded = state.typography.clone();
            let expected = loaded.as_ref().map(|loaded| loaded.stamp);
            note(ui, "Project fonts stay with this brand. Nothing is installed globally.");
            note(ui, &state.typography_message);
            if file_actions(ui, &root, expected, busy, state.typography_edit_stamp.is_some()) {
                state.typography_edit_stamp = None;
                state.typography_name.clear();
                state.typography_role.clear();
                state.typography_message.clear();
                state.typography = None;
                state.next_sync = Instant::now();
                jobs::cancel::<Snapshot>(ui.ctx(), SYNC);
                return;
            }
            let Some(loaded) = loaded else {
                note(ui, "Add TTF or OTF files to start a kit, or load a .omatype file beside its .omabrand folder.");
                return;
            };
            let (changed, save) = name_editor(
                ui, &mut state.typography_name, "Typography name", "Save name", !busy,
            );
            if changed {
                state.typography_edit_stamp.get_or_insert(loaded.stamp);
            }
            if save {
                save_edits(ui.ctx(), &root, &loaded, state);
            }
            ui.add(
                egui::TextEdit::singleline(&mut state.typography_query)
                    .desired_width(ui.available_width())
                    .hint_text("Filter role, family or file…"),
            );
            let query = state.typography_query.to_lowercase();
            egui::ScrollArea::vertical()
                .id_salt("brand-font-roles")
                .max_height(160.0)
                .show(ui, |ui| {
                    for (index, font) in loaded.fonts.iter().enumerate() {
                        let searchable = format!("{} {} {}", font.role, font.family, font.file);
                        if !searchable.to_lowercase().contains(&query) {
                            continue;
                        }
                        ui.horizontal(|ui| {
                            if ui.small_button("Apply")
                                .on_hover_text("Use on selected text, or the next text you create")
                                .clicked()
                                && let Err(error) = studio.apply_project_font(font)
                            {
                                state.typography_message = error;
                            }
                            let label = format!("{} · {}", font.role, font.family);
                            if ui.add_enabled(
                                !busy && state.typography_edit_stamp.is_none(),
                                egui::Button::selectable(state.typography_selected == index, label)
                                    .truncate(),
                            ).on_hover_text(&font.file).clicked() {
                                state.typography_selected = index;
                                state.typography_role = font.role.clone();
                            }
                        });
                    }
                });
            if loaded.kit.roles.get(state.typography_selected).is_some() {
                let (changed, save) = name_editor(
                    ui, &mut state.typography_role, "Heading, Body, Caption…", "Save role", !busy,
                );
                if changed {
                    state.typography_edit_stamp.get_or_insert(loaded.stamp);
                }
                if save {
                    save_edits(ui.ctx(), &root, &loaded, state);
                }
                if ui.add_enabled(!busy, egui::Button::new("Remove role"))
                    .on_hover_text("The copied font file stays available to existing artwork")
                    .clicked()
                {
                    let mut kit = edited_kit(&loaded, state);
                    kit.roles.remove(state.typography_selected);
                    let target = root.clone();
                    start(ui.ctx(), root.clone(), move || {
                        crate::typography::save(&target, &kit, expected).map(Some)
                    });
                }
            }
        });
    ui.add_space(6.0);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::typography::{FontRole, LoadedKit, TypeKit};

    fn kit(stamp: u128, names: &[&str]) -> LoadedKit {
        LoadedKit {
            kit: TypeKit {
                version: 1,
                name: "Fieldwork".into(),
                roles: names
                    .iter()
                    .map(|name| FontRole {
                        name: (*name).into(),
                        font: format!("fonts/{name}.ttf"),
                    })
                    .collect(),
            },
            fonts: vec![],
            stamp,
        }
    }

    #[test]
    fn refresh_keeps_pending_role_edits_and_reconciles_selection_by_identity() {
        let mut state = Libraries::default();
        receive(&mut state, kit(1, &["Heading", "Body"])).unwrap();
        state.typography_selected = 1;
        state.typography_role = "Reading".into();
        state.typography_name = "My draft".into();
        state.typography_edit_stamp = Some(1);
        assert!(receive(&mut state, kit(2, &["Body", "Heading"])).is_err());
        assert_eq!(state.typography.as_ref().unwrap().stamp, 1);
        assert_eq!(state.typography_selected, 1);
        assert_eq!(state.typography_role, "Reading");
        assert_eq!(state.typography_name, "My draft");
        let draft = edited_kit(state.typography.as_ref().unwrap(), &state);
        assert_eq!(draft.roles[1].name, "Reading");
        assert_eq!(draft.name, "My draft");

        // Once the draft is resolved, reorderings retain the selected font role.
        state.typography_edit_stamp = None;
        receive(&mut state, kit(2, &["Body", "Heading"])).unwrap();
        assert_eq!(state.typography_selected, 0);
        assert_eq!(state.typography_role, "Body");
        receive(&mut state, kit(3, &["Heading"])).unwrap();
        assert_eq!(state.typography_selected, 0);
        assert_eq!(state.typography_role, "Heading");
    }

    #[test]
    fn compact_brand_panel_scrolls_to_role_actions_without_growing() {
        let font_path = &crate::text::fonts()[0].path;
        let bytes = Arc::new(std::fs::read(font_path).unwrap());
        for size in [vec2(960.0, 640.0), vec2(1600.0, 900.0)] {
            let ctx = egui::Context::default();
            theme::apply(&ctx);
            let mut studio = Studio::new();
            let mut loaded = kit(
                1,
                &["Heading", "Body", "Caption", "Quote", "Label", "Display"],
            );
            loaded.fonts = loaded
                .kit
                .roles
                .iter()
                .map(|role| crate::typography::LoadedFont {
                    role: role.name.clone(),
                    file: role.font.clone(),
                    id: format!("omatype:{:032x}", crate::typography::fingerprint(&bytes)),
                    family: crate::text::font_name(&bytes).unwrap(),
                    bytes: bytes.clone(),
                })
                .collect();
            receive(&mut studio.libraries, loaded).unwrap();
            studio.libraries.root = Some(PathBuf::from("/tmp/typography-layout-fixture"));
            studio.libraries.sidebar = Sidebar::Brand;
            let mut save_seen = false;
            let mut remove_seen = false;
            for frame in 0..24 {
                let mut events = vec![egui::Event::PointerMoved(egui::pos2(size.x - 250.0, 360.0))];
                if frame >= 5 {
                    events.push(egui::Event::MouseWheel {
                        unit: egui::MouseWheelUnit::Point,
                        delta: vec2(0.0, -24.0),
                        phase: egui::TouchPhase::Move,
                        modifiers: egui::Modifiers::NONE,
                    });
                }
                let mut output = ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                        events,
                        time: Some(frame as f64 / 60.0),
                        ..Default::default()
                    },
                    |ui| {
                        egui::Panel::top("test-chrome")
                            .exact_size(82.0)
                            .show(ui, |_| {});
                        egui::Panel::bottom("test-hud")
                            .exact_size(58.0)
                            .show(ui, |_| {});
                        let before = ui.available_width();
                        crate::ui::studios::right_panel(ui, &mut studio);
                        assert!((287.0..=290.0).contains(&(before - ui.available_width())));
                    },
                );
                output.textures_delta.clear();
                let visible = |label: &str| {
                    output.shapes.iter().any(|clipped| {
                    matches!(&clipped.shape, egui::Shape::Text(text)
                        if text.galley.job.text == label
                            && clipped.clip_rect.expand(0.5).contains_rect(text.visual_bounding_rect()))
                })
                };
                assert!(
                    visible("Brand"),
                    "the sidebar tabs stay fixed while scrolling"
                );
                save_seen |= visible("Save role");
                remove_seen |= visible("Remove role");
            }
            assert!(
                save_seen && remove_seen,
                "font role actions must be reachable at {size:?}"
            );
        }
    }
}
