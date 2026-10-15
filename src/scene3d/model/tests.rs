#![allow(clippy::unwrap_used)]
use super::*;
use base64::Engine;
use serde_json::{Value, json};

fn encoded(bytes: &[u8]) -> String {
    base64::engine::general_purpose::STANDARD.encode(bytes)
}
fn png() -> Vec<u8> {
    let mut bytes = std::io::Cursor::new(Vec::new());
    image::RgbaImage::from_pixel(1, 1, image::Rgba([255, 64, 0, 128]))
        .write_to(&mut bytes, image::ImageFormat::Png)
        .unwrap();
    bytes.into_inner()
}
fn view(bin: &mut Vec<u8>, views: &mut Vec<Value>, bytes: &[u8]) -> usize {
    while bin.len() % 4 != 0 {
        bin.push(0);
    }
    let index = views.len();
    views.push(json!({"buffer":0,"byteOffset":bin.len(),"byteLength":bytes.len()}));
    bin.extend(bytes);
    index
}
fn floats(values: &[f32]) -> Vec<u8> {
    values.iter().flat_map(|v| v.to_le_bytes()).collect()
}
fn animated_gltf() -> (Value, Vec<u8>) {
    let mut bin = vec![];
    let mut views = vec![];
    let mut accessors = vec![];
    let mut accessor = |bytes: &[u8], ct: usize, count: usize, kind: &str| {
        let v = view(&mut bin, &mut views, bytes);
        let index = accessors.len();
        accessors.push(json!({"bufferView":v,"componentType":ct,"count":count,"type":kind}));
        index
    };
    let pos = accessor(
        &floats(&[0., 0., 0., 1., 0., 0., 0., 1., 0.]),
        5126,
        3,
        "VEC3",
    );
    let uv = accessor(&floats(&[0., 0., 1., 0., 0., 1.]), 5126, 3, "VEC2");
    let joints = accessor(&[0; 12], 5121, 3, "VEC4");
    let weights = accessor(
        &floats(&[1., 0., 0., 0., 1., 0., 0., 0., 1., 0., 0., 0.]),
        5126,
        3,
        "VEC4",
    );
    let indices = accessor(&[0, 0, 1, 0, 2, 0], 5123, 3, "SCALAR");
    let times = accessor(&floats(&[0., 2.]), 5126, 2, "SCALAR");
    let translations = accessor(&floats(&[0., 0., 0., 2., 0., 0.]), 5126, 2, "VEC3");
    let j = json!({
        "asset":{"version":"2.0"},"buffers":[{"uri":format!("data:application/octet-stream;base64,{}",encoded(&bin)),"byteLength":bin.len()}],
        "bufferViews":views,"accessors":accessors,
        "images":[{"uri":format!("data:image/png;base64,{}",encoded(&png()))}],
        "textures":[{"source":0}],"materials":[{"name":"Orange glass","pbrMetallicRoughness":{"baseColorFactor":[1.,0.5,0.,0.6],"metallicFactor":0.,"roughnessFactor":0.4,"baseColorTexture":{"index":0}},"alphaMode":"BLEND","doubleSided":true}],
        "meshes":[{"name":"Triangle","primitives":[{"attributes":{"POSITION":pos,"TEXCOORD_0":uv,"JOINTS_0":joints,"WEIGHTS_0":weights},"indices":indices,"material":0}]}],
        "nodes":[{"name":"Animated mesh","mesh":0,"skin":0,"camera":0,"children":[1],"extensions":{"KHR_lights_punctual":{"light":0}}},{"name":"Joint"}],
        "skins":[{"joints":[1]}],
        "animations":[{"name":"Slide","samplers":[{"input":times,"output":translations,"interpolation":"LINEAR"}],"channels":[{"sampler":0,"target":{"node":0,"path":"translation"}}]}],
        "cameras":[{"name":"Camera","type":"perspective","perspective":{"yfov":1.0,"znear":0.1,"zfar":100.0}}],
        "extensionsUsed":["KHR_lights_punctual"],"extensionsRequired":["KHR_lights_punctual"],
        "extensions":{"KHR_lights_punctual":{"lights":[{"name":"Key light","type":"point","color":[1.,0.8,0.6],"intensity":5.0,"range":20.0}]}},
        "scenes":[{"nodes":[0]}],"scene":0
    });
    (j, bin)
}
fn reopen(model: &Model) -> Model {
    let bytes = serde_json::to_vec(model).unwrap();
    let restored: Model = serde_json::from_slice(&bytes).unwrap();
    restored.validate().unwrap();
    assert_eq!(model, &restored);
    restored
}

