//! Brand bank controls, previews, and asset tiles.
use super::*;

fn key(asset: &crate::brand::Asset) -> String {
    format!("{:?}", asset.fingerprint())
}
pub(super) fn brand(ui: &mut Ui, studio: &mut Studio, s: &mut Libraries) {
    ui.label(RichText::new("Made for this brand.").size(21.).strong());
    note(ui, "Your logos, imagery and reusable artwork.");
    ui.add_space(10.);
    project_picker(ui, studio, s);
    ui.add_space(8.);
    let busy = jobs::is_running::<BrandResult>(ui.ctx(), BRAND_ACTION);
    bank_actions(ui, studio, s, busy);
    if busy {
        ui.spinner();
    }
    note(ui, &s.brand_message);
    let Some(catalog) = s.catalog.clone() else {
        ui.add_space(18.);
        note(
            ui,
            "Keep reusable artwork in .omabrand inside your project. Subfolders become searchable categories. Choose a folder or create a bank to begin.",
        );
        return;
    };
    ui.add_space(8.);
    ui.horizontal(|ui| {
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            // Measure the action first so desktop fonts cannot make the row wider
            // than its parent and grow the resizable panel on each frame.
            let save = ui
                .add_enabled(
                    !busy && s.brand_name != catalog.name,
                    egui::Button::new("Save"),
                )
                .clicked();
            ui.add(
                egui::TextEdit::singleline(&mut s.brand_name)
                    .desired_width(ui.available_width())
                    .hint_text("Brand name"),
            );
            if save {
                let root = catalog.root.clone();
                let name = s.brand_name.clone();
                jobs::start(ui.ctx(), BRAND_ACTION, move || {
                    crate::brand::save_name(&root, &name)?;
                    Ok(BrandResult {
                        root,
                        message: "Brand name saved.".into(),
                    })
                });
            }
        });
    });
    ui.add(
        egui::TextEdit::singleline(&mut s.brand_query)
            .hint_text("Filter name, folder or file type…")
            .desired_width(f32::INFINITY),
    );
    ui.horizontal_wrapped(|ui| {
        for kind in ["All", "Image", "SVG", "omadesign"] {
            ui.selectable_value(&mut s.brand_kind, kind.into(), kind);
        }
    });
    let assets: Vec<_> = catalog
        .assets
        .iter()
        .filter(|a| {
            a.matches(&s.brand_query) && (s.brand_kind == "All" || a.kind.label() == s.brand_kind)
        })
        .collect();
    let photo = studio.persona == crate::tools::Persona::Photo;
    let gesture = if photo {
        "double-click to place in Design"
    } else {
        "drag to artboard"
    };
    note(
        ui,
        &format!(
            "{} of {} assets · {gesture}",
            assets.len(),
            catalog.assets.len()
        ),
    );
    if !catalog.warnings.is_empty() {
        ui.collapsing(format!("{} library notes", catalog.warnings.len()), |ui| {
            for warning in &catalog.warnings {
                note(ui, warning);
            }
        });
    }
    let cache_id = egui::Id::new("brand-preview-cache");
    let mut cache = ui
        .ctx()
        .data(|d| d.get_temp::<Previews>(cache_id))
        .unwrap_or_default();
    if cache.root.as_ref() != Some(&catalog.root) {
        cache = Previews {
            root: Some(catalog.root.clone()),
            ..Default::default()
        };
    }
    if let Some(Ok((root, images))) = jobs::poll::<PreviewBatch>(ui.ctx(), PREVIEW)
        && root == catalog.root
    {
        for (key, result) in images {
            Arc::make_mut(&mut cache.textures).insert(
                key.clone(),
                result.map(|image| {
                    ui.ctx().load_texture(
                        key,
                        egui::ColorImage::from_rgba_unmultiplied(
                            [image.w as usize, image.h as usize],
                            &image.data,
                        ),
                        egui::TextureOptions::LINEAR,
                    )
                }),
            );
        }
    }
    // Keep a bounded texture cache. Decode only tiles visible in the scroller.
    if cache
        .catalog
        .as_ref()
        .is_none_or(|old| !Arc::ptr_eq(old, &catalog))
    {
        let valid: std::collections::HashSet<_> = catalog.assets.iter().map(key).collect();
        Arc::make_mut(&mut cache.textures).retain(|k, _| valid.contains(k));
        cache.catalog = Some(catalog.clone());
    }
    let mut needed = Vec::new();
    let mut visible = std::collections::HashSet::new();
    let grid = egui::ScrollArea::vertical()
        .id_salt("brand-bank-grid")
        .auto_shrink([false, false])
        .show_rows(ui, 125., assets.len().div_ceil(2), |ui, rows| {
            let width = ((ui.available_width() - 8.) * 0.5).max(70.);
            for row in rows {
                ui.horizontal(|ui| {
                    for col in 0..2 {
                        let Some(asset) = assets.get(row * 2 + col) else {
                            continue;
                        };
                        let k = key(asset);
                        visible.insert(k.clone());
                        ui.push_id(&asset.relative_path, |ui| {
                            ui.vertical(|ui| {
                                ui.set_width(width);
                                let (rect, response) = ui
                                    .allocate_exact_size(vec2(width, 86.), if photo { Sense::click() } else { Sense::click_and_drag() });
                                checker(ui, rect);
                                ui.painter().rect_stroke(
                                    rect,
                                    7.,
                                    Stroke::new(
                                        1.,
                                        if response.hovered() {
                                            theme::accent()
                                        } else {
                                            theme::border()
                                        },
                                    ),
                                    egui::StrokeKind::Inside,
                                );
                                match cache.textures.get(&k) {
                                    Some(Ok(texture)) => {
                                        let size = texture.size_vec2();
                                        let scale = ((rect.width() - 12.) / size.x)
                                            .min((rect.height() - 12.) / size.y);
                                        let dest = egui::Rect::from_center_size(
                                            rect.center(),
                                            size * scale,
                                        );
                                        ui.painter().image(
                                            texture.id(),
                                            dest,
                                            egui::Rect::from_min_max(
                                                egui::pos2(0., 0.),
                                                egui::pos2(1., 1.),
                                            ),
                                            Color32::WHITE,
                                        );
                                    }
                                    Some(Err(_)) => {
                                        ui.painter().text(
                                            rect.center(),
                                            egui::Align2::CENTER_CENTER,
                                            asset.kind.label(),
                                            egui::FontId::proportional(13.),
                                            theme::fg_weak(),
                                        );
                                    }
                                    None => {
                                        ui.painter().text(
                                            rect.center(),
                                            egui::Align2::CENTER_CENTER,
                                            "···",
                                            egui::FontId::proportional(18.),
                                            theme::fg_weak(),
                                        );
                                        needed.push((*asset).clone());
                                    }
                                }
                                if !photo {
                                    response.dnd_set_drag_payload(BrandDrag {
                                        path: asset.path(&catalog.root),
                                    });
                                }
                                let tip = if let Some(Err(error)) = cache.textures.get(&k) {
                                    format!("{}\n{error}", asset.relative_path.display())
                                } else {
                                    format!(
                                        "{} · {}\nDouble-click to place at the artboard centre in Design",
                                        asset.relative_path.display(),
                                        asset.kind.label()
                                    )
                                };
                                let response = response
                                    .on_hover_text(tip)
                                    .on_hover_cursor(if photo { egui::CursorIcon::PointingHand } else { egui::CursorIcon::Grab });
                                if response.double_clicked() {
                                    let center = studio
                                        .doc
                                        .artboards
                                        .iter()
                                        .find(|a| studio.artboard_sel.contains(&a.id))
                                        .map(|a| a.bounds().center())
                                        .unwrap_or(Pt::new(
                                            studio.doc.width * 0.5,
                                            studio.doc.height * 0.5,
                                        ));
                                    queue_place(
                                        ui.ctx(),
                                        studio,
                                        asset.path(&catalog.root),
                                        center,
                                    );
                                }
                                ui.add(
                                    egui::Label::new(RichText::new(&asset.name).size(12.))
                                        .truncate(),
                                );
                                ui.label(
                                    RichText::new(asset.kind.label())
                                        .small()
                                        .color(theme::fg_weak()),
                                );
                            });
                        });
                    }
                });
            }
            if assets.is_empty() {
                note(
                    ui,
                    if catalog.assets.is_empty() {
                        "Add logos, images, SVGs or .oma artwork. You can also drop files here."
                    } else {
                        "No matching assets. Try another name, folder or type."
                    },
                );
            }
        });
    if cache.textures.len() > 256 {
        Arc::make_mut(&mut cache.textures).retain(|k, _| visible.contains(k));
    }
    if !needed.is_empty() && !jobs::is_running::<PreviewBatch>(ui.ctx(), PREVIEW) {
        let root = catalog.root.clone();
        needed.truncate(12);
        jobs::start(ui.ctx(), PREVIEW, move || {
            let results: Vec<_> = needed
                .iter()
                .map(|a| (key(a), crate::brand::load_thumbnail(&root, a, 192)))
                .collect();
            Ok((root, results))
        });
    }
    ui.ctx().data_mut(|d| d.insert_temp(cache_id, cache));
    if !busy
        && ui.ctx().input(|i| {
            i.pointer
                .hover_pos()
                .is_some_and(|p| grid.inner_rect.contains(p))
        })
    {
        let files: Vec<PathBuf> = ui.ctx().input(|i| {
            i.raw
                .dropped_files
                .iter()
                .map(|f| f.path().to_path_buf())
                .collect()
        });
        if !files.is_empty() {
            ui.ctx().input_mut(|i| i.raw.dropped_files.clear());
            let root = catalog.root.clone();
            jobs::start(ui.ctx(), BRAND_ACTION, move || {
                let added = crate::brand::add_files(&root, &files)?;
                Ok(BrandResult {
                    root,
                    message: format!("Added {} assets.", added.len()),
                })
            });
        }
    }
}

