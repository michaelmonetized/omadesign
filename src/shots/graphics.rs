use super::*;
use crate::{
    color::Blend,
    document::{Document, Layer, Stroke},
    gradient::{Gradient, GradientKind, GradientStop},
};

pub(super) fn design_tools(s: &mut Studio) {
    s.doc = Document::new("Gradient playground", 1000., 740., 72.);
    s.doc.layers = vec![Layer::vector("Gradient studies")];
    s.active_layer = Some(0);
    s.persona = Persona::Design;
    s.tool = Tool::Gradient;
    s.show_welcome = false;
    s.show_rulers = true;
    s.need_fit = true;
    let mut shapes = Vec::new();
    for (index, kind) in GradientKind::ALL.into_iter().enumerate() {
        let x = 70. + (index % 2) as f32 * 460.;
        let y = 60. + (index / 2) as f32 * 350.;
        let mut gradient = Gradient::new(kind, Rgba::from_hex(0x6028EA), Rgba::from_hex(0xFFCE5C));
        gradient.stops.insert(
            1,
            GradientStop {
                offset: 0.48,
                color: Rgba::from_hex(0xFF568E),
            },
        );
        gradient.set_angle(
            35.,
            crate::geom::Bounds::from_min_size(Pt::new(x, y), Pt::new(330., 230.)),
        );
        let mut stroke = Gradient::new(
            GradientKind::Linear,
            Rgba::from_hex(0x3FDDCC),
            Rgba::from_hex(0x282055),
        );
        stroke.stops.insert(
            1,
            GradientStop {
                offset: 0.5,
                color: Rgba::WHITE,
            },
        );
        let geom = if kind == GradientKind::Shape {
            Geom::Star {
                center: Pt::new(x + 165., y + 115.),
                outer: Pt::splat(118.),
                inner: 0.6,
                points: 6,
            }
        } else {
            Geom::Rect {
                origin: Pt::new(x, y),
                size: Pt::new(330., 230.),
                radius: 28.,
            }
        };
        let mut shape = Shape::new(
            geom,
            Style {
                fill: Fill::Gradient(gradient),
                stroke: Some(Stroke {
                    width: 12.,
                    gradient: Some(stroke),
                    ..Default::default()
                }),
            },
        );
        shape.name = format!("{} / gradient fill and stroke", kind.name());
        if index == 0 {
            s.selection = vec![(0, shape.id)];
        }
        if index == 3 {
            shape.opacity = 0.9;
            shape.blend = Blend::Multiply;
        }
        shapes.push(shape);
    }
    *s.doc.layers[0].kind.shapes_mut().unwrap() = shapes;
    s.layer_expanded.insert(s.doc.layers[0].id);
    s.status =
        "Gradient G · click ramp to add stops · Fill / Stroke chooses the paint target".into();
}
