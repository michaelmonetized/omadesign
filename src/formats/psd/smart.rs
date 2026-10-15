//! Retained placed sources, projective placement and editable Photoshop smart filters.
use super::{
    codec::Reader,
    descriptor::{self, Descriptor, Value as D},
    warn,
};
use crate::{
    color::Blend,
    document::{Cmd, Document, Layer, LayerKind, Pixels},
    geom::Pt,
    source_attachment::SourceAttachment,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use crate::deep_color::storage;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SmartObject {
    pub descriptor: Descriptor,
    pub quad: [f64; 8],
    pub size: [f64; 2],
    pub preview_transform: [f32; 6],
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub external_link: Option<Descriptor>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub edit_path: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub retained_blocks: Vec<([u8; 4], Vec<u8>)>,
}

pub(super) struct LinkedFile {
    pub id: String,
    pub name: String,
    pub bytes: Option<Vec<u8>>,
    pub external: Option<Descriptor>,
}
#[derive(Clone)]
pub struct SmartFilter {
    pub index: usize,
    pub name: String,
    pub enabled: bool,
    pub opacity: f32,
    pub blend: Blend,
    pub settings: Option<crate::raster::Settings>,
}

fn text(d: &Descriptor, key: &str) -> String {
    d.get(key).and_then(D::text).unwrap_or("").into()
}
fn number(d: &Descriptor, key: &str, default: f64) -> f64 {
    d.get(key)
        .and_then(D::number)
        .filter(|v| v.is_finite())
        .unwrap_or(default)
}
fn boolean(d: &Descriptor, key: &str, default: bool) -> bool {
    if let Some(D::Bool(v)) = d.get(key) {
        *v
    } else {
        default
    }
}
fn unicode(r: &mut Reader<'_>) -> Result<String, String> {
    let n = r.u32()? as usize;
    if n > 1_048_576 || n > r.remaining() / 2 {
        return Err("Invalid Photoshop linked filename".into());
    }
    let units = (0..n).map(|_| r.u16()).collect::<Result<Vec<_>, _>>()?;
    Ok(String::from_utf16_lossy(&units)
        .trim_end_matches('\0')
        .into())
}

pub(super) fn linked_files(bytes: &[u8]) -> Result<Vec<LinkedFile>, String> {
    let mut r = Reader::new(bytes);
    let mut files = Vec::new();
    let mut total = 0usize;
    while r.remaining() >= 8 {
        if files.len() >= 8192 {
            return Err("Excessive Photoshop linked sources".into());
        }
        let n = usize::try_from(r.u64()?).map_err(|_| "Photoshop source length overflow")?;
        let mut item = Reader::new(r.take(n)?);
        let kind = item.key()?;
        let version = item.u32()?;
        if !(1..=7).contains(&version) {
            return Err("Unsupported Photoshop linked-source version".into());
        }
        let len = item.u8()? as usize;
        let id = String::from_utf8_lossy(item.take(len)?).into_owned();
        let name = unicode(&mut item)?;
        item.take(8)?;
        let length =
            usize::try_from(item.u64()?).map_err(|_| "Photoshop embedded source too large")?;
        if item.u8()? != 0 {
            let _ = descriptor::read_versioned(&mut item)?;
        }
        let external = if &kind == b"liFE" {
            let d = descriptor::read_versioned(&mut item)?;
            if version > 3 {
                item.take(16)?;
            }
            Some(d)
        } else {
            None
        };
        let file_size = if &kind == b"liFE" {
            usize::try_from(item.u64()?).map_err(|_| "Photoshop external source too large")?
        } else {
            0
        };
        let source = if &kind == b"liFD" && length > 0 {
            Some(item.take(length)?.to_vec())
        } else {
            None
        };
        if &kind == b"liFA" {
            item.take(8)?;
        }
        if version >= 5 {
            let _ = unicode(&mut item)?;
        }
        if version >= 6 {
            item.take(8)?;
        }
        if version >= 7 {
            item.take(1)?;
        }
        let source = if &kind == b"liFE" && version == 2 && file_size > 0 {
            Some(item.take(file_size)?.to_vec())
        } else {
            source
        };
        total = total
            .checked_add(source.as_ref().map_or(0, Vec::len))
            .ok_or("Photoshop retained source size overflow")?;
        if total > 512 * 1024 * 1024 {
            return Err("Photoshop retained sources exceed 512 MiB".into());
        }
        files.push(LinkedFile {
            id,
            name,
            bytes: source,
            external,
        });
        let pad = (4 - n % 4) % 4;
        if pad <= r.remaining() {
            r.take(pad)?;
        }
    }
    Ok(files)
}

fn matrix(layer: &Layer) -> [f32; 6] {
    let t = crate::compositor::layer_pixel_transform(layer);
    [t.sx, t.ky, t.kx, t.sy, t.tx, t.ty]
}
fn transform(m: [f32; 6], x: f64, y: f64) -> (f64, f64) {
    (
        m[0] as f64 * x + m[2] as f64 * y + m[4] as f64,
        m[1] as f64 * x + m[3] as f64 * y + m[5] as f64,
    )
}
fn inverse(m: [f32; 6]) -> Result<[f32; 6], String> {
    let d = m[0] * m[3] - m[1] * m[2];
    if !d.is_finite() || d.abs() < 1e-12 {
        return Err("Smart-object placement is singular".into());
    }
    Ok([
        m[3] / d,
        -m[1] / d,
        -m[2] / d,
        m[0] / d,
        (m[2] * m[5] - m[3] * m[4]) / d,
        (m[1] * m[4] - m[0] * m[5]) / d,
    ])
}

/// Decode a standard Photoshop placed-layer record.
/// Args: tagged key/data, saved preview layer, optional source size and conversion notes.
/// Returns an editable placement model retaining all filter and warp descriptor fields.
pub fn decode(
    key: [u8; 4],
    bytes: &[u8],
    layer: &Layer,
    source_size: Option<(u32, u32)>,
    warnings: &mut Vec<String>,
) -> Result<SmartObject, String> {
    let mut r = Reader::new(bytes);
    let mut descriptor;
    if matches!(&key, b"SoLd" | b"SoLE") {
        if r.take(4)? != b"soLD" || !matches!(r.u32()?, 4 | 5) {
            return Err("Invalid Photoshop smart-object descriptor".into());
        }
        descriptor = descriptor::read_versioned(&mut r)?;
    } else {
        if r.take(4)? != b"plcL" || r.u32()? != 3 {
            return Err("Unsupported Photoshop placed-layer record".into());
        }
        let len = r.u8()? as usize;
        let id = String::from_utf8_lossy(r.take(len)?).to_string();
        r.take(16)?;
        let quad = (0..8)
            .map(|_| {
                r.take(8)
                    .map(|b| D::Double(f64::from_be_bytes(b.try_into().unwrap())))
            })
            .collect::<Result<Vec<_>, _>>()?;
        descriptor = Descriptor::new("null")
            .with("Idnt", D::Text(id.clone()))
            .with("placed", D::Text(id))
            .with("Type", D::Integer(2))
            .with("Trnf", D::List(quad));
        if r.remaining() >= 8 {
            r.take(4)?;
            if let Ok(warp) = descriptor::read_versioned(&mut r) {
                descriptor.set("warp", D::Object(warp));
            }
        }
    }
    let values = descriptor
        .get("nonAffineTransform")
        .or_else(|| descriptor.get("Trnf"))
        .and_then(D::list)
        .ok_or("Smart object has no placement quad")?;
    if values.len() != 8 {
        return Err("Invalid Photoshop placement quad".into());
    }
    let mut quad = [0.; 8];
    for (out, value) in quad.iter_mut().zip(values) {
        *out = value
            .number()
            .filter(|v| v.is_finite() && v.abs() <= 1e7)
            .ok_or("Invalid Photoshop placement coordinate")?;
    }
    let size = descriptor.get("Sz  ").and_then(D::object);
    let fallback = source_size.unwrap_or((
        layer.kind.pixels().map_or(1, |p| p.w),
        layer.kind.pixels().map_or(1, |p| p.h),
    ));
    let size = [
        size.map_or(fallback.0 as f64, |d| number(d, "Wdth", fallback.0 as f64)),
        size.map_or(fallback.1 as f64, |d| number(d, "Hght", fallback.1 as f64)),
    ];
    if size
        .iter()
        .any(|v| !v.is_finite() || *v <= 0. || *v > 300000.)
    {
        return Err("Invalid Photoshop smart-object source dimensions".into());
    }
    let smart = SmartObject {
        descriptor,
        quad,
        size,
        preview_transform: matrix(layer),
        external_link: None,
        edit_path: None,
        retained_blocks: Vec::new(),
    };
    for filter in smart.filters() {
        if filter.settings.is_none() {
            warn(
                warnings,
                format!(
                    "Smart filter '{}' is retained for Photoshop export. Its saved preview is used until that filter is disabled; native recomputation is unavailable.",
                    filter.name
                ),
            );
        }
    }
    if smart.has_warp() {
        warn(
            warnings,
            "Smart-object warp data is retained for Photoshop export. Native source replacement requires disabling the unsupported warp; the imported saved preview is retained.",
        );
    }
    Ok(smart)
}
impl SmartObject {
    /// Read the placement after current native move/scale/rotation/shear edits.
    /// Args: owning layer. Returns the transformed Photoshop quadrilateral.
    pub fn current_quad(&self, layer: &Layer) -> Result<[f64; 8], String> {
        let inverse = inverse(self.preview_transform)?;
        let current = matrix(layer);
        let mut out = [0.; 8];
        for i in 0..4 {
            let (x, y) = transform(inverse, self.quad[i * 2], self.quad[i * 2 + 1]);
            let (x, y) = transform(current, x, y);
            out[i * 2] = x;
            out[i * 2 + 1] = y;
        }
        Ok(out)
    }
    pub fn has_warp(&self) -> bool {
        if self.descriptor.get("quiltWarp").is_some() {
            return true;
        }
        let Some(warp) = self.descriptor.get("warp").and_then(D::object) else {
            return false;
        };
        match text(warp, "warpStyle").as_str() {
            "" | "warpNone" => false,
            "warpCustom" => !identity_mesh(warp),
            _ => true,
        }
    }
    /// Read editable smart-filter controls from the retained Photoshop descriptor.
    /// Args: none. Returns ordered filter controls; unsupported filters retain their full descriptors.
    pub fn filters(&self) -> Vec<SmartFilter> {
        let Some(stack) = self.descriptor.get("filterFX").and_then(D::object) else {
            return Vec::new();
        };
        let active = boolean(stack, "enab", true);
        stack
            .get("filterFXList")
            .and_then(D::list)
            .unwrap_or(&[])
            .iter()
            .enumerate()
            .filter_map(|(index, value)| {
                let d = value.object()?;
                let filter = d.get("Fltr").and_then(D::object);
                let class = filter.map_or("", |d| d.class.as_str());
                let id = number(d, "filterID", 0.) as i32;
                let mut settings = match class {
                    "GsnB" => Some(crate::raster::Settings::new(
                        crate::raster::Kind::GaussianBlur,
                    )),
                    "MtnB" => Some(crate::raster::Settings::new(
                        crate::raster::Kind::MotionBlur,
                    )),
                    "BrgC" => Some(crate::raster::Settings::new(
                        crate::raster::Kind::BrightnessContrast,
                    )),
                    "Invr" | "invert" => {
                        Some(crate::raster::Settings::new(crate::raster::Kind::Invert))
                    }
                    _ if id == i32::from_be_bytes(*b"Invr") => {
                        Some(crate::raster::Settings::new(crate::raster::Kind::Invert))
                    }
                    _ => None,
                };
                if let (Some(settings), Some(filter)) = (&mut settings, filter) {
                    match class {
                        "GsnB" => settings.values[0] = number(filter, "Rds ", 0.) as f32,
                        "MtnB" => {
                            settings.values[0] = number(filter, "Dstn", 0.) as f32;
                            settings.values[1] = number(filter, "Angl", 0.) as f32;
                        }
                        "BrgC" => {
                            settings.values[0] = number(filter, "Brgh", 0.) as f32 / 100.;
                            settings.values[1] = number(filter, "Cntr", 0.) as f32 / 100.;
                        }
                        _ => {}
                    }
                }
                let blend = d.get("blendOptions").and_then(D::object);
                let opacity = blend
                    .map_or(1., |b| (number(b, "Opct", 100.) / 100.) as f32)
                    .clamp(0., 1.);
                let key = blend
                    .map(|b| text(b, "Md  "))
                    .unwrap_or_else(|| "Nrml".into());
                let mode = match key.as_str() {
                    "Mltp" => Blend::Multiply,
                    "Scrn" => Blend::Screen,
                    "Ovrl" => Blend::Overlay,
                    "Dfrn" => Blend::Difference,
                    "Drkn" => Blend::Darken,
                    "Lghn" => Blend::Lighten,
                    "CDdg" => Blend::ColorDodge,
                    "CBrn" => Blend::ColorBurn,
                    "HrdL" => Blend::HardLight,
                    "SftL" => Blend::SoftLight,
                    "Xclu" => Blend::Exclusion,
                    "H   " => Blend::Hue,
                    "Strt" => Blend::Saturation,
                    "Clr " => Blend::Color,
                    "Lmns" => Blend::Luminosity,
                    "Nrml" | "normal" => Blend::Normal,
                    _ => {
                        settings = None;
                        Blend::Normal
                    }
                };
                Some(SmartFilter {
                    index,
                    name: text(d, "Nm  "),
                    enabled: active && boolean(d, "enab", true),
                    opacity,
                    blend: mode,
                    settings,
                })
            })
            .collect()
    }
    /// Update a supported smart filter without destroying original source pixels.
    /// Args: ordered filter index, enable flag and optional primary parameter. Returns an edited descriptor model.
    pub fn with_filter(
        &self,
        index: usize,
        enabled: bool,
        parameter: Option<f32>,
    ) -> Result<Self, String> {
        let mut out = self.clone();
        let stack =
            object_mut(&mut out.descriptor, "filterFX").ok_or("Smart object has no filters")?;
        if enabled {
            stack.set("enab", D::Bool(true));
        }
        let Some(D::List(list)) = stack
            .fields
            .iter_mut()
            .find(|(k, _)| k == "filterFXList")
            .map(|(_, v)| v)
        else {
            return Err("Smart object has no filter list".into());
        };
        let d = list
            .get_mut(index)
            .and_then(|v| if let D::Object(d) = v { Some(d) } else { None })
            .ok_or("Smart filter index is invalid")?;
        d.set("enab", D::Bool(enabled));
        if let Some(parameter) = parameter {
            if !parameter.is_finite() {
                return Err("Invalid smart filter parameter".into());
            }
            let filter =
                object_mut(d, "Fltr").ok_or("This filter has no editable native parameter")?;
            let key = match filter.class.as_str() {
                "GsnB" => "Rds ",
                "MtnB" => "Dstn",
                "BrgC" => "Brgh",
                _ => {
                    return Err(
                        "Smart filter parameters are retained but unsupported natively".into(),
                    );
                }
            };
            filter.set(
                key,
                if key == "Brgh" {
                    D::Double((parameter.clamp(-1., 1.) * 100.) as f64)
                } else {
                    D::Unit {
                        unit: *b"#Pxl",
                        value: parameter.clamp(0., 1000.) as f64,
                    }
                },
            );
        }
        Ok(out)
    }
}
fn identity_mesh(warp: &Descriptor) -> bool {
    if ["warpValue", "warpPerspective", "warpPerspectiveOther"]
        .iter()
        .any(|k| number(warp, k, 0.).abs() > 1e-9)
    {
        return false;
    }
    let (u, v) = (
        number(warp, "uOrder", 0.) as usize,
        number(warp, "vOrder", 0.) as usize,
    );
    if !(2..=64).contains(&u) || !(2..=64).contains(&v) {
        return false;
    }
    let Some(bounds) = warp.get("bounds").and_then(D::object) else {
        return false;
    };
    let Some(mesh) = warp.get("customEnvelopeWarp").and_then(D::object) else {
        return false;
    };
    let Some(D::ObjectArray { items, .. }) = mesh.get("meshPoints") else {
        return false;
    };
    if items.len() != 2 {
        return false;
    }
    for (axis, unit, values) in items {
        if unit != b"#Pxl" || values.len() != u * v {
            return false;
        }
        let (start, end, horizontal) = match axis.as_str() {
            "Hrzn" => (number(bounds, "Left", 0.), number(bounds, "Rght", 0.), true),
            "Vrtc" => (
                number(bounds, "Top ", 0.),
                number(bounds, "Btom", 0.),
                false,
            ),
            _ => return false,
        };
        for (i, &value) in values.iter().enumerate() {
            let t = if horizontal {
                (i % u) as f64 / (u - 1) as f64
            } else {
                (i / u) as f64 / (v - 1) as f64
            };
            if (value - (start + (end - start) * t)).abs() > 1e-6 {
                return false;
            }
        }
    }
    true
}

/// Scale an identity warp when a replacement source changes dimensions.
/// Args: retained placement and new source dimensions. Returns after updating its coordinate grid and size.
fn resize_identity_source(smart: &mut SmartObject, size: [f64; 2]) {
    let scale = [size[0] / smart.size[0], size[1] / smart.size[1]];
    if let Some(warp) = object_mut(&mut smart.descriptor, "warp")
        && identity_mesh(warp)
    {
        if let Some(bounds) = object_mut(warp, "bounds") {
            for (key, axis) in [("Left", 0), ("Rght", 0), ("Top ", 1), ("Btom", 1)] {
                if let Some(value) = bounds
                    .fields
                    .iter_mut()
                    .find(|(k, _)| k == key)
                    .map(|(_, v)| v)
                {
                    match value {
                        D::Double(v) | D::Unit { value: v, .. } => *v *= scale[axis],
                        D::UnitFloat { value, .. } => *value *= scale[axis] as f32,
                        _ => {
                            if let Some(v) = value.number() {
                                *value = D::Double(v * scale[axis]);
                            }
                        }
                    }
                }
            }
        }
        if let Some(mesh) = object_mut(warp, "customEnvelopeWarp")
            && let Some(D::ObjectArray { items, .. }) = mesh
                .fields
                .iter_mut()
                .find(|(k, _)| k == "meshPoints")
                .map(|(_, v)| v)
        {
            for (axis, _, values) in items {
                let factor = scale[usize::from(axis.as_str() == "Vrtc")];
                for v in values {
                    *v *= factor;
                }
            }
        }
    }
    smart.size = size;
}

fn object_mut<'a>(d: &'a mut Descriptor, key: &str) -> Option<&'a mut Descriptor> {
    match &mut d.fields.iter_mut().find(|(k, _)| k == key)?.1 {
        D::Object(d) => Some(d),
        _ => None,
    }
}

