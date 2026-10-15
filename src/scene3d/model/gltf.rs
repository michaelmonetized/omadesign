//! Adapted from EffectCraft 0311bdc34d0dc10c2d18ff7875b4f52d483e5e27.
//! Copyright (c) 2026 ArtCraft Team and the EffectCraft contributors.
//! Used under the MIT license; see docs/licenses/effectcraft-model-MIT.txt.

//! glTF 2.0 import, written from the Khronos glTF 2.0 specification.
//!
//! Supports `.gltf` (JSON with external `.bin`/image files or `data:` URIs) and `.glb` (binary
//! container: a JSON chunk and a BIN chunk). Imports the default scene's node hierarchy (TRS or
//! matrix), triangle primitives (`TRIANGLES`, `TRIANGLE_STRIP`, `TRIANGLE_FAN`) with POSITION,
//! NORMAL, TANGENT, TEXCOORD_0, JOINTS_0 and WEIGHTS_0, PBR metallic-roughness materials
//! (base colour, metallic-roughness, normal, occlusion and emissive textures; alpha modes;
//! double-sided; `KHR_materials_emissive_strength`, `KHR_materials_unlit`), embedded or
//! external PNG/JPEG images with sampler wrap modes, skins and animations (step, linear and
//! cubic-spline translation/rotation/scale), perspective and orthographic cameras, and
//! `KHR_lights_punctual` lights (directional, point, spot). Sparse accessors and morph targets
//! return explicit unsupported-feature errors.

use std::sync::Arc;

use serde_json::Value;

use super::{
    AlphaMode, Animation, Channel, ChannelPath, Interp, Mat4, Material, Mesh, Model, ModelError,
    Node, Primitive, Result, Skin, TexRef, Texture, Wrap, perr,
};

const GLB_MAGIC: u32 = 0x4654_6C67;
const CHUNK_JSON: u32 = 0x4E4F_534A;
const CHUNK_BIN: u32 = 0x004E_4942;

/// Split a GLB container into its JSON text and BIN chunk.
pub fn split_glb(b: &[u8]) -> Result<(&[u8], Option<&[u8]>)> {
    let u32_at = |o: usize| -> Option<u32> {
        b.get(o..o + 4)
            .map(|s| u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
    };
    if u32_at(0) != Some(GLB_MAGIC) {
        return Err(perr("not a GLB file"));
    }
    if u32_at(4) != Some(2) {
        return Err(ModelError::Unsupported(format!(
            "GLB version {}",
            u32_at(4).unwrap_or(0)
        )));
    }
    let total = u32_at(8).unwrap_or(0) as usize;
    if total != b.len() || total < 12 {
        return Err(perr("GLB declared length does not match file"));
    }
    let mut o = 12;
    let mut json = None;
    let mut bin = None;
    while o + 8 <= total {
        let len = u32_at(o).unwrap_or(0) as usize;
        let ty = u32_at(o + 4).unwrap_or(0);
        let end = o
            .checked_add(8)
            .and_then(|v| v.checked_add(len))
            .filter(|&v| v <= total)
            .ok_or_else(|| perr("GLB chunk runs past the end of the file"))?;
        if len % 4 != 0 {
            return Err(perr("GLB chunk is not four-byte aligned"));
        }
        let data = b
            .get(o + 8..end)
            .ok_or_else(|| perr("GLB chunk runs past the end of the file"))?;
        match ty {
            CHUNK_JSON if json.is_none() => json = Some(data),
            CHUNK_BIN if bin.is_none() => bin = Some(data),
            _ => {}
        }
        o = end;
    }
    if o != total {
        return Err(perr("GLB truncated chunk header"));
    }
    Ok((json.ok_or_else(|| perr("GLB without a JSON chunk"))?, bin))
}

/// Decode standard base64 (with or without padding; whitespace ignored).
pub fn base64_decode(s: &str) -> Option<Vec<u8>> {
    use base64::Engine;
    let compact: String = s.chars().filter(|c| !c.is_ascii_whitespace()).collect();
    base64::engine::general_purpose::STANDARD
        .decode(&compact)
        .or_else(|_| base64::engine::general_purpose::STANDARD_NO_PAD.decode(&compact))
        .ok()
}

/// Percent-decode a relative URI.
pub fn uri_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%'
            && i + 2 < b.len()
            && let Ok(v) =
                u8::from_str_radix(std::str::from_utf8(&b[i + 1..i + 3]).unwrap_or("zz"), 16)
        {
            out.push(v);
            i += 3;
            continue;
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Bytes of a `data:` URI or an external resource.
fn load_uri(uri: &str, resolve: &dyn Fn(&str) -> Option<Vec<u8>>) -> Result<Vec<u8>> {
    if uri.len() > super::validation::MAX_SOURCE_BYTES {
        return Err(perr("data URI exceeds source byte limit"));
    }
    if let Some(rest) = uri.strip_prefix("data:") {
        let (meta, data) = rest
            .split_once(',')
            .ok_or_else(|| perr("malformed data URI"))?;
        return if meta.ends_with(";base64") {
            base64_decode(data).ok_or_else(|| perr("bad base64 in data URI"))
        } else {
            percent_bytes(data)
        };
    }
    let name = uri_decode(uri);
    resolve(&name).ok_or(ModelError::Missing(name))
}

struct Doc<'a> {
    j: &'a Value,
    buffers: Vec<Vec<u8>>,
}

fn arr<'v>(v: &'v Value, k: &str) -> &'v [Value] {
    v.get(k)
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[])
}
fn idx(v: &Value, k: &str) -> Option<usize> {
    v.get(k).and_then(Value::as_u64).map(|x| x as usize)
}
fn num(v: &Value, k: &str, d: f64) -> f64 {
    v.get(k).and_then(Value::as_f64).unwrap_or(d)
}
fn nums<const N: usize>(v: &Value, k: &str, d: [f64; N]) -> [f64; N] {
    let mut out = d;
    if let Some(a) = v.get(k).and_then(Value::as_array) {
        for (i, x) in a.iter().take(N).enumerate() {
            out[i] = x.as_f64().unwrap_or(d[i]);
        }
    }
    out
}

