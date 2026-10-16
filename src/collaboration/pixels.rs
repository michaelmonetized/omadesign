//! Lossless atomic pixel cells backed by the shared native allocation pool.
use super::{Cell, PIXEL_BUDGET};
use crate::{
    deep_color::{Depth, NativePixels, storage},
    document::Pixels,
    resources::{self, Allocation, CancellationCheck},
};
use base64::Engine as _;
use serde_json::{Value, json};
use std::{
    io::{Read, Write},
    sync::Arc,
};

/// Compress an admitted buffer without an untracked output allocation.
/// Args: byte chunks. Returns the exact lossless wire string or an admission error.
fn encode(chunks: impl IntoIterator<Item = impl AsRef<[u8]>>) -> Result<String, String> {
    let mut encoder = zstd::stream::write::Encoder::new(storage::Builder::<u8>::new()?, 1)
        .map_err(|e| e.to_string())?;
    for chunk in chunks {
        encoder
            .write_all(chunk.as_ref())
            .map_err(|e| e.to_string())?;
    }
    let bytes = encoder.finish().map_err(|e| e.to_string())?.finish()?;
    let _encoded =
        resources::global().reserve_now((bytes.len() as u64 + 2) / 3 * 4, &storage::check)?;
    Ok(base64::engine::general_purpose::STANDARD.encode(bytes.as_slice()))
}

/// Pack every authoritative channel and its derived display as one property.
/// Args: validated pixels. Returns a lossless wire cell without converting through display bytes.
pub(super) fn pack(pixels: &Pixels) -> Result<Cell, String> {
    pixels.validate_native()?;
    if pixels.data.len() as u64 != super::pixel_size(pixels)? {
        return Err("Invalid live pixel buffer size".into());
    }
    let native = pixels
        .native
        .as_ref()
        .map(|n| -> Result<Value, String> {
            let mut value = json!({"depth":n.depth,"intent":n.intent,
            "samples":encode(n.samples.iter().map(|v| v.to_le_bytes()))?,
            "icc_bytes":n.icc_profile.len(),
            "icc_profile":encode([n.icc_profile.as_slice()])?});
            if let Some(inks) = &n.cmyk {
                value["cmyk"] = json!(encode(inks.iter().map(|v| v.to_le_bytes()))?);
            }
            Ok(value)
        })
        .transpose()?;
    Ok(Cell::Pixels {
        w: pixels.w,
        h: pixels.h,
        data: encode([pixels.data.as_slice()])?,
        native,
    })
}

fn text<'a>(value: &'a Value, key: &str) -> Result<&'a str, String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("Missing live native {key}"))
}

/// Bound all decoded display, native, CMYKA and ICC bytes before allocating.
/// Args: atomic pixel cell. Returns its complete decoded footprint, independent of workspace admission.
pub(super) fn size(cell: &Cell) -> Result<u64, String> {
    let Cell::Pixels { w, h, native, .. } = cell else {
        return Err("Expected atomic live pixels".into());
    };
    descriptor_size(u64::from(*w), u64::from(*h), native.as_ref())
}

/// Bound a retained descriptor without copying compressed strings or decoding history.
/// Args: dimensions and optional authoritative metadata. Returns the existing decoded-byte quota.
pub(super) fn descriptor_size(w: u64, h: u64, native: Option<&Value>) -> Result<u64, String> {
    let display = w
        .checked_mul(h)
        .and_then(|n| n.checked_mul(4))
        .filter(|n| *n > 0 && *n <= PIXEL_BUDGET)
        .ok_or("Live pixel dimensions exceed the decode budget")?;
    let Some(native) = native.filter(|value| !value.is_null()) else {
        return Ok(display);
    };
    let fields = native.as_object().ok_or("Invalid live native payload")?;
    if fields.keys().any(|k| {
        ![
            "depth",
            "samples",
            "cmyk",
            "icc_profile",
            "icc_bytes",
            "intent",
        ]
        .contains(&k.as_str())
    }) {
        return Err("Unsupported live native field".into());
    }
    let _: Depth =
        serde_json::from_value(native.get("depth").cloned().ok_or("Missing native depth")?)
            .map_err(|e| e.to_string())?;
    if native
        .get("intent")
        .and_then(Value::as_u64)
        .filter(|n| *n <= 3)
        .is_none()
    {
        return Err("Invalid live native rendering intent".into());
    }
    text(native, "samples")?;
    text(native, "icc_profile")?;
    let icc = native
        .get("icc_bytes")
        .and_then(Value::as_u64)
        .filter(|n| *n <= 16 * 1024 * 1024)
        .ok_or("Invalid live ICC size")?;
    let inks = if fields.contains_key("cmyk") {
        text(native, "cmyk")?;
        display.checked_mul(5).ok_or("Live CMYKA size overflow")?
    } else {
        0
    };
    display
        .checked_mul(5)
        .and_then(|n| n.checked_add(inks))
        .and_then(|n| n.checked_add(icc))
        .filter(|n| *n <= PIXEL_BUDGET)
        .ok_or_else(|| "Live document exceeds the total pixel budget".into())
}

