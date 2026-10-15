//! Profile and ink operations on shared authoritative allocations.
use super::{Depth, NativePixels, cms, storage};
use crate::{
    document::Pixels,
    resources::{self, Allocation},
};
use std::sync::Arc;

impl Pixels {
    /// Retain CMYKA authoring and derive display RGB through the requested profile.
    /// Args: dimensions, depth, straight inks/alpha, ICC bytes and intent. Returns shared authoritative storage.
    pub fn from_cmyk(
        w: u32,
        h: u32,
        depth: Depth,
        inks: Vec<f32>,
        profile: Vec<u8>,
        intent: u32,
    ) -> Result<Self, String> {
        Self::from_cmyk_allocation(w, h, depth, storage::adopt(inks)?, profile, intent)
    }
    pub fn from_cmyk_allocation(
        w: u32,
        h: u32,
        depth: Depth,
        inks: Arc<Allocation<f32>>,
        profile: Vec<u8>,
        intent: u32,
    ) -> Result<Self, String> {
        if inks.len() != storage::count(w, h, 5)? || inks.iter().any(|v| !v.is_finite()) {
            return Err("Invalid native CMYKA samples".into());
        }
        let rgba = cms::transform_alpha(&inks, cms::Model::Cmyk, &profile, true, intent)?;
        let mut out = Self::from_sample_allocation(w, h, depth, rgba)?;
        let native = out.native.as_mut().unwrap();
        native.cmyk = Some(inks);
        native.intent = intent;
        native.icc_profile = storage::adopt(profile)?;
        Ok(out)
    }

    /// Tag authored RGB without transforming its source samples.
    /// Args: ICC bytes and intent. Returns a profile-aware display cache while retaining the authored RGB.
    pub fn assign_rgb_profile(&mut self, profile: Vec<u8>, intent: u32) -> Result<(), String> {
        cms::validate_profile(&profile, cms::Model::Rgb)?;
        let samples = self.samples()?;
        let display = cms::transform_alpha(&samples, cms::Model::Rgb, &profile, true, intent)?;
        let data = storage::collect(display.iter().map(|&v| super::byte(v)))?;
        let depth = self.native.as_ref().map_or(Depth::U8, |n| n.depth);
        let native = NativePixels {
            samples,
            depth,
            cmyk: None,
            icc_profile: storage::adopt(profile)?,
            intent,
        };
        self.data = data;
        self.native = Some(native);
        self.touch();
        Ok(())
    }

    /// Convert between authored profiles while preserving alpha and working precision.
    /// Args: actual target document color settings. Returns a converted image, sharing storage when its color already matches.
    pub fn in_document_color(&self, color: &cms::DocumentColor) -> Result<Self, String> {
        if color.intent > 3 {
            return Err("Invalid document rendering intent".into());
        }
        if let Some(native) = &self.native {
            if native.cmyk.is_some() == (color.model == cms::Model::Cmyk) && native.icc_profile.as_slice() == color.icc_profile.as_slice() {
                let mut out = if native.intent == color.intent || native.icc_profile.is_empty() {self.clone()}
                else if let Some(inks) = &native.cmyk {Self::from_cmyk_allocation(self.w,self.h,color.depth,inks.clone(),color.icc_profile.clone(),color.intent)?}
                else {let mut copy = self.clone();copy.assign_rgb_profile(color.icc_profile.clone(),color.intent)?;copy};
                let n = out.native.as_mut().unwrap();n.intent = color.intent;n.depth = color.depth;return Ok(out);
            }
        }
        let source = self.samples()?;
        let display = if let Some(native) = &self.native {
            if let Some(inks) = &native.cmyk {
                if native.intent != color.intent {cms::transform_alpha(inks,cms::Model::Cmyk,&native.icc_profile,true,color.intent)?} else {source}
            } else if !native.icc_profile.is_empty() {cms::transform_alpha(&source,cms::Model::Rgb,&native.icc_profile,true,color.intent)?} else {source}
        } else {source};
        if color.model == cms::Model::Cmyk {
            let inks = cms::transform_alpha(&display,cms::Model::Cmyk,&color.icc_profile,false,color.intent)?;
            drop(display);
            Self::from_cmyk_allocation(self.w,self.h,color.depth,inks,color.icc_profile.clone(),color.intent)
        } else {
            let rgba = if color.icc_profile.is_empty() {display} else {cms::transform_alpha(&display,cms::Model::Rgb,&color.icc_profile,false,color.intent)?};
            let mut out = Self::from_sample_allocation(self.w,self.h,color.depth,rgba)?;
            if !color.icc_profile.is_empty() {out.assign_rgb_profile(color.icc_profile.clone(),color.intent)?;}
            out.native.as_mut().unwrap().intent = color.intent;
            Ok(out)
        }
    }

