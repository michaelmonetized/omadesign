//! Private local MCP transport. Tool calls cross a bounded queue to the UI thread.
use serde_json::{Value, json};
use std::{
    io::{BufRead, BufReader, Read, Write},
    os::unix::{
        fs::PermissionsExt,
        io::AsRawFd,
        net::{SocketAddr, UnixListener, UnixStream},
    },
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
        mpsc::{self, Receiver, SyncSender},
    },
    time::{Duration, Instant},
};

pub const MAX_MESSAGE: usize = 8 * 1024 * 1024;
pub struct Call {
    pub name: String,
    pub arguments: Value,
    pub prepared: Option<Result<super::tools::files::Prepared, String>>,
    pub reply: mpsc::Sender<Value>,
    pub deadline: Instant,
}

pub struct Bridge {
    pub socket: PathBuf,
    pub token: String,
    pub calls: Receiver<Call>,
    pub accepting: Arc<AtomicBool>,
    pub writable: Arc<AtomicBool>,
    stop: Arc<AtomicBool>,
}

pub fn read_message(reader: &mut impl BufRead) -> Result<Option<Value>, String> {
    let mut bytes = Vec::new();
    let count = reader
        .take((MAX_MESSAGE + 1) as u64)
        .read_until(b'\n', &mut bytes)
        .map_err(|e| e.to_string())?;
    if count == 0 {
        return Ok(None);
    }
    if count > MAX_MESSAGE || bytes.last() != Some(&b'\n') {
        return Err("Protocol message is oversized or incomplete".into());
    }
    serde_json::from_slice(&bytes)
        .map(Some)
        .map_err(|e| format!("Invalid protocol JSON: {e}"))
}
pub fn write_message(writer: &mut impl Write, value: &Value) -> Result<(), String> {
    serde_json::to_writer(&mut *writer, value).map_err(|e| e.to_string())?;
    writer
        .write_all(b"\n")
        .and_then(|_| writer.flush())
        .map_err(|e| e.to_string())
}
pub fn failure(message: impl Into<String>) -> Value {
    json!({"content":[{"type":"text","text":message.into()}],"isError":true})
}

impl Bridge {
    pub fn start(ctx: eframe::egui::Context) -> Result<Self, String> {
        Self::start_in(ctx, &std::env::temp_dir())
    }

    /// Create the authenticated bridge beneath a chosen scratch folder.
    /// Args: repaint context and scratch root. Returns the bridge or its setup error.
    fn start_in(ctx: eframe::egui::Context, temporary: &std::path::Path) -> Result<Self, String> {
        let mut random = [0u8; 24];
        std::fs::File::open("/dev/urandom")
            .and_then(|mut f| f.read_exact(&mut random))
            .map_err(|e| e.to_string())?;
        let token: String = random.iter().map(|b| format!("{b:02x}")).collect();
        let directory = temporary.join(format!(
            "omadesign-acp-{}-{}",
            std::process::id(),
            &token[..12]
        ));
        std::fs::create_dir(&directory).map_err(|e| e.to_string())?;
        std::fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o700))
            .map_err(|e| e.to_string())?;
        let mut socket = directory.join("tools.sock");
        let directory_handle =
            if cfg!(target_os = "linux") && SocketAddr::from_pathname(&socket).is_err() {
                let handle = std::fs::File::open(&directory).map_err(|e| e.to_string())?;
                socket = PathBuf::from(format!(
                    "/proc/{}/fd/{}/tools.sock",
                    std::process::id(),
                    handle.as_raw_fd()
                ));
                Some(handle)
            } else {
                None
            };
        let listener = UnixListener::bind(&socket).map_err(|e| e.to_string())?;
        listener.set_nonblocking(true).map_err(|e| e.to_string())?;
        let (send, calls) = mpsc::sync_channel(16);
        let accepting = Arc::new(AtomicBool::new(false));
        let stop = Arc::new(AtomicBool::new(false));
        let stopped = stop.clone();
        let allowed = accepting.clone();
        let secret = token.clone();
        std::thread::spawn(move || {
            let clients = Arc::new(AtomicUsize::new(0));
            while !stopped.load(Ordering::Acquire) {
                match listener.accept() {
                    Ok((stream, _)) if clients.load(Ordering::Relaxed) < 8 => {
                        clients.fetch_add(1, Ordering::Relaxed);
                        let (send, allowed, stopped, secret, ctx, clients) = (
                            send.clone(),
                            allowed.clone(),
                            stopped.clone(),
                            secret.clone(),
                            ctx.clone(),
                            clients.clone(),
                        );
                        std::thread::spawn(move || {
                            serve(stream, send, allowed, stopped, &secret, ctx);
                            clients.fetch_sub(1, Ordering::Relaxed);
                        });
                    }
                    Ok(_) => (),
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(25))
                    }
                    Err(_) => break,
                }
            }
            drop(listener);
            let _ = std::fs::remove_dir_all(directory);
            drop(directory_handle);
        });
        Ok(Self {
            socket,
            token,
            calls,
            accepting,
            writable: Arc::new(AtomicBool::new(false)),
            stop,
        })
    }

    pub fn server(&self, binary: &std::path::Path) -> Value {
        json!({"name":"omadesign", "command":binary, "args":["--agent-mcp"], "env":[
            {"name":"OMADESIGN_AGENT_SOCKET","value":self.socket},
            {"name":"OMADESIGN_AGENT_TOKEN","value":self.token}
        ]})
    }
}

