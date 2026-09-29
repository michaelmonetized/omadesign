use super::*;
use crate::{
    app::Studio,
    document::{Document, Layer},
    geom::Geom,
};
use serde_json::{Value, json};

fn studio() -> Studio {
    let mut s = Studio::new();
    s.doc = Document::new("Native agent tools", 640.0, 480.0, 96.0);
    s.doc.layers = vec![Layer::vector("Artwork")];
    s.active_layer = Some(0);
    s.show_welcome = false;
    s
}
fn call(studio: &mut Studio, name: &str, mut args: Value) -> Value {
    if tools::mutates(name) {
        args["revision"] = json!(studio.canvas_gen);
    }
    let result = tools::execute(studio, name, &args, true).unwrap();
    serde_json::from_str(result["content"][0]["text"].as_str().unwrap()).unwrap()
}

#[test]
fn live_tools_create_editable_text_gradients_and_frames_and_undo_every_step() {
    let mut s = studio();
    let original = crate::project::encode(&s.doc).unwrap();
    let frame=call(&mut s,"add_shape",json!({"layer":0,"shape":{"kind":"frame","name":"Card","x":30,"y":30,"width":400,"height":300,"gradient":["#102820","#386850"],"radius":16}}))["id"].as_u64().unwrap();
    let text=call(&mut s,"add_shape",json!({"layer":0,"shape":{"kind":"text","name":"Heading","text":"FERN & FORM","x":60,"y":70,"font_size":42,"fill":"#FFF9E6","parent":frame}}))["id"].as_u64().unwrap();
    assert!(
        matches!(&s.doc.find_shape(0,text).unwrap().geom,Geom::Text(run) if run.content=="FERN & FORM"&&!run.contours.is_empty())
    );
    assert_eq!(
        s.doc.find_shape(0, text).unwrap().layout.parent,
        Some(frame)
    );
    call(
        &mut s,
        "update_shape",
        json!({"layer":0,"id":text,"changes":{"text":"GROW SLOWLY","fill":"#A8D5BA"}}),
    );
    assert!(
        matches!(&s.doc.find_shape(0,text).unwrap().geom,Geom::Text(run) if run.content=="GROW SLOWLY")
    );
    call(
        &mut s,
        "set_effects",
        json!({"layer":0,"id":frame,"shadow":{"x":3,"y":5,"blur":8,"color":"#00000040"}}),
    );
    let encoded = crate::project::encode(&s.doc).unwrap();
    let reopened = crate::project::decode(&encoded).unwrap();
    assert_eq!(reopened.find_shape(0, text), s.doc.find_shape(0, text));
    let snapshot = tools::execute(&mut s, "get_canvas_snapshot", &json!({}), false).unwrap();
    assert_eq!(snapshot["content"][0]["mimeType"], "image/png");
    assert_eq!(s.history.len(), 4);
    for _ in 0..4 {
        s.undo();
    }
    assert_eq!(crate::project::encode(&s.doc).unwrap(), original);
    for _ in 0..4 {
        s.redo();
    }
    assert_eq!(crate::project::encode(&s.doc).unwrap(), encoded);
}

#[test]
fn tools_reject_stale_revisions_readonly_locked_parents_and_invalid_edits_atomically() {
    let mut s = studio();
    let revision = s.canvas_gen;
    let frame = call(
        &mut s,
        "add_shape",
        json!({"layer":0,"shape":{"kind":"frame"}}),
    )["id"]
        .as_u64()
        .unwrap();
    let child = call(
        &mut s,
        "add_shape",
        json!({"layer":0,"shape":{"parent":frame}}),
    )["id"]
        .as_u64()
        .unwrap();
    let before = crate::project::encode(&s.doc).unwrap();
    assert!(
        tools::execute(
            &mut s,
            "add_shape",
            &json!({"revision":revision,"layer":0,"shape":{}}),
            true
        )
        .unwrap_err()
        .contains("Canvas changed")
    );
    let args = json!({"revision":s.canvas_gen,"layer":0,"id":child,"changes":{"fill":"#ff0000"}});
    assert!(tools::execute(&mut s, "update_shape", &args, false).is_err());
    let args = json!({"revision":s.canvas_gen,"layer":0,"id":frame,"changes":{"parent":frame}});
    assert!(tools::execute(&mut s, "update_shape", &args, true).is_err());
    let args = json!({"revision":s.canvas_gen,"layer":0,"ids":[child,99999999]});
    assert!(tools::execute(&mut s, "remove_shapes", &args, true).is_err());
    assert_eq!(crate::project::encode(&s.doc).unwrap(), before);
    s.doc.find_shape_mut(0, frame).unwrap().locked = true;
    let args = json!({"revision":s.canvas_gen,"layer":0,"id":child,"changes":{"x":20}});
    assert!(tools::execute(&mut s, "update_shape", &args, true).is_err());
}

