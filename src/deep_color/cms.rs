//! Profile conversion through Little CMS 2. Authored samples are never replaced by display bytes.
use super::Depth;
use super::storage;
use crate::resources::{self, Allocation};
use serde::{Deserialize, Serialize};
use std::{ffi::c_void, sync::Arc};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Model {
    #[default]
    Rgb,
    Cmyk,
}
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DocumentColor {
    pub model: Model,
    pub depth: Depth,
    pub icc_profile: Vec<u8>,
    pub intent: u32,
}

/// Check an embedded profile before native transformation.
/// Args: ICC bytes and expected authoring model. Returns an error for mismatched or malformed data.
pub fn validate_profile(profile: &[u8], model: Model) -> Result<(), String> {
    if profile.is_empty() {
        return Ok(());
    }
    if profile.len() < 128
        || profile.len() > 16 * 1024 * 1024
        || profile.get(36..40) != Some(b"acsp")
        || u32::from_be_bytes(profile[..4].try_into().unwrap()) as usize != profile.len()
        || profile.get(16..20)
            != Some(match model {
                Model::Rgb => b"RGB ",
                Model::Cmyk => b"CMYK",
            })
    {
        return Err(
            "Embedded ICC profile is invalid or does not match the document color model".into(),
        );
    }
    Ok(())
}

/// Convert authored RGB/CMYK color components using an embedded ICC profile.
/// Args: interleaved color samples without alpha, source model, profile, direction and intent.
/// Returns transformed color components; untagged CMYK uses an explicitly uncalibrated device formula.
pub fn transform(
    samples: &[f32],
    model: Model,
    profile: &[u8],
    to_display: bool,
    intent: u32,
) -> Result<Arc<Allocation<f32>>, String> {
    transform_components(samples, model, profile, to_display, intent, false)
}

/// Transform authored RGBA or CMYKA directly, preserving straight alpha.
/// Args: interleaved color and alpha, profile, direction and intent. Returns admitted target color plus exact alpha.
pub fn transform_alpha(samples: &[f32], model: Model, profile: &[u8], to_display: bool, intent: u32) -> Result<Arc<Allocation<f32>>, String> {
    transform_components(samples, model, profile, to_display, intent, true)
}

fn transform_components(samples: &[f32], model: Model, profile: &[u8], to_display: bool, intent: u32, alpha: bool) -> Result<Arc<Allocation<f32>>, String> {
    validate_profile(profile, model)?;
    let native = if model == Model::Cmyk {4} else {3};
    let input = if to_display {native} else {3};
    let output = if to_display {3} else {native};
    let input_stride = input + usize::from(alpha);
    let output_stride = output + usize::from(alpha);
    if samples.len() % input_stride != 0 || samples.iter().any(|v| !v.is_finite()) || intent > 3 {return Err("Invalid ICC samples or rendering intent".into());}
    let count = (samples.len() / input_stride).checked_mul(output_stride).ok_or("ICC output dimensions overflow")?;
    let mut converted = resources::global().allocate_now::<f32,_>(count, &storage::check)?;
    if profile.is_empty() {
        for (src,dst) in samples.chunks_exact(input_stride).zip(converted.as_mut_slice().chunks_exact_mut(output_stride)) {
            if model == Model::Rgb {dst[..output].copy_from_slice(&src[..input]);}
            else if to_display {for c in 0..3 {dst[c] = (1. - src[c]) * (1. - src[3]);}}
            else {
                let k = 1. - src[0].max(src[1]).max(src[2]).clamp(0.,1.);
                let d = 1. - k;
                if d < 1e-8 {dst[..4].copy_from_slice(&[0.,0.,0.,1.]);}
                else {for c in 0..3 {dst[c] = (1. - src[c] - k) / d;}dst[3] = k;}
            }
            if alpha {dst[output] = src[input];}
        }
        return Ok(Arc::new(converted));
    }
    let _engine_memory = resources::global().reserve_now(64 * 1024 * 1024, &storage::check)?;
    let api = Api::load()?;
    unsafe {
        let original = (api.open)(profile.as_ptr().cast(), profile.len() as u32);
        let srgb = (api.srgb)();
        if original.is_null() || srgb.is_null() {
            if !original.is_null() {(api.close)(original);}
            if !srgb.is_null() {(api.close)(srgb);}
            return Err("Little CMS rejected the embedded ICC profile".into());
        }
        let format = |channels:usize| (1u32 << 22) | ((if channels == 4 {6} else {4}) << 16) | ((u32::from(alpha)) << 7) | ((channels as u32) << 3) | 4;
        let (src,dst) = if to_display {(original,srgb)} else {(srgb,original)};
        let flags = 0x0100 | if alpha {0x04000000} else {0};
        let transform = (api.create)(src,format(input),dst,format(output),intent,flags);
        if transform.is_null() {(api.close)(original);(api.close)(srgb);return Err("The ICC profile lacks the requested color transform".into());}
        let mut scaled = [0f32;5 * 1024];
        for (src,dst) in samples.chunks(1024 * input_stride).zip(converted.as_mut_slice().chunks_mut(1024 * output_stride)) {
            let source = if input == 4 {
                scaled[..src.len()].copy_from_slice(src);
                for pixel in scaled[..src.len()].chunks_exact_mut(input_stride) {for c in &mut pixel[..4] {*c *= 100.;}}
                &scaled[..src.len()]
            } else {src};
            (api.apply)(transform,source.as_ptr().cast(),dst.as_mut_ptr().cast(),(src.len() / input_stride) as u32);
            if output == 4 {for pixel in dst.chunks_exact_mut(output_stride) {for c in &mut pixel[..4] {*c /= 100.;}}}
        }
        (api.delete)(transform);(api.close)(original);(api.close)(srgb);
    }
    if converted.iter().any(|v| !v.is_finite()) {return Err("ICC transform produced nonfinite samples".into());}
    Ok(Arc::new(converted))
}

