use crate::{
    app::{Studio, photo_session::catalog::Command},
    photo::catalog::CatalogFilter,
};
use eframe::egui::{self, Button, Id, RichText, ScrollArea, Ui};

#[derive(Clone, Default)]
struct State {
    text: String,
    keyword: String,
    stack: String,
    min_rating: u8,
    keywords: String,
    stack_name: String,
    previous: Vec<String>,
}

pub(super) fn show(ui: &mut Ui, studio: &mut Studio) {
    let busy = studio.photo.catalog.busy();
    ui.horizontal_wrapped(|ui| {
        if ui
            .add_enabled(
                !busy && studio.photo.catalog.path.is_none(),
                Button::new("Open / create…"),
            )
            .clicked()
        {
            studio.request_file_dialog(crate::project::dialog_folder, |_, studio, path| {
                studio.photo.catalog_command(Command::Open(path))
            });
        }
        if ui
            .add_enabled(
                !busy && studio.photo.catalog.path.is_some(),
                Button::new("Import folder…"),
            )
            .clicked()
        {
            studio.request_file_dialog(crate::project::dialog_folder, |_, studio, path| {
                studio.photo.catalog_command(Command::Import(path))
            });
        }
        if ui
            .add_enabled(
                !busy && studio.photo.catalog.path.is_some(),
                Button::new("Close catalog"),
            )
            .clicked()
        {
            studio.photo.close_catalog();
        }
    });
    if busy {
        ui.horizontal(|ui| {
            ui.spinner();
            ui.label("Saving / reading catalog…");
        });
    }
    if let Some(path) = &studio.photo.catalog.path {
        ui.add(egui::Label::new(RichText::new(path.to_string_lossy()).small().weak()).truncate());
    } else if !busy {
        ui.label("Choose an empty folder to create a catalog, or an existing catalog folder to resume. Originals stay where they are.");
    }
    if !studio.photo.catalog.notes.is_empty() {
        ui.collapsing("Catalog recovery", |ui| {
            for note in &studio.photo.catalog.notes {
                ui.label(note);
            }
        });
    }
    if !studio.photo.catalog.error.is_empty() {
        ui.collapsing("Catalog needs attention", |ui| {
            ui.label(&studio.photo.catalog.error);
        });
        if ui
            .add_enabled(!busy, Button::new("Retry saving edits"))
            .clicked()
        {
            studio.photo.retry_catalog_save();
        }
    }
    if studio.photo.catalog.path.is_none() {
        return;
    }
    let id = Id::new("photo-catalog-controls");
    let mut state = ui.data(|d| d.get_temp::<State>(id).unwrap_or_default());
    ui.add(
        egui::TextEdit::singleline(&mut state.text)
            .hint_text("Search names, paths, keywords")
            .char_limit(256),
    );
    ui.collapsing("Filter", |ui| {
        ui.horizontal(|ui| {
            ui.label("At least");
            ui.add(egui::DragValue::new(&mut state.min_rating).range(0..=5));
            ui.label("stars");
        });
        ui.add(
            egui::TextEdit::singleline(&mut state.keyword)
                .hint_text("Keyword")
                .char_limit(256),
        );
        ui.add(
            egui::TextEdit::singleline(&mut state.stack)
                .hint_text("Stack")
                .char_limit(256),
        );
        if ui.button("Clear filters").clicked() {
            state.text.clear();
            state.keyword.clear();
            state.stack.clear();
            state.min_rating = 0;
        }
    });
    let filter = CatalogFilter {
        min_rating: state.min_rating,
        keyword: (!state.keyword.trim().is_empty()).then(|| state.keyword.trim().to_lowercase()),
        stack: (!state.stack.trim().is_empty()).then(|| state.stack.trim().to_lowercase()),
        text: state.text.trim().to_lowercase(),
    };
    let mut command = None;
    let selected = studio
        .photo
        .catalog
        .selected
        .iter()
        .cloned()
        .collect::<Vec<_>>();
    if selected != state.previous {
        if selected.len() == 1
            && let Some(row) = studio
                .photo
                .catalog
                .rows
                .iter()
                .find(|r| r.id == selected[0])
        {
            state.keywords = row.keywords.iter().cloned().collect::<Vec<_>>().join(", ");
            state.stack_name = row.stack.clone().unwrap_or_default();
        }
        state.previous = selected.clone();
    }
    ui.add_enabled_ui(!busy && !selected.is_empty(), |ui| {
        ui.collapsing(format!("Edit {} selected", selected.len()), |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label("Rating");
                for rating in 0..=5 {
                    if ui.small_button(rating.to_string()).clicked() {
                        command = Some(Command::Rating(selected.clone(), rating));
                    }
                }
            });
            ui.add(
                egui::TextEdit::singleline(&mut state.keywords)
                    .hint_text("Keywords, separated by commas")
                    .char_limit(4096),
            );
            if ui.button("Replace keywords").clicked() {
                command = Some(Command::Keywords(
                    selected.clone(),
                    state
                        .keywords
                        .split(',')
                        .map(str::trim)
                        .filter(|k| !k.is_empty())
                        .map(str::to_owned)
                        .collect(),
                ));
            }
            ui.add(
                egui::TextEdit::singleline(&mut state.stack_name)
                    .hint_text("Stack name")
                    .char_limit(256),
            );
            ui.horizontal_wrapped(|ui| {
                if ui
                    .add_enabled(
                        !state.stack_name.trim().is_empty(),
                        Button::new("Stack selected"),
                    )
                    .clicked()
                {
                    command = Some(Command::Stack(
                        selected.clone(),
                        Some(state.stack_name.trim().into()),
                    ));
                }
                if ui.button("Unstack").clicked() {
                    command = Some(Command::Stack(selected.clone(), None));
                }
            });
            if selected.len() == 1 {
                if ui.button("Open saved edits").clicked() {
                    command = Some(Command::Load(selected[0].clone()));
                }
                if ui.button("Relink original…").clicked() {
                    let entry = selected[0].clone();
                    studio.request_file_dialog(
                        crate::project::dialog_photo,
                        move |_, studio, path| {
                            studio.photo.catalog_command(Command::Relink(entry, path))
                        },
                    );
                }
            }
        });
    });
    let catalog = &mut studio.photo.catalog;
    let filtered = catalog
        .rows
        .iter()
        .filter(|r| {
            r.rating >= filter.min_rating
                && filter
                    .keyword
                    .as_ref()
                    .is_none_or(|k| r.keywords.iter().any(|value| value.to_lowercase() == *k))
                && filter.stack.as_ref().is_none_or(|s| {
                    r.stack
                        .as_ref()
                        .is_some_and(|value| value.to_lowercase() == *s)
                })
                && (filter.text.is_empty()
                    || r.name.to_lowercase().contains(&filter.text)
                    || r.path
                        .to_string_lossy()
                        .to_lowercase()
                        .contains(&filter.text)
                    || r.keywords
                        .iter()
                        .any(|k| k.to_lowercase().contains(&filter.text)))
        })
        .collect::<Vec<_>>();
    ui.horizontal_wrapped(|ui| {
        ui.label(
            RichText::new(format!(
                "{} / {} photos",
                filtered.len(),
                catalog.rows.len()
            ))
            .small(),
        );
        if ui.small_button("Select filtered").clicked() {
            catalog.selected = filtered.iter().map(|row| row.id.clone()).collect();
        }
        if ui.small_button("None").clicked() {
            catalog.selected.clear();
        }
    });
    ui.label(
        RichText::new("Double-click to open · edits save in this catalog")
            .small()
            .weak(),
    );
    ScrollArea::vertical()
        .id_salt("photo-catalog-entries")
        .show_rows(ui, 52., filtered.len(), |ui, range| {
            for index in range {
                let row = filtered[index];
                let label = format!(
                    "{}\n{}{}",
                    row.name,
                    "★".repeat(row.rating as usize),
                    row.stack
                        .as_ref()
                        .map(|s| format!(" · {s}"))
                        .unwrap_or_default()
                );
                let response = ui
                    .add_sized(
                        [ui.available_width(), 48.],
                        Button::selectable(catalog.selected.contains(&row.id), label).truncate(),
                    )
                    .on_hover_text(format!(
                        "{}\n{}",
                        row.path.display(),
                        row.keywords.iter().cloned().collect::<Vec<_>>().join(", ")
                    ));
                if response.clicked() {
                    if !ui.input(|i| i.modifiers.command || i.modifiers.shift) {
                        catalog.selected.clear();
                    }
                    if !catalog.selected.insert(row.id.clone()) {
                        catalog.selected.remove(&row.id);
                    }
                }
                if response.double_clicked() && !busy {
                    command = Some(Command::Load(row.id.clone()));
                }
            }
        });
    if let Some(command) = command {
        studio.photo.catalog_command(command);
    }
    ui.data_mut(|d| d.insert_temp(id, state));
}
