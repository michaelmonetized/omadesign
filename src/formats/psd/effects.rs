//! The standard Normal color overlay maps exactly to a native color matrix.

use super::codec::Reader;
use super::warn;
use crate::filter::{FilterStack, Fx};

pub(super) fn decode(bytes: &[u8], warnings: &mut Vec<String>) -> Result<FilterStack, String> {
    let mut r = Reader::new(bytes);
    if r.u16()? != 0 {
        return Err("Invalid Photoshop legacy effects version".into());
    }
    let n = r.u16()?;
    if n > 128 {
        return Err("Too many Photoshop layer effects".into());
    }
    let mut filters = FilterStack::default();
    for _ in 0..n {
        if r.take(4)? != b"8BIM" {
            return Err("Invalid Photoshop effect signature".into());
        }
        let key = r.key()?;
        let mut data = r.section(false)?;
        if &key == b"cmnS" && data.remaining() >= 5 {
            data.take(4)?;
            filters.enabled = data.u8()? != 0;
        } else if &key == b"sofi" {
            let version = data.u32()?;
            if version != 2 {
                warn(
                    warnings,
                    "An unsupported Photoshop color overlay was not recreated.",
                );
                continue;
            }
            let sig = data.key()?;
            let blend = if &sig == b"8BIM" { data.key()? } else { sig };
            let space = data.u16()?;
            let color = [data.u16()?, data.u16()?, data.u16()?];
            data.take(2)?;
            let opacity = data.u8()? as f32 / 255.0;
            let enabled = data.u8()? != 0;
            if !enabled {
                continue;
            }
            if &blend != b"norm" || space != 0 {
                warn(
                    warnings,
                    "A Photoshop color overlay with a non-Normal blend or non-RGB color was not recreated.",
                );
                continue;
            }
            let t = 1.0 - opacity;
            let c = color.map(|v| v as f32 / 65535.0 * opacity);
            filters.items.push(Fx::ColorMatrix {
                values: [
                    t, 0.0, 0.0, 0.0, c[0], 0.0, t, 0.0, 0.0, c[1], 0.0, 0.0, t, 0.0, c[2], 0.0,
                    0.0, 0.0, 1.0, 0.0,
                ],
            });
        } else {
            warn(
                warnings,
                "Photoshop effects beyond supported Normal color overlays are not recreated and can change the appearance.",
            );
        }
    }
    Ok(filters)
}

pub(super) struct Overlay {
    color: [f64; 3],
    opacity: f64,
    enabled: bool,
}

pub(super) fn overlay(stack: &FilterStack) -> Option<Overlay> {
    let [Fx::ColorMatrix { values: m }] = stack.items.as_slice() else {
        return None;
    };
    if m.iter().any(|v| !v.is_finite()) {
        return None;
    }
    let a = 1.0 - m[0];
    if !(0.00001..=1.0).contains(&a)
        || (m[0] - m[6]).abs() > 0.00001
        || (m[0] - m[12]).abs() > 0.00001
        || (m[18] - 1.0).abs() > 0.00001
    {
        return None;
    }
    if [1, 2, 3, 5, 7, 8, 10, 11, 13, 15, 16, 17, 19]
        .iter()
        .any(|i| m[*i].abs() > 0.00001)
    {
        return None;
    }
    let color = [m[4] / a, m[9] / a, m[14] / a];
    if color.iter().any(|v| !(0.0..=1.0).contains(v)) {
        return None;
    }
    Some(Overlay {
        color: color.map(|v| v as f64 * 255.0),
        opacity: a as f64 * 100.0,
        enabled: stack.enabled,
    })
}

/// Both the legacy and object-based representations make the overlay usable in
/// readers that support either generation of Photoshop's effect format.
pub(super) fn encode(overlay: &Overlay) -> (Vec<u8>, Vec<u8>) {
    let mut legacy = vec![0, 0, 0, 2];
    legacy.extend_from_slice(b"8BIMcmnS");
    legacy.extend_from_slice(&7u32.to_be_bytes());
    legacy.extend_from_slice(&[0, 0, 0, 0, u8::from(overlay.enabled), 0, 0]);
    legacy.extend_from_slice(b"8BIMsofi");
    legacy.extend_from_slice(&34u32.to_be_bytes());
    legacy.extend_from_slice(&2u32.to_be_bytes());
    legacy.extend_from_slice(b"8BIMnorm");
    color(&mut legacy, overlay.color);
    legacy.push((overlay.opacity / 100.0 * 255.0).round() as u8);
    legacy.push(1);
    color(&mut legacy, overlay.color);

    let mut modern = Vec::new();
    modern.extend_from_slice(&0u32.to_be_bytes());
    modern.extend_from_slice(&16u32.to_be_bytes());
    descriptor(&mut modern, b"Lefx", 3);
    key(&mut modern, b"masterFXSwitch");
    modern.extend_from_slice(b"bool");
    modern.push(u8::from(overlay.enabled));
    key(&mut modern, b"Scl ");
    modern.extend_from_slice(b"UntF#Prc");
    modern.extend_from_slice(&100f64.to_be_bytes());
    key(&mut modern, b"SoFi");
    modern.extend_from_slice(b"Objc");
    descriptor(&mut modern, b"SoFi", 6);
    for name in [b"enab".as_slice(), b"present", b"showInDialog"] {
        key(&mut modern, name);
        modern.extend_from_slice(b"bool");
        modern.push(1);
    }
    key(&mut modern, b"Md  ");
    modern.extend_from_slice(b"enum");
    key(&mut modern, b"BlnM");
    key(&mut modern, b"Nrml");
    key(&mut modern, b"Opct");
    modern.extend_from_slice(b"UntF#Prc");
    modern.extend_from_slice(&overlay.opacity.to_be_bytes());
    key(&mut modern, b"Clr ");
    modern.extend_from_slice(b"Objc");
    descriptor(&mut modern, b"RGBC", 3);
    for (name, value) in [b"Rd  ", b"Grn ", b"Bl  "].into_iter().zip(overlay.color) {
        key(&mut modern, name);
        modern.extend_from_slice(b"doub");
        modern.extend_from_slice(&value.to_be_bytes());
    }
    (legacy, modern)
}

fn color(out: &mut Vec<u8>, rgb: [f64; 3]) {
    out.extend_from_slice(&0u16.to_be_bytes());
    for value in rgb {
        out.extend_from_slice(&((value / 255.0 * 65535.0).round() as u16).to_be_bytes());
    }
    out.extend_from_slice(&0u16.to_be_bytes());
}

fn descriptor(out: &mut Vec<u8>, class: &[u8], count: u32) {
    out.extend_from_slice(&0u32.to_be_bytes()); // Empty Unicode name.
    key(out, class);
    out.extend_from_slice(&count.to_be_bytes());
}

fn key(out: &mut Vec<u8>, value: &[u8]) {
    out.extend_from_slice(
        &(if value.len() == 4 {
            0
        } else {
            value.len() as u32
        })
        .to_be_bytes(),
    );
    out.extend_from_slice(value);
}
