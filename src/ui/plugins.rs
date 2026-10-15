//! Native plugin browser, parameters, tools and cancellable background execution.
use crate::{
    app::Studio,
    plugins::{self, Action, Catalog, Gesture, Output, Plugin},
    tools::{Persona, Tool},
};
use eframe::egui::{self, Id, PointerButton, Rect, Response, Ui, pos2};
use std::{
    hash::{Hash, Hasher},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};
const STATE: &str = "lua-plugins-ui";
const SCAN: &str = "lua-plugin-scan";
const RUN: &str = "lua-plugin-run";
const INSTALL: &str = "lua-plugin-install";
#[derive(Clone, Default)]
struct State {
    open: bool,
    loaded: bool,
    catalog: Catalog,
    error: String,
    category: String,
    query: String,
    chosen: Option<(Plugin, Action)>,
    params: serde_json::Map<String, serde_json::Value>,
    tool: Option<(Plugin, Action, serde_json::Value)>,
    gesture: Option<Gesture>,
    cancel: Option<Arc<AtomicBool>>,
    owner: String,
    selection: Vec<(usize, u64)>,
    events: Vec<(Plugin, Action)>,
    behaviors: bool,
}
struct Finished {
    owner: String,
    before: u64,
    selection: Vec<(usize, u64)>,
    output: Output,
}
fn state(ctx: &egui::Context) -> State {
    ctx.data(|d| d.get_temp::<State>(Id::new(STATE)))
        .unwrap_or_default()
}
fn store(ctx: &egui::Context, state: State) {
    ctx.data_mut(|d| d.insert_temp(Id::new(STATE), state));
}
pub(super) fn open(ctx: &egui::Context) {
    let mut s = state(ctx);
    s.open = true;
    store(ctx, s);
}
pub fn is_open(ctx: &egui::Context) -> bool {
    state(ctx).open
}
fn fingerprint(doc: &crate::document::Document) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    if let Ok(bytes) = serde_json::to_vec(doc) {
        bytes.hash(&mut hasher);
    }
    hasher.finish()
}
fn reload(ctx: &egui::Context) {
    super::jobs::start(ctx, SCAN, || Ok(plugins::discover()));
}
pub fn menu(ui: &mut Ui, studio: &mut Studio) {
    ui.menu_button("Plugins", |ui| {
        if ui.button("Manage plugins…").clicked() {
            let mut s = state(ui.ctx());
            s.open = true;
            store(ui.ctx(), s);
            ui.close();
        }
        let s = state(ui.ctx());
        if s.tool.is_some() && ui.button("Exit plugin tool  Esc").clicked() {
            let mut s = s;
            s.tool = None;
            s.gesture = None;
            store(ui.ctx(), s);
            ui.close();
            return;
        }
        ui.separator();
        for plugin in s.catalog.plugins.iter().filter(|p| p.enabled) {
            ui.menu_button(&plugin.name, |ui| {
                for action in plugin.actions.iter().filter(|a| a.event.is_none()) {
                    if ui.button(&action.name).clicked() {
                        choose(ui.ctx(), plugin.clone(), action.clone(), studio);
                        ui.close();
                    }
                }
            });
        }
    });
}
fn choose(ctx: &egui::Context, plugin: Plugin, action: Action, studio: &mut Studio) {
    let mut s = state(ctx);
    s.params = action
        .parameters
        .iter()
        .map(|p| (p.id.clone(), p.default.clone()))
        .collect();
    s.chosen = Some((plugin, action));
    s.open = true;
    store(ctx, s);
    studio.status = "Choose plugin parameters, then Run or Activate tool".into();
}

/// Describe the active plugin tool without starting it.
/// Args: context. Returns the plugin and action identities, or none for the normal canvas tools.
pub(super) fn keyboard_tool(ctx: &egui::Context) -> Option<(String, String, String)> {
    state(ctx).tool.map(|(plugin, action, params)| (plugin.id, action.id, params.to_string()))
}