#[test]
fn gltf_import_preserves_textures_skin_cameras_lights_animation_and_reopen() {
    let (j, _) = animated_gltf();
    let model = load("scene.gltf", &serde_json::to_vec(&j).unwrap(), &|_| None).unwrap();
    let model = reopen(&model);
    assert_eq!((model.vertex_count(), model.triangle_count()), (3, 1));
    assert_eq!(model.textures[0].data[0][3], 128. / 255.);
    assert_eq!(model.materials[0].alpha_mode, AlphaMode::Blend);
    assert_eq!(model.materials[0].base_color, [1., 0.5, 0., 0.6]);
    assert_eq!(model.placed(false, Some(0), 1.)[0].world.0[0][3], 1.);
    assert_eq!(model.placed(true, Some(0), 1.)[0].world.0[0][3], 1.);
    let instances = model.instances(Some(0), 1.);
    let p = &instances[0].mesh.primitives[0];
    assert_eq!(
        skin_point(&instances[0], p, 0, p.positions[0]),
        [1., 0., 0.]
    );
    assert_eq!(model.animations[0].duration(), 2.);
}

#[test]
fn binary_glb_matches_gltf_and_checks_container_length() {
    let (mut j, mut bin) = animated_gltf();
    let expected = load("a.gltf", &serde_json::to_vec(&j).unwrap(), &|_| None).unwrap();
    j["buffers"][0].as_object_mut().unwrap().remove("uri");
    let mut json = serde_json::to_vec(&j).unwrap();
    while json.len() % 4 != 0 {
        json.push(b' ');
    }
    while bin.len() % 4 != 0 {
        bin.push(0);
    }
    let total = 12 + 8 + json.len() + 8 + bin.len();
    let mut glb = b"glTF".to_vec();
    glb.extend(2u32.to_le_bytes());
    glb.extend((total as u32).to_le_bytes());
    glb.extend((json.len() as u32).to_le_bytes());
    glb.extend(0x4e4f534au32.to_le_bytes());
    glb.extend(json);
    glb.extend((bin.len() as u32).to_le_bytes());
    glb.extend(0x004e4942u32.to_le_bytes());
    glb.extend(bin);
    assert_eq!(expected, load("a.glb", &glb, &|_| None).unwrap());
    glb.pop();
    assert!(
        load("a.glb", &glb, &|_| None)
            .unwrap_err()
            .to_string()
            .contains("length")
    );
}

#[test]
fn obj_material_relative_texture_alpha_and_reopen() {
    let obj=b"mtllib materials/red.mtl\nv 0 0 0\nv 1 0 0\nv 0 1 0\nvt 0 0\nvt 1 0\nvt 0 1\nusemtl Red\nf 1/1 2/2 3/3\n";
    let texture = png();
    let resolver = |uri: &str| match uri {
        "materials/red.mtl" => Some(b"newmtl Red\nKd 1 0 0\nd 0.5\nmap_Kd red.png\n".to_vec()),
        "materials/red.png" => Some(texture.clone()),
        _ => None,
    };
    let model = reopen(&load("triangle.obj", obj, &resolver).unwrap());
    assert_eq!(model.materials[0].base_color, [1., 0., 0., 0.5]);
    assert_eq!(model.materials[0].alpha_mode, AlphaMode::Blend);
    assert_eq!(model.meshes[0].primitives[0].uvs[0], [0., 1.]);
    assert!(matches!(
        load("triangle.obj", obj, &|_| None),
        Err(ModelError::Missing(_))
    ));
}

#[test]
fn ascii_and_binary_stl_import_rotate_and_reopen() {
    let ascii=b"solid Triangle\nfacet normal 0 0 1\nouter loop\nvertex 0 0 0\nvertex 1 0 0\nvertex 0 1 0\nendloop\nendfacet\nendsolid Triangle\n";
    let model = reopen(&load("triangle.stl", ascii, &|_| None).unwrap());
    assert_eq!(model.meshes[0].primitives[0].positions[2], [0., 0., -1.]);
    let mut binary = vec![0u8; 80];
    binary.extend(1u32.to_le_bytes());
    binary.extend(floats(&[0., 0., 1., 0., 0., 0., 1., 0., 0., 0., 1., 0.]));
    binary.extend(0u16.to_le_bytes());
    let binary = load("triangle.stl", &binary, &|_| None).unwrap();
    assert_eq!(
        binary.meshes[0].primitives[0].positions,
        model.meshes[0].primitives[0].positions
    );
}

