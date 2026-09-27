use super::{
    Purpose, bridge,
    config::{self, Settings},
    runtime::{Command, Connection, Event},
    tools,
};
use crate::app::Studio;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    path::PathBuf,
    sync::{Arc, Mutex, atomic::Ordering, mpsc},
    time::{Duration, Instant},
};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Entry {
    pub role: String,
    pub text: String,
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub status: String,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Thread {
    pub id: String,
    pub title: String,
    pub settings: Settings,
    pub purpose: Purpose,
    pub session_id: Option<String>,
    pub document: Option<PathBuf>,
    pub messages: Vec<Entry>,
    pub updated: u64,
}
impl Thread {
    fn new(settings: Settings, purpose: Purpose, document: Option<PathBuf>) -> Self {
        Self {
            id: crate::project::new_swap_id(),
            title: "New design conversation".into(),
            settings,
            purpose,
            session_id: None,
            document,
            messages: vec![],
            updated: 0,
        }
    }
}

pub struct Workspace {
    pub visible: bool,
    pub loaded: bool,
    pub settings: Settings,
    pub purpose: Purpose,
    pub request: String,
    pub thread: Option<Thread>,
    pub history: Vec<Thread>,
    pub connection: Option<Connection>,
    pub owner: Option<String>,
    pub busy: bool,
    pub ready: bool,
    pub connecting: bool,
    pub status: String,
    pub error: String,
    pub permissions: Vec<(Value, Value)>,
    pub metadata: Value,
    pub initialization: Value,
    pub show_settings: bool,
    pub show_history: bool,
    pub follow_canvas: bool,
    pub config_args: String,
    pub config_directory: String,
    pub focus_prompt: bool,
    pub edits: usize,
    dirty: bool,
    last_save: Instant,
    writer: Option<mpsc::Sender<Thread>>,
    writer_thread: Option<std::thread::JoinHandle<()>>,
    save_error: Arc<Mutex<Option<String>>>,
    pending_prompt: Option<String>,
    restoring: bool,
}
impl Default for Workspace {
    fn default() -> Self {
        let settings = Settings::default();
        Self {
            visible: false,
            loaded: false,
            config_args: serde_json::to_string(&settings.profile.args).unwrap(),
            config_directory: settings.directory.display().to_string(),
            settings,
            purpose: Purpose::Create,
            request: String::new(),
            thread: None,
            history: vec![],
            connection: None,
            owner: None,
            busy: false,
            ready: false,
            connecting: false,
            status: "Choose a local agent".into(),
            error: String::new(),
            permissions: vec![],
            metadata: Value::Null,
            initialization: Value::Null,
            show_settings: false,
            show_history: false,
            follow_canvas: true,
            focus_prompt: false,
            edits: 0,
            dirty: false,
            last_save: Instant::now(),
            writer: None,
            writer_thread: None,
            save_error: Arc::new(Mutex::new(None)),
            pending_prompt: None,
            restoring: false,
        }
    }
}
impl Workspace {
    pub fn load(&mut self) {
        if self.loaded {
            return;
        }
        self.loaded = true;
        self.settings = Settings::load();
        self.sync_fields();
        self.refresh_history();
        let (tx, rx) = mpsc::channel::<Thread>();
        self.writer = Some(tx);
        let errors = self.save_error.clone();
        self.writer_thread = Some(std::thread::spawn(move || {
            while let Ok(mut thread) = rx.recv() {
                // Preserve each conversation, coalescing only consecutive saves of the same one.
                let mut queued = vec![];
                while let Ok(next) = rx.try_recv() {
                    if next.id == thread.id {
                        thread = next;
                    } else {
                        queued.push(thread);
                        thread = next;
                    }
                }
                queued.push(thread);
                for thread in queued {
                    let result = config::write(
                        &config::root()
                            .join("threads")
                            .join(format!("{}.json", thread.id)),
                        &thread,
                    )
                    .and_then(|_| thread.settings.save());
                    if let Err(e) = result {
                        *errors.lock().unwrap() = Some(e);
                    }
                }
            }
        }));
    }
    pub fn sync_fields(&mut self) {
        self.config_args = serde_json::to_string(&self.settings.profile.args).unwrap();
        self.config_directory = self.settings.directory.display().to_string();
    }
    pub fn refresh_history(&mut self) {
        let mut files: Vec<_> = std::fs::read_dir(config::root().join("threads"))
            .into_iter()
            .flatten()
            .flatten()
            .filter(|e| {
                e.path().extension().is_some_and(|v| v == "json")
                    && e.file_type().is_ok_and(|t| t.is_file())
                    && e.metadata().is_ok_and(|m| m.len() < 4 * 1024 * 1024)
            })
            .collect();
        files.sort_by_key(|e| std::cmp::Reverse(e.metadata().and_then(|m| m.modified()).ok()));
        self.history = files
            .into_iter()
            .take(50)
            .filter_map(|e| {
                std::fs::read(e.path())
                    .ok()
                    .and_then(|b| serde_json::from_slice::<Thread>(&b).ok())
            })
            .filter(|t| {
                !t.id.is_empty() && t.id.len() <= 64 && t.id.bytes().all(|c| c.is_ascii_hexdigit())
            })
            .collect();
        self.history.sort_by_key(|t| std::cmp::Reverse(t.updated));
        self.history.truncate(50);
    }
    pub fn note(&mut self, role: &str, text: impl Into<String>) {
        let Some(thread) = &mut self.thread else {
            return;
        };
        thread.messages.push(Entry {
            role: role.into(),
            text: text.into().chars().take(32_000).collect(),
            id: String::new(),
            status: String::new(),
        });
        if thread.messages.len() > 400 {
            thread.messages.drain(..thread.messages.len() - 400);
        }
        self.dirty = true;
    }
    pub fn persist(&mut self, force: bool) {
        if !self.dirty || (!force && self.last_save.elapsed() < Duration::from_secs(1)) {
            return;
        }
        if let Some(thread) = &mut self.thread {
            while thread.messages.len() > 400
                || (thread.messages.len() > 1
                    && thread.messages.iter().map(|m| m.text.len()).sum::<usize>()
                        > 2 * 1024 * 1024)
            {
                thread.messages.remove(0);
            }
            thread.settings = self.settings.clone();
            thread.updated = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            if let Some(writer) = &self.writer {
                if writer.send(thread.clone()).is_err() {
                    self.error = "Could not save the conversation".into();
                }
            }
        }
        self.last_save = Instant::now();
        self.dirty = false;
    }
    pub fn disconnect(&mut self) {
        if let Some(c) = &self.connection {
            c.cancel();
        }
        self.connection = None;
        self.ready = false;
        self.connecting = false;
        self.busy = false;
        self.permissions.clear();
        self.pending_prompt = None;
        self.persist(true);
    }
    pub fn new_thread(&mut self, studio: &Studio) {
        self.disconnect();
        self.thread = Some(Thread::new(
            self.settings.clone(),
            self.purpose,
            studio.path.clone(),
        ));
        self.owner = Some(studio.swap_id.clone());
        self.edits = 0;
        self.error.clear();
        self.status = "Ready for a new conversation".into();
        self.metadata = Value::Null;
    }
    pub fn connect(
        &mut self,
        studio: &Studio,
        ctx: &eframe::egui::Context,
        binary: PathBuf,
    ) -> Result<(), String> {
        self.load();
        self.settings.profile.args = serde_json::from_str::<Vec<String>>(&self.config_args)
            .map_err(|_| "Arguments must be a JSON array of strings")?;
        self.settings.directory = PathBuf::from(&self.config_directory)
            .canonicalize()
            .map_err(|e| format!("Project folder: {e}"))?;
        if self.thread.as_ref().is_some_and(|t| {
            t.settings.profile != self.settings.profile
                || t.settings.directory != self.settings.directory
                || t.purpose != self.purpose
        }) {
            self.new_thread(studio);
        }
        if self.thread.is_none() {
            self.new_thread(studio);
        }
        if self.owner.as_deref().is_some_and(|id| id != studio.swap_id) {
            return Err("This conversation belongs to another document. Open a new conversation for this canvas.".into());
        }
        self.owner = Some(studio.swap_id.clone());
        let resume = self.thread.as_ref().and_then(|t| t.session_id.clone());
        self.restoring = resume.is_some();
        self.connection = Some(Connection::start(
            self.settings.profile.clone(),
            self.settings.directory.clone(),
            resume,
            binary,
            ctx.clone(),
        )?);
        self.ready = false;
        self.connecting = true;
        self.status = format!("Connecting to {}…", self.settings.profile.name);
        self.error.clear();
        self.dirty = true;
        self.persist(true);
        Ok(())
    }
    pub fn send_prompt(
        &mut self,
        studio: &mut Studio,
        ctx: &eframe::egui::Context,
        binary: PathBuf,
    ) -> Result<(), String> {
        let request = self.request.trim().to_owned();
        if request.is_empty() {
            return Err("Describe what you want to make or change".into());
        }
        if request.len() > 32_768 {
            return Err("Keep the brief under 32 KB".into());
        }
        if self.busy || self.connecting {
            return Err("Wait for the current turn or stop it first".into());
        }
        if self.owner.as_deref().is_some_and(|id| id != studio.swap_id) {
            return Err("Start a new conversation for this document".into());
        }
        if !self.ready {
            self.connect(studio, ctx, binary)?;
        }
        if self.thread.as_ref().is_some_and(|t| t.messages.is_empty()) {
            self.thread.as_mut().unwrap().title = request
                .lines()
                .next()
                .unwrap_or("Design")
                .chars()
                .take(70)
                .collect();
        }
        self.note("user", request.clone());
        self.request.clear();
        self.error.clear();
        if self.purpose == Purpose::Create {
            studio.show_welcome = false;
            studio.need_fit = true;
        }
        if self.ready {
            self.start_turn(studio, &request)?;
        } else {
            self.pending_prompt = Some(request);
        }
        self.persist(true);
        Ok(())
    }
    fn start_turn(&mut self, studio: &Studio, request: &str) -> Result<(), String> {
        let guidance = if self.purpose == Purpose::Learn {
            "Help the user learn Omadesign. This is a read-only session. Use get_documentation and live canvas context. Do not change files or artwork."
        } else {
            "You are a designer working directly in the user's live Omadesign canvas. Use the omadesign MCP native design tools to create and refine editable artwork. The user watches each tool result appear immediately. Build in meaningful incremental steps: foundation, composition, typography, details, then inspect a canvas snapshot and refine. Never produce SVG code, scripts, terminal artwork or .oma files as a substitute for these tools. Do not use filesystem/shell tools for design changes. Keep narration concise and visual. Get current document context before editing; every mutation requires its current revision and returns the next revision. If a revision is stale, re-read context and adapt. Preserve existing work unless asked to replace it. Make native text with installed fonts. Use unlocked vector layers; create one when needed. Read-only tools work in all sessions. Inspect the snapshot before calling the design finished."
        };
        let prompt = format!(
            "Omadesign design harness instructions:\n{guidance}\nCanvas: {} ({} × {} pixels). Live editing: {}.\n\nUser request:\n{request}",
            studio.doc.name,
            studio.doc.width,
            studio.doc.height,
            self.settings.live_edits && self.purpose == Purpose::Create
        );
        self.connection
            .as_ref()
            .ok_or("Agent is disconnected")?
            .send(Command::Prompt(prompt))?;
        self.busy = true;
        self.status = "Working on your design…".into();
        Ok(())
    }
    pub fn stop(&mut self) {
        if let Some(c) = &self.connection {
            c.cancel();
        }
        self.pending_prompt = None;
        self.status = "Stopping…".into();
        self.permissions.clear();
    }
    pub fn restore(&mut self, thread: Thread, studio: &Studio) -> Result<(), String> {
        if thread.document.is_none() || thread.document != studio.path {
            return Err("Open this conversation's saved document before continuing it. Unsaved-document conversations remain readable in history.".into());
        }
        self.disconnect();
        self.settings = thread.settings.clone();
        self.purpose = thread.purpose;
        self.sync_fields();
        self.thread = Some(thread);
        self.owner = Some(studio.swap_id.clone());
        self.status = "Conversation loaded · Connect to continue".into();
        self.show_history = false;
        Ok(())
    }
    pub fn poll(&mut self, studio: &mut Studio, ctx: &eframe::egui::Context) {
        if self.owner.as_deref() == Some(&studio.swap_id)
            && let Some(thread) = &mut self.thread
            && thread.document != studio.path
        {
            thread.document = studio.path.clone();
            self.dirty = true;
        }
        if let Some(error) = self.save_error.lock().unwrap().take() {
            self.error = format!("Conversation save failed: {error}");
        }
        if self.connection.is_some() && self.owner.as_deref() != Some(&studio.swap_id) {
            self.note(
                "system",
                "Disconnected because the active document changed.",
            );
            self.disconnect();
            self.status = "Open a new conversation for this document".into();
        }
        let events: Vec<_> = self
            .connection
            .as_ref()
            .map(|c| c.events.try_iter().take(256).collect())
            .unwrap_or_default();
        for event in events {
            match event {
                Event::Initialized(value) => {
                    self.initialization = value;
                }
                Event::Session { id, data, restored } => {
                    self.metadata = data;
                    self.ready = true;
                    self.connecting = false;
                    self.status = "Connected · ready to design".into();
                    if let Some(thread) = &mut self.thread {
                        thread.session_id = Some(id);
                    }
                    if self.restoring && !restored {
                        self.note("system","This agent started a new session; previous messages remain available here.");
                    }
                    self.restoring = false;
                    self.dirty = true;
                    if let Some(prompt) = self.pending_prompt.take() {
                        if let Err(e) = self.start_turn(studio, &prompt) {
                            self.error = e;
                        }
                    }
                }
                Event::Update(update) => {
                    // Saved messages are already present; session/load replay must
                    // not duplicate them. Configuration updates remain useful.
                    if !self.restoring
                        || matches!(
                            update["sessionUpdate"].as_str(),
                            Some("config_option_update" | "current_mode_update")
                        )
                    {
                        self.update(update);
                    }
                }
                Event::Permission { id, params } => {
                    self.permissions.push((id, params));
                    self.status = "Agent needs your permission".into();
                }
                Event::Complete(reason) => {
                    self.busy = false;
                    self.permissions.clear();
                    self.status = if reason == "cancelled" {
                        "Stopped · completed edits remain undoable".into()
                    } else if reason == "error" {
                        "Turn failed · completed edits remain undoable".into()
                    } else {
                        format!("Ready · {} live edits", self.edits)
                    };
                    self.persist(true);
                }
                Event::Error(error) => {
                    self.error = error;
                    self.connecting = false;
                }
                Event::Closed => {
                    self.connection = None;
                    self.ready = false;
                    self.connecting = false;
                    self.busy = false;
                    self.permissions.clear();
                    self.pending_prompt = None;
                    self.persist(true);
                }
            }
        }
        // One native operation per frame lets the canvas paint between tool calls.
        if let Some(call) = self
            .connection
            .as_ref()
            .and_then(|c| c.bridge.calls.try_recv().ok())
        {
            let allowed = self.busy
                && self
                    .connection
                    .as_ref()
                    .is_some_and(|c| c.bridge.accepting.load(Ordering::Acquire))
                && call.deadline > Instant::now()
                && self.owner.as_deref() == Some(&studio.swap_id);
            let result = if !allowed {
                Err("This design turn is no longer active".into())
            } else {
                tools::execute(
                    studio,
                    &call.name,
                    &call.arguments,
                    self.settings.live_edits && self.purpose == Purpose::Create,
                )
            };
            if result.is_ok() && tools::mutates(&call.name) {
                self.edits += 1;
                if self.follow_canvas {
                    studio.need_fit = true;
                }
                let object_name = call
                    .arguments
                    .get("shape")
                    .and_then(|s| s["name"].as_str())
                    .or_else(|| {
                        call.arguments
                            .get("changes")
                            .and_then(|s| s["name"].as_str())
                    })
                    .or_else(|| call.arguments["name"].as_str());
                let action = match call.name.as_str() {
                    "add_shape" => "Added",
                    "update_shape" => "Updated",
                    "create_layer" => "Created layer",
                    "update_layer" => "Updated layer",
                    "remove_shapes" => "Removed objects",
                    "set_effects" => "Applied effects",
                    _ => "Updated design",
                };
                self.note(
                    "design",
                    object_name
                        .map_or_else(|| action.to_string(), |name| format!("{action} {name}")),
                );
                if let Some(thread) = &mut self.thread {
                    thread.document = studio.path.clone();
                }
            }
            let _ = call.reply.send(result.unwrap_or_else(bridge::failure));
            ctx.request_repaint();
        }
        self.persist(false);
        if self.busy || self.connecting {
            ctx.request_repaint_after(Duration::from_millis(33));
        }
    }
    fn update(&mut self, value: Value) {
        match value["sessionUpdate"].as_str().unwrap_or_default() {
            "agent_message_chunk" => {
                if let Some(text) = value["content"]["text"].as_str() {
                    if let Some(thread) = &mut self.thread {
                        if let Some(last) = thread
                            .messages
                            .last_mut()
                            .filter(|m| m.role == "assistant" && m.text.len() < 64_000)
                        {
                            last.text
                                .extend(text.chars().take(64_000 - last.text.len()));
                        } else {
                            self.note("assistant", text);
                        }
                        self.dirty = true;
                    }
                }
            }
            "agent_thought_chunk" => {
                self.status = "Thinking about the design…".into();
            }
            "tool_call" | "tool_call_update" => {
                let id = value["toolCallId"].as_str().unwrap_or_default();
                let Some(thread) = &mut self.thread else {
                    return;
                };
                let pos = thread
                    .messages
                    .iter()
                    .position(|m| m.role == "tool" && m.id == id);
                if let Some(i) = pos {
                    let m = &mut thread.messages[i];
                    if let Some(title) = value["title"].as_str() {
                        m.text = title.chars().take(500).collect();
                    }
                    if let Some(status) = value["status"].as_str() {
                        m.status = status.into();
                    }
                } else {
                    thread.messages.push(Entry {
                        role: "tool".into(),
                        id: id.into(),
                        text: value["title"]
                            .as_str()
                            .unwrap_or("Using a tool")
                            .chars()
                            .take(500)
                            .collect(),
                        status: value["status"].as_str().unwrap_or("pending").into(),
                    });
                }
                self.dirty = true;
            }
            "plan" => {
                let steps = value["entries"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|e| {
                        e["content"]
                            .as_str()
                            .map(|s| format!("{} · {s}", e["status"].as_str().unwrap_or("pending")))
                    })
                    .collect::<Vec<_>>()
                    .join("\n");
                self.note("plan", steps);
            }
            "current_mode_update" => {
                self.metadata["modes"]["currentModeId"] = value["currentModeId"].clone();
            }
            "current_model_update" => {
                self.metadata["models"]["currentModelId"] = value["currentModelId"].clone();
            }
            "config_option_update" => {
                if value["configOptions"].is_array() {
                    self.metadata["configOptions"] = value["configOptions"].clone();
                }
            }
            _ => (),
        }
    }
    pub fn permission(&mut self, id: Value, option: Option<String>) {
        if let Some(c) = &self.connection {
            let _ = c.send(Command::Permission {
                id: id.clone(),
                option,
            });
        }
        self.permissions.retain(|(i, _)| *i != id);
        self.status = "Working on your design…".into();
    }
}
impl Drop for Workspace {
    fn drop(&mut self) {
        self.persist(true);
        self.writer.take();
        if let Some(writer) = self.writer_thread.take() {
            let _ = writer.join();
        }
    }
}