/// Run the active plugin tool with explicit canvas coordinates.
/// Args: context, editor, captured tool identity, finite points and native modifier choices.
/// Returns a rejection for a changed tool, disabled target or running operation.
pub(super) fn keyboard_gesture(ctx: &egui::Context, studio: &Studio, expected: &(String, String, String), points: &[crate::geom::Pt], shift: bool, alt: bool) -> Result<(), String> {
    let mut s = state(ctx);
    let (plugin, action, params) = s.tool.clone().ok_or("The plugin tool is no longer active.")?;
    if (&plugin.id, &action.id) != (&expected.0, &expected.1) || params.to_string() != expected.2 { return Err("The plugin tool or parameters changed. Open Tool points again.".into()); }
    if !plugin.enabled || studio.show_welcome || studio.file_dialog_pending() || studio.updates.freezing || studio.op.is_some() || studio.type_edit.is_some() || studio.pixel_edit.is_some() || studio.free_transform.is_some() || studio.deformation.is_some() || s.gesture.is_some() || super::jobs::is_running::<Finished>(ctx, RUN) {
        return Err("Finish the current edit or plugin run first.".into());
    }
    if points.is_empty() || points.len() > 4096 || points.iter().any(|p| !p.x.is_finite() || !p.y.is_finite() || p.x.abs() > 1_000_000. || p.y.abs() > 1_000_000.) { return Err("Use 1–4096 finite canvas points within 1000000 px.".into()); }
    let gesture = Gesture { points: points.iter().map(|p| [p.x, p.y]).collect(), shift, alt, ctrl: false };
    start(ctx, studio, &mut s, plugin, action, params, Some(gesture));
    store(ctx, s);
    Ok(())
}

/// Decorate an existing plugin control with its purpose and native focus.
/// Args: UI, actual widget response and purpose. Returns no value and preserves its value semantics.
fn name(ui: &Ui, response: &Response, purpose: &str) {
    ui.ctx().accesskit_node_builder(response.id, |node| node.set_label(purpose));
    if response.has_focus() {
        response.scroll_to_me(Some(egui::Align::Center));
        ui.painter().rect_stroke(response.rect.expand(2.), 3., ui.visuals().selection.stroke, egui::StrokeKind::Outside);
    }
}

