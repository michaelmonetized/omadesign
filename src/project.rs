use crate::document::{
    Anchor, Document, Fill, Geometry, Layer, LayerBlend, LayerKind, RasterLayer, Shape, Style,
    VectorLayer,
};
use base64::Engine as _;
use eframe::egui::{pos2, Color32, Stroke, Vec2};
use serde::{Deserialize, Serialize};

const FILE_NAME: &str = "atelier-project.atelier";
const VERSION: u32 = 2;

#[derive(Serialize, Deserialize)]
pub(crate) struct ProjectFile {
    version: u32,
    width: f32,
    height: f32,
    layers: Vec<ProjectLayer>,
}

#[derive(Serialize, Deserialize)]
struct ProjectLayer {
    name: String,
    visible: bool,
    locked: bool,
    opacity: f32,
    blend: String,
    mask: Option<String>,
    kind: ProjectLayerKind,
}

#[derive(Serialize, Deserialize)]
enum ProjectLayerKind {
    Vector(Vec<ProjectShape>),
    Raster(String),
}

#[derive(Serialize, Deserialize)]
struct ProjectShape {
    id: u64,
    geom: ProjectGeom,
    fill: ProjectFill,
    stroke: Option<ProjectStroke>,
}

#[derive(Serialize, Deserialize)]
enum ProjectFill {
    None,
    Solid([u8; 4]),
    Linear {
        from: [f32; 2],
        to: [f32; 2],
        c0: [u8; 4],
        c1: [u8; 4],
    },
}

#[derive(Serialize, Deserialize)]
struct ProjectStroke {
    color: [u8; 4],
    width: f32,
}

#[derive(Serialize, Deserialize)]
struct ProjectAnchor {
    pt: [f32; 2],
    h_in: [f32; 2],
    h_out: [f32; 2],
}

#[derive(Serialize, Deserialize)]
enum ProjectGeom {
    Rect {
        origin: [f32; 2],
        size: [f32; 2],
    },
    Ellipse {
        center: [f32; 2],
        radii: [f32; 2],
    },
    Polyline {
        points: Vec<[f32; 2]>,
        closed: bool,
    },
    Path {
        anchors: Vec<ProjectAnchor>,
        closed: bool,
    },
    Text {
        subpaths: Vec<Vec<[f32; 2]>>,
        content: String,
        px: f32,
    },
    MultiPolygon {
        contours: Vec<Vec<[f32; 2]>>,
    },
}

fn p2t(p: eframe::egui::Pos2) -> [f32; 2] {
    [p.x, p.y]
}

fn t2p(a: [f32; 2]) -> eframe::egui::Pos2 {
    pos2(a[0], a[1])
}

fn v2t(v: Vec2) -> [f32; 2] {
    [v.x, v.y]
}

fn t2v(a: [f32; 2]) -> Vec2 {
    Vec2::new(a[0], a[1])
}

fn c2a(c: Color32) -> [u8; 4] {
    [c.r(), c.g(), c.b(), c.a()]
}

fn a2c(a: [u8; 4]) -> Color32 {
    Color32::from_rgba_unmultiplied(a[0], a[1], a[2], a[3])
}

fn fill_to_project(f: &Fill) -> ProjectFill {
    match f {
        Fill::None => ProjectFill::None,
        Fill::Solid(c) => ProjectFill::Solid(c2a(*c)),
        Fill::Linear { from, to, c0, c1 } => ProjectFill::Linear {
            from: *from,
            to: *to,
            c0: c2a(*c0),
            c1: c2a(*c1),
        },
    }
}

fn fill_from_project(f: &ProjectFill) -> Fill {
    match f {
        ProjectFill::None => Fill::None,
        ProjectFill::Solid(c) => Fill::Solid(a2c(*c)),
        ProjectFill::Linear { from, to, c0, c1 } => Fill::Linear {
            from: *from,
            to: *to,
            c0: a2c(*c0),
            c1: a2c(*c1),
        },
    }
}

fn blend_to_string(b: LayerBlend) -> String {
    b.name().to_lowercase().replace(' ', "-")
}

fn blend_from_string(s: &str) -> LayerBlend {
    match s {
        "multiply" => LayerBlend::Multiply,
        "screen" => LayerBlend::Screen,
        "overlay" => LayerBlend::Overlay,
        "darken" => LayerBlend::Darken,
        "lighten" => LayerBlend::Lighten,
        "difference" => LayerBlend::Difference,
        "exclusion" => LayerBlend::Exclusion,
        "hard-light" => LayerBlend::HardLight,
        "soft-light" => LayerBlend::SoftLight,
        _ => LayerBlend::Normal,
    }
}

