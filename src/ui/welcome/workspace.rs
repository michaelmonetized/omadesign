use super::{catalog, team};
use crate::app::{Studio, WelcomePage};
use crate::tools::Persona;
use crate::ui::{agent, icons, theme::*};
use eframe::egui::{
    self, Align, Align2, Button, Frame, Id, Layout, Margin, Rect, RichText, ScrollArea, Sense,
    Stroke, Ui, pos2, vec2,
};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

const STATE: &str = "welcome-workspace";

#[derive(Clone, Default)]
struct Selection {
    paths: BTreeSet<PathBuf>,
}
impl Selection {
    fn click(&mut self, path: &Path, shift: bool) -> bool {
        if shift || !self.paths.is_empty() {
            if !self.paths.remove(path) {
                self.paths.insert(path.to_owned());
            }
            false
        } else {
            true
        }
    }
}

#[derive(Clone, Default)]
struct State {
    filter: Option<Persona>,
    project: Option<PathBuf>,
    team: bool,
    left: Selection,
    right: Selection,
    size: Option<Persona>,
    brand: bool,
    entered: bool,
    narrow_panel: usize,
}

pub(super) fn modal_open(ctx: &egui::Context) -> bool {
    ctx.data(|d| d.get_temp::<State>(Id::new(STATE)))
        .is_some_and(|s| s.size.is_some() || s.brand)
}

pub(super) fn leave(ctx: &egui::Context) {
    ctx.data_mut(|d| {
        if let Some(mut state) = d.get_temp::<State>(Id::new(STATE)) {
            if state.brand || state.size.is_some() || state.entered {
                state.brand = false;
                state.size = None;
                state.entered = false;
                d.insert_temp(Id::new(STATE), state);
            }
        }
    });
}

pub(super) fn ready(ctx: &egui::Context) -> bool {
    catalog::ready(ctx) && team::ready(ctx)
}

