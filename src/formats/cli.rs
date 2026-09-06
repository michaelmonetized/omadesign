//! Headless interchange entry points use exactly the same codecs as the desktop.
use crate::document::{Document, Layer, Pixels};
use crate::geom::Pt;
use std::path::Path;

pub fn document(path: &Path) -> Result<Document, String> {
    match crate::import::open_any(path)? {
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
        let doc = document(Path::new(input))?;
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