fn geom_to_project(g: &Geometry) -> ProjectGeom {
    match g {
        Geometry::Rect { origin, size } => ProjectGeom::Rect {
            origin: p2t(*origin),
            size: [size.x, size.y],
        },
        Geometry::Ellipse { center, radii } => ProjectGeom::Ellipse {
            center: p2t(*center),
            radii: [radii.x, radii.y],
        },
        Geometry::Polyline { points, closed } => ProjectGeom::Polyline {
            points: points.iter().map(|p| p2t(*p)).collect(),
            closed: *closed,
        },
        Geometry::Path { anchors, closed } => ProjectGeom::Path {
            anchors: anchors
                .iter()
                .map(|a| ProjectAnchor {
                    pt: p2t(a.pt),
                    h_in: v2t(a.h_in),
                    h_out: v2t(a.h_out),
                })
                .collect(),
            closed: *closed,
        },
        Geometry::Text {
            subpaths,
            content,
            px,
        } => ProjectGeom::Text {
            subpaths: subpaths
                .iter()
                .map(|sp| sp.iter().map(|p| p2t(*p)).collect())
                .collect(),
            content: content.clone(),
            px: *px,
        },
        Geometry::MultiPolygon { contours } => ProjectGeom::MultiPolygon {
            contours: contours
                .iter()
                .map(|c| c.iter().map(|p| p2t(*p)).collect())
                .collect(),
        },
    }
}

fn geom_from_project(g: &ProjectGeom) -> Geometry {
    match g {
        ProjectGeom::Rect { origin, size } => Geometry::Rect {
            origin: t2p(*origin),
            size: Vec2::new(size[0], size[1]),
        },
        ProjectGeom::Ellipse { center, radii } => Geometry::Ellipse {
            center: t2p(*center),
            radii: Vec2::new(radii[0], radii[1]),
        },
        ProjectGeom::Polyline { points, closed } => Geometry::Polyline {
            points: points.iter().map(|a| t2p(*a)).collect(),
            closed: *closed,
        },
        ProjectGeom::Path { anchors, closed } => Geometry::Path {
            anchors: anchors
                .iter()
                .map(|a| Anchor {
                    pt: t2p(a.pt),
                    h_in: t2v(a.h_in),
                    h_out: t2v(a.h_out),
                })
                .collect(),
            closed: *closed,
        },
        ProjectGeom::Text {
            subpaths,
            content,
            px,
        } => Geometry::Text {
            subpaths: subpaths
                .iter()
                .map(|sp| sp.iter().map(|a| t2p(*a)).collect())
                .collect(),
            content: content.clone(),
            px: *px,
        },
        ProjectGeom::MultiPolygon { contours } => Geometry::MultiPolygon {
            contours: contours.iter().map(|c| c.iter().map(|a| t2p(*a)).collect()).collect(),
        },
    }
}

fn shape_to_project(s: &Shape) -> ProjectShape {
    ProjectShape {
        id: s.id,
        geom: geom_to_project(&s.geom),
        fill: fill_to_project(&s.style.fill),
        stroke: s.style.stroke.as_ref().map(|st| ProjectStroke {
            color: c2a(st.color),
            width: st.width,
        }),
    }
}

fn shape_from_project(s: &ProjectShape) -> Shape {
    Shape {
        id: s.id,
        geom: geom_from_project(&s.geom),
        style: Style {
            fill: fill_from_project(&s.fill),
            stroke: s
                .stroke
                .as_ref()
                .map(|st| Stroke::new(st.width, a2c(st.color))),
        },
    }
}

fn raster_to_b64(pm: &tiny_skia::Pixmap) -> String {
    let png = pm.encode_png().unwrap_or_default();
    base64::engine::general_purpose::STANDARD.encode(png)
}

fn raster_from_b64(s: &str) -> Option<RasterLayer> {
    let bytes = base64::engine::general_purpose::STANDARD.decode(s).ok()?;
    let pixmap = tiny_skia::Pixmap::decode_png(&bytes).ok()?;
    Some(RasterLayer { pixmap, version: 0 })
}

fn to_project(doc: &Document) -> ProjectFile {
    ProjectFile {
        version: VERSION,
        width: doc.width,
        height: doc.height,
        layers: doc
            .layers
            .iter()
            .map(|l| ProjectLayer {
                name: l.name.clone(),
                visible: l.visible,
                locked: l.locked,
                opacity: l.opacity,
                blend: blend_to_string(l.blend),
                mask: l.mask.as_ref().map(|m| raster_to_b64(&m.pixmap)),
                kind: match &l.kind {
                    LayerKind::Vector(v) => ProjectLayerKind::Vector(
                        v.shapes.iter().map(shape_to_project).collect(),
                    ),
                    LayerKind::Raster(r) => ProjectLayerKind::Raster(raster_to_b64(&r.pixmap)),
                },
            })
            .collect(),
    }
}