const FBX: &str = r#"FBXHeaderExtension: { FBXVersion: 7400 }
Objects: {
 Geometry: 100, "Geometry::Triangle", "Mesh" {
  Vertices: *9 { a: 0,0,0,1,0,0,0,1,0 }
  PolygonVertexIndex: *3 { a: 0,1,-3 }
 }
 Model: 200, "Model::Triangle", "Mesh" {
  Properties70: { P: "Lcl Translation", "Lcl Translation", "", "A",2,0,0 }
 }
 Material: 300, "Material::Glass", "" {
  ShadingModel: "lambert"
  Properties70: {
   P: "DiffuseColor", "Color", "", "A",1,0,0
   P: "Opacity", "double", "Number", "",0.5
  }
 }
}
Connections: { C: "OO",200,0 C: "OO",100,200 C: "OO",300,200 }
"#;

#[test]
fn fbx_mesh_material_hierarchy_and_reopen() {
    let model = reopen(&load("triangle.fbx", FBX.as_bytes(), &|_| None).unwrap());
    assert_eq!(model.triangle_count(), 1);
    assert_eq!(model.materials[0].base_color, [1., 0., 0., 0.5]);
    assert_eq!(model.instances(None, 0.)[0].world.0[0][3], 2.);
    let unsupported = FBX.replace(
        "Objects: {",
        "Objects: { AnimationStack: 900, \"AnimStack::Take\", \"\" {}",
    );
    assert!(matches!(
        load("animated.fbx", unsupported.as_bytes(), &|_| None),
        Err(ModelError::Unsupported(_))
    ));
}

#[test]
fn interpolation_includes_step_shortest_quaternion_and_cubic_tangents() {
    let mut c = Channel {
        node: 0,
        path: ChannelPath::Translation,
        interp: Interp::Step,
        times: vec![0., 2.],
        values: vec![0., 0., 0., 2., 0., 0.],
    };
    assert_eq!(c.sample(1.).unwrap()[0], 0.);
    c.interp = Interp::Linear;
    assert_eq!(c.sample(1.).unwrap()[0], 1.);
    c.interp = Interp::CubicSpline;
    c.values = vec![
        0., 0., 0., 0., 0., 0., 2., 0., 0., 0., 0., 0., 2., 0., 0., 0., 0., 0.,
    ];
    assert_eq!(c.sample(1.).unwrap()[0], 1.5);
    c.path = ChannelPath::Rotation;
    c.interp = Interp::Linear;
    c.values = vec![0., 0., 0., 1., 0., 0., 0., -1.];
    assert_eq!(c.sample(1.).unwrap(), [0., 0., 0., 1.]);
}

#[test]
fn malformed_resources_geometry_graph_and_animation_fail_without_panics() {
    let (j, _) = animated_gltf();
    for (label, mut bad) in [
        ("accessor", j.clone()),
        ("cycle", j.clone()),
        ("index", j.clone()),
        ("sparse", j.clone()),
        ("morph", j.clone()),
        ("uri", j.clone()),
        ("animation", j.clone()),
    ] {
        match label {
            "accessor" => bad["accessors"][0]["count"] = json!(u64::MAX),
            "cycle" => bad["nodes"][1]["children"] = json!([0]),
            "index" => bad["nodes"][0]["mesh"] = json!(999),
            "sparse" => bad["accessors"][0]["sparse"] = json!({}),
            "morph" => bad["meshes"][0]["primitives"][0]["targets"] = json!([]),
            "uri" => bad["buffers"][0]["uri"] = json!("../outside.bin"),
            "animation" => bad["animations"][0]["channels"][0]["target"]["node"] = json!(999),
            _ => {}
        }
        assert!(
            load("bad.gltf", &serde_json::to_vec(&bad).unwrap(), &|_| Some(
                vec![]
            ))
            .is_err(),
            "{label}"
        );
    }
    let mut model = prim::model(
        prim::PrimitiveKind::Cube,
        &prim::PrimParams::default(),
        Material::default(),
    );
    model.meshes[0].primitives[0].indices[0] = u32::MAX;
    assert!(model.validate().is_err());
    model.meshes[0].primitives[0].compute_normals();
    model.meshes[0].primitives[0].flat_normals();
    model.meshes[0].primitives[0].compute_tangents();
    assert!(load("bad.obj", b"v 0 0 0\nf -9223372036854775808 1 1", &|_| None).is_err());
    assert!(
        load(
            "bad.stl",
            b"solid Bad\nfacet\nvertex NaN 0 0\nendfacet",
            &|_| None
        )
        .is_err()
    );
}