#[test]
fn translating_existing_paths_preserves_native_geometry() {
    let mut s = studio();
    let id = call(
        &mut s,
        "add_shape",
        json!({"layer":0,"shape":{"kind":"path","points":[[10,10],[50,20],[20,60]],"closed":true}}),
    )["id"]
        .as_u64()
        .unwrap();
    call(
        &mut s,
        "update_shape",
        json!({"layer":0,"id":id,"changes":{"x":110,"y":210}}),
    );
    let shape = s.doc.find_shape(0, id).unwrap();
    assert!(matches!(shape.geom, Geom::Path { .. }));
    assert_eq!(shape.geom.bbox().min, crate::geom::Pt::new(110.0, 210.0));
}

#[test]
fn removing_frames_is_atomic_and_undo_restores_stacking_and_animation() {
    let mut s = studio();
    let frame = call(
        &mut s,
        "add_shape",
        json!({"layer":0,"shape":{"kind":"frame"}}),
    )["id"]
        .as_u64()
        .unwrap();
    let child = call(
        &mut s,
        "add_shape",
        json!({"layer":0,"shape":{"parent":frame}}),
    )["id"]
        .as_u64()
        .unwrap();
    call(
        &mut s,
        "add_shape",
        json!({"layer":0,"shape":{"name":"Keep above frame"}}),
    );
    s.doc.motion.set_key(
        child,
        crate::motion::Prop::X,
        1.0,
        40.0,
        crate::motion::Ease::Linear,
    );
    s.doc.find_shape_mut(0, child).unwrap().locked = true;
    let before = crate::project::encode(&s.doc).unwrap();
    let args = json!({"revision":s.canvas_gen,"layer":0,"ids":[frame]});
    assert!(tools::execute(&mut s, "remove_shapes", &args, true).is_err());
    assert_eq!(crate::project::encode(&s.doc).unwrap(), before);
    s.doc.find_shape_mut(0, child).unwrap().locked = false;
    let before = crate::project::encode(&s.doc).unwrap();
    call(&mut s, "remove_shapes", json!({"layer":0,"ids":[frame]}));
    assert_eq!(s.doc.layers[0].kind.shapes().unwrap().len(), 1);
    assert!(!s.doc.motion.has_shape(child));
    s.undo();
    assert_eq!(crate::project::encode(&s.doc).unwrap(), before);
}

#[test]
fn canvas_context_paginates_and_never_sends_pixel_mask_buffers() {
    let mut s = studio();
    for _ in 0..52 {
        call(&mut s, "add_shape", json!({"layer":0,"shape":{}}));
    }
    let id = s.doc.layers[0].kind.shapes().unwrap()[0].id;
    s.doc.find_shape_mut(0, id).unwrap().mask = Some(crate::document::Pixels::new(512, 512));
    let document = call(&mut s, "get_document", json!({}));
    assert_eq!(document["layers"][0]["objects"], 52);
    assert_eq!(document["layers"][0]["items"].as_array().unwrap().len(), 20);
    let objects = call(&mut s, "get_objects", json!({"layer":0}));
    assert_eq!(objects["next_offset"], 50);
    assert_eq!(
        objects["objects"][0]["mask"],
        json!({"width":512,"height":512})
    );
    assert!(serde_json::to_vec(&objects).unwrap().len() < 100_000);
    let remaining = call(&mut s, "get_objects", json!({"layer":0,"offset":50}));
    assert_eq!(remaining["objects"].as_array().unwrap().len(), 2);
    assert!(remaining["next_offset"].is_null());
}