impl Doc<'_> {
    /// An accessor as `f32` rows of `n` components (normalised integers mapped to 0..1 / -1..1).
    fn floats(&self, a: usize) -> Result<(Vec<f32>, usize)> {
        let acc = arr(self.j, "accessors")
            .get(a)
            .ok_or_else(|| perr(format!("no accessor {a}")))?;
        let n = match acc.get("type").and_then(Value::as_str).unwrap_or("") {
            "SCALAR" => 1,
            "VEC2" => 2,
            "VEC3" => 3,
            "VEC4" => 4,
            "MAT2" => 4,
            "MAT3" => 9,
            "MAT4" => 16,
            t => return Err(perr(format!("accessor type `{t}`"))),
        };
        let count = idx(acc, "count").unwrap_or(0);
        let ct = idx(acc, "componentType").unwrap_or(5126);
        let norm = acc
            .get("normalized")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let csize = match ct {
            5120 | 5121 => 1,
            5122 | 5123 => 2,
            5125 | 5126 => 4,
            _ => return Err(perr(format!("componentType {ct}"))),
        };
        let length = count
            .checked_mul(n)
            .filter(|&v| v <= super::validation::MAX_FLOATS)
            .ok_or_else(|| perr("accessor exceeds float budget"))?;
        let mut out = vec![0.0f32; length];
        let Some(bv) = idx(acc, "bufferView") else {
            // No buffer view: all zeros (sparse substitution is not supported).
            return Ok((out, n));
        };
        let view = arr(self.j, "bufferViews")
            .get(bv)
            .ok_or_else(|| perr(format!("no bufferView {bv}")))?;
        let buf = self
            .buffers
            .get(idx(view, "buffer").unwrap_or(0))
            .ok_or_else(|| perr("bufferView buffer out of range"))?;
        let view_start = idx(view, "byteOffset").unwrap_or(0);
        let view_end = view_start
            .checked_add(idx(view, "byteLength").unwrap_or(0))
            .ok_or_else(|| perr("buffer view overflow"))?;
        let buf = buf
            .get(view_start..view_end)
            .ok_or_else(|| perr("buffer view outside buffer"))?;
        let base = idx(acc, "byteOffset").unwrap_or(0);
        let elem = csize * n;
        // Matrices of 1/2-byte components pad columns to 4 bytes; ignored (rare).
        let stride = idx(view, "byteStride").filter(|s| *s > 0).unwrap_or(elem);
        if stride < elem {
            return Err(perr("accessor stride is shorter than one element"));
        }
        for i in 0..count {
            for c in 0..n {
                let o = i
                    .checked_mul(stride)
                    .and_then(|v| v.checked_add(base))
                    .and_then(|v| v.checked_add(c * csize))
                    .ok_or_else(|| perr("accessor offset overflow"))?;
                let end = o
                    .checked_add(csize)
                    .ok_or_else(|| perr("accessor offset overflow"))?;
                let s = buf
                    .get(o..end)
                    .ok_or_else(|| perr("accessor reads past its buffer view"))?;
                out[i * n + c] = match ct {
                    5126 => f32::from_le_bytes([s[0], s[1], s[2], s[3]]),
                    5125 => u32::from_le_bytes([s[0], s[1], s[2], s[3]]) as f32,
                    5123 => {
                        let v = u16::from_le_bytes([s[0], s[1]]) as f32;
                        if norm { v / 65535.0 } else { v }
                    }
                    5122 => {
                        let v = i16::from_le_bytes([s[0], s[1]]) as f32;
                        if norm { (v / 32767.0).max(-1.0) } else { v }
                    }
                    5121 => {
                        let v = s[0] as f32;
                        if norm { v / 255.0 } else { v }
                    }
                    _ => {
                        let v = s[0] as i8 as f32;
                        if norm { (v / 127.0).max(-1.0) } else { v }
                    }
                };
            }
        }
        Ok((out, n))
    }

    /// Integer accessor (indices, joints) read exactly.
    fn ints(&self, a: usize) -> Result<Vec<u32>> {
        let acc = arr(self.j, "accessors")
            .get(a)
            .ok_or_else(|| perr(format!("no accessor {a}")))?;
        let ct = idx(acc, "componentType").unwrap_or(5125);
        if ct == 5126 {
            return Ok(self.floats(a)?.0.into_iter().map(|v| v as u32).collect());
        }
        let n = match acc.get("type").and_then(Value::as_str).unwrap_or("SCALAR") {
            "VEC4" => 4,
            "VEC3" => 3,
            "VEC2" => 2,
            _ => 1,
        };
        let count = idx(acc, "count").unwrap_or(0);
        let csize = match ct {
            5120 | 5121 => 1,
            5122 | 5123 => 2,
            _ => 4,
        };
        let length = count
            .checked_mul(n)
            .filter(|&v| v <= super::validation::MAX_FLOATS)
            .ok_or_else(|| perr("accessor exceeds integer budget"))?;
        let Some(bv) = idx(acc, "bufferView") else {
            return Ok(vec![0; length]);
        };
        let view = arr(self.j, "bufferViews")
            .get(bv)
            .ok_or_else(|| perr(format!("no bufferView {bv}")))?;
        let buf = self
            .buffers
            .get(idx(view, "buffer").unwrap_or(0))
            .ok_or_else(|| perr("bufferView buffer out of range"))?;
        let view_start = idx(view, "byteOffset").unwrap_or(0);
        let view_end = view_start
            .checked_add(idx(view, "byteLength").unwrap_or(0))
            .ok_or_else(|| perr("buffer view overflow"))?;
        let buf = buf
            .get(view_start..view_end)
            .ok_or_else(|| perr("buffer view outside buffer"))?;
        let base = idx(acc, "byteOffset").unwrap_or(0);
        let stride = idx(view, "byteStride")
            .filter(|s| *s > 0)
            .unwrap_or(csize * n);
        if stride < csize * n {
            return Err(perr("integer accessor stride is shorter than one element"));
        }
        let mut out = Vec::with_capacity(length);
        for i in 0..count {
            for c in 0..n {
                let o = i
                    .checked_mul(stride)
                    .and_then(|v| v.checked_add(base))
                    .and_then(|v| v.checked_add(c * csize))
                    .ok_or_else(|| perr("accessor offset overflow"))?;
                let end = o
                    .checked_add(csize)
                    .ok_or_else(|| perr("accessor offset overflow"))?;
                let s = buf
                    .get(o..end)
                    .ok_or_else(|| perr("accessor reads past its buffer view"))?;
                out.push(match csize {
                    1 => s[0] as u32,
                    2 => u16::from_le_bytes([s[0], s[1]]) as u32,
                    _ => u32::from_le_bytes([s[0], s[1], s[2], s[3]]),
                });
            }
        }
        Ok(out)
    }
}

