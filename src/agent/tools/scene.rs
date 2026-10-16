//! Revision-checked editable scene controls.
use super::*;
use crate::scene3d::{self, Scene};
use serde::{Serialize, de::DeserializeOwned};

fn patch<T: Serialize + DeserializeOwned>(
    target: &mut T,
    args: &Value,
    allowed: &[&str],
) -> Result<(), String> {
    let fields = args
        .as_object()
        .ok_or("ScenePatch: provide an object patch")?;
    let mut value = serde_json::to_value(&*target).map_err(|e| e.to_string())?;
    for (key, v) in fields {
        if !allowed.contains(&key.as_str()) {
            return Err(format!("ScenePatch: unknown field {key}"));
        }
        value[key] = v.clone();
    }
    *target = serde_json::from_value(value).map_err(|e| format!("ScenePatch: {e}"))?;
    Ok(())
}

pub fn execute(studio: &mut Studio, name: &str, args: &Value) -> Result<Value, String> {
    if name == "get_scene3d" {
        return Ok(
            json!({"revision":studio.canvas_gen,"scene":studio.doc.motion.scene3d.as_ref().map(|s|json!({"camera":s.camera,"imported_camera":s.imported_camera,"lights":s.lights,"ambient":s.ambient,"imported_lights":s.imported_lights,"objects":s.objects.iter().map(|o|json!({"id":o.id,"name":o.name,"translation":o.translation,"rotation":o.rotation,"scale":o.scale,"animation":o.animation,"time_offset":o.time_offset,"time_scale":o.time_scale,"looped":o.looped,"opacity":o.opacity,"visible":o.visible,"vertices":o.model.vertex_count(),"triangles":o.model.triangle_count(),"nodes":o.model.nodes,"materials":o.model.materials,"cameras":o.model.cameras,"model_lights":o.model.lights,"clips":o.model.animations.iter().enumerate().map(|(i,a)|json!({"index":i,"name":a.name,"duration":a.duration()})).collect::<Vec<_>>()})).collect::<Vec<_>>()}))}),
        );
    }
    let mut motion = studio.doc.motion.clone();
    let scene = motion.scene3d.get_or_insert_with(Scene::default);
    let operation = text(args, "operation", "")?;
    let mut created = None;
    match operation.as_str() {
        "import"=> {let path=files::path(args)?;let model=scene3d::import_model(&path)?;created=Some(scene.add(model,path.file_stem().unwrap_or_default().to_string_lossy().into())?);},
        "primitive"=> {let kind=scene3d::model::prim::PrimitiveKind::parse(&text(args,"kind","Cube")?).ok_or("Unknown primitive")?;let params=scene3d::model::prim::PrimParams {size:[2.;3],radius:1.,tube:0.25,height:2.,segments:24,rings:16};created=Some(scene.add(scene3d::model::prim::model(kind,&params,scene3d::model::Material::plain([0.65,0.3,0.12,1.],0.,0.6)),kind.label().into())?);},
        "camera"=>patch(&mut scene.camera,&args["patch"],&["position","target","up","projection","near","far"] )?,
        "lighting"=> {if let Some(v)=args.get("lights") {scene.lights=serde_json::from_value(v.clone()).map_err(|e|e.to_string())?;}if let Some(v)=args.get("ambient") {scene.ambient=serde_json::from_value(v.clone()).map_err(|e|e.to_string())?;}if let Some(v)=args.get("imported_camera") {scene.imported_camera=serde_json::from_value(v.clone()).map_err(|e|e.to_string())?;}scene.imported_lights=boolean(args,"imported_lights",scene.imported_lights)?;},
        "object"|"node"|"material"|"remove"=> {
            let id=args["object"].as_u64().ok_or("SceneObject: object id required")?;
            if operation=="remove" {if !scene.objects.iter().any(|o|o.id==id) {return Err("SceneObject: missing object".into());}scene.objects.retain(|o|o.id!=id);if scene.imported_camera.is_some_and(|(o,_)|o==id) {scene.imported_camera=None;}}
            else {let o=scene.objects.iter_mut().find(|o|o.id==id).ok_or("SceneObject: missing object")?;
                match operation.as_str() {
                    "object"=>patch(o,&args["patch"],&["name","translation","rotation","scale","animation","time_offset","time_scale","looped","opacity","visible"] )?,
                    "node"=> {let n=index(args,"node")?;let model=std::sync::Arc::make_mut(&mut o.model);let node=model.nodes.get_mut(n).ok_or("SceneNode: missing node")?;if node.matrix.is_some() && args["patch"].as_object().is_some_and(|p|p.keys().any(|k|k!="name")) {return Err("SceneNodeTransform: this node uses a matrix; edit its model object transform to preserve it".into());}patch(node,&args["patch"],&["name","translation","rotation","scale"])?;},
                    "material"=> {let i=index(args,"material")?;let model=std::sync::Arc::make_mut(&mut o.model);let material=model.materials.get_mut(i).ok_or("SceneMaterial: missing material")?;patch(material,&args["patch"],&["name","base_color","base_color_tex","metallic","roughness","metallic_roughness_tex","normal_tex","normal_scale","occlusion_tex","occlusion_strength","emissive","emissive_tex","alpha_mode","alpha_cutoff","double_sided","unlit"])?;},_=>{}
                }
            }
        },
        _=>return Err("SceneOperation: use import, primitive, camera, lighting, object, node, material or remove".into()),
    }
    scene.validate()?;
    motion.duration = scene
        .objects
        .iter()
        .flat_map(|o| &o.model.animations)
        .map(|a| a.duration() as f32)
        .fold(motion.duration, f32::max);
    let mut candidate = studio.doc.clone();
    candidate.motion = motion.clone();
    crate::motion::compositions::validate(&candidate)?;
    studio.commit(Cmd::Batch(vec![Cmd::SetMotion {
        before: studio.doc.motion.clone(),
        after: motion,
    }]));
    Ok(json!({"revision":studio.canvas_gen,"created":created}))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn scene_tools_edit_render_save_reopen_and_undo_atomically() {
        let mut s = Studio::new();
        s.doc = crate::document::Document::new("Scene", 64., 64., 96.);
        s.doc.layers.clear();
        s.doc.artboards.clear();
        s.doc.artboardless = true;
        let call = |s: &mut Studio, name: &str, mut args: Value| {
            args["revision"] = json!(s.canvas_gen);
            super::super::execute(s, name, &args, true)
        };
        call(
            &mut s,
            "set_scene3d",
            json!({"operation":"primitive","kind":"Cube"}),
        )
        .unwrap();
        let id = s.doc.motion.scene3d.as_ref().unwrap().objects[0].id;
        let a = crate::anim_export::render_frame(&s.doc, 0., 1., true).unwrap();
        call(
            &mut s,
            "set_scene3d",
            json!({"operation":"object","object":id,"patch":{"rotation":[10,40,20]}}),
        )
        .unwrap();
        let b = crate::anim_export::render_frame(&s.doc, 0., 1., true).unwrap();
        assert_ne!(a, b);
        let before = crate::project::encode(&s.doc).unwrap();
        let revision = s.canvas_gen;
        assert!(
            call(
                &mut s,
                "set_scene3d",
                json!({"operation":"object","object":id,"patch":{"scale":[0,0,0]}})
            )
            .is_err()
        );
        assert_eq!(s.canvas_gen, revision);
        assert_eq!(crate::project::encode(&s.doc).unwrap(), before);
        let reopened = crate::project::decode(&before).unwrap();
        assert_eq!(s.doc.motion, reopened.motion);
        s.undo();
        assert_eq!(
            crate::anim_export::render_frame(&s.doc, 0., 1., true).unwrap(),
            a
        );
    }
}