#[test]
fn all_parametric_primitives_are_bounded_finite_and_serializable() {
    for kind in prim::PrimitiveKind::ALL {
        let params = prim::PrimParams {
            segments: 12,
            rings: 6,
            ..Default::default()
        };
        let model = prim::model(
            kind,
            &params,
            Material::plain([0.3, 0.5, 0.8, 0.7], 0.2, 0.4),
        );
        model.validate().unwrap();
        reopen(&model);
        assert!(model.triangle_count() > 0);
    }
    let old: Model = serde_json::from_str("{}").unwrap();
    old.validate().unwrap();
    let old: Node = serde_json::from_str("{}").unwrap();
    assert_eq!(old.scale, [1.; 3]);
    assert_eq!(old.rotation, [0., 0., 0., 1.]);
}

fn binary_fbx(version: u32) -> Vec<u8> {
    use fbx::{FbxNode, Prop};
    use std::io::Write;
    fn prop(p: &Prop, out: &mut Vec<u8>) {
        match p {
            Prop::Int(v) => {
                out.push(b'L');
                out.extend(v.to_le_bytes());
            }
            Prop::Float(v) => {
                out.push(b'D');
                out.extend(v.to_le_bytes());
            }
            Prop::Str(v) => {
                out.push(b'S');
                out.extend((v.len() as u32).to_le_bytes());
                out.extend(v.as_bytes());
            }
            Prop::Bytes(v) => {
                out.push(b'R');
                out.extend((v.len() as u32).to_le_bytes());
                out.extend(v);
            }
            Prop::Ints(v) => array(
                b'l',
                v.len(),
                v.iter().flat_map(|x| x.to_le_bytes()).collect(),
                out,
            ),
            Prop::Floats(v) => array(
                b'd',
                v.len(),
                v.iter().flat_map(|x| x.to_le_bytes()).collect(),
                out,
            ),
        }
    }
    fn array(kind: u8, len: usize, raw: Vec<u8>, out: &mut Vec<u8>) {
        let mut writer =
            flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
        writer.write_all(&raw).unwrap();
        let compressed = writer.finish().unwrap();
        out.push(kind);
        out.extend((len as u32).to_le_bytes());
        out.extend(1u32.to_le_bytes());
        out.extend((compressed.len() as u32).to_le_bytes());
        out.extend(compressed);
    }
    fn record(n: &FbxNode, out: &mut Vec<u8>, wide: bool) {
        let offset = out.len();
        let header = if wide { 25 } else { 13 };
        out.resize(offset + header, 0);
        out.extend(n.name.as_bytes());
        let begin = out.len();
        let count = if n.child("a").is_some() {
            if n.name == "PolygonVertexIndex" {
                prop(&Prop::Ints(n.ints()), out);
            } else {
                prop(&Prop::Floats(n.floats()), out);
            }
            1
        } else {
            for p in &n.props {
                prop(p, out);
            }
            n.props.len()
        };
        let len = out.len() - begin;
        let children: Vec<_> = n.children.iter().filter(|c| c.name != "a").collect();
        if !children.is_empty() {
            for c in children {
                record(c, out, wide);
            }
            out.resize(out.len() + header, 0);
        }
        let end = out.len();
        if wide {
            out[offset..offset + 8].copy_from_slice(&(end as u64).to_le_bytes());
            out[offset + 8..offset + 16].copy_from_slice(&(count as u64).to_le_bytes());
            out[offset + 16..offset + 24].copy_from_slice(&(len as u64).to_le_bytes());
        } else {
            out[offset..offset + 4].copy_from_slice(&(end as u32).to_le_bytes());
            out[offset + 4..offset + 8].copy_from_slice(&(count as u32).to_le_bytes());
            out[offset + 8..offset + 12].copy_from_slice(&(len as u32).to_le_bytes());
        }
        out[offset + header - 1] = n.name.len() as u8;
    }
    let (_, nodes) = fbx::read(FBX.as_bytes()).unwrap();
    let mut out = fbx::BINARY_MAGIC.to_vec();
    out.extend(version.to_le_bytes());
    for n in nodes {
        record(&n, &mut out, version >= 7500);
    }
    out.resize(out.len() + if version >= 7500 { 25 } else { 13 }, 0);
    out
}

#[test]
fn binary_fbx_both_record_sizes_decode_compressed_arrays_and_reopen() {
    let expected = load("triangle.fbx", FBX.as_bytes(), &|_| None).unwrap();
    for version in [7400, 7500] {
        let mut bytes = binary_fbx(version);
        let model = load("triangle.fbx", &bytes, &|_| None).unwrap();
        assert_eq!(expected, model);
        reopen(&model);
        bytes.pop();
        assert!(load("broken.fbx", &bytes, &|_| None).is_err());
    }
}