pub(super) fn show(ui: &mut Ui, studio: &mut Studio) {
    let ctx = ui.ctx().clone();
    let mut state = ctx
        .data(|d| d.get_temp::<State>(Id::new(STATE)))
        .unwrap_or_default();
    if !state.entered {
        if studio.welcome_page == WelcomePage::Templates {
            crate::ui::templates::open(&ctx, studio, Persona::Design);
        }
        state.entered = true;
    }
    if !agent::is_open(&ctx)
        && !studio.show_templates
        && !state.brand
        && state.size.is_none()
        && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
    {
        state.left.paths.clear();
        state.right.paths.clear();
    }
    let snapshot = catalog::snapshot(&ctx);
    let has_team = team::has_shared(&ctx, studio);
    if !has_team {
        state.team = false;
    }
    let mut previews = catalog::Previews::begin(&ctx);
    ui.painter()
        .rect_filled(ui.available_rect_before_wrap(), 0., welcome_dark());
    ui.add_space(22.);
    let mut full = ui.available_rect_before_wrap();
    let narrow = full.width() < 820.;
    if narrow {
        ui.horizontal(|ui| {
            for (index, label) in ["Your Work", "Create", "Projects"].into_iter().enumerate() {
                ui.selectable_value(&mut state.narrow_panel, index, label);
            }
        });
        ui.add_space(10.);
        full = ui.available_rect_before_wrap();
    }
    let gap = 20.;
    let center_width = (full.width() * 0.19).clamp(260., 360.);
    let side = ((full.width() - center_width - gap * 2.) * 0.5).max(1.);
    let left = if narrow {
        full
    } else {
        Rect::from_min_size(full.min, vec2(side, full.height()))
    };
    let middle = if narrow {
        Rect::from_min_size(
            pos2(full.center().x - full.width().min(360.) * 0.5, full.top()),
            vec2(full.width().min(360.), full.height()),
        )
    } else {
        Rect::from_min_size(
            pos2(left.right() + gap, full.top()),
            vec2(center_width, full.height()),
        )
    };
    let right = if narrow {
        full
    } else {
        Rect::from_min_max(pos2(middle.right() + gap, full.top()), full.max)
    };
    if !narrow || state.narrow_panel == 0 {
        ui.scope_builder(
            egui::UiBuilder::new().id_salt("your-work").max_rect(left),
            |ui| {
                ui.set_width(left.width());
                ui.horizontal_wrapped(|ui| {
                    ui.add_space(4.);
                    ui.label(RichText::new("Your Work").strong().size(19.));
                    for (page, label) in [
                        (WelcomePage::Recents, "recent"),
                        (WelcomePage::Recovered, "recovered"),
                    ] {
                        let selected = if page == WelcomePage::Recents {
                            studio.welcome_page != WelcomePage::Recovered
                        } else {
                            studio.welcome_page == page
                        };
                        if ui.selectable_label(selected, label).clicked() {
                            studio.set_welcome_page(page);
                            state.left.paths.clear();
                        }
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let (rect, response) =
                            ui.allocate_exact_size(vec2(30., 28.), Sense::click());
                        let color = if state.filter.is_some() {
                            p().error
                        } else if response.hovered() || response.has_focus() {
                            p().blue
                        } else {
                            fg_weak()
                        };
                        ui.painter().text(
                            rect.center(),
                            Align2::CENTER_CENTER,
                            "\u{E266}",
                            icons::font(21.),
                            color,
                        );
                        egui::Popup::menu(&response).show(|ui| {
                            if ui
                                .selectable_value(&mut state.filter, None, "All modes")
                                .clicked()
                            {
                                ui.close();
                            }
                            for (mode, label) in modes() {
                                if ui
                                    .selectable_value(&mut state.filter, Some(mode), label)
                                    .clicked()
                                {
                                    ui.close();
                                }
                            }
                        });
                        response.on_hover_text("Filter by mode");
                    });
                });
                ui.add_space(12.);
                browser_panel(ui, "work-files", |ui| {
                    if studio.welcome_page == WelcomePage::Recovered {
                        let entries = catalog::recovered(&ctx);
                        selection_bar(ui, studio, &mut state.left, true);
                        let documents: Vec<_> = entries
                            .iter()
                            .filter(|d| state.filter.is_none_or(|m| d.matches_mode(m)))
                            .collect();
                        document_grid(ui, studio, &documents, &mut state.left, &mut previews, true);
                        if documents.is_empty() {
                            quiet(ui, "No recovered documents.");
                        }
                    } else {
                        selection_bar(ui, studio, &mut state.left, false);
                        let documents: Vec<_> = snapshot
                            .catalog
                            .documents
                            .iter()
                            .filter(|d| state.filter.is_none_or(|m| d.matches_mode(m)))
                            .collect();
                        document_grid(
                            ui,
                            studio,
                            &documents,
                            &mut state.left,
                            &mut previews,
                            false,
                        );
                        if documents.is_empty() && !snapshot.scanning {
                            quiet(ui, "Your .oma documents will appear here.");
                        }
                    }
                    if snapshot.scanning {
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.weak("Finding your work…")
                                .on_hover_text(format!("{} entries checked", snapshot.visited));
                        });
                    }
                    if !snapshot.catalog.warnings.is_empty() {
                        ui.collapsing(
                            format!("{} scan notes", snapshot.catalog.warnings.len()),
                            |ui| {
                                for warning in &snapshot.catalog.warnings {
                                    ui.small(warning);
                                }
                            },
                        );
                    }
                    if ui.small_button("Refresh").clicked() {
                        catalog::refresh(&ctx);
                    }
                });
            },
        );
    }
    if !narrow || state.narrow_panel == 1 {
        ui.scope_builder(
            egui::UiBuilder::new()
                .id_salt("welcome-create")
                .max_rect(middle),
            |ui| {
                ui.set_width(middle.width());
                ScrollArea::vertical()
                    .id_salt("welcome-actions")
                    .auto_shrink([false, false])
                    .max_height(middle.height())
                    .show(ui, |ui| {
                        center(ui, studio, &mut state);
                    });
            },
        );
    }
    if !narrow || state.narrow_panel == 2 {
        ui.scope_builder(
            egui::UiBuilder::new()
                .id_salt("welcome-projects")
                .max_rect(right),
            |ui| {
                ui.set_width(right.width());
                ui.horizontal(|ui| {
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.add_space(14.);
                        ui.label(RichText::new("Projects").strong().size(19.));
                        if ui.selectable_label(!state.team, "recent").clicked() {
                            state.team = false;
                        }
                        if has_team && ui.selectable_label(state.team, "team").clicked() {
                            state.team = true;
                        }
                    });
                });
                ui.add_space(12.);
                browser_panel(ui, "project-files", |ui| {
                    if state.team {
                        team::show(ui, studio);
                        return;
                    }
                    if let Some(path) = state.project.clone() {
                        ui.horizontal_wrapped(|ui| {
                            if ui.small_button("← Projects").clicked() {
                                state.project = None;
                                state.right.paths.clear();
                            }
                            if let Some(parent) = snapshot
                                .catalog
                                .projects
                                .iter()
                                .filter(|p| path != p.path && path.starts_with(&p.path))
                                .max_by_key(|p| p.path.components().count())
                            {
                                if ui.small_button(format!("↑ {}", parent.name)).clicked() {
                                    state.project = Some(parent.path.clone());
                                    state.right.paths.clear();
                                }
                            }
                        });
                        let name = snapshot
                            .catalog
                            .projects
                            .iter()
                            .find(|p| p.path == path)
                            .map(|p| p.name.as_str())
                            .unwrap_or_else(|| {
                                path.file_name()
                                    .and_then(|n| n.to_str())
                                    .unwrap_or("Project")
                            });
                        ui.label(RichText::new(name).strong())
                            .on_hover_text(path.display().to_string());
                        ui.add_space(10.);
                        let projects = snapshot.catalog.subprojects(&path);
                        project_grid(ui, &projects, &mut state, &mut previews);
                        selection_bar(ui, studio, &mut state.right, false);
                        let documents = snapshot.catalog.documents_in(&path);
                        document_grid(
                            ui,
                            studio,
                            &documents,
                            &mut state.right,
                            &mut previews,
                            false,
                        );
                        if documents.is_empty() && projects.is_empty() {
                            quiet(ui, "This project's artwork will appear here.");
                        }
                        if ui.small_button("Edit brand…").clicked() {
                            studio.libraries.choose_folder(&studio.swap_id, path);
                            state.brand = true;
                        }
                    } else {
                        let projects: Vec<_> = snapshot.catalog.projects.iter().collect();
                        project_grid(ui, &projects, &mut state, &mut previews);
                        if projects.is_empty() && !snapshot.scanning {
                            quiet(ui, "Folders with .omabrand appear here.");
                        }
                    }
                    if snapshot.scanning {
                        ui.spinner();
                    }
                });
            },
        );
    }
    previews.finish(&ctx);
    if studio.updates.latest.is_some() {
        let size = vec2(182., 42.);
        let rect = Rect::from_min_size(full.right_bottom() - size - vec2(20., 20.), size);
        ui.scope_builder(egui::UiBuilder::new().max_rect(rect), |ui| {
            if ui
                .add_sized(
                    size,
                    Button::new(RichText::new("↑  Update Available").strong()).fill(accent()),
                )
                .clicked()
            {
                studio.settings_page = 1;
                studio.show_preferences = true;
            }
        });
    }
    dialogs(&ctx, studio, &mut state);
    ctx.data_mut(|d| d.insert_temp(Id::new(STATE), state));
}