/// Draw one native plugin parameter inside its persisted action and owner scope.
/// Args: UI, parameter definition and owned JSON draft. Returns whether the draft can be run.
fn parameter_control(ui: &mut Ui, parameter: &plugins::Parameter, value: &mut serde_json::Value) -> bool {
    let label = &parameter.label;
    match parameter.kind.as_str() {
        "number" => {
            let mut number = value.as_f64().unwrap_or(0.);
            let min = parameter.min.unwrap_or(-100000.);
            let max = parameter.max.unwrap_or(100000.);
            let invalid_range = !min.is_finite() || !max.is_finite() || min > max;
            let error = if invalid_range { Some("This plugin parameter has an invalid range") }
                else if !number.is_finite() || number < min || number > max { Some("Choose a value within this parameter's range") }
                else { None };
            let range = if invalid_range { 0. ..=0. } else { min..=max };
            let result = super::fields::number(ui, &parameter.id, label, &mut number, range, "", None, super::fields::FieldState { locked: invalid_range, error, ..Default::default() });
            if result.changed { *value = number.into(); }
            !invalid_range && number.is_finite() && number >= min && number <= max
        }
        "boolean" => {
            ui.horizontal(|ui| {
                let label_response = ui.label(label);
                let mut flag = value.as_bool().unwrap_or(false);
                let response = ui.checkbox(&mut flag, "").labelled_by(label_response.id);
                name(ui, &response, label);
                if response.changed() { *value = flag.into(); }
            });
            true
        }
        _ => {
            let label_response = ui.label(label);
            let mut text = value.as_str().unwrap_or("").to_owned();
            let response = ui.add(egui::TextEdit::singleline(&mut text).id(ui.make_persistent_id("value"))).labelled_by(label_response.id);
            name(ui, &response, label);
            if response.changed() { *value = text.clone().into(); }
            let error = (parameter.kind == "color" && crate::color::Rgba::parse_hex(&text).is_none()).then_some("Enter a color such as #336699 or #33669980");
            if let Some(error) = error {
                let message = ui.colored_label(ui.visuals().error_fg_color, error);
                ui.ctx().accesskit_node_builder(response.id, |node| {
                    node.set_invalid(egui::accesskit::Invalid::True);
                    node.set_error_message(message.id.accesskit_id());
                    node.push_described_by(message.id.accesskit_id());
                });
                ui.ctx().accesskit_node_builder(message.id, |node| node.set_live(egui::accesskit::Live::Polite));
            }
            error.is_none()
        }
    }
}
fn start(
    ctx: &egui::Context,
    studio: &Studio,
    s: &mut State,
    plugin: Plugin,
    action: Action,
    params: serde_json::Value,
    gesture: Option<Gesture>,
) {
    if super::jobs::is_running::<Finished>(ctx, RUN) {
        s.error = "A plugin is already running".into();
        return;
    }
    let doc = studio.doc.clone();
    let owner = studio.swap_id.clone();
    let selection = studio.selection.clone();
    let active = studio.active_layer;
    let cancel = Arc::new(AtomicBool::new(false));
    s.cancel = Some(cancel.clone());
    s.error.clear();
    super::jobs::start(ctx, RUN, move || {
        let before = fingerprint(&doc);
        let output = plugins::run(
            &plugin,
            &action.id,
            doc,
            selection.clone(),
            active,
            params,
            gesture,
            cancel,
        )?;
        Ok(Finished {
            owner,
            before,
            selection,
            output,
        })
    });
}
pub fn tick(ctx: &egui::Context, studio: &mut Studio) {
    let mut s = state(ctx);
    if !s.loaded {
        s.loaded = true;
        s.behaviors = std::fs::read(plugins::root().join("behaviors.json"))
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or(false);
        reload(ctx);
    }
    if let Some(result) = super::jobs::poll::<Catalog>(ctx, SCAN) {
        match result {
            Ok(catalog) => s.catalog = catalog,
            Err(error) => s.error = error,
        }
    }
    if let Some(result) = super::jobs::poll::<Plugin>(ctx, INSTALL) {
        match result {
            Ok(plugin) => {
                studio.status = format!("Installed {} {}", plugin.name, plugin.version);
                reload(ctx);
            }
            Err(error) => s.error = error,
        }
    }
    if let Some(result) = super::jobs::poll::<Finished>(ctx, RUN) {
        let cancelled = s.cancel.take().is_some_and(|c| c.load(Ordering::Relaxed));
        match result{
            Ok(done) if !cancelled && done.owner==studio.swap_id && studio.op.is_none() && done.selection==studio.selection && done.before==fingerprint(&studio.doc)=>{
                if !done.output.commands.is_empty(){studio.commit(crate::document::Cmd::Batch(done.output.commands));studio.show_welcome=false;studio.selection=done.output.selection;}
                if let Some(brush)=done.output.brush{studio.switch_persona(Persona::Pixel);studio.brush=brush;studio.set_tool(Tool::Brush);s.tool=None;}
                if !done.output.palettes.is_empty(){
                    let draft=&mut studio.libraries.personal;
                    if !draft.loaded { draft.palettes=crate::palette::load();draft.loaded=true; }
                    match crate::palette::merge(&mut draft.palettes,done.output.palettes) {
                        Err(error)=>s.error=error,
                        Ok(_)=> { draft.dirty=true; match crate::palette::save(&draft.palettes) {
                            Ok(())=>{draft.dirty=false;draft.stamp=crate::app::libraries::file_stamp(&crate::palette::path()).ok().flatten();},
                            Err(error)=>s.error=error
                        }}
                    }
                    studio.libraries.sidebar=crate::app::libraries::Sidebar::Palettes;
                }
                studio.status=if done.output.message.is_empty(){"Plugin completed · Undo restores document edits".into()}else{done.output.message};
                s.selection=studio.selection.clone();s.owner=studio.swap_id.clone();
            },
            Ok(_)=>s.error="Plugin result discarded because the document or selection changed, or the run was cancelled".into(),
            Err(error)=>s.error=error,
        }
    }
    if ctx.input(|i| i.key_pressed(egui::Key::Escape)) && s.tool.is_some() {
        s.tool = None;
        s.gesture = None;
        studio.status = "Plugin tool stopped".into();
    }
    if s.tool.is_some() && studio.tool != Tool::Select {
        s.tool = None;
        s.gesture = None;
    }
    let event = if s.owner != studio.swap_id {
        Some("document_opened")
    } else if s.selection != studio.selection {
        Some("selection_changed")
    } else {
        None
    };
    s.owner = studio.swap_id.clone();
    s.selection = studio.selection.clone();
    if s.behaviors && !studio.show_welcome && studio.op.is_none() {
        if let Some(event) = event {
            for plugin in s.catalog.plugins.iter().filter(|p| p.enabled) {
                for action in plugin
                    .actions
                    .iter()
                    .filter(|a| a.event.as_deref() == Some(event))
                {
                    if s.events.len() < 128 {
                        s.events.push((plugin.clone(), action.clone()));
                    }
                }
            }
        }
        if !super::jobs::is_running::<Finished>(ctx, RUN) && !s.events.is_empty() {
            let (plugin, action) = s.events.remove(0);
            start(
                ctx,
                studio,
                &mut s,
                plugin,
                action,
                serde_json::json!({}),
                None,
            );
        }
    } else {
        s.events.clear();
    }
    store(ctx, s);
}
pub fn show(ctx: &egui::Context, studio: &mut Studio) {
    let mut s = state(ctx);
    if !s.open {
        return;
    }
    let mut open = true;
    egui::Window::new("Lua plugins").open(&mut open).default_size([670.,570.]).show(ctx,|ui|{
        ui.horizontal_wrapped(|ui|{
            if ui.button("Install plugin…").clicked(){studio.request_file_dialog(||rfd::FileDialog::new().add_filter("Omadesign plugin",&["lua","omaplug","zip"]).pick_file(),|ctx,_,path|{super::jobs::start(ctx,INSTALL,move||plugins::install(&path));});}
            if ui.button("Install folder…").clicked(){studio.request_file_dialog(crate::project::dialog_folder,|ctx,_,path|{super::jobs::start(ctx,INSTALL,move||plugins::install(&path));});}
            if ui.button("Reload").clicked(){reload(ctx);}
            if ui.button("Plugin folder").clicked(){let root=plugins::root();let _=std::fs::create_dir_all(&root);let _=std::process::Command::new("xdg-open").arg(root).spawn();}
            ui.hyperlink_to("Authoring guide","https://omadesign.app/docs/plugins");
        });
        if ui.checkbox(&mut s.behaviors,"Run enabled plugins’ document and selection behaviors").changed(){
            let root=plugins::root();let _=std::fs::create_dir_all(&root);
            if let Err(error)=crate::formats::write_atomic(&root.join("behaviors.json"),if s.behaviors{b"true"}else{b"false"}){s.error=error;}
        }
        if super::jobs::is_running::<Finished>(ctx,RUN){ui.horizontal(|ui|{ui.spinner();ui.label("Running plugin…");if ui.button("Cancel run").clicked(){if let Some(cancel)=&s.cancel{cancel.store(true,Ordering::Relaxed);}}});}
        if !s.error.is_empty(){ui.colored_label(super::theme::p().error,&s.error);}
        for error in &s.catalog.errors{ui.colored_label(super::theme::p().error,error);}
        ui.separator();
        if let Some((plugin,action))=s.chosen.clone(){
            ui.heading(&action.name);ui.label(&action.description);
            let mut valid = true;
            for parameter in &action.parameters {
                let value=s.params.entry(parameter.id.clone()).or_insert(parameter.default.clone());
                valid &= ui.push_id(("plugin-parameter",&studio.swap_id,&studio.selection,&plugin.id,&action.id,&parameter.id), |ui| parameter_control(ui,parameter,value)).inner;
            }
            ui.horizontal(|ui|{
                if ui.add_enabled(valid && !studio.file_dialog_pending() && !studio.updates.freezing && !super::jobs::is_running::<Finished>(ctx,RUN),egui::Button::new(if action.tool{"Activate tool"}else{"Run"})).clicked(){
                    let params=serde_json::Value::Object(s.params.clone());
                    if action.tool{studio.show_welcome=false;studio.set_tool(Tool::Select);s.tool=Some((plugin.clone(),action.clone(),params));s.gesture=None;s.open=false;studio.status=format!("{} · drag on canvas · Esc to exit",action.name);}
                    else{start(ctx,studio,&mut s,plugin.clone(),action.clone(),params,None);}
                }
                if ui.button("Back to plugins").clicked(){s.chosen=None;}
            });
        }else{
            ui.horizontal(|ui|{let response=ui.add(egui::TextEdit::singleline(&mut s.query).id(ui.make_persistent_id("plugin-search")).hint_text("Find a plugin or action…"));name(ui,&response,"Find a plugin or action");
                let response=egui::ComboBox::from_id_salt("plugin-category").selected_text(if s.category.is_empty(){"All categories"}else{&s.category}).show_ui(ui,|ui|{ui.selectable_value(&mut s.category,String::new(),"All categories");for category in ["Filters","Effects","Icons","Brushes","Tools","Behaviors","Batch","Patterns","Gradients","Swatches"]{ui.selectable_value(&mut s.category,category.into(),category);}}).response;name(ui,&response,"Plugin category");
            });
            egui::ScrollArea::vertical().max_height(390.).show(ui,|ui|{
                let mut chosen=None;
                for plugin in &mut s.catalog.plugins{
                    ui.horizontal(|ui|{if ui.checkbox(&mut plugin.enabled,&plugin.name).changed(){if let Err(error)=plugins::set_enabled(&plugin.id,plugin.enabled){s.error=error;}}ui.weak(&plugin.version);});
                    ui.small(&plugin.description);
                    ui.add_enabled_ui(plugin.enabled,|ui|{for action in &plugin.actions{
                        if (s.category.is_empty()||s.category==action.category)&&(s.query.is_empty()||format!("{} {}",plugin.name,action.name).to_lowercase().contains(&s.query.to_lowercase())) {
                            if ui.button(format!("{} · {}",action.category,action.name)).on_hover_text(&action.description).clicked(){chosen=Some((plugin.clone(),action.clone()));}
                        }
                    }});ui.separator();
                }
                if let Some((plugin,action))=chosen{s.params=action.parameters.iter().map(|p|(p.id.clone(),p.default.clone())).collect();s.chosen=Some((plugin,action));}
                if s.catalog.plugins.is_empty(){ui.label("Install a .lua file, .omaplug bundle or plugin folder. Bundled examples are in the application’s share/omadesign/plugins directory.");}
            });
        }
    });
    s.open &= open;
    store(ctx, s);
}

