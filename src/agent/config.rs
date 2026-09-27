use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Profile {
    pub name: String,
    pub command: String,
    pub args: Vec<String>,
}

pub fn presets() -> Vec<Profile> {
    [
        (
            "Codex",
            "npx",
            vec!["--yes", "@agentclientprotocol/codex-acp@1.13.1"],
        ),
        (
            "Claude",
            "npx",
            vec!["--yes", "@agentclientprotocol/claude-agent-acp@0.81.2"],
        ),
        ("Gemini", "gemini", vec!["--acp"]),
        ("OpenCode", "opencode", vec!["acp"]),
        ("Custom ACP agent", "", vec![]),
    ]
    .into_iter()
    .map(|(name, command, args)| Profile {
        name: name.into(),
        command: command.into(),
        args: args.into_iter().map(str::to_owned).collect(),
    })
    .collect()
}

pub fn root() -> PathBuf {
    std::env::var_os("XDG_DATA_HOME")
        .filter(|p| !p.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".local/share")
        })
        .join("omadesign/agent")
}

pub fn write<T: Serialize>(path: &std::path::Path, value: &T) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    let parent = path.parent().ok_or("Invalid agent data path")?;
    std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o700))
        .map_err(|e| e.to_string())?;
    crate::formats::write_atomic(
        path,
        &serde_json::to_vec_pretty(value).map_err(|e| e.to_string())?,
    )?;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
        .map_err(|e| e.to_string())
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Settings {
    pub profile: Profile,
    pub directory: PathBuf,
    pub live_edits: bool,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            profile: presets().remove(0),
            directory: std::env::var_os("HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|| "/tmp".into()),
            live_edits: true,
        }
    }
}

impl Settings {
    pub fn load() -> Self {
        std::fs::read(root().join("settings.json"))
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default()
    }
    pub fn save(&self) -> Result<(), String> {
        write(&root().join("settings.json"), self)
    }
}