fn modes() -> [(Persona, &'static str); 5] {
    [
        (Persona::Design, "Vector"),
        (Persona::Pixel, "Raster"),
        (Persona::Layout, "Layout"),
        (Persona::Photo, "Photo"),
        (Persona::Motion, "Motion"),
    ]
}
fn quiet(ui: &mut Ui, text: &str) {
    ui.add_space(22.);
    ui.label(RichText::new(text).color(fg_weak()));
    ui.add_space(12.);
}

fn browser_panel(ui: &mut Ui, id: &'static str, body: impl FnOnce(&mut Ui)) {
    let height = ui.available_height();
    let rect = ui.available_rect_before_wrap();
    gradient_rect(ui, rect, 20., welcome_light(), welcome_dark(), false);
    Frame::new()
        .fill(egui::Color32::TRANSPARENT)
        .corner_radius(20.)
        .inner_margin(Margin::same(14))
        .show(ui, |ui| {
            ui.set_min_height((height - 28.).max(1.));
            ScrollArea::vertical()
                .id_salt(id)
                .auto_shrink([false, false])
                .max_height((height - 28.).max(1.))
                .show(ui, body);
        });
}

fn center(ui: &mut Ui, studio: &mut Studio, state: &mut State) {
    ui.add_space(22.);
    logo(ui);
    ui.vertical_centered(|ui| {
        ui.label(
            RichText::new(format!("alpha-{}", env!("CARGO_PKG_VERSION")))
                .small()
                .color(fg_weak()),
        );
    });
    ui.add_space(16.);
    for (persona, label) in [
        (Persona::Design, "Vector"),
        (Persona::Pixel, "Raster"),
        (Persona::Layout, "Layout"),
        (Persona::Photo, "Photo"),
    ] {
        let height = 74.;
        let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), height), Sense::hover());
        gradient_rect(ui, rect, 14., welcome_light(), welcome_dark(), false);
        ui.painter().rect_stroke(
            rect,
            14.,
            Stroke::new(1., border()),
            egui::StrokeKind::Inside,
        );
        let extras = match persona {
            Persona::Photo => 2,
            Persona::Pixel => 0,
            _ => 1,
        };
        let main = Rect::from_min_max(
            rect.min,
            pos2(rect.right() - extras as f32 * 54., rect.bottom()),
        );
        let response = ui.interact(main, Id::new(("welcome-new", persona)), Sense::click());
        if response.hovered() {
            gradient_rect(
                ui,
                main.shrink(1.),
                13.,
                bg_widget_hover(),
                welcome_dark(),
                false,
            );
        }
        ui.painter().text(
            main.left_center() + vec2(21., 0.),
            Align2::CENTER_CENTER,
            "+",
            egui::FontId::proportional(28.),
            fg_weak(),
        );
        ui.painter().text(
            main.left_center() + vec2(45., 0.),
            Align2::LEFT_CENTER,
            label,
            egui::FontId::proportional(23.),
            fg(),
        );
        response.widget_info(|| {
            egui::WidgetInfo::labeled(egui::WidgetType::Button, true, format!("New {label}"))
        });
        if response.clicked() {
            match persona {
                Persona::Photo => studio.switch_persona(persona),
                Persona::Pixel => state.size = Some(persona),
                _ => crate::ui::templates::open(ui.ctx(), studio, persona),
            }
        }
        for i in 0..extras {
            let target = Rect::from_min_size(
                pos2(rect.right() - (extras - i) as f32 * 54., rect.top()),
                vec2(54., height),
            );
            let response = ui.interact(
                target,
                Id::new(("welcome-extra", persona, i)),
                Sense::click(),
            );
            let is_folder = persona == Persona::Photo && i == 0;
            let tip = if is_folder {
                "Open photo folder"
            } else if persona == Persona::Photo {
                "Open image"
            } else {
                "Blank document size"
            };
            let tint = if response.hovered() {
                bg_widget_hover()
            } else {
                welcome_dark()
            };
            gradient_rect(ui, target.shrink(1.), 12., welcome_light(), tint, true);
            ui.painter().text(
                target.center(),
                Align2::CENTER_CENTER,
                if is_folder {
                    icons::ph::FOLDER_OPEN
                } else if persona == Persona::Photo {
                    icons::ph::IMAGES
                } else {
                    "\u{E704}"
                },
                icons::font(24.),
                fg_weak(),
            );
            response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, tip));
            if response.on_hover_text(tip).clicked() {
                if persona != Persona::Photo {
                    state.size = Some(persona);
                } else if is_folder {
                    studio.request_file_dialog(crate::project::dialog_folder, |_, studio, path| {
                        studio.switch_persona(Persona::Photo);
                        studio.photo.set_folder(&path.to_string_lossy());
                    });
                } else {
                    studio.request_file_dialog(crate::project::dialog_photo, |_, studio, path| {
                        studio.switch_persona(Persona::Photo);
                        studio.photo.import_file(&path);
                    });
                }
            }
        }
        ui.add_space(9.);
    }
    let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), 74.), Sense::click());
    gradient_rect(
        ui,
        rect,
        14.,
        if response.hovered() {
            bg_widget_hover()
        } else {
            welcome_light()
        },
        welcome_dark(),
        false,
    );
    ui.painter().rect_stroke(
        rect,
        14.,
        Stroke::new(1., border()),
        egui::StrokeKind::Inside,
    );
    ui.painter().text(
        rect.left_center() + vec2(21., 0.),
        Align2::CENTER_CENTER,
        "+",
        egui::FontId::proportional(28.),
        fg_weak(),
    );
    ui.painter().text(
        rect.left_center() + vec2(45., 0.),
        Align2::LEFT_CENTER,
        "Project",
        egui::FontId::proportional(23.),
        fg(),
    );
    response
        .widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, "New Project"));
    if response.clicked() {
        state.brand = true;
    }
    ui.add_space(16.);
    let links = [
        (
            "newspaper-clipping",
            format!("What's new in {}", env!("CARGO_PKG_VERSION")),
            format!(
                "https://omadesign.app/updates/{}",
                env!("CARGO_PKG_VERSION")
            ),
        ),
        (
            "book",
            "Read the docs".into(),
            "https://omadesign.app/docs/".into(),
        ),
        (
            "bug-beetle",
            "File a bug report".into(),
            "https://github.com/michaelmonetized/omadesign/issues/new".into(),
        ),
    ];
    for (glyph, label, url) in links {
        link(ui, glyph, &label, &url);
    }
    for (glyph, label, purpose) in [
        ("sparkle", "Learn with AI", crate::agent::Purpose::Learn),
        ("robot", "Create with agent", crate::agent::Purpose::Create),
    ] {
        if action(ui, glyph, label) {
            agent::open(ui.ctx(), purpose, state.project.clone());
        }
    }
    link(
        ui,
        "git-branch",
        "Contribute",
        "https://omadesign.app/docs/contributing",
    );
    link(
        ui,
        "discord-logo",
        "Join the conversation",
        "https://discord.gg/ejkZS2RBx",
    );
    if crate::cloud::signed_in(&studio.cloud_identity) {
        link(ui, "cloud", "Open cloud", "https://omadesign.app/cloud");
    } else if action(ui, "cloud", "Sign up for cloud") {
        studio.cloud_modal = crate::app::CloudModal::SignIn;
        studio.connect_cloud();
    }
    ui.add_space(16.);
}

