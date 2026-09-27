//! Persistent ACP conversation beside the live canvas.
use crate::{
    agent::{Purpose, runtime::Command, workspace::Workspace},
    app::Studio,
};
use eframe::egui::{self, Id, RichText};
use std::path::PathBuf;

const OPEN: &str = "design-agent-open";
const VISIBLE: &str = "design-agent-visible";

fn tool_title(title: &str) -> String {
    let name = title
        .strip_prefix("mcp.omadesign.")
        .or_else(|| title.strip_prefix("mcp__omadesign__"));
    match name {
        Some("get_document") => "Reading the canvas",
        Some("get_objects") => "Inspecting objects",
        Some("list_fonts") => "Finding typography",
        Some("get_canvas_snapshot") => "Reviewing the design",
        Some("add_shape") => "Adding to the design",
        Some("update_shape") => "Refining an object",
        Some("create_layer" | "update_layer") => "Organizing layers",
        Some("set_effects") => "Styling an object",
        Some("remove_shapes") => "Removing objects",
        Some("get_documentation") => "Reading the guide",
        _ if title == "Guardian Review" => "Checking the change",
        _ => title,
    }
    .into()
}
#[derive(Clone)]
struct Open {
    purpose: Purpose,
    directory: Option<PathBuf>,
}

pub(super) fn open(ctx: &egui::Context, purpose: Purpose, directory: Option<PathBuf>) {
    ctx.data_mut(|d| {
        d.insert_temp(Id::new(OPEN), Open { purpose, directory });
        d.insert_temp(Id::new(VISIBLE), true);
    });
}
pub(super) fn is_open(ctx: &egui::Context) -> bool {
    ctx.data(|d| d.get_temp::<bool>(Id::new(VISIBLE)).unwrap_or(false))
}

pub(super) fn tick(ctx: &egui::Context, studio: &mut Studio) {
    let mut agent = std::mem::take(&mut studio.agent);
    let open = ctx.data_mut(|d| {
        let request = d.get_temp::<Open>(Id::new(OPEN));
        d.remove::<Open>(Id::new(OPEN));
        request
    });
    if let Some(request) = open {
        agent.load();
        agent.visible = true;
        agent.focus_prompt = true;
        if !agent.busy && !agent.connecting && agent.purpose != request.purpose {
            agent.purpose = request.purpose;
            agent.new_thread(studio);
            agent.request.clear();
        }
        if agent
            .thread
            .as_ref()
            .is_none_or(|t| t.session_id.is_none() && t.messages.is_empty())
        {
            agent.purpose = request.purpose;
            if let Some(dir) = request.directory {
                agent.settings.directory = dir;
                agent.sync_fields();
            }
        }
    }
    agent.poll(studio, ctx);
    ctx.data_mut(|d| d.insert_temp(Id::new(VISIBLE), agent.visible));
    studio.agent = agent;
}

pub(super) fn button(ui: &mut egui::Ui, studio: &mut Studio) {
    if ui
        .selectable_label(studio.agent.visible, "Agent")
        .on_hover_text("Design live with your local AI agent")
        .clicked()
    {
        if studio.agent.visible {
            studio.agent.visible = false;
            ui.ctx()
                .data_mut(|d| d.insert_temp(Id::new(VISIBLE), false));
        } else {
            studio.agent.load();
            studio.agent.visible = true;
            studio.agent.focus_prompt = true;
            ui.ctx().data_mut(|d| d.insert_temp(Id::new(VISIBLE), true));
        }
    }
}

