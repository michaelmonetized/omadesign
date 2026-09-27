//! Pixel workflow operations. RGB and mask always share the same pixel grid.
use super::{Settings, models::Model, upscale};
use crate::{background_removal::Source, ml::Progress};
use image::{
    GrayImage, RgbaImage,
    imageops::{FilterType, resize},
};
use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Workflow {
    pub before: u32,
    pub keep_original: bool,
    pub cutout_only: bool,
    pub after: u32,
    pub model: Model,
}
impl Default for Workflow {
    fn default() -> Self {
        Self {
            before: 1,
            keep_original: true,
            cutout_only: false,
            after: 1,
            model: Model::General,
        }
    }
}
impl Workflow {
    pub fn uses_ai(self) -> bool {
        self.before > 1 && !self.cutout_only || self.cutout_only && self.after > 1
    }
    pub fn dimensions(self, source: &Source) -> Result<(u32, u32), String> {
        if ![1, 2, 4].contains(&self.before) || ![1, 2, 4].contains(&self.after) {
            return Err("Choose ×2 or ×4".into());
        }
        let processing = if self.cutout_only {
            self.after
        } else {
            self.before
        };
        let dims = super::dimensions(source.w, source.h, processing as f64)?;
        if dims.0 as u64 * dims.1 as u64 > 64_000_000 {
            return Err("Pixel layers and background removal support up to 64 megapixels. Choose a smaller upscale.".into());
        }
        if self.cutout_only && source.existing_mask.is_none() {
            return Err(
                "Remove the background or add a layer mask before upscaling a cutout".into(),
            );
        }
        Ok(if !self.cutout_only && self.keep_original {
            (source.w, source.h)
        } else {
            dims
        })
    }
}
fn resize_mask(data: &[u8], w: u32, h: u32, size: (u32, u32)) -> Arc<Vec<u8>> {
    Arc::new(
        resize(
            &RgbaImage::from_raw(w, h, data.to_vec()).unwrap(),
            size.0,
            size.1,
            FilterType::CatmullRom,
        )
        .into_raw(),
    )
}
pub fn prepare(
    source: &Source,
    workflow: Workflow,
    progress: &dyn Progress,
) -> Result<Source, String> {
    workflow.dimensions(source)?;
    source.region()?;
    if workflow.cutout_only || workflow.before == 1 {
        return Ok(source.clone());
    }
    let rgba = RgbaImage::from_raw(source.w, source.h, source.rgba.as_ref().clone())
        .ok_or("Invalid layer pixels")?;
    let output = upscale(
        &rgba,
        Settings {
            factor: workflow.before as f64,
            model: workflow.model,
            ..Default::default()
        },
        progress,
    )?;
    let size = output.dimensions();
    let selection = source.selection.as_ref().map(|s| {
        Arc::new(
            resize(
                &GrayImage::from_raw(source.w, source.h, s.as_ref().clone()).unwrap(),
                size.0,
                size.1,
                FilterType::CatmullRom,
            )
            .into_raw(),
        )
    });
    let existing_mask = source
        .existing_mask
        .as_ref()
        .map(|m| resize_mask(m, source.w, source.h, size));
    progress.check()?;
    Ok(Source {
        w: size.0,
        h: size.1,
        rgba: Arc::new(output.into_raw()),
        selection,
        existing_mask,
    })
}
pub fn finish(
    original: &Source,
    prepared: &Source,
    mask: Arc<Vec<u8>>,
    workflow: Workflow,
    progress: &dyn Progress,
) -> Result<(Source, Arc<Vec<u8>>), String> {
    if workflow.cutout_only {
        let rgba = RgbaImage::from_raw(original.w, original.h, original.rgba.as_ref().clone())
            .ok_or("Invalid layer pixels")?;
        let output = upscale(
            &rgba,
            Settings {
                factor: workflow.after as f64,
                model: workflow.model,
                ..Default::default()
            },
            progress,
        )?;
        let size = output.dimensions();
        let mask = resize_mask(&mask, original.w, original.h, size);
        progress.check()?;
        return Ok((
            Source {
                w: size.0,
                h: size.1,
                rgba: Arc::new(output.into_raw()),
                selection: None,
                existing_mask: None,
            },
            mask,
        ));
    }
    if workflow.before > 1 && workflow.keep_original {
        let mut mask = resize_mask(&mask, prepared.w, prepared.h, (original.w, original.h))
            .as_ref()
            .clone();
        // Resampling must never spill a matte change outside the original
        // selection. Preserve those exact bytes (including alpha).
        if let Some(selection) = &original.selection {
            for (i, coverage) in selection.iter().enumerate() {
                if *coverage == 0 {
                    mask[i * 4..i * 4 + 4].copy_from_slice(
                        original
                            .existing_mask
                            .as_ref()
                            .map_or(&[255; 4], |m| &m[i * 4..i * 4 + 4]),
                    );
                }
            }
        }
        progress.check()?;
        return Ok((original.clone(), Arc::new(mask)));
    }
    Ok((prepared.clone(), mask))
}