type Handle = *mut c_void;
struct Api {
    open: unsafe extern "C" fn(*const c_void, u32) -> Handle,
    srgb: unsafe extern "C" fn() -> Handle,
    close: unsafe extern "C" fn(Handle) -> i32,
    create: unsafe extern "C" fn(Handle, u32, Handle, u32, u32, u32) -> Handle,
    apply: unsafe extern "C" fn(Handle, *const c_void, *mut c_void, u32),
    delete: unsafe extern "C" fn(Handle),
}
unsafe extern "C" {
    fn cmsOpenProfileFromMem(bytes: *const c_void, size: u32) -> Handle;
    fn cmsCreate_sRGBProfile() -> Handle;
    fn cmsCloseProfile(profile: Handle) -> i32;
    fn cmsCreateTransform(
        src: Handle,
        input: u32,
        dst: Handle,
        output: u32,
        intent: u32,
        flags: u32,
    ) -> Handle;
    fn cmsDoTransform(transform: Handle, input: *const c_void, output: *mut c_void, count: u32);
    fn cmsDeleteTransform(transform: Handle);
    fn cmsGetEncodedCMMversion() -> i32;
}

/// Check the statically packaged color engine.
/// Args: none. Returns its encoded version or an actionable package-integrity error.
pub fn runtime_probe() -> Result<i32, String> {
    let version = unsafe { cmsGetEncodedCMMversion() };
    if version != 2190 {
        return Err(format!(
            "Built-in Little CMS version {version} differs from pinned 2190. Reinstall the complete Omadesign package or rebuild from the pinned vendor sources."
        ));
    }
    Ok(version)
}
impl Api {
    fn load() -> Result<Self, String> {
        runtime_probe()?;
        Ok(Self {
            open: cmsOpenProfileFromMem,
            srgb: cmsCreate_sRGBProfile,
            close: cmsCloseProfile,
            create: cmsCreateTransform,
            apply: cmsDoTransform,
            delete: cmsDeleteTransform,
        })
    }
}

pub fn relative_intent() -> u32 {
    1
}

impl crate::document::Document {
    /// Prepare one undoable document color conversion.
    /// Args: target authoring mode, depth, ICC profile and intent. Returns commands for every raster and the document settings.
    pub fn color_conversion(&self, color: DocumentColor) -> Result<crate::document::Cmd, String> {
        validate_profile(&color.icc_profile, color.model)?;
        let mut commands = Vec::new();
        for (index, layer) in self.layers.iter().enumerate() {
            if let Some(before) = layer.kind.pixels() {
                let mut after = before.in_document_color(&color)?;
                if let Some(native) = &mut after.native {
                    native.depth = color.depth;
                }
                if color.depth == Depth::U8 {
                    if let Some(inks) = after.native.as_ref().and_then(|n| n.cmyk.as_ref()) {
                        after = crate::document::Pixels::from_cmyk_allocation(
                            after.w,
                            after.h,
                            Depth::U8,
                            storage::collect(inks.iter().map(|v| super::byte(*v) as f32 / 255.))?,
                            color.icc_profile.clone(),
                            color.intent,
                        )?;
                    } else {
                        after.set_sample_allocation(storage::collect(
                            after
                                .samples()?
                                .iter()
                                .copied()
                                .map(|v| super::byte(v) as f32 / 255.),
                        )?)?;
                    }
                }
                if color.depth == Depth::U8
                    && color.model == Model::Rgb
                    && color.icc_profile.is_empty()
                {
                    after =
                        crate::document::Pixels::from_byte_allocation(after.w,after.h,after.data.clone())?;
                }
                commands.push(crate::document::Cmd::NativePixels {
                    layer: index,
                    mask: false,
                    before: before.clone(),
                    after,
                });
            }
        }
        commands.push(crate::document::Cmd::SetDocumentColor {
            before: self.color.clone(),
            after: color,
        });
        Ok(crate::document::Cmd::Batch(commands))
    }
}