/// Open a retained source as editable native artwork.
/// Args: document and smart layer index. Returns a separate source document without changing the original attachment.
pub fn source_document(doc: &Document, index: usize) -> Result<Document, String> {
    let layer = doc
        .layers
        .get(index)
        .ok_or("Smart-object layer is missing")?;
    let id = layer
        .source_attachment
        .as_deref()
        .ok_or("This linked source is missing. Use Replace source to relink it.")?;
    let source = doc
        .source(id)
        .ok_or("Retained smart-object source is missing")?;
    source.validate()?;
    let bytes = source.bytes();
    if bytes.starts_with(b"8BPS") {
        return super::decode(bytes, &source.name).map(|p| p.document);
    }
    if source.format.eq_ignore_ascii_case("oma") {
        return crate::project::decode(std::str::from_utf8(bytes).map_err(|e| e.to_string())?);
    }
    if bytes.starts_with(b"%PDF-") {
        return crate::formats::pdf::read(bytes, &source.name).map(|p| p.0);
    }
    if source.format.eq_ignore_ascii_case("svg")
        || std::str::from_utf8(bytes.get(..bytes.len().min(512)).unwrap_or(bytes))
            .is_ok_and(|s| s.contains("<svg"))
    {
        return crate::formats::svg::read(
            std::str::from_utf8(bytes).map_err(|e| e.to_string())?,
            &source.name,
        )
        .map(|p| p.0);
    }
    let pixels = crate::formats::decode_image(bytes)?;
    let mut out = Document::new(&source.name, 1., 1., doc.dpi);
    out.width = pixels.w as f32;
    out.height = pixels.h as f32;
    out.transparent = true;
    out.artboards = crate::document::Artboard::tiled(1, out.width, out.height);
    if let Some(n) = &pixels.native {
        out.color = crate::deep_color::cms::DocumentColor {
            model: if n.cmyk.is_some() {
                crate::deep_color::cms::Model::Cmyk
            } else {
                crate::deep_color::cms::Model::Rgb
            },
            depth: n.depth,
            icc_profile: n.icc_profile.to_vec(),
            intent: n.intent,
        };
    }
    out.layers = vec![Layer::placed_raster(
        &source.name,
        pixels,
        Pt::ZERO,
        Pt::new(out.width, out.height),
    )];
    Ok(out)
}

