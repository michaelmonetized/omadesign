use super::*;
use crate::{app::masking::SelectionSpace, document::Pixels, paint};
fn points(value: &Value) -> Result<Vec<Pt>, String> {
    let points = value.as_array().ok_or("points must be [x,y] pairs")?;
    if points.is_empty() || points.len() > 4096 {
        return Err("Provide 1–4096 points".into());
    }
    points
        .iter()
        .map(|p| {
            if p.as_array().is_none_or(|p| p.len() != 2) {
                return Err("Each point needs x and y".into());
            }
            let p = json!({"x":p[0],"y":p[1]});
            Ok(Pt::new(number(&p, "x", 0.)?, number(&p, "y", 0.)?))
        })
        .collect()
}
pub fn execute(studio: &mut Studio, name: &str, args: &Value) -> Result<Value, String> {
    let li = index(args, "layer")?;
    editing::editable_layer(studio, li)?;
    if name == "set_mask" {
        mask(studio, li, args)?;
    } else {
        let layer = &studio.doc.layers[li];
        let pixels = layer.kind.pixels().ok_or("Choose a raster layer")?;
        let (w, h) = (pixels.w, pixels.h);
        if u64::from(w) * u64::from(h) > 64_000_000 {
            return Err("Pixel operation exceeds 64 megapixels".into());
        }
        if name == "set_pixel_selection" {
            let kind = text(args, "kind", "")?;
            let x = number(args, "x", 0.)?;
            let y = number(args, "y", 0.)?;
            let width = number(args, "width", w as f32)?;
            let height = number(args, "height", h as f32)?;
            let old = studio.pixel_sel_mask(li).map(|m| m.into_owned());
            let mut values = match kind.as_str() {
                "rect" => paint::fill_rect_mask(w, h, x, y, x + width, y + height),
                "ellipse" => paint::fill_ellipse_mask(w, h, x, y, x + width, y + height),
                "polygon" => {
                    let pts = points(&args["points"])?;
                    if pts.len() < 3 {
                        return Err("Polygon needs at least 3 points".into());
                    }
                    paint::fill_poly_mask(w, h, &pts)
                }
                "wand" => paint::wand_mask(
                    &pixels.to_pixmap().ok_or("Invalid pixels")?,
                    Pt::new(x, y),
                    number(args, "tolerance", 32.)?.clamp(0., 510.),
                ),
                "all" => vec![255; w as usize * h as usize],
                "none" => {
                    studio.set_pixel_sel(None);
                    studio.mark();
                    return Ok(json!({"revision":studio.canvas_gen,"selected_pixels":0}));
                }
                "invert" => old.as_ref().map_or_else(
                    || vec![255; w as usize * h as usize],
                    |m| m.iter().map(|v| 255 - v).collect(),
                ),
                _ => return Err("Unknown pixel selection kind".into()),
            };
            values = match args["combine"].as_str().unwrap_or("replace") {
                "replace" => values,
                "add" => paint::combine_masks(old.as_deref(), &values, paint::PixelCombine::Add),
                "subtract" => {
                    paint::combine_masks(old.as_deref(), &values, paint::PixelCombine::Subtract)
                }
                "intersect" => values
                    .iter()
                    .zip(old.unwrap_or_else(|| vec![0; values.len()]))
                    .map(|(a, b)| (*a).min(b))
                    .collect(),
                _ => return Err("Unknown selection combination".into()),
            };
            let radius = args["feather"].as_u64().unwrap_or(0);
            if radius > 1024 {
                return Err("Feather radius must be 0–1024".into());
            }
            if radius > 0 {
                values = paint::feather_mask(&values, w, h, radius as u32);
            }
            let space = SelectionSpace {
                w,
                h,
                transform: crate::compositor::layer_pixel_transform(layer),
            };
            studio.active_layer = Some(li);
            studio.selection = vec![(li, 0)];
            studio.replace_pixel_selection(values, space);
            studio.mark();
        } else if name == "paint_stroke" {
            let mask = boolean(args, "mask", false)?;
            let original = if mask {
                layer
                    .mask
                    .as_ref()
                    .ok_or("Create the layer mask before painting it")?
            } else {
                pixels
            };
            if mask && (original.w != w || original.h != h) {
                return Err(
                    "Mask grid differs from image; recreate its selection mask first".into(),
                );
            }
            let before = original.data.clone();
            let original_pm = original.to_pixmap().ok_or("Invalid pixel data")?;
            let mut pm = original_pm.clone();
            let pts = points(&args["points"])?;
            if pts
                .iter()
                .any(|p| p.x < 0. || p.y < 0. || p.x >= w as f32 || p.y >= h as f32)
            {
                return Err("Stroke points must be within the source-pixel bounds".into());
            }
            let operation = text(args, "operation", "paint")?;
            if !["paint", "erase", "smudge", "clone", "heal", "fill"].contains(&operation.as_str())
            {
                return Err("Unknown paint operation".into());
            }
            let source = if matches!(operation.as_str(), "clone" | "heal") {
                points(&json!([args["source"]]))?[0]
            } else {
                Pt::ZERO
            };
            let brush = paint::Brush {
                size: number(args, "size", 20.)?.clamp(1., 512.),
                hardness: number(args, "hardness", 0.8)?.clamp(0., 1.),
                opacity: number(args, "opacity", 1.)?.clamp(0., 1.),
                flow: number(args, "flow", 1.)?.clamp(0., 1.),
                spacing: number(args, "spacing", 0.2)?.clamp(0.05, 2.),
                color: color(&text(
                    args,
                    "color",
                    if mask { "#FFFFFF" } else { "#000000" },
                )?)?,
            };
            let distance = pts.windows(2).map(|p| (p[1] - p[0]).length()).sum::<f32>();
            let estimate =
                (distance / (brush.size * brush.spacing).max(0.5) + 1.) * brush.size * brush.size;
            if estimate > 100_000_000. {
                return Err(
                    "Stroke is too large for one live operation; split into shorter strokes".into(),
                );
            }
            let selection = studio.pixel_sel_mask(li).map(|m| m.into_owned());
            match operation.as_str() {
                "fill" => paint::flood_fill_clipped(
                    &mut pm,
                    pts[0],
                    brush.color,
                    number(args, "tolerance", 32.)?.clamp(0., 510.),
                    selection.as_deref(),
                ),
                "paint" | "erase" => {
                    paint::stamp(&mut pm, pts[0], &brush, operation == "erase");
                    for pair in pts.windows(2) {
                        paint::stroke_to(&mut pm, pair[0], pair[1], &brush, operation == "erase");
                    }
                }
                "smudge" => {
                    for pair in pts.windows(2) {
                        paint::smudge_stroke(&mut pm, pair[0], pair[1], &brush);
                    }
                }
                "clone" => {
                    paint::clone_stamp(&mut pm, pts[0], source, &brush);
                    for pair in pts.windows(2) {
                        paint::clone_stroke(
                            &mut pm,
                            pair[0],
                            pair[1],
                            source + (pair[0] - pts[0]),
                            &brush,
                        );
                    }
                }
                "heal" => {
                    paint::heal_stamp(&mut pm, &original_pm, pts[0], source, &brush);
                    for pair in pts.windows(2) {
                        paint::heal_stroke(
                            &mut pm,
                            &original_pm,
                            pair[0],
                            pair[1],
                            source - pts[0],
                            &brush,
                        );
                    }
                }
                _ => unreachable!(),
            }
            if let Some(selection) = selection {
                paint::feather_edit(&mut pm, &original_pm, &selection);
            }
            let after = Pixels::from_pixmap(&pm).data;
            studio.commit(Cmd::Batch(vec![Cmd::Pixels {
                layer: li,
                mask,
                before,
                after,
            }]));
        } else {
            return Err("Unknown pixel tool".into());
        }
    }
    Ok(
        json!({"revision":studio.canvas_gen,"layer":li,"selected_pixels":studio.pixel_sel.as_ref().map(|m|paint::selected_count(m))}),
    )
}
fn mask(studio: &mut Studio, li: usize, args: &Value) -> Result<(), String> {
    let operation = text(args, "operation", "")?;
    let id = args
        .get("id")
        .map(|_| index(args, "id").map(|i| i as u64))
        .transpose()?;
    if operation == "selection" && studio.pixel_sel.is_none() {
        return Err("Make a pixel selection first".into());
    }
    if let Some(id) = id {
        let s = object(studio, li, id)?;
        if operation == "selection" {
            studio.mask_object_from_selection(li, id);
            return Ok(());
        }
        let after = match operation.as_str() {
            "remove" => None,
            "invert" => {
                let mut p = s.mask.clone().ok_or("Object has no mask")?;
                for v in p.data.chunks_exact_mut(4) {
                    let amount = (v[0] as u32 * v[3] as u32 / 255) as u8;
                    let a = 255 - amount;
                    v.copy_from_slice(&[a, a, a, 255]);
                }
                p.touch();
                Some(p)
            }
            "reveal" | "hide" => {
                let mut p = Pixels::new(1, 1);
                let a = if operation == "reveal" { 255 } else { 0 };
                p.data.copy_from_slice(&[a, a, a, 255]);
                p.touch();
                Some(p)
            }
            _ => {
                return Err(
                    "Object masks support selection, reveal, hide, invert and remove".into(),
                );
            }
        };
        studio.commit(Cmd::Batch(vec![Cmd::SetShapeMask {
            layer: li,
            id,
            before: s.mask,
            after,
        }]));
    } else {
        if matches!(operation.as_str(), "invert" | "apply") && studio.doc.layers[li].mask.is_none()
        {
            return Err("Layer has no mask".into());
        }
        if operation == "apply" && studio.doc.layers[li].kind.pixels().is_none() {
            return Err("Applying a mask to pixels requires a raster layer".into());
        }
        match operation.as_str() {
            "selection" => studio.mask_from_selection(li),
            "reveal" => studio.add_layer_mask(li, true),
            "hide" => studio.add_layer_mask(li, false),
            "invert" => studio.invert_layer_mask(li),
            "remove" => studio.remove_layer_mask(li),
            "apply" => studio.apply_layer_mask(li),
            _ => return Err("Unknown mask operation".into()),
        }
    }
    Ok(())
}