#[cfg(all(test, target_os = "linux"))]
mod socket_tests {
    use super::*;

    #[test]
    fn long_project_scratch_keeps_authenticated_socket_calls_and_cleanup() {
        struct Owned(PathBuf);
        impl Drop for Owned {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
        let owned = Owned(std::env::temp_dir().join(format!(
            "long-agent-parent-{}-{}",
            std::process::id(),
            crate::document::next_id()
        )));
        let temporary = owned.0.join("x".repeat(100));
        std::fs::create_dir_all(&temporary).unwrap();
        let bridge = Bridge::start_in(eframe::egui::Context::default(), &temporary).unwrap();
        let socket = bridge.socket.clone();
        assert!(
            socket
                .to_string_lossy()
                .starts_with(&format!("/proc/{}/fd/", std::process::id()))
        );
        let directory = std::fs::read_dir(&temporary)
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path();
        assert_eq!(
            std::fs::metadata(directory).unwrap().permissions().mode() & 0o777,
            0o700
        );
        bridge.accepting.store(true, Ordering::Release);
        let token = bridge.token.clone();
        let address = socket.clone();
        let worker = std::thread::spawn(move || {
            let mut stream = UnixStream::connect(address).unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            write_message(
                &mut stream,
                &json!({"token":token,"name":"get_document","arguments":{}}),
            )
            .unwrap();
            read_message(&mut BufReader::new(stream)).unwrap().unwrap()
        });
        let call = bridge.calls.recv_timeout(Duration::from_secs(2)).unwrap();
        assert_eq!(call.name, "get_document");
        call.reply.send(json!({"accepted":true})).unwrap();
        assert_eq!(worker.join().unwrap(), json!({"accepted":true}));
        drop(bridge);
        for _ in 0..100 {
            if !socket.exists() {
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(!socket.exists());
        assert_eq!(std::fs::read_dir(&temporary).unwrap().count(), 0);
    }
}
impl Drop for Bridge {
    fn drop(&mut self) {
        self.accepting.store(false, Ordering::Release);
        self.stop.store(true, Ordering::Release);
    }
}

fn serve(
    mut stream: UnixStream,
    send: SyncSender<Call>,
    accepting: Arc<AtomicBool>,
    stop: Arc<AtomicBool>,
    token: &str,
    ctx: eframe::egui::Context,
) {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(10)));
    let _ = stream.set_write_timeout(Some(Duration::from_secs(10)));
    let Ok(copy) = stream.try_clone() else {
        return;
    };
    let mut reader = BufReader::new(copy);
    let Ok(Some(request)) = read_message(&mut reader) else {
        return;
    };
    if request["token"].as_str() != Some(token) {
        let _ = write_message(&mut stream, &failure("Invalid design session"));
        return;
    }
    if !accepting.load(Ordering::Acquire) || stop.load(Ordering::Acquire) {
        let _ = write_message(
            &mut stream,
            &failure("No active design turn; send a prompt first"),
        );
        return;
    }
    let (reply, receive) = mpsc::channel();
    let name = request["name"].as_str().unwrap_or_default();
    let prepared = super::tools::files::prepare(name, &request["arguments"]);
    let call = Call {
        prepared,
        name: name.into(),
        arguments: request["arguments"].clone(),
        reply,
        deadline: Instant::now() + Duration::from_secs(60),
    };
    if send.try_send(call).is_err() {
        let _ = write_message(
            &mut stream,
            &failure("Editor is busy; retry after the current tool finishes"),
        );
        return;
    }
    ctx.request_repaint();
    let start = Instant::now();
    let result = loop {
        match receive.recv_timeout(Duration::from_millis(100)) {
            Ok(result) => break result,
            Err(mpsc::RecvTimeoutError::Disconnected) => break failure("Editor disconnected"),
            Err(_) if stop.load(Ordering::Acquire) || !accepting.load(Ordering::Acquire) => {
                break failure("Design turn cancelled");
            }
            Err(_) if start.elapsed() > Duration::from_secs(60) => {
                break failure("Editor tool timed out");
            }
            Err(_) => (),
        }
    };
    let _ = write_message(&mut stream, &result);
}

/// Runs in the child launched by the ACP agent, never in the editor process.
pub fn stdio() -> Result<(), String> {
    let socket =
        std::env::var("OMADESIGN_AGENT_SOCKET").map_err(|_| "Missing live editor connection")?;
    let token = std::env::var("OMADESIGN_AGENT_TOKEN").map_err(|_| "Missing live editor token")?;
    let mut input = BufReader::new(std::io::stdin().lock());
    let mut output = std::io::stdout().lock();
    while let Some(request) = read_message(&mut input)? {
        let Some(id) = request.get("id").cloned() else {
            continue;
        };
        let result = match request["method"].as_str().unwrap_or_default() {
            "initialize" => {
                json!({"protocolVersion":"2025-06-18","capabilities":{"tools":{}},"serverInfo":{"name":"omadesign","version":env!("OMADESIGN_VERSION")}})
            }
            "ping" => json!({}),
            "tools/list" => json!({"tools":super::tools::catalog()}),
            "tools/call" => {
                let result = (|| -> Result<Value, String> {
                    let mut stream = UnixStream::connect(&socket).map_err(|e| e.to_string())?;
                    stream
                        .set_read_timeout(Some(Duration::from_secs(65)))
                        .map_err(|e| e.to_string())?;
                    stream
                        .set_write_timeout(Some(Duration::from_secs(10)))
                        .map_err(|e| e.to_string())?;
                    write_message(
                        &mut stream,
                        &json!({"token":token,"name":request["params"]["name"],"arguments":request["params"]["arguments"]}),
                    )?;
                    read_message(&mut BufReader::new(stream))?
                        .ok_or_else(|| "Editor closed the connection".into())
                })();
                result.unwrap_or_else(failure)
            }
            _ => {
                write_message(
                    &mut output,
                    &json!({"jsonrpc":"2.0","id":id,"error":{"code":-32601,"message":"Unknown MCP method"}}),
                )?;
                continue;
            }
        };
        write_message(
            &mut output,
            &json!({"jsonrpc":"2.0","id":id,"result":result}),
        )?;
    }
    Ok(())
}