fn apply_filters(mut source: Pixels, smart: &SmartObject) -> Result<Pixels, String> {
    if smart.has_warp() {
        return Err("This retained Photoshop warp cannot be recomputed natively; original source and saved preview remain intact".into());
    }
    if !smart.retained_blocks.is_empty() && smart.filters().iter().any(|f| f.enabled) {
        return Err("Photoshop smart-filter masks are retained with the saved preview. Disable the filters before native recomputation; export PSD to keep their original mask data.".into());
    }
    for filter in smart.filters().into_iter().filter(|f| f.enabled) {
        let settings=filter.settings.ok_or_else(||format!("Smart filter '{}' has no native renderer. Disable it to recompute; its original descriptor remains retained.",filter.name))?;
        let filtered = source.filtered(&settings, None)?;
        let a = source.samples()?;
        let b = filtered.samples()?;
        let samples = storage::collect(a
            .chunks_exact(4)
            .zip(b.chunks_exact(4))
            .flat_map(|(a, b)| {
                let mut b: [f32; 4] = b.try_into().unwrap();
                if filter.blend != Blend::Normal {
                    let mixed = crate::deep_color::blend_pixel(
                        [a[0], a[1], a[2], 1.],
                        [b[0], b[1], b[2], 1.],
                        filter.blend,
                        1.,
                    );
                    b[..3].copy_from_slice(&mixed[..3]);
                }
                let t = filter.opacity;
                let alpha = a[3] * (1. - t) + b[3] * t;
                let mut out = [0., 0., 0., alpha];
                for c in 0..3 {
                    out[c] = if alpha > 1e-12 {
                        (a[c] * a[3] * (1. - t) + b[c] * b[3] * t) / alpha
                    } else {
                        a[c]
                    };
                }
                out
            }))?;
        source.set_sample_allocation(samples)?;
    }
    Ok(source)
}
fn inverse_projective(quad: [f64; 8]) -> Result<[f64; 8], String> {
    let uv = [(0., 0.), (1., 0.), (1., 1.), (0., 1.)];
    let mut equations = [[0f64; 9]; 8];
    for i in 0..4 {
        let x = quad[i * 2];
        let y = quad[i * 2 + 1];
        let (u, v) = uv[i];
        equations[i * 2] = [x, y, 1., 0., 0., 0., -u * x, -u * y, u];
        equations[i * 2 + 1] = [0., 0., 0., x, y, 1., -v * x, -v * y, v];
    }
    for col in 0..8 {
        let pivot = (col..8)
            .max_by(|&a, &b| equations[a][col].abs().total_cmp(&equations[b][col].abs()))
            .unwrap();
        if equations[pivot][col].abs() < 1e-12 {
            return Err("Smart-object perspective transform is singular".into());
        }
        equations.swap(col, pivot);
        let div = equations[col][col];
        for v in &mut equations[col][col..] {
            *v /= div;
        }
        for row in 0..8 {
            if row == col {
                continue;
            }
            let factor = equations[row][col];
            for k in col..9 {
                equations[row][k] -= factor * equations[col][k];
            }
        }
    }
    let out = std::array::from_fn(|i| equations[i][8]);
    if out.iter().any(|v| !v.is_finite()) {
        return Err("Invalid smart-object perspective transform".into());
    }
    Ok(out)
}
fn placed_pixels(source: &Pixels, quad: [f64; 8]) -> Result<(Pixels, Pt), String> {
    if quad.iter().any(|v| !v.is_finite() || v.abs() > 1e7) {
        return Err("Smart-object transform exceeds supported coordinates".into());
    }
    let left = [quad[0], quad[2], quad[4], quad[6]]
        .into_iter()
        .fold(f64::INFINITY, f64::min)
        .floor();
    let top = [quad[1], quad[3], quad[5], quad[7]]
        .into_iter()
        .fold(f64::INFINITY, f64::min)
        .floor();
    let right = [quad[0], quad[2], quad[4], quad[6]]
        .into_iter()
        .fold(f64::NEG_INFINITY, f64::max)
        .ceil();
    let bottom = [quad[1], quad[3], quad[5], quad[7]]
        .into_iter()
        .fold(f64::NEG_INFINITY, f64::max)
        .ceil();
    let (w, h) = ((right - left) as u32, (bottom - top) as u32);
    let count = storage::count(w,h,4)?;
    let m = inverse_projective(quad)?;
    let mut samples = crate::resources::global().allocate_now::<f32,_>(count,&storage::check)?;
    for (i, p) in samples.as_mut_slice().chunks_exact_mut(4).enumerate() {
        let x = left + (i % w as usize) as f64 + 0.5;
        let y = top + (i / w as usize) as f64 + 0.5;
        let div = m[6] * x + m[7] * y + 1.;
        if div.abs() < 1e-12 {
            continue;
        }
        let u = (m[0] * x + m[1] * y + m[2]) / div;
        let v = (m[3] * x + m[4] * y + m[5]) / div;
        if !(0.0..1.0).contains(&u) || !(0.0..1.0).contains(&v) {
            continue;
        }
        p.copy_from_slice(&crate::deep_color::sample_at(
            source,
            (u * source.w as f64) as f32,
            (v * source.h as f64) as f32,
        ));
    }
    let mut placed = Pixels::from_sample_allocation(
        w,
        h,
        source
            .native
            .as_ref()
            .map_or(crate::deep_color::Depth::U8, |n| n.depth),
        Arc::new(samples),
    )?;
    if let Some(native) = &source.native {placed.native.as_mut().unwrap().intent = native.intent;}
    Ok((placed, Pt::new(left as f32, top as f32)))
}

