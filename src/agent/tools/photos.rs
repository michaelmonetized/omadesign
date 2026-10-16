use super::*;
pub fn execute(studio: &mut Studio, name: &str, args: &Value) -> Result<Value, String> {
    match name {
        "get_photos" => {
            let offset = args["offset"].as_u64().unwrap_or(0) as usize;
            let photos:Vec<_>=studio.photo.images.iter().enumerate().skip(offset).take(50).map(|(i,p)|json!({"index":i,"name":p.name,"source":p.source,"width":p.full.w,"height":p.full.h,"raw":p.raw.is_some(),"adjustments":p.develop})).collect();
            let next = offset.saturating_add(photos.len());
            return Ok(
                json!({"photos":photos,"selected":studio.photo.selected,"photo_revision":studio.photo.edit_revision,"revision":studio.canvas_gen,"next_offset":(next<studio.photo.images.len()).then_some(next),"loading":studio.photo.is_loading(),"saving":studio.photo.is_saving()}),
            );
        }
        "select_photo" => {
            let i = index(args, "photo")?;
            if i >= studio.photo.images.len() {
                return Err("Photo no longer exists".into());
            }
            studio.photo.select_image(i);
            studio.mark();
        }
        "develop_photo" => {
            check_revision(studio, args)?;
            let i = index(args, "photo")?;
            let photo = studio.photo.images.get(i).ok_or("Photo no longer exists")?;
            let before = photo.develop.clone();
            let mut after = if boolean(args, "reset", false)? {
                crate::photo::DevelopParams::default()
            } else {
                before.clone()
            };
            if boolean(args, "auto_tone", false)? {
                crate::photo::auto_tone(&mut after, &photo.preview);
            }
            if let Some(patch) = args.get("changes") {
                let mut value = serde_json::to_value(&after).unwrap();
                editing::merge(&mut value, patch)?;
                after = serde_json::from_value(value).map_err(|e| e.to_string())?;
            }
            crate::photo::edits::validate(&after)?;
            studio.photo.select_image(i);
            studio.photo.images[i].develop = after;
            studio.photo.record_edit(before, false);
            studio.mark();
        }
        "save_document" => return save(studio, args),
        _ => return Err("Unknown Photo tool".into()),
    }
    Ok(
        json!({"revision":studio.canvas_gen,"photo_revision":studio.photo.edit_revision,"selected":studio.photo.selected}),
    )
}
pub fn check_revision(studio: &Studio, args: &Value) -> Result<(), String> {
    if args["photo_revision"].as_u64() != Some(studio.photo.edit_revision) {
        return Err("Photo changed. Call get_photos and retry with its photo_revision".into());
    }
    if studio.photo.is_loading() || studio.photo.is_batching() || studio.photo.is_saving() {
        return Err("Photo operation is in progress; retry when it completes".into());
    }
    Ok(())
}
fn save(studio: &mut Studio, args: &Value) -> Result<Value, String> {
    let path = files::output_path(args)?;
    let source = args["source"].as_str().unwrap_or("canvas");
    let notes =
        if source == "photo" {
            check_revision(studio, args)?;
            let i = args["photo"]
                .as_u64()
                .map(|i| i as usize)
                .or(studio.photo.selected)
                .ok_or("Select a photo")?;
            let photo = studio.photo.images.get(i).ok_or("Photo no longer exists")?;
            if photo.source.as_ref().is_some_and(|src| {
                src.canonicalize().ok() == path.canonicalize().ok() && path.exists()
            }) {
                return Err("Keep the original photo: export to another filename".into());
            }
            if path.extension().is_some_and(|e| e == "omaphoto") {
                crate::photo::edits::save_to(
                    &path,
                    photo.source.as_deref().ok_or("No photo source path")?,
                    photo
                        .source_identity
                        .as_ref()
                        .ok_or("No photo source identity")?,
                    &photo.develop,
                )?;
                vec![]
            } else {
                crate::formats::cli::export(&crate::formats::cli::photo_document(photo)?, &path)?
            }
        } else if source == "canvas" {
            let format = args["format"].as_str().unwrap_or("auto");
            match format {
                "auto" => crate::formats::cli::export(&studio.doc, &path)?,
                "animated_svg" => {
                    let report = crate::svg::animated_export(&studio.doc)?;
                    crate::formats::write_atomic(&path, report.svg.as_bytes())?;
                    report.warnings
                }
                "lottie" => {
                    crate::formats::write_atomic(
                        &path,
                        crate::motion::export_lottie(&studio.doc)?.as_bytes(),
                    )?;
                    vec![]
                }
                "dotlottie" => {
                    crate::formats::write_atomic(
                        &path,
                        &crate::motion::export_dotlottie(&studio.doc)?,
                    )?;
                    vec![]
                }
                _ => return Err("Unknown export format".into()),
            }
        } else {
            return Err("Choose canvas or photo".into());
        };
    Ok(
        json!({"path":path,"notes":notes,"revision":studio.canvas_gen,"photo_revision":studio.photo.edit_revision}),
    )
}

#[cfg(test)]
mod motion_svg_tests {
    use super::*;
    #[test]
    fn save_tool_surfaces_animated_svg_reductions() {
        let mut studio = Studio::new();
        studio.doc = crate::svg::motion_tests::fixture("singular");
        let root = std::env::var_os("CRAFT_MOTION_COMMON_QA_DIR")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| {
                std::env::current_dir()
                    .unwrap()
                    .join(".artifacts/motion-common-save")
            });
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join(format!("reduced-save-{}.svg", crate::document::next_id()));
        let result = execute(
            &mut studio,
            "save_document",
            &json!({"path":path,"format":"animated_svg"}),
        )
        .unwrap();
        assert!(result["notes"].as_array().unwrap().iter().any(|v| {
            v.as_str().is_some_and(|v| {
                v.contains("singular") && v.contains("Swatch") && v.contains("Art")
            })
        }));
        assert!(path.exists());
        std::fs::remove_file(path).unwrap();
    }
}
