use crate::app::Studio;
use eframe::egui::{self, RichText};

pub fn show(ctx: &egui::Context, studio: &mut Studio) {
    if !studio.show_preferences {
        return;
    }
    let before = studio.startup_preferences.clone();
    let height = (ctx.content_rect().height() - 100.).clamp(240., 680.);
    let mut open = true;
    let response = egui::Modal::new(egui::Id::new("omadesign-settings")).show(ctx, |ui| {
        ui.set_width(460.);
        ui.set_max_height(height);
        ui.horizontal(|ui| {
            ui.heading("omadesign");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .add_enabled(!studio.updates.freezing, egui::Button::new("Close"))
                    .clicked()
                {
                    open = false;
                }
            });
        });
        ui.add_space(8.);
        ui.horizontal(|ui| {
            for (page, label) in [(0, "Config"), (1, "Update"), (2, "About")] {
                if ui
                    .add_enabled(
                        !studio.updates.freezing,
                        egui::Button::selectable(studio.settings_page == page, label),
                    )
                    .clicked()
                {
                    studio.settings_page = page;
                }
            }
            ui.hyperlink_to("Docs ↗", "https://omadesign.app/docs/");
        });
        ui.separator();
        egui::ScrollArea::vertical()
            .max_height(height - 100.)
            .show(ui, |ui| match studio.settings_page {
                1 => update(ui, studio),
                2 => about(ui, studio),
                _ => config(ui, studio),
            });
    });
    open &= studio.show_preferences;
    studio.show_preferences = open && (!response.should_close() || studio.updates.freezing);
    if before != studio.startup_preferences {
        studio.save_startup_preferences();
        crate::telemetry::set_consent(studio.startup_preferences.anonymous_usage);
        if before.ui_font != studio.startup_preferences.ui_font {
            super::theme::apply_preferences(ctx, &studio.startup_preferences.ui_font);
        }
    }
}
fn config(ui: &mut egui::Ui, studio: &mut Studio) {
    ui.strong("Appearance");
    ui.label("UI font");
    ui.text_edit_singleline(&mut studio.startup_preferences.ui_font)
        .on_hover_text("Font family or font file path. Leave empty to follow your desktop font.");
    ui.horizontal(|ui| {
        ui.label("Font size");
        ui.add(
            egui::Slider::new(&mut studio.startup_preferences.ui_font_size, 10..=24).suffix(" px"),
        );
    });
    ui.small("The interface scales with the font size to keep controls readable.");
    ui.separator();
    ui.strong("Startup");
    super::welcome::startup_preferences(ui, studio);
    ui.separator();
    ui.strong("Canvas");
    ui.checkbox(
        &mut studio.startup_preferences.guides_locked_by_default,
        "Lock guides in new documents",
    );
    if ui
        .checkbox(&mut studio.startup_preferences.show_rulers, "Show rulers")
        .changed()
    {
        studio.show_rulers = studio.startup_preferences.show_rulers;
    }
    if ui
        .checkbox(
            &mut studio.startup_preferences.show_key_hud,
            "Show shortcut hints",
        )
        .changed()
    {
        studio.show_key_hud = studio.startup_preferences.show_key_hud;
    }
    ui.separator();
    ui.strong("Privacy and updates");
    ui.checkbox(
        &mut studio.startup_preferences.anonymous_usage,
        "Send anonymous usage data",
    );
    ui.small("Off by default. Sends counts of activity, tools, modes and features, plus error categories. No persistent identifier, documents, file paths, typed text, screenshots or retained IP addresses.");
    ui.hyperlink_to("Privacy details ↗", "https://omadesign.app/docs/privacy/");

    ui.separator();
    ui.small("Saved automatically in ~/.config/omadesign/preferences.json");
    ui.separator();
    ui.collapsing("Photo search providers", |ui| asset_keys(ui));
    if ui.button("Keyboard shortcuts").clicked() {
        studio.show_preferences = false;
        studio.show_shortcuts = true;
    }
}
fn update(ui: &mut egui::Ui, studio: &mut Studio) {
    ui.strong(format!("Installed: {}", env!("CARGO_PKG_VERSION")));
    ui.add_space(8.);
    ui.label(&studio.updates.status);
    if studio.updates.busy {
        ui.spinner();
    }
    ui.add_space(8.);
    if ui
        .add_enabled(!studio.updates.busy, egui::Button::new("Check for updates"))
        .clicked()
    {
        studio.updates.check(ui.ctx());
    }
    if let Some(release) = studio.updates.latest.clone() {
        if ui
            .add_enabled(
                !studio.updates.busy,
                egui::Button::new(format!("Update to {} and restart", release.version)),
            )
            .clicked()
        {
            studio.updates.install(ui.ctx());
        }
    }
    ui.add_space(8.);
    ui.label("The official installer verifies the download. Once installed, Omadesign saves open documents and unsaved photo edits to recovery storage, then restarts into your workspace.");
    ui.add_space(8.);
    ui.checkbox(
        &mut studio.startup_preferences.check_updates,
        "Check for updates automatically",
    );
    ui.small("Update checks contact GitHub and are separate from optional usage data.");
}
fn about(ui: &mut egui::Ui, studio: &mut Studio) {
    let key = egui::Id::new("about-official-logo");
    let texture = ui
        .ctx()
        .data_mut(|d| d.get_temp::<egui::TextureHandle>(key))
        .or_else(|| {
            let tree = usvg::Tree::from_data(
                include_bytes!("../../assets/omadesign-wordmark.svg"),
                &usvg::Options::default(),
            )
            .ok()?;
            let scale = 880. / tree.size().width().max(tree.size().height());
            let mut pixels = tiny_skia::Pixmap::new(
                (tree.size().width() * scale).ceil() as u32,
                (tree.size().height() * scale).ceil() as u32,
            )?;
            resvg::render(
                &tree,
                tiny_skia::Transform::from_scale(scale, scale),
                &mut pixels.as_mut(),
            );
            let image = egui::ColorImage::from_rgba_premultiplied(
                [pixels.width() as usize, pixels.height() as usize],
                pixels.data(),
            );
            let texture =
                ui.ctx()
                    .load_texture("official logo", image, egui::TextureOptions::LINEAR);
            ui.ctx().data_mut(|d| d.insert_temp(key, texture.clone()));
            Some(texture)
        });
    ui.vertical_centered(|ui| {
        if let Some(texture) = texture {
            ui.image((texture.id(), texture.size_vec2() * 0.5));
        }
        let version = env!("CARGO_PKG_VERSION");
        let base = version.split('-').next().unwrap_or(version);
        ui.label(
            RichText::new(format!(
                "omadesign-alpha{}-{base}",
                if version.contains("nightly") {
                    "-nightly"
                } else {
                    ""
                }
            ))
            .strong(),
        );
        ui.small(format!("Semantic version: {version}"));
        ui.add_space(8.);
        ui.label("Native Linux studio for design, layout, pixels, photography and motion.");
        ui.hyperlink_to(
            "Source code ↗",
            "https://github.com/michaelmonetized/omadesign",
        );
        if let Some(release) = &studio.updates.latest {
            if ui
                .button(format!("Update available: {}", release.version))
                .clicked()
            {
                studio.settings_page = 1;
            }
        }
    });
}
#[derive(Clone, Default)]
struct AssetKeys {
    pixabay: String,
    pexels: String,
    status: String,
}
fn asset_keys(ui: &mut egui::Ui) {
    let key = egui::Id::new("settings-asset-keys");
    let mut state = ui
        .ctx()
        .data(|d| d.get_temp::<AssetKeys>(key))
        .unwrap_or_else(|| {
            let (pixabay, pexels) = crate::asset_browser::configured_keys();
            AssetKeys {
                pixabay,
                pexels,
                status: String::new(),
            }
        });
    ui.label("Pixabay API key");
    ui.add(egui::TextEdit::singleline(&mut state.pixabay).password(true));
    ui.label("Pexels API key");
    ui.add(egui::TextEdit::singleline(&mut state.pexels).password(true));
    ui.small("Stored locally in assets.toml. Environment variables override these keys.");
    if ui.button("Save provider keys").clicked() {
        state.status =
            match crate::asset_browser::save_configured_keys(&state.pixabay, &state.pexels) {
                Ok(()) => "Provider keys saved.".into(),
                Err(e) => e,
            };
    }
    if !state.status.is_empty() {
        ui.label(&state.status);
    }
    ui.ctx().data_mut(|d| d.insert_temp(key, state));
}