fn action(ui: &mut Ui, glyph: &'static str, label: &str) -> bool {
    let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), 32.), Sense::click());
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, label));
    if response.hovered() || response.has_focus() {
        ui.painter().rect_filled(rect, 6., bg_widget_hover());
    }
    let source: &[u8] = match glyph {
        "newspaper-clipping" => {
            include_bytes!("../../../assets/phosphor/welcome/newspaper-clipping.svg")
        }
        "book" => include_bytes!("../../../assets/phosphor/welcome/book.svg"),
        "bug-beetle" => include_bytes!("../../../assets/phosphor/welcome/bug-beetle.svg"),
        "sparkle" => include_bytes!("../../../assets/phosphor/welcome/sparkle.svg"),
        "robot" => include_bytes!("../../../assets/phosphor/welcome/robot.svg"),
        "git-branch" => include_bytes!("../../../assets/phosphor/welcome/git-branch.svg"),
        "discord-logo" => include_bytes!("../../../assets/phosphor/welcome/discord-logo.svg"),
        _ => include_bytes!("../../../assets/phosphor/welcome/cloud.svg"),
    };
    let white = String::from_utf8_lossy(source).replace("currentColor", "#ffffff");
    if let Some(texture) = icons::svg_texture(ui, glyph, white.as_bytes()) {
        let size = texture.size_vec2();
        let size = size * (17. / size.x.max(size.y));
        ui.painter().image(
            texture.id(),
            Rect::from_center_size(rect.left_center() + vec2(20., 0.), size),
            Rect::from_min_max(pos2(0., 0.), pos2(1., 1.)),
            fg_weak(),
        );
    }
    ui.painter().text(
        rect.left_center() + vec2(43., 0.),
        Align2::LEFT_CENTER,
        label,
        egui::FontId::proportional(14.),
        fg_weak(),
    );
    response
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .clicked()
}
fn link(ui: &mut Ui, glyph: &'static str, label: &str, url: &str) {
    if action(ui, glyph, label) {
        ui.ctx().open_url(egui::OpenUrl::new_tab(url));
    }
}

