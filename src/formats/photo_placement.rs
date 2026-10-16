//! Share developed photo allocations with Design without using its display cache.
use crate::{
    deep_color::{Depth, cms::DocumentColor, cms::Model},
    document::{Document, Layer, Pixels},
    formats::raw::Cancellation,
    geom::Pt,
    photo::{AdmittedImage, AdmittedSamples, PhotoImage},
    resources,
};

#[cfg(test)]
#[path = "photo_placement/tests.rs"]
mod tests;

/// Build a developed photo document for its actual destination profile.
/// Args: immutable photo snapshot, existing document color or a new document, and job cancellation.
/// Returns a placed pixel layer with admitted storage; RAW retains straight encoded F32 samples.
pub(crate) fn document(
    photo: &PhotoImage,
    target: Option<&DocumentColor>,
    cancel: &Cancellation,
) -> Result<Document, String> {
    cancel.check()?;
    let (pixels, color) = if photo.raw.is_some() {
        let AdmittedSamples {
            width,
            height,
            samples,
            memory,
        } = photo.developed_f32(cancel)?;
        let source =
            Pixels::from_admitted_samples(width, height, Depth::F32, samples, memory, cancel)?;
        let mut color = target.cloned().unwrap_or(DocumentColor {
            model: Model::Rgb,
            depth: Depth::F32,
            icc_profile: vec![],
            intent: 1,
        });
        color.depth = Depth::F32;
        cancel.check()?;
        (source.in_document_color(&color)?, color)
    } else {
        let AdmittedImage { image, memory } = photo.render_full_cancellable(cancel)?;
        let source = Pixels::from_byte_allocation(
            image.w,
            image.h,
            resources::global().adopt(image.data, memory)?,
        )?;
        if let Some(color) = target {
            cancel.check()?;
            (source.in_document_color(color)?, color.clone())
        } else {
            (source, DocumentColor::default())
        }
    };
    cancel.check()?;
    let (width, height) = (pixels.w as f32, pixels.h as f32);
    let mut doc = Document::new(&photo.name, width, height, 72.);
    doc.transparent = true;
    doc.color = color;
    doc.layers = vec![Layer::placed_raster(
        &photo.name,
        pixels,
        Pt::ZERO,
        Pt::new(width, height),
    )];
    doc.import_notes = photo.notes.clone();
    if photo.raw.is_some() {
        doc.import_notes.push("RAW development is preserved as a floating-point pixel layer. Keep the camera original and .omaphoto settings for RAW editing.".into());
    }
    Ok(doc)
}