/// Parse a `.gltf` (JSON) or `.glb` file.
pub fn parse(bytes: &[u8], resolve: &dyn Fn(&str) -> Option<Vec<u8>>) -> Result<Model> {
    super::validation::check_source(bytes)?;
    let (json, bin) = if bytes.starts_with(b"glTF") {
        split_glb(bytes)?
    } else {
        (bytes, None)
    };
    let j: Value = serde_json::from_slice(json).map_err(|e| perr(format!("glTF JSON: {e}")))?;
    let ver = j
        .get("asset")
        .and_then(|a| a.get("version"))
        .and_then(Value::as_str)
        .unwrap_or("");
    if ver != "2.0" {
        return Err(ModelError::Unsupported(format!(
            "glTF version `{ver}` (2.0 required)"
        )));
    }
    preflight(&j)?;
    for ext in arr(&j, "extensionsRequired") {
        let e = ext.as_str().unwrap_or("");
        if !matches!(
            e,
            "KHR_materials_unlit" | "KHR_materials_emissive_strength" | "KHR_lights_punctual"
        ) {
            return Err(ModelError::Unsupported(format!("required extension {e}")));
        }
    }
    let mut buffers = vec![];
    for (i, b) in arr(&j, "buffers").iter().enumerate() {
        let mut data = match b.get("uri").and_then(Value::as_str) {
            Some(u) => load_uri(u, resolve)?,
            None if i == 0 => bin
                .map(<[u8]>::to_vec)
                .ok_or_else(|| perr("buffer 0 has no uri and the file has no BIN chunk"))?,
            None => vec![],
        };
        if data.len() > super::validation::MAX_SOURCE_BYTES
            || idx(b, "byteLength").is_none_or(|n| n > data.len())
        {
            return Err(perr(format!(
                "buffer {i}: missing bytes or byte budget exceeded"
            )));
        }
        if buffers.iter().map(Vec::len).sum::<usize>() + data.len()
            > super::validation::MAX_RESOURCE_BYTES
        {
            return Err(perr("embedded buffer budget exceeded"));
        }
        data.truncate(idx(b, "byteLength").unwrap_or(0));
        buffers.push(data);
    }
    let d = Doc { j: &j, buffers };

    // Images → textures (decoded once per image; samplers set the wrap modes).
    let mut images: Vec<Option<Arc<Texture>>> = vec![];
    let mut pixels = 0usize;
    for im in arr(&j, "images") {
        let data = if let Some(u) = im.get("uri").and_then(Value::as_str) {
            load_uri(u, resolve)?
        } else if let Some(bv) = idx(im, "bufferView") {
            let v = arr(&j, "bufferViews")
                .get(bv)
                .ok_or_else(|| perr("image bufferView out of range"))?;
            let b = d
                .buffers
                .get(idx(v, "buffer").unwrap_or(0))
                .ok_or_else(|| perr("image buffer out of range"))?;
            let o = idx(v, "byteOffset").unwrap_or(0);
            let end = o
                .checked_add(idx(v, "byteLength").unwrap_or(0))
                .ok_or_else(|| perr("image offset overflow"))?;
            b.get(o..end)
                .ok_or_else(|| perr("image outside buffer"))?
                .to_vec()
        } else {
            return Err(perr("image requires uri or bufferView"));
        };
        let texture = Texture::decode(&data)?;
        pixels = pixels
            .checked_add(texture.data.len())
            .ok_or_else(|| perr("texture pixel overflow"))?;
        if pixels > super::validation::MAX_TEXTURE_PIXELS {
            return Err(perr("total texture pixel budget exceeded"));
        }
        images.push(Some(Arc::new(texture)));
    }
    let wrap = |v: u64| match v {
        33071 => Wrap::Clamp,
        33648 => Wrap::Mirror,
        _ => Wrap::Repeat,
    };
    let mut textures: Vec<Arc<Texture>> = vec![];
    let mut tex_map: Vec<Option<usize>> = vec![];
    for t in arr(&j, "textures") {
        let Some(img) = idx(t, "source").and_then(|s| images.get(s).cloned().flatten()) else {
            return Err(perr("texture source out of range"));
        };
        let samp = idx(t, "sampler").and_then(|s| arr(&j, "samplers").get(s));
        let ws = samp
            .and_then(|s| s.get("wrapS"))
            .and_then(Value::as_u64)
            .unwrap_or(10497);
        let wt = samp
            .and_then(|s| s.get("wrapT"))
            .and_then(Value::as_u64)
            .unwrap_or(10497);
        let w = [wrap(ws), wrap(wt)];
        if textures.iter().map(|t| t.data.len()).sum::<usize>() + img.data.len()
            > super::validation::MAX_TEXTURE_PIXELS
        {
            return Err(perr("texture sampler copies exceed pixel budget"));
        }
        let tex = if img.wrap == w {
            img
        } else {
            Arc::new(Texture {
                wrap: w,
                ..(*img).clone()
            })
        };
        tex_map.push(Some(textures.len()));
        textures.push(tex);
    }
    let tref = |v: Option<&Value>| -> Option<TexRef> {
        let v = v?;
        let t = tex_map.get(idx(v, "index")?).copied().flatten()?;
        Some(TexRef {
            texture: t,
            uv_set: idx(v, "texCoord").unwrap_or(0) as u32,
        })
    };

    let mut materials = vec![];
    for m in arr(&j, "materials") {
        let pbr = m
            .get("pbrMetallicRoughness")
            .cloned()
            .unwrap_or(Value::Null);
        let bc = nums(&pbr, "baseColorFactor", [1.0; 4]);
        let ext = m.get("extensions");
        let strength = ext
            .and_then(|e| e.get("KHR_materials_emissive_strength"))
            .map_or(1.0, |e| num(e, "emissiveStrength", 1.0));
        let em = nums(m, "emissiveFactor", [0.0; 3]);
        materials.push(Material {
            name: m
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            base_color: bc.map(|v| v as f32),
            base_color_tex: tref(pbr.get("baseColorTexture")),
            metallic: num(&pbr, "metallicFactor", 1.0) as f32,
            roughness: num(&pbr, "roughnessFactor", 1.0) as f32,
            metallic_roughness_tex: tref(pbr.get("metallicRoughnessTexture")),
            normal_tex: tref(m.get("normalTexture")),
            normal_scale: m.get("normalTexture").map_or(1.0, |n| num(n, "scale", 1.0)) as f32,
            occlusion_tex: tref(m.get("occlusionTexture")),
            occlusion_strength: m
                .get("occlusionTexture")
                .map_or(1.0, |n| num(n, "strength", 1.0)) as f32,
            emissive: [
                (em[0] * strength) as f32,
                (em[1] * strength) as f32,
                (em[2] * strength) as f32,
            ],
            emissive_tex: tref(m.get("emissiveTexture")),
            alpha_mode: match m.get("alphaMode").and_then(Value::as_str) {
                Some("MASK") => AlphaMode::Mask,
                Some("BLEND") => AlphaMode::Blend,
                _ => AlphaMode::Opaque,
            },
            alpha_cutoff: num(m, "alphaCutoff", 0.5) as f32,
            double_sided: m
                .get("doubleSided")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            unlit: ext.and_then(|e| e.get("KHR_materials_unlit")).is_some(),
        });
    }

    let mut meshes = vec![];
    let (mut vertex_count, mut triangle_count) = (0usize, 0usize);
    for m in arr(&j, "meshes") {
        let mut mesh = Mesh {
            name: m
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            primitives: vec![],
        };
        for p in arr(m, "primitives") {
            if let Some(prim) = primitive(&d, p, &materials)? {
                vertex_count = vertex_count
                    .checked_add(prim.positions.len())
                    .ok_or_else(|| perr("vertex count overflow"))?;
                triangle_count = triangle_count
                    .checked_add(prim.triangle_count())
                    .ok_or_else(|| perr("triangle count overflow"))?;
                if vertex_count > super::validation::MAX_VERTICES
                    || triangle_count > super::validation::MAX_TRIANGLES
                {
                    return Err(perr("glTF geometry budget exceeded"));
                }
                mesh.primitives.push(prim);
            }
        }
        meshes.push(mesh);
    }

    let mut nodes = vec![];
    for n in arr(&j, "nodes") {
        let matrix = n
            .get("matrix")
            .and_then(Value::as_array)
            .filter(|a| a.len() == 16)
            .map(|a| {
                // Column-major in the file.
                let v: Vec<f64> = a.iter().map(|x| x.as_f64().unwrap_or(0.0)).collect();
                let mut m = [[0.0; 4]; 4];
                for c in 0..4 {
                    for r in 0..4 {
                        m[r][c] = v[c * 4 + r];
                    }
                }
                Mat4(m)
            });
        nodes.push(Node {
            name: n
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            translation: nums(n, "translation", [0.0; 3]),
            rotation: nums(n, "rotation", [0.0, 0.0, 0.0, 1.0]),
            scale: nums(n, "scale", [1.0; 3]),
            matrix,
            mesh: idx(n, "mesh"),
            skin: idx(n, "skin"),
            camera: idx(n, "camera"),
            light: n
                .get("extensions")
                .and_then(|e| e.get("KHR_lights_punctual"))
                .and_then(|e| idx(e, "light")),
            children: arr(n, "children")
                .iter()
                .filter_map(Value::as_u64)
                .map(|c| c as usize)
                .collect(),
        });
    }

    let scene = idx(&j, "scene").unwrap_or(0);
    let roots: Vec<usize> = match arr(&j, "scenes").get(scene) {
        Some(s) => arr(s, "nodes")
            .iter()
            .filter_map(Value::as_u64)
            .map(|c| c as usize)
            .collect(),
        None => {
            // No scene: every node that is nobody's child.
            let mut child = vec![false; nodes.len()];
            for n in &nodes {
                for &c in &n.children {
                    if c < child.len() {
                        child[c] = true;
                    }
                }
            }
            (0..nodes.len()).filter(|&i| !child[i]).collect()
        }
    };

    let mut skins = vec![];
    for s in arr(&j, "skins") {
        let joints: Vec<usize> = arr(s, "joints")
            .iter()
            .filter_map(Value::as_u64)
            .map(|c| c as usize)
            .collect();
        let inverse_bind = match idx(s, "inverseBindMatrices") {
            Some(a) => {
                let (f, _) = d.floats(a)?;
                f.as_chunks::<16>()
                    .0
                    .iter()
                    .map(|v| {
                        let mut m = [[0.0; 4]; 4];
                        for c in 0..4 {
                            for r in 0..4 {
                                m[r][c] = v[c * 4 + r] as f64;
                            }
                        }
                        Mat4(m)
                    })
                    .collect()
            }
            None => vec![Mat4::IDENTITY; joints.len()],
        };
        skins.push(Skin {
            joints,
            inverse_bind,
        });
    }

    let mut animations = vec![];
    let mut animation_floats = 0usize;
    for a in arr(&j, "animations") {
        let samplers = arr(a, "samplers");
        let mut anim = Animation {
            name: a
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            channels: vec![],
        };
        for c in arr(a, "channels") {
            let Some(t) = c.get("target") else { continue };
            let Some(node) = idx(t, "node") else { continue };
            let path = match t.get("path").and_then(Value::as_str) {
                Some("translation") => ChannelPath::Translation,
                Some("rotation") => ChannelPath::Rotation,
                Some("scale") => ChannelPath::Scale,
                _ => continue,
            };
            let Some(s) = idx(c, "sampler").and_then(|s| samplers.get(s)) else {
                continue;
            };
            let interp = match s.get("interpolation").and_then(Value::as_str) {
                Some("STEP") => Interp::Step,
                Some("CUBICSPLINE") => Interp::CubicSpline,
                _ => Interp::Linear,
            };
            let (Some(input), Some(output)) = (idx(s, "input"), idx(s, "output")) else {
                continue;
            };
            let (times, _) = d.floats(input)?;
            let (values, _) = d.floats(output)?;
            animation_floats = animation_floats
                .checked_add(times.len() + values.len())
                .ok_or_else(|| perr("animation float count overflow"))?;
            if animation_floats > super::validation::MAX_FLOATS {
                return Err(perr("animation float budget exceeded"));
            }
            anim.channels.push(Channel {
                node,
                path,
                interp,
                times,
                values,
            });
        }
        animations.push(anim);
    }

    let cameras = arr(&j, "cameras")
        .iter()
        .enumerate()
        .map(|(i, c)| camera(c, i))
        .collect();
    let lights = j
        .get("extensions")
        .and_then(|e| e.get("KHR_lights_punctual"))
        .map(|e| {
            arr(e, "lights")
                .iter()
                .enumerate()
                .map(|(i, l)| light(l, i))
                .collect()
        })
        .unwrap_or_default();
    let model = Model {
        meshes,
        nodes,
        roots,
        materials,
        textures,
        skins,
        animations,
        cameras,
        lights,
    };
    model.validate()?;
    Ok(model)
}