fn from_project(p: &ProjectFile) -> Option<Document> {
    let mut layers = vec![];
    for l in &p.layers {
        let kind = match &l.kind {
            ProjectLayerKind::Vector(shapes) => LayerKind::Vector(VectorLayer {
                shapes: shapes.iter().map(shape_from_project).collect(),
            }),
            ProjectLayerKind::Raster(data) => LayerKind::Raster(raster_from_b64(data)?),
        };
        layers.push(Layer {
            name: l.name.clone(),
            visible: l.visible,
            locked: l.locked,
            opacity: l.opacity,
            blend: blend_from_string(&l.blend),
            mask: l.mask.as_ref().and_then(|s| raster_from_b64(s)),
            kind,
        });
    }
    Some(Document {
        width: p.width,
        height: p.height,
        layers,
    })
}

pub fn save(doc: &Document) -> Result<String, String> {
    let json = serde_json::to_string(&to_project(doc)).map_err(|e| e.to_string())?;
    std::fs::write(FILE_NAME, json).map_err(|e| e.to_string())?;
    Ok(FILE_NAME.to_string())
}

pub fn load() -> Result<Document, String> {
    let json = std::fs::read_to_string(FILE_NAME).map_err(|e| e.to_string())?;
    let p: ProjectFile = serde_json::from_str(&json).map_err(|e| e.to_string())?;
    from_project(&p).ok_or_else(|| "project file corrupt".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::next_shape_id;

    #[test]
    fn roundtrip_preserves_document() {
        let mut doc = Document::new(200.0, 150.0);
        doc.layers.push(Layer {
            name: "extra".into(),
            visible: false,
            locked: true,
            opacity: 0.7,
            blend: LayerBlend::Multiply,
            mask: tiny_skia::Pixmap::new(200, 150).map(|pm| RasterLayer {
                pixmap: pm,
                version: 0,
            }),
            kind: LayerKind::Vector(VectorLayer {
                shapes: vec![
                    Shape {
                        id: next_shape_id(),
                        geom: Geometry::Rect {
                            origin: pos2(1.0, 2.0),
                            size: Vec2::new(30.0, 40.0),
                        },
                        style: Style {
                            fill: Fill::Linear {
                                from: [0.0, 0.0],
                                to: [1.0, 1.0],
                                c0: Color32::from_rgb(1, 2, 3),
                                c1: Color32::from_rgb(4, 5, 6),
                            },
                            stroke: Some(Stroke::new(2.5, Color32::from_rgb(9, 8, 7))),
                        },
                    },
                    Shape {
                        id: next_shape_id(),
                        geom: Geometry::Path {
                            anchors: vec![
                                Anchor::corner(pos2(0.0, 0.0)),
                                Anchor {
                                    pt: pos2(10.0, 0.0),
                                    h_in: Vec2::new(-5.0, 0.0),
                                    h_out: Vec2::new(5.0, 0.0),
                                },
                            ],
                            closed: true,
                        },
                        style: Style {
                            fill: Fill::Solid(Color32::WHITE),
                            stroke: None,
                        },
                    },
                ],
            }),
        });
        if let Some(r) = doc.layers[0].kind.raster_mut() {
            r.pixmap.data_mut()[0] = 200;
            r.pixmap.data_mut()[3] = 255;
        }

        let json = serde_json::to_string(&to_project(&doc)).unwrap();
        let p: ProjectFile = serde_json::from_str(&json).unwrap();
        let back = from_project(&p).unwrap();

        assert_eq!(back.width, 200.0);
        assert_eq!(back.layers.len(), doc.layers.len());
        let extra = &back.layers[2];
        assert_eq!(extra.name, "extra");
        assert!(!extra.visible && extra.locked);
        assert_eq!(extra.opacity, 0.7);
        assert_eq!(extra.blend, LayerBlend::Multiply);
        assert!(extra.mask.is_some());
        assert_eq!(extra.vector_len(), 2);
        let shapes = extra.kind_vector().unwrap();
        assert_eq!(shapes[0].geom.bbox().min, pos2(1.0, 2.0));
        assert!(matches!(shapes[0].style.fill, Fill::Linear { .. }));
        assert!(matches!(&shapes[1].geom, Geometry::Path { closed: true, .. }));
        let r0 = back.layers[0].raster().unwrap();
        assert_eq!(r0.pixmap.data()[0], 200);
        assert_eq!(r0.pixmap.data()[3], 255);
    }
}