fn logo(ui: &mut Ui) {
    ui.vertical_centered(|ui| {
        if let Some(texture) = icons::svg_texture(
            ui,
            "welcome-logo-058",
            include_bytes!("../../../assets/omadesign.svg"),
        ) {
            let size = texture.size_vec2();
            let scale = (225. / size.x).min(127.5 / size.y);
            ui.image((texture.id(), size * scale));
        }
    });
}

fn welcome_dark() -> egui::Color32 {
    [bg_window(), bg_panel(), p().bg_extreme]
        .into_iter()
        .min_by(|a, b| a.intensity().total_cmp(&b.intensity()))
        .unwrap()
}
fn welcome_light() -> egui::Color32 {
    if bg_window().intensity() > bg_panel().intensity() {
        bg_window()
    } else {
        bg_panel()
    }
}

/// A rounded mesh interpolates theme colors without divider lines or hard bands.
fn gradient_rect(
    ui: &Ui,
    rect: Rect,
    radius: f32,
    light: egui::Color32,
    dark: egui::Color32,
    horizontal: bool,
) {
    if !ui.is_rect_visible(rect) {
        return;
    }
    let r = radius.min(rect.width() * 0.5).min(rect.height() * 0.5);
    let color = |p: egui::Pos2| {
        let t = if horizontal {
            (p.x - rect.left()) / rect.width()
        } else {
            (p.y - rect.top()) / rect.height()
        };
        egui::Color32::from(egui::Rgba::from(light) * (1. - t) + egui::Rgba::from(dark) * t)
    };
    let mut mesh = egui::Mesh::default();
    mesh.colored_vertex(rect.center(), color(rect.center()));
    for (c, start) in [
        (rect.right_top() + vec2(-r, r), -90_f32),
        (rect.right_bottom() + vec2(-r, -r), 0.),
        (rect.left_bottom() + vec2(r, -r), 90.),
        (rect.left_top() + vec2(r, r), 180.),
    ] {
        for step in 0..=8 {
            let a = (start + step as f32 * 90. / 8.).to_radians();
            let p = c + vec2(a.cos(), a.sin()) * r;
            mesh.colored_vertex(p, color(p));
        }
    }
    for i in 1..mesh.vertices.len() {
        mesh.add_triangle(
            0,
            i as u32,
            if i + 1 == mesh.vertices.len() {
                1
            } else {
                (i + 1) as u32
            },
        );
    }
    ui.painter().add(egui::Shape::mesh(mesh));
}