fn name_or(v: &Value, d: String) -> String {
    v.get("name")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .unwrap_or(d)
}

/// A glTF camera (glTF 2.0 §5.12): `perspective` {yfov, aspectRatio?, znear} or
/// `orthographic` {xmag, ymag}.
fn camera(c: &Value, i: usize) -> super::ModelCamera {
    let projection = match (c.get("type").and_then(Value::as_str), c.get("orthographic")) {
        (Some("orthographic"), Some(o)) => super::CameraProjection::Orthographic {
            xmag: num(o, "xmag", 1.0),
            ymag: num(o, "ymag", 1.0),
            znear: num(o, "znear", 0.0),
            zfar: num(o, "zfar", 1000.0),
        },
        _ => {
            let p = c.get("perspective").cloned().unwrap_or(Value::Null);
            super::CameraProjection::Perspective {
                yfov: num(&p, "yfov", 0.8),
                aspect: p.get("aspectRatio").and_then(Value::as_f64),
                znear: num(&p, "znear", 0.01),
                zfar: p.get("zfar").and_then(Value::as_f64),
            }
        }
    };
    super::ModelCamera {
        name: name_or(c, format!("Camera {}", i + 1)),
        projection,
    }
}

/// A `KHR_lights_punctual` light: type, color, intensity, range, spot cone angles.
fn light(l: &Value, i: usize) -> super::ModelLight {
    let kind = match l.get("type").and_then(Value::as_str) {
        Some("directional") => super::ModelLightKind::Directional,
        Some("spot") => {
            let s = l.get("spot").cloned().unwrap_or(Value::Null);
            let outer = num(&s, "outerConeAngle", std::f64::consts::FRAC_PI_4);
            super::ModelLightKind::Spot {
                inner: num(&s, "innerConeAngle", 0.0),
                outer,
            }
        }
        _ => super::ModelLightKind::Point,
    };
    super::ModelLight {
        name: name_or(l, format!("Light {}", i + 1)),
        kind,
        color: nums(l, "color", [1.0; 3]),
        intensity: num(l, "intensity", 1.0),
        range: l.get("range").and_then(Value::as_f64),
    }
}