pub fn canvas_input(
    ui: &Ui,
    studio: &mut Studio,
    response: &Response,
    rect: Rect,
    panning: bool,
) -> bool {
    let ctx = ui.ctx();
    let mut s = state(ctx);
    let Some((plugin, action, params)) = s.tool.clone() else {
        return false;
    };
    if panning {
        return false;
    }
    let point = ctx.input(|i| i.pointer.interact_pos()).map(|p| {
        let p = studio.view.pointer_to_world(
            crate::geom::Pt::new(rect.left(), rect.top()),
            crate::geom::Pt::new(p.x, p.y),
        );
        [p.x, p.y]
    });
    if response.is_pointer_button_down_on()
        && s.gesture.is_none()
        && !super::jobs::is_running::<Finished>(ctx, RUN)
    {
        let mods = ctx.input(|i| i.modifiers);
        s.gesture = Some(Gesture {
            points: vec![],
            alt: mods.alt,
            ctrl: mods.ctrl,
            shift: mods.shift,
        });
    }
    if let Some(gesture) = &mut s.gesture {
        if let Some(point) = point {
            if gesture.points.last() != Some(&point) && gesture.points.len() < 8192 {
                gesture.points.push(point);
            }
        }
    }
    if response.drag_stopped_by(PointerButton::Primary) || response.clicked() {
        if let Some(gesture) = s.gesture.take() {
            start(ctx, studio, &mut s, plugin, action, params, Some(gesture));
        }
    }
    ctx.set_cursor_icon(egui::CursorIcon::Crosshair);
    store(ctx, s);
    true
}