fn connection_settings(ui: &mut egui::Ui, agent: &mut Workspace) {
    ui.add_enabled_ui(!agent.busy&&!agent.connecting&&!agent.ready,|ui|{
        let mut chosen=None;
        egui::ComboBox::from_id_salt("agent-provider").selected_text(&agent.settings.profile.name).show_ui(ui,|ui|{
            for profile in crate::agent::config::presets(){if ui.selectable_label(profile.name==agent.settings.profile.name,&profile.name).clicked(){chosen=Some(profile);}}
        });
        if let Some(profile)=chosen{agent.settings.profile=profile;agent.sync_fields();}
        if agent.settings.profile.command=="npx"{ui.small("The ACP adapter is downloaded on first connection. Your local agent sign-in is reused.");}
        ui.label("Project folder");ui.add(egui::TextEdit::singleline(&mut agent.config_directory).desired_width(f32::INFINITY));
        egui::CollapsingHeader::new("Agent command").show(ui,|ui|{
            ui.label("Executable");ui.add(egui::TextEdit::singleline(&mut agent.settings.profile.command).desired_width(f32::INFINITY));
            ui.label("Arguments (JSON array)");ui.add(egui::TextEdit::multiline(&mut agent.config_args).desired_rows(2).desired_width(f32::INFINITY));
            ui.small("Any local ACP agent speaking JSON-RPC over stdio.");
        });
    });
    ui.checkbox(&mut agent.settings.live_edits, "Allow live canvas edits");
    ui.checkbox(&mut agent.follow_canvas, "Keep the design in view");
}

