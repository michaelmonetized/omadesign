//! Provider-scoped, agent-advertised models and reasoning controls.
use crate::{
    agent::{
        discovery::{self, OptionSet, Provider, Status},
        workspace::Workspace,
    },
    app::Studio,
};
use eframe::egui::{self, RichText};

fn label(option: &OptionSet) -> &str {
    option
        .choices
        .iter()
        .find(|c| c.id == option.current)
        .map_or(&option.current, |c| c.label.as_str())
}
fn available(agent: &Workspace) -> serde_json::Value {
    if agent.ready {
        agent.metadata.clone()
    } else {
        agent
            .discovery
            .providers
            .iter()
            .find(|p| p.profile.name == agent.settings.profile.name)
            .map_or(serde_json::Value::Null, |p| p.metadata.clone())
    }
}
fn connect(
    agent: &mut Workspace,
    studio: &Studio,
    ctx: &egui::Context,
    provider: &Provider,
    model: Option<(&OptionSet, &str)>,
) {
    if agent.settings.profile.name != provider.profile.name
        || (!agent.ready && agent.settings.profile != provider.profile)
    {
        agent.disconnect();
        agent.thread = None;
        agent.settings.profile = provider.profile.clone();
        agent.sync_fields();
        agent.new_thread(studio);
    }
    if let Some((option, value)) = model {
        agent.choose_option(option, value);
    }
    if !agent.ready && !agent.connecting {
        match std::env::current_exe()
            .map_err(|e| e.to_string())
            .and_then(|binary| agent.connect(studio, ctx, binary))
        {
            Ok(()) => (),
            Err(e) => agent.error = e,
        }
    }
}
pub(super) fn other_options(ui: &mut egui::Ui, agent: &mut Workspace) {
    ui.add_enabled_ui(agent.ready && !agent.busy && !agent.connecting, |ui| {
        for option in discovery::options(&agent.metadata)
            .into_iter()
            .filter(|o| !matches!(o.category.as_str(), "model" | "thought_level"))
        {
            ui.label(&option.name);
            dropdown(ui, agent, &option);
        }
    });
}
fn dropdown(ui: &mut egui::Ui, agent: &mut Workspace, option: &OptionSet) {
    let current = if agent.ready {
        option.current.as_str()
    } else {
        agent
            .settings
            .selections
            .get(&format!("{}/{}", agent.settings.profile.name, option.id))
            .map_or(option.current.as_str(), String::as_str)
    }
    .to_owned();
    let text = option
        .choices
        .iter()
        .find(|c| c.id == current)
        .map_or(current.as_str(), |c| c.label.as_str())
        .to_owned();
    egui::ComboBox::from_id_salt(("agent-choice", &option.id))
        .selected_text(text)
        .width(88.0)
        .show_ui(ui, |ui| {
            for choice in &option.choices {
                if ui
                    .selectable_label(choice.id == current, &choice.label)
                    .clicked()
                {
                    agent.choose_option(option, &choice.id);
                }
            }
        })
        .response
        .on_hover_text(&option.name);
}
pub(super) fn picker(ui: &mut egui::Ui, studio: &Studio, agent: &mut Workspace) {
    let metadata = available(agent);
    let options = discovery::options(&metadata);
    let model = options.iter().find(|o| o.category == "model");
    let model_text = model.map_or("Choose model", label);
    ui.add_enabled_ui(!agent.busy && !agent.connecting, |ui| {
        ui.horizontal(|ui| {
            egui::containers::menu::MenuButton::new(format!("{} ▾", model_text))
                .config(
                    egui::containers::menu::MenuConfig::new()
                        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside),
                )
                .ui(ui, |ui| {
                    let mut providers = agent.discovery.providers.clone();
                    if let Some(p) = providers
                        .iter_mut()
                        .find(|p| p.profile.name == agent.settings.profile.name)
                    {
                        if agent.ready {
                            p.status = Status::Ready;
                            p.metadata = agent.metadata.clone();
                            p.profile = agent.settings.profile.clone();
                            p.detail = "Connected to this canvas".into();
                        }
                    } else {
                        providers.push(Provider {
                            profile: agent.settings.profile.clone(),
                            status: if agent.ready {
                                Status::Ready
                            } else {
                                Status::Unavailable
                            },
                            metadata: agent.metadata.clone(),
                            detail: "Configure this ACP command in Connection".into(),
                        });
                    }
                    ui.set_width(420.0);
                    ui.horizontal(|ui| {
                        ui.add(
                            egui::TextEdit::singleline(&mut agent.discovery.search)
                                .hint_text("Search models…")
                                .desired_width(310.0),
                        );
                        if ui
                            .small_button("↻")
                            .on_hover_text("Refresh installed agents and authorization")
                            .clicked()
                        {
                            agent
                                .discovery
                                .poll(ui.ctx(), &agent.settings.directory, true);
                        }
                    });
                    ui.separator();
                    ui.horizontal_top(|ui| {
                        ui.vertical(|ui| {
                            if ui
                                .selectable_label(
                                    agent.discovery.filter.is_none()
                                        && !agent.discovery.favorites_only,
                                    "All",
                                )
                                .clicked()
                            {
                                agent.discovery.filter = None;
                                agent.discovery.favorites_only = false;
                            }
                            if ui
                                .selectable_label(agent.discovery.favorites_only, "★")
                                .on_hover_text("Favorite models")
                                .clicked()
                            {
                                agent.discovery.favorites_only = true;
                                agent.discovery.filter = None;
                            }
                            for provider in &providers {
                                if ui
                                    .selectable_label(
                                        agent.discovery.filter.as_ref()
                                            == Some(&provider.profile.name),
                                        &provider.profile.name,
                                    )
                                    .on_hover_text(format!(
                                        "{} · {}",
                                        provider.profile.name,
                                        provider.status.label()
                                    ))
                                    .clicked()
                                {
                                    agent.discovery.filter = Some(provider.profile.name.clone());
                                    agent.discovery.favorites_only = false;
                                }
                            }
                        });
                        ui.separator();
                        egui::ScrollArea::vertical()
                            .id_salt("model-results")
                            .max_height(330.0)
                            .min_scrolled_height(220.0)
                            .show(ui, |ui| {
                                ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
                                    ui.set_width(310.0);
                                    let query = agent.discovery.search.to_lowercase();
                                    let mut count = 0;
                                    for provider in &providers {
                                        if agent
                                            .discovery
                                            .filter
                                            .as_ref()
                                            .is_some_and(|f| f != &provider.profile.name)
                                        {
                                            continue;
                                        }
                                        let model = discovery::options(&provider.metadata)
                                            .into_iter()
                                            .find(|o| o.category == "model");
                                        ui.label(
                                            RichText::new(format!(
                                                "{} · {}",
                                                provider.profile.name,
                                                if agent.ready
                                                    && provider.profile.name
                                                        == agent.settings.profile.name
                                                {
                                                    "Connected"
                                                } else {
                                                    provider.status.label()
                                                }
                                            ))
                                            .small()
                                            .strong(),
                                        )
                                        .on_hover_text(&provider.detail);
                                        if let Some(model) = model {
                                            let mut last_group = String::new();
                                            for choice in &model.choices {
                                                let key = format!(
                                                    "{}/{}",
                                                    provider.profile.name, choice.id
                                                );
                                                let favorite =
                                                    agent.settings.favorites.contains(&key);
                                                if agent.discovery.favorites_only && !favorite {
                                                    continue;
                                                }
                                                if !format!(
                                                    "{} {} {}",
                                                    provider.profile.name, choice.label, choice.id
                                                )
                                                .to_lowercase()
                                                .contains(&query)
                                                {
                                                    continue;
                                                }
                                                if choice.group != last_group {
                                                    last_group = choice.group.clone();
                                                    if !last_group.is_empty() {
                                                        ui.small(&last_group);
                                                    }
                                                }
                                                count += 1;
                                                ui.horizontal(|ui| {
                                                    if ui
                                                        .small_button(if favorite {
                                                            "★"
                                                        } else {
                                                            "☆"
                                                        })
                                                        .on_hover_text("Favorite model")
                                                        .clicked()
                                                    {
                                                        if favorite {
                                                            agent
                                                                .settings
                                                                .favorites
                                                                .retain(|v| v != &key);
                                                        } else {
                                                            agent.settings.favorites.push(key);
                                                        }
                                                        agent.save_preferences();
                                                    }
                                                    let selected = provider.profile.name
                                                        == agent.settings.profile.name
                                                        && choice.id == model.current;
                                                    if ui
                                                        .add(
                                                            egui::Button::selectable(
                                                                selected,
                                                                &choice.label,
                                                            )
                                                            .truncate(),
                                                        )
                                                        .clicked()
                                                    {
                                                        connect(
                                                            agent,
                                                            studio,
                                                            ui.ctx(),
                                                            provider,
                                                            Some((&model, &choice.id)),
                                                        );
                                                        ui.close();
                                                    }
                                                });
                                            }
                                        } else if provider.status == Status::Checking {
                                            ui.spinner();
                                        } else if provider.status != Status::NotInstalled {
                                            let text = if provider.status == Status::AdapterMissing
                                            {
                                                "Set up ACP adapter"
                                            } else {
                                                "Connect / sign in"
                                            };
                                            if ui.button(text).clicked() {
                                                connect(agent, studio, ui.ctx(), provider, None);
                                                agent.show_settings = true;
                                                ui.close();
                                            }
                                        }
                                        ui.add_space(6.0);
                                    }
                                    if count == 0 {
                                        ui.small("No matching models available yet.");
                                    }
                                });
                            });
                    });
                })
                .0
                .on_hover_text(format!("Provider: {}", agent.settings.profile.name));
            if let Some(effort) = options.iter().find(|o| o.category == "thought_level") {
                dropdown(ui, agent, effort);
            } else {
                ui.label(RichText::new("Auto effort").small())
                    .on_hover_text("This agent does not advertise an effort control.");
            }
        });
    });
}