/// Recompute a placed object from its retained source and current filter parameters.
/// Args: owning document, layer index and edited placement/filter model. Returns an undoable layer replacement.
pub fn refresh(doc: &Document, index: usize, mut smart: SmartObject) -> Result<Cmd, String> {
    let before = doc
        .layers
        .get(index)
        .ok_or("Smart-object layer is missing")?;
    let quad = smart.current_quad(before)?;
    let source = source_document(doc, index)?;
    resize_identity_source(&mut smart, [source.width as f64, source.height as f64]);
    let pixels = crate::deep_color::composite(&source, 1)?.in_document_color(
        &crate::deep_color::cms::DocumentColor {
            depth: crate::deep_color::Depth::F32,
            intent: source.color.intent,
            ..Default::default()
        },
    )?;
    let pixels = apply_filters(pixels, &smart)?;
    let (pixels, origin) = placed_pixels(&pixels, quad)?;
    let mut after = before.clone();
    if let Some(mask) = &before.mask {
        let inverse = inverse(matrix(before))?;
        let original = before
            .kind
            .pixels()
            .ok_or("Smart-object preview pixels are missing")?;
        let mut samples = crate::resources::global().allocate_now::<f32,_>(storage::count(pixels.w,pixels.h,4)?,&storage::check)?;
        for y in 0..pixels.h {
            for x in 0..pixels.w {
                let offset = (y as usize * pixels.w as usize + x as usize) * 4;
                let (x, y) = transform(
                    inverse,
                    origin.x as f64 + x as f64 + 0.5,
                    origin.y as f64 + y as f64 + 0.5,
                );
                let value = crate::deep_color::sample_at(
                    mask,
                    (x * mask.w as f64 / original.w as f64) as f32,
                    (y * mask.h as f64 / original.h as f64) as f32,
                );
                samples.as_mut_slice()[offset..offset+4].copy_from_slice(&value);
            }
        }
        let mut remapped = mask.reframed(pixels.w, pixels.h, 0, 0)?;
        remapped.set_sample_allocation(Arc::new(samples))?;
        after.mask = Some(remapped);
    }
    let size = Pt::new(pixels.w as f32, pixels.h as f32);
    after.kind = LayerKind::Raster {
        pixels,
        origin,
        size,
        rotation: 0.,
        shear: 0.,
    };
    smart.quad = quad;
    smart.preview_transform = matrix(&after);
    after.psd_smart = Some(smart);
    Ok(Cmd::SetSmartLayer {
        index,
        before: Box::new(before.clone()),
        after: Box::new(after),
    })
}

