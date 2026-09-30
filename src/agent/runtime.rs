//! ACP v1 over newline-delimited JSON-RPC, with an independently polled UI.
use super::{
    bridge::{self, Bridge},
    config::Profile,
};
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    io::{BufReader, Read},
    os::unix::process::CommandExt,
    path::PathBuf,
    process::{Child, ChildStdin, Command as Process, Stdio},
    sync::{
        atomic::Ordering,
        mpsc::{self, Receiver, Sender},
    },
    time::{Duration, Instant},
};

#[derive(Debug)]
pub enum Event {
    Initialized(Value),
    Session {
        id: String,
        data: Value,
        restored: bool,
    },
    Update(Value),
    Permission {
        id: Value,
        params: Value,
    },
    Complete(String),
    Error(String),
    Closed,
}
pub enum Command {
    Prompt(Vec<Value>),
    Cancel,
    Permission { id: Value, option: Option<String> },
    Authenticate(String),
    Model(String),
    Mode(String),
    Config { id: String, value: String },
    Shutdown,
}

pub struct Connection {
    pub bridge: Bridge,
    pub events: Receiver<Event>,
    pub commands: Sender<Command>,
    pub(crate) worker: Option<std::thread::JoinHandle<()>>,
}
impl Connection {
    pub fn start(
        profile: Profile,
        cwd: PathBuf,
        resume: Option<String>,
        binary: PathBuf,
        ctx: eframe::egui::Context,
    ) -> Result<Self, String> {
        if profile.command.trim().is_empty() {
            return Err("Choose an ACP agent command first".into());
        }
        let cwd = cwd
            .canonicalize()
            .map_err(|e| format!("Project folder: {e}"))?;
        if !cwd.is_dir() {
            return Err("Project folder must be a directory".into());
        }
        let bridge = Bridge::start(ctx.clone())?;
        let server = bridge.server(&binary);
        let (send, events) = mpsc::channel();
        let (commands, receive) = mpsc::channel();
        let accepting = bridge.accepting.clone();
        let writable = bridge.writable.clone();
        let worker = std::thread::spawn(move || {
            let result = run(
                profile,
                cwd,
                resume,
                server,
                receive,
                &send,
                &ctx,
                accepting.clone(),
                writable,
            );
            accepting.store(false, Ordering::Release);
            if let Err(e) = result {
                let _ = send.send(Event::Error(e));
            }
            let _ = send.send(Event::Closed);
            ctx.request_repaint();
        });
        Ok(Self {
            bridge,
            events,
            commands,
            worker: Some(worker),
        })
    }
    pub fn send(&self, command: Command) -> Result<(), String> {
        self.commands
            .send(command)
            .map_err(|_| "Agent disconnected".into())
    }
    pub fn cancel(&self) {
        self.bridge.accepting.store(false, Ordering::Release);
        let _ = self.send(Command::Cancel);
    }
}
impl Drop for Connection {
    fn drop(&mut self) {
        self.bridge.accepting.store(false, Ordering::Release);
        let _ = self.send(Command::Shutdown);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

struct ProcessGuard(Child);
impl Drop for ProcessGuard {
    fn drop(&mut self) {
        // The adapter and any MCP helper belong to this private process group.
        let _ = Process::new("kill")
            .args(["-TERM", "--", &format!("-{}", self.0.id())])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
#[derive(Clone)]
enum Pending {
    Initialize,
    Session(Option<String>),
    Authenticate,
    Prompt,
    Configure(Option<(&'static str, String)>),
}

fn request(
    input: &mut ChildStdin,
    pending: &mut HashMap<u64, Pending>,
    next: &mut u64,
    method: &str,
    params: Value,
    kind: Pending,
) -> Result<(), String> {
    let id = *next;
    *next += 1;
    pending.insert(id, kind);
    bridge::write_message(
        input,
        &json!({"jsonrpc":"2.0","id":id,"method":method,"params":params}),
    )
}

fn run(
    profile: Profile,
    cwd: PathBuf,
    resume: Option<String>,
    server: Value,
    commands: Receiver<Command>,
    events: &Sender<Event>,
    ctx: &eframe::egui::Context,
    accepting: std::sync::Arc<std::sync::atomic::AtomicBool>,
    writable: std::sync::Arc<std::sync::atomic::AtomicBool>,
) -> Result<(), String> {
    let child = Process::new(&profile.command)
        .args(&profile.args)
        .current_dir(&cwd)
        .env("OMADESIGN_AGENT_CLIENT", "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .process_group(0)
        .spawn()
        .map_err(|e| format!("Could not start {}: {e}", profile.name))?;
    let mut child = ProcessGuard(child);
    let mut input = child.0.stdin.take().ok_or("Agent stdin unavailable")?;
    let output = child.0.stdout.take().ok_or("Agent stdout unavailable")?;
    let errors = child.0.stderr.take().ok_or("Agent stderr unavailable")?;
    let diagnostics = std::sync::Arc::new(std::sync::Mutex::new(Vec::<u8>::new()));
    let tail = diagnostics.clone();
    std::thread::spawn(move || {
        let mut errors = errors;
        let mut buf = [0; 2048];
        while let Ok(n) = errors.read(&mut buf) {
            if n == 0 {
                break;
            }
            let mut tail = tail.lock().unwrap();
            tail.extend_from_slice(&buf[..n]);
            if tail.len() > 8192 {
                let excess = tail.len() - 8192;
                tail.drain(..excess);
            }
        }
    });
    let (wire_send, wire) = mpsc::sync_channel(256);
    let repaint = ctx.clone();
    std::thread::spawn(move || {
        let mut reader = BufReader::new(output);
        loop {
            let message = bridge::read_message(&mut reader);
            let ended = !matches!(&message, Ok(Some(_)));
            if wire_send.send(message).is_err() {
                break;
            }
            repaint.request_repaint();
            if ended {
                break;
            }
        }
    });
    let mut pending = HashMap::new();
    let mut next = 1;
    request(
        &mut input,
        &mut pending,
        &mut next,
        "initialize",
        json!({"protocolVersion":1,"clientInfo":{"name":"omadesign","title":"Omadesign","version":env!("CARGO_PKG_VERSION")},"clientCapabilities":{"fs":{"readTextFile":true,"writeTextFile":true},"terminal":false}}),
        Pending::Initialize,
    )?;
    let mut session: Option<String> = None;
    let mut permissions: HashMap<String, (Value, Vec<String>)> = HashMap::new();
    let mut startup = Some(Instant::now());
    let mut cancelling: Option<Instant> = None;
    let mut queued_prompt: Option<Vec<Value>> = None;
    let mut configurations = std::collections::VecDeque::new();
    loop {
        for command in commands.try_iter() {
            match command {
                Command::Shutdown => return Ok(()),
                Command::Cancel => {
                    queued_prompt = None;
                    accepting.store(false, Ordering::Release);
                    if let Some(id) = &session {
                        bridge::write_message(
                            &mut input,
                            &json!({"jsonrpc":"2.0","method":"session/cancel","params":{"sessionId":id}}),
                        )?;
                    }
                    for (_, (id, _)) in permissions.drain() {
                        bridge::write_message(
                            &mut input,
                            &json!({"jsonrpc":"2.0","id":id,"result":{"outcome":{"outcome":"cancelled"}}}),
                        )?;
                    }
                    cancelling = Some(Instant::now());
                }
                Command::Permission { id, option } => {
                    if let Some((_, allowed)) = permissions.remove(&id.to_string()) {
                        let outcome = match option.filter(|v| allowed.contains(v)) {
                            Some(id) => json!({"outcome":"selected","optionId":id}),
                            None => json!({"outcome":"cancelled"}),
                        };
                        bridge::write_message(
                            &mut input,
                            &json!({"jsonrpc":"2.0","id":id,"result":{"outcome":outcome}}),
                        )?;
                    }
                }
                Command::Authenticate(method) => {
                    request(
                        &mut input,
                        &mut pending,
                        &mut next,
                        "authenticate",
                        json!({"methodId":method}),
                        Pending::Authenticate,
                    )?;
                    startup = Some(Instant::now());
                }
                command => {
                    let Some(id) = &session else {
                        let _ =
                            events.send(Event::Error("Connect to an agent session first".into()));
                        let _ = events.send(Event::Complete("not_connected".into()));
                        continue;
                    };
                    match command {
                        Command::Prompt(text) => {
                            if queued_prompt.is_some()
                                || pending.values().any(|p| matches!(p, Pending::Prompt))
                            {
                                let _ = events
                                    .send(Event::Error("An agent turn is already running".into()));
                                continue;
                            }
                            cancelling = None;
                            queued_prompt = Some(text);
                        }
                        Command::Model(model) => configurations.push_back((
                            "session/set_model",
                            json!({"sessionId":id,"modelId":model}),
                            Pending::Configure(Some(("model", model))),
                        )),
                        Command::Mode(mode) => configurations.push_back((
                            "session/set_mode",
                            json!({"sessionId":id,"modeId":mode}),
                            Pending::Configure(Some(("mode", mode))),
                        )),
                        Command::Config { id: option, value } => configurations.push_back((
                            "session/set_config_option",
                            json!({"sessionId":id,"configId":option,"value":value}),
                            Pending::Configure(None),
                        )),
                        _ => (),
                    }
                }
            }
            ctx.request_repaint();
        }
        if !pending
            .values()
            .any(|p| matches!(p, Pending::Configure(_) | Pending::Prompt))
            && let Some((method, params, kind)) = configurations.pop_front()
        {
            request(&mut input, &mut pending, &mut next, method, params, kind)?;
        }
        if cancelling.is_none()
            && configurations.is_empty()
            && !pending.values().any(|p| matches!(p, Pending::Configure(_)))
            && let Some(text) = queued_prompt.take()
            && let Some(id) = &session
        {
            accepting.store(true, Ordering::Release);
            request(
                &mut input,
                &mut pending,
                &mut next,
                "session/prompt",
                json!({"sessionId":id,"prompt":text}),
                Pending::Prompt,
            )?;
        }
        if startup.is_some_and(|t| t.elapsed() > Duration::from_secs(120)) {
            return Err("Agent connection timed out. Check its command and local sign-in.".into());
        }
        if cancelling.is_some_and(|t| t.elapsed() > Duration::from_secs(5)) {
            let _ = events.send(Event::Complete("cancelled".into()));
            return Ok(());
        }
        let message = match wire.recv_timeout(Duration::from_millis(20)) {
            Ok(Ok(Some(v))) => v,
            Ok(Ok(None)) => {
                let diagnostic = String::from_utf8_lossy(&diagnostics.lock().unwrap()).into_owned();
                return Err(format!(
                    "{} exited. {}",
                    profile.name,
                    diagnostic.chars().take(1500).collect::<String>()
                ));
            }
            Ok(Err(e)) => return Err(e),
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                return Err("Agent output disconnected".into());
            }
            Err(mpsc::RecvTimeoutError::Timeout) => continue,
        };
        if let Some(method) = message["method"].as_str() {
            match method {
                "session/update" => {
                    if session
                        .as_deref()
                        .is_none_or(|id| message["params"]["sessionId"].as_str() == Some(id))
                    {
                        let _ = events.send(Event::Update(message["params"]["update"].clone()));
                    }
                }
                "fs/read_text_file" | "fs/write_text_file" => {
                    if let Some(id) = message.get("id") {
                        let params = &message["params"];
                        let result = filesystem_request(
                            method,
                            params,
                            session.as_deref(),
                            accepting.load(Ordering::Acquire) && cancelling.is_none(),
                            writable.load(Ordering::Acquire),
                        );
                        let reply = match result {
                            Ok(result) => json!({"jsonrpc":"2.0","id":id,"result":result}),
                            Err(error) => {
                                json!({"jsonrpc":"2.0","id":id,"error":{"code":-32000,"message":error}})
                            }
                        };
                        bridge::write_message(&mut input, &reply)?;
                    }
                }
                "session/request_permission" => {
                    if let Some(id) = message.get("id") {
                        if cancelling.is_some() || !accepting.load(Ordering::Acquire) {
                            bridge::write_message(
                                &mut input,
                                &json!({"jsonrpc":"2.0","id":id,"result":{"outcome":{"outcome":"cancelled"}}}),
                            )?;
                        } else {
                            let params = message["params"].clone();
                            let options = params["options"]
                                .as_array()
                                .into_iter()
                                .flatten()
                                .filter_map(|v| v["optionId"].as_str().map(str::to_owned))
                                .collect();
                            permissions.insert(id.to_string(), (id.clone(), options));
                            let _ = events.send(Event::Permission {
                                id: id.clone(),
                                params,
                            });
                        }
                    }
                }
                _ => {
                    if let Some(id) = message.get("id") {
                        bridge::write_message(
                            &mut input,
                            &json!({"jsonrpc":"2.0","id":id,"error":{"code":-32601,"message":"Client capability not supported"}}),
                        )?;
                    }
                }
            }
        } else if let Some(id) = message["id"].as_u64() {
            let Some(kind) = pending.remove(&id) else {
                continue;
            };
            if let Some(error) = message.get("error") {
                let _ = events.send(Event::Error(
                    error["message"]
                        .as_str()
                        .unwrap_or("Agent request failed")
                        .to_owned(),
                ));
                if matches!(kind, Pending::Configure(_)) {
                    configurations.clear();
                    if queued_prompt.take().is_some() {
                        let _ = events.send(Event::Complete("error".into()));
                    }
                }
                if matches!(kind, Pending::Prompt) {
                    accepting.store(false, Ordering::Release);
                    cancelling = None;
                    let _ = events.send(Event::Complete("error".into()));
                }
                startup = None;
                ctx.request_repaint();
                continue;
            }
            let result = &message["result"];
            match kind {
                Pending::Initialize => {
                    if result["protocolVersion"].as_u64() != Some(1) {
                        return Err("Agent does not support ACP protocol version 1".into());
                    }
                    let _ = events.send(Event::Initialized(result.clone()));
                    let restore = resume
                        .clone()
                        .filter(|_| result["agentCapabilities"]["loadSession"] == true);
                    let mut params = json!({"cwd":cwd,"mcpServers":[server]});
                    if let Some(id) = &restore {
                        params["sessionId"] = json!(id);
                    }
                    request(
                        &mut input,
                        &mut pending,
                        &mut next,
                        if restore.is_some() {
                            "session/load"
                        } else {
                            "session/new"
                        },
                        params,
                        Pending::Session(restore),
                    )?;
                }
                Pending::Session(restored) => {
                    session = result["sessionId"]
                        .as_str()
                        .map(str::to_owned)
                        .or_else(|| restored.clone());
                    let id = session.clone().ok_or("Agent did not return a session ID")?;
                    startup = None;
                    let _ = events.send(Event::Session {
                        id,
                        data: result.clone(),
                        restored: restored.is_some(),
                    });
                }
                Pending::Authenticate => {
                    request(
                        &mut input,
                        &mut pending,
                        &mut next,
                        "session/new",
                        json!({"cwd":cwd,"mcpServers":[server]}),
                        Pending::Session(None),
                    )?;
                }
                Pending::Prompt => {
                    accepting.store(false, Ordering::Release);
                    cancelling = None;
                    let _ = events.send(Event::Complete(
                        result["stopReason"].as_str().unwrap_or("end_turn").into(),
                    ));
                }
                Pending::Configure(setting) => {
                    if let Some((kind, id)) = setting {
                        let update = if kind == "model" {
                            json!({"sessionUpdate":"current_model_update","currentModelId":id})
                        } else {
                            json!({"sessionUpdate":"current_mode_update","currentModeId":id})
                        };
                        let _ = events.send(Event::Update(update));
                    }
                    let _=events.send(Event::Update(json!({"sessionUpdate":"config_option_update","configOptions":result["configOptions"]})));
                }
            }
        }
        ctx.request_repaint();
    }
}

/// ACP file delegation follows the same active-turn/read-only gate as MCP.
/// Text files are assets/context; live document edits use the native tool host.
pub(crate) fn filesystem_request(
    method: &str,
    args: &Value,
    session: Option<&str>,
    active: bool,
    writable: bool,
) -> Result<Value, String> {
    if !active || session.is_none() || args["sessionId"].as_str() != session {
        return Err("No matching active agent turn".into());
    }
    let path = super::tools::files::path(args)?;
    match method {
        "fs/read_text_file" => Ok(json!({"content":super::tools::files::read_text(&path,args)?})),
        "fs/write_text_file" => {
            if !writable {
                return Err(
                    "This session is read-only. Enable live edits in Create to write files.".into(),
                );
            }
            let content = args["content"]
                .as_str()
                .ok_or("content must be UTF-8 text")?;
            if content.len() > 4 * 1024 * 1024 {
                return Err("Text write exceeds 4 MiB".into());
            }
            if path.exists() {
                super::tools::files::regular(&path, 8 * 1024 * 1024)?;
            }
            crate::formats::write_atomic(&path, content.as_bytes())?;
            Ok(json!({}))
        }
        _ => Err("Unknown filesystem method".into()),
    }
}