fn selection_bar(ui: &mut Ui, studio: &mut Studio, selection: &mut Selection, recovered: bool) {
    if selection.paths.is_empty() {
        return;
    }
    ui.horizontal_wrapped(|ui| {
        ui.label(format!("{} selected", selection.paths.len()));
        if ui
            .button(if recovered {
                "Recover selected"
            } else {
                "Open selected"
            })
            .clicked()
        {
            let paths = std::mem::take(&mut selection.paths).into_iter().collect();
            if recovered {
                studio.recover_paths(paths);
            } else {
                studio.open_paths(paths);
            }
        }
        if ui.small_button("Clear").clicked() {
            selection.paths.clear();
        }
    });
    ui.add_space(8.);
}

fn document_grid(
    ui: &mut Ui,
    studio: &mut Studio,
    entries: &[&catalog::DocumentEntry],
    selection: &mut Selection,
    previews: &mut catalog::Previews,
    recovered: bool,
) {
    let gap = 10.;
    let columns = ((ui.available_width() + gap) / 150.).floor().clamp(1., 3.) as usize;
    let width = (ui.available_width() - gap * (columns - 1) as f32) / columns as f32;
    let top = ui.cursor().top();
    let left = ui.max_rect().left();
    let mut bottoms = vec![top; columns];
    for entry in entries {
        let column = (0..columns)
            .min_by(|a, b| bottoms[*a].total_cmp(&bottoms[*b]))
            .unwrap();
        let aspect = previews.aspect(entry).unwrap_or(entry.aspect).max(0.0001);
        let height = width / aspect;
        let rect = Rect::from_min_size(
            pos2(left + column as f32 * (width + gap), bottoms[column]),
            vec2(width, height),
        );
        bottoms[column] += height + gap;
        if !ui.is_rect_visible(rect) {
            continue;
        }
        let response = ui.interact(
            rect,
            Id::new(("welcome-file", recovered, &entry.path, ui.id())),
            Sense::click(),
        );
        let selected = selection.paths.contains(&entry.path);
        ui.painter().rect_filled(rect, 6., bg_window());
        let mut preview_error = entry.error.clone();
        match previews.image(ui.ctx(), entry) {
            Some(Ok(texture)) => {
                ui.painter().image(
                    texture.id(),
                    rect,
                    Rect::from_min_max(pos2(0., 0.), pos2(1., 1.)),
                    egui::Color32::WHITE,
                );
            }
            Some(Err(error)) => {
                preview_error = Some(error);
                ui.painter().text(
                    rect.center(),
                    Align2::CENTER_CENTER,
                    "Preview unavailable",
                    egui::FontId::proportional(12.),
                    fg_weak(),
                );
            }
            None => {
                ui.painter().text(
                    rect.center(),
                    Align2::CENTER_CENTER,
                    "…",
                    egui::FontId::proportional(18.),
                    fg_weak(),
                );
            }
        }
        if selected || response.hovered() || response.has_focus() {
            ui.painter().rect_stroke(
                rect,
                5.,
                Stroke::new(
                    if selected { 2. } else { 1. },
                    if selected { accent() } else { fg_weak() },
                ),
                egui::StrokeKind::Inside,
            );
            let label = Rect::from_min_max(
                pos2(rect.left(), (rect.bottom() - 32.).max(rect.top())),
                rect.max,
            );
            ui.painter()
                .rect_filled(label, 5., bg_panel().gamma_multiply(0.94));
            let painter = ui.painter().with_clip_rect(label.shrink(6.));
            painter.text(
                label.left_center() + vec2(8., 0.),
                Align2::LEFT_CENTER,
                &entry.name,
                egui::FontId::proportional(12.),
                fg(),
            );
        }
        response.widget_info(|| {
            egui::WidgetInfo::selected(egui::WidgetType::Button, true, selected, &entry.name)
        });
        let clicked = response.clicked();
        response.on_hover_text(match preview_error {
            Some(error) => format!("{}\n{error}", entry.path.display()),
            None => entry.path.display().to_string(),
        });
        if clicked && selection.click(&entry.path, ui.input(|i| i.modifiers.shift)) {
            if recovered {
                studio.recover_swap(entry.path.clone());
            } else {
                studio.open_path(entry.path.clone());
            }
        }
    }
    let bottom = bottoms.into_iter().fold(top, f32::max);
    ui.allocate_rect(
        Rect::from_min_max(pos2(left, top), pos2(left + ui.available_width(), bottom)),
        Sense::hover(),
    );
}

