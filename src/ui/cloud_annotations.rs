//! Read-only rendering of the normalized review geometry shared with the web client.
use eframe::egui::{self, Color32, Rect, Stroke, Ui};
use serde_json::Value;

pub fn paint(ui: &Ui, area: Rect, annotation: &Value, index: usize) {
    let number = |key: &str| annotation[key].as_f64().unwrap_or(0.) as f32;
    let point =
        |x: f32, y: f32| area.min + area.size() * egui::vec2(x.clamp(0., 1.), y.clamp(0., 1.));
    let origin = point(number("x"), number("y"));
    let unit = area.width().min(area.height());
    let opacity = if annotation["resolved"] == true {
        0.4
    } else {
        1.
    };
    let accent = Color32::from_rgb(213, 255, 114).gamma_multiply(opacity);
    let shape = annotation["shape"].as_str().unwrap_or("pin");
    match shape {
        "stamp" => {
            let (key, svg): (&'static str, &[u8]) = match annotation["stamp"].as_str().unwrap_or("")
            {
                "check" => (
                    "review-check",
                    include_bytes!("../../site/public/media/review/check.svg"),
                ),
                "x" => (
                    "review-x",
                    include_bytes!("../../site/public/media/review/x.svg"),
                ),
                "heart" => (
                    "review-heart",
                    include_bytes!("../../site/public/media/review/heart.svg"),
                ),
                "question" => (
                    "review-question",
                    include_bytes!("../../site/public/media/review/question.svg"),
                ),
                "exclaim" => (
                    "review-exclaim",
                    include_bytes!("../../site/public/media/review/exclaim.svg"),
                ),
                _ => return,
            };
            // Render with the source viewBox intact: the same anchor and size as the web SVG.
            let id = egui::Id::new(key);
            let texture = ui
                .ctx()
                .data(|d| d.get_temp::<egui::TextureHandle>(id))
                .or_else(|| {
                    let tree = usvg::Tree::from_data(svg, &usvg::Options::default()).ok()?;
                    let mut pixels = tiny_skia::Pixmap::new(208, 208)?;
                    resvg::render(
                        &tree,
                        tiny_skia::Transform::from_scale(0.5, 0.5),
                        &mut pixels.as_mut(),
                    );
                    let texture = ui.ctx().load_texture(
                        key,
                        egui::ColorImage::from_rgba_premultiplied([208, 208], pixels.data()),
                        egui::TextureOptions::LINEAR,
                    );
                    ui.ctx().data_mut(|d| d.insert_temp(id, texture.clone()));
                    Some(texture)
                });
            if let Some(texture) = texture {
                let size = (unit * 0.12).max(42.);
                let rect = Rect::from_min_size(origin - egui::vec2(size * (0.028 / 0.075), size * (0.068 / 0.075)), egui::vec2(size, size));
                ui.painter().image(
                    texture.id(),
                    rect,
                    Rect::from_min_max(egui::pos2(0., 0.), egui::pos2(1., 1.)),
                    Color32::WHITE.gamma_multiply(opacity),
                );
            }
        }
        "highlight" | "brush" => {
            let Some(points) = annotation["points"].as_array() else {
                return;
            };
            let points: Vec<_> = points
                .iter()
                .take(512)
                .filter_map(|p| Some(point(p["x"].as_f64()? as f32, p["y"].as_f64()? as f32)))
                .collect();
            if points.len() < 2 {
                return;
            }
            let color = annotation["color"]
                .as_str()
                .and_then(|s| s.strip_prefix('#'))
                .and_then(|s| u32::from_str_radix(s, 16).ok())
                .unwrap_or(0xb784ff);
            let alpha = (number("opacity").clamp(0.1, 1.) * opacity * 255.).round() as u8;
            let stroke = Stroke::new(
                number("strokeWidth").clamp(0.001, 0.05) * unit,
                Color32::from_rgba_unmultiplied(
                    (color >> 16) as u8,
                    (color >> 8) as u8,
                    color as u8,
                    alpha,
                ),
            );
            ui.painter().add(egui::Shape::line(points, stroke));
        }
        "rectangle" => {
            ui.painter().rect_stroke(
                Rect::from_two_pos(origin, point(number("endX"), number("endY"))),
                0.,
                Stroke::new(2., accent),
                egui::StrokeKind::Inside,
            );
        }
        _ => {}
    }
    if shape == "pin" || shape == "rectangle" {
        ui.painter().circle_filled(origin, (unit * 0.016).max(13.), accent);
        ui.painter().text(
            origin,
            egui::Align2::CENTER_CENTER,
            (index + 1).to_string(),
            egui::FontId::proportional((unit * 0.018).max(14.)),
            Color32::BLACK.gamma_multiply(opacity),
        );
    }
}
