//! Native export boundaries. Artwork outside a selected scope never enters the output.
use crate::{
    document::{Artboard, Document, LayerKind},
    geom::{Bounds, Pt},
};
use std::collections::HashSet;

#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum Scope {
    #[default]
    Document,
    Artboard(u64),
    Selection,
}

pub fn artboard(doc: &Document, id: u64) -> Result<Document, String> {
    let board = doc
        .artboards
        .iter()
        .find(|a| a.id == id)
        .ok_or("That artboard no longer exists")?;
    let mut output = doc.clone();
    output.artboards = vec![board.clone()];
    crop(output, board.bounds())
}

pub fn selection(doc: &Document, selected: &[(usize, u64)]) -> Result<Document, String> {
    let mut ids: HashSet<(usize, u64)> = selected.iter().copied().collect();
    for &(layer, id) in selected {
        ids.extend(
            crate::layout::descendants(doc, layer, id)
                .into_iter()
                .map(|id| (layer, id)),
        );
    }
    let mut containers = HashSet::new();
    let mut bounds: Option<Bounds> = None;
    let mut layers = HashSet::new();
    for &(layer, id) in &ids {
        let Some(item) = doc.layers.get(layer) else {
            continue;
        };
        let next = if id == 0 {
            item.kind.raster_bounds()
        } else {
            doc.find_shape(layer, id).map(|shape| {
                let b = shape.world_bbox();
                let mut points = [
                    tiny_skia::Point::from_xy(b.min.x, b.min.y),
                    tiny_skia::Point::from_xy(b.max.x, b.min.y),
                    tiny_skia::Point::from_xy(b.max.x, b.max.y),
                    tiny_skia::Point::from_xy(b.min.x, b.max.y),
                ];
                crate::compositor::shape_parent_transform(doc, layer, id).map_points(&mut points);
                let mut b = Bounds::from_pt(Pt::new(points[0].x, points[0].y));
                for point in &points[1..] {
                    b.union_pt(Pt::new(point.x, point.y));
                }
                let mut parent = shape.layout.parent;
                for _ in 0..64 {
                    let Some(parent_id) = parent else { break };
                    if !containers.insert((layer, parent_id)) {
                        break;
                    }
                    parent = doc
                        .find_shape(layer, parent_id)
                        .and_then(|s| s.layout.parent);
                }
                b
            })
        };
        if let Some(next) = next {
            bounds = Some(bounds.map_or(next, |b| b.union(next)));
            layers.insert(layer);
            layers.extend(doc.layer_ancestors(layer));
        }
    }
    let bounds = bounds.ok_or("Select artwork to export")?;
    let mut output = doc.clone();
    output.layers = doc
        .layers
        .iter()
        .enumerate()
        .filter(|(i, _)| layers.contains(i))
        .map(|(i, layer)| {
            let mut layer = layer.clone();
            if let Some(shapes) = layer.kind.shapes_mut() {
                shapes.retain(|s| ids.contains(&(i, s.id)) || containers.contains(&(i, s.id)));
                for shape in shapes {
                    if !ids.contains(&(i, shape.id)) {
                        shape.style.fill = crate::document::Fill::None;
                        shape.style.stroke = None;
                        shape.layout.image = None;
                    }
                }
            }
            layer
        })
        .collect();
    output.transparent = true;
    output.artboards.clear();
    let mut output = crop(output, bounds)?;
    output.artboards.push(Artboard::new(
        0,
        Pt::ZERO,
        Pt::new(output.width, output.height),
    ));
    Ok(output)
}

fn crop(mut doc: Document, bounds: Bounds) -> Result<Document, String> {
    if !bounds.min.x.is_finite()
        || !bounds.min.y.is_finite()
        || !bounds.width().is_finite()
        || !bounds.height().is_finite()
        || bounds.width() < 0.
        || bounds.height() < 0.
    {
        return Err("The export area has invalid dimensions".into());
    }
    let delta = bounds.min * -1.;
    doc.width = bounds.width().max(1.);
    doc.height = bounds.height().max(1.);
    doc.cloud = None;
    doc.comments.clear();
    doc.guides.clear();
    for layer in &mut doc.layers {
        if let Some(shapes) = layer.kind.shapes_mut() {
            for shape in shapes {
                shape.geom.translate(delta);
            }
        } else if let LayerKind::Raster { origin, .. } = &mut layer.kind {
            *origin += delta;
        }
        layer.mask_origin += delta;
    }
    for board in &mut doc.artboards {
        board.origin += delta;
    }
    Ok(doc)
}