/// Decode a bounded frame into the one shared pool without a pressure wait.
/// Args: wire string, exact byte count and cancellation owner. Returns admitted immutable bytes.
fn decode<C: CancellationCheck + ?Sized>(
    text: &str,
    count: usize,
    check: &C,
) -> Result<Arc<Allocation<u8>>, String> {
    check.check()?;
    let input = base64::read::DecoderReader::new(
        text.as_bytes(),
        &base64::engine::general_purpose::STANDARD,
    );
    let mut decoder = zstd::stream::read::Decoder::new(input).map_err(|e| e.to_string())?;
    let _window = resources::global().reserve_now(128 * 1024 * 1024, check)?;
    decoder.window_log_max(27).map_err(|e| e.to_string())?;
    let mut bytes = resources::global().allocate_now::<u8, _>(count, check)?;
    for chunk in bytes.as_mut_slice().chunks_mut(65536) {
        check.check()?;
        decoder
            .read_exact(chunk)
            .map_err(|_| "Invalid live pixel buffer size")?;
    }
    let mut tail = [0u8; 1];
    if decoder.read(&mut tail).map_err(|e| e.to_string())? != 0 {
        return Err("Invalid live pixel buffer size".into());
    }
    check.check()?;
    Ok(Arc::new(bytes))
}
fn floats<C: CancellationCheck + ?Sized>(
    text: &str,
    count: usize,
    check: &C,
) -> Result<Arc<Allocation<f32>>, String> {
    let bytes = decode(
        text,
        count.checked_mul(4).ok_or("Live sample size overflow")?,
        check,
    )?;
    let mut out = resources::global().allocate_now::<f32, _>(count, check)?;
    for (src, dst) in bytes
        .chunks(65536)
        .zip(out.as_mut_slice().chunks_mut(16384))
    {
        check.check()?;
        for (src, dst) in src.chunks_exact(4).zip(dst) {
            *dst = f32::from_le_bytes(src.try_into().unwrap());
            if !dst.is_finite() {
                return Err("Nonfinite live native sample".into());
            }
        }
    }
    Ok(Arc::new(out))
}

/// Materialize an atomic cell without replacing the destination on failure.
/// Args: complete cell and cancellation owner. Returns validated native pixels in the shared pool.
pub(super) fn unpack<C: CancellationCheck + ?Sized>(
    cell: &Cell,
    check: &C,
) -> Result<Pixels, String> {
    size(cell)?;
    let Cell::Pixels { w, h, data, native } = cell else {
        unreachable!()
    };
    let count = storage::count(*w, *h, 4)?;
    let data = decode(data, count, check)?;
    let mut pixels = Pixels::from_byte_allocation(*w, *h, data)?;
    if let Some(n) = native {
        pixels.native = Some(NativePixels {
            depth: serde_json::from_value(n["depth"].clone()).map_err(|e| e.to_string())?,
            samples: floats(text(n, "samples")?, count, check)?,
            cmyk: n
                .get("cmyk")
                .map(|_| floats(text(n, "cmyk")?, storage::count(*w, *h, 5)?, check))
                .transpose()?,
            icc_profile: decode(
                text(n, "icc_profile")?,
                n["icc_bytes"].as_u64().unwrap() as usize,
                check,
            )?,
            intent: n["intent"].as_u64().unwrap() as u32,
        });
    }
    pixels.validate_native()?;
    check.check()?;
    Ok(pixels)
}
