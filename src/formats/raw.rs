//! Full sensor RAW decoding into oriented, camera-white-balanced linear sRGB.
//!
//! LibRaw performs black-level subtraction, demosaicing and the camera matrix.
//! Its 16-bit output deliberately has neither a display transfer function nor
//! automatic brightness. DNG BaselineExposure is reported separately; the Photo
//! pipeline applies it before display conversion. Neither thumbnails nor camera
//! JPEG previews are used. The source buffer is borrowed and never modified.

use std::{ffi::c_void, os::raw::c_char};

// These crates provide the statically built decompression libraries consumed by
// the C++ bridge. Referencing them retains their native link metadata.
use libz_sys as _;
use mozjpeg_sys as _;
use serde::{Deserialize, Serialize};

pub const VERSION: &str = "LibRaw 0.22.2";
pub const MAX_INPUT_BYTES: usize = 512 * 1024 * 1024;
pub const MAX_PIXELS: u64 = 64_000_000;
pub const EXTENSIONS: &[&str] = &[
    "3fr", "arw", "bay", "cap", "cr2", "cr3", "crw", "dcr", "dcs", "dng", "drf", "erf", "fff",
    "iiq", "k25", "kdc", "mdc", "mef", "mos", "mrw", "nef", "nrw", "orf", "pef", "ptx", "pxn",
    "raf", "raw", "rw2", "rwl", "rwz", "sr2", "srf", "srw", "sti", "x3f",
];

