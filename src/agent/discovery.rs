//! Local executable discovery and non-generating ACP capability probes.
use super::{
    config::{self, Profile},
    runtime::{Connection, Event},
};
use serde_json::Value;
use std::{
    path::{Path, PathBuf},
    sync::mpsc,
    time::{Duration, Instant},
};

#[derive(Clone, Debug, PartialEq)]
pub enum Status {
    Checking,
    NotInstalled,
    AdapterMissing,
    Ready,
    SignIn,
    Unavailable,
}
impl Status {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Checking => "Checking…",
            Self::NotInstalled => "Not installed",
            Self::AdapterMissing => "ACP adapter needed",
            Self::Ready => "Ready",
            Self::SignIn => "Sign-in required",
            Self::Unavailable => "Unavailable",
        }
    }
}
#[derive(Clone)]
pub struct Provider {
    pub profile: Profile,
    pub status: Status,
    pub metadata: Value,
    pub detail: String,
}
#[derive(Default)]
pub struct Discovery {
    pub providers: Vec<Provider>,
    pub search: String,
    pub filter: Option<String>,
    pub favorites_only: bool,
    receive: Option<mpsc::Receiver<(usize, Provider)>>,
    last: Option<Instant>,
    directory: PathBuf,
}

pub fn executable(name: &str) -> Option<PathBuf> {
    use std::os::unix::fs::PermissionsExt;
    let usable = |p: &Path| {
        std::fs::metadata(p).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
    };
    if name.contains('/') {
        let p = PathBuf::from(name);
        return usable(&p).then_some(p);
    }
    std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default())
        .map(|p| p.join(name))
        .find(|p| usable(p))
}
fn cached_adapter(name: &str) -> Option<PathBuf> {
    if let Some(p) = executable(name) {
        return Some(p);
    }
    let home = PathBuf::from(std::env::var_os("HOME")?);
    let root = std::env::var_os("npm_config_cache")
        .or_else(|| std::env::var_os("NPM_CONFIG_CACHE"))
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(".npm"))
        .join("_npx");
    let mut choices: Vec<_> = std::fs::read_dir(root)
        .ok()?
        .flatten()
        .take(256)
        .map(|e| e.path().join("node_modules/.bin").join(name))
        .filter(|p| executable(&p.to_string_lossy()).is_some())
        .collect();
    choices
        .sort_by_key(|p| std::cmp::Reverse(std::fs::metadata(p).and_then(|m| m.modified()).ok()));
    choices.into_iter().next()
}
pub fn installed_profiles() -> Vec<Provider> {
    config::presets()
        .into_iter()
        .filter(|p| p.name != "Custom ACP agent")
        .map(|mut profile| {
            let binary = match profile.name.as_str() {
                "Codex" => "codex",
                "Claude" => "claude",
                "Gemini" => "gemini",
                _ => "opencode",
            };
            let mut status = if executable(binary).is_some() {
                Status::Checking
            } else {
                Status::NotInstalled
            };
            if matches!(profile.name.as_str(), "Codex" | "Claude") {
                let adapter = if profile.name == "Codex" {
                    "codex-acp"
                } else {
                    "claude-agent-acp"
                };
                if let Some(path) = cached_adapter(adapter) {
                    profile.command = path.to_string_lossy().into_owned();
                    profile.args.clear();
                    status = Status::Checking;
                } else if status == Status::Checking {
                    status = Status::AdapterMissing;
                }
            }
            Provider {
                profile,
                status,
                metadata: Value::Null,
                detail: String::new(),
            }
        })
        .collect()
}
impl Discovery {
    pub fn poll(&mut self, ctx: &eframe::egui::Context, directory: &Path, force: bool) {
        if let Some(receive) = &self.receive {
            while let Ok((index, provider)) = receive.try_recv() {
                if let Some(slot) = self.providers.get_mut(index) {
                    *slot = provider;
                }
            }
        }
        let checking = self.providers.iter().any(|p| p.status == Status::Checking);
        if checking {
            ctx.request_repaint_after(Duration::from_millis(100));
        }
        if self.last.is_some()
            && (checking
                || (!force
                    && self.directory == directory
                    && self.last.unwrap().elapsed() < Duration::from_secs(300)))
        {
            return;
        }
        self.providers = installed_profiles();
        self.last = Some(Instant::now());
        self.directory = directory.to_path_buf();
        let (send, receive) = mpsc::channel();
        self.receive = Some(receive);
        for (i, provider) in self
            .providers
            .clone()
            .into_iter()
            .enumerate()
            .filter(|(_, p)| p.status == Status::Checking)
        {
            let send = send.clone();
            let ctx = ctx.clone();
            let directory = directory.to_path_buf();
            std::thread::spawn(move || {
                let mut provider = provider;
                match std::env::current_exe()
                    .map_err(|e| e.to_string())
                    .and_then(|binary| {
                        Connection::start(
                            provider.profile.clone(),
                            directory,
                            None,
                            binary,
                            ctx.clone(),
                        )
                    }) {
                    Err(e) => {
                        provider.status = Status::Unavailable;
                        provider.detail = e;
                    }
                    Ok(connection) => {
                        let deadline = Instant::now() + Duration::from_secs(20);
                        loop {
                            if Instant::now() >= deadline {
                                provider.status = Status::Unavailable;
                                provider.detail =
                                    "ACP probe timed out; refresh after checking the local agent."
                                        .into();
                                break;
                            }
                            match connection.events.recv_timeout(Duration::from_millis(100)) {
                                Ok(Event::Session { data, .. }) => {
                                    provider.status = Status::Ready;
                                    provider.metadata = data;
                                    provider.detail =
                                        "Local ACP session ready. No generation request was sent."
                                            .into();
                                    break;
                                }
                                Ok(Event::Error(e)) => {
                                    let lower = e.to_lowercase();
                                    provider.status = if [
                                        "auth",
                                        "login",
                                        "log in",
                                        "sign in",
                                        "credential",
                                        "api key",
                                    ]
                                    .iter()
                                    .any(|s| lower.contains(s))
                                    {
                                        Status::SignIn
                                    } else {
                                        Status::Unavailable
                                    };
                                    provider.detail = e;
                                    break;
                                }
                                Ok(Event::Closed) => {
                                    provider.status = Status::Unavailable;
                                    provider.detail =
                                        "Agent exited during capability discovery".into();
                                    break;
                                }
                                Ok(Event::Permission { id, .. }) => {
                                    let _ = connection.send(super::runtime::Command::Permission {
                                        id,
                                        option: None,
                                    });
                                }
                                _ => (),
                            }
                        }
                    }
                }
                let _ = send.send((i, provider));
                ctx.request_repaint();
            });
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Choice {
    pub id: String,
    pub label: String,
    pub group: String,
}
#[derive(Clone, Debug)]
pub struct OptionSet {
    pub id: String,
    pub name: String,
    pub category: String,
    pub current: String,
    pub choices: Vec<Choice>,
    pub legacy: bool,
}
pub fn options(metadata: &Value) -> Vec<OptionSet> {
    let mut result = vec![];
    for option in metadata["configOptions"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|o| o["type"] == "select")
    {
        let Some(id) = option["id"].as_str() else {
            continue;
        };
        let mut choices = vec![];
        for value in option["options"].as_array().into_iter().flatten() {
            let group = value["name"].as_str().unwrap_or_default();
            if let Some(values) = value["options"].as_array() {
                for value in values {
                    if let Some(c) = choice(value, group) {
                        choices.push(c);
                    }
                }
            } else if let Some(c) = choice(value, "") {
                choices.push(c);
            }
        }
        let lower = id.to_lowercase();
        let category = option["category"]
            .as_str()
            .map(str::to_owned)
            .unwrap_or_else(|| {
                if lower.contains("model") {
                    "model".into()
                } else if lower.contains("effort")
                    || lower.contains("reason")
                    || lower.contains("thought")
                {
                    "thought_level".into()
                } else {
                    lower
                }
            });
        result.push(OptionSet {
            id: id.into(),
            name: option["name"].as_str().unwrap_or(id).into(),
            category,
            current: option["currentValue"].as_str().unwrap_or_default().into(),
            choices,
            legacy: false,
        });
    }
    for (field, list, current, category) in [
        ("models", "availableModels", "currentModelId", "model"),
        ("modes", "availableModes", "currentModeId", "mode"),
    ] {
        if result.iter().any(|o| o.category == category) {
            continue;
        }
        let choices = metadata[field][list]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|v| {
                let id = v[if field == "models" { "modelId" } else { "id" }].as_str()?;
                Some(Choice {
                    id: id.into(),
                    label: v["name"].as_str().unwrap_or(id).into(),
                    group: String::new(),
                })
            })
            .collect::<Vec<_>>();
        if !choices.is_empty() {
            result.push(OptionSet {
                id: category.into(),
                name: category.into(),
                category: category.into(),
                current: metadata[field][current].as_str().unwrap_or_default().into(),
                choices,
                legacy: true,
            });
        }
    }
    result
}
fn choice(v: &Value, group: &str) -> Option<Choice> {
    let id = v["value"].as_str()?;
    Some(Choice {
        id: id.into(),
        label: v["name"].as_str().unwrap_or(id).into(),
        group: group.into(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn capabilities_are_provider_advertised_grouped_and_replace_legacy_choices() {
        let metadata = json!({"configOptions":[
            {"id":"model","name":"Model","category":"model","type":"select","currentValue":"fast","options":[{"name":"Current","group":"new","options":[{"value":"fast","name":"Fast model"}]},{"name":"Legacy","group":"old","options":[{"value":"old","name":"Older model"}]}]},
            {"id":"reasoning","name":"Effort","category":"thought_level","type":"select","currentValue":"high","options":[{"value":"low","name":"Low"},{"value":"high","name":"High"}]}],
            "models":{"currentModelId":"stale","availableModels":[{"modelId":"stale","name":"Stale"}]}});
        let options = options(&metadata);
        assert_eq!(options.len(), 2);
        assert_eq!(options[0].choices.len(), 2);
        assert_eq!(options[0].choices[1].group, "Legacy");
        assert_eq!(options[1].category, "thought_level");
        assert!(!options[0].legacy);
        let newer = json!({"configOptions":[{"id":"model","name":"Model","category":"model","type":"select","currentValue":"other","options":[{"value":"other","name":"Other"}]}]});
        assert_eq!(
            super::options(&newer).len(),
            1,
            "Removed effort controls must disappear"
        );
    }
    #[test]
    fn legacy_models_and_modes_remain_supported_without_inventing_effort() {
        let options = options(
            &json!({"models":{"currentModelId":"agent-model","availableModels":[{"modelId":"agent-model","name":"Agent Model"}]},"modes":{"currentModeId":"ask","availableModes":[{"id":"ask","name":"Ask"}]}}),
        );
        assert_eq!(options.len(), 2);
        assert!(options.iter().all(|o| o.legacy));
        assert!(!options.iter().any(|o| o.category == "thought_level"));
    }
}
