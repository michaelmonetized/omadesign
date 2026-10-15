use crate::{
    app::Studio,
    deep_color::{
        Depth,
        cms::{DocumentColor, Model},
    },
};
use eframe::egui::Ui;

pub(super) fn menu(ui: &mut Ui, studio: &mut Studio) {
    ui.menu_button("Color and precision", |ui| {
        ui.label(format!(
            "{:?} · {} bits/channel",
            studio.doc.color.model,
            studio.doc.color.depth.bits()
        ));
        for (label, depth) in [
            ("8 bits/channel (reduces precision)", Depth::U8),
            ("16 bits/channel", Depth::U16),
            ("32-bit float", Depth::F32),
        ] {
            if ui
                .selectable_label(studio.doc.color.depth == depth, label)
                .clicked()
            {
                let mut color = studio.doc.color.clone();
                color.depth = depth;
                convert(studio, color);
                ui.close();
            }
        }
        ui.separator();
        if ui.button("Convert with ICC profile…").clicked() {
            if let Some(path) = rfd::FileDialog::new()
                .add_filter("ICC profiles", &["icc", "icm"])
                .pick_file()
            {
                let result = (|| {
                    if std::fs::metadata(&path).map_err(|e| e.to_string())?.len() > 16 * 1024 * 1024
                    {
                        return Err("ICC profile exceeds 16 MiB".to_string());
                    }
                    let profile = std::fs::read(path).map_err(|e| e.to_string())?;
                    let model = match profile.get(16..20) {
                        Some(b"RGB ") => Model::Rgb,
                        Some(b"CMYK") => Model::Cmyk,
                        _ => return Err("Choose an RGB or CMYK ICC profile".into()),
                    };
                    Ok(DocumentColor {
                        model,
                        depth: studio.doc.color.depth,
                        icc_profile: profile,
                        intent: 1,
                    })
                })();
                match result {
                    Ok(color) => convert(studio, color),
                    Err(error) => studio.status = error,
                }
            }
            ui.close();
        }
        if ui.button("Convert to untagged sRGB").clicked() {
            let mut color = studio.doc.color.clone();
            color.model = Model::Rgb;
            color.icc_profile.clear();
            convert(studio, color);
            ui.close();
        }
        if studio.doc.color.model == Model::Cmyk && studio.doc.color.icc_profile.is_empty() {
            ui.label("CMYK preview is uncalibrated. Choose an ICC profile for color conversion.");
        }
    });
}
fn convert(studio: &mut Studio, color: DocumentColor) {
    studio.end_pixel_stroke(false);
    match studio.doc.color_conversion(color) {
        Ok(command) => {
            studio.commit(command);
            studio.status =
                "Document color conversion applied · undo restores source samples".into();
        }
        Err(error) => studio.status = error,
    }
}