pub fn is_extension(extension: &str) -> bool {
    EXTENSIONS
        .iter()
        .any(|candidate| candidate.eq_ignore_ascii_case(extension))
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct RawMetadata {
    pub make: String,
    pub model: String,
    pub lens: String,
    pub iso: f32,
    /// Exposure time in seconds.
    pub shutter: f32,
    pub aperture: f32,
    /// Physical focal length in millimeters, when supplied by the camera.
    pub focal_length: f32,
    /// Original LibRaw flip code; the returned pixels are already oriented.
    pub orientation: i32,
    pub camera_wb: [f32; 4],
    /// DNG baseline exposure in stops, not yet applied to `pixels`.
    pub baseline_exposure: f32,
    /// Camera timestamp as parsed by LibRaw; no timezone is inferred.
    pub timestamp: i64,
}

#[derive(Clone, Debug)]
pub struct RawImage {
    pub width: u32,
    pub height: u32,
    /// Interleaved, oriented, linear sRGB. Every pixel has exactly 3 channels.
    pub pixels: Vec<u16>,
    pub metadata: RawMetadata,
    pub warnings: Vec<String>,
}

#[repr(C)]
struct NativeImage {
    owner: *mut c_void,
    data: *const u8,
    data_size: u64,
    width: u32,
    height: u32,
    colors: u32,
    bits: u32,
    warnings: u32,
    raw_count: u32,
    dng_opcodes: u32,
    orientation: i32,
    iso: f32,
    shutter: f32,
    aperture: f32,
    focal_length: f32,
    camera_wb: [f32; 4],
    baseline_exposure: f32,
    timestamp: i64,
    make: [c_char; 64],
    model: [c_char; 64],
    lens: [c_char; 128],
    error: [c_char; 256],
}

unsafe extern "C" {
    fn oma_raw_decode(bytes: *const u8, length: usize, image: *mut NativeImage) -> i32;
    fn oma_raw_free(owner: *mut c_void);
}

impl Drop for NativeImage {
    fn drop(&mut self) {
        // SAFETY: The bridge returns sole ownership of this allocation, or null.
        // `NativeImage` is never cloned or moved across the FFI boundary again.
        unsafe { oma_raw_free(self.owner) };
    }
}

pub fn decode(bytes: &[u8]) -> Result<RawImage, String> {
    if bytes.is_empty() || bytes.len() > MAX_INPUT_BYTES {
        return Err("RAW input must contain between 1 byte and 512 MiB".into());
    }
    // SAFETY: Every field is a plain number, fixed byte array, or nullable raw
    // pointer. All-zero initialization is valid, including the Drop owner.
    let mut native: NativeImage = unsafe { std::mem::zeroed() };
    // SAFETY: The borrowed byte buffer remains alive through synchronous decode.
    // The C++ adapter validates input/layout, catches exceptions and writes only
    // within the exactly matching repr(C) output. It never retains input bytes.
    let result = unsafe { oma_raw_decode(bytes.as_ptr(), bytes.len(), &mut native) };
    if result != 0 {
        let error = text(&native.error);
        return Err(format!("Could not decode camera RAW: {error}"));
    }
    let count = u64::from(native.width) * u64::from(native.height);
    if count == 0
        || count > MAX_PIXELS
        || native.bits != 16
        || !matches!(native.colors, 1 | 3)
        || native.data_size != count * u64::from(native.colors) * 2
        || native.data.is_null()
    {
        return Err("The RAW decoder returned an invalid pixel buffer".into());
    }
    // SAFETY: The bridge allocation owns exactly `data_size` readable bytes and
    // stays alive until `native` is dropped after this copy. Validation above
    // also bounds this slice well below isize::MAX on supported platforms.
    let samples = unsafe { std::slice::from_raw_parts(native.data, native.data_size as usize) };
    let mut pixels: Vec<u16> = Vec::new();
    pixels
        .try_reserve_exact(count as usize * 3)
        .map_err(|_| "Not enough memory to retain the 16-bit RAW image".to_owned())?;
    if native.colors == 3 {
        // SAFETY: Reservation above owns enough storage for every validated
        // byte. The source is a distinct live native allocation, and every u16
        // bit pattern is valid. LibRaw memory images use host byte order. Copy
        // once rather than walking millions of samples in a development build.
        unsafe {
            std::ptr::copy_nonoverlapping(
                samples.as_ptr(),
                pixels.as_mut_ptr().cast::<u8>(),
                samples.len(),
            );
            pixels.set_len(count as usize * 3);
        }
    } else {
        for sample in samples.as_chunks::<2>().0 {
            let value = u16::from_ne_bytes([sample[0], sample[1]]);
            pixels.extend([value; 3]);
        }
    }
    let mut warnings = Vec::new();
    if native.warnings & (1 << 2) != 0 {
        warnings
            .push("Camera white balance is missing or invalid; daylight balance was used.".into());
    }
    if native.warnings & (1 << 3) != 0 {
        warnings.push("This RAW file does not include complete camera metadata.".into());
    }
    if native.warnings & ((1 << 26) | (1 << 27)) != 0 {
        warnings.push("Some DNG processing instructions were not applied by the decoder.".into());
    } else if native.dng_opcodes != 0 {
        warnings.push("This DNG includes processing instructions; lens corrections and some DNG effects may differ from the camera preview.".into());
    }
    if native.raw_count > 1 {
        warnings.push(format!(
            "This file contains {} RAW frames; the first frame was opened.",
            native.raw_count
        ));
    }
    Ok(RawImage {
        width: native.width,
        height: native.height,
        pixels,
        metadata: RawMetadata {
            make: text(&native.make),
            model: text(&native.model),
            lens: text(&native.lens),
            iso: positive(native.iso),
            shutter: positive(native.shutter),
            aperture: positive(native.aperture),
            focal_length: positive(native.focal_length),
            orientation: native.orientation,
            camera_wb: native.camera_wb.map(positive),
            baseline_exposure: if native.baseline_exposure.is_finite()
                && (-16.0..=16.0).contains(&native.baseline_exposure)
            {
                native.baseline_exposure
            } else {
                0.0
            },
            timestamp: native.timestamp,
        },
        warnings,
    })
}

fn positive(value: f32) -> f32 {
    if value.is_finite() && value > 0.0 {
        value
    } else {
        0.0
    }
}

fn text<const N: usize>(field: &[c_char; N]) -> String {
    let bytes: Vec<u8> = field
        .iter()
        .take_while(|&&byte| byte != 0)
        .map(|&byte| byte.to_ne_bytes()[0])
        .collect();
    String::from_utf8_lossy(&bytes)
        .chars()
        .filter(|character| !character.is_control())
        .collect::<String>()
        .trim()
        .to_owned()
}

#[cfg(test)]
#[path = "raw/tests.rs"]
mod tests;