fn session_options(ui: &mut egui::Ui, agent: &mut Workspace) {
    let options = agent.metadata["configOptions"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    ui.add_enabled_ui(agent.ready && !agent.busy, |ui| {
        if !options.is_empty() {
            for option in options.iter().filter(|v| v["type"] == "select") {
                let Some(id) = option["id"].as_str() else {
                    continue;
                };
                let mut choices = vec![];
                for v in option["options"].as_array().into_iter().flatten() {
                    if let Some(group) = v["options"].as_array() {
                        choices.extend(group.iter().cloned());
                    } else {
                        choices.push(v.clone());
                    }
                }
                let current = option["currentValue"].as_str().unwrap_or_default();
                let label = choices
                    .iter()
                    .find(|v| v["value"] == current)
                    .and_then(|v| v["name"].as_str())
                    .unwrap_or(current);
                egui::ComboBox::from_id_salt(("agent-config", id))
                    .selected_text(label)
                    .width(220.0)
                    .show_ui(ui, |ui| {
                        for choice in choices {
                            let value = choice["value"].as_str().unwrap_or_default();
                            if ui
                                .selectable_label(
                                    value == current,
                                    choice["name"].as_str().unwrap_or(value),
                                )
                                .clicked()
                            {
                                if let Some(c) = &agent.connection {
                                    let _ = c.send(Command::Config {
                                        id: id.into(),
                                        value: value.into(),
                                    });
                                }
                            }
                        }
                    })
                    .response
                    .on_hover_text(option["name"].as_str().unwrap_or(id));
            }
        } else {
            for (key, available, current, label) in [
                ("models", "availableModels", "currentModelId", "model"),
                ("modes", "availableModes", "currentModeId", "mode"),
            ] {
                let choices = agent.metadata[key][available]
                    .as_array()
                    .cloned()
                    .unwrap_or_default();
                if choices.is_empty() {
                    continue;
                }
                let active = agent.metadata[key][current]
                    .as_str()
                    .unwrap_or_default()
                    .to_owned();
                let display = choices
                    .iter()
                    .find(|v| v[if key == "models" { "modelId" } else { "id" }] == active)
                    .and_then(|v| v["name"].as_str())
                    .unwrap_or(&active);
                egui::ComboBox::from_id_salt(("agent-session", key))
                    .selected_text(display)
                    .width(220.0)
                    .show_ui(ui, |ui| {
                        for choice in choices {
                            let id = choice[if key == "models" { "modelId" } else { "id" }]
                                .as_str()
                                .unwrap_or_default();
                            if ui
                                .selectable_label(
                                    id == active,
                                    choice["name"].as_str().unwrap_or(id),
                                )
                                .clicked()
                            {
                                if let Some(c) = &agent.connection {
                                    let _ = c.send(if label == "model" {
                                        Command::Model(id.into())
                                    } else {
                                        Command::Mode(id.into())
                                    });
                                }
                            }
                        }
                    });
            }
        }
    });
}

fn body(ui: &mut egui::Ui, studio: &mut Studio, agent: &mut Workspace, height: f32) {
    let start_y = ui.cursor().top();
    ui.horizontal(|ui| {
        if ui
            .add_enabled(!agent.busy && !agent.connecting, egui::Button::new("New"))
            .clicked()
        {
            agent.new_thread(studio);
            agent.focus_prompt = true;
        }
        if ui.selectable_label(agent.show_history, "History").clicked() {
            agent.refresh_history();
            agent.show_history = !agent.show_history;
        }
        if ui
            .selectable_label(agent.show_settings, "Connection")
            .clicked()
        {
            agent.show_settings = !agent.show_settings;
        }
    });
    ui.separator();
    if agent.show_settings || agent.thread.is_none() {
        egui::ScrollArea::vertical()
            .id_salt("agent-settings-scroll")
            .max_height(240.0)
            .show(ui, |ui| connection_settings(ui, agent));
    }
    if agent.show_history {
        let history = agent.history.clone();
        egui::ScrollArea::vertical()
            .id_salt("agent-history")
            .max_height(180.0)
            .show(ui, |ui| {
                if history.is_empty() {
                    ui.small("Your conversations will appear here.");
                }
                for thread in history {
                    ui.push_id(&thread.id, |ui| {
                        egui::CollapsingHeader::new(&thread.title).show(ui, |ui| {
                            ui.small(format!(
                                "{} · {}",
                                thread.settings.profile.name,
                                thread
                                    .document
                                    .as_ref()
                                    .map_or("Unsaved canvas".into(), |p| p.display().to_string())
                            ));
                            if ui
                                .add_enabled(
                                    !agent.busy && !agent.connecting,
                                    egui::Button::new("Continue in saved document"),
                                )
                                .clicked()
                            {
                                if let Err(e) = agent.restore(thread.clone(), studio) {
                                    agent.error = e;
                                }
                            }
                            for entry in &thread.messages {
                                if matches!(entry.role.as_str(), "user" | "assistant") {
                                    ui.label(RichText::new(&entry.role).strong());
                                    ui.label(&entry.text);
                                }
                            }
                        });
                    });
                }
            });
    }
    ui.horizontal(|ui| {
        if agent.connecting || agent.busy {
            ui.spinner();
        }
        ui.label(
            RichText::new(&agent.status)
                .small()
                .color(super::theme::fg_weak()),
        );
    });
    if agent.ready {
        egui::CollapsingHeader::new(format!("{} settings", agent.settings.profile.name))
            .show(ui, |ui| session_options(ui, agent));
    }
    if !agent.error.is_empty() {
        ui.label(
            RichText::new(&agent.error)
                .small()
                .color(egui::Color32::LIGHT_RED),
        );
        if !agent.ready {
            let methods = agent.initialization["authMethods"]
                .as_array()
                .cloned()
                .unwrap_or_default();
            for method in methods {
                if ui
                    .button(method["name"].as_str().unwrap_or("Sign in"))
                    .clicked()
                {
                    if let Some(c) = &agent.connection {
                        let _ = c.send(Command::Authenticate(
                            method["id"].as_str().unwrap_or_default().into(),
                        ));
                        agent.connecting = true;
                        agent.error.clear();
                    }
                }
            }
        }
    }
    if let Some((id, permission)) = agent.permissions.first().cloned() {
        egui::Frame::new()
            .fill(super::theme::bg_widget())
            .inner_margin(8)
            .show(ui, |ui| {
                ui.label(RichText::new("Permission requested").strong());
                ui.label(
                    permission["toolCall"]["title"]
                        .as_str()
                        .unwrap_or("The agent wants to use a tool"),
                );
                if let Some(input) = permission["toolCall"].get("rawInput") {
                    egui::CollapsingHeader::new("Details").show(ui, |ui| {
                        ui.label(
                            serde_json::to_string_pretty(input)
                                .unwrap_or_default()
                                .chars()
                                .take(3000)
                                .collect::<String>(),
                        );
                    });
                }
                ui.horizontal_wrapped(|ui| {
                    for option in permission["options"].as_array().into_iter().flatten() {
                        if ui
                            .button(option["name"].as_str().unwrap_or("Choose"))
                            .clicked()
                        {
                            agent.permission(
                                id.clone(),
                                option["optionId"].as_str().map(str::to_owned),
                            );
                        }
                    }
                    if ui.button("Cancel request").clicked() {
                        agent.permission(id.clone(), None);
                    }
                });
            });
    }
    let transcript_height = (height - (ui.cursor().top() - start_y) - 12.0).max(50.0);
    egui::ScrollArea::vertical().id_salt("agent-transcript").max_height(transcript_height).min_scrolled_height(transcript_height).stick_to_bottom(true).auto_shrink([false,false]).show(ui,|ui|{
                if let Some(thread)=&agent.thread{
                    for (i,entry) in thread.messages.iter().enumerate(){ui.push_id(i,|ui|{
                        match entry.role.as_str(){
                            "user"=>{ui.add_space(8.0);ui.label(RichText::new("You").strong());ui.label(&entry.text);}
                            "assistant"=>{ui.add_space(8.0);ui.label(RichText::new(&agent.settings.profile.name).strong().color(super::theme::accent()));ui.add(egui::Label::new(&entry.text).wrap().selectable(true));}
                            "tool"=>{let icon=match entry.status.as_str(){"completed"=>"✓","failed"=>"!",_=>"·"};ui.small(format!("{icon} {}",tool_title(&entry.text)));}
                            "design"=>{ui.label(RichText::new(format!("✓ Canvas · {}",entry.text)).small().color(super::theme::accent()));}
                            "plan"=>{egui::CollapsingHeader::new("Design steps").default_open(true).show(ui,|ui|{ui.small(&entry.text);});}
                            _=>{ui.label(RichText::new(&entry.text).small().color(super::theme::fg_weak()));}
                        }
                    });}
                }else{ui.add_space(12.0);ui.label("Describe a design. Watch it take shape here.");ui.small("Native shapes, editable typography, gradients and layers. Every canvas edit can be undone.");}
            });
}

pub(super) fn panel(ui: &mut egui::Ui, studio: &mut Studio) {
    if !studio.agent.visible {
        return;
    }
    let mut agent = std::mem::take(&mut studio.agent);
    egui::Panel::right("design-agent-panel")
        .resizable(true)
        .default_size(360.0)
        .size_range(290.0..=540.0)
        .frame(
            egui::Frame::new()
                .fill(super::theme::bg_panel())
                .inner_margin(egui::Margin::same(12)),
        )
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("Design with agent").strong().size(16.0));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.small_button("×").on_hover_text("Hide panel").clicked() {
                        agent.visible = false;
                    }
                });
            });
            let body_height = (ui.available_height() - 150.0).max(80.0);
            egui::ScrollArea::vertical()
                .id_salt("agent-body")
                .max_height(body_height)
                .auto_shrink([false, false])
                .show(ui, |ui| body(ui, studio, &mut agent, body_height));
            ui.separator();
            ui.horizontal(|ui| {
                ui.add_enabled_ui(!agent.busy && !agent.ready && !agent.connecting, |ui| {
                    ui.selectable_value(&mut agent.purpose, Purpose::Create, "Create");
                    ui.selectable_value(&mut agent.purpose, Purpose::Learn, "Learn");
                });
                if agent.ready && !agent.busy && ui.small_button("Disconnect").clicked() {
                    agent.disconnect();
                    agent.status = "Disconnected".into();
                }
            });
            let field = ui.add(
                egui::TextEdit::multiline(&mut agent.request)
                    .desired_rows(3)
                    .desired_width(f32::INFINITY)
                    .hint_text(if agent.purpose == Purpose::Learn {
                        "Ask about your design or Omadesign…"
                    } else {
                        "What should we make or change?"
                    }),
            );
            if agent.focus_prompt {
                field.request_focus();
                agent.focus_prompt = false;
            }
            let send_key = field.has_focus()
                && ui.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, egui::Key::Enter));
            ui.horizontal(|ui| {
                if agent.busy || agent.connecting {
                    if ui.button("Stop").clicked() {
                        agent.stop();
                    }
                } else {
                    if ui
                        .add_enabled(
                            !agent.request.trim().is_empty(),
                            egui::Button::new("Send").fill(super::theme::accent_soft()),
                        )
                        .clicked()
                        || send_key
                    {
                        let result = std::env::current_exe()
                            .map_err(|e| e.to_string())
                            .and_then(|exe| agent.send_prompt(studio, ui.ctx(), exe));
                        if let Err(e) = result {
                            agent.error = e;
                        }
                    }
                    if !agent.ready && ui.button("Connect").clicked() {
                        let result = std::env::current_exe()
                            .map_err(|e| e.to_string())
                            .and_then(|exe| agent.connect(studio, ui.ctx(), exe));
                        if let Err(e) = result {
                            agent.error = e;
                        }
                    }
                }
                ui.small("Ctrl+Enter to send");
            });
        });
    agent.persist(false);
    ui.ctx()
        .data_mut(|d| d.insert_temp(Id::new(VISIBLE), agent.visible));
    studio.agent = agent;
}