/// A pixel selection clips the native layer tree through a placed group mask.
/// Feathered coverage and rotated/scaled selection spaces stay intact.
pub fn pixels(
    doc: &Document,
    coverage: &[u8],
    w: u32,
    h: u32,
    transform: tiny_skia::Transform,
) -> Result<Document, String> {
    if coverage.len() != w as usize * h as usize {
        return Err("Invalid selection coverage".into());
    }
    let (x0, y0, x1, y1) =
        crate::paint::selection_bounds(coverage, w, h).ok_or("The pixel selection is empty")?;
    let mut corners = [
        tiny_skia::Point::from_xy(x0 as f32, y0 as f32),
        tiny_skia::Point::from_xy(x1 as f32, y0 as f32),
        tiny_skia::Point::from_xy(x1 as f32, y1 as f32),
        tiny_skia::Point::from_xy(x0 as f32, y1 as f32),
    ];
    transform.map_points(&mut corners);
    if corners.iter().any(|p| !p.x.is_finite() || !p.y.is_finite()) {
        return Err("Invalid selection transform".into());
    }
    let mut bounds = Bounds::from_pt(Pt::new(corners[0].x, corners[0].y));
    for p in &corners[1..] {
        bounds.union_pt(Pt::new(p.x, p.y));
    }
    bounds.min = Pt::new(bounds.min.x.floor(), bounds.min.y.floor());
    bounds.max = Pt::new(bounds.max.x.ceil(), bounds.max.y.ceil());
    let (mw, mh) = (
        bounds.width().max(1.) as u32,
        bounds.height().max(1.) as u32,
    );
    if mw as u64 * mh as u64 > 64_000_000 {
        return Err("The pixel selection export exceeds 64 megapixels".into());
    }
    let source = crate::document::Pixels::from_rgba(
        w,
        h,
        coverage.iter().flat_map(|a| [255, 255, 255, *a]).collect(),
    )
    .and_then(|p| p.to_pixmap())
    .ok_or("Invalid selection coverage")?;
    let mut mask = tiny_skia::Pixmap::new(mw, mh).ok_or("Could not allocate selection mask")?;
    mask.draw_pixmap(
        0,
        0,
        source.as_ref(),
        &tiny_skia::PixmapPaint {
            quality: tiny_skia::FilterQuality::Bilinear,
            ..Default::default()
        },
        tiny_skia::Transform::from_translate(-bounds.min.x, -bounds.min.y).pre_concat(transform),
        None,
    );
    let mut output = doc.clone();
    let mut group = crate::document::Layer::group("Export selection");
    group.pass_through = false;
    group.mask = Some(crate::document::Pixels::from_pixmap(&mask));
    group.mask_origin = bounds.min;
    group.mask_size = Pt::new(mw as f32, mh as f32);
    for layer in &mut output.layers {
        if layer.parent.is_none() {
            layer.parent = Some(group.id);
        }
    }
    output.layers.push(group);
    output.transparent = true;
    output.artboards.clear();
    crop(output, bounds)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        color::Rgba,
        document::{Fill, Layer, Shape, Style},
        geom::Geom,
    };
    fn fixture() -> (Document, u64, u64) {
        let mut doc = Document::new("Export scopes", 80., 60., 96.);
        let board = Artboard::new(1, Pt::new(120., 20.), Pt::new(40., 30.));
        let mut picked = Shape::new(
            Geom::Rect {
                origin: Pt::new(125., 25.),
                size: Pt::new(12., 8.),
                radius: 0.,
            },
            Style {
                fill: Fill::Solid(Rgba::rgb(200, 40, 30)),
                stroke: None,
            },
        );
        picked.name = "Picked artwork".into();
        let mut sibling = picked.clone();
        sibling.id = crate::document::next_id();
        sibling.name = "Excluded sibling".into();
        sibling.geom.translate(Pt::new(-100., 0.));
        let mut layer = Layer::vector("Art");
        layer
            .kind
            .shapes_mut()
            .unwrap()
            .extend([picked.clone(), sibling]);
        let board_id = board.id;
        doc.artboards.push(board);
        doc.layers = vec![layer];
        (doc, board_id, picked.id)
    }
    #[test]
    fn export_artboard_outside_document_and_selection_keep_native_content() {
        let (doc, board_id, shape_id) = fixture();
        let before = crate::project::encode(&doc).unwrap();
        let board = artboard(&doc, board_id).unwrap();
        assert_eq!((board.width, board.height), (40., 30.));
        let pm = crate::compositor::render_export(&board, 1).unwrap();
        assert_eq!(pm.pixel(6, 6).unwrap().red(), 200);
        let selected = selection(&doc, &[(0, shape_id)]).unwrap();
        assert_eq!((selected.width, selected.height), (12., 8.));
        assert_eq!(selected.layers[0].kind.shapes().unwrap().len(), 1);
        let svg = crate::svg::export(&selected).unwrap();
        assert!(!svg.contains("Excluded sibling"));
        let (png, _) = crate::export::encode(
            &crate::export::Source::Document(selected.clone()),
            crate::export::Settings {
                scale: 2,
                ..Default::default()
            },
            &crate::ml::NoProgress,
        )
        .unwrap();
        let png = image::load_from_memory(&png).unwrap();
        assert_eq!((png.width(), png.height()), (24, 16));
        assert_eq!(before, crate::project::encode(&doc).unwrap());
    }
    #[test]
    fn export_pixel_selection_preserves_feather_and_native_layers() {
        let mut doc = Document::new("Mask", 8., 8., 96.);
        let mut layer = Layer::raster("Pixels", 8, 8);
        layer.kind.pixels_mut().unwrap().data = [220, 60, 30, 255].repeat(64);
        doc.layers = vec![layer];
        let mut coverage = vec![0; 64];
        coverage[2 * 8 + 3] = 128;
        coverage[2 * 8 + 4] = 255;
        let result = pixels(&doc, &coverage, 8, 8, tiny_skia::Transform::identity()).unwrap();
        assert_eq!((result.width, result.height), (2., 1.));
        assert_eq!(result.layers.len(), 2);
        let pm = crate::compositor::render_export(&result, 1).unwrap();
        assert!((pm.pixel(0, 0).unwrap().alpha() as i32 - 128).abs() <= 1);
        assert_eq!(pm.pixel(1, 0).unwrap().alpha(), 255);
        assert!(pixels(&doc, &coverage[..4], 8, 8, tiny_skia::Transform::identity()).is_err());
    }
    #[test]
    fn export_nested_selection_preserves_parent_rotation_and_excludes_siblings() {
        let (mut doc, _, id) = fixture();
        let mut frame = Shape::new(
            Geom::Rect {
                origin: Pt::new(120., 20.),
                size: Pt::new(40., 30.),
                radius: 0.,
            },
            Style {
                fill: Fill::Solid(Rgba::WHITE),
                stroke: None,
            },
        );
        frame.layout = crate::layout::FrameLayout::frame();
        frame.rotation = std::f32::consts::FRAC_PI_2;
        doc.find_shape_mut(0, id).unwrap().layout.parent = Some(frame.id);
        doc.layers[0].kind.shapes_mut().unwrap().push(frame);
        let result = selection(&doc, &[(0, id)]).unwrap();
        assert!((result.width - 8.).abs() < 0.01 && (result.height - 12.).abs() < 0.01);
        let pm = crate::compositor::render_export(&result, 1).unwrap();
        assert_eq!(pm.pixel(3, 5).unwrap().red(), 200);
        assert_eq!(result.layers[0].kind.shapes().unwrap().len(), 2);
    }
}