    pub(super) fn retain_edited_color(&self, out: &mut Self) -> Result<(), String> {
        let Some(native) = &self.native else {
            return Ok(());
        };
        if let Some(inks) = &native.cmyk {
            let rgba = out.samples()?;
            let converted = cms::transform_alpha(&rgba,cms::Model::Cmyk,&native.icc_profile,false,native.intent)?;
            let authored =
                storage::collect(converted.chunks_exact(5).enumerate().flat_map(|(i, c)| {
                    let a = rgba[i * 4 + 3];
                    if rgba[i * 4..i * 4 + 3] == native.samples[i * 4..i * 4 + 3] {
                        [
                            inks[i * 5],
                            inks[i * 5 + 1],
                            inks[i * 5 + 2],
                            inks[i * 5 + 3],
                            a,
                        ]
                    } else {
                        [c[0], c[1], c[2], c[3], a]
                    }
                }))?;
            out.native.as_mut().unwrap().cmyk = Some(authored);
            out.native.as_mut().unwrap().icc_profile = native.icc_profile.clone();
            out.native.as_mut().unwrap().intent=native.intent;
        } else if !native.icc_profile.is_empty() {
            out.assign_rgb_profile(native.icc_profile.to_vec(), native.intent)?;
        }
        out.native.as_mut().unwrap().intent = native.intent;
        Ok(())
    }

    pub(super) fn reframe_inks(
        &self,
        w: u32,
        h: u32,
        dx: i64,
        dy: i64,
    ) -> Option<Result<Self, String>> {
        let native = self.native.as_ref()?;
        let inks = native.cmyk.as_ref()?;
        Some((|| {
            let mut output = resources::global()
                .allocate_now::<f32, _>(storage::count(w, h, 5)?, &storage::check)?;
            for y in 0..self.h {
                for x in 0..self.w {
                    let (nx, ny) = (x as i64 + dx, y as i64 + dy);
                    if nx < 0 || ny < 0 || nx >= w as i64 || ny >= h as i64 {
                        continue;
                    }
                    let i = (ny as usize * w as usize + nx as usize) * 5;
                    let j = (y as usize * self.w as usize + x as usize) * 5;
                    output.as_mut_slice()[i..i + 5].copy_from_slice(&inks[j..j + 5]);
                }
            }
            Self::from_cmyk_allocation(
                w,
                h,
                native.depth,
                Arc::new(output),
                native.icc_profile.to_vec(),
                native.intent,
            )
        })())
    }

    pub(super) fn resize_inks(&self, w: u32, h: u32) -> Option<Result<Self, String>> {
        let native = self.native.as_ref()?;
        let inks = native.cmyk.as_ref()?;
        Some((|| {
            let mut output = resources::global()
                .allocate_now::<f32, _>(storage::count(w, h, 5)?, &storage::check)?;
            for (i, dst) in output.as_mut_slice().chunks_exact_mut(5).enumerate() {
                let x = ((i % w as usize) as f32 + 0.5) * self.w as f32 / w as f32 - 0.5;
                let y = ((i / w as usize) as f32 + 0.5) * self.h as f32 / h as f32 - 0.5;
                let (ix, iy) = (x.floor() as i64, y.floor() as i64);
                let (fx, fy) = (x - x.floor(), y - y.floor());
                for (dx, dy, weight) in [
                    (0, 0, (1. - fx) * (1. - fy)),
                    (1, 0, fx * (1. - fy)),
                    (0, 1, (1. - fx) * fy),
                    (1, 1, fx * fy),
                ] {
                    let sx = (ix + dx).clamp(0, self.w as i64 - 1) as usize;
                    let sy = (iy + dy).clamp(0, self.h as i64 - 1) as usize;
                    let src = &inks[(sy * self.w as usize + sx) * 5..][..5];
                    for c in 0..4 {
                        dst[c] += src[c] * src[4] * weight;
                    }
                    dst[4] += src[4] * weight;
                }
                if dst[4] > 0. {
                    for c in 0..4 {
                        dst[c] /= dst[4];
                    }
                }
            }
            Self::from_cmyk_allocation(
                w,
                h,
                native.depth,
                Arc::new(output),
                native.icc_profile.to_vec(),
                native.intent,
            )
        })())
    }

    pub fn paint_color(&self, color: crate::color::Rgba) -> Result<[f32; 4], String> {
        let rgb = [
            color.r as f32 / 255.,
            color.g as f32 / 255.,
            color.b as f32 / 255.,
        ];
        let converted = if let Some(n) = &self.native {
            if n.cmyk.is_none() && !n.icc_profile.is_empty() {
                cms::transform(&rgb, cms::Model::Rgb, &n.icc_profile, false, n.intent)?
            } else {
                storage::adopt(rgb.to_vec())?
            }
        } else {
            storage::adopt(rgb.to_vec())?
        };
        Ok([
            converted[0],
            converted[1],
            converted[2],
            color.a as f32 / 255.,
        ])
    }
}