fn project_grid(
    ui: &mut Ui,
    projects: &[&catalog::ProjectEntry],
    state: &mut State,
    previews: &mut catalog::Previews,
) {
    let columns = ((ui.available_width() + 10.) / 150.).floor().clamp(1., 3.) as usize;
    let width = (ui.available_width() - 10. * (columns - 1) as f32) / columns as f32;
    for row in projects.chunks(columns) {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 10.;
            for project in row {
                let (rect, response) = ui.allocate_exact_size(vec2(width, width), Sense::click());
                if !ui.is_rect_visible(rect) {
                    continue;
                }
                let outline = rect.shrink(7.);
                let images: Vec<_> = previews
                    .project_images(ui.ctx(), project)
                    .into_iter()
                    .filter_map(|image| image.and_then(Result::ok))
                    .collect();
                for (i, texture) in images.iter().take(3).enumerate() {
                    let size = texture.size_vec2();
                    let scale =
                        (outline.width() * 0.68 / size.x).min(outline.height() * 0.65 / size.y);
                    let card = Rect::from_center_size(
                        outline.center() + vec2((i as f32 - 1.) * 8., (i as f32 - 1.) * 3.),
                        size * scale,
                    );
                    let angle = (i as f32 - 1.) * 0.12;
                    let rot = egui::emath::Rot2::from_angle(angle);
                    let mut mesh = egui::Mesh::with_texture(texture.id());
                    for (point, uv) in [
                        (card.left_top(), pos2(0., 0.)),
                        (card.right_top(), pos2(1., 0.)),
                        (card.right_bottom(), pos2(1., 1.)),
                        (card.left_bottom(), pos2(0., 1.)),
                    ] {
                        mesh.vertices.push(egui::epaint::Vertex {
                            pos: card.center() + rot * (point - card.center()),
                            uv,
                            color: egui::Color32::WHITE,
                        });
                    }
                    mesh.indices = vec![0, 1, 2, 0, 2, 3];
                    ui.painter().add(egui::Shape::mesh(mesh));
                }
                let source = include_str!("../../../assets/project-folder.svg")
                    .replace("#BAC2DE", "#ffffff")
                    .replace("stroke-width=\"4.00\"", "stroke-width=\"10.00\"");
                if let Some(texture) = icons::svg_texture(ui, "project-folder", source.as_bytes()) {
                    ui.painter().image(
                        texture.id(),
                        outline,
                        Rect::from_min_max(pos2(0., 0.), pos2(1., 1.)),
                        if response.hovered() {
                            accent()
                        } else {
                            fg_weak()
                        },
                    );
                }
                if images.is_empty() {
                    ui.painter().text(
                        outline.center(),
                        Align2::CENTER_CENTER,
                        icons::ph::FOLDER_OPEN,
                        icons::font(28.),
                        fg_weak(),
                    );
                }
                if response.hovered() || response.has_focus() {
                    let label =
                        Rect::from_min_max(pos2(rect.left(), rect.bottom() - 26.), rect.max);
                    ui.painter().rect_filled(label, 4., bg_panel());
                    ui.painter().with_clip_rect(label).text(
                        label.center(),
                        Align2::CENTER_CENTER,
                        &project.name,
                        egui::FontId::proportional(12.),
                        fg(),
                    );
                }
                response.widget_info(|| {
                    egui::WidgetInfo::labeled(egui::WidgetType::Button, true, &project.name)
                });
                if response
                    .on_hover_text(project.path.display().to_string())
                    .clicked()
                {
                    state.project = Some(project.path.clone());
                    state.right.paths.clear();
                }
            }
        });
        ui.add_space(10.);
    }
}