fn primitive(d: &Doc, p: &Value, materials: &[Material]) -> Result<Option<Primitive>> {
    let mode = idx(p, "mode").unwrap_or(4);
    if !matches!(mode, 4..=6) {
        return Err(ModelError::Unsupported(format!(
            "glTF primitive mode {mode}; triangles required"
        )));
    }
    let attrs = p
        .get("attributes")
        .ok_or_else(|| perr("primitive without attributes"))?;
    let pa = idx(attrs, "POSITION").ok_or_else(|| perr("primitive without POSITION"))?;
    let (pos, _) = d.floats(pa)?;
    let positions: Vec<[f32; 3]> = pos
        .as_chunks::<3>()
        .0
        .iter()
        .map(|c| [c[0], c[1], c[2]])
        .collect();
    let n = positions.len();
    let vec3s = |a: Option<usize>| -> Result<Vec<[f32; 3]>> {
        Ok(match a {
            Some(a) => d
                .floats(a)?
                .0
                .as_chunks::<3>()
                .0
                .iter()
                .map(|c| [c[0], c[1], c[2]])
                .collect(),
            None => vec![],
        })
    };
    let mut normals = vec3s(idx(attrs, "NORMAL"))?;
    let uvs: Vec<[f32; 2]> = match idx(attrs, "TEXCOORD_0") {
        Some(a) => d
            .floats(a)?
            .0
            .as_chunks::<2>()
            .0
            .iter()
            .map(|c| [c[0], c[1]])
            .collect(),
        None => vec![],
    };
    let tangents: Vec<[f32; 4]> = match idx(attrs, "TANGENT") {
        Some(a) => d
            .floats(a)?
            .0
            .as_chunks::<4>()
            .0
            .iter()
            .map(|c| [c[0], c[1], c[2], c[3]])
            .collect(),
        None => vec![],
    };
    let joints: Vec<[u16; 4]> = match idx(attrs, "JOINTS_0") {
        Some(a) => d
            .ints(a)?
            .as_chunks::<4>()
            .0
            .iter()
            .map(|c| [c[0] as u16, c[1] as u16, c[2] as u16, c[3] as u16])
            .collect(),
        None => vec![],
    };
    let weights: Vec<[f32; 4]> = match idx(attrs, "WEIGHTS_0") {
        Some(a) => d
            .floats(a)?
            .0
            .as_chunks::<4>()
            .0
            .iter()
            .map(|c| [c[0], c[1], c[2], c[3]])
            .collect(),
        None => vec![],
    };
    let raw: Vec<u32> = match idx(p, "indices") {
        Some(a) => d.ints(a)?,
        None => (0..n as u32).collect(),
    };
    if raw.iter().any(|&i| i as usize >= n) {
        return Err(perr("index out of range"));
    }
    if mode == 4 && raw.len() % 3 != 0 {
        return Err(perr("triangle index count is not divisible by three"));
    }
    let mut indices = Vec::with_capacity(raw.len());
    match mode {
        5 => {
            for i in 2..raw.len() {
                let (a, b, c) = (raw[i - 2], raw[i - 1], raw[i]);
                if i % 2 == 0 {
                    indices.extend([a, b, c])
                } else {
                    indices.extend([b, a, c])
                }
            }
        }
        6 => {
            for i in 2..raw.len() {
                indices.extend([raw[0], raw[i - 1], raw[i]]);
            }
        }
        _ => indices.extend(raw.as_chunks::<3>().0.iter().flatten()),
    }
    if [
        normals.len(),
        uvs.len(),
        tangents.len(),
        joints.len(),
        weights.len(),
    ]
    .into_iter()
    .any(|len| len != 0 && len != n)
        || joints.is_empty() != weights.is_empty()
    {
        return Err(perr("glTF vertex attribute lengths disagree"));
    }
    let material = idx(p, "material");
    if material.is_some_and(|m| m >= materials.len()) {
        return Err(perr("primitive material index out of range"));
    }
    let skinned = joints.len() == n && weights.len() == n;
    let mut prim = Primitive {
        positions,
        normals,
        uvs: if uvs.len() == n { uvs } else { vec![] },
        tangents: if tangents.len() == n {
            tangents
        } else {
            vec![]
        },
        joints: if skinned { joints } else { vec![] },
        weights: if skinned { weights } else { vec![] },
        indices,
        material,
    };
    if prim.normals.is_empty() {
        if prim.indices.len() > super::validation::MAX_VERTICES {
            return Err(perr("flat normals exceed vertex budget"));
        }
        prim.flat_normals();
        prim.tangents.clear();
    }
    let needs_tangents = material.is_some_and(|m| materials[m].normal_tex.is_some());
    if needs_tangents && prim.tangents.is_empty() {
        prim.compute_tangents();
    }
    Ok(Some(prim))
}

