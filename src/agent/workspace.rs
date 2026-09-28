use super::{
    Purpose,
    attachments::{self, Attachment},
    bridge,
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
    #[serde(default)]
    pub attachments: Vec<Attachment>,
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
    pub discovery: super::discovery::Discovery,
    pub loaded: bool,
    pub settings: Settings,
    pub purpose: Purpose,
    pub request: String,
    pub attachments: Vec<Attachment>,
    pub attachment_jobs: Vec<attachments::Job>,
    turn_job: Option<attachments::TurnJob>,
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
    pending_prompt: Option<(String, Vec<Attachment>)>,
    restoring: bool,
}
impl Default for Workspace {
    fn default() -> Self {
        let settings = Settings::default();
        Self {
            visible: false,
            discovery: Default::default(),
            loaded: false,
            config_args: serde_json::to_string(&settings.profile.args).unwrap(),
            config_directory: settings.directory.display().to_string(),
            settings,
            purpose: Purpose::Create,
            request: String::new(),
            attachments: vec![],
            attachment_jobs: vec![],
            turn_job: None,
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
        self.discovery.filter = Some(self.settings.profile.name.clone());
        self.sync_fields();
        self.refresh_history();
        let (tx, rx) = mpsc::channel::<Thread>();
        self.writer = Some(tx);
        let errors = self.save_error.clone();
        let mut projects = self
            .history
            .iter()
            .map(|t| t.settings.directory.clone())
            .collect::<Vec<_>>();
        projects.push(self.settings.directory.clone());
        projects.sort();
        projects.dedup();
        self.writer_thread = Some(std::thread::spawn(move || {
            // Upgrade existing caches when the Agent panel opens, without
            // waiting for another paste or blocking the native UI on disk I/O.
            for project in projects {
                if let Err(e) = attachments::secure_existing_cache(&project) {
                    *errors.lock().unwrap() = Some(format!("Attachment privacy: {e}"));
                }
            }
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
    pub fn attach(
        &mut self,
        studio: &Studio,
        input: attachments::Input,
        range: std::ops::Range<usize>,
        ctx: &eframe::egui::Context,
    ) {
        if self.attachment_jobs.len() >= 4 {
            self.error = "Wait for the current attachments to finish before pasting more".into();
            return;
        }
        if self.thread.is_none() {
            let request = std::mem::take(&mut self.request);
            self.new_thread(studio);
            self.request = request;
        }
        let thread = self.thread.as_ref().unwrap().id.clone();
        let directory = self
            .settings
            .directory
            .join(".omadesign")
            .join("agent-attachments")
            .join(&thread);
        let replaced = self
            .request
            .chars()
            .skip(range.start)
            .take(range.end - range.start)
            .collect();
        self.focus_prompt = true;
        self.attachment_jobs.push(attachments::spawn(
            input,
            directory,
            thread,
            range,
            replaced,
            ctx.clone(),
        ));
    }
    pub fn poll_attachments(&mut self) -> Option<attachments::Insert> {
        // Finish in paste order, even if a later small payload decodes sooner.
        let result = match self.attachment_jobs.first()?.rx.try_recv() {
            Ok(result) => result,
            Err(mpsc::TryRecvError::Empty) => return None,
            Err(_) => Err("Attachment worker stopped unexpectedly".into()),
        };
        let job = self.attachment_jobs.remove(0);
        if self.thread.as_ref().map(|t| t.id.as_str()) != Some(job.thread.as_str()) {
            return None;
        }
        match result {
            Ok(prepared) => {
                let current = self
                    .request
                    .chars()
                    .skip(job.at)
                    .take(job.end - job.at)
                    .collect::<String>();
                let end = if current == job.replaced {
                    job.end
                } else {
                    job.at
                };
                let mut prospective = self.request.chars().take(job.at).collect::<String>();
                prospective.push_str(&prepared.text);
                prospective.extend(self.request.chars().skip(end));
                let retained = self
                    .attachments
                    .iter()
                    .filter(|a| prospective.contains(&a.token()))
                    .count();
                if retained + prepared.attachments.len() > attachments::MAX_ATTACHMENTS {
                    self.error = "A prompt can contain at most 20 attachments".into();
                    return None;
                }
                self.error.clear();
                Some(attachments::Insert {
                    at: job.at,
                    end,
                    text: prepared.text,
                    attachments: prepared.attachments,
                })
            }
            Err(e) => {
                self.error = e;
                None
            }
        }
    }
    pub fn rebase_attachment_jobs(&mut self, before: &str) {
        if self.attachment_jobs.is_empty() || before == self.request {
            return;
        }
        let old: Vec<_> = before.chars().collect();
        let new: Vec<_> = self.request.chars().collect();
        let start = old.iter().zip(&new).take_while(|(a, b)| a == b).count();
        let suffix = old[start..]
            .iter()
            .rev()
            .zip(new[start..].iter().rev())
            .take_while(|(a, b)| a == b)
            .count();
        let end = old.len() - suffix;
        let added = new.len() - suffix - start;
        self.rebase_attachment_edit(start..end, added);
    }
    pub fn rebase_attachment_edit(&mut self, range: std::ops::Range<usize>, added: usize) {
        let (start, end) = (range.start, range.end);
        if start == end && added == 0 {
            return;
        }
        let delta = added as isize - (end - start) as isize;
        let count = self.attachment_jobs.len();
        self.attachment_jobs.retain_mut(|job| {
            if end <= job.at {
                job.at = job.at.saturating_add_signed(delta);
                job.end = job.end.saturating_add_signed(delta);
                true
            } else if start >= job.end {
                true
            } else {
                // The user replaced the pending destination. Keep their edit;
                // a later worker completion must not overwrite unrelated text.
                false
            }
        });
        if self.attachment_jobs.len() != count {
            self.error = "Paste cancelled because its destination was edited; paste again at the new cursor position".into();
        }
    }
    pub fn insert_attachment_result(&mut self, insert: attachments::Insert) -> usize {
        let len = self.request.chars().count();
        let at = insert.at.min(len);
        let end = insert.end.min(len).max(at);
        let byte = |index| {
            self.request
                .char_indices()
                .nth(index)
                .map(|(i, _)| i)
                .unwrap_or(self.request.len())
        };
        let start_byte = byte(at);
        let end_byte = byte(end);
        self.request
            .replace_range(start_byte..end_byte, &insert.text);
        attachments::reconcile(&self.request, &mut self.attachments);
        self.attachments.extend(insert.attachments);
        let added = insert.text.chars().count();
        let delta = added as isize - (end - at) as isize;
        for job in &mut self.attachment_jobs {
            let shift = |position: usize| {
                if position >= end {
                    position.saturating_add_signed(delta)
                } else if position >= at {
                    at + added
                } else {
                    position
                }
            };
            job.at = shift(job.at);
            job.end = shift(job.end).max(job.at);
        }
        at + added
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
            attachments: vec![],
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
        if let Some((request, attachments)) = self.pending_prompt.take() {
            self.restore_unsent(request, attachments);
        }
        if let Some(job) = self.turn_job.take() {
            self.restore_unsent(job.request, job.attachments);
        }
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
        self.attachments.clear();
        self.attachment_jobs.clear();
        self.request.clear();
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
        if attachments::typed_len(&request, &self.attachments) > 32_768 {
            return Err("Keep the brief under 32 KB".into());
        }
        if !self.attachment_jobs.is_empty() {
            return Err("Wait for attachments to finish preparing".into());
        }
        if self.busy || self.connecting {
            return Err("Wait for the current turn or stop it first".into());
        }
        if self.owner.as_deref().is_some_and(|id| id != studio.swap_id) {
            return Err("Start a new conversation for this document".into());
        }
        if !self.ready {
            let draft = std::mem::take(&mut self.attachments);
            let result = self.connect(studio, ctx, binary);
            self.request = request.clone();
            self.attachments = draft;
            result?;
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
        let attachments = std::mem::take(&mut self.attachments);
        self.request.clear();
        self.error.clear();
        if self.purpose == Purpose::Create {
            studio.show_welcome = false;
            studio.need_fit = true;
        }
        if self.ready {
            self.start_turn(studio, request, attachments, ctx)?;
        } else {
            self.pending_prompt = Some((request, attachments));
        }
        self.persist(true);
        Ok(())
    }
    fn start_turn(
        &mut self,
        _studio: &Studio,
        request: String,
        attachments: Vec<Attachment>,
        ctx: &eframe::egui::Context,
    ) -> Result<(), String> {
        let (tx, rx) = mpsc::channel();
        let capabilities = self.initialization["agentCapabilities"]["promptCapabilities"].clone();
        let job = attachments::TurnJob {
            rx,
            request: request.clone(),
            attachments: attachments.clone(),
        };
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let result = attachments::payload(request, attachments, &capabilities);
            let _ = tx.send(result);
            ctx.request_repaint();
        });
        self.turn_job = Some(job);
        self.busy = true;
        self.status = "Preparing attachments…".into();
        Ok(())
    }
    fn dispatch_turn(&mut self, studio: &Studio, turn: attachments::Turn) -> Result<(), String> {
        let request = &turn.request;
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
        let mut blocks = vec![serde_json::json!({"type":"text","text":prompt})];
        blocks.extend(turn.blocks);
        self.connection
            .as_ref()
            .ok_or("Agent is disconnected")?
            .send(Command::Prompt(blocks))?;
        self.note("user", turn.request.clone());
        if let Some(entry) = self.thread.as_mut().and_then(|t| t.messages.last_mut()) {
            // The brief limit excludes markers; retain all user references in history.
            entry.text = turn.request;
            entry.attachments = turn.attachments;
        }
        self.persist(true);
        self.busy = true;
        self.status = "Working on your design…".into();
        Ok(())
    }
    pub fn save_preferences(&mut self) {
        self.dirty = true;
        if self.thread.is_some() && self.writer.is_some() {
            self.persist(true);
        } else if let Err(e) = self.settings.save() {
            self.error = e;
        }
    }
    pub fn choose_option(&mut self, option: &super::discovery::OptionSet, value: &str) {
        if !option.choices.iter().any(|c| c.id == value) {
            return;
        }
        self.settings.selections.insert(
            format!("{}/{}", self.settings.profile.name, option.id),
            value.into(),
        );
        self.save_preferences();
        if self.ready
            && let Some(c) = &self.connection
        {
            let command = if option.legacy {
                if option.category == "model" {
                    Command::Model(value.into())
                } else {
                    Command::Mode(value.into())
                }
            } else {
                Command::Config {
                    id: option.id.clone(),
                    value: value.into(),
                }
            };
            if let Err(e) = c.send(command) {
                self.error = e;
            }
        }
    }
    fn restore_options(&mut self) {
        for option in super::discovery::options(&self.metadata) {
            if let Some(value) = self
                .settings
                .selections
                .get(&format!("{}/{}", self.settings.profile.name, option.id))
                .cloned()
                && value != option.current
                && option.choices.iter().any(|c| c.id == value)
            {
                self.choose_option(&option, &value);
            }
        }
    }
    fn restore_unsent(&mut self, mut request: String, mut attachments: Vec<Attachment>) {
        // The composer stays editable while preparation runs. Preserve both the
        // unsent turn and any next draft, including pending paste destinations.
        if !request.is_empty() && !self.request.is_empty() {
            request.push('\n');
        }
        let offset = request.chars().count();
        for job in &mut self.attachment_jobs {
            job.at += offset;
            job.end += offset;
        }
        request.push_str(&self.request);
        attachments.append(&mut self.attachments);
        self.request = request;
        self.attachments = attachments;
        self.focus_prompt = true;
    }
    pub fn stop(&mut self) {
        if let Some(job) = self.turn_job.take() {
            self.restore_unsent(job.request, job.attachments);
            self.busy = false;
            self.status = "Stopped preparing attachments".into();
            return;
        }
        if let Some(c) = &self.connection {
            c.cancel();
        }
        if let Some((request, attachments)) = self.pending_prompt.take() {
            self.restore_unsent(request, attachments);
        }
        self.status = "Stopping…".into();
        self.permissions.clear();
    }
    pub fn restore(&mut self, thread: Thread, studio: &Studio) -> Result<(), String> {
        if thread.document.is_none() || thread.document != studio.path {
            return Err("Open this conversation's saved document before continuing it. Unsaved-document conversations remain readable in history.".into());
        }
        self.disconnect();
        let favorites = self.settings.favorites.clone();
        self.settings = thread.settings.clone();
        self.settings.favorites = favorites;
        self.purpose = thread.purpose;
        self.sync_fields();
        self.request.clear();
        self.attachments.clear();
        self.attachment_jobs.clear();
        self.thread = Some(thread);
        self.owner = Some(studio.swap_id.clone());
        self.status = "Conversation loaded · Connect to continue".into();
        self.show_history = false;
        Ok(())
    }
    pub fn poll(&mut self, studio: &mut Studio, ctx: &eframe::egui::Context) {
        if let Some(result) = self
            .turn_job
            .as_ref()
            .and_then(|job| match job.rx.try_recv() {
                Ok(r) => Some(r),
                Err(mpsc::TryRecvError::Disconnected) => {
                    Some(Err("Attachment preparation worker stopped".into()))
                }
                Err(_) => None,
            })
        {
            let job = self.turn_job.take().unwrap();
            let result = result.and_then(|turn| self.dispatch_turn(studio, turn));
            if let Err(e) = result {
                self.error = e;
                self.busy = false;
                self.status = "Could not send attachments".into();
                self.restore_unsent(job.request, job.attachments);
            }
        }

        if self.owner.as_deref() == Some(&studio.swap_id)
            && let Some(thread) = &mut self.thread
            && thread.document != studio.path
        {
            thread.document = studio.path.clone();
            self.dirty = true;
        }
        if let Some(error) = self.save_error.lock().unwrap().take() {
            self.error = format!("Agent data: {error}");
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
                    self.restore_options();
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
                    if let Some((prompt, attachments)) = self.pending_prompt.take() {
                        if let Err(e) = self.start_turn(studio, prompt, attachments, ctx) {
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
                    if !self.ready
                        && let Some((request, attachments)) = self.pending_prompt.take()
                    {
                        self.restore_unsent(request, attachments);
                    }
                }
                Event::Closed => {
                    self.connection = None;
                    self.ready = false;
                    self.connecting = false;
                    self.busy = false;
                    self.permissions.clear();
                    if let Some((request, attachments)) = self.pending_prompt.take() {
                        self.restore_unsent(request, attachments);
                    }
                    if let Some(job) = self.turn_job.take() {
                        self.restore_unsent(job.request, job.attachments);
                    }
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
                        attachments: vec![],
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

#[cfg(test)]
mod attachment_recovery_tests {
    use super::*;

    #[test]
    fn readiness_pending_paste_handles_deletion_suffix_and_destination_edits() {
        for (after, expected) in [
            ("replace tail", Some((0, 7))),
            ("你好 replace tail with suffix", Some((3, 10))),
            ("你好 changed tail", None),
        ] {
            let mut workspace = Workspace::default();
            let (_tx, rx) = mpsc::channel();
            workspace.attachment_jobs.push(attachments::Job {
                thread: "thread".into(),
                at: 3,
                end: 10,
                replaced: "replace".into(),
                rx,
            });
            workspace.request = after.into();
            workspace.rebase_attachment_jobs("你好 replace tail");
            assert_eq!(
                workspace.attachment_jobs.first().map(|j| (j.at, j.end)),
                expected
            );
            assert_eq!(workspace.request, after);
            if expected.is_none() {
                assert!(workspace.error.contains("destination was edited"));
            }
        }
    }

    #[test]
    fn readiness_startup_exit_or_rejection_restores_unsent_draft_and_attachments() {
        for (script, exits) in [
            ("exit 17", true),
            (
                r#"printf '%s\n' '{"jsonrpc":"2.0","id":1,"error":{"code":-32000,"message":"Rejected startup"}}'; while read ignored; do :; done"#,
                false,
            ),
        ] {
            let mut studio = Studio::new();
            let ctx = eframe::egui::Context::default();
            let mut workspace = Workspace::default();
            workspace.loaded = true;
            workspace.settings.profile = config::Profile {
                name: "Startup failure regression".into(),
                command: "/bin/sh".into(),
                args: vec!["-c".into(), script.into()],
            };
            workspace.config_args =
                serde_json::to_string(&workspace.settings.profile.args).unwrap();
            workspace.config_directory = std::env::temp_dir().display().to_string();
            let attachment = Attachment {
                id: "private".into(),
                kind: attachments::Kind::Text,
                mime: "text/plain".into(),
                name: "brief.txt".into(),
                size: 10,
                source: "/tmp/unsent-private-brief.txt".into(),
                dimensions: None,
                delivery: String::new(),
                preview: None,
                thumbnail: None,
            };
            let draft = format!("Keep my unsent brief {}", attachment.token());
            workspace.request = draft.clone();
            workspace.attachments.push(attachment);
            workspace
                .send_prompt(&mut studio, &ctx, std::env::current_exe().unwrap())
                .unwrap();
            assert!(workspace.connecting);
            workspace.request = "New notes typed while connecting".into();
            let deadline = Instant::now() + Duration::from_secs(10);
            while (if exits {
                workspace.connection.is_some()
            } else {
                workspace.error.is_empty()
            }) && Instant::now() < deadline
            {
                workspace.poll(&mut studio, &ctx);
                std::thread::sleep(Duration::from_millis(10));
            }
            if exits {
                assert!(workspace.connection.is_none(), "ACP fixture did not exit");
            }
            assert!(!workspace.error.is_empty());
            assert_eq!(
                workspace.request,
                format!("{draft}\nNew notes typed while connecting")
            );
            assert_eq!(workspace.attachments.len(), 1);
            assert_eq!(workspace.attachments[0].id, "private");
            assert!(
                workspace
                    .thread
                    .as_ref()
                    .unwrap()
                    .messages
                    .iter()
                    .all(|e| e.role != "user")
            );
            assert!(workspace.pending_prompt.is_none());
            workspace.disconnect();
        }
    }

    #[test]
    fn attachment_failed_or_stopped_send_preserves_concurrent_draft_and_paste_position() {
        for scenario in [
            "failed",
            "stopped",
            "connecting",
            "disconnect-connecting",
            "disconnect-preparing",
            "document-change",
        ] {
            let mut studio = Studio::new();
            let ctx = eframe::egui::Context::default();
            let mut workspace = Workspace::default();
            workspace.new_thread(&studio);
            let attachment = |id: &str| Attachment {
                id: id.into(),
                kind: attachments::Kind::File,
                mime: "application/pdf".into(),
                name: format!("{id}.pdf"),
                size: 1,
                source: format!("/tmp/{id}.pdf").into(),
                dimensions: None,
                delivery: String::new(),
                preview: None,
                thumbnail: None,
            };
            let old = attachment("old");
            let new = attachment("new");
            let old_request = format!("Previous {}", old.token());
            let next_request = format!("Next {}", new.token());
            workspace.request = next_request.clone();
            workspace.attachments = vec![new];
            let (_paste_tx, paste_rx) = mpsc::channel();
            workspace.attachment_jobs.push(attachments::Job {
                thread: workspace.thread.as_ref().unwrap().id.clone(),
                at: 2,
                end: 4,
                replaced: "xt".into(),
                rx: paste_rx,
            });
            if matches!(
                scenario,
                "connecting" | "disconnect-connecting" | "document-change"
            ) {
                workspace.pending_prompt = Some((old_request.clone(), vec![old]));
                if scenario == "connecting" {
                    workspace.stop();
                } else if scenario == "document-change" {
                    let (commands, _receiver) = mpsc::channel();
                    let (_sender, events) = mpsc::channel();
                    workspace.connection = Some(Connection {
                        bridge: bridge::Bridge::start(ctx.clone()).unwrap(),
                        commands,
                        events,
                        worker: None,
                    });
                    workspace.owner = Some("previous-document".into());
                    workspace.poll(&mut studio, &ctx);
                    assert!(workspace.connection.is_none());
                } else {
                    workspace.disconnect();
                }
            } else {
                let (tx, rx) = mpsc::channel();
                workspace.turn_job = Some(attachments::TurnJob {
                    rx,
                    request: old_request.clone(),
                    attachments: vec![old],
                });
                workspace.busy = true;
                if scenario == "failed" {
                    tx.send(Err("Attachment disappeared before send".into()))
                        .unwrap();
                    workspace.poll(&mut studio, &ctx);
                    assert!(workspace.error.contains("disappeared"));
                } else if scenario == "disconnect-preparing" {
                    workspace.disconnect();
                } else {
                    workspace.stop();
                }
                assert!(!workspace.busy);
            }
            assert_eq!(workspace.request, format!("{old_request}\n{next_request}"));
            assert_eq!(
                workspace
                    .attachments
                    .iter()
                    .map(|a| a.id.as_str())
                    .collect::<Vec<_>>(),
                vec!["old", "new"]
            );
            assert_eq!(
                workspace.attachment_jobs[0].at,
                old_request.chars().count() + 3
            );
            assert_eq!(
                workspace.attachment_jobs[0].end,
                old_request.chars().count() + 5
            );
            assert!(workspace.pending_prompt.is_none());
            assert!(workspace.turn_job.is_none());
        }
    }
}