fn dialogs(ctx: &egui::Context, studio: &mut Studio, state: &mut State) {
    if let Some(persona) = state.size {
        let mut close = false;
        let response = egui::Modal::new(Id::new("welcome-size")).show(ctx, |ui| {
            ui.set_width((ctx.content_rect().width() - 100.).clamp(300., 850.));
            ui.heading(format!(
                "New {} document",
                modes().iter().find(|(m, _)| *m == persona).unwrap().1
            ));
            ui.add_space(12.);
            if ctx.content_rect().height() < 600. {
                ScrollArea::vertical()
                    .id_salt("welcome-size-scroll")
                    .max_height((ctx.content_rect().height() - 110.).max(120.))
                    .show(ui, |ui| {
                        super::new_page(ui, studio);
                    });
            } else {
                super::new_page(ui, studio);
            }
            if ui.button("Cancel").clicked() {
                close = true;
            }
        });
        if !studio.show_welcome {
            studio.switch_persona(persona);
            studio.doc.workspace = Some(persona);
            if persona == Persona::Pixel {
                studio.active_layer = studio
                    .doc
                    .layers
                    .iter()
                    .position(|layer| layer.kind.pixels().is_some());
            }
            state.size = None;
        } else if close || response.should_close() {
            state.size = None;
        }
    }
    if state.brand {
        let mut open = true;
        egui::Window::new("Project brand")
            .id(Id::new("welcome-brand-editor"))
            .open(&mut open)
            .default_size(vec2(
                700_f32.min(ctx.content_rect().width() - 50.),
                600_f32.min(ctx.content_rect().height() - 70.),
            ))
            .max_width((ctx.content_rect().width() - 50.).max(220.))
            .max_height((ctx.content_rect().height() - 70.).max(150.))
            .show(ctx, |ui| {
                studio.libraries.sidebar = crate::app::libraries::Sidebar::Brand;
                crate::ui::library::show(ui, studio);
            });
        if !open {
            state.brand = false;
            catalog::refresh(ctx);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn center_frame(
        ctx: &egui::Context,
        studio: &mut Studio,
        state: &mut State,
        events: Vec<egui::Event>,
    ) -> egui::FullOutput {
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, vec2(960., 1080.))),
                events,
                ..Default::default()
            },
            |ui| {
                ui.set_width(360.);
                center(ui, studio, state);
                dialogs(ctx, studio, state);
            },
        );
        output.textures_delta.clear();
        output
    }

    fn click(ctx: &egui::Context, studio: &mut Studio, state: &mut State, position: egui::Pos2) {
        for pressed in [true, false] {
            center_frame(
                ctx,
                studio,
                state,
                vec![
                    egui::Event::PointerMoved(position),
                    egui::Event::PointerButton {
                        pos: position,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
            );
        }
    }

    #[test]
    fn creation_targets_choose_templates_sizes_or_photo_without_a_motion_start() {
        for persona in [
            Persona::Design,
            Persona::Pixel,
            Persona::Layout,
            Persona::Photo,
        ] {
            let ctx = egui::Context::default();
            crate::ui::theme::apply(&ctx);
            let mut studio = Studio::new();
            let mut state = State::default();
            center_frame(&ctx, &mut studio, &mut state, vec![]);
            center_frame(&ctx, &mut studio, &mut state, vec![]);
            assert!(
                ctx.read_response(Id::new(("welcome-new", Persona::Motion)))
                    .is_none()
            );
            let position = ctx
                .read_response(Id::new(("welcome-new", persona)))
                .unwrap()
                .rect
                .center();
            click(&ctx, &mut studio, &mut state, position);
            match persona {
                Persona::Design | Persona::Layout => {
                    assert!(studio.show_templates);
                    assert!(studio.show_welcome);
                    assert!(state.size.is_none());
                }
                Persona::Pixel => {
                    assert_eq!(state.size, Some(persona));
                    assert!(!studio.show_templates);
                    assert!(studio.show_welcome);
                }
                Persona::Photo => {
                    assert!(!studio.show_welcome);
                    assert_eq!(studio.persona, persona);
                    assert!(studio.doc.artboards.is_empty());
                }
                _ => unreachable!(),
            }
        }
    }

    #[test]
    fn blank_size_dialog_creates_a_paintable_raster_at_the_selected_size() {
        let ctx = egui::Context::default();
        crate::ui::theme::apply(&ctx);
        let mut studio = Studio::new();
        studio.custom_w = 64.;
        studio.custom_h = 48.;
        studio.custom_dpi = 96.;
        let mut state = State {
            size: Some(Persona::Pixel),
            ..Default::default()
        };
        center_frame(&ctx, &mut studio, &mut state, vec![]);
        for _ in 0..4 {
            center_frame(&ctx, &mut studio, &mut state, vec![]);
        }
        let output = center_frame(&ctx, &mut studio, &mut state, vec![]);
        let position = output
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Text(text) if text.galley.job.text == "Create document" => {
                    Some(text.pos + text.galley.size() * 0.5)
                }
                _ => None,
            })
            .unwrap_or_else(|| {
                panic!(
                    "Create document button missing: {:?}",
                    output
                        .shapes
                        .iter()
                        .filter_map(|shape| match &shape.shape {
                            egui::Shape::Text(text) => Some(text.galley.job.text.clone()),
                            _ => None,
                        })
                        .collect::<Vec<_>>()
                )
            });
        click(&ctx, &mut studio, &mut state, position);
        assert!(!studio.show_welcome);
        assert_eq!(studio.persona, Persona::Pixel);
        assert_eq!(studio.doc.workspace, Some(Persona::Pixel));
        assert_eq!(
            (studio.doc.width, studio.doc.height, studio.doc.dpi),
            (64., 48., 96.)
        );
        assert_eq!(studio.tool, crate::tools::Tool::Brush);
        assert!(studio.raster_target().is_some());
    }

    #[test]
    fn shift_starts_selection_and_following_clicks_toggle_without_opening() {
        let a = Path::new("/tmp/a.oma");
        let b = Path::new("/tmp/b.oma");
        let mut selection = Selection::default();
        assert!(selection.click(a, false));
        assert!(!selection.click(a, true));
        assert!(!selection.click(b, false));
        assert_eq!(selection.paths.len(), 2);
        assert!(!selection.click(a, false));
        assert!(!selection.click(b, false));
        assert!(selection.paths.is_empty());
        assert!(selection.click(b, false));
    }
}