/// Replace/relink a smart source and recompute its non-destructive preview.
/// Args: document, layer index, original filename and immutable file bytes. Returns one undoable batch retaining both originals.
pub fn replace_source(
    doc: &Document,
    index: usize,
    name: &str,
    bytes: Vec<u8>,
) -> Result<Cmd, String> {
    let before = doc
        .layers
        .get(index)
        .ok_or("Smart-object layer is missing")?;
    let mut smart = before
        .psd_smart
        .clone()
        .ok_or("Layer is not a smart object")?;
    let format = std::path::Path::new(name)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("data")
        .to_ascii_lowercase();
    let source = SourceAttachment::new(name, &format, bytes)?;
    source.validate()?;
    let mut prepared = doc.clone();
    let id = prepared.retain_source(source);
    if prepared
        .sources
        .iter()
        .map(|s| s.bytes().len())
        .sum::<usize>()
        > 512 * 1024 * 1024
    {
        return Err("Retained sources exceed 512 MiB".into());
    }
    prepared.layers[index].source_attachment = Some(id.clone());
    smart.descriptor.set("Idnt", D::Text(id));
    smart.external_link = None;
    prepared.layers[index].psd_smart = Some(smart.clone());
    let mut command = refresh(&prepared, index, smart)?;
    if let Cmd::SetSmartLayer { before: old, .. } = &mut command {
        *old = Box::new(before.clone());
    }
    Ok(Cmd::Batch(vec![
        Cmd::SetSources {
            before: doc.sources.clone(),
            after: prepared.sources,
        },
        command,
    ]))
}

/// Write editable Photoshop placement and smart-filter metadata.
/// Args: owning layer. Returns a standard SoLd block reflecting native placement edits.
pub fn encode(layer: &Layer) -> Result<Vec<u8>, String> {
    let smart = layer
        .psd_smart
        .as_ref()
        .ok_or("Missing smart-object metadata")?;
    let quad = smart.current_quad(layer)?;
    let mut d = smart.descriptor.clone();
    d.set("Trnf", D::List(quad.into_iter().map(D::Double).collect()));
    d.set(
        "nonAffineTransform",
        D::List(quad.into_iter().map(D::Double).collect()),
    );
    d.set(
        "Sz  ",
        D::Object(
            Descriptor::new("Pnt ")
                .with("Wdth", D::Double(smart.size[0]))
                .with("Hght", D::Double(smart.size[1])),
        ),
    );
    for (key, value) in [
        ("Type", 2),
        ("PgNm", 1),
        ("totalPages", 1),
        ("Annt", 16),
        ("frameCount", 1),
    ] {
        if d.get(key).is_none() {
            d.set(key, D::Integer(value));
        }
    }
    if d.get("placed").is_none() {
        d.set("placed", D::Text(text(&d, "Idnt")));
    }
    if d.get("frameStep").is_none() {
        d.set(
            "frameStep",
            D::Object(
                Descriptor::new("Rtnl")
                    .with("numerator", D::Integer(0))
                    .with("denominator", D::Integer(600)),
            ),
        );
    }
    if d.get("duration").is_none() {
        d.set(
            "duration",
            D::Object(
                Descriptor::new("Rtnl")
                    .with("numerator", D::Integer(0))
                    .with("denominator", D::Integer(600)),
            ),
        );
    }
    if d.get("Rslt").is_none() {
        d.set(
            "Rslt",
            D::Unit {
                unit: *b"#Rsl",
                value: 72.,
            },
        );
    }
    if d.get("warp").is_none() {
        d.set(
            "warp",
            D::Object(
                Descriptor::new("warp")
                    .with(
                        "warpStyle",
                        D::Enum {
                            class: "warpStyle".into(),
                            value: "warpNone".into(),
                        },
                    )
                    .with("warpValue", D::Double(0.))
                    .with("warpPerspective", D::Double(0.))
                    .with("warpPerspectiveOther", D::Double(0.))
                    .with(
                        "warpRotate",
                        D::Enum {
                            class: "Ornt".into(),
                            value: "Hrzn".into(),
                        },
                    ),
            ),
        );
    }
    let mut out = b"soLD".to_vec();
    out.extend_from_slice(&4u32.to_be_bytes());
    descriptor::write_versioned(&mut out, &d);
    Ok(out)
}
fn write_unicode(out: &mut Vec<u8>, value: &str) {
    let v: Vec<u16> = value.encode_utf16().chain(std::iter::once(0)).collect();
    out.extend_from_slice(&(v.len() as u32).to_be_bytes());
    for v in v {
        out.extend_from_slice(&v.to_be_bytes());
    }
}