pub fn paint(ui: &Ui, studio: &Studio, rect: Rect) {
    if let Some(gesture) = state(ui.ctx()).gesture {
        let points: Vec<_> = gesture
            .points
            .iter()
            .map(|[x, y]| {
                let p = studio.view.to_screen(crate::geom::Pt::new(*x, *y));
                pos2(rect.left() + p.x, rect.top() + p.y)
            })
            .collect();
        if points.len() > 1 {
            ui.painter_at(rect).add(egui::Shape::line(
                points,
                egui::Stroke::new(1.5, super::theme::accent()),
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tap(key: egui::Key, modifiers: egui::Modifiers) -> Vec<egui::Event> {
        [true, false].into_iter().map(|pressed| egui::Event::Key { key, physical_key: Some(key), pressed, repeat: false, modifiers }).collect()
    }

    fn keyboard_frame(ctx: &egui::Context, studio: &mut Studio, events: Vec<egui::Event>) -> egui::FullOutput {
        let mut output = ctx.run_ui(egui::RawInput { events, screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1000., 900.))), ..Default::default() }, |ui| {
            studio.handle_shortcuts(ui.ctx());
            ui.interact(ui.max_rect(), Id::new("studio-canvas"), egui::Sense::click());
            show(ui.ctx(), studio);
            super::super::keyboard_tools::show(ui.ctx(), studio);
        });
        output.textures_delta.clear();
        output
    }

    fn named<'a>(output: &'a egui::FullOutput, label: &str) -> (egui::accesskit::NodeId, &'a egui::accesskit::Node) {
        output.platform_output.accesskit_update.as_ref().unwrap().nodes.iter().find_map(|(id, node)| (node.label() == Some(label)).then_some((*id, node))).unwrap_or_else(|| panic!("missing {label}"))
    }

    fn tab_to(ctx: &egui::Context, studio: &mut Studio, label: &str) -> egui::FullOutput {
        for _ in 0..96 {
            let output = keyboard_frame(ctx, studio, vec![]);
            if ctx.memory(|memory| memory.focused()).is_some_and(|id| id.accesskit_id() == named(&output, label).0) { return output; }
            keyboard_frame(ctx, studio, tap(egui::Key::Tab, egui::Modifiers::NONE));
        }
        panic!("Tab did not reach {label}");
    }

    #[test]
    fn keyboard_plugin_parameters_and_tool_create_native_artwork_without_pointer_setup() {
        use egui::{Key, Modifiers};
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        let mut studio = Studio::new();
        studio.doc = crate::document::Document::new("Keyboard plugin", 32., 32., 72.);
        studio.show_welcome = false;
        assert!(studio.selection.is_empty());
        let mut plugin = plugins::inspect(&std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("plugins/studio-starter/main.lua")).unwrap();
        plugin.enabled = true;
        store(&ctx, State { open: true, loaded: true, catalog: Catalog { plugins: vec![plugin], errors: vec![] }, ..Default::default() });
        tab_to(&ctx, &mut studio, "Find a plugin or action");
        keyboard_frame(&ctx, &mut studio, vec![egui::Event::Text("ribbon".into())]);
        tab_to(&ctx, &mut studio, "Tools · Ribbon path");
        keyboard_frame(&ctx, &mut studio, tap(Key::Enter, Modifiers::NONE));
        tab_to(&ctx, &mut studio, "Color");
        let chord = Modifiers::CTRL | Modifiers::COMMAND;
        keyboard_frame(&ctx, &mut studio, tap(Key::A, chord));
        let output = keyboard_frame(&ctx, &mut studio, vec![egui::Event::Text("invalid".into())]);
        let color = named(&output, "Color").1;
        assert_eq!(color.invalid(), Some(egui::accesskit::Invalid::True));
        let message = color.error_message().unwrap();
        assert!(color.described_by().contains(&message));
        assert!(output.platform_output.accesskit_update.as_ref().unwrap().nodes.iter().any(|(id, node)| *id == message && node.live() == Some(egui::accesskit::Live::Polite)));
        assert!(named(&output, "Activate tool").1.is_disabled());
        assert!(keyboard_tool(&ctx).is_none());
        keyboard_frame(&ctx, &mut studio, tap(Key::A, chord));
        keyboard_frame(&ctx, &mut studio, vec![egui::Event::Text("#336699".into())]);
        tab_to(&ctx, &mut studio, "Activate tool");
        keyboard_frame(&ctx, &mut studio, tap(Key::Enter, Modifiers::NONE));
        keyboard_frame(&ctx, &mut studio, vec![]);
        assert!(keyboard_tool(&ctx).is_some());
        keyboard_frame(&ctx, &mut studio, tap(Key::F8, Modifiers::NONE));
        keyboard_frame(&ctx, &mut studio, tap(Key::F9, Modifiers::NONE));
        keyboard_frame(&ctx, &mut studio, vec![]);
        assert!(super::super::keyboard_tools::is_open(&ctx));
        keyboard_frame(&ctx, &mut studio, tap(Key::A, chord));
        keyboard_frame(&ctx, &mut studio, vec![egui::Event::Text("4,4; 16,20; 28,4".into())]);
        keyboard_frame(&ctx, &mut studio, tap(Key::Enter, chord));
        assert!(!super::super::keyboard_tools::is_open(&ctx));
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while super::super::jobs::is_running::<Finished>(&ctx, RUN) {
            assert!(std::time::Instant::now() < deadline);
            tick(&ctx, &mut studio);
            std::thread::yield_now();
        }
        assert!(state(&ctx).error.is_empty(), "{}", state(&ctx).error);
        let (layer, id) = studio.primary().unwrap();
        let shape = studio.doc.find_shape(layer, id).unwrap();
        assert_eq!(shape.name, "Ribbon");
        assert!(matches!(&shape.geom, crate::geom::Geom::Path { anchors, .. } if anchors.len() == 3));
        keyboard_frame(&ctx, &mut studio, vec![]);
        keyboard_frame(&ctx, &mut studio, tap(Key::Z, chord));
        assert!(studio.doc.find_shape(layer, id).is_none());
    }

    #[test]
    fn background_results_commit_once_and_discard_when_document_changes() {
        for stale in [false, true] {
            let ctx = egui::Context::default();
            let mut studio = Studio::new();
            studio.history.clear();
            let plugin = plugins::inspect(
                &std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("plugins/studio-starter/main.lua"),
            )
            .unwrap();
            let action = plugin
                .actions
                .iter()
                .find(|a| a.id == "dot-field")
                .unwrap()
                .clone();
            let mut s = State {
                loaded: true,
                ..Default::default()
            };
            start(
                &ctx,
                &studio,
                &mut s,
                plugin,
                action,
                serde_json::json!({}),
                None,
            );
            store(&ctx, s);
            if stale {
                studio.doc.name = "Edited while running".into();
            }
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
            while super::super::jobs::is_running::<Finished>(&ctx, RUN) {
                assert!(std::time::Instant::now() < deadline);
                tick(&ctx, &mut studio);
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
            assert_eq!(studio.history.len(), usize::from(!stale));
            if stale {
                assert!(state(&ctx).error.contains("discarded"));
            } else {
                assert!(!studio.show_welcome);
                assert!(
                    studio
                        .doc
                        .layers
                        .iter()
                        .any(|l| l.kind.shapes().is_some_and(|s| s.len() == 48))
                );
                studio.undo();
                assert!(
                    studio
                        .doc
                        .layers
                        .iter()
                        .all(|l| l.kind.shapes().is_none_or(|s| s.is_empty()))
                );
            }
        }
    }
}
