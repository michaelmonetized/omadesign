use crate::{app::Studio, document::Cmd, formats::psd::smart};
use eframe::egui::Ui;

pub(super) fn inspector(ui: &mut Ui, studio: &mut Studio, index: usize) {
    let Some(layer) = studio.doc.layers.get(index) else {
        return;
    };
    let Some(object) = layer.psd_smart.clone() else {
        if ui.button("Make placed source").clicked() {
            let command = smart::make_smart(&studio.doc, index);
            apply(studio, command);
        }
        return;
    };
    let source = layer
        .source_attachment
        .as_ref()
        .and_then(|id| studio.doc.source(id));
    ui.label(source.map_or("Linked source is missing", |s| s.name.as_str()));
    ui.horizontal(|ui| {
        if ui.button("Edit source…").clicked() {
            let result = (|| {
                let doc = smart::source_document(&studio.doc, index)?;
                let name = format!("{}-edit.psd", doc.name);
                let Some(path) = rfd::FileDialog::new()
                    .set_file_name(name)
                    .add_filter("Editable Photoshop source", &["psd"])
                    .save_file()
                else {
                    return Ok(None);
                };
                let out = crate::formats::psd::encode(&doc, false)?;
                crate::formats::write_atomic(&path, &out.bytes)?;
                let before = studio.doc.layers[index].clone();
                let mut after = before.clone();
                after.psd_smart.as_mut().unwrap().edit_path =
                    Some(path.to_string_lossy().into_owned());
                studio.commit(Cmd::SetSmartLayer {
                    index,
                    before: Box::new(before),
                    after: Box::new(after),
                });
                Ok::<_, String>(Some((doc, path)))
            })();
            match result {
                Ok(Some((doc, path))) => studio.open_document(doc, Some(path)),
                Ok(None) => {}
                Err(error) => studio.status = error,
            }
        }
        if ui.button("Replace source…").clicked()
            && let Some(path) = crate::project::dialog_place()
        {
            let result = (|| {
                if std::fs::metadata(&path).map_err(|e| e.to_string())?.len() > 512 * 1024 * 1024 {
                    return Err("Source exceeds 512 MiB".into());
                }
                let bytes = std::fs::read(&path).map_err(|e| e.to_string())?;
                smart::replace_source(
                    &studio.doc,
                    index,
                    &path.file_name().unwrap_or_default().to_string_lossy(),
                    bytes,
                )
            })();
            apply(studio, result);
        }
    });
    if let Some(path) = &object.edit_path
        && ui.button("Reload edited source").clicked()
    {
        let result = (|| {
            let path = std::path::Path::new(path);
            if std::fs::metadata(path).map_err(|e| e.to_string())?.len() > 512 * 1024 * 1024 {
                return Err("Source exceeds 512 MiB".into());
            }
            let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
            smart::replace_source(
                &studio.doc,
                index,
                &path.file_name().unwrap_or_default().to_string_lossy(),
                bytes,
            )
        })();
        apply(studio, result);
    }
    for filter in object.filters() {
        let mut enabled = filter.enabled;
        let changed = ui.checkbox(&mut enabled, &filter.name).changed();
        let mut parameter = filter.settings.as_ref().map(|s| s.values[0]);
        let range = if filter
            .settings
            .as_ref()
            .is_some_and(|s| s.kind == crate::raster::Kind::BrightnessContrast)
        {
            -1.0..=1.0
        } else {
            0.0..=1000.0
        };
        let parameter_changed = if let Some(value) = &mut parameter {
            ui.add(eframe::egui::DragValue::new(value).speed(0.1).range(range))
                .changed()
        } else {
            ui.small("Saved preview · retained Photoshop filter");
            false
        };
        if changed || parameter_changed {
            let result = object
                .with_filter(
                    filter.index,
                    enabled,
                    parameter_changed.then_some(parameter.unwrap_or(0.)),
                )
                .and_then(|edited| smart::refresh(&studio.doc, index, edited));
            apply(studio, result);
        }
    }
    if ui.button("Add Gaussian smart filter").clicked() {
        let command = object
            .with_blur(2.)
            .and_then(|edited| smart::refresh(&studio.doc, index, edited));
        apply(studio, command);
    }
    if ui.button("Rasterize placed source").clicked() {
        let before = studio.doc.layers[index].clone();
        let mut after = before.clone();
        after.psd_smart = None;
        after.source_attachment = None;
        studio.commit(Cmd::SetSmartLayer {
            index,
            before: Box::new(before),
            after: Box::new(after),
        });
    }
}
fn apply(studio: &mut Studio, command: Result<Cmd, String>) {
    match command {
        Ok(command) => {
            studio.commit(command);
            studio.status =
                "Placed source updated · original bytes retained · one undo step".into();
        }
        Err(error) => studio.status = error,
    }
}
