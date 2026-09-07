//! Headless interchange entry points use exactly the same codecs as the desktop.
use crate::document::{Document, Layer, Pixels};
use crate::geom::Pt;
use std::path::Path;

pub fn document(path: &Path) -> Result<Document, String> {
    match crate::import::open_any(path)? {
        crate::import::Imported::Photo(photo) => photo_document(&photo),
        crate::import::Imported::Document(doc) => Ok(doc),
        crate::import::Imported::Raster { name, image } => {
            let mut doc = Document::new(name, image.w as f32, image.h as f32, 72.);
            doc.transparent = true;
            let size = Pt::new(image.w as f32, image.h as f32);
            let pixels = Pixels::from_rgba(image.w, image.h, image.data).ok_or("Invalid image")?;
            doc.layers = vec![Layer::placed_raster("Image", pixels, Pt::ZERO, size)];
            Ok(doc)
        }
        crate::import::Imported::Svg { name, svg } => {
            let (mut doc, notes) = super::svg::read(&svg, &name)?;
            doc.import_notes = notes;
            Ok(doc)
        }
    }
}

/// Design documents receive developed pixels; the Photo source/settings stay separate.
pub fn photo_document(photo: &crate::photo::PhotoImage) -> Result<Document, String> {
    let image = photo.render_full();
    let mut doc = Document::new(&photo.name, image.w as f32, image.h as f32, 72.);
    doc.transparent = true;
    let size = Pt::new(image.w as f32, image.h as f32);
    let pixels =
        Pixels::from_rgba(image.w, image.h, image.data).ok_or("Invalid developed photo")?;
    doc.layers = vec![Layer::placed_raster(&photo.name, pixels, Pt::ZERO, size)];
    doc.import_notes = photo.notes.clone();
    if photo.raw.is_some() {
        doc.import_notes.push("Photo development was rendered to an 8-bit pixel layer for Design. Keep the camera original and .omaphoto settings for RAW editing.".into());
    }
    Ok(doc)
}
pub fn export(doc: &Document, path: &Path) -> Result<Vec<String>, String> {
    let extension = path
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let (bytes,notes)=match extension.as_str(){
        "oma"=>{crate::project::save_to(doc,path)?;return Ok(vec![]);}
        "svg"=>(crate::svg::export(doc)?.into_bytes(),vec![]),
        "png"=>(crate::compositor::export_png(doc,1)?,vec![]),
        "jpg"|"jpeg"=>(crate::compositor::export_jpeg(doc,1,95)?,vec![]),
        "psd"|"psb"=>{let exported=super::psd::encode(doc,extension=="psb")?;(exported.bytes,exported.warnings)},
        "pdf"=>super::pdf::write(doc)?,
        "ora"=>super::openraster::write(doc)?,
        _=>return Err("Export as .oma, .svg, .png, .jpg, .psd, .psb, .ora or .pdf. Affinity and Illustrator native writers are not available.".into())
    };
    super::write_atomic(path, &bytes)?;
    Ok(notes)
}

pub fn run(args: &[String]) -> Option<Result<(), String>> {
    let inspect = args.iter().position(|a| a == "--inspect");
    let convert = args.iter().position(|a| a == "--convert");
    let index = inspect.or(convert)?;
    Some((|| {
        let input = args
            .get(index + 1)
            .ok_or("Use --inspect FILE or --convert FILE --output FILE")?;
        let mut opened_photo = None;
        if crate::import::classify(Path::new(input)) == "raw" {
            let photo = crate::photo::PhotoImage::load(Path::new(input))?;
            if inspect.is_some() {
                let raw = photo.raw.as_ref().ok_or("Missing RAW source")?;
                println!("{}",serde_json::to_string_pretty(&serde_json::json!({"name":photo.name,"width":raw.width,"height":raw.height,"kind":"camera RAW","bits":16,"color_space":"linear sRGB","decoder":super::raw::VERSION,"metadata":raw.metadata,"develop":photo.develop,"notes":photo.notes})).map_err(|e|e.to_string())?);
                return Ok(());
            }
            let output = args
                .windows(2)
                .find(|a| a[0] == "--output")
                .map(|a| Path::new(&a[1]))
                .ok_or("Use --convert INPUT --output OUTPUT")?;
            let extension = output
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_ascii_lowercase();
            if matches!(extension.as_str(), "png" | "jpg" | "jpeg" | "tif" | "tiff") {
                photo.export_to(output)?;
                for note in &photo.notes {
                    eprintln!("{note}");
                }
                println!("Developed full-resolution RAW to {}", output.display());
                return Ok(());
            }
            opened_photo = Some(photo);
        }
        let doc = match opened_photo {
            Some(photo) => photo_document(&photo)?,
            None => document(Path::new(input))?,
        };
        if inspect.is_some() {
            let layers=doc.layers.iter().map(|l|serde_json::json!({"id":l.id,"parent":l.parent,"name":l.name,"kind":if l.is_group{"group"}else{l.kind.tag()},"visible":l.visible,"opacity":l.opacity,"mask":l.mask.is_some(),"objects":l.kind.shapes().map_or(0,|s|s.len())})).collect::<Vec<_>>();
            println!("{}",serde_json::to_string_pretty(&serde_json::json!({"name":doc.name,"width":doc.width,"height":doc.height,"pages":doc.artboards.len(),"layers":layers,"notes":doc.import_notes})).map_err(|e|e.to_string())?);
        } else {
            let output = args
                .windows(2)
                .find(|a| a[0] == "--output")
                .map(|a| &a[1])
                .ok_or("Use --convert INPUT --output OUTPUT")?;
            if Path::new(input)
                .canonicalize()
                .ok()
                .zip(Path::new(output).canonicalize().ok())
                .is_some_and(|(a, b)| a == b)
            {
                return Err(
                    "Choose a different output file to preserve the source document".into(),
                );
            }
            let notes = export(&doc, Path::new(output))?;
            for note in doc.import_notes.iter().chain(notes.iter()) {
                eprintln!("{note}");
            }
            println!("Converted {} layers to {}", doc.layers.len(), output);
        }
        Ok(())
    })())
}
