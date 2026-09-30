use super::*;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub(super) struct Options {
    pub format: Format,
    pub scale: u32,
    pub ai: bool,
    pub upscale: upscale::Settings,
    pub scope: export::target::Scope,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            format: Format::Png,
            scale: 1,
            ai: false,
            upscale: Default::default(),
            scope: Default::default(),
        }
    }
}
#[derive(Clone, Serialize, Deserialize)]
pub(super) struct Preset {
    pub name: String,
    pub options: Options,
}
#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub(super) struct Memory {
    pub last: Options,
    pub files: BTreeMap<String, Options>,
    pub presets: Vec<Preset>,
    #[serde(skip)]
    sessions: BTreeMap<String, Options>,
}
impl Memory {
    fn record(&mut self, file: Option<&str>, owner: &str, value: Options) {
        self.last = value.clone();
        self.sessions.insert(owner.into(), value.clone());
        if let Some(file) = file {
            self.files.insert(file.into(), value);
        }
        while self.files.len() > 256 {
            self.files.pop_first();
        }
    }
}
fn key() -> Id {
    Id::new("export-options-memory")
}
fn path() -> PathBuf {
    crate::project::data_dir().join("export-options.json")
}
fn memory(ctx: &Context) -> Memory {
    if let Some(memory) = ctx.data(|d| d.get_temp::<Memory>(key())) {
        return memory;
    }
    let memory: Memory = std::fs::read(path())
        .ok()
        .filter(|bytes| bytes.len() <= 1024 * 1024)
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default();
    ctx.data_mut(|d| d.insert_temp(key(), memory.clone()));
    memory
}
pub(super) fn recall(ctx: &Context, file: Option<&str>, owner: &str) -> Options {
    let memory = memory(ctx);
    let mut value = file
        .and_then(|key| memory.files.get(key))
        .or_else(|| memory.sessions.get(owner))
        .unwrap_or(&memory.last)
        .clone();
    value.scale = value.scale.clamp(1, 8);
    if !value.upscale.factor.is_finite() || !(1.01..=32.).contains(&value.upscale.factor) {
        value.upscale = Default::default();
    }
    value.upscale.tile = value.upscale.tile.clamp(32, 512);
    value
}
fn persist(ctx: &Context, memory: Memory) -> Result<(), String> {
    ctx.data_mut(|d| d.insert_temp(key(), memory.clone()));
    let path = path();
    std::fs::create_dir_all(path.parent().unwrap()).map_err(|e| e.to_string())?;
    let bytes = serde_json::to_vec_pretty(&memory).map_err(|e| e.to_string())?;
    export::write_atomic(&path, &bytes, &crate::ml::NoProgress)?;
    Ok(())
}
pub(super) fn remember(
    ctx: &Context,
    file: Option<&str>,
    owner: &str,
    value: Options,
) -> Result<(), String> {
    let mut memory = memory(ctx);
    memory.record(file, owner, value);
    persist(ctx, memory)
}
pub(super) fn presets(ctx: &Context) -> Vec<Preset> {
    let mut presets = vec![];
    for (name, format, scale) in [
        ("PNG · 1×", Format::Png, 1),
        ("PNG · 2×", Format::Png, 2),
        ("JPEG · 1×", Format::Jpeg, 1),
        ("SVG · vector", Format::Svg, 1),
        ("PSD · layered", Format::Psd, 1),
    ] {
        presets.push(Preset {
            name: name.into(),
            options: Options {
                format,
                scale,
                ..Default::default()
            },
        });
    }
    presets.extend(memory(ctx).presets);
    presets
}
pub(super) fn save_preset(ctx: &Context, name: &str, mut options: Options) -> Result<(), String> {
    let name = name.trim();
    if name.is_empty() || name.chars().count() > 80 {
        return Err("Use a preset name between 1 and 80 characters".into());
    }
    options.scope = Default::default();
    let mut memory = memory(ctx);
    if let Some(preset) = memory.presets.iter_mut().find(|p| p.name == name) {
        preset.options = options;
    } else {
        if memory.presets.len() >= 64 {
            return Err("You can save up to 64 export presets".into());
        }
        memory.presets.push(Preset {
            name: name.into(),
            options,
        });
    }
    persist(ctx, memory)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn export_memory_keeps_file_session_defaults_and_roundtrips_presets() {
        let mut memory = Memory::default();
        let a = Options {
            format: Format::Jpeg,
            scale: 3,
            scope: Scope::Selection,
            ..Default::default()
        };
        let b = Options {
            format: Format::Psd,
            ..Default::default()
        };
        memory.record(Some("/design/one.oma"), "one", a.clone());
        memory.record(None, "unsaved", b.clone());
        memory.presets.push(Preset {
            name: "Print".into(),
            options: a.clone(),
        });
        let ctx = Context::default();
        ctx.data_mut(|d| d.insert_temp(key(), memory.clone()));
        assert_eq!(recall(&ctx, Some("/design/one.oma"), "reopened"), a);
        assert_eq!(recall(&ctx, None, "unsaved"), b);
        assert_eq!(recall(&ctx, None, "fresh"), b);
        let restored: Memory =
            serde_json::from_slice(&serde_json::to_vec(&memory).unwrap()).unwrap();
        assert!(restored.sessions.is_empty());
        assert_eq!(restored.files["/design/one.oma"], a);
        assert_eq!(restored.presets[0].name, "Print");
        assert_eq!(restored.presets[0].options, a);
    }
}