#[cfg(test)]
mod tests {
    use super::*;
    fn frame(
        ctx: &egui::Context,
        studio: &mut Studio,
        size: egui::Vec2,
        events: Vec<egui::Event>,
    ) -> Vec<(String, egui::Rect, egui::Rect)> {
        let mut out = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                events,
                ..Default::default()
            },
            |ui| panel(ui, studio),
        );
        fn collect(
            shape: &egui::Shape,
            clip: egui::Rect,
            labels: &mut Vec<(String, egui::Rect, egui::Rect)>,
        ) {
            match shape {
                egui::Shape::Text(t) => labels.push((
                    t.galley.text().into(),
                    t.galley.rect.translate(t.pos.to_vec2()),
                    clip,
                )),
                egui::Shape::Vec(shapes) => {
                    for shape in shapes {
                        collect(shape, clip, labels);
                    }
                }
                _ => (),
            }
        }
        let mut labels = vec![];
        for shape in &out.shapes {
            collect(&shape.shape, shape.clip_rect, &mut labels);
        }
        out.textures_delta.clear();
        labels
    }
    #[test]
    fn native_panel_accepts_unicode_briefs_without_editing_artwork() {
        let ctx = egui::Context::default();
        crate::ui::theme::apply(&ctx);
        let mut s = Studio::new();
        s.agent.visible = true;
        s.agent.loaded = true;
        s.agent.focus_prompt = true;
        let before = crate::project::encode(&s.doc).unwrap();
        let size = egui::vec2(960.0, 640.0);
        frame(&ctx, &mut s, size, vec![]);
        frame(&ctx, &mut s, size, vec![]);
        frame(
            &ctx,
            &mut s,
            size,
            vec![egui::Event::Text(
                "Make a poster — 你好\nwith 'quotes' & $symbols".into(),
            )],
        );
        assert_eq!(
            s.agent.request,
            "Make a poster — 你好\nwith 'quotes' & $symbols"
        );
        assert!(s.agent.connection.is_none());
        assert_eq!(crate::project::encode(&s.doc).unwrap(), before);
    }
    #[test]
    fn composer_and_stop_stay_visible_with_long_errors_and_permission_requests() {
        for size in [egui::vec2(960.0, 640.0), egui::vec2(1440.0, 900.0)] {
            let ctx = egui::Context::default();
            crate::ui::theme::apply(&ctx);
            let mut s = Studio::new();
            s.agent.visible = true;
            s.agent.loaded = true;
            s.agent.busy = true;
            s.agent.show_settings = true;
            s.agent.error =
                "Connection detail that must wrap without hiding the stop control. ".repeat(40);
            s.agent.permissions.push((serde_json::json!(1),serde_json::json!({"toolCall":{"title":"Read a project file"},"options":[{"optionId":"allow","name":"Allow once"},{"optionId":"reject","name":"Reject"}]})));
            frame(&ctx, &mut s, size, vec![]);
            let labels = frame(&ctx, &mut s, size, vec![]);
            for wanted in ["Stop", "Ctrl+Enter to send"] {
                let (_, rect, clip) = labels
                    .iter()
                    .find(|(text, _, _)| text == wanted)
                    .unwrap_or_else(|| panic!("Missing {wanted}"));
                assert!(
                    rect.bottom() <= size.y && rect.right() <= size.x,
                    "{wanted} outside window {size:?}: {rect:?}"
                );
                assert!(clip.contains(rect.center()), "{wanted} clipped");
            }
        }
    }
}
