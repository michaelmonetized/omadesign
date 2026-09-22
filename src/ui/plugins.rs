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
            for parameter in &action.parameters{ui.horizontal(|ui|{
                ui.label(&parameter.label);let value=s.params.entry(parameter.id.clone()).or_insert(parameter.default.clone());
                match parameter.kind.as_str(){
                    "boolean"=>{let mut b=value.as_bool().unwrap_or(false);if ui.checkbox(&mut b,"").changed(){*value=b.into();}},
                    "number"=>{let mut n=value.as_f64().unwrap_or(0.);if ui.add(egui::DragValue::new(&mut n).range(parameter.min.unwrap_or(-100000.)..=parameter.max.unwrap_or(100000.))).changed(){*value=n.into();}},
                    _=>{let mut text=value.as_str().unwrap_or("").to_string();if ui.text_edit_singleline(&mut text).changed(){*value=text.into();}},
                }
            });}
            ui.horizontal(|ui|{
                if ui.add_enabled(!super::jobs::is_running::<Finished>(ctx,RUN),egui::Button::new(if action.tool{"Activate tool"}else{"Run"})).clicked(){
                    let params=serde_json::Value::Object(s.params.clone());
                    if action.tool{studio.show_welcome=false;studio.set_tool(Tool::Select);s.tool=Some((plugin.clone(),action.clone(),params));s.gesture=None;s.open=false;studio.status=format!("{} · drag on canvas · Esc to exit",action.name);}
                    else{start(ctx,studio,&mut s,plugin.clone(),action.clone(),params,None);}
                }
                if ui.button("Back to plugins").clicked(){s.chosen=None;}
            });
        }else{
            ui.horizontal(|ui|{ui.add(egui::TextEdit::singleline(&mut s.query).hint_text("Find a plugin or action…"));
                egui::ComboBox::from_id_salt("plugin-category").selected_text(if s.category.is_empty(){"All categories"}else{&s.category}).show_ui(ui,|ui|{ui.selectable_value(&mut s.category,String::new(),"All categories");for category in ["Filters","Effects","Icons","Brushes","Tools","Behaviors","Batch","Patterns","Gradients","Swatches"]{ui.selectable_value(&mut s.category,category.into(),category);}});
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