/// Encode retained originals without an unadmitted full-source intermediate copy.
/// Args: document with validated source references. Returns admitted embedded and external Photoshop link blocks.
pub(super) fn write_links(doc: &Document) -> Result<(Arc<crate::resources::Allocation<u8>>, Arc<crate::resources::Allocation<u8>>), String> {
    let mut embedded = storage::Builder::new()?;
    let mut external = storage::Builder::new()?;
    let mut used = std::collections::HashMap::new();
    for layer in &doc.layers {
        let Some(smart) = &layer.psd_smart else {
            continue;
        };
        let id = text(&smart.descriptor, "Idnt");
        let source = layer
            .source_attachment
            .as_deref()
            .map(|id| {
                doc.source(id)
                    .ok_or("Smart-object original source is missing")
            })
            .transpose()?;
        if let Some(source) = source {
            source.validate()?;
        }
        let source_id = source.map(|s| s.id.clone());
        if let Some(old) = used.insert(id.clone(), source_id.clone()) {
            if old != source_id {
                return Err(
                    "Two smart objects share a Photoshop ID but contain different sources".into(),
                );
            }
            continue;
        }
        if id.len() > 255 {
            return Err("Photoshop source ID is too long".into());
        }
        let linked = smart.external_link.is_some();
        let mut item = if linked {
            b"liFE".to_vec()
        } else {
            b"liFD".to_vec()
        };
        item.extend_from_slice(&2u32.to_be_bytes());
        item.push(id.len() as u8);
        item.extend_from_slice(id.as_bytes());
        let name = source
            .map(|s| s.name.as_str())
            .unwrap_or("Missing linked source");
        write_unicode(&mut item, name);
        let file_type = match source.map(|s| s.format.as_str()).unwrap_or("") {
            "png" => *b"PNGf",
            "psd" | "psb" => *b"8BPS",
            "jpg" | "jpeg" => *b"JPEG",
            "tif" | "tiff" => *b"TIFF",
            "svg" => *b"SVG ",
            "pdf" | "ai" => *b"PDF ",
            _ => *b"    ",
        };
        item.extend_from_slice(&file_type);
        item.extend_from_slice(b"8BIM");
        let size = source.map_or(0, |s| s.bytes().len()) as u64;
        item.extend_from_slice(&size.to_be_bytes());
        item.push(0);
        if let Some(link) = &smart.external_link {
            descriptor::write_versioned(&mut item, link);
            item.extend_from_slice(&size.to_be_bytes());
        }
        let item_len = item.len().checked_add(usize::try_from(size).map_err(|_| "Photoshop source length overflows")?).ok_or("Photoshop link length overflows")?;
        let out = if linked { &mut external } else { &mut embedded };
        out.extend(&(item_len as u64).to_be_bytes())?;
        out.extend(&item)?;
        if let Some(source) = source {out.extend(source.bytes())?;}
        out.extend(&[0;3][..(4-out.len()%4)%4])?;
    }
    Ok((embedded.finish()?, external.finish()?))
}

