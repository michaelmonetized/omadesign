//! Versioned, local Lua extensions. Scripts run off the UI thread against an
//! isolated document; only a complete, validated command batch reaches history.
mod cli;
mod host;
mod install;
#[cfg(test)]
mod tests;
use crate::{
    document::{Cmd, Document},
    paint::Brush,
    palette::Palette,
};
pub use cli::cli;
pub use install::{discover, install, root, set_enabled};
use mlua::{Function, HookTriggers, Lua, LuaOptions, LuaSerdeExt, StdLib, Table, Value, VmState};
use serde::{Deserialize, Serialize};
use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

pub const API_VERSION: u32 = 1;
pub const MAX_SCRIPT_BYTES: u64 = 2 * 1024 * 1024;
const MAX_OPERATIONS: usize = 20_000;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Parameter {
    pub id: String,
    pub label: String,
    #[serde(default = "number_kind")]
    pub kind: String,
    #[serde(default)]
    pub default: serde_json::Value,
    #[serde(default)]
    pub min: Option<f64>,
    #[serde(default)]
    pub max: Option<f64>,
}
fn number_kind() -> String {
    "number".into()
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Action {
    pub id: String,
    pub name: String,
    #[serde(default = "command_category")]
    pub category: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub parameters: Vec<Parameter>,
    #[serde(default)]
    pub tool: bool,
    #[serde(default)]
    pub event: Option<String>,
}
fn command_category() -> String {
    "Commands".into()
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Plugin {
    pub id: String,
    pub name: String,
    pub version: String,
    pub api: u32,
    #[serde(default)]
    pub description: String,
    #[serde(skip)]
    pub path: PathBuf,
    #[serde(default)]
    pub actions: Vec<Action>,
    #[serde(skip)]
    pub enabled: bool,
}
#[derive(Clone, Default)]
pub struct Catalog {
    pub plugins: Vec<Plugin>,
    pub errors: Vec<String>,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Gesture {
    pub points: Vec<[f32; 2]>,
    pub alt: bool,
    pub shift: bool,
    pub ctrl: bool,
}
#[derive(Debug)]
pub struct Output {
    pub commands: Vec<Cmd>,
    pub brush: Option<Brush>,
    pub palettes: Vec<Palette>,
    pub message: String,
    pub selection: Vec<(usize, u64)>,
}

fn vm(cancel: Arc<AtomicBool>) -> Result<Lua, String> {
    let lua = Lua::new_with(
        StdLib::TABLE | StdLib::STRING | StdLib::MATH | StdLib::UTF8,
        LuaOptions::default(),
    )
    .map_err(|e| e.to_string())?;
    lua.set_memory_limit(64 * 1024 * 1024)
        .map_err(|e| e.to_string())?;
    let start = Instant::now();
    lua.set_hook(
        HookTriggers::new().every_nth_instruction(10_000),
        move |_, _| {
            if cancel.load(Ordering::Relaxed) || start.elapsed() > Duration::from_secs(15) {
                return Err(mlua::Error::runtime(
                    "Plugin cancelled or exceeded the 15 second execution limit",
                ));
            }
            Ok(VmState::Continue)
        },
    )
    .map_err(|e| e.to_string())?;
    // Base functions that can read a file are excluded, even without io/package.
    for name in [
        "dofile",
        "loadfile",
        "load",
        "collectgarbage",
        "pcall",
        "xpcall",
    ] {
        lua.globals()
            .set(name, Value::Nil)
            .map_err(|e| e.to_string())?;
    }
    Ok(lua)
}
fn load(lua: &Lua, path: &std::path::Path) -> Result<Table, String> {
    let meta = std::fs::metadata(path).map_err(|e| e.to_string())?;
    if meta.len() > MAX_SCRIPT_BYTES {
        return Err("Plugin source exceeds 2 MiB".into());
    }
    let source = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    lua.load(&source)
        .set_name(path.to_string_lossy())
        .set_mode(mlua::chunk::ChunkMode::Text)
        .eval()
        .map_err(|e| e.to_string())
}
pub fn inspect(path: &std::path::Path) -> Result<Plugin, String> {
    let lua = vm(Arc::new(AtomicBool::new(false)))?;
    let table = load(&lua, path)?;
    let mut plugin: Plugin = lua
        .from_value_with(
            Value::Table(table.clone()),
            mlua::serde::de::Options::new().deny_unsupported_types(false),
        )
        .map_err(|e| e.to_string())?;
    if plugin.api != API_VERSION {
        return Err(format!(
            "Unsupported plugin API {}; this app supports {}",
            plugin.api, API_VERSION
        ));
    }
    if !valid_id(&plugin.id) || plugin.name.trim().is_empty() {
        return Err("Plugin needs a valid id and name".into());
    }
    if plugin.actions.len() > 128 {
        return Err("Plugin exceeds 128 actions".into());
    }
    let mut ids = std::collections::HashSet::new();
    let actions: Table = table.get("actions").map_err(|e| e.to_string())?;
    for (i, action) in plugin.actions.iter().enumerate() {
        if !valid_id(&action.id) || !ids.insert(&action.id) || action.name.trim().is_empty() {
            return Err("Action IDs must be unique, use letters, numbers, dots, hyphens or underscores, and have a name".into());
        }
        let item: Table = actions.get(i + 1).map_err(|e| e.to_string())?;
        let _: Function = item
            .get("run")
            .map_err(|_| format!("{} needs a run function", action.id))?;
        if action.parameters.len() > 24 {
            return Err("Action exceeds 24 parameters".into());
        }
        let mut fields = std::collections::HashSet::new();
        for field in &action.parameters {
            if !fields.insert(&field.id)
                || !valid_id(&field.id)
                || !matches!(field.kind.as_str(), "number" | "text" | "color" | "boolean")
            {
                return Err(format!("Invalid parameter {}", field.id));
            }
        }
        validate_parameters(action, &serde_json::json!({}))?;
        if action
            .event
            .as_ref()
            .is_some_and(|e| !matches!(e.as_str(), "selection_changed" | "document_opened"))
        {
            return Err("Supported events: selection_changed, document_opened".into());
        }
    }
    plugin.path = path.to_owned();
    plugin.enabled = true;
    Ok(plugin)
}
fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 100
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
        && !id.starts_with('.')
}

fn validate_parameters(
    action: &Action,
    values: &serde_json::Value,
) -> Result<serde_json::Map<String, serde_json::Value>, String> {
    let values = values
        .as_object()
        .ok_or("Plugin parameters must be a JSON object")?;
    if values
        .keys()
        .any(|id| !action.parameters.iter().any(|p| &p.id == id))
    {
        return Err("Unknown plugin parameter".into());
    }
    let mut result = serde_json::Map::new();
    for field in &action.parameters {
        if field.min.zip(field.max).is_some_and(|(min, max)| min > max) {
            return Err(format!("Invalid range for {}", field.id));
        }
        let value = values.get(&field.id).unwrap_or(&field.default);
        let valid = match field.kind.as_str() {
            "number" => value.as_f64().is_some_and(|n| {
                n.is_finite()
                    && field.min.is_none_or(|min| n >= min)
                    && field.max.is_none_or(|max| n <= max)
            }),
            "boolean" => value.is_boolean(),
            "text" => value.as_str().is_some_and(|s| s.len() <= 16384),
            "color" => value.as_str().is_some_and(|s| {
                crate::color::Rgba::parse_hex(s.trim_start_matches('#')).is_some()
            }),
            _ => false,
        };
        if !valid {
            return Err(format!("Invalid value for {} ({})", field.label, field.id));
        }
        result.insert(field.id.clone(), value.clone());
    }
    Ok(result)
}

pub fn run(
    plugin: &Plugin,
    action_id: &str,
    doc: Document,
    selection: Vec<(usize, u64)>,
    active_layer: Option<usize>,
    parameters: serde_json::Value,
    gesture: Option<Gesture>,
    cancel: Arc<AtomicBool>,
) -> Result<Output, String> {
    let lua = vm(cancel.clone())?;
    let state = host::State::new(
        doc,
        selection,
        active_layer,
        plugin
            .path
            .parent()
            .unwrap_or(std::path::Path::new("."))
            .to_owned(),
        cancel,
    );
    host::register(&lua, state.clone()).map_err(|e| e.to_string())?;
    let plugin_table = load(&lua, &plugin.path)?;
    let actions: Table = plugin_table.get("actions").map_err(|e| e.to_string())?;
    let mut selected = None;
    for action in actions.sequence_values::<Table>() {
        let action = action.map_err(|e| e.to_string())?;
        if action.get::<String>("id").map_err(|e| e.to_string())? == action_id {
            selected = Some(action);
            break;
        }
    }
    let selected = selected.ok_or("Plugin action no longer exists; reload plugins")?;
    let function: Function = selected.get("run").map_err(|e| e.to_string())?;
    let context = host::context(&lua, &state, gesture).map_err(|e| e.to_string())?;
    let action = plugin
        .actions
        .iter()
        .find(|a| a.id == action_id)
        .ok_or("Unknown action")?;
    let params = validate_parameters(action, &parameters)?;
    let params = lua.to_value(&params).map_err(|e| e.to_string())?;
    function
        .call::<()>((context, params))
        .map_err(|e| e.to_string())?;
    let mut state = state.borrow_mut();
    state.check().map_err(|e| e.to_string())?;
    Ok(Output {
        commands: std::mem::take(&mut state.commands),
        brush: state.brush,
        palettes: std::mem::take(&mut state.palettes),
        message: state.message.clone(),
        selection: state.selection.clone(),
    })
}