fn preflight(j: &Value) -> Result<()> {
    for key in [
        "accessors",
        "bufferViews",
        "meshes",
        "nodes",
        "materials",
        "skins",
        "animations",
        "cameras",
        "scenes",
    ] {
        if arr(j, key).len() > super::validation::MAX_NODES {
            return Err(perr(format!("glTF {key} count exceeds import limit")));
        }
    }
    for key in ["buffers", "images", "textures", "samplers"] {
        if arr(j, key).len() > 256 {
            return Err(perr(format!("glTF {key} count exceeds resource limit")));
        }
    }
    let mut pending = vec![j];
    while let Some(value) = pending.pop() {
        match value {
            Value::Object(o) => {
                if let Some(Value::Object(ext)) = o.get("extensions") {
                    for key in ext.keys() {
                        if !matches!(
                            key.as_str(),
                            "KHR_materials_unlit"
                                | "KHR_materials_emissive_strength"
                                | "KHR_lights_punctual"
                        ) {
                            return Err(ModelError::Unsupported(format!("glTF extension {key}")));
                        }
                    }
                }
                pending.extend(o.values());
            }
            Value::Array(a) => pending.extend(a),
            _ => {}
        }
    }
    for (i, a) in arr(j, "accessors").iter().enumerate() {
        if a.get("sparse").is_some() {
            return Err(ModelError::Unsupported(format!(
                "glTF accessor {i}: sparse substitution"
            )));
        }
        let width = match a.get("type").and_then(Value::as_str) {
            Some("SCALAR") => 1,
            Some("VEC2") => 2,
            Some("VEC3") => 3,
            Some("VEC4" | "MAT2") => 4,
            Some("MAT3") => 9,
            Some("MAT4") => 16,
            _ => return Err(perr(format!("accessor {i}: invalid type"))),
        };
        let count = idx(a, "count").ok_or_else(|| perr(format!("accessor {i}: invalid count")))?;
        if count == 0
            || count
                .checked_mul(width)
                .is_none_or(|n| n > super::validation::MAX_FLOATS)
        {
            return Err(perr(format!("accessor {i}: count exceeds float budget")));
        }
        let ct = idx(a, "componentType").ok_or_else(|| perr("accessor componentType required"))?;
        if !matches!(ct, 5120 | 5121 | 5122 | 5123 | 5125 | 5126) {
            return Err(perr(format!("accessor {i}: invalid componentType")));
        }
        if width > 4 && ct != 5126 {
            return Err(ModelError::Unsupported(format!(
                "accessor {i}: packed integer matrix"
            )));
        }
        if idx(a, "bufferView").is_some_and(|v| v >= arr(j, "bufferViews").len()) {
            return Err(perr(format!("accessor {i}: bufferView out of range")));
        }
    }
    for (i, v) in arr(j, "bufferViews").iter().enumerate() {
        if idx(v, "buffer").is_none_or(|b| b >= arr(j, "buffers").len())
            || idx(v, "byteLength").is_none_or(|n| n > super::validation::MAX_RESOURCE_BYTES)
            || idx(v, "byteStride").is_some_and(|n| n == 0 || n > 252)
        {
            return Err(perr(format!(
                "bufferView {i}: invalid buffer, length or stride"
            )));
        }
    }
    let accessor = |index: usize, width: &str, unsigned: bool| -> Result<()> {
        let a = arr(j, "accessors")
            .get(index)
            .ok_or_else(|| perr("attribute accessor index out of range"))?;
        if a.get("type").and_then(Value::as_str) != Some(width)
            || (unsigned && !matches!(idx(a, "componentType"), Some(5121 | 5123 | 5125)))
        {
            return Err(perr(format!("accessor {index}: unexpected attribute type")));
        }
        Ok(())
    };
    let mut primitives = 0usize;
    for mesh in arr(j, "meshes") {
        if mesh.get("weights").is_some() {
            return Err(ModelError::Unsupported("glTF morph weights".into()));
        }
        primitives += arr(mesh, "primitives").len();
        if primitives > super::validation::MAX_NODES {
            return Err(perr("glTF primitive count exceeds import limit"));
        }
        for p in arr(mesh, "primitives") {
            if p.get("targets").is_some() {
                return Err(ModelError::Unsupported("glTF morph targets".into()));
            }
            if let Some(a) = p.get("attributes").and_then(Value::as_object) {
                for (name, value) in a {
                    let index = value
                        .as_u64()
                        .ok_or_else(|| perr("attribute index must be unsigned"))?
                        as usize;
                    let (width, unsigned) = match name.as_str() {
                        "POSITION" | "NORMAL" => ("VEC3", false),
                        "TEXCOORD_0" => ("VEC2", false),
                        "TANGENT" | "WEIGHTS_0" => ("VEC4", false),
                        "JOINTS_0" => ("VEC4", true),
                        _ => {
                            return Err(ModelError::Unsupported(format!(
                                "glTF vertex attribute {name}"
                            )));
                        }
                    };
                    accessor(index, width, unsigned)?;
                    if name == "JOINTS_0"
                        && idx(&arr(j, "accessors")[index], "componentType") == Some(5125)
                    {
                        return Err(perr("JOINTS_0 requires unsigned bytes or shorts"));
                    }
                }
            }
            if let Some(i) = idx(p, "indices") {
                accessor(i, "SCALAR", true)?;
            }
        }
    }
    let textures = arr(j, "textures");
    for t in textures {
        if idx(t, "source").is_none_or(|n| n >= arr(j, "images").len())
            || idx(t, "sampler").is_some_and(|n| n >= arr(j, "samplers").len())
        {
            return Err(perr("texture source or sampler out of range"));
        }
    }
    for m in arr(j, "materials") {
        let mut refs = vec![m];
        if let Some(p) = m.get("pbrMetallicRoughness") {
            refs.push(p);
        }
        for object in refs {
            for key in [
                "baseColorTexture",
                "metallicRoughnessTexture",
                "normalTexture",
                "occlusionTexture",
                "emissiveTexture",
            ] {
                if let Some(r) = object.get(key) {
                    if idx(r, "index").is_none_or(|n| n >= textures.len()) {
                        return Err(perr(format!("{key}: texture index out of range")));
                    }
                    if idx(r, "texCoord").unwrap_or(0) != 0 {
                        return Err(ModelError::Unsupported(format!(
                            "{key}: only TEXCOORD_0 is supported"
                        )));
                    }
                }
            }
        }
    }
    for n in arr(j, "nodes") {
        if n.get("weights").is_some() {
            return Err(ModelError::Unsupported("glTF node morph weights".into()));
        }
        for (key, width) in [
            ("translation", 3),
            ("rotation", 4),
            ("scale", 3),
            ("matrix", 16),
        ] {
            if let Some(a) = n.get(key) {
                if a.as_array()
                    .is_none_or(|v| v.len() != width || v.iter().any(|c| c.as_f64().is_none()))
                {
                    return Err(perr(format!("node {key}: incorrect numeric array")));
                }
            }
        }
        if arr(n, "children").iter().any(|v| v.as_u64().is_none()) {
            return Err(perr("node children must be unsigned indices"));
        }
    }
    if !arr(j, "scenes").is_empty() && idx(j, "scene").unwrap_or(0) >= arr(j, "scenes").len() {
        return Err(perr("default scene index out of range"));
    }
    for scene in arr(j, "scenes") {
        if arr(scene, "nodes").iter().any(|v| v.as_u64().is_none()) {
            return Err(perr("scene roots must be unsigned indices"));
        }
    }
    for s in arr(j, "skins") {
        if arr(s, "joints").iter().any(|v| v.as_u64().is_none()) {
            return Err(perr("skin joints must be unsigned indices"));
        }
        if let Some(a) = idx(s, "inverseBindMatrices") {
            accessor(a, "MAT4", false)?;
        }
    }
    for a in arr(j, "animations") {
        if arr(a, "channels").len() > super::validation::MAX_NODES {
            return Err(perr("animation channel limit exceeded"));
        }
        for s in arr(a, "samplers") {
            if !matches!(
                s.get("interpolation").and_then(Value::as_str),
                None | Some("STEP" | "LINEAR" | "CUBICSPLINE")
            ) {
                return Err(perr("unknown glTF animation interpolation"));
            }
            accessor(
                idx(s, "input").ok_or_else(|| perr("animation input required"))?,
                "SCALAR",
                false,
            )?;
        }
        for c in arr(a, "channels") {
            let t = c
                .get("target")
                .ok_or_else(|| perr("animation target required"))?;
            let width = match t.get("path").and_then(Value::as_str) {
                Some("translation" | "scale") => "VEC3",
                Some("rotation") => "VEC4",
                v => {
                    return Err(ModelError::Unsupported(format!(
                        "glTF animation target {v:?}"
                    )));
                }
            };
            let s = arr(a, "samplers")
                .get(idx(c, "sampler").ok_or_else(|| perr("animation sampler required"))?)
                .ok_or_else(|| perr("animation sampler out of range"))?;
            if idx(t, "node").is_none_or(|n| n >= arr(j, "nodes").len()) {
                return Err(perr("animation target node out of range"));
            }
            accessor(
                idx(s, "output").ok_or_else(|| perr("animation output required"))?,
                width,
                false,
            )?;
        }
    }
    for c in arr(j, "cameras") {
        if !matches!(
            c.get("type").and_then(Value::as_str),
            Some("perspective" | "orthographic")
        ) {
            return Err(perr("unknown glTF camera type"));
        }
    }
    Ok(())
}

fn percent_bytes(s: &str) -> Result<Vec<u8>> {
    let input = s.as_bytes();
    let mut output = Vec::with_capacity(input.len());
    let mut i = 0;
    while i < input.len() {
        if input[i] == b'%' {
            let hex = input
                .get(i + 1..i + 3)
                .and_then(|b| std::str::from_utf8(b).ok())
                .and_then(|s| u8::from_str_radix(s, 16).ok())
                .ok_or_else(|| perr("invalid percent escape in data URI"))?;
            output.push(hex);
            i += 3;
        } else {
            output.push(input[i]);
            i += 1;
        }
    }
    Ok(output)
}