/// Wrap existing raster pixels as a non-destructive placed source.
/// Args: document and raster layer index. Returns one undoable attachment and metadata edit, preserving the current appearance.
pub fn make_smart(doc: &Document, index: usize) -> Result<Cmd, String> {
    let before = doc.layers.get(index).ok_or("Layer is missing")?;
    if before.psd_smart.is_some() {
        return Err("Layer is already a smart object".into());
    }
    let pixels = before.kind.pixels().ok_or("Choose a raster layer")?;
    let mut source = Document::new(&before.name, 1., 1., doc.dpi);
    source.width = pixels.w as f32;
    source.height = pixels.h as f32;
    source.transparent = true;
    source.color = doc.color.clone();
    source.artboards = crate::document::Artboard::tiled(1, source.width, source.height);
    source.layers = vec![Layer::placed_raster(
        &before.name,
        pixels.clone(),
        Pt::ZERO,
        Pt::new(source.width, source.height),
    )];
    let bytes = super::encode(&source, false)?.bytes;
    let attachment = SourceAttachment::new(&format!("{}.psd", before.name), "psd", bytes)?;
    let id = attachment.id.clone();
    let mut prepared = doc.clone();
    prepared.retain_source(attachment);
    let m = matrix(before);
    let mut quad = [0.; 8];
    for (i, (x, y)) in [
        (0., 0.),
        (pixels.w as f64, 0.),
        (pixels.w as f64, pixels.h as f64),
        (0., pixels.h as f64),
    ]
    .into_iter()
    .enumerate()
    {
        let (x, y) = transform(m, x, y);
        quad[i * 2] = x;
        quad[i * 2 + 1] = y;
    }
    let d = Descriptor::new("null")
        .with("Idnt", D::Text(id.clone()))
        .with("placed", D::Text(id.clone()))
        .with("Type", D::Integer(2));
    let mut after = before.clone();
    after.source_attachment = Some(id);
    after.psd_smart = Some(SmartObject {
        descriptor: d,
        quad,
        size: [pixels.w as f64, pixels.h as f64],
        preview_transform: m,
        external_link: None,
        edit_path: None,
        retained_blocks: Vec::new(),
    });
    Ok(Cmd::Batch(vec![
        Cmd::SetSources {
            before: doc.sources.clone(),
            after: prepared.sources,
        },
        Cmd::SetSmartLayer {
            index,
            before: Box::new(before.clone()),
            after: Box::new(after),
        },
    ]))
}
impl SmartObject {
    /// Add an editable Gaussian smart filter.
    /// Args: radius in source pixels. Returns a new model retaining all existing filters.
    pub fn with_blur(&self, radius: f32) -> Result<Self, String> {
        if !radius.is_finite() {
            return Err("Invalid smart blur radius".into());
        }
        let mut out = self.clone();
        if object_mut(&mut out.descriptor, "filterFX").is_none() {
            out.descriptor.set(
                "filterFX",
                D::Object(
                    Descriptor::new("filterFXStyle")
                        .with("enab", D::Bool(true))
                        .with("validAtPosition", D::Bool(true))
                        .with("filterMaskEnable", D::Bool(true))
                        .with("filterMaskLinked", D::Bool(false))
                        .with("filterMaskExtendWithWhite", D::Bool(true))
                        .with("filterFXList", D::List(Vec::new())),
                ),
            );
        }
        let stack = object_mut(&mut out.descriptor, "filterFX").unwrap();
        let Some(D::List(list)) = stack
            .fields
            .iter_mut()
            .find(|(k, _)| k == "filterFXList")
            .map(|(_, v)| v)
        else {
            return Err("Malformed smart filter list".into());
        };
        let color = |value: f64| {
            D::Object(
                Descriptor::new("RGBC")
                    .with("Rd  ", D::Double(value))
                    .with("Grn ", D::Double(value))
                    .with("Bl  ", D::Double(value)),
            )
        };
        list.push(D::Object(
            Descriptor::new("filterFX")
                .with("Nm  ", D::Text("Gaussian Blur".into()))
                .with("enab", D::Bool(true))
                .with("hasoptions", D::Bool(true))
                .with("FrgC", color(0.))
                .with("BckC", color(255.))
                .with(
                    "blendOptions",
                    D::Object(
                        Descriptor::new("blendOptions")
                            .with(
                                "Opct",
                                D::Unit {
                                    unit: *b"#Prc",
                                    value: 100.,
                                },
                            )
                            .with(
                                "Md  ",
                                D::Enum {
                                    class: "BlnM".into(),
                                    value: "Nrml".into(),
                                },
                            ),
                    ),
                )
                .with(
                    "Fltr",
                    D::Object(Descriptor::new("GsnB").with(
                        "Rds ",
                        D::Unit {
                            unit: *b"#Pxl",
                            value: radius.clamp(0., 1000.) as f64,
                        },
                    )),
                )
                .with("filterID", D::Integer(i32::from_be_bytes(*b"GsnB"))),
        ));
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const FIXTURE: &[u8] = include_bytes!("fixtures/independent-smart.psd");
    #[test]
    fn independent_smart_source_transform_and_filters_survive_edit_save_and_export() {
        let result = crate::formats::psd::decode(FIXTURE, "Independent smart").unwrap();
        let mut doc = result.document;
        assert!(
            !result
                .warnings
                .iter()
                .any(|n| n.contains("could not be read")),
            "{:?}",
            result.warnings
        );
        let layer = &doc.layers[0];
        let original = doc
            .source(layer.source_attachment.as_deref().unwrap())
            .unwrap()
            .clone();
        assert!(original.bytes().starts_with(b"\x89PNG"));
        let smart = layer.psd_smart.as_ref().unwrap().clone();
        assert_eq!(smart.quad, [0., 0., 8., 0., 7., 8., 0., 7.]);
        assert!(
            !smart.has_warp(),
            "Independent identity mesh must remain recomputable"
        );
        assert_eq!(smart.filters().len(), 1);
        assert_eq!(
            smart.filters()[0].settings.as_ref().unwrap().values[0],
            0.75
        );
        doc = crate::project::decode(&crate::project::encode(&doc).unwrap()).unwrap();
        assert_eq!(doc.sources[0].bytes(), original.bytes());
        let smart = doc.layers[0].psd_smart.clone().unwrap();
        let command = refresh(&doc, 0, smart.with_filter(0, true, Some(0.)).unwrap()).unwrap();
        crate::document::apply(&mut doc, &command);
        let sharp = doc.layers[0].kind.pixels().unwrap().samples().unwrap();
        let smart = doc.layers[0].psd_smart.clone().unwrap();
        let command2 = refresh(&doc, 0, smart.with_filter(0, true, Some(1.5)).unwrap()).unwrap();
        crate::document::apply(&mut doc, &command2);
        assert_ne!(sharp, doc.layers[0].kind.pixels().unwrap().samples().unwrap());
        assert_eq!(
            doc.source(doc.layers[0].source_attachment.as_deref().unwrap())
                .unwrap()
                .bytes(),
            original.bytes()
        );
        if let LayerKind::Raster { origin, .. } = &mut doc.layers[0].kind {
            *origin = *origin + Pt::new(2., 3.);
        }
        let expected = doc.layers[0]
            .psd_smart
            .as_ref()
            .unwrap()
            .current_quad(&doc.layers[0])
            .unwrap();
        for large in [false, true] {
            let out = super::super::encode(&doc, large).unwrap();
            let reopened = super::super::decode(&out.bytes, "Reopened")
                .unwrap()
                .document;
            let layer = &reopened.layers[0];
            let smart = layer.psd_smart.as_ref().unwrap();
            assert_eq!(smart.quad, expected);
            assert_eq!(smart.filters()[0].settings.as_ref().unwrap().values[0], 1.5);
            assert_eq!(
                reopened
                    .source(layer.source_attachment.as_deref().unwrap())
                    .unwrap()
                    .bytes(),
                original.bytes()
            );
            if !large && let Ok(dir) = std::env::var("OMADESIGN_DEEP_QA_DIR") {
                std::fs::write(
                    std::path::Path::new(&dir).join("editable-smart.psd"),
                    out.bytes,
                )
                .unwrap();
            }
        }
        crate::document::apply(&mut doc, &crate::document::invert_cmd(command2));
        assert_eq!(doc.layers[0].kind.pixels().unwrap().samples().unwrap(), sharp);
    }
    #[test]
    fn retained_filter_masks_and_unknown_blends_refuse_lossy_recompute() {
        let mut doc = super::super::decode(FIXTURE, "Filter masks")
            .unwrap()
            .document;
        let smart = doc.layers[0].psd_smart.as_mut().unwrap();
        smart.retained_blocks.push((*b"FMsk", vec![0; 12]));
        assert!(
            refresh(&doc, 0, doc.layers[0].psd_smart.clone().unwrap())
                .err()
                .unwrap()
                .contains("smart-filter masks")
        );
        let encoded = crate::project::encode(&doc).unwrap();
        doc = crate::project::decode(&encoded).unwrap();
        let out = super::super::encode(&doc, false).unwrap();
        let imported = super::super::decode(&out.bytes, "Retained mask").unwrap();
        assert!(
            imported
                .warnings
                .iter()
                .any(|n| n.contains("smart-filter masks"))
        );
        assert_eq!(
            imported.document.layers[0]
                .psd_smart
                .as_ref()
                .unwrap()
                .retained_blocks,
            vec![(*b"FMsk", vec![0; 12])]
        );
        let mut smart = doc.layers[0].psd_smart.clone().unwrap();
        smart.retained_blocks.clear();
        let stack = object_mut(&mut smart.descriptor, "filterFX").unwrap();
        let D::List(list) = &mut stack
            .fields
            .iter_mut()
            .find(|(k, _)| k == "filterFXList")
            .unwrap()
            .1
        else {
            panic!()
        };
        let D::Object(filter) = &mut list[0] else {
            panic!()
        };
        object_mut(filter, "blendOptions").unwrap().set(
            "Md  ",
            D::Enum {
                class: "BlnM".into(),
                value: "unknownMode".into(),
            },
        );
        assert!(smart.filters()[0].settings.is_none());
        assert!(refresh(&doc, 0, smart).is_err());
    }

    #[test]
    fn transformed_native_mask_keeps_consistent_authoritative_grids_on_refresh() {
        let mut doc = super::super::decode(FIXTURE, "Native placed mask")
            .unwrap()
            .document;
        let profile = include_bytes!("fixtures/fogra39l-cc0.icc").to_vec();
        doc.layers[0].mask = Some(
            Pixels::from_cmyk(
                8,
                8,
                crate::deep_color::Depth::U16,
                [0.1, 0.2, 0.3, 0.4, 1.].repeat(64),
                profile.clone(),
                1,
            )
            .unwrap(),
        );
        if let LayerKind::Raster { size, rotation, .. } = &mut doc.layers[0].kind {
            *size = Pt::new(12., 10.);
            *rotation = 0.25;
        }
        let command = refresh(&doc, 0, doc.layers[0].psd_smart.clone().unwrap()).unwrap();
        crate::document::apply(&mut doc, &command);
        let mask = doc.layers[0].mask.as_ref().unwrap();
        mask.validate_native().unwrap();
        assert_eq!(
            (mask.w, mask.h),
            (
                doc.layers[0].kind.pixels().unwrap().w,
                doc.layers[0].kind.pixels().unwrap().h
            )
        );
        assert_eq!(mask.native.as_ref().unwrap().icc_profile.as_slice(), profile.as_slice());
        let saved = crate::project::decode(&crate::project::encode(&doc).unwrap()).unwrap();
        assert_eq!(saved.layers[0].mask.as_ref().unwrap().native, mask.native);
    }

    #[test]
    fn placed_cmyk_source_retains_its_rendering_intent_and_native_color() {
        use crate::deep_color::{Depth,cms::{DocumentColor,Model}};
        let profile = include_bytes!("fixtures/fogra39l-cc0.icc").to_vec();
        let original = Pixels::from_cmyk(1,1,Depth::F32,vec![1.,0.,0.,0.,1.],profile.clone(),1).unwrap();
        let expected = original.sample(0);
        let mut source = Document::new("CMYK source",1.,1.,72.);
        source.transparent = true;
        source.color = DocumentColor {model:Model::Cmyk,depth:Depth::F32,icc_profile:profile,intent:1};
        source.layers=vec![Layer::placed_raster("Source",original,Pt::ZERO,Pt::new(1.,1.))];
        let bytes=crate::project::encode(&source).unwrap().into_bytes();
        let mut doc=super::super::decode(FIXTURE,"Placed CMYK").unwrap().document;
        doc.layers[0].psd_smart=Some(doc.layers[0].psd_smart.as_ref().unwrap().with_filter(0,false,None).unwrap());
        let command=replace_source(&doc,0,"source.oma",bytes.clone()).unwrap();
        crate::document::apply(&mut doc,&command);
        let placed=doc.layers[0].kind.pixels().unwrap();
        let native=placed.native.as_ref().unwrap();
        assert_eq!(native.intent,1);
        assert!(native.cmyk.is_none() && native.icc_profile.is_empty());
        let center=placed.sample((placed.h as usize/2)*placed.w as usize+placed.w as usize/2);
        assert!(center[3]>0.);
        for c in 0..3 {assert!((center[c]-expected[c]).abs()<0.00001,"{center:?} != {expected:?}");}
        assert_eq!(doc.source(doc.layers[0].source_attachment.as_deref().unwrap()).unwrap().bytes(),bytes);
    }

    #[test]
    fn source_replacement_is_undoable_and_old_original_stays_recoverable() {
        let mut doc = super::super::decode(FIXTURE, "Replace").unwrap().document;
        let old = doc.layers[0].source_attachment.clone();
        let quad = doc.layers[0]
            .psd_smart
            .as_ref()
            .unwrap()
            .current_quad(&doc.layers[0])
            .unwrap();
        let mut source = source_document(&doc, 0).unwrap();
        let pixels = source.layers[0].kind.pixels_mut().unwrap();
        let mut values = pixels.copy_samples(&storage::check).unwrap();
        for p in values.as_mut_slice().chunks_exact_mut(4) {
            p[0] = 0.875;
        }
        pixels.set_sample_allocation(Arc::new(values)).unwrap();
        *pixels = pixels.reframed(8, 4, 0, 0).unwrap();
        source.width = 8.;
        source.height = 4.;
        source.artboards = crate::document::Artboard::tiled(1, 8., 4.);
        if let LayerKind::Raster { size, .. } = &mut source.layers[0].kind {
            *size = Pt::new(8., 4.);
        }
        let bytes = super::super::encode(&source, false).unwrap().bytes;
        let command = replace_source(&doc, 0, "edited.psd", bytes.clone()).unwrap();
        crate::document::apply(&mut doc, &command);
        assert_ne!(doc.layers[0].source_attachment, old);
        assert_eq!(doc.layers[0].psd_smart.as_ref().unwrap().size, [8., 4.]);
        assert!(!doc.layers[0].psd_smart.as_ref().unwrap().has_warp());
        let output = super::super::encode(&doc, false).unwrap();
        let exported = super::super::decode(&output.bytes, "Relinked")
            .unwrap()
            .document;
        assert_eq!(
            exported.layers[0].psd_smart.as_ref().unwrap().size,
            [8., 4.]
        );
        assert!(!exported.layers[0].psd_smart.as_ref().unwrap().has_warp());
        if let Ok(dir) = std::env::var("OMADESIGN_DEEP_QA_DIR") {
            std::fs::write(
                std::path::Path::new(&dir).join("replaced-smart.psd"),
                output.bytes,
            )
            .unwrap();
        }
        assert_eq!(
            doc.layers[0]
                .psd_smart
                .as_ref()
                .unwrap()
                .current_quad(&doc.layers[0])
                .unwrap(),
            quad
        );
        assert!(doc.source(old.as_deref().unwrap()).is_some());
        assert_eq!(
            doc.source(doc.layers[0].source_attachment.as_deref().unwrap())
                .unwrap()
                .bytes(),
            bytes
        );
        crate::document::apply(&mut doc, &crate::document::invert_cmd(command.clone()));
        assert_eq!(doc.layers[0].source_attachment, old);
        crate::document::apply(&mut doc, &command);
        let saved = crate::project::decode(&crate::project::encode(&doc).unwrap()).unwrap();
        assert_eq!(saved.sources.len(), 2);
    }
    #[test]
    fn unsupported_filter_and_missing_external_source_keep_explicit_fallback() {
        let mut doc = super::super::decode(FIXTURE, "Fallback").unwrap().document;
        let mut smart = doc.layers[0].psd_smart.clone().unwrap();
        let stack = object_mut(&mut smart.descriptor, "filterFX").unwrap();
        let D::List(list) = &mut stack
            .fields
            .iter_mut()
            .find(|(k, _)| k == "filterFXList")
            .unwrap()
            .1
        else {
            panic!()
        };
        let D::Object(filter) = &mut list[0] else {
            panic!()
        };
        filter.set(
            "Fltr",
            D::Object(
                Descriptor::new("ThirdPartyFilter").with("VendorParameter", D::Double(0.1234567)),
            ),
        );
        assert!(
            refresh(&doc, 0, smart.clone())
                .unwrap_err()
                .contains("no native renderer")
        );
        smart.external_link = Some(
            Descriptor::new("ExternalFileLink")
                .with("fullPath", D::Text("missing/source.png".into()))
                .with("relPath", D::Text("source.png".into())),
        );
        doc.layers[0].psd_smart = Some(smart);
        doc.layers[0].source_attachment = None;
        let out = super::super::encode(&doc, false).unwrap();
        let loaded = super::super::decode(&out.bytes, "Missing").unwrap();
        assert!(
            loaded
                .warnings
                .iter()
                .any(|n| n.contains("no embedded source"))
        );
        assert!(loaded.document.layers[0].source_attachment.is_none());
        assert!(loaded.document.sources.is_empty());
        assert!(
            loaded.document.layers[0]
                .psd_smart
                .as_ref()
                .unwrap()
                .external_link
                .is_some()
        );
    }
}