#[test]
fn bridge_rejects_forged_idle_and_cancelled_calls_and_cleans_socket() {
    use std::{io::BufReader, os::unix::net::UnixStream, sync::atomic::Ordering, time::Duration};
    let ctx = eframe::egui::Context::default();
    let b = bridge::Bridge::start(ctx).unwrap();
    let socket = b.socket.clone();
    let invoke = |token: &str| {
        let mut stream = UnixStream::connect(&socket).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        bridge::write_message(
            &mut stream,
            &json!({"token":token,"name":"get_document","arguments":{}}),
        )
        .unwrap();
        bridge::read_message(&mut BufReader::new(stream))
            .unwrap()
            .unwrap()
    };
    assert_eq!(invoke("forged")["isError"], true);
    assert_eq!(invoke(&b.token)["isError"], true);
    b.accepting.store(true, Ordering::Release);
    let token = b.token.clone();
    let target = socket.clone();
    let thread = std::thread::spawn(move || {
        let mut stream = UnixStream::connect(target).unwrap();
        bridge::write_message(
            &mut stream,
            &json!({"token":token,"name":"get_document","arguments":{}}),
        )
        .unwrap();
        bridge::read_message(&mut BufReader::new(stream))
            .unwrap()
            .unwrap()
    });
    let call = b.calls.recv_timeout(Duration::from_secs(2)).unwrap();
    assert_eq!(call.name, "get_document");
    b.accepting.store(false, Ordering::Release);
    assert_eq!(thread.join().unwrap()["isError"], true);
    drop(b);
    for _ in 0..100 {
        if !socket.exists() {
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(!socket.exists());
}

#[test]
fn workspace_cancels_before_applying_queued_tools_to_another_document() {
    use std::sync::atomic::Ordering;
    let mut s = studio();
    let ctx = eframe::egui::Context::default();
    let b = bridge::Bridge::start(ctx.clone()).unwrap();
    let (_tx, events) = std::sync::mpsc::channel();
    let (commands, _rx) = std::sync::mpsc::channel();
    let mut agent = Workspace::default();
    agent.loaded = true;
    agent.owner = Some(s.swap_id.clone());
    agent.busy = true;
    b.accepting.store(true, Ordering::Release);
    agent.connection = Some(runtime::Connection {
        bridge: b,
        events,
        commands,
        worker: None,
    });
    s.swap_id = "another-document".into();
    let before = crate::project::encode(&s.doc).unwrap();
    agent.poll(&mut s, &ctx);
    assert!(agent.connection.is_none());
    assert!(!agent.busy);
    assert_eq!(crate::project::encode(&s.doc).unwrap(), before);
}

#[test]
fn transcripts_and_custom_profiles_round_trip_without_credentials() {
    let directory = std::env::temp_dir().join(format!(
        "omadesign-agent-store-{}",
        crate::project::new_swap_id()
    ));
    let settings = config::Settings {
        profile: config::Profile {
            name: "Local agent".into(),
            command: "/opt/agent runner".into(),
            args: vec!["--acp".into(), "literal $argument".into()],
        },
        directory: directory.clone(),
        live_edits: true,
        ..Default::default()
    };
    let path = directory.join("settings.json");
    config::write(&path, &settings).unwrap();
    let loaded: config::Settings = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(loaded.profile, settings.profile);
    use std::os::unix::fs::PermissionsExt;
    assert_eq!(
        std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o600
    );
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn acp_transport_negotiates_sessions_streams_permissions_and_cancels() {
    use std::time::{Duration, Instant};
    let script = r#"
import json,sys,threading
configured=0
inflight=False
def emit(value):
 print(json.dumps(value),flush=True)
def result(id,value):
 emit({'jsonrpc':'2.0','id':id,'result':value})
prompt=None
for line in sys.stdin:
 m=json.loads(line); method=m.get('method'); p=m.get('params',{})
 if method=='initialize':
  assert p['protocolVersion']==1 and p['clientCapabilities']['terminal']==False
  result(m['id'],{'protocolVersion':1,'agentCapabilities':{'loadSession':True},'authMethods':[]})
 elif method=='session/new':
  assert p['cwd'].startswith('/') and p['mcpServers'][0]['name']=='omadesign'
  assert p['mcpServers'][0]['args']==['--agent-mcp']
  result(m['id'],{'sessionId':'real-wire-fixture'})
 elif method=='session/set_config_option':
  assert not inflight, 'Configuration changes must be serialized'
  inflight=True
  def finish(id=m['id'], value=p['value'], option=p['configId']):
   global configured,inflight
   configured+=1
   inflight=False
   result(id,{'configOptions':[{'id':option,'category':'thought_level','type':'select','currentValue':value,'options':[{'value':value,'name':value}]}]})
  threading.Timer(0.05,finish).start()
 elif method=='session/prompt':
  assert configured==2 and not inflight, 'Prompt raced configuration acknowledgement'
  assert p['sessionId']=='real-wire-fixture'
  prompt=m['id']
  emit({'jsonrpc':'2.0','method':'session/update','params':{'sessionId':'real-wire-fixture','update':{'sessionUpdate':'agent_message_chunk','content':{'type':'text','text':'Building live'}}}})
  emit({'jsonrpc':'2.0','id':'permission-1','method':'session/request_permission','params':{'sessionId':'real-wire-fixture','toolCall':{'toolCallId':'t1','title':'Read project notes'},'options':[{'optionId':'allow','name':'Allow once','kind':'allow_once'}]}})
 elif method=='session/cancel':
  result(prompt,{'stopReason':'cancelled'})
 elif m.get('id')=='permission-1':
  assert m['result']['outcome']['outcome'] in ('selected','cancelled')
  if m['result']['outcome']['outcome']=='selected':
   result(prompt,{'stopReason':'end_turn'})
"#;
    let ctx = eframe::egui::Context::default();
    let c = runtime::Connection::start(
        config::Profile {
            name: "Protocol fixture".into(),
            command: "python3".into(),
            args: vec!["-u".into(), "-c".into(), script.into()],
        },
        std::env::temp_dir(),
        None,
        std::env::current_exe().unwrap(),
        ctx,
    )
    .unwrap();
    let wait = |accept: fn(&runtime::Event) -> bool| {
        let start = Instant::now();
        loop {
            let e = c
                .events
                .recv_timeout(Duration::from_secs(5))
                .expect("ACP event");
            if let runtime::Event::Error(e) = &e {
                panic!("ACP failed: {e}");
            }
            if accept(&e) {
                return e;
            }
            assert!(start.elapsed() < Duration::from_secs(5));
        }
    };
    wait(|e| matches!(e, runtime::Event::Session { .. }));
    c.send(runtime::Command::Config {
        id: "model".into(),
        value: "fixture-model".into(),
    })
    .unwrap();
    c.send(runtime::Command::Config {
        id: "effort".into(),
        value: "high".into(),
    })
    .unwrap();
    c.send(runtime::Command::Prompt("Design".into())).unwrap();
    assert!(
        matches!(wait(|e|matches!(e,runtime::Event::Update(v) if v["sessionUpdate"]=="agent_message_chunk")),runtime::Event::Update(v) if v["content"]["text"]=="Building live")
    );
    let runtime::Event::Permission { id, .. } =
        wait(|e| matches!(e, runtime::Event::Permission { .. }))
    else {
        unreachable!()
    };
    c.send(runtime::Command::Permission {
        id,
        option: Some("allow".into()),
    })
    .unwrap();
    assert!(
        matches!(wait(|e|matches!(e,runtime::Event::Complete(_))),runtime::Event::Complete(reason) if reason=="end_turn")
    );
    c.send(runtime::Command::Prompt("Another design".into()))
        .unwrap();
    wait(|e| matches!(e, runtime::Event::Permission { .. }));
    c.cancel();
    assert!(
        matches!(wait(|e|matches!(e,runtime::Event::Complete(_))),runtime::Event::Complete(reason) if reason=="cancelled")
    );
    assert!(
        !c.bridge
            .accepting
            .load(std::sync::atomic::Ordering::Acquire)
    );
}