fn bank_actions(ui: &mut Ui, studio: &Studio, state: &mut Libraries, busy: bool) {
    ui.horizontal_wrapped(|ui| {
        if ui
            .button("Load bank…")
            .on_hover_text("Choose a project folder or its .omabrand directory")
            .clicked()
            && let Some(folder) = crate::project::dialog_folder()
        {
            state.choose_folder(&studio.swap_id, folder);
        }
        if let Some(root) = state.root.clone() {
            if state.catalog.is_none() {
                if ui.add_enabled(!busy, egui::Button::new("Create bank")).clicked() {
                    let name = root.file_name()
                        .map(|name| name.to_string_lossy().into_owned())
                        .unwrap_or_else(|| "Brand".into());
                    jobs::start(ui.ctx(), BRAND_ACTION, move || {
                        crate::brand::create(&root, &name)?;
                        Ok(BrandResult {
                            root,
                            message: "Brand bank created. Add some artwork.".into(),
                        })
                    });
                }
            } else {
                ui.menu_button("···", |ui| {
                    if ui.add_enabled(!busy, egui::Button::new("Add assets…")).clicked() {
                        ui.close();
                        if let Some(files) = rfd::FileDialog::new()
                            .add_filter("Brand artwork", &[
                                "png", "jpg", "jpeg", "webp", "tif", "tiff", "bmp", "gif", "svg", "oma",
                            ])
                            .pick_files()
                        {
                            let root = root.clone();
                            jobs::start(ui.ctx(), BRAND_ACTION, move || {
                                let added = crate::brand::add_files(&root, &files)?;
                                Ok(BrandResult {
                                    root,
                                    message: format!("Added {} assets. Originals kept in place.", added.len()),
                                })
                            });
                        }
                    }
                    if ui
                        .add_enabled(!busy, egui::Button::new("Save bank copy…"))
                        .on_hover_text("Copy the complete .omabrand folder, including its name and nested folders")
                        .clicked()
                    {
                        ui.close();
                        if let Some(destination) = crate::project::dialog_folder() {
                            let root = root.clone();
                            jobs::start(ui.ctx(), BRAND_ACTION, move || {
                                let output = crate::brand::export_copy_bank(&root, &destination)?;
                                Ok(BrandResult {
                                    root,
                                    message: format!("Saved bank copy to {}", output.display()),
                                })
                            });
                        }
                    }
                    if ui.button("Refresh now").clicked() {
                        state.next_sync = Instant::now();
                        ui.close();
                    }
                });
            }
        }
    });
}
