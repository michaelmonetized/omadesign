use super::*;
use crate::{
    app::Studio,
    document::{Document, Layer},
    geom::Pt,
    tools::Persona,
};
use serde_json::{Value, json};
use std::path::PathBuf;
struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let p = std::env::temp_dir().join(format!(
            "omadesign-all-modes-{}-{}",
            std::process::id(),
            crate::document::next_id()
        ));
        std::fs::create_dir(&p).unwrap();
        let bytes: Vec<u8> = (0..32 * 24)
            .flat_map(|i| {
                [
                    if i % 32 < 16 { 240 } else { 40 },
                    (i / 32 * 8) as u8,
                    90,
                    255,
                ]
            })
            .collect();
        image::save_buffer(
            p.join("Disk photo with spaces.png"),
            &bytes,
            32,
            24,
            image::ColorType::Rgba8,
        )
        .unwrap();
        Self(p)
    }
    fn photo(&self) -> PathBuf {
        self.0.join("Disk photo with spaces.png")
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn studio() -> Studio {
    let mut s = Studio::new();
    s.doc = Document::new("All modes", 640., 480., 96.);
    s.doc.layers = vec![Layer::vector("Existing artwork")];
    s.active_layer = Some(0);
    s.show_welcome = false;
    s
}
fn call(s: &mut Studio, name: &str, mut args: Value) -> Value {
    if tools::mutates(name) {
        args["revision"] = json!(s.canvas_gen);
    }
    let result = tools::execute(s, name, &args, true).unwrap_or_else(|e| panic!("{name}: {e}"));
    if result["content"][0]["type"] == "image" {
        return result;
    }
    serde_json::from_str(result["content"][0]["text"].as_str().unwrap()).unwrap()
}
fn import(s: &mut Studio, f: &Fixture) -> usize {
    let result = call(
        s,
        "import_file",
        json!({"path":f.photo(),"x":10,"y":20,"width":160}),
    );
    result["selection"][0][0].as_u64().unwrap() as usize
}
#[test]
fn disk_photos_import_in_every_workspace_with_source_pixels_undo_and_persistence() {
    let f = Fixture::new();
    let original = std::fs::read(f.photo()).unwrap();
    for mode in [
        Persona::Design,
        Persona::Pixel,
        Persona::Photo,
        Persona::Layout,
        Persona::Motion,
    ] {
        let mut s = studio();
        s.persona = mode;
        let before = crate::project::encode(&s.doc).unwrap();
        let li = import(&mut s, &f);
        assert_eq!(s.doc.layers.len(), 2);
        assert!(s.doc.layers[li].kind.pixels().is_some());
        assert_eq!(
            s.doc.layers[li].kind.raster_xform().unwrap(),
            (Pt::new(10., 20.), Pt::new(160., 120.), 0.)
        );
        let encoded = crate::project::encode(&s.doc).unwrap();
        let reopened = crate::project::decode(&encoded).unwrap();
        assert_eq!(
            reopened.layers[li].kind.pixels().unwrap().data,
            s.doc.layers[li].kind.pixels().unwrap().data
        );
        call(
            &mut s,
            "editor_action",
            json!({"action":"undo","history":"canvas"}),
        );
        assert_eq!(crate::project::encode(&s.doc).unwrap(), before);
        call(
            &mut s,
            "editor_action",
            json!({"action":"redo","history":"canvas"}),
        );
        assert_eq!(crate::project::encode(&s.doc).unwrap(), encoded);
    }
    assert_eq!(std::fs::read(f.photo()).unwrap(), original);
}
#[test]
fn raster_selection_transform_mask_paint_and_motion_use_native_history() {
    let f = Fixture::new();
    let mut s = studio();
    let li = import(&mut s, &f);
    let source = s.doc.layers[li].kind.pixels().unwrap().data.clone();
    call(&mut s, "select_objects", json!({"layer":li,"ids":[0]}));
    call(
        &mut s,
        "transform_raster",
        json!({"layer":li,"x":60,"y":70,"rotation":30,"width":200,"height":150,"opacity":0.8}),
    );
    call(
        &mut s,
        "set_pixel_selection",
        json!({"layer":li,"kind":"rect","x":0,"y":0,"width":16,"height":24}),
    );
    call(
        &mut s,
        "set_mask",
        json!({"layer":li,"operation":"selection"}),
    );
    let mask = s.doc.layers[li].mask.as_ref().unwrap();
    assert_eq!(mask.data[0], 255);
    assert_eq!(mask.data[(20 * 4) as usize], 0);
    assert_eq!(s.doc.layers[li].kind.pixels().unwrap().data, source);
    call(
        &mut s,
        "paint_stroke",
        json!({"layer":li,"points":[[5,5],[8,10]],"color":"#00FF00","size":5}),
    );
    let painted = s.doc.layers[li].kind.pixels().unwrap().data.clone();
    assert_ne!(painted, source);
    for y in 0..24 {
        assert_eq!(
            &painted[(y * 32 + 16) * 4..(y * 32 + 32) * 4],
            &source[(y * 32 + 16) * 4..(y * 32 + 32) * 4]
        );
    }
    call(
        &mut s,
        "editor_action",
        json!({"action":"undo","history":"canvas"}),
    );
    assert_eq!(s.doc.layers[li].kind.pixels().unwrap().data, source);
    call(
        &mut s,
        "set_keyframes",
        json!({"layer":li,"id":0,"property":"Rotation","keys":[{"time":0,"value":0},{"time":1,"value":90}]}),
    );
    let id = s.doc.layers[li].id;
    assert!((s.doc.motion.pose(id, 1.).rotation - std::f32::consts::FRAC_PI_2).abs() < 0.001);
    call(
        &mut s,
        "apply_motion_preset",
        json!({"preset":"Fade in","targets":[{"layer":li,"id":0}],"time":0}),
    );
    assert!(s.doc.motion.tracks.iter().all(|t| t.shape == id));
    let encoded = crate::project::encode(&s.doc).unwrap();
    let reopened = crate::project::decode(&encoded).unwrap();
    assert_eq!(reopened.motion, s.doc.motion);
    let before = crate::compositor::render_export_at(&s.doc, 1., 0., true).unwrap();
    let after = crate::compositor::render_export_at(&s.doc, 1., 1., true).unwrap();
    assert_ne!(before.data(), after.data());
}
#[test]
fn layout_import_and_image_fill_remain_embedded_editable_and_bounded_in_context() {
    let f = Fixture::new();
    let mut s = studio();
    let frame = call(
        &mut s,
        "add_shape",
        json!({"layer":0,"shape":{"kind":"frame","x":20,"y":20,"width":400,"height":300}}),
    )["id"]
        .as_u64()
        .unwrap();
    call(
        &mut s,
        "import_file",
        json!({"path":f.photo(),"destination":"frame","layer":0,"id":frame,"x":40,"y":40,"width":160}),
    );
    let child = s.selection[0].1;
    assert_eq!(
        s.doc.find_shape(0, child).unwrap().layout.parent,
        Some(frame)
    );
    call(
        &mut s,
        "set_image_fill",
        json!({"path":f.photo(),"layer":0,"id":child,"fit":"contain","focal_x":0.25}),
    );
    assert!(s.doc.find_shape(0, child).unwrap().layout.image.is_some());
    call(
        &mut s,
        "set_layout",
        json!({"layer":0,"id":frame,"changes":{"stack":{"direction":"Horizontal","gap":12,"padding":[10,10,10,10],"align":"Center","flow":"Stack","justify":"Start","cross_gap":0,"columns":2}}}),
    );
    assert_eq!(
        s.doc
            .find_shape(0, frame)
            .unwrap()
            .layout
            .stack
            .as_ref()
            .unwrap()
            .gap,
        12.
    );
    let objects = call(&mut s, "get_objects", json!({"layer":0}));
    assert!(!objects.to_string().contains("iVBOR"));
    let reopened = crate::project::decode(&crate::project::encode(&s.doc).unwrap()).unwrap();
    assert!(
        reopened
            .find_shape(0, child)
            .unwrap()
            .layout
            .image
            .is_some()
    );
}
#[test]
fn photo_development_is_nondestructive_revision_checked_and_undoable() {
    let f = Fixture::new();
    let original = std::fs::read(f.photo()).unwrap();
    let mut s = studio();
    call(
        &mut s,
        "import_file",
        json!({"path":f.photo(),"destination":"photo"}),
    );
    assert_eq!(s.photo.images.len(), 1);
    let before = s.photo.images[0].develop.clone();
    let photo_revision = s.photo.edit_revision;
    call(
        &mut s,
        "develop_photo",
        json!({"photo":0,"photo_revision":photo_revision,"changes":{"exposure":1.5,"crop":[0.1,0.1,0.9,0.9],"rotate":90}}),
    );
    assert_eq!(s.photo.images[0].develop.exposure, 1.5);
    let revision = s.canvas_gen;
    assert!(tools::execute(&mut s,"develop_photo",&json!({"revision":revision,"photo":0,"photo_revision":photo_revision,"changes":{"exposure":0}}),true).is_err());
    let preview = call(&mut s, "get_canvas_snapshot", json!({"source":"photo"}));
    assert_eq!(preview["content"][0]["mimeType"], "image/png");
    call(
        &mut s,
        "editor_action",
        json!({"action":"undo","history":"photo"}),
    );
    assert_eq!(s.photo.images[0].develop, before);
    call(
        &mut s,
        "editor_action",
        json!({"action":"redo","history":"photo"}),
    );
    assert_eq!(s.photo.images[0].develop.exposure, 1.5);
    let rev = s.photo.edit_revision;
    call(
        &mut s,
        "save_document",
        json!({"path":f.0.join("developed.png"),"source":"photo","photo_revision":rev}),
    );
    assert!(f.0.join("developed.png").exists());
    assert_eq!(std::fs::read(f.photo()).unwrap(), original);
}
#[test]
fn file_preview_discovery_and_explicit_export_handle_spaces_and_errors() {
    let f = Fixture::new();
    let mut s = studio();
    let listing = call(&mut s, "list_files", json!({"path":f.0,"query":"photo"}));
    assert_eq!(listing["entries"].as_array().unwrap().len(), 1);
    let preview = call(&mut s, "read_file", json!({"path":f.photo()}));
    assert_eq!(preview["content"][0]["type"], "image");
    import(&mut s, &f);
    let output = f.0.join("editable.oma");
    call(&mut s, "save_document", json!({"path":output}));
    let before = std::fs::read(&output).unwrap();
    let revision = s.canvas_gen;
    assert!(
        tools::execute(
            &mut s,
            "save_document",
            &json!({"revision":revision,"path":output}),
            true
        )
        .unwrap_err()
        .contains("exists")
    );
    assert_eq!(std::fs::read(output).unwrap(), before);
    let revision = s.canvas_gen;
    assert!(
        tools::execute(
            &mut s,
            "import_file",
            &json!({"revision":revision,"path":f.0.join("missing.jpg")}),
            true
        )
        .is_err()
    );
    assert_eq!(s.canvas_gen, revision);
}
#[test]
fn decoded_import_cannot_cross_revision_readonly_or_manual_gesture_gates() {
    let f = Fixture::new();
    let mut s = studio();
    let args = json!({"revision":s.canvas_gen,"path":f.photo()});
    let prepared = tools::files::prepare("import_file", &args);
    s.mark();
    let before = crate::project::encode(&s.doc).unwrap();
    assert!(
        tools::execute_prepared(&mut s, "import_file", &args, true, prepared, &f.0)
            .unwrap_err()
            .contains("Canvas changed")
    );
    let args = json!({"revision":s.canvas_gen,"path":f.photo()});
    assert!(tools::execute(&mut s, "import_file", &args, false).is_err());
    s.free_transform = Some(vec![]);
    assert!(tools::execute(&mut s, "import_file", &args, true).is_err());
    assert_eq!(crate::project::encode(&s.doc).unwrap(), before);
}
#[test]
fn acp_filesystem_delegation_enforces_session_and_edit_permissions() {
    let f = Fixture::new();
    let path = f.0.join("context.txt");
    std::fs::write(&path, "one\ntwo\nthree\n").unwrap();
    let args = json!({"sessionId":"session","path":path,"line":2,"limit":1});
    let read =
        runtime::filesystem_request("fs/read_text_file", &args, Some("session"), true, false)
            .unwrap();
    assert_eq!(read["content"], "two\n");
    assert!(
        runtime::filesystem_request("fs/read_text_file", &args, Some("other"), true, true).is_err()
    );
    assert!(
        runtime::filesystem_request("fs/read_text_file", &args, Some("session"), false, true)
            .is_err()
    );
    let args = json!({"sessionId":"session","path":path,"content":"updated\n"});
    assert!(
        runtime::filesystem_request("fs/write_text_file", &args, Some("session"), true, false)
            .is_err()
    );
    runtime::filesystem_request("fs/write_text_file", &args, Some("session"), true, true).unwrap();
    assert_eq!(std::fs::read_to_string(path).unwrap(), "updated\n");
}
#[test]
fn catalog_has_consistent_revision_and_mutation_gates() {
    let mut names = std::collections::HashSet::new();
    for tool in tools::catalog() {
        let name = tool["name"].as_str().unwrap();
        assert!(names.insert(name.to_owned()));
        assert_eq!(
            tools::mutates(name),
            tool["inputSchema"]["properties"].get("revision").is_some(),
            "{name}"
        );
        assert_eq!(tool["annotations"]["readOnlyHint"], !tools::mutates(name));
    }
}

#[test]
fn advanced_properties_canvas_and_group_edits_validate_and_undo_atomically() {
    let mut s = studio();
    let before = crate::project::encode(&s.doc).unwrap();
    call(
        &mut s,
        "configure_canvas",
        json!({"name":"Resized","width":800,"height":600,"transparent":true}),
    );
    assert_eq!(s.doc.artboards[0].size, Pt::new(800., 600.));
    call(&mut s, "editor_action", json!({"action":"undo"}));
    assert_eq!(crate::project::encode(&s.doc).unwrap(), before);
    let id = call(
        &mut s,
        "add_shape",
        json!({"layer":0,"shape":{"kind":"rect","width":100,"height":80}}),
    )["id"]
        .as_u64()
        .unwrap();
    call(
        &mut s,
        "patch_object",
        json!({"layer":0,"id":id,"changes":{"fill_opacity":0.4,"corners":[1,2,3,4]}}),
    );
    assert_eq!(s.doc.find_shape(0, id).unwrap().fill_opacity, 0.4);
    call(
        &mut s,
        "patch_object",
        json!({"layer":0,"id":id,"changes":{"locked":true}}),
    );
    let args = json!({"revision":s.canvas_gen,"layer":0,"id":id,"changes":{"fill_opacity":0.8}});
    assert!(tools::execute(&mut s, "patch_object", &args, true).is_err());
    call(
        &mut s,
        "patch_object",
        json!({"layer":0,"id":id,"changes":{"locked":false}}),
    );
    call(
        &mut s,
        "set_filter_stack",
        json!({"layer":0,"id":id,"effects":[{"name":"Gaussian blur","parameters":{"std":2}}]}),
    );
    assert_eq!(s.doc.find_shape(0, id).unwrap().filters.items.len(), 1);
    let group = call(
        &mut s,
        "create_layer",
        json!({"name":"Group","kind":"group"}),
    )["id"]
        .as_u64()
        .unwrap();
    call(&mut s, "update_layer", json!({"layer":0,"parent":group}));
    assert_eq!(s.doc.layers[0].parent, Some(group));
    let before = crate::project::encode(&s.doc).unwrap();
    let args = json!({"revision":s.canvas_gen,"layer":1,"parent":group});
    assert!(tools::execute(&mut s, "update_layer", &args, true).is_err());
    assert_eq!(crate::project::encode(&s.doc).unwrap(), before);
}

#[test]
fn acp_filesystem_capabilities_work_over_the_actual_jsonrpc_connection() {
    use std::time::{Duration, Instant};
    let f = Fixture::new();
    let script = f.0.join("peer.py");
    std::fs::write(f.0.join("read.txt"), "first\nsecond\n").unwrap();
    std::fs::write(&script,r#"import sys,json,os
cwd=None
for line in sys.stdin:
 m=json.loads(line);method=m.get('method');p=m.get('params',{})
 def out(v):print(json.dumps({'jsonrpc':'2.0',**v}),flush=True)
 if method=='initialize':
  assert p['clientCapabilities']['fs']=={'readTextFile':True,'writeTextFile':True}
  out({'id':m['id'],'result':{'protocolVersion':1,'agentCapabilities':{}}})
 elif method=='session/new':
  cwd=p['cwd'];out({'id':m['id'],'result':{'sessionId':'files'}})
 elif method=='session/prompt':
  prompt=m['id'];out({'id':100,'method':'fs/read_text_file','params':{'sessionId':'files','path':cwd+'/read.txt','line':2,'limit':1}})
 elif m.get('id')==100:
  assert m['result']['content']=='second\n'
  out({'id':101,'method':'fs/write_text_file','params':{'sessionId':'files','path':cwd+'/written.txt','content':'round trip\n'}})
 elif m.get('id')==101:
  assert m.get('result')=={}
  out({'id':prompt,'result':{'stopReason':'end_turn'}})
"#).unwrap();
    let c = runtime::Connection::start(
        config::Profile {
            name: "ACP file peer".into(),
            command: "python3".into(),
            args: vec![script.display().to_string()],
        },
        f.0.clone(),
        None,
        std::env::current_exe().unwrap(),
        eframe::egui::Context::default(),
    )
    .unwrap();
    c.bridge
        .writable
        .store(true, std::sync::atomic::Ordering::Release);
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut complete = false;
    while Instant::now() < deadline {
        match c.events.recv_timeout(Duration::from_secs(2)).unwrap() {
            runtime::Event::Session { .. } => c
                .send(runtime::Command::Prompt(vec![
                    json!({"type":"text","text":"Read and write test files"}),
                ]))
                .unwrap(),
            runtime::Event::Complete(reason) => {
                assert_eq!(reason, "end_turn");
                complete = true;
                break;
            }
            runtime::Event::Error(e) => panic!("{e}"),
            _ => (),
        }
    }
    assert!(complete);
    assert_eq!(
        std::fs::read_to_string(f.0.join("written.txt")).unwrap(),
        "round trip\n"
    );
}

#[test]
fn layer_duplication_copies_ordinary_and_empty_layers_atomically() {
    for layer in [Layer::vector("Empty vector"), Layer::raster("Pixels", 3, 2), Layer::group("Empty group")] {
        let mut s = studio();
        s.doc.layers = vec![layer];
        let before = crate::project::encode(&s.doc).unwrap();
        call(&mut s, "editor_action", json!({"action":"duplicate_layer", "layer":0}));
        assert_eq!(s.doc.layers.len(), 2);
        assert_ne!(s.doc.layers[0].id, s.doc.layers[1].id);
        s.undo();
        assert_eq!(crate::project::encode(&s.doc).unwrap(), before);
    }
}

#[test]
fn incompatible_canvas_import_dimensions_fail_without_changing_the_document() {
    let f = Fixture::new();
    let mut s = studio();
    let before = crate::project::encode(&s.doc).unwrap();
    let args = json!({"revision":s.canvas_gen,"path":f.photo(),"x":10,"y":20,"width":160,"height":100});
    let error = tools::execute(&mut s, "import_file", &args, true).unwrap_err();
    assert!(error.contains("aspect ratio"), "{error}");
    assert_eq!(crate::project::encode(&s.doc).unwrap(), before);
}
